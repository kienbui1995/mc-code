use std::path::{Path, PathBuf};

use crate::error::ToolError;

/// Validates that file operations stay within the project root.
pub struct Sandbox {
    root: PathBuf,
    pub(crate) protected: Vec<String>,
}

impl Sandbox {
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            protected: vec![
                ".env".into(),
                ".env.*".into(),
                ".git/*".into(),
                "*.pem".into(),
                "*.key".into(),
                "id_rsa*".into(),
            ],
        }
    }

    /// Add extra protected patterns from config.
    #[must_use]
    pub fn with_protected(mut self, patterns: Vec<String>) -> Self {
        self.protected.extend(patterns);
        self
    }

    /// Check if a path is within the sandbox. Resolves `..` and symlinks.
    pub fn check(&self, path: &str) -> Result<PathBuf, ToolError> {
        let target = if Path::new(path).is_absolute() {
            PathBuf::from(path)
        } else {
            self.root.join(path)
        };

        // Canonicalize what exists, then check prefix
        let resolved = resolve_path(&target);

        if resolved.starts_with(&self.root) {
            // Check protected patterns
            let rel = resolved
                .strip_prefix(&self.root)
                .unwrap_or(&resolved)
                .to_string_lossy();
            for pat in &self.protected {
                if glob_match(pat, &rel) {
                    return Err(ToolError::PermissionDenied(format!(
                        "path '{path}' matches protected pattern '{pat}'"
                    )));
                }
            }
            Ok(resolved)
        } else {
            Err(ToolError::PermissionDenied(format!(
                "path '{}' is outside workspace root '{}'",
                path,
                self.root.display()
            )))
        }
    }
}

/// Resolve a path as much as possible without requiring it to exist.
fn resolve_path(path: &Path) -> PathBuf {
    // Try full canonicalize first
    if let Ok(canon) = path.canonicalize() {
        return canon;
    }
    // If file doesn't exist yet, canonicalize the parent
    if let Some(parent) = path.parent() {
        if let Ok(canon_parent) = parent.canonicalize() {
            if let Some(name) = path.file_name() {
                return canon_parent.join(name);
            }
        }
    }
    // Fallback: normalize manually
    normalize(path)
}

/// Simple glob matching: `*` matches any chars in a segment, `*/*` matches path separators.
fn glob_match(pattern: &str, path: &str) -> bool {
    if pattern.contains('/') || pattern.contains('*') {
        // Use glob crate for proper matching
        glob::Pattern::new(pattern).is_ok_and(|p| p.matches(path))
    } else {
        // Exact filename match
        path == pattern || path.ends_with(&format!("/{pattern}"))
    }
}

/// Simple path normalization (resolve `.` and `..` without filesystem access).
fn normalize(path: &Path) -> PathBuf {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                parts.pop();
            }
            std::path::Component::CurDir => {}
            c => parts.push(c.as_os_str().to_owned()),
        }
    }
    parts.iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn allows_paths_within_root() {
        let dir = std::env::temp_dir().join(format!("mc-sandbox-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("test.txt"), "hi").unwrap();

        let sandbox = Sandbox::new(dir.clone());
        assert!(sandbox.check("test.txt").is_ok());
        assert!(sandbox.check("./subdir/new.txt").is_ok());

        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn blocks_path_traversal() {
        let dir = std::env::temp_dir().join(format!("mc-sandbox-trav-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();

        let sandbox = Sandbox::new(dir.clone());
        assert!(sandbox.check("../../etc/passwd").is_err());
        assert!(sandbox.check("/etc/passwd").is_err());

        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn allows_absolute_path_within_root() {
        let dir = std::env::temp_dir().join(format!("mc-sandbox-abs-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();

        let sandbox = Sandbox::new(dir.clone());
        let abs = dir.join("file.txt").to_string_lossy().to_string();
        assert!(sandbox.check(&abs).is_ok());

        fs::remove_dir_all(dir).ok();
    }
}
