use std::collections::BTreeMap;
use std::path::Path;

/// Lightweight codebase index: file tree + extracted symbols.
/// Injected into system prompt so LLM knows the project structure.
pub struct RepoMap {
    entries: BTreeMap<String, Vec<String>>, // relative_path → symbols
}

impl RepoMap {
    /// Scan workspace and build repo map. Respects .gitignore via simple heuristics.
    #[must_use]
    pub fn build(root: &Path) -> Self {
        let mut entries = BTreeMap::new();
        scan_dir(root, root, &mut entries);
        Self { entries }
    }

    /// Format as string for system prompt injection.
    #[must_use]
    pub fn to_prompt_section(&self) -> String {
        if self.entries.is_empty() {
            return String::new();
        }
        let mut lines = Vec::new();
        for (path, symbols) in &self.entries {
            if symbols.is_empty() {
                lines.push(format!("  {path}"));
            } else {
                lines.push(format!("  {path}: {}", symbols.join(", ")));
            }
        }
        // Cap at ~4K chars to avoid bloating system prompt
        let mut result = String::from("\n\n## Repository Map\n");
        let mut total = result.len();
        for line in &lines {
            if total + line.len() > 4000 {
                result.push_str(&format!("  ... and {} more files\n", lines.len()));
                break;
            }
            result.push_str(line);
            result.push('\n');
            total += line.len() + 1;
        }
        result
    }

    #[must_use]
    pub fn file_count(&self) -> usize {
        self.entries.len()
    }
}

fn scan_dir(dir: &Path, root: &Path, entries: &mut BTreeMap<String, Vec<String>>) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        // Skip hidden, build artifacts, deps
        if name.starts_with('.') || SKIP_DIRS.contains(&name.as_str()) {
            continue;
        }

        if path.is_dir() {
            scan_dir(&path, root, entries);
        } else if is_code_file(&name) {
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .display()
                .to_string();
            let symbols = extract_symbols(&path);
            entries.insert(rel, symbols);
        }
    }
}

const SKIP_DIRS: &[&str] = &[
    "target",
    "node_modules",
    "dist",
    "build",
    ".git",
    "__pycache__",
    "vendor",
    ".next",
    "out",
    "coverage",
];

fn is_code_file(name: &str) -> bool {
    matches!(
        name.rsplit('.').next(),
        Some(
            "rs" | "py"
                | "ts"
                | "tsx"
                | "js"
                | "jsx"
                | "go"
                | "rb"
                | "java"
                | "c"
                | "h"
                | "cpp"
                | "toml"
                | "yaml"
                | "yml"
                | "md"
        )
    )
}

/// Extract top-level symbol names via simple regex-like line scanning.
fn extract_symbols(path: &Path) -> Vec<String> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut symbols = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(name) = extract_rust_symbol(trimmed)
            .or_else(|| extract_python_symbol(trimmed))
            .or_else(|| extract_ts_symbol(trimmed))
        {
            symbols.push(name);
        }
    }
    // Cap symbols per file
    symbols.truncate(20);
    symbols
}

fn extract_rust_symbol(line: &str) -> Option<String> {
    for prefix in [
        "pub fn ",
        "pub async fn ",
        "fn ",
        "pub struct ",
        "pub enum ",
        "pub trait ",
        "impl ",
    ] {
        if let Some(rest) = line.strip_prefix(prefix) {
            return rest
                .split(['(', '<', ' ', '{', ':'])
                .next()
                .filter(|s| !s.is_empty())
                .map(|s| {
                    format!(
                        "{}{s}",
                        if prefix.contains("struct") {
                            "struct "
                        } else if prefix.contains("enum") {
                            "enum "
                        } else if prefix.contains("trait") {
                            "trait "
                        } else if prefix.contains("impl") {
                            "impl "
                        } else {
                            "fn "
                        }
                    )
                });
        }
    }
    None
}

fn extract_python_symbol(line: &str) -> Option<String> {
    if let Some(rest) = line.strip_prefix("def ") {
        return rest.split('(').next().map(|s| format!("def {s}"));
    }
    if let Some(rest) = line.strip_prefix("class ") {
        return rest.split(['(', ':']).next().map(|s| format!("class {s}"));
    }
    None
}

fn extract_ts_symbol(line: &str) -> Option<String> {
    for prefix in [
        "export function ",
        "export async function ",
        "export class ",
        "export interface ",
        "function ",
    ] {
        if let Some(rest) = line.strip_prefix(prefix) {
            return rest
                .split(['(', '<', ' ', '{', ':'])
                .next()
                .filter(|s| !s.is_empty())
                .map(String::from);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_rust_symbols() {
        assert_eq!(
            extract_rust_symbol("pub fn hello(x: i32)"),
            Some("fn hello".into())
        );
        assert_eq!(
            extract_rust_symbol("pub struct Foo {"),
            Some("struct Foo".into())
        );
        assert_eq!(extract_rust_symbol("impl Bar {"), Some("impl Bar".into()));
        assert!(extract_rust_symbol("let x = 5;").is_none());
    }

    #[test]
    fn extract_python_symbols() {
        assert_eq!(
            extract_python_symbol("def main():"),
            Some("def main".into())
        );
        assert_eq!(
            extract_python_symbol("class MyApp(Base):"),
            Some("class MyApp".into())
        );
        assert!(extract_python_symbol("x = 5").is_none());
    }

    #[test]
    fn build_repo_map() {
        let dir = std::env::temp_dir().join(format!("mc-repo-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(
            dir.join("src/main.rs"),
            "pub fn main() {}\npub struct App {}",
        )
        .unwrap();
        std::fs::write(dir.join("README.md"), "# Hello").unwrap();

        let map = RepoMap::build(&dir);
        assert!(map.file_count() >= 2);
        let section = map.to_prompt_section();
        assert!(section.contains("Repository Map"));
        assert!(section.contains("fn main"));

        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn prompt_section_empty_for_empty_dir() {
        let dir = std::env::temp_dir().join(format!("mc-repo-empty-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let map = RepoMap::build(&dir);
        assert_eq!(map.to_prompt_section(), "");
        std::fs::remove_dir_all(dir).ok();
    }
}
