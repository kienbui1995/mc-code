use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
/// Role.
pub enum Role {
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
/// Block.
pub enum Block {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: String,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        name: String,
        output: String,
        is_error: bool,
    },
    #[serde(rename = "image")]
    Image {
        source: ImageSource,
        media_type: String,
    },
    #[serde(rename = "thinking")]
    Thinking { text: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
/// Imagesource.
pub enum ImageSource {
    Base64 { data: String },
    Path { path: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Conversationmessage.
pub struct ConversationMessage {
    pub role: Role,
    pub blocks: Vec<Block>,
}

impl ConversationMessage {
    #[must_use]
    /// User.
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            blocks: vec![Block::Text { text: text.into() }],
        }
    }

    #[must_use]
    /// Assistant.
    pub fn assistant(text: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            blocks: vec![Block::Text { text: text.into() }],
        }
    }

    #[must_use]
    /// Tool use.
    pub fn tool_use(
        id: impl Into<String>,
        name: impl Into<String>,
        input: impl Into<String>,
    ) -> Self {
        Self {
            role: Role::Assistant,
            blocks: vec![Block::ToolUse {
                id: id.into(),
                name: name.into(),
                input: input.into(),
            }],
        }
    }

    #[must_use]
    /// Tool result.
    pub fn tool_result(
        id: impl Into<String>,
        name: impl Into<String>,
        output: impl Into<String>,
        is_error: bool,
    ) -> Self {
        Self {
            role: Role::Tool,
            blocks: vec![Block::ToolResult {
                tool_use_id: id.into(),
                name: name.into(),
                output: output.into(),
                is_error,
            }],
        }
    }

    /// Push block.
    pub fn push_block(&mut self, block: Block) {
        self.blocks.push(block);
    }

    #[must_use]
    /// Content len.
    pub fn content_len(&self) -> usize {
        self.blocks
            .iter()
            .map(|b| match b {
                Block::Text { text } | Block::Thinking { text } => text.chars().count(),
                Block::ToolUse { input, .. } => input.chars().count(),
                Block::ToolResult { output, .. } => output.chars().count(),
                Block::Image { .. } => 6400,
            })
            .sum()
    }

    #[must_use]
    /// Summary.
    pub fn summary(&self, max_text: usize) -> String {
        self.blocks
            .iter()
            .map(|b| match b {
                Block::Text { text } => truncate(text, max_text),
                Block::ToolUse { name, .. } => format!("[tool_use: {name}]"),
                Block::ToolResult {
                    name,
                    output,
                    is_error,
                    ..
                } => {
                    if *is_error {
                        format!("[error: {name}]")
                    } else {
                        format!("[{name}: {}]", truncate(output, 80))
                    }
                }
                Block::Image { media_type, .. } => format!("[image: {media_type}]"),
                Block::Thinking { text } => format!("[thinking: {}]", truncate(text, 80)),
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[must_use]
    /// Contains text.
    pub fn contains_text(&self, needle: &str) -> bool {
        self.blocks.iter().any(|b| match b {
            Block::Text { text } | Block::Thinking { text } => text.contains(needle),
            Block::ToolResult { output, .. } => output.contains(needle),
            Block::ToolUse { input, .. } => input.contains(needle),
            Block::Image { .. } => false,
        })
    }
}

/// Truncate.
pub fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let end = s
        .char_indices()
        .map(|(i, _)| i)
        .take_while(|&i| i <= max)
        .last()
        .unwrap_or(0);
    format!("{}...", &s[..end])
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
/// Session.
pub struct Session {
    pub messages: Vec<ConversationMessage>,
    pub input_tokens: u32,
    pub output_tokens: u32,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub branch_id: Option<String>,
    #[serde(default)]
    pub parent_branch: Option<String>,
    #[serde(default)]
    pub fork_point: Option<usize>,
}

impl Session {
    /// Save.
    pub fn save(&self, path: &Path) -> Result<(), std::io::Error> {
        let json =
            serde_json::to_string_pretty(self).map_err(|e| std::io::Error::other(e.to_string()))?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, json)
    }

    /// Load.
    pub fn load(path: &Path) -> Result<Self, std::io::Error> {
        let json = fs::read_to_string(path)?;
        serde_json::from_str(&json).map_err(|e| std::io::Error::other(e.to_string()))
    }

    /// Search messages for a query string. Returns matching (index, role, snippet).
    #[must_use]
    pub fn search(&self, query: &str) -> Vec<(usize, String, String)> {
        let q = query.to_lowercase();
        let mut results = Vec::new();
        for (i, msg) in self.messages.iter().enumerate() {
            for block in &msg.blocks {
                if let Block::Text { text } = block {
                    if let Some(pos) = text.to_lowercase().find(&q) {
                        let start = pos.saturating_sub(50);
                        let end = (pos + q.len() + 50).min(text.len());
                        let snippet = format!("...{}...", &text[start..end]);
                        let role = format!("{:?}", msg.role);
                        results.push((i, role, snippet));
                        break;
                    }
                }
            }
        }
        results
    }

    /// Export session as readable markdown.
    #[must_use]
    pub fn to_markdown(&self) -> String {
        let mut out = String::from("# Conversation Export\n\n");
        for msg in &self.messages {
            match msg.role {
                Role::User => out.push_str("## 🧑 User\n\n"),
                Role::Assistant => out.push_str("## 🤖 Assistant\n\n"),
                Role::Tool => out.push_str("## 🔧 Tool\n\n"),
            }
            for block in &msg.blocks {
                match block {
                    Block::Text { text } => {
                        out.push_str(text);
                        out.push_str("\n\n");
                    }
                    Block::ToolUse { name, input, .. } => {
                        out.push_str(&format!("**Tool call:** `{name}`\n"));
                        if !input.is_empty() {
                            out.push_str(&format!("```json\n{}\n```\n\n", truncate(input, 500)));
                        }
                    }
                    Block::ToolResult {
                        name,
                        output,
                        is_error,
                        ..
                    } => {
                        let icon = if *is_error { "❌" } else { "✅" };
                        out.push_str(&format!("{icon} **{name}**\n"));
                        if !output.is_empty() {
                            out.push_str(&format!("```\n{}\n```\n\n", truncate(output, 1000)));
                        }
                    }
                    Block::Thinking { text } => {
                        out.push_str(&format!(
                            "<details>\n<summary>💭 Thinking</summary>\n\n{}\n\n</details>\n\n",
                            truncate(text, 500)
                        ));
                    }
                    Block::Image { .. } => {
                        out.push_str("*[image]*\n\n");
                    }
                }
            }
            out.push_str("---\n\n");
        }
        out.push_str(&format!(
            "*Tokens: {} input, {} output*\n",
            self.input_tokens, self.output_tokens
        ));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_save_load_roundtrip() {
        let path = std::path::PathBuf::from(format!("/tmp/mc-session-{}.json", std::process::id()));
        let mut session = Session::default();
        session.messages.push(ConversationMessage::user("hello"));
        session.messages.push(ConversationMessage::assistant("hi"));
        session.input_tokens = 10;
        session.save(&path).unwrap();
        let loaded = Session::load(&path).unwrap();
        assert_eq!(loaded.messages.len(), 2);
        assert_eq!(loaded.input_tokens, 10);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn multi_block_assistant() {
        let mut msg = ConversationMessage::assistant("thinking...");
        msg.push_block(Block::ToolUse {
            id: "t1".into(),
            name: "bash".into(),
            input: "{}".into(),
        });
        assert_eq!(msg.blocks.len(), 2);
    }

    #[test]
    fn serde_roundtrip() {
        let msg = ConversationMessage::tool_use("t1", "bash", r#"{"cmd":"ls"}"#);
        let json = serde_json::to_string(&msg).unwrap();
        let back: ConversationMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(back.role, Role::Assistant);
        assert_eq!(back.blocks.len(), 1);
    }

    #[test]
    fn contains_text_search() {
        let msg = ConversationMessage::tool_result("t1", "bash", "denied by policy", true);
        assert!(msg.contains_text("denied"));
        assert!(!msg.contains_text("allowed"));
    }
}

#[test]
fn to_markdown_basic() {
    let mut session = Session::default();
    session.messages.push(ConversationMessage::user("hello"));
    session
        .messages
        .push(ConversationMessage::assistant("hi there"));
    let md = session.to_markdown();
    assert!(md.contains("# Conversation Export"));
    assert!(md.contains("🧑 User"));
    assert!(md.contains("hello"));
    assert!(md.contains("🤖 Assistant"));
    assert!(md.contains("hi there"));
}

#[test]
fn to_markdown_with_tool_calls() {
    let mut session = Session::default();
    session.messages.push(ConversationMessage::tool_use(
        "t1",
        "bash",
        r#"{"command":"ls"}"#,
    ));
    session.messages.push(ConversationMessage::tool_result(
        "t1", "bash", "file.txt", false,
    ));
    let md = session.to_markdown();
    assert!(md.contains("`bash`"));
    assert!(md.contains("✅ **bash**"));
    assert!(md.contains("file.txt"));
}

#[test]
fn to_markdown_empty_session() {
    let session = Session::default();
    let md = session.to_markdown();
    assert!(md.contains("# Conversation Export"));
    assert!(md.contains("Tokens: 0 input, 0 output"));
}

#[test]
fn to_markdown_with_thinking() {
    let mut session = Session::default();
    let mut msg = ConversationMessage::assistant("answer");
    msg.push_block(Block::Thinking {
        text: "reasoning here".into(),
    });
    session.messages.push(msg);
    let md = session.to_markdown();
    assert!(md.contains("💭 Thinking"));
    assert!(md.contains("reasoning here"));
}

#[test]
fn to_markdown_with_error_result() {
    let mut session = Session::default();
    session.messages.push(ConversationMessage::tool_result(
        "t1",
        "bash",
        "command failed",
        true,
    ));
    let md = session.to_markdown();
    assert!(md.contains("❌"));
}
