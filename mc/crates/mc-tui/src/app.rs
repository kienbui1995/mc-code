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
    Usage { input: u32, output: u32 },
    Done,
    Error(String),
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
    pub should_quit: bool,
    pub plan_mode: bool,
    pub compact_requested: bool,
    pub save_requested: Option<String>,
    pub load_requested: Option<String>,
    /// Whether user has manually scrolled up (disables auto-scroll).
    auto_scroll: bool,
    /// Terminal height for scroll calculations.
    pub viewport_height: u16,
}

impl App {
    #[must_use]
    pub fn new(model: String) -> Self {
        let history_path = std::env::var_os("HOME")
            .map(|h| {
                std::path::PathBuf::from(h)
                    .join(".local/share/magic-code/history")
            });
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
            should_quit: false,
            plan_mode: false,
            compact_requested: false,
            save_requested: None,
            load_requested: None,
            auto_scroll: true,
            viewport_height: 20,
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

    fn handle_slash_command(&mut self, cmd: &str) {
        let parts: Vec<&str> = cmd.splitn(2, ' ').collect();
        match parts[0] {
            "/help" => {
                self.output_lines.push(String::new());
                self.output_lines.push(
                    "Commands: /help /quit /status /cost /compact /save <name> /load <name> /plan"
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
                self.output_lines
                    .push("Compaction requested.".into());
                self.compact_requested = true;
            }
            "/cost" => {
                let cost = mc_core::ModelRegistry::default().estimate_cost(
                    &self.model,
                    self.total_input_tokens,
                    self.total_output_tokens,
                );
                self.output_lines.push(format!(
                    "Session cost: ${cost:.4} ({} input + {} output tokens)",
                    self.total_input_tokens, self.total_output_tokens
                ));
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
