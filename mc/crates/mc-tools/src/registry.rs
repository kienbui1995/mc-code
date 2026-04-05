use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use serde_json::Value;
use tokio::sync::mpsc;
use tokio::sync::Mutex;

use crate::bash::BashTool;
use crate::error::ToolError;
use crate::file_ops::{EditFileTool, ReadFileTool, WriteFileTool};
use crate::mcp::McpClient;
use crate::sandbox::Sandbox;
use crate::search::{GlobSearchTool, GrepSearchTool};
use crate::spec::{all_tool_specs, ToolSpec};
use crate::web::{WebFetchTool, WebSearchTool};

const DEFAULT_MAX_OUTPUT: usize = 100_000;
const DEFAULT_TOOL_TIMEOUT: Duration = Duration::from_secs(120);

pub struct ToolRegistry {
    sandbox: Option<Sandbox>,
    max_output_bytes: usize,
    tool_timeout: Duration,
    mcp_clients: HashMap<String, Mutex<McpClient>>,
    mcp_tool_specs: Vec<ToolSpec>,
}

impl ToolRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self {
            sandbox: None,
            max_output_bytes: DEFAULT_MAX_OUTPUT,
            tool_timeout: DEFAULT_TOOL_TIMEOUT,
            mcp_clients: HashMap::new(),
            mcp_tool_specs: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_workspace_root(mut self, root: PathBuf) -> Self {
        self.sandbox = Some(Sandbox::new(root));
        self
    }

    /// Add file protection patterns (e.g. `.env`, `*.key`).
    #[must_use]
    pub fn with_protected_patterns(mut self, patterns: Vec<String>) -> Self {
        if let Some(ref mut sandbox) = self.sandbox {
            sandbox.protected.extend(patterns);
        }
        self
    }

    #[must_use]
    pub fn with_max_output(mut self, bytes: usize) -> Self {
        self.max_output_bytes = bytes;
        self
    }

    /// Connect an MCP server and discover its tools.
    pub async fn add_mcp_server(
        &mut self,
        name: &str,
        command: &str,
        args: &[String],
        env: &[(String, String)],
    ) -> Result<usize, ToolError> {
        let mut client = McpClient::connect(name, command, args, env).await?;
        let tools = client.discover_tools().await?;
        let count = tools.len();
        self.mcp_tool_specs.extend(tools);
        self.mcp_clients
            .insert(name.to_string(), Mutex::new(client));
        Ok(count)
    }

    /// All tool specs including MCP tools.
    #[must_use]
    pub fn all_specs(&self) -> Vec<ToolSpec> {
        let mut specs = all_tool_specs();
        specs.extend(self.mcp_tool_specs.clone());
        specs
    }

    #[must_use]
    pub fn specs() -> Vec<ToolSpec> {
        all_tool_specs()
    }

    pub async fn execute(&self, name: &str, input: &Value) -> Result<String, ToolError> {
        tracing::debug!(tool = name, "executing tool");

        let result = tokio::time::timeout(self.tool_timeout, self.execute_inner(name, input))
            .await
            .map_err(|_| {
                ToolError::ExecutionFailed(format!(
                    "tool '{name}' timed out after {}s",
                    self.tool_timeout.as_secs()
                ))
            })??;

        // Truncate large outputs
        Ok(self.truncate_output(result))
    }

    /// Like `execute`, but streams output chunks for tools that support it (currently bash).
    pub async fn execute_streaming(
        &self,
        name: &str,
        input: &Value,
        output_tx: &mpsc::UnboundedSender<String>,
    ) -> Result<String, ToolError> {
        tracing::debug!(tool = name, "executing tool (streaming)");

        let result = if name == "bash" {
            let cmd = str_field(input, "command")?;
            let timeout = input
                .get("timeout")
                .and_then(Value::as_u64)
                .map(Duration::from_millis);
            // BashTool::execute_streaming has its own timeout handling
            BashTool::execute_streaming(&cmd, timeout.or(Some(self.tool_timeout)), output_tx)
                .await?
        } else {
            tokio::time::timeout(self.tool_timeout, self.execute_inner(name, input))
                .await
                .map_err(|_| {
                    ToolError::ExecutionFailed(format!(
                        "tool '{name}' timed out after {}s",
                        self.tool_timeout.as_secs()
                    ))
                })??
        };

        Ok(self.truncate_output(result))
    }

    async fn execute_inner(&self, name: &str, input: &Value) -> Result<String, ToolError> {
        match name {
            "bash" => {
                let cmd = str_field(input, "command")?;
                let timeout = input
                    .get("timeout")
                    .and_then(Value::as_u64)
                    .map(Duration::from_millis);
                BashTool::execute(&cmd, timeout).await
            }
            "read_file" => {
                let path = self.check_path(str_field(input, "path")?)?;
                let offset = input
                    .get("offset")
                    .and_then(Value::as_u64)
                    .map(|v| v as usize);
                let limit = input
                    .get("limit")
                    .and_then(Value::as_u64)
                    .map(|v| v as usize);
                tokio::task::spawn_blocking(move || ReadFileTool::execute(&path, offset, limit))
                    .await
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?
            }
            "write_file" => {
                let path = self.check_path(str_field(input, "path")?)?;
                let content = str_field(input, "content")?;
                let old = std::fs::read_to_string(&path).unwrap_or_default();
                let result = tokio::task::spawn_blocking(move || {
                    let diff = make_diff(&old, &content, &path);
                    WriteFileTool::execute(&path, &content).map(|msg| format!("{diff}\n{msg}"))
                })
                .await
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))??;
                Ok(result)
            }
            "edit_file" => {
                let path = self.check_path(str_field(input, "path")?)?;
                let old_str = str_field(input, "old_string")?;
                let new_str = str_field(input, "new_string")?;
                let all = input
                    .get("replace_all")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let old_content = std::fs::read_to_string(&path).unwrap_or_default();
                let result = tokio::task::spawn_blocking(move || -> Result<String, ToolError> {
                    let res = EditFileTool::execute(&path, &old_str, &new_str, all)?;
                    let new_content = std::fs::read_to_string(&path).unwrap_or_default();
                    let diff = make_diff(&old_content, &new_content, &path);
                    Ok(format!("{diff}\n{res}"))
                })
                .await
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))??;
                Ok(result)
            }
            "glob_search" => {
                let pattern = str_field(input, "pattern")?;
                let path = input.get("path").and_then(Value::as_str).map(String::from);
                tokio::task::spawn_blocking(move || {
                    GlobSearchTool::execute(&pattern, path.as_deref())
                })
                .await
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?
            }
            "grep_search" => {
                let pattern = str_field(input, "pattern")?;
                let path = input.get("path").and_then(Value::as_str).map(String::from);
                let glob = input.get("glob").and_then(Value::as_str).map(String::from);
                tokio::task::spawn_blocking(move || {
                    GrepSearchTool::execute(&pattern, path.as_deref(), glob.as_deref())
                })
                .await
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?
            }
            "web_fetch" => {
                let url = str_field(input, "url")?;
                WebFetchTool::execute(&url).await
            }
            "web_search" => {
                let query = str_field(input, "query")?;
                WebSearchTool::execute(&query).await
            }
            _ => {
                // Check MCP tools: name format is mcp_{server}_{tool}
                if let Some(rest) = name.strip_prefix("mcp_") {
                    if let Some(sep) = rest.find('_') {
                        let server = &rest[..sep];
                        let tool = &rest[sep + 1..];
                        if let Some(client) = self.mcp_clients.get(server) {
                            let mut c = client.lock().await;
                            return c.call_tool(tool, input).await;
                        }
                    }
                }
                Err(ToolError::NotFound(name.to_string()))
            }
        }
    }

    /// Validate path against sandbox if configured.
    fn check_path(&self, path: String) -> Result<String, ToolError> {
        if let Some(ref sandbox) = self.sandbox {
            sandbox.check(&path)?;
        }
        Ok(path)
    }

    fn truncate_output(&self, output: String) -> String {
        if output.len() <= self.max_output_bytes {
            return output;
        }
        let end = output
            .char_indices()
            .map(|(i, _)| i)
            .take_while(|&i| i <= self.max_output_bytes)
            .last()
            .unwrap_or(0);
        format!(
            "{}...\n[truncated, {} bytes total]",
            &output[..end],
            output.len()
        )
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

fn str_field(input: &Value, field: &str) -> Result<String, ToolError> {
    input
        .get(field)
        .and_then(Value::as_str)
        .map(String::from)
        .ok_or_else(|| ToolError::InvalidInput(format!("missing field: {field}")))
}

fn make_diff(old: &str, new: &str, path: &str) -> String {
    use similar::{ChangeTag, TextDiff};
    if old.is_empty() {
        return format!("(new file: {path})");
    }
    let diff = TextDiff::from_lines(old, new);
    let mut out = String::new();
    for change in diff.iter_all_changes() {
        let prefix = match change.tag() {
            ChangeTag::Delete => "-",
            ChangeTag::Insert => "+",
            ChangeTag::Equal => continue,
        };
        out.push_str(&format!("{prefix}{change}"));
    }
    if out.is_empty() {
        "(no changes)".into()
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn executes_bash() {
        let reg = ToolRegistry::new();
        let out = reg
            .execute("bash", &json!({"command": "echo hi"}))
            .await
            .unwrap();
        assert!(out.contains("hi"));
    }

    #[tokio::test]
    async fn rejects_unknown_tool() {
        let reg = ToolRegistry::new();
        let err = reg.execute("nope", &json!({})).await.unwrap_err();
        assert!(matches!(err, ToolError::NotFound(_)));
    }

    #[test]
    fn specs_has_all_tools() {
        let specs = ToolRegistry::specs();
        assert_eq!(specs.len(), 11);
        let names: Vec<_> = specs.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"bash"));
        assert!(names.contains(&"edit_file"));
        assert!(names.contains(&"subagent"));
        assert!(names.contains(&"web_fetch"));
        assert!(names.contains(&"web_search"));
    }

    #[test]
    fn truncates_large_output() {
        let reg = ToolRegistry::new().with_max_output(20);
        let result = reg.truncate_output("a".repeat(100));
        assert!(result.contains("truncated"));
        assert!(result.contains("100 bytes"));
    }

    #[tokio::test]
    async fn sandbox_blocks_outside_path() {
        let dir = std::env::temp_dir().join(format!("mc-reg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let reg = ToolRegistry::new().with_workspace_root(dir.clone());
        let err = reg
            .execute("read_file", &json!({"path": "/etc/hostname"}))
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::PermissionDenied(_)));
        std::fs::remove_dir_all(dir).ok();
    }
}
