use std::path::{Path, PathBuf};

/// Result from cross-session full-text search.
#[derive(Debug, Clone)]
pub struct FtsResult {
    pub session_file: String,
    pub timestamp: String,
    pub snippet: String,
}

/// Search all saved session JSON files for a query string.
/// Returns matching snippets with session file names.
#[must_use]
pub fn search_all_sessions(sessions_dir: &Path, query: &str) -> Vec<FtsResult> {
    let q = query.to_lowercase();
    let mut results = Vec::new();

    let entries = match std::fs::read_dir(sessions_dir) {
        Ok(e) => e,
        Err(_) => return results,
    };

    let mut files: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "json"))
        .collect();

    // Most recent first
    files.sort_by(|a, b| {
        b.metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
            .cmp(
                &a.metadata()
                    .and_then(|m| m.modified())
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH),
            )
    });

    for path in files.iter().take(100) {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let session: crate::Session = match serde_json::from_str(&content) {
            Ok(s) => s,
            Err(_) => continue,
        };

        let file_name = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        for msg in &session.messages {
            let mut matched = false;
            for block in &msg.blocks {
                let text = match block {
                    crate::session::Block::Text { text } => text.as_str(),
                    crate::session::Block::Thinking { text } => text.as_str(),
                    crate::session::Block::ToolUse { input, .. } => input.as_str(),
                    crate::session::Block::ToolResult { output, .. } => output.as_str(),
                    _ => continue,
                };
                let lower = text.to_lowercase();
                if let Some(char_pos) = lower.find(&q) {
                    // Safe byte slicing via char_indices
                    let byte_start = lower[..char_pos]
                        .char_indices()
                        .rev()
                        .nth(59)
                        .map_or(0, |(i, _)| i);
                    let byte_end = lower[char_pos..]
                        .char_indices()
                        .nth(q.len() + 60)
                        .map_or(text.len(), |(i, _)| char_pos + i);
                    results.push(FtsResult {
                        session_file: file_name.clone(),
                        timestamp: session.created_at.clone(),
                        snippet: format!("...{}...", &text[byte_start..byte_end]),
                    });
                    matched = true;
                    break;
                }
            }
            if matched && results.len() >= 20 {
                break;
            }
        }

        if results.len() >= 20 {
            break;
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_empty_dir() {
        let dir = std::env::temp_dir().join(format!("mc-fts-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let results = search_all_sessions(&dir, "anything");
        assert!(results.is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }
}
