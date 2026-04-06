use std::path::{Path, PathBuf};
use std::process::Command;

/// Discovered project context for system prompt construction.
#[derive(Debug, Clone)]
/// Projectcontext.
pub struct ProjectContext {
    pub cwd: PathBuf,
    pub git_status: Option<String>,
    pub instruction_files: Vec<InstructionFile>,
    pub detected_stack: Vec<String>,
}

#[derive(Debug, Clone)]
/// Instructionfile.
pub struct InstructionFile {
    pub path: PathBuf,
    pub content: String,
}

impl ProjectContext {
    #[must_use]
    /// Discover.
    pub fn discover(cwd: &Path) -> Self {
        let instruction_files = discover_instruction_files(cwd);
        let git_status = read_git_status(cwd);
        let detected_stack = detect_stack(cwd);

        Self {
            cwd: cwd.to_path_buf(),
            git_status,
            instruction_files,
            detected_stack,
        }
    }
}

const INSTRUCTION_FILE_NAMES: &[&str] = &[
    "MAGIC_CODE.md",
    "AGENTS.md",
    "CLAUDE.md",
    ".cursorrules",
    ".magic-code/instructions.md",
];

fn discover_instruction_files(cwd: &Path) -> Vec<InstructionFile> {
    INSTRUCTION_FILE_NAMES
        .iter()
        .filter_map(|name| {
            let path = cwd.join(name);
            std::fs::read_to_string(&path).ok().map(|content| {
                let resolved = resolve_imports(&content, path.parent().unwrap_or(cwd), 0);
                InstructionFile {
                    path,
                    content: resolved,
                }
            })
        })
        .collect()
}

/// Resolve `@path/to/file.md` imports in instruction files (max depth 3).
fn resolve_imports(content: &str, base: &Path, depth: u8) -> String {
    if depth >= 3 {
        return content.to_string();
    }
    let mut result = String::with_capacity(content.len());
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(import_path) = trimmed.strip_prefix('@') {
            let resolved = base.join(import_path);
            if let Ok(imported) = std::fs::read_to_string(&resolved) {
                result.push_str(&resolve_imports(
                    &imported,
                    resolved.parent().unwrap_or(base),
                    depth + 1,
                ));
            } else {
                result.push_str(line);
            }
        } else {
            result.push_str(line);
        }
        result.push('\n');
    }
    result
}

fn read_git_status(cwd: &Path) -> Option<String> {
    Command::new("git")
        .args(["status", "--short"])
        .current_dir(cwd)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

fn detect_stack(cwd: &Path) -> Vec<String> {
    let markers: &[(&str, &str)] = &[
        ("Cargo.toml", "rust"),
        ("package.json", "node"),
        ("pyproject.toml", "python"),
        ("go.mod", "go"),
        ("Gemfile", "ruby"),
        ("pom.xml", "java-maven"),
        ("build.gradle", "java-gradle"),
    ];

    markers
        .iter()
        .filter(|(file, _)| cwd.join(file).exists())
        .map(|(_, stack)| (*stack).to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn detects_rust_project() {
        let dir = std::env::temp_dir().join(format!("mc-project-test-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("create dir");
        fs::write(dir.join("Cargo.toml"), "[package]\nname = \"test\"").expect("write");

        let ctx = ProjectContext::discover(&dir);
        assert!(ctx.detected_stack.contains(&"rust".to_string()));

        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn reads_instruction_files() {
        let dir = std::env::temp_dir().join(format!("mc-instr-test-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("create dir");
        fs::write(dir.join("MAGIC_CODE.md"), "# Project rules").expect("write");

        let ctx = ProjectContext::discover(&dir);
        assert_eq!(ctx.instruction_files.len(), 1);
        assert!(ctx.instruction_files[0].content.contains("Project rules"));

        fs::remove_dir_all(dir).ok();
    }
}
