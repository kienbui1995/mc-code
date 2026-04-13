use crate::history::InputHistory;
use crate::input::InputBuffer;

/// Appevent.
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
/// Uimessage.
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
    /// Streaming tool input preview for write tools (partial JSON as it arrives).
    ToolInputDelta {
        name: String,
        partial: String,
    },
}

/// Commands that need processing by main.rs (require runtime/provider access).
/// Effort level controlling thinking budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Effortlevel.
pub enum EffortLevel {
    /// ○ Low: minimal thinking, fast and cheap (~4K tokens).
    Low,
    /// ◐ Medium: balanced (default, ~10K tokens).
    Medium,
    /// ● High: deep reasoning for complex tasks (~32K tokens).
    High,
}

impl EffortLevel {
    #[must_use]
    /// Symbol.
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Low => "○",
            Self::Medium => "◐",
            Self::High => "●",
        }
    }

    #[must_use]
    /// Thinking budget.
    pub fn thinking_budget(self) -> Option<u32> {
        match self {
            Self::Low => None, // no extended thinking
            Self::Medium => Some(10_000),
            Self::High => Some(32_000),
        }
    }

    /// Cycle to next effort level.
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Low => Self::Medium,
            Self::Medium => Self::High,
            Self::High => Self::Low,
        }
    }
}

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
    Export(String),
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
    Loop {
        interval_secs: u64,
        prompt: String,
    },
    LoopStop,
    AcceptEdit {
        path: String,
        diff: String,
    },
    /// Run a shell command asynchronously (main.rs handles via tokio).
    RunShell(String),
    /// Plugin management: install, list, remove, update.
    Plugin(String),
    /// Toggle `review_writes` mode.
    ReviewToggle,
    /// Toggle auto-test mode.
    AutoTestToggle,
    /// Toggle auto-commit mode.
    AutoCommitToggle,
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
/// App.
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
    /// Per-turn cost breakdown: (`turn_number`, `input_tokens`, `output_tokens`, cost, model).
    pub turn_costs: Vec<(u32, u32, u32, f64, String)>,
    /// Per-tool call count for cost analysis.
    pub tool_call_counts: std::collections::HashMap<String, u32>,
    pub context_usage_pct: u8,
    pub should_quit: bool,
    /// Double-tap quit confirmation.
    pub pending_quit: bool,
    pub plan_mode: bool,
    pub dry_run: bool,
    pub review_writes: bool,
    /// Command queue — consumed by main.rs each frame.
    pub pending_command: Option<PendingCommand>,
    /// Whether user has manually scrolled up (disables auto-scroll).
    auto_scroll: bool,
    /// Spinner frame counter for animation.
    pub spinner_tick: u8,
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
    /// Effort level: low (○), medium (◐), high (●). Controls thinking budget.
    pub effort: EffortLevel,
}

impl App {
    #[must_use]
    /// New.
    pub fn new(model: String) -> Self {
        let history_path = std::env::var_os("HOME")
            .map(|h| std::path::PathBuf::from(h).join(".local/share/magic-code/history"));
        let history = history_path.map_or_else(|| InputHistory::new(1000), InputHistory::load_from);

        Self {
            input: InputBuffer::default(),
            history,
            output_lines: vec![
                "Welcome to magic-code. Type /help for commands.".into(),
                crate::commands::random_tip().into(),
            ],
            scroll_offset: 0,
            model,
            total_input_tokens: 0,
            total_output_tokens: 0,
            session_cost: 0.0,
            turn_costs: Vec::new(),
            tool_call_counts: std::collections::HashMap::new(),
            context_usage_pct: 0,
            should_quit: false,
            pending_quit: false,
            plan_mode: false,
            dry_run: false,
            review_writes: false,
            pending_command: None,
            auto_scroll: true,
            spinner_tick: 0,
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
            effort: EffortLevel::Medium,
            state: AgentState::Idle,
        }
    }

    /// Push a line to output.
    pub fn push(&mut self, line: &str) {
        self.output_lines.push(line.to_string());
    }

    /// Handle event.
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
                self.cap_output_lines();
                if self.auto_scroll {
                    self.scroll_to_bottom();
                }
                // Bell notification — alerts user in other tabs/windows
                eprint!("\x07");
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
        crate::commands::handle(self, cmd);
    }

    /// Submit input.
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
            // Auto-detect effort keywords (like Claude Code's "think hard")
            let lower = trimmed.to_lowercase();
            if lower.contains("ultrathink")
                || lower.contains("think harder")
                || lower.contains("think hard")
                || lower.contains("think deeply")
            {
                self.effort = EffortLevel::High;
            } else if lower.starts_with("think") {
                self.effort = EffortLevel::Medium;
            }
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
        "/update",
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
        "/security-review",
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
        "/providers",
        "/config",
        "/add",
        "/sessions",
        "/spec",
        "/vim",
        "/effort",
        "/rewind",
        "/debug",
        "/btw",
        "/loop",
        "/cron",
        "/connect",
        "/tasks",
        "/resume",
        "/agents",
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

    /// Advance spinner and return current frame character.
    pub fn spinner_char(&mut self) -> char {
        const FRAMES: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
        self.spinner_tick = self.spinner_tick.wrapping_add(1);
        FRAMES[self.spinner_tick as usize % FRAMES.len()]
    }

    /// Cap `output_lines` to prevent unbounded memory growth.
    fn cap_output_lines(&mut self) {
        const MAX_OUTPUT_LINES: usize = 10_000;
        if self.output_lines.len() > MAX_OUTPUT_LINES {
            let drain = self.output_lines.len() - MAX_OUTPUT_LINES;
            self.output_lines.drain(..drain);
            self.output_lines
                .insert(0, "[...earlier output trimmed...]".into());
        }
    }

    fn max_scroll(&self) -> u16 {
        let total = self.output_lines.len() as u16;
        // viewport_height - 2 for borders
        let visible = self.viewport_height.saturating_sub(2);
        total.saturating_sub(visible)
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
