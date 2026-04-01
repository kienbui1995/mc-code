use std::time::Duration;

use serde_json::Value;

use crate::bash::BashTool;
use crate::error::ToolError;
use crate::file_ops::{EditFileTool, ReadFileTool, WriteFileTool};
use crate::search::{GlobSearchTool, GrepSearchTool};
use crate::spec::{all_tool_specs, ToolSpec};

pub struct ToolRegistry;

impl ToolRegistry {
    #[must_use]
    pub fn specs() -> Vec<ToolSpec> {
        all_tool_specs()
    }

    pub async fn execute(name: &str, input: &Value) -> Result<String, ToolError> {
        tracing::debug!(tool = name, "executing tool");
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
                let path = str_field(input, "path")?;
                let offset = input.get("offset").and_then(Value::as_u64).map(|v| v as usize);
                let limit = input.get("limit").and_then(Value::as_u64).map(|v| v as usize);
                tokio::task::spawn_blocking(move || ReadFileTool::execute(&path, offset, limit))
                    .await
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?
            }
            "write_file" => {
                let path = str_field(input, "path")?;
                let content = str_field(input, "content")?;
                tokio::task::spawn_blocking(move || WriteFileTool::execute(&path, &content))
                    .await
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?
            }
            "edit_file" => {
                let path = str_field(input, "path")?;
                let old = str_field(input, "old_string")?;
                let new = str_field(input, "new_string")?;
                let all = input.get("replace_all").and_then(Value::as_bool).unwrap_or(false);
                tokio::task::spawn_blocking(move || EditFileTool::execute(&path, &old, &new, all))
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
            _ => Err(ToolError::NotFound(name.to_string())),
        }
    }
}

fn str_field(input: &Value, field: &str) -> Result<String, ToolError> {
    input
        .get(field)
        .and_then(Value::as_str)
        .map(String::from)
        .ok_or_else(|| ToolError::InvalidInput(format!("missing field: {field}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn executes_bash() {
        let out = ToolRegistry::execute("bash", &json!({"command": "echo hi"}))
            .await
            .unwrap();
        assert!(out.contains("hi"));
    }

    #[tokio::test]
    async fn rejects_unknown_tool() {
        let err = ToolRegistry::execute("nope", &json!({})).await.unwrap_err();
        assert!(matches!(err, ToolError::NotFound(_)));
    }

    #[test]
    fn specs_has_all_tools() {
        let specs = ToolRegistry::specs();
        assert_eq!(specs.len(), 7);
        let names: Vec<_> = specs.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"bash"));
        assert!(names.contains(&"edit_file"));
        assert!(names.contains(&"grep_search"));
        assert!(names.contains(&"subagent"));
    }
}
