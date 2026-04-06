use crate::history::InputHistory;
use crate::input::InputBuffer;

pub enum AppEvent {
    UserSubmit(String),
    SlashCommand(String),
    Quit,
    StreamDelta(String),
    StreamDone,
    ToolCall(String),
    Error(String),
}

/// Messages sent from background LLM task to TUI.
#[derive(Debug, Clone)]
pub enum UiMessage {
    Delta(String),
    ToolCall(String),
    Usage {
        input: u32,
        output: u32,
    },
    Done {
        ttft_ms: u64,
        total_ms: u64,
    },
    Error(String),
    /// Permission prompt: tool name, input summary. TUI should respond via `permission_response`.
    PermissionPrompt {
        tool: String,
        input: String,
    },
    /// Stream failed, TUI should discard partial output from current attempt.
    StreamReset,
    /// Retry in progress after stream failure.
    RetryAttempt {
        attempt: u32,
        max: u32,
        reason: String,
    },
    /// Streaming tool output (e.g. bash stdout/stderr lines arriving in real-time).
    ToolOutputDelta(String),
}

/// Commands that need processing by main.rs (require runtime/provider access).
#[derive(Debug, Clone)]
/// Commands queued by slash commands, consumed by main.rs event loop.
pub enum PendingCommand {
    Compact,
    Save(String),
    Load(String),
    Undo,
    CostTotal,
    ImageAttach(String),
    Memory(String),
    ThinkingToggle,
    Branch(String),
    Git(String),
    ModelSwitch(String),
    Export,
    Init,
    Summary,
    Search(String),
    Doctor,
    Review,
    Retry,
    Tokens,
    Context,
    CopyToClipboard(String),
    Rewind(usize),
    Debug,
    Btw(String),
    Loop { interval_secs: u64, prompt: String },
    LoopStop,
}

/// Agent processing state.
#[derive(Debug, Clone, PartialEq, Eq)]
/// Current state of the AI agent.
pub enum AgentState {
    Idle,
    Streaming,
    ToolExecuting(String),
    WaitingPermission,
}

#[allow(clippy::struct_excessive_bools)]
pub struct App {
    pub input: InputBuffer,
    pub history: InputHistory,
    pub output_lines: Vec<String>,
    pub scroll_offset: u16,
    pub state: AgentState,
    pub model: String,
    pub total_input_tokens: u32,
    pub total_output_tokens: u32,
    pub session_cost: f64,
    pub context_usage_pct: u8,
    pub should_quit: bool,
    pub plan_mode: bool,
    pub dry_run: bool,
    /// Command queue — consumed by main.rs each frame.
    pub pending_command: Option<PendingCommand>,
    /// Whether user has manually scrolled up (disables auto-scroll).
    auto_scroll: bool,
    pub viewport_height: u16,
    /// Pending permission prompt (tool, input).
    pub permission_pending: Option<(String, String)>,
    pub permission_response: Option<bool>,
    pub always_allowed: std::collections::HashSet<String>,
    pub pinned_messages: Vec<usize>,
    pub last_user_input: Option<String>,
    pub ttft_ms: u64,
    pub turn_time_ms: u64,
    pub theme: String,
    pub aliases: std::collections::HashMap<String, String>,
    pub session_start: std::time::Instant,
    pub last_tool_output: Option<String>,
    /// Vim mode: None = disabled, Some(mode) = active.
    pub vim_mode: Option<crate::input::VimMode>,
    /// Transcript mode: show raw conversation.
    pub transcript_mode: bool,
    /// Custom commands loaded from .magic-code/commands/*.md.
    pub custom_commands: Vec<(String, String)>,
}

impl App {
    #[must_use]
    pub fn new(model: String) -> Self {
        let history_path = std::env::var_os("HOME")
            .map(|h| std::path::PathBuf::from(h).join(".local/share/magic-code/history"));
        let history = history_path.map_or_else(|| InputHistory::new(1000), InputHistory::load_from);

        Self {
            input: InputBuffer::default(),
            history,
            output_lines: vec![
                "Welcome to magic-code. Type /help for commands.".into(),
                random_tip().into(),
            ],
            scroll_offset: 0,
            model,
            total_input_tokens: 0,
            total_output_tokens: 0,
            session_cost: 0.0,
            context_usage_pct: 0,
            should_quit: false,
            plan_mode: false,
            dry_run: false,
            pending_command: None,
            auto_scroll: true,
            viewport_height: 20,
            permission_pending: None,
            permission_response: None,
            always_allowed: std::collections::HashSet::new(),
            pinned_messages: Vec::new(),
            last_user_input: None,
            ttft_ms: 0,
            turn_time_ms: 0,
            theme: "dark".into(),
            aliases: std::collections::HashMap::new(),
            session_start: std::time::Instant::now(),
            last_tool_output: None,
            vim_mode: None,
            transcript_mode: false,
            custom_commands: load_custom_commands(),
            state: AgentState::Idle,
        }
    }

    pub fn handle_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::UserSubmit(text) => {
                self.history.push(&text);
                self.output_lines.push(format!("\n› {text}"));
                self.output_lines.push(String::new());
                self.state = AgentState::Streaming;
                self.auto_scroll = true;
                self.scroll_to_bottom();
            }
            AppEvent::SlashCommand(cmd) => {
                self.handle_slash_command(&cmd);
                self.scroll_to_bottom();
            }
            AppEvent::Quit => self.should_quit = true,
            AppEvent::StreamDelta(text) => {
                // Split on newlines to handle multi-line deltas
                let parts: Vec<&str> = text.split('\n').collect();
                for (i, part) in parts.iter().enumerate() {
                    if i > 0 {
                        self.output_lines.push(String::new());
                    }
                    if let Some(last) = self.output_lines.last_mut() {
                        last.push_str(part);
                    } else {
                        self.output_lines.push((*part).to_string());
                    }
                }
                if self.auto_scroll {
                    self.scroll_to_bottom();
                }
            }
            AppEvent::StreamDone => {
                self.state = AgentState::Idle;
                self.output_lines.push(String::new());
                if self.auto_scroll {
                    self.scroll_to_bottom();
                }
            }
            AppEvent::ToolCall(name) => {
                self.state = AgentState::ToolExecuting(name.clone());
                self.last_tool_output = None;
                self.output_lines.push(format!("  ⚙ tool: {name}"));
                if self.auto_scroll {
                    self.scroll_to_bottom();
                }
            }
            AppEvent::Error(msg) => {
                self.output_lines.push(format!("  ✗ error: {msg}"));
                self.state = AgentState::Idle;
                if self.auto_scroll {
                    self.scroll_to_bottom();
                }
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    fn handle_slash_command(&mut self, cmd: &str) {
        let parts: Vec<&str> = cmd.splitn(2, ' ').collect();
        match parts[0] {
            "/help" => {
                self.output_lines.push(String::new());
                self.output_lines.push(
                    "Commands: /help /quit /status /cost /plan /compact /undo /save /load /image /memory /thinking /fork /branches /switch /diff /log /commit /stash /clear /export /model /init"
                        .into(),
                );
            }
            "/quit" => self.should_quit = true,
            "/status" => {
                self.output_lines.push(format!(
                    "Model: {} | Tokens: {}↓ {}↑ | Messages: {} | Plan mode: {}",
                    self.model,
                    self.total_input_tokens,
                    self.total_output_tokens,
                    self.output_lines.len(),
                    self.plan_mode
                ));
            }
            "/plan" => {
                self.plan_mode = !self.plan_mode;
                self.output_lines.push(format!(
                    "Plan mode: {}",
                    if self.plan_mode {
                        "ON (LLM will plan, not execute)"
                    } else {
                        "OFF"
                    }
                ));
            }
            "/compact" => {
                self.output_lines.push("Compaction requested.".into());
                self.pending_command = Some(PendingCommand::Compact);
            }
            "/undo" => {
                self.pending_command = Some(PendingCommand::Undo);
            }
            "/cost" => {
                if parts.get(1) == Some(&"--total") {
                    self.pending_command = Some(PendingCommand::CostTotal);
                } else {
                    self.output_lines.push(format!(
                        "Session cost: ${:.4} ({} input + {} output tokens)",
                        self.session_cost, self.total_input_tokens, self.total_output_tokens
                    ));
                }
            }
            "/save" => {
                let name = parts.get(1).unwrap_or(&"default");
                self.output_lines
                    .push(format!("Session save requested: {name}"));
                self.pending_command = Some(PendingCommand::Save(name.to_string()));
            }
            "/load" => {
                let name = parts.get(1).unwrap_or(&"default");
                self.output_lines
                    .push(format!("Session load requested: {name}"));
                self.pending_command = Some(PendingCommand::Load(name.to_string()));
            }
            "/image" => {
                if let Some(path) = parts.get(1) {
                    self.output_lines.push(format!("  🖼 image: {path}"));
                    self.pending_command = Some(PendingCommand::ImageAttach(path.to_string()));
                } else {
                    self.output_lines
                        .push("Usage: /image <path> [prompt]".into());
                }
            }
            "/memory" => {
                self.pending_command = Some(PendingCommand::Memory(
                    parts.get(1).unwrap_or(&"list").to_string(),
                ));
            }
            "/thinking" => {
                self.pending_command = Some(PendingCommand::ThinkingToggle);
            }
            "/fork" => {
                self.pending_command = Some(PendingCommand::Branch("fork".into()));
            }
            "/branches" => {
                self.pending_command = Some(PendingCommand::Branch("list".into()));
            }
            "/switch" => {
                if let Some(name) = parts.get(1) {
                    self.pending_command = Some(PendingCommand::Branch(format!("switch {name}")));
                } else {
                    self.output_lines
                        .push("Usage: /switch <branch-name>".into());
                }
            }
            "/branch" => {
                if let Some(args) = parts.get(1) {
                    self.pending_command = Some(PendingCommand::Branch(args.to_string()));
                } else {
                    self.output_lines
                        .push("Usage: /branch delete <name>".into());
                }
            }
            "/diff" => self.pending_command = Some(PendingCommand::Git("diff".into())),
            "/log" => self.pending_command = Some(PendingCommand::Git("log".into())),
            "/commit" => self.pending_command = Some(PendingCommand::Git("commit".into())),
            "/stash" => {
                let sub = if parts.get(1) == Some(&"pop") {
                    "stash_pop"
                } else {
                    "stash"
                };
                self.pending_command = Some(PendingCommand::Git(sub.into()));
            }
            "/clear" => {
                self.output_lines.clear();
                self.output_lines
                    .push("Output cleared. Session history preserved.".into());
                self.scroll_offset = 0;
            }
            "/export" => self.pending_command = Some(PendingCommand::Export),
            "/model" => {
                if let Some(name) = parts.get(1) {
                    self.pending_command = Some(PendingCommand::ModelSwitch(name.to_string()));
                } else {
                    self.output_lines.push(format!(
                        "Current model: {}. Usage: /model <name>",
                        self.model
                    ));
                }
            }
            "/init" => self.pending_command = Some(PendingCommand::Init),
            "/summary" => self.pending_command = Some(PendingCommand::Summary),
            "/search" => {
                if let Some(q) = parts.get(1) {
                    self.pending_command = Some(PendingCommand::Search(q.to_string()));
                } else {
                    self.output_lines.push("Usage: /search <keyword>".into());
                }
            }
            "/dry-run" => {
                self.dry_run = !self.dry_run;
                self.output_lines.push(format!(
                    "Dry-run mode: {}",
                    if self.dry_run {
                        "ON (tools shown but not executed)"
                    } else {
                        "OFF"
                    }
                ));
            }
            "/doctor" => self.pending_command = Some(PendingCommand::Doctor),
            "/review" => self.pending_command = Some(PendingCommand::Review),
            "/retry" => {
                if let Some(ref input) = self.last_user_input.clone() {
                    self.output_lines.push(format!("⟳ Retrying: {input}"));
                    self.pending_command = Some(PendingCommand::Retry);
                } else {
                    self.output_lines.push("Nothing to retry.".into());
                }
            }
            "/pin" => {
                let idx = self.output_lines.len().saturating_sub(1);
                self.pinned_messages.push(idx);
                self.output_lines
                    .push(format!("📌 Pinned message at line {idx}"));
            }
            "/theme" => {
                self.theme = if self.theme == "dark" {
                    "light".into()
                } else {
                    "dark".into()
                };
                self.output_lines.push(format!("Theme: {}", self.theme));
            }
            "/copy" => {
                let last_response: String = self
                    .output_lines
                    .iter()
                    .rev()
                    .take_while(|l| !l.starts_with('›'))
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("\n");
                self.pending_command = Some(PendingCommand::CopyToClipboard(last_response));
                self.output_lines.push("📋 Copied to clipboard.".into());
            }
            "/version" => {
                self.output_lines.push(format!(
                    "magic-code v{} ({} {})",
                    env!("CARGO_PKG_VERSION"),
                    std::env::consts::OS,
                    std::env::consts::ARCH,
                ));
            }
            "/history" => {
                self.output_lines.push("Input history:".into());
                for (i, entry) in self.history.entries().iter().rev().take(20).enumerate() {
                    self.output_lines.push(format!("  {}: {entry}", i + 1));
                }
            }
            "/tokens" => self.pending_command = Some(PendingCommand::Tokens),
            "/context" => self.pending_command = Some(PendingCommand::Context),
            "/alias" => {
                if let (Some(name), Some(expansion)) = (parts.get(1), parts.get(2)) {
                    self.aliases
                        .insert(format!("/{name}"), expansion.to_string());
                    self.output_lines
                        .push(format!("Alias: /{name} → {expansion}"));
                } else if self.aliases.is_empty() {
                    self.output_lines
                        .push("No aliases. Usage: /alias <name> <command>".into());
                } else {
                    for (k, v) in &self.aliases {
                        self.output_lines.push(format!("  {k} → {v}"));
                    }
                }
            }
            "/run" => {
                if parts.len() > 1 {
                    let full = parts[1..].join(" ");
                    self.output_lines.push(format!("$ {full}"));
                    match std::process::Command::new("sh")
                        .arg("-c")
                        .arg(&full)
                        .output()
                    {
                        Ok(o) => {
                            let out = String::from_utf8_lossy(&o.stdout);
                            let err = String::from_utf8_lossy(&o.stderr);
                            for line in out.lines() {
                                self.output_lines.push(format!("  {line}"));
                            }
                            if !err.is_empty() {
                                self.output_lines.push(format!("  STDERR: {}", err.trim()));
                            }
                            self.last_tool_output = Some(out.to_string());
                        }
                        Err(e) => self.output_lines.push(format!("  ✗ {e}")),
                    }
                } else {
                    self.output_lines.push("Usage: /run <command>".into());
                }
            }
            "/grep" => {
                if let Some(pattern) = parts.get(1) {
                    let args = if let Some(path) = parts.get(2) {
                        vec!["grep", "-rn", "--color=never", pattern, path]
                    } else {
                        vec!["grep", "-rn", "--color=never", pattern, "."]
                    };
                    match std::process::Command::new(args[0])
                        .args(&args[1..])
                        .output()
                    {
                        Ok(o) => {
                            let out = String::from_utf8_lossy(&o.stdout);
                            let lines: Vec<&str> = out.lines().take(30).collect();
                            if lines.is_empty() {
                                self.output_lines.push("No matches.".into());
                            } else {
                                for line in &lines {
                                    self.output_lines.push(format!("  {line}"));
                                }
                                let total = out.lines().count();
                                if total > 30 {
                                    self.output_lines
                                        .push(format!("  ... and {} more", total - 30));
                                }
                            }
                        }
                        Err(e) => self.output_lines.push(format!("  ✗ {e}")),
                    }
                } else {
                    self.output_lines
                        .push("Usage: /grep <pattern> [path]".into());
                }
            }
            "/time" => {
                let elapsed = self.session_start.elapsed();
                let mins = elapsed.as_secs() / 60;
                let secs = elapsed.as_secs() % 60;
                self.output_lines
                    .push(format!("Session time: {mins}m {secs}s"));
            }
            "/whoami" => {
                self.output_lines.push(format!(
                    "Model: {} | Plan: {} | Dry-run: {} | Theme: {}",
                    self.model,
                    if self.plan_mode { "ON" } else { "OFF" },
                    if self.dry_run { "ON" } else { "OFF" },
                    self.theme,
                ));
            }
            "/tip" => {
                self.output_lines.push(format!("💡 {}", random_tip()));
            }
            "/last" => {
                if let Some(ref out) = self.last_tool_output {
                    for line in out.lines().take(50) {
                        self.output_lines.push(format!("  {line}"));
                    }
                } else {
                    self.output_lines.push("No tool output yet.".into());
                }
            }
            "/files" => {
                let path = parts.get(1).unwrap_or(&".");
                match std::process::Command::new("ls").args(["-la", path]).output() {
                    Ok(o) => {
                        for line in String::from_utf8_lossy(&o.stdout).lines() {
                            self.output_lines.push(format!("  {line}"));
                        }
                    }
                    Err(e) => self.output_lines.push(format!("  ✗ {e}")),
                }
            }
            "/cat" => {
                if let Some(path) = parts.get(1) {
                    match std::fs::read_to_string(path) {
                        Ok(content) => {
                            for line in content.lines().take(100) {
                                self.output_lines.push(format!("  {line}"));
                            }
                            let total = content.lines().count();
                            if total > 100 {
                                self.output_lines
                                    .push(format!("  ... ({} more lines)", total - 100));
                            }
                        }
                        Err(e) => self.output_lines.push(format!("  ✗ {e}")),
                    }
                } else {
                    self.output_lines.push("Usage: /cat <file>".into());
                }
            }
            "/models" => {
                self.output_lines.push("Known models (use /model <name> to switch):".into());
                for m in &[
                    "claude-sonnet-4-20250514", "claude-haiku", "gpt-4o", "gpt-4o-mini",
                    "gemini-2.5-flash", "gemini-2.5-pro", "llama3", "mistral",
                ] {
                    self.output_lines.push(format!("  {m}"));
                }
            }
            "/open" => {
                if let Some(path) = parts.get(1) {
                    let editor =
                        std::env::var("EDITOR").unwrap_or_else(|_| "vi".into());
                    self.output_lines
                        .push(format!("Opening {path} in {editor}..."));
                    let _ = std::process::Command::new(&editor).arg(path).status();
                } else {
                    self.output_lines.push("Usage: /open <file>".into());
                }
            }
            "/wc" => {
                match std::process::Command::new("sh")
                    .arg("-c")
                    .arg("find . -name '*.rs' -o -name '*.py' -o -name '*.ts' -o -name '*.go' | head -500 | xargs wc -l 2>/dev/null | tail -1")
                    .output()
                {
                    Ok(o) => {
                        let out = String::from_utf8_lossy(&o.stdout);
                        self.output_lines
                            .push(format!("  {}", out.trim()));
                    }
                    Err(e) => self.output_lines.push(format!("  ✗ {e}")),
                }
            }
            "/tree" => {
                let depth = parts.get(1).and_then(|d| d.parse::<u8>().ok()).unwrap_or(2);
                match std::process::Command::new("sh")
                    .arg("-c")
                    .arg(format!("find . -maxdepth {depth} -not -path '*/target/*' -not -path '*/.git/*' -not -path '*/node_modules/*' | sort | head -80"))
                    .output()
                {
                    Ok(o) => {
                        for line in String::from_utf8_lossy(&o.stdout).lines() {
                            self.output_lines.push(format!("  {line}"));
                        }
                    }
                    Err(e) => self.output_lines.push(format!("  ✗ {e}")),
                }
            }
            "/head" => {
                if let Some(path) = parts.get(1) {
                    let n = parts.get(2).and_then(|s| s.parse::<usize>().ok()).unwrap_or(20);
                    match std::fs::read_to_string(path) {
                        Ok(c) => { for line in c.lines().take(n) { self.output_lines.push(format!("  {line}")); } }
                        Err(e) => self.output_lines.push(format!("  ✗ {e}")),
                    }
                } else { self.output_lines.push("Usage: /head <file> [lines]".into()); }
            }
            "/tail" => {
                if let Some(path) = parts.get(1) {
                    let n = parts.get(2).and_then(|s| s.parse::<usize>().ok()).unwrap_or(20);
                    match std::fs::read_to_string(path) {
                        Ok(c) => {
                            let lines: Vec<&str> = c.lines().collect();
                            let start = lines.len().saturating_sub(n);
                            for line in &lines[start..] { self.output_lines.push(format!("  {line}")); }
                        }
                        Err(e) => self.output_lines.push(format!("  ✗ {e}")),
                    }
                } else { self.output_lines.push("Usage: /tail <file> [lines]".into()); }
            }
            "/pwd" => {
                self.output_lines.push(format!("  {}", std::env::current_dir().unwrap_or_default().display()));
            }
            "/env" => {
                for var in &["ANTHROPIC_API_KEY", "OPENAI_API_KEY", "GEMINI_API_KEY", "EDITOR", "SHELL", "HOME"] {
                    let val = std::env::var(var).unwrap_or_else(|_| "(not set)".into());
                    let masked = if var.contains("KEY") && val.len() > 8 {
                        format!("{}...{}", &val[..4], &val[val.len()-4..])
                    } else { val };
                    self.output_lines.push(format!("  {var}={masked}"));
                }
            }
            "/size" => {
                if let Some(path) = parts.get(1) {
                    match std::fs::metadata(path) {
                        Ok(m) => {
                            let kb = m.len() / 1024;
                            self.output_lines.push(format!("  {path}: {} bytes ({kb} KB)", m.len()));
                        }
                        Err(e) => self.output_lines.push(format!("  ✗ {e}")),
                    }
                } else { self.output_lines.push("Usage: /size <file>".into()); }
            }
            "/todo" => {
                // Find all TODO/FIXME in codebase — genuinely useful during development
                match std::process::Command::new("sh")
                    .arg("-c")
                    .arg("grep -rn --color=never 'TODO\\|FIXME\\|HACK\\|XXX' . --include='*.rs' --include='*.py' --include='*.ts' --include='*.js' --include='*.go' 2>/dev/null | grep -v target/ | head -30")
                    .output()
                {
                    Ok(o) => {
                        let out = String::from_utf8_lossy(&o.stdout);
                        if out.is_empty() { self.output_lines.push("No TODOs found ✓".into()); }
                        else { for line in out.lines() { self.output_lines.push(format!("  {line}")); } }
                    }
                    Err(e) => self.output_lines.push(format!("  ✗ {e}")),
                }
            }
            "/recent" => {
                // Files modified in last hour — see what the agent changed
                match std::process::Command::new("sh")
                    .arg("-c")
                    .arg("find . -name '*.rs' -o -name '*.py' -o -name '*.ts' -o -name '*.js' -o -name '*.go' -o -name '*.toml' -o -name '*.md' | xargs ls -lt 2>/dev/null | head -15")
                    .output()
                {
                    Ok(o) => {
                        let out = String::from_utf8_lossy(&o.stdout);
                        if out.is_empty() { self.output_lines.push("No recent changes.".into()); }
                        else {
                            self.output_lines.push("Recently modified:".into());
                            for line in out.lines() { self.output_lines.push(format!("  {line}")); }
                        }
                    }
                    Err(e) => self.output_lines.push(format!("  ✗ {e}")),
                }
            }
            "/ship" => {
                // git add all + commit with LLM message — the most common workflow
                self.output_lines.push("Staging all changes...".into());
                let _ = std::process::Command::new("git").args(["add", "-A"]).output();
                self.pending_command = Some(PendingCommand::Git("commit".into()));
            }
            "/test" => {
                // Auto-detect test runner and run
                let cmd = if std::path::Path::new("Cargo.toml").exists() || std::path::Path::new("mc/Cargo.toml").exists() {
                    "cargo test --workspace 2>&1 | tail -20"
                } else if std::path::Path::new("package.json").exists() {
                    "npm test 2>&1 | tail -20"
                } else if std::path::Path::new("pytest.ini").exists() || std::path::Path::new("setup.py").exists() {
                    "python -m pytest 2>&1 | tail -20"
                } else if std::path::Path::new("go.mod").exists() {
                    "go test ./... 2>&1 | tail -20"
                } else if std::path::Path::new("Makefile").exists() {
                    "make test 2>&1 | tail -20"
                } else {
                    self.output_lines.push("No test runner detected.".into());
                    return;
                };
                self.output_lines.push(format!("Running: {}", cmd.split(" 2>&1").next().unwrap_or(cmd)));
                match std::process::Command::new("sh").arg("-c").arg(cmd).output() {
                    Ok(o) => {
                        for line in String::from_utf8_lossy(&o.stdout).lines() {
                            self.output_lines.push(format!("  {line}"));
                        }
                        if o.status.success() { self.output_lines.push("  ✓ Tests passed".into()); }
                        else { self.output_lines.push("  ✗ Tests failed".into()); }
                    }
                    Err(e) => self.output_lines.push(format!("  ✗ {e}")),
                }
            }
            "/permissions" => {
                // Claude Code style: show and toggle permission mode
                if let Some(mode) = parts.get(1) {
                    self.output_lines.push(format!("Permission mode → {mode} (restart to apply)"));
                } else {
                    self.output_lines.push(format!(
                        "Permission mode: {} | Dry-run: {}",
                        if self.dry_run { "dry-run" } else { "active" },
                        if self.dry_run { "ON" } else { "OFF" },
                    ));
                    self.output_lines.push("  Modes: read-only, workspace-write, full-access".into());
                    self.output_lines.push("  Toggle dry-run: /dry-run".into());
                }
            }
            "/config" => {
                // Show current runtime config — Codex/Claude Code have this
                self.output_lines.push("Current config:".into());
                self.output_lines.push(format!("  model: {}", self.model));
                self.output_lines.push(format!("  plan_mode: {}", self.plan_mode));
                self.output_lines.push(format!("  dry_run: {}", self.dry_run));
                self.output_lines.push(format!("  theme: {}", self.theme));
                self.output_lines.push(format!("  session_time: {}s", self.session_start.elapsed().as_secs()));
                self.output_lines.push(format!("  tokens: {}↓ {}↑", self.total_input_tokens, self.total_output_tokens));
                self.output_lines.push(format!("  cost: ${:.4}", self.session_cost));
                self.output_lines.push(format!("  context: {}%", self.context_usage_pct));
                self.output_lines.push("  Edit: .magic-code/config.toml".into());
            }
            "/add" => {
                // Add file/dir content to next prompt — Claude Code's /add-dir
                if let Some(path) = parts.get(1) {
                    let p = std::path::Path::new(path);
                    if p.is_file() {
                        match std::fs::read_to_string(p) {
                            Ok(content) => {
                                let lines = content.lines().count();
                                let current = self.input.as_str().to_string();
                                let prefix = if current.is_empty() { String::new() } else { format!("{current}\n\n") };
                                self.input.set(&format!("{prefix}[{path} ({lines} lines)]:\n```\n{content}\n```"));
                                self.output_lines.push(format!("📎 Added {path} ({lines} lines) to input"));
                            }
                            Err(e) => self.output_lines.push(format!("  ✗ {e}")),
                        }
                    } else if p.is_dir() {
                        let mut files = Vec::new();
                        if let Ok(entries) = std::fs::read_dir(p) {
                            for e in entries.flatten().take(20) {
                                if e.path().is_file() {
                                    files.push(e.path().display().to_string());
                                }
                            }
                        }
                        self.output_lines.push(format!("📁 Dir {path}: {} files", files.len()));
                        for f in &files { self.output_lines.push(format!("  {f}")); }
                    } else {
                        self.output_lines.push(format!("  ✗ not found: {path}"));
                    }
                } else { self.output_lines.push("Usage: /add <file|dir>".into()); }
            }
            "/sessions" => {
                // List/delete saved sessions — essential session management
                if parts.get(1) == Some(&"delete") {
                    if let Some(name) = parts.get(2) {
                        self.pending_command = Some(PendingCommand::Search(format!("__delete__{name}")));
                        self.output_lines.push(format!("Deleting session: {name}"));
                    } else { self.output_lines.push("Usage: /sessions delete <name>".into()); }
                } else {
                    self.pending_command = Some(PendingCommand::Search("__list__".into()));
                }
            }
            "/spec" => {
                // Kiro-style: generate spec before coding
                let task = if parts.len() > 1 { parts[1..].join(" ") } else { String::new() };
                let spec_prompt = if task.is_empty() {
                    "Before writing any code, create a brief technical specification:\n1. Requirements (what needs to be done)\n2. Approach (how to implement)\n3. Files to modify\n4. Edge cases\n5. Testing strategy\n\nThen ask me to confirm before proceeding.".to_string()
                } else {
                    format!("Before implementing: {task}\n\nCreate a brief technical specification:\n1. Requirements\n2. Approach\n3. Files to modify\n4. Edge cases\n5. Testing strategy\n\nThen ask me to confirm before proceeding.")
                };
                self.input.set(&spec_prompt);
                self.output_lines.push("📋 Spec mode: AI will plan before coding".into());
            }
            "/template" => {
                if let Some(name) = parts.get(1) {
                    let prompt = match *name {
                        "review" => "Review the recent code changes. Check for bugs, security issues, performance problems, and style. Be specific about line numbers.",
                        "refactor" => "Refactor the code I'm about to show you. Improve readability, reduce duplication, and follow best practices. Show the changes as diffs.",
                        "test" => "Write comprehensive tests for the code I'm about to show you. Cover edge cases, error paths, and happy paths.",
                        "explain" => "Explain this code in detail. What does it do, how does it work, and what are the key design decisions?",
                        "document" => "Add documentation to this code. Include doc comments, inline comments for complex logic, and a module-level overview.",
                        "optimize" => "Analyze this code for performance. Identify bottlenecks and suggest optimizations with benchmarks.",
                        "security" => "Audit this code for security vulnerabilities. Check for injection, auth issues, data leaks, and unsafe patterns.",
                        _ => {
                            self.output_lines.push(format!("Unknown template: {name}. Available: review, refactor, test, explain, document, optimize, security"));
                            return;
                        }
                    };
                    self.output_lines.push(format!("📋 Template: {name}"));
                    self.output_lines.push(format!("  {prompt}"));
                    self.input.set(prompt);
                } else {
                    self.output_lines.push(
                        "Templates: review, refactor, test, explain, document, optimize, security"
                            .into(),
                    );
                    self.output_lines.push("Usage: /template <name>".into());
                }
            }
            "/vim" => {
                self.vim_mode = if self.vim_mode.is_some() {
                    self.output_lines.push("Vim mode: OFF".into());
                    None
                } else {
                    self.output_lines.push("Vim mode: ON (Esc=Normal, i=Insert)".into());
                    Some(crate::input::VimMode::Insert)
                };
            }
            "/rewind" => {
                let n = parts.get(1).and_then(|s| s.parse::<usize>().ok()).unwrap_or(1);
                self.pending_command = Some(PendingCommand::Rewind(n));
            }
            "/debug" => self.pending_command = Some(PendingCommand::Debug),
            "/btw" => {
                if parts.len() > 1 {
                    let q = parts[1..].join(" ");
                    self.pending_command = Some(PendingCommand::Btw(q));
                } else {
                    self.output_lines.push("Usage: /btw <question>".into());
                }
            }
            "/loop" => {
                if parts.get(1) == Some(&"stop") {
                    self.pending_command = Some(PendingCommand::LoopStop);
                } else if let (Some(interval), Some(_)) = (parts.get(1), parts.get(2)) {
                    let secs = parse_interval(interval);
                    if secs > 0 {
                        let prompt = parts[2..].join(" ");
                        self.pending_command = Some(PendingCommand::Loop {
                            interval_secs: secs,
                            prompt,
                        });
                        self.output_lines.push(format!("🔄 Loop: every {secs}s"));
                    } else {
                        self.output_lines.push("Invalid interval. Use: /loop 5m <prompt>".into());
                    }
                } else {
                    self.output_lines.push("Usage: /loop <interval> <prompt> | /loop stop".into());
                }
            }
            _ => {
                // Check custom markdown commands
                let cmd_name = parts[0].trim_start_matches('/');
                if let Some((_, content)) = self.custom_commands.iter().find(|(n, _)| n == cmd_name)
                {
                    let args = if parts.len() > 1 {
                        parts[1..].join(" ")
                    } else {
                        String::new()
                    };
                    let prompt = content.replace("$ARGUMENTS", &args);
                    self.input.set(&prompt);
                    self.output_lines
                        .push(format!("📋 Command: {cmd_name}"));
                } else {
                    self.output_lines.push(format!("Unknown command: {cmd}"));
                }
            }
        }
    }

    pub fn submit_input(&mut self) -> Option<AppEvent> {
        if self.state != AgentState::Idle || self.input.is_empty() {
            return None;
        }
        let text = self.input.take();
        let trimmed = text.trim().to_string();
        self.history.reset_cursor();
        if trimmed.starts_with('/') {
            Some(AppEvent::SlashCommand(trimmed))
        } else {
            self.last_user_input = Some(trimmed.clone());
            Some(AppEvent::UserSubmit(trimmed))
        }
    }

    const SLASH_COMMANDS: &[&str] = &[
        "/help",
        "/quit",
        "/status",
        "/cost",
        "/plan",
        "/compact",
        "/undo",
        "/save",
        "/load",
        "/image",
        "/memory",
        "/thinking",
        "/fork",
        "/branches",
        "/switch",
        "/diff",
        "/log",
        "/commit",
        "/stash",
        "/clear",
        "/export",
        "/model",
        "/init",
        "/summary",
        "/search",
        "/doctor",
        "/template",
        "/review",
        "/retry",
        "/pin",
        "/theme",
        "/copy",
        "/version",
        "/history",
        "/tokens",
        "/context",
        "/alias",
        "/run",
        "/grep",
        "/time",
        "/whoami",
        "/tip",
        "/last",
        "/files",
        "/cat",
        "/models",
        "/open",
        "/wc",
        "/tree",
        "/head",
        "/tail",
        "/pwd",
        "/env",
        "/size",
        "/todo",
        "/recent",
        "/ship",
        "/test",
        "/permissions",
        "/config",
        "/add",
        "/sessions",
        "/spec",
        "/vim",
        "/rewind",
        "/debug",
        "/btw",
        "/loop",
    ];

    /// Tab-complete slash commands. Returns true if completion was applied.
    pub fn tab_complete(&mut self) -> bool {
        let text = self.input.as_str();
        if !text.starts_with('/') || text.contains(' ') {
            return false;
        }
        let matches: Vec<&&str> = Self::SLASH_COMMANDS
            .iter()
            .filter(|cmd| cmd.starts_with(text) && **cmd != text)
            .collect();
        match matches.len() {
            1 => {
                self.input.set(matches[0]);
                true
            }
            2.. => {
                self.output_lines
                    .push(matches.iter().map(|c| **c).collect::<Vec<_>>().join("  "));
                true
            }
            _ => false,
        }
    }

    /// Scroll up by `n` lines.
    pub fn scroll_up(&mut self, n: u16) {
        self.auto_scroll = false;
        self.scroll_offset = self.scroll_offset.saturating_sub(n);
    }

    /// Scroll down by `n` lines.
    pub fn scroll_down(&mut self, n: u16) {
        let max = self.max_scroll();
        self.scroll_offset = (self.scroll_offset + n).min(max);
        if self.scroll_offset >= max {
            self.auto_scroll = true;
        }
    }

    /// Jump to top.
    pub fn scroll_home(&mut self) {
        self.auto_scroll = false;
        self.scroll_offset = 0;
    }

    /// Jump to bottom.
    pub fn scroll_end(&mut self) {
        self.auto_scroll = true;
        self.scroll_to_bottom();
    }

    /// Navigate input history up.
    pub fn history_up(&mut self) {
        if let Some(entry) = self.history.up() {
            self.input.set(entry);
        }
    }

    /// Navigate input history down.
    pub fn history_down(&mut self) {
        if let Some(entry) = self.history.down() {
            self.input.set(entry);
        } else {
            self.input.clear();
        }
    }

    fn scroll_to_bottom(&mut self) {
        self.scroll_offset = self.max_scroll();
    }

    fn max_scroll(&self) -> u16 {
        let total = self.output_lines.len() as u16;
        // viewport_height - 2 for borders
        let visible = self.viewport_height.saturating_sub(2);
        total.saturating_sub(visible)
    }
}

fn parse_interval(s: &str) -> u64 {
    if let Some(m) = s.strip_suffix('m') {
        m.parse::<u64>().unwrap_or(0) * 60
    } else if let Some(h) = s.strip_suffix('h') {
        h.parse::<u64>().unwrap_or(0) * 3600
    } else if let Some(sec) = s.strip_suffix('s') {
        sec.parse::<u64>().unwrap_or(0)
    } else {
        s.parse::<u64>().unwrap_or(0)
    }
}

fn load_custom_commands() -> Vec<(String, String)> {
    let dir = std::path::Path::new(".magic-code/commands");
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "md"))
        .filter_map(|e| {
            let name = e.path().file_stem()?.to_string_lossy().to_string();
            let content = std::fs::read_to_string(e.path()).ok()?;
            Some((name, content))
        })
        .collect()
}

fn random_tip() -> &'static str {
    const TIPS: &[&str] = &[
        "Use @filename to include file content in your prompt",
        "Press Tab to auto-complete slash commands",
        "/template review — get a code review from the AI",
        "/model <name> — switch models mid-session",
        "/diff — see what files changed",
        "/commit — auto-generate commit messages with AI",
        "/undo — revert the last turn's file changes",
        "/run <cmd> — quick shell command without AI",
        "/grep <pattern> — quick search without AI",
        "/cost --total — see all-time spending",
        "Ctrl+C cancels the current turn, not the app",
        "Mouse scroll works in the output area",
        "/doctor — check your setup",
        "/export — save conversation as markdown",
        "/dry-run — preview tool calls without executing",
    ];
    TIPS[std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as usize
        % TIPS.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slash_commands() {
        let mut app = App::new("test".into());
        app.handle_event(AppEvent::SlashCommand("/help".into()));
        assert!(app.output_lines.iter().any(|l| l.contains("/help")));
    }

    #[test]
    fn quit_command() {
        let mut app = App::new("test".into());
        app.handle_event(AppEvent::SlashCommand("/quit".into()));
        assert!(app.should_quit);
    }

    #[test]
    fn stream_appends() {
        let mut app = App::new("test".into());
        app.handle_event(AppEvent::StreamDelta("hello ".into()));
        app.handle_event(AppEvent::StreamDelta("world".into()));
        assert!(app.output_lines.last().unwrap().contains("hello world"));
    }

    #[test]
    fn submit_returns_event() {
        let mut app = App::new("test".into());
        app.input.insert('h');
        app.input.insert('i');
        let event = app.submit_input();
        assert!(matches!(event, Some(AppEvent::UserSubmit(_))));
    }

    #[test]
    fn scroll_up_down() {
        let mut app = App::new("test".into());
        app.viewport_height = 10;
        for i in 0..30 {
            app.output_lines.push(format!("line {i}"));
        }
        app.scroll_to_bottom();
        let max = app.scroll_offset;
        assert!(max > 0);

        app.scroll_up(5);
        assert!(!app.auto_scroll);
        assert_eq!(app.scroll_offset, max - 5);

        app.scroll_end();
        assert!(app.auto_scroll);
        assert_eq!(app.scroll_offset, max);
    }

    #[test]
    fn multiline_delta() {
        let mut app = App::new("test".into());
        let initial = app.output_lines.len();
        app.handle_event(AppEvent::StreamDelta("line1\nline2\nline3".into()));
        assert_eq!(app.output_lines.len(), initial + 2); // 2 new lines added
        assert!(app.output_lines.last().unwrap().contains("line3"));
    }
}
