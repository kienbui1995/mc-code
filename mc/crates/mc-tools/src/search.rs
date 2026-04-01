use std::path::Path;

use crate::error::ToolError;

pub struct GlobSearchTool;
pub struct GrepSearchTool;

impl GlobSearchTool {
    pub fn execute(pattern: &str, base_path: Option<&str>) -> Result<String, ToolError> {
        let full_pattern = match base_path {
            Some(base) => format!("{base}/{pattern}"),
            None => pattern.to_string(),
        };
        let paths: Vec<String> = glob::glob(&full_pattern)
            .map_err(|e| ToolError::InvalidInput(e.to_string()))?
            .filter_map(Result::ok)
            .map(|p| p.display().to_string())
            .collect();
        Ok(format!("Found {} files:\n{}", paths.len(), paths.join("\n")))
    }
}

impl GrepSearchTool {
    pub fn execute(pattern: &str, path: Option<&str>, file_glob: Option<&str>) -> Result<String, ToolError> {
        let re = regex::Regex::new(pattern)
            .map_err(|e| ToolError::InvalidInput(format!("invalid regex: {e}")))?;

        let base = Path::new(path.unwrap_or("."));
        let glob_pattern = file_glob.unwrap_or("*");
        let glob_matcher = glob::Pattern::new(glob_pattern)
            .map_err(|e| ToolError::InvalidInput(e.to_string()))?;

        let mut results = Vec::new();
        for entry in walkdir::WalkDir::new(base).max_depth(10).into_iter().filter_map(Result::ok) {
            if !entry.file_type().is_file() {
                continue;
            }
            let fname = entry.file_name().to_string_lossy();
            if !glob_matcher.matches(&fname) {
                continue;
            }
            if let Ok(content) = std::fs::read_to_string(entry.path()) {
                for (i, line) in content.lines().enumerate() {
                    if re.is_match(line) {
                        results.push(format!("{}:{}: {}", entry.path().display(), i + 1, line));
                        if results.len() >= 100 {
                            results.push("... (truncated at 100 matches)".to_string());
                            return Ok(results.join("\n"));
                        }
                    }
                }
            }
        }

        if results.is_empty() {
            Ok("No matches found.".to_string())
        } else {
            Ok(format!("{} matches:\n{}", results.len(), results.join("\n")))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn glob_finds_files() {
        let dir = format!("/tmp/mc-glob-{}", std::process::id());
        fs::create_dir_all(&dir).unwrap();
        fs::write(format!("{dir}/a.txt"), "a").unwrap();
        fs::write(format!("{dir}/b.rs"), "b").unwrap();

        let out = GlobSearchTool::execute("*.txt", Some(&dir)).unwrap();
        assert!(out.contains("1 files"));
        assert!(out.contains("a.txt"));

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn grep_finds_matches() {
        let dir = format!("/tmp/mc-grep-{}", std::process::id());
        fs::create_dir_all(&dir).unwrap();
        fs::write(format!("{dir}/test.txt"), "hello world\nfoo bar\nhello again").unwrap();

        let out = GrepSearchTool::execute("hello", Some(&dir), Some("*.txt")).unwrap();
        assert!(out.contains("2 matches"));
        assert!(out.contains("hello world"));
        assert!(out.contains("hello again"));

        fs::remove_dir_all(&dir).ok();
    }
}
