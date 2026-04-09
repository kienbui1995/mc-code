use std::fs;
use std::path::Path;

use crate::error::ToolError;

/// Readfiletool.
pub struct ReadFileTool;
/// Writefiletool.
pub struct WriteFileTool;
/// Editfiletool.
pub struct EditFileTool;

impl ReadFileTool {
    /// Execute.
    pub fn execute(
        path: &str,
        offset: Option<usize>,
        limit: Option<usize>,
    ) -> Result<String, ToolError> {
        // Jupyter notebook support: extract cell sources as readable text
        if path.ends_with(".ipynb") {
            let content = std::fs::read_to_string(path)
                .map_err(|e| ToolError::ExecutionFailed(format!("read {path}: {e}")))?;
            return Ok(extract_notebook_cells(&content));
        }

        let content = fs::read_to_string(path).map_err(ToolError::Io)?;
        let lines: Vec<&str> = content.lines().collect();
        let total = lines.len();

        // Large file preview: if >500 lines and no specific range requested
        if total > 500 && offset.is_none() && limit.is_none() {
            let first: Vec<&str> = lines[..50].to_vec();
            let last: Vec<&str> = lines[total - 20..].to_vec();
            let size = content.len();
            return Ok(format!(
                "File: {path} ({total} lines, {size} bytes) [PREVIEW — use offset/limit for full content]\n\n{}\n\n... [{} lines omitted] ...\n\n{}",
                first.join("\n"),
                total - 70,
                last.join("\n"),
            ));
        }

        let start = offset.unwrap_or(0).min(total);
        let end = limit.map_or(total, |l| (start + l).min(total));
        let selected: Vec<&str> = lines[start..end].to_vec();
        Ok(format!(
            "File: {path}\nLines {}-{} of {total}\n\n{}",
            start + 1,
            end,
            selected.join("\n")
        ))
    }
}

impl WriteFileTool {
    /// Execute.
    pub fn execute(path: &str, content: &str) -> Result<String, ToolError> {
        if let Some(parent) = Path::new(path).parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).map_err(ToolError::Io)?;
            }
        }
        fs::write(path, content).map_err(ToolError::Io)?;
        let line_count = content.lines().count();
        Ok(format!("Wrote {line_count} lines to {path}"))
    }
}

impl EditFileTool {
    /// Execute.
    pub fn execute(
        path: &str,
        old_string: &str,
        new_string: &str,
        replace_all: bool,
    ) -> Result<String, ToolError> {
        let original = fs::read_to_string(path).map_err(ToolError::Io)?;

        if !original.contains(old_string) {
            return Err(ToolError::InvalidInput(format!(
                "old_string not found in {path}"
            )));
        }

        let updated = if replace_all {
            original.replace(old_string, new_string)
        } else {
            original.replacen(old_string, new_string, 1)
        };

        let diff = generate_diff(path, &original, &updated);
        fs::write(path, &updated).map_err(ToolError::Io)?;
        Ok(diff)
    }
}

/// Batch edit: apply multiple edits in one call.
pub struct BatchEditTool;

impl BatchEditTool {
    /// Apply multiple edits. Each edit is {path, old_string, new_string, replace_all?}.
    /// Edits are applied in order. If any fails, the batch is aborted (no partial writes).
    pub fn execute(edits: &[serde_json::Value]) -> Result<String, ToolError> {
        let mut planned: Vec<(String, String, String)> = Vec::new();
        for edit in edits {
            let path = edit
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidInput("edit missing 'path'".into()))?;
            let old_str = edit
                .get("old_string")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidInput("edit missing 'old_string'".into()))?;
            let new_str = edit
                .get("new_string")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidInput("edit missing 'new_string'".into()))?;
            let content = std::fs::read_to_string(path)
                .map_err(|e| ToolError::ExecutionFailed(format!("read {path}: {e}")))?;
            if !content.contains(old_str) {
                return Err(ToolError::ExecutionFailed(format!(
                    "old_string not found in {path}"
                )));
            }
            planned.push((path.to_string(), old_str.to_string(), new_str.to_string()));
        }
        let mut count = 0;
        for (path, old_str, new_str) in &planned {
            let replace_all = edits
                .get(count)
                .and_then(|e| e.get("replace_all"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            EditFileTool::execute(path, old_str, new_str, replace_all)?;
            count += 1;
        }
        Ok(format!("Applied {count} edits successfully"))
    }
}

/// Apply a unified diff patch to files.
pub struct ApplyPatchTool;

impl ApplyPatchTool {
    /// Apply a unified diff patch using `git apply`.
    pub fn execute(patch: &str) -> Result<String, ToolError> {
        let tmp = std::env::temp_dir().join(format!("mc-patch-{}.diff", std::process::id()));
        std::fs::write(&tmp, patch)
            .map_err(|e| ToolError::ExecutionFailed(format!("write temp: {e}")))?;

        let stat = std::process::Command::new("git")
            .args(["apply", "--stat", &tmp.to_string_lossy()])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default();

        let result = std::process::Command::new("git")
            .args(["apply", &tmp.to_string_lossy()])
            .output();

        let _ = std::fs::remove_file(&tmp);

        match result {
            Ok(o) if o.status.success() => Ok(format!("Patch applied successfully\n{stat}")),
            Ok(o) => {
                let err = String::from_utf8_lossy(&o.stderr);
                Err(ToolError::ExecutionFailed(format!(
                    "git apply failed: {err}"
                )))
            }
            Err(e) => Err(ToolError::ExecutionFailed(format!("git apply: {e}"))),
        }
    }
}

fn generate_diff(path: &str, old: &str, new: &str) -> String {
    use similar::{ChangeTag, TextDiff};
    let diff = TextDiff::from_lines(old, new);
    let mut out = format!("--- {path}\n+++ {path}\n");
    for change in diff.iter_all_changes() {
        let prefix = match change.tag() {
            ChangeTag::Delete => "-",
            ChangeTag::Insert => "+",
            ChangeTag::Equal => " ",
        };
        out.push_str(prefix);
        out.push_str(change.as_str().unwrap_or(""));
        if !change.as_str().unwrap_or("").ends_with('\n') {
            out.push('\n');
        }
    }
    out
}

/// Extract cell sources from a Jupyter notebook JSON.
fn extract_notebook_cells(json: &str) -> String {
    let Ok(nb) = serde_json::from_str::<serde_json::Value>(json) else {
        return json.to_string(); // fallback: return raw JSON
    };
    let Some(cells) = nb.get("cells").and_then(|c| c.as_array()) else {
        return json.to_string();
    };
    let mut output = String::new();
    for (i, cell) in cells.iter().enumerate() {
        let cell_type = cell
            .get("cell_type")
            .and_then(|t| t.as_str())
            .unwrap_or("unknown");
        let source = cell
            .get("source")
            .map(|s| {
                if let Some(arr) = s.as_array() {
                    arr.iter().filter_map(|v| v.as_str()).collect::<String>()
                } else {
                    s.as_str().unwrap_or("").to_string()
                }
            })
            .unwrap_or_default();
        if !source.trim().is_empty() {
            output.push_str(&format!("# Cell {} [{}]\n{}\n\n", i + 1, cell_type, source));
        }
    }
    if output.is_empty() {
        json.to_string()
    } else {
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp(name: &str) -> String {
        format!("/tmp/mc-test-{}-{}", name, std::process::id())
    }

    #[test]
    fn read_write_roundtrip() {
        let path = tmp("rw");
        WriteFileTool::execute(&path, "line1\nline2\nline3").unwrap();
        let out = ReadFileTool::execute(&path, None, None).unwrap();
        assert!(out.contains("line1"));
        assert!(out.contains("Lines 1-3 of 3"));
        fs::remove_file(&path).ok();
    }

    #[test]
    fn read_with_offset_limit() {
        let path = tmp("offset");
        WriteFileTool::execute(&path, "a\nb\nc\nd\ne").unwrap();
        let out = ReadFileTool::execute(&path, Some(1), Some(2)).unwrap();
        assert!(out.contains("Lines 2-3 of 5"));
        assert!(out.contains('b'));
        assert!(out.contains('c'));
        fs::remove_file(&path).ok();
    }

    #[test]
    fn edit_generates_diff() {
        let path = tmp("edit");
        WriteFileTool::execute(&path, "hello world\nfoo bar").unwrap();
        let diff = EditFileTool::execute(&path, "hello", "goodbye", false).unwrap();
        assert!(diff.contains("-hello world"));
        assert!(diff.contains("+goodbye world"));
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("goodbye world"));
        fs::remove_file(&path).ok();
    }

    #[test]
    fn edit_not_found_errors() {
        let path = tmp("editnf");
        WriteFileTool::execute(&path, "abc").unwrap();
        let err = EditFileTool::execute(&path, "xyz", "new", false).unwrap_err();
        assert!(matches!(err, ToolError::InvalidInput(_)));
        fs::remove_file(&path).ok();
    }
}
