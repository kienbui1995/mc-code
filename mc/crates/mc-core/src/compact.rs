use mc_provider::{
    CompletionRequest, InputMessage, ProviderEvent,
};
use mc_provider::types::{ContentBlock, MessageRole};

use crate::runtime::LlmProvider;
use crate::session::{ConversationMessage, Session};

/// Estimate token count from session messages (rough: 4 chars ≈ 1 token).
#[must_use]
pub fn estimate_tokens(session: &Session) -> usize {
    session.messages.iter().map(|m| m.content.len() / 4 + 1).sum()
}

/// Check if session should be compacted.
#[must_use]
pub fn should_compact(session: &Session, context_window: usize, threshold: f64) -> bool {
    let estimated = estimate_tokens(session);
    let limit = (context_window as f64 * threshold) as usize;
    estimated >= limit
}

/// Naive compaction: truncate old messages and insert text summary.
pub fn compact_session(session: &mut Session, preserve_recent: usize) {
    if session.messages.len() <= preserve_recent {
        return;
    }

    let split = session.messages.len() - preserve_recent;
    let old_messages: Vec<_> = session.messages.drain(..split).collect();

    let mut summary_parts = Vec::new();
    for msg in &old_messages {
        match msg.role.as_str() {
            "user" => summary_parts.push(format!("User asked: {}", truncate(&msg.content, 100))),
            "assistant" if msg.tool_name.is_none() => {
                summary_parts.push(format!("Assistant: {}", truncate(&msg.content, 100)));
            }
            "assistant" => {
                summary_parts.push(format!("Tool call: {}", msg.tool_name.as_deref().unwrap_or("?")));
            }
            "tool" => {
                summary_parts.push(format!(
                    "Tool result ({}): {}",
                    msg.tool_name.as_deref().unwrap_or("?"),
                    truncate(&msg.content, 80)
                ));
            }
            _ => {}
        }
    }

    let summary = format!(
        "[Session compacted. Summary of {} earlier messages:\n{}\n]",
        old_messages.len(),
        summary_parts.join("\n")
    );

    session.messages.insert(0, ConversationMessage::user(summary));
}

/// Smart compaction: use LLM to summarize old messages, preserving key context.
pub async fn smart_compact(
    provider: &dyn LlmProvider,
    session: &mut Session,
    model: &str,
    preserve_recent: usize,
) -> Result<(), mc_provider::ProviderError> {
    if session.messages.len() <= preserve_recent {
        return Ok(());
    }

    let split = session.messages.len() - preserve_recent;
    let old_messages: Vec<_> = session.messages.drain(..split).collect();

    // Build a transcript of old messages for the LLM to summarize
    let mut transcript = String::new();
    for msg in &old_messages {
        let role = match msg.role.as_str() {
            "user" => "User",
            "assistant" if msg.tool_name.is_some() => "Tool Call",
            "assistant" => "Assistant",
            "tool" => "Tool Result",
            r => r,
        };
        transcript.push_str(&format!("[{role}] {}\n", truncate(&msg.content, 500)));
    }

    let prompt = format!(
        "Summarize this conversation concisely. Preserve: key decisions, file paths mentioned, \
         code changes made, errors encountered, and current task state. Be factual, no opinions.\n\n\
         ---\n{transcript}---\n\nSummary:"
    );

    let request = CompletionRequest {
        model: model.to_string(),
        max_tokens: 1024,
        system_prompt: Some("You are a conversation summarizer. Output only the summary.".into()),
        messages: vec![InputMessage {
            role: MessageRole::User,
            content: vec![ContentBlock::Text { text: prompt }],
        }],
        tools: Vec::new(),
        tool_choice: None,
    };

    // Collect summary from stream
    let mut summary = String::new();
    let mut stream = provider.stream(&request);
    loop {
        let next = crate::runtime::next_event(&mut stream).await;
        match next {
            Some(Ok(ProviderEvent::TextDelta(t))) => summary.push_str(&t),
            Some(Ok(ProviderEvent::MessageStop)) | None => break,
            Some(Err(e)) => {
                // Fallback to naive compaction on error
                tracing::warn!("smart compaction failed, falling back to naive: {e}");
                let naive_summary = build_naive_summary(&old_messages);
                session.messages.insert(0, ConversationMessage::user(naive_summary));
                return Ok(());
            }
            _ => {}
        }
    }

    if summary.trim().is_empty() {
        let naive_summary = build_naive_summary(&old_messages);
        session.messages.insert(0, ConversationMessage::user(naive_summary));
    } else {
        let compacted = format!(
            "[Session compacted via LLM. Summary of {} earlier messages:\n{}\n]",
            old_messages.len(),
            summary.trim()
        );
        session.messages.insert(0, ConversationMessage::user(compacted));
    }

    Ok(())
}

fn build_naive_summary(old_messages: &[ConversationMessage]) -> String {
    let mut parts = Vec::new();
    for msg in old_messages {
        match msg.role.as_str() {
            "user" => parts.push(format!("User asked: {}", truncate(&msg.content, 100))),
            "assistant" if msg.tool_name.is_none() => {
                parts.push(format!("Assistant: {}", truncate(&msg.content, 100)));
            }
            "assistant" => {
                parts.push(format!("Tool call: {}", msg.tool_name.as_deref().unwrap_or("?")));
            }
            "tool" => {
                parts.push(format!(
                    "Tool result ({}): {}",
                    msg.tool_name.as_deref().unwrap_or("?"),
                    truncate(&msg.content, 80)
                ));
            }
            _ => {}
        }
    }
    format!(
        "[Session compacted. Summary of {} earlier messages:\n{}\n]",
        old_messages.len(),
        parts.join("\n")
    )
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let end = s.char_indices().map(|(i, _)| i).take_while(|&i| i <= max).last().unwrap_or(0);
    format!("{}...", &s[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_preserves_recent() {
        let mut session = Session::default();
        for i in 0..10 {
            session.messages.push(ConversationMessage::user(format!("msg {i}")));
            session.messages.push(ConversationMessage::assistant(format!("reply {i}")));
        }
        assert_eq!(session.messages.len(), 20);

        compact_session(&mut session, 4);
        assert_eq!(session.messages.len(), 5);
        assert!(session.messages[0].content.contains("compacted"));
    }

    #[test]
    fn truncate_safe_with_multibyte() {
        let s = "hello🌍world";
        let t = truncate(s, 6);
        assert!(t.ends_with("..."));
        assert!(t.len() < s.len() + 3);
    }

    #[test]
    fn should_compact_threshold() {
        let mut session = Session::default();
        for _ in 0..100 {
            session.messages.push(ConversationMessage::user("a".repeat(400)));
        }
        assert!(should_compact(&session, 200_000, 0.05));
        assert!(!should_compact(&session, 200_000, 0.99));
    }
}
