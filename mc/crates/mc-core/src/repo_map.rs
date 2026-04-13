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
    /// Build.
    pub fn build(root: &Path) -> Self {
        let mut entries = BTreeMap::new();
        scan_dir(root, root, &mut entries);
        Self { entries }
    }

    /// Format as string for system prompt injection.
    #[must_use]
    /// To prompt section.
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
    /// File count.
    pub fn file_count(&self) -> usize {
        self.entries.len()
    }

    /// Search symbols and files by query terms. Returns ranked results.
    #[must_use]
    pub fn search(&self, query: &str, max_results: usize) -> Vec<SearchResult> {
        let terms: Vec<String> = query.split_whitespace().map(|t| t.to_lowercase()).collect();
        if terms.is_empty() {
            return Vec::new();
        }

        let mut results: Vec<SearchResult> = Vec::new();

        for (path, symbols) in &self.entries {
            let path_lower = path.to_lowercase();
            let mut score: f64 = 0.0;
            let mut matched_symbols = Vec::new();

            for term in &terms {
                // Path match (filename is worth more)
                if let Some(fname) = path.rsplit('/').next() {
                    if fname.to_lowercase().contains(term) {
                        score += 3.0;
                    } else if path_lower.contains(term) {
                        score += 1.0;
                    }
                }
                // Symbol match
                for sym in symbols {
                    let sym_lower = sym.to_lowercase();
                    if sym_lower.contains(term) {
                        score += 2.0;
                        if !matched_symbols.contains(sym) {
                            matched_symbols.push(sym.clone());
                        }
                    }
                }
            }

            if score > 0.0 {
                results.push(SearchResult {
                    path: path.clone(),
                    symbols: matched_symbols,
                    score,
                });
            }
        }

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(max_results);
        results
    }
}

/// A search result from the codebase index.
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub path: String,
    pub symbols: Vec<String>,
    pub score: f64,
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

    // Try tree-sitter first (accurate AST), fall back to regex
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let symbols = extract_symbols_treesitter(ext, &content)
        .unwrap_or_else(|| extract_symbols_regex(&content));

    let mut symbols = symbols;
    symbols.truncate(30);
    symbols
}

fn extract_symbols_treesitter(ext: &str, source: &str) -> Option<Vec<String>> {
    let language = match ext {
        "rs" => tree_sitter_rust::LANGUAGE,
        "py" => tree_sitter_python::LANGUAGE,
        "js" | "jsx" => tree_sitter_javascript::LANGUAGE,
        "ts" | "tsx" => tree_sitter_typescript::LANGUAGE_TYPESCRIPT,
        "go" => tree_sitter_go::LANGUAGE,
        _ => return None,
    };

    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language.into()).ok()?;
    let tree = parser.parse(source, None)?;
    let root = tree.root_node();
    let bytes = source.as_bytes();

    let mut symbols = Vec::new();
    collect_symbols(&root, bytes, ext, &mut symbols);
    Some(symbols)
}

fn collect_symbols(node: &tree_sitter::Node, source: &[u8], ext: &str, symbols: &mut Vec<String>) {
    let kind = node.kind();
    let name = match ext {
        "rs" => match kind {
            "function_item" | "struct_item" | "enum_item" | "trait_item" | "impl_item" => {
                node.child_by_field_name("name").map(|n| {
                    let prefix = match kind {
                        "struct_item" => "struct ",
                        "enum_item" => "enum ",
                        "trait_item" => "trait ",
                        "impl_item" => "impl ",
                        _ => "fn ",
                    };
                    format!("{prefix}{}", n.utf8_text(source).unwrap_or(""))
                })
            }
            _ => None,
        },
        "py" => match kind {
            "function_definition" => node
                .child_by_field_name("name")
                .map(|n| format!("def {}", n.utf8_text(source).unwrap_or(""))),
            "class_definition" => node
                .child_by_field_name("name")
                .map(|n| format!("class {}", n.utf8_text(source).unwrap_or(""))),
            _ => None,
        },
        "js" | "jsx" | "ts" | "tsx" => match kind {
            "function_declaration" => node
                .child_by_field_name("name")
                .map(|n| n.utf8_text(source).unwrap_or("").to_string()),
            "class_declaration" => node
                .child_by_field_name("name")
                .map(|n| format!("class {}", n.utf8_text(source).unwrap_or(""))),
            "export_statement" => None, // recurse into children
            _ => None,
        },
        "go" => match kind {
            "function_declaration" => node
                .child_by_field_name("name")
                .map(|n| format!("func {}", n.utf8_text(source).unwrap_or(""))),
            "type_declaration" => node
                .child_by_field_name("name")
                .or_else(|| {
                    (0..node.child_count())
                        .filter_map(|i| node.child(i))
                        .find(|c| c.kind() == "type_spec")
                        .and_then(|ts| ts.child_by_field_name("name"))
                })
                .map(|n| format!("type {}", n.utf8_text(source).unwrap_or(""))),
            _ => None,
        },
        _ => None,
    };

    if let Some(name) = name {
        if !name.is_empty() {
            symbols.push(name);
        }
    }

    let mut cursor = node.walk();
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            collect_symbols(&child, source, ext, symbols);
        }
    }
}

/// Fallback regex extraction for unsupported languages.
fn extract_symbols_regex(content: &str) -> Vec<String> {
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

    #[test]
    fn search_finds_symbols() {
        let dir = std::env::temp_dir().join(format!("mc-search-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(
            dir.join("src/auth.rs"),
            "pub fn authenticate(user: &str) {}\npub struct AuthConfig {}",
        )
        .unwrap();
        std::fs::write(dir.join("src/main.rs"), "pub fn main() {}").unwrap();

        let map = RepoMap::build(&dir);
        let results = map.search("auth", 5);
        assert!(!results.is_empty());
        assert!(results[0].path.contains("auth"));
        assert!(results[0].score > 0.0);

        let results2 = map.search("nonexistent_xyz", 5);
        assert!(results2.is_empty());

        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn search_ranks_by_relevance() {
        let dir = std::env::temp_dir().join(format!("mc-rank-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("src/handler.rs"), "pub fn handle_request() {}").unwrap();
        std::fs::write(dir.join("src/utils.rs"), "pub fn helper() {}").unwrap();

        let map = RepoMap::build(&dir);
        let results = map.search("handler handle", 5);
        assert!(!results.is_empty());
        // handler.rs should rank higher (path + symbol match)
        assert!(results[0].path.contains("handler"));

        std::fs::remove_dir_all(dir).ok();
    }
}
