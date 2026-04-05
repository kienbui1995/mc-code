use std::fs;
use std::path::Path;

use crate::error::ToolError;

pub struct ReadFileTool;
pub struct WriteFileTool;
pub struct EditFileTool;

impl ReadFileTool {
    pub fn execute(
        path: &str,
        offset: Option<usize>,
        limit: Option<usize>,
    ) -> Result<String, ToolError> {
        let content = fs::read_to_string(path).map_err(ToolError::Io)?;
        let lines: Vec<&str> = content.lines().collect();
        let start = offset.unwrap_or(0).min(lines.len());
        let end = limit.map_or(lines.len(), |l| (start + l).min(lines.len()));
        let selected: Vec<&str> = lines[start..end].to_vec();
        Ok(format!(
            "File: {path}\nLines {}-{} of {}\n\n{}",
            start + 1,
            end,
            lines.len(),
            selected.join("\n")
        ))
    }
}

impl WriteFileTool {
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
