use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMessage {
    pub role: String,
    pub content: String,
    /// For `tool_use`: tool call id
    pub tool_use_id: Option<String>,
    /// For `tool_use`: tool name
    pub tool_name: Option<String>,
    /// For `tool_result`: whether it errored
    pub is_error: Option<bool>,
}

impl ConversationMessage {
    #[must_use]
    pub fn user(text: impl Into<String>) -> Self {
        Self { role: "user".into(), content: text.into(), tool_use_id: None, tool_name: None, is_error: None }
    }

    #[must_use]
    pub fn assistant(text: impl Into<String>) -> Self {
        Self { role: "assistant".into(), content: text.into(), tool_use_id: None, tool_name: None, is_error: None }
    }

    #[must_use]
    pub fn tool_use(id: impl Into<String>, name: impl Into<String>, input: impl Into<String>) -> Self {
        Self { role: "assistant".into(), content: input.into(), tool_use_id: Some(id.into()), tool_name: Some(name.into()), is_error: None }
    }

    #[must_use]
    pub fn tool_result(id: impl Into<String>, name: impl Into<String>, output: impl Into<String>, is_error: bool) -> Self {
        Self { role: "tool".into(), content: output.into(), tool_use_id: Some(id.into()), tool_name: Some(name.into()), is_error: Some(is_error) }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Session {
    pub messages: Vec<ConversationMessage>,
    pub input_tokens: u32,
    pub output_tokens: u32,
}

impl Session {
    pub fn save(&self, path: &Path) -> Result<(), std::io::Error> {
        let json = serde_json::to_string_pretty(self).map_err(|e| std::io::Error::other(e.to_string()))?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, json)
    }

    pub fn load(path: &Path) -> Result<Self, std::io::Error> {
        let json = fs::read_to_string(path)?;
        serde_json::from_str(&json).map_err(|e| std::io::Error::other(e.to_string()))
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
}
