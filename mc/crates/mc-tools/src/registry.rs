use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use serde_json::Value;
use tokio::sync::mpsc;
use tokio::sync::Mutex;

use crate::bash::BashTool;
use crate::error::ToolError;
use crate::file_ops::{ApplyPatchTool, BatchEditTool, EditFileTool, ReadFileTool, WriteFileTool};
use crate::mcp::McpClient;
use crate::sandbox::Sandbox;
use crate::search::{GlobSearchTool, GrepSearchTool};
use crate::spec::{all_tool_specs, ToolSpec};
use crate::web::{WebFetchTool, WebSearchTool};

const DEFAULT_MAX_OUTPUT: usize = 100_000;
const DEFAULT_TOOL_TIMEOUT: Duration = Duration::from_secs(120);

/// Toolregistry.
pub struct ToolRegistry {
    sandbox: Option<Sandbox>,
    max_output_bytes: usize,
    tool_timeout: Duration,
    mcp_clients: HashMap<String, Mutex<McpClient>>,
    mcp_tool_specs: Vec<ToolSpec>,
    plugin_specs: Vec<ToolSpec>,
    cached_specs: std::sync::OnceLock<Vec<ToolSpec>>,
    read_files: std::sync::Arc<std::sync::Mutex<std::collections::HashSet<String>>>,
    /// When true, write_file/edit_file require interactive approval with diff preview.
    pub review_writes: std::sync::atomic::AtomicBool,
}

impl ToolRegistry {
    #[must_use]
    /// New.
    pub fn new() -> Self {
        Self {
            sandbox: None,
            max_output_bytes: DEFAULT_MAX_OUTPUT,
            tool_timeout: DEFAULT_TOOL_TIMEOUT,
            mcp_clients: HashMap::new(),
            mcp_tool_specs: Vec::new(),
            plugin_specs: Vec::new(),
            cached_specs: std::sync::OnceLock::new(),
            read_files: std::sync::Arc::new(
                std::sync::Mutex::new(std::collections::HashSet::new()),
            ),
            review_writes: std::sync::atomic::AtomicBool::new(false),
        }
    }

    #[must_use]
    /// With workspace root.
    pub fn with_workspace_root(mut self, root: PathBuf) -> Self {
        self.plugin_specs = crate::plugin::discover_plugins(&root);
        if !self.plugin_specs.is_empty() {
            tracing::info!(count = self.plugin_specs.len(), "plugins discovered");
        }
        self.sandbox = Some(Sandbox::new(root));
        self
    }

    /// Add file protection patterns (e.g. `.env`, `*.key`).
    #[must_use]
    /// With protected patterns.
    pub fn with_protected_patterns(mut self, patterns: Vec<String>) -> Self {
        if let Some(ref mut sandbox) = self.sandbox {
            sandbox.protected.extend(patterns);
        }
        self
    }

    /// Grant access to an additional directory outside the workspace root.
    #[must_use]
    pub fn with_extra_root(mut self, root: PathBuf) -> Self {
        if let Some(ref mut sandbox) = self.sandbox {
            sandbox.extra_roots.push(root);
        }
        self
    }

    #[must_use]
    /// With max output.
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
        self.cached_specs = std::sync::OnceLock::new(); // invalidate cache
        self.mcp_clients
            .insert(name.to_string(), Mutex::new(client));
        Ok(count)
    }

    /// All tool specs including MCP tools.
    #[must_use]
    /// All specs.
    pub fn all_specs(&self) -> &[ToolSpec] {
        self.cached_specs.get_or_init(|| {
            let mut specs = all_tool_specs();
            specs.extend(self.mcp_tool_specs.clone());
            specs.extend(self.plugin_specs.clone());
            specs
        })
    }

    #[must_use]
    /// Specs.
    pub fn specs() -> Vec<ToolSpec> {
        all_tool_specs()
    }

    /// Clear the set of tracked read files (useful for session reset).
    pub fn clear_read_tracking(&self) {
        if let Ok(mut reads) = self.read_files.lock() {
            reads.clear();
        }
    }

    /// Execute.
    pub async fn execute(&self, name: &str, input: &Value) -> Result<String, ToolError> {
        tracing::debug!(tool = name, "executing tool");
        Self::validate_input(name, input)?;

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
        } else if name == "web_fetch" {
            let url = str_field(input, "url")?;
            WebFetchTool::execute_streaming(&url, output_tx).await?
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

    #[allow(clippy::too_many_lines)]
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
                let path_clone = path.clone();
                let result = tokio::task::spawn_blocking(move || {
                    ReadFileTool::execute(&path_clone, offset, limit)
                })
                .await
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
                if let Ok(ref _output) = result {
                    if let Ok(mut reads) = self.read_files.lock() {
                        reads.insert(path.clone());
                    }
                }
                result
            }
            "write_file" => {
                let path = self.check_path(str_field(input, "path")?)?;
                // Read-before-write enforcement
                {
                    let reads = self.read_files.lock().unwrap_or_else(|e| e.into_inner());
                    let file_exists = std::path::Path::new(&path).exists();
                    if file_exists && !reads.contains(&path) {
                        return Err(ToolError::ExecutionFailed(
                            format!("Cannot write to '{}': file has not been read in this session. Use read_file first.", path)
                        ));
                    }
                }
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
                // Read-before-write enforcement
                {
                    let reads = self.read_files.lock().unwrap_or_else(|e| e.into_inner());
                    let file_exists = std::path::Path::new(&path).exists();
                    if file_exists && !reads.contains(&path) {
                        return Err(ToolError::ExecutionFailed(
                            format!("Cannot write to '{}': file has not been read in this session. Use read_file first.", path)
                        ));
                    }
                }
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
            "batch_edit" => {
                let edits = input
                    .get("edits")
                    .and_then(|v| v.as_array())
                    .ok_or_else(|| ToolError::InvalidInput("missing 'edits' array".into()))?;
                let edits_clone = edits.clone();
                tokio::task::spawn_blocking(move || BatchEditTool::execute(&edits_clone))
                    .await
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?
            }
            "apply_patch" => {
                let patch = str_field(input, "patch")?;
                tokio::task::spawn_blocking(move || ApplyPatchTool::execute(&patch))
                    .await
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?
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
            _ if name.starts_with("plugin_") => {
                let plugin_input = input.get("input").and_then(|v| v.as_str()).unwrap_or("");
                let workspace = self
                    .sandbox
                    .as_ref()
                    .map_or_else(|| PathBuf::from("."), |s| s.root().to_path_buf());
                crate::plugin::execute_plugin(&workspace, name, plugin_input).await
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

    fn validate_input(name: &str, input: &Value) -> Result<(), ToolError> {
        let require = |field: &str| {
            if input
                .get(field)
                .and_then(|v| v.as_str())
                .is_none_or(str::is_empty)
            {
                Err(ToolError::InvalidInput(format!(
                    "{name}: missing required field '{field}'"
                )))
            } else {
                Ok(())
            }
        };
        match name {
            "bash" => require("command"),
            "read_file" | "write_file" => require("path"),
            "edit_file" => {
                require("path")?;
                require("old_string")?;
                require("new_string")
            }
            "web_fetch" => require("url"),
            "web_search" => require("query"),
            "lsp_query" => {
                require("file")?;
                require("method")
            }
            _ => Ok(()),
        }
    }

    fn truncate_output(&self, output: String) -> String {
        if output.len() <= self.max_output_bytes {
            return output;
        }
        // Persist large output to disk instead of truncating
        let path = std::env::temp_dir().join(format!(
            "mc-output-{}.txt",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        ));
        if std::fs::write(&path, &output).is_ok() {
            let preview_end = output
                .char_indices()
                .map(|(i, _)| i)
                .take_while(|&i| i <= 2000)
                .last()
                .unwrap_or(0);
            format!(
                "{}...\n\n[Full output ({} bytes) saved to: {}]\n[Use read_file to access specific sections]",
                &output[..preview_end],
                output.len(),
                path.display()
            )
        } else {
            // Fallback: truncate
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
        assert_eq!(specs.len(), 26);
        let names: Vec<_> = specs.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"bash"));
        assert!(names.contains(&"edit_file"));
        assert!(names.contains(&"subagent"));
        assert!(names.contains(&"web_fetch"));
        assert!(names.contains(&"web_search"));
        assert!(names.contains(&"lsp_query"));
    }

    #[test]
    fn truncates_large_output() {
        let reg = ToolRegistry::new().with_max_output(20);
        let result = reg.truncate_output("a".repeat(100));
        // Large output is now persisted to disk with preview
        assert!(result.contains("saved to") || result.contains("truncated"));
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
