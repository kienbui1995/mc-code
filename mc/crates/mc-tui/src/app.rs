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
    Done,
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

#[allow(clippy::struct_excessive_bools)]
pub struct App {
    pub input: InputBuffer,
    pub history: InputHistory,
    pub output_lines: Vec<String>,
    pub scroll_offset: u16,
    pub waiting: bool,
    pub model: String,
    pub total_input_tokens: u32,
    pub total_output_tokens: u32,
    pub session_cost: f64,
    pub should_quit: bool,
    pub plan_mode: bool,
    pub compact_requested: bool,
    pub save_requested: Option<String>,
    pub load_requested: Option<String>,
    pub undo_requested: bool,
    /// Whether user has manually scrolled up (disables auto-scroll).
    auto_scroll: bool,
    /// Terminal height for scroll calculations.
    pub viewport_height: u16,
    /// Pending permission prompt (tool, input).
    pub permission_pending: Option<(String, String)>,
    /// User's response to permission prompt: Some(true) = allow, Some(false) = deny.
    pub permission_response: Option<bool>,
    /// Tools always allowed (user pressed 'A').
    pub always_allowed: std::collections::HashSet<String>,
    /// User requested `/cost --total`.
    pub cost_total_requested: bool,
    /// Pending image attachment path.
    pub image_pending: Option<String>,
    /// Pending memory command.
    pub memory_command: Option<String>,
    /// Toggle thinking display.
    pub thinking_toggle: bool,
    /// Pending branch command.
    pub branch_command: Option<String>,
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
            output_lines: vec!["Welcome to magic-code. Type /help for commands.".into()],
            scroll_offset: 0,
            waiting: false,
            model,
            total_input_tokens: 0,
            total_output_tokens: 0,
            session_cost: 0.0,
            should_quit: false,
            plan_mode: false,
            compact_requested: false,
            save_requested: None,
            load_requested: None,
            undo_requested: false,
            auto_scroll: true,
            viewport_height: 20,
            permission_pending: None,
            permission_response: None,
            always_allowed: std::collections::HashSet::new(),
            cost_total_requested: false,
            image_pending: None,
            memory_command: None,
            thinking_toggle: false,
            branch_command: None,
        }
    }

    pub fn handle_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::UserSubmit(text) => {
                self.history.push(&text);
                self.output_lines.push(format!("\n› {text}"));
                self.output_lines.push(String::new());
                self.waiting = true;
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
                self.waiting = false;
                self.output_lines.push(String::new());
                if self.auto_scroll {
                    self.scroll_to_bottom();
                }
            }
            AppEvent::ToolCall(name) => {
                self.output_lines.push(format!("  ⚙ tool: {name}"));
                if self.auto_scroll {
                    self.scroll_to_bottom();
                }
            }
            AppEvent::Error(msg) => {
                self.output_lines.push(format!("  ✗ error: {msg}"));
                self.waiting = false;
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
                    "Commands: /help /quit /status /cost /plan /compact /undo /save /load /image /memory /thinking /fork /branches /switch"
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
                self.compact_requested = true;
            }
            "/undo" => {
                self.undo_requested = true;
            }
            "/cost" => {
                if parts.get(1) == Some(&"--total") {
                    self.cost_total_requested = true;
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
                self.save_requested = Some(name.to_string());
            }
            "/load" => {
                let name = parts.get(1).unwrap_or(&"default");
                self.output_lines
                    .push(format!("Session load requested: {name}"));
                self.load_requested = Some(name.to_string());
            }
            "/image" => {
                if let Some(path) = parts.get(1) {
                    self.output_lines.push(format!("  🖼 image: {path}"));
                    self.image_pending = Some(path.to_string());
                } else {
                    self.output_lines
                        .push("Usage: /image <path> [prompt]".into());
                }
            }
            "/memory" => {
                self.memory_command = Some(parts.get(1).unwrap_or(&"list").to_string());
            }
            "/thinking" => {
                self.thinking_toggle = true;
            }
            "/fork" => {
                self.branch_command = Some("fork".into());
            }
            "/branches" => {
                self.branch_command = Some("list".into());
            }
            "/switch" => {
                if let Some(name) = parts.get(1) {
                    self.branch_command = Some(format!("switch {name}"));
                } else {
                    self.output_lines
                        .push("Usage: /switch <branch-name>".into());
                }
            }
            "/branch" => {
                if let Some(args) = parts.get(1) {
                    self.branch_command = Some(args.to_string());
                } else {
                    self.output_lines
                        .push("Usage: /branch delete <name>".into());
                }
            }
            _ => {
                self.output_lines.push(format!("Unknown command: {cmd}"));
            }
        }
    }

    pub fn submit_input(&mut self) -> Option<AppEvent> {
        if self.waiting || self.input.is_empty() {
            return None;
        }
        let text = self.input.take();
        let trimmed = text.trim().to_string();
        self.history.reset_cursor();
        if trimmed.starts_with('/') {
            Some(AppEvent::SlashCommand(trimmed))
        } else {
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
