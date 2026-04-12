use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Permissionmode.
pub enum PermissionMode {
    Allow,
    Deny,
    Prompt,
    /// Auto-classify: read tools auto-allow, safe bash auto-allow, dangerous deny.
    Auto,
}

#[derive(Debug, Clone)]
/// Permissionrequest.
pub struct PermissionRequest {
    pub tool_name: String,
    pub input_summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Permissionoutcome.
pub enum PermissionOutcome {
    Allow,
    Deny { reason: String },
}

/// Permissionprompter.
pub trait PermissionPrompter: Send {
    fn decide(&mut self, request: &PermissionRequest) -> PermissionOutcome;
}

#[derive(Clone)]
/// Permissionpolicy.
pub struct PermissionPolicy {
    default_mode: PermissionMode,
    tool_modes: BTreeMap<String, PermissionMode>,
}

impl PermissionPolicy {
    #[must_use]
    /// New.
    pub fn new(default_mode: PermissionMode) -> Self {
        Self {
            default_mode,
            tool_modes: BTreeMap::new(),
        }
    }

    #[must_use]
    /// Get default permission mode.
    pub fn mode(&self) -> PermissionMode {
        self.default_mode
    }

    #[must_use]
    /// With tool mode.
    pub fn with_tool_mode(mut self, tool: impl Into<String>, mode: PermissionMode) -> Self {
        self.tool_modes.insert(tool.into(), mode);
        self
    }

    #[must_use]
    /// Authorize.
    pub fn authorize(
        &self,
        tool_name: &str,
        input_summary: &str,
        prompter: Option<&mut dyn PermissionPrompter>,
    ) -> PermissionOutcome {
        let mode = self
            .tool_modes
            .get(tool_name)
            .copied()
            .unwrap_or(self.default_mode);
        match mode {
            PermissionMode::Allow => PermissionOutcome::Allow,
            PermissionMode::Deny => PermissionOutcome::Deny {
                reason: format!("tool '{tool_name}' denied by policy"),
            },
            PermissionMode::Auto => auto_classify(tool_name, input_summary, prompter),
            PermissionMode::Prompt => match prompter {
                Some(p) => p.decide(&PermissionRequest {
                    tool_name: tool_name.to_string(),
                    input_summary: input_summary.to_string(),
                }),
                None => PermissionOutcome::Deny {
                    reason: format!("tool '{tool_name}' requires interactive approval"),
                },
            },
        }
    }
}

/// Auto-classify tool permissions based on safety heuristics.
fn auto_classify(
    tool_name: &str,
    input_summary: &str,
    prompter: Option<&mut dyn PermissionPrompter>,
) -> PermissionOutcome {
    // Read tools: always allow
    if matches!(
        tool_name,
        "read_file"
            | "glob_search"
            | "grep_search"
            | "web_fetch"
            | "web_search"
            | "lsp_query"
            | "memory_read"
    ) {
        return PermissionOutcome::Allow;
    }
    // Write/edit in workspace: allow
    if matches!(tool_name, "write_file" | "edit_file" | "memory_write") {
        return PermissionOutcome::Allow;
    }
    // Bash: deep command classification
    if tool_name == "bash" {
        return classify_bash_command(input_summary, prompter);
    }
    // Everything else: prompt
    match prompter {
        Some(p) => p.decide(&PermissionRequest {
            tool_name: tool_name.to_string(),
            input_summary: input_summary.to_string(),
        }),
        None => PermissionOutcome::Deny {
            reason: format!("tool '{tool_name}' requires approval"),
        },
    }
}

/// Deep bash command classification with compound command support.
fn classify_bash_command(
    input: &str,
    prompter: Option<&mut dyn PermissionPrompter>,
) -> PermissionOutcome {
    let cmd = input.trim();

    // Split compound commands outside of quotes
    let parts = split_shell_commands(cmd);

    let mut needs_prompt = false;
    for part in &parts {
        match classify_single_command(part.trim()) {
            CommandRisk::Safe => {}
            CommandRisk::Dangerous(reason) => {
                return PermissionOutcome::Deny {
                    reason: format!("blocked: {reason}"),
                };
            }
            CommandRisk::NeedsReview => needs_prompt = true,
        }
    }

    if needs_prompt {
        match prompter {
            Some(p) => p.decide(&PermissionRequest {
                tool_name: "bash".into(),
                input_summary: input.to_string(),
            }),
            None => PermissionOutcome::Deny {
                reason: "bash command requires approval".into(),
            },
        }
    } else {
        PermissionOutcome::Allow
    }
}

/// Split shell command on ;, &&, || operators while respecting quotes.
fn split_shell_commands(cmd: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut in_single = false;
    let mut in_double = false;
    let bytes = cmd.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'\'' && !in_double {
            in_single = !in_single;
        } else if c == b'"' && !in_single {
            in_double = !in_double;
        } else if !in_single && !in_double {
            if c == b';' {
                parts.push(&cmd[start..i]);
                start = i + 1;
            } else if c == b'&' && i + 1 < bytes.len() && bytes[i + 1] == b'&' {
                parts.push(&cmd[start..i]);
                start = i + 2;
                i += 1;
            } else if c == b'|' && i + 1 < bytes.len() && bytes[i + 1] == b'|' {
                parts.push(&cmd[start..i]);
                start = i + 2;
                i += 1;
            } else if c == b'|' {
                parts.push(&cmd[start..i]);
                start = i + 1;
            }
        }
        i += 1;
    }
    if start < cmd.len() {
        parts.push(&cmd[start..]);
    }
    parts
}

enum CommandRisk {
    Safe,
    NeedsReview,
    Dangerous(String),
}

fn classify_single_command(cmd: &str) -> CommandRisk {
    const DANGEROUS: &[&str] = &[
        "sudo",
        "su",
        "mkfs",
        "fdisk",
        "mount",
        "umount",
        "iptables",
        "systemctl",
        "service",
        "shutdown",
        "reboot",
        "passwd",
        "useradd",
        "userdel",
    ];
    const DANGEROUS_PATTERNS: &[&str] = &[
        "rm -rf /",
        "rm -rf ~",
        "rm -rf /*",
        "> /dev/",
        "curl|sh",
        "curl|bash",
        "wget|sh",
        "wget|bash",
        "dd if=",
        "chmod 777",
        "eval $(",
        "`curl",
        "$(curl",
    ];
    const SAFE: &[&str] = &[
        "ls",
        "cat",
        "head",
        "tail",
        "wc",
        "grep",
        "rg",
        "find",
        "fd",
        "echo",
        "printf",
        "pwd",
        "env",
        "which",
        "type",
        "date",
        "whoami",
        "hostname",
        "uname",
        "file",
        "stat",
        "diff",
        "sort",
        "uniq",
        "tr",
        "cut",
        "awk",
        "sed",
        "jq",
        "tree",
        "du",
        "df",
        "free",
        "top",
        "ps",
        "lsof",
        "pgrep",
        "git status",
        "git log",
        "git diff",
        "git show",
        "git branch",
        "git remote",
        "git tag",
        "git stash list",
        "git blame",
    ];
    const BUILD_SAFE: &[&str] = &[
        "cargo test",
        "cargo build",
        "cargo check",
        "cargo clippy",
        "cargo fmt",
        "cargo run",
        "cargo doc",
        "cargo bench",
        "npm test",
        "npm run",
        "npm install",
        "npm ci",
        "npx",
        "yarn test",
        "yarn build",
        "yarn install",
        "pnpm test",
        "pnpm run",
        "pnpm install",
        "python",
        "python3",
        "pytest",
        "pip install",
        "go test",
        "go build",
        "go run",
        "go vet",
        "make",
        "cmake",
        "gradle",
        "mvn",
        "node",
        "deno",
        "bun",
        "rustc",
        "gcc",
        "g++",
        "clang",
    ];
    const GIT_WRITE_SAFE: &[&str] = &[
        "git add",
        "git commit",
        "git push",
        "git pull",
        "git fetch",
        "git checkout",
        "git switch",
        "git merge",
        "git rebase",
        "git stash",
        "git cherry-pick",
        "git reset",
    ];

    let first_word = cmd.split_whitespace().next().unwrap_or("");

    if DANGEROUS.contains(&first_word) {
        return CommandRisk::Dangerous(format!("{first_word} blocked"));
    }
    for pat in DANGEROUS_PATTERNS {
        if cmd.contains(pat) {
            return CommandRisk::Dangerous(format!("dangerous: {pat}"));
        }
    }
    if SAFE.iter().any(|s| cmd.starts_with(s)) {
        return CommandRisk::Safe;
    }
    if BUILD_SAFE.iter().any(|s| cmd.starts_with(s)) {
        return CommandRisk::Safe;
    }
    if GIT_WRITE_SAFE.iter().any(|s| cmd.starts_with(s)) {
        return CommandRisk::Safe;
    }
    CommandRisk::NeedsReview
}

#[cfg(test)]
mod tests {
    use super::*;

    struct AllowAll;
    impl PermissionPrompter for AllowAll {
        fn decide(&mut self, _: &PermissionRequest) -> PermissionOutcome {
            PermissionOutcome::Allow
        }
    }

    #[test]
    fn allow_mode_passes() {
        let policy = PermissionPolicy::new(PermissionMode::Allow);
        assert_eq!(
            policy.authorize("bash", "ls", None),
            PermissionOutcome::Allow
        );
    }

    #[test]
    fn deny_mode_blocks() {
        let policy = PermissionPolicy::new(PermissionMode::Deny);
        assert!(matches!(
            policy.authorize("bash", "rm -rf", None),
            PermissionOutcome::Deny { .. }
        ));
    }

    #[test]
    fn prompt_mode_delegates() {
        let policy = PermissionPolicy::new(PermissionMode::Prompt);
        assert_eq!(
            policy.authorize("bash", "ls", Some(&mut AllowAll)),
            PermissionOutcome::Allow
        );
    }

    #[test]
    fn tool_override() {
        let policy = PermissionPolicy::new(PermissionMode::Allow)
            .with_tool_mode("bash", PermissionMode::Deny);
        assert!(matches!(
            policy.authorize("bash", "ls", None),
            PermissionOutcome::Deny { .. }
        ));
        assert_eq!(
            policy.authorize("read_file", "x", None),
            PermissionOutcome::Allow
        );
    }
}
