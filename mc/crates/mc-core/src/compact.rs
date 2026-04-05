use mc_provider::types::{ContentBlock, MessageRole};
use mc_provider::{CompletionRequest, InputMessage, ProviderEvent};

use crate::runtime::LlmProvider;
use crate::session::{ConversationMessage, Role, Session};

/// Estimate token count (rough: 4 chars ~ 1 token).
#[must_use]
pub fn estimate_tokens(session: &Session) -> usize {
    session
        .messages
        .iter()
        .map(|m| m.content_len() / 4 + 1)
        .sum()
}

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
    let old: Vec<_> = session.messages.drain(..split).collect();
    let summary = build_naive_summary(&old);
    session
        .messages
        .insert(0, ConversationMessage::user(summary));
}

/// Smart compaction: use LLM to summarize old messages.
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
    let old: Vec<_> = session.messages.drain(..split).collect();

    let mut transcript = String::new();
    for msg in &old {
        let label = match msg.role {
            Role::User => "User",
            Role::Assistant => "Assistant",
            Role::Tool => "Tool",
        };
        transcript.push_str(&format!("[{label}] {}\n", msg.summary(500)));
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
        thinking_budget: None,
    };

    let mut summary = String::new();
    let mut stream = provider.stream(&request);
    loop {
        match crate::runtime::next_event(&mut stream).await {
            Some(Ok(ProviderEvent::TextDelta(t))) => summary.push_str(&t),
            Some(Ok(ProviderEvent::MessageStop)) | None => break,
            Some(Err(e)) => {
                tracing::warn!("smart compaction failed, falling back to naive: {e}");
                session
                    .messages
                    .insert(0, ConversationMessage::user(build_naive_summary(&old)));
                return Ok(());
            }
            _ => {}
        }
    }

    let text = if summary.trim().is_empty() {
        build_naive_summary(&old)
    } else {
        format!(
            "[Session compacted via LLM. Summary of {} earlier messages:\n{}\n]",
            old.len(),
            summary.trim()
        )
    };
    session.messages.insert(0, ConversationMessage::user(text));
    Ok(())
}

fn build_naive_summary(old: &[ConversationMessage]) -> String {
    let parts: Vec<String> = old
        .iter()
        .map(|m| {
            let label = match m.role {
                Role::User => "User",
                Role::Assistant => "Assistant",
                Role::Tool => "Tool",
            };
            format!("{label}: {}", m.summary(100))
        })
        .collect();
    format!(
        "[Session compacted. Summary of {} earlier messages:\n{}\n]",
        old.len(),
        parts.join("\n")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::truncate;

    #[test]
    fn compact_preserves_recent() {
        let mut session = Session::default();
        for i in 0..10 {
            session
                .messages
                .push(ConversationMessage::user(format!("msg {i}")));
            session
                .messages
                .push(ConversationMessage::assistant(format!("reply {i}")));
        }
        assert_eq!(session.messages.len(), 20);
        compact_session(&mut session, 4);
        assert_eq!(session.messages.len(), 5);
        assert!(session.messages[0].contains_text("compacted"));
    }

    #[test]
    fn should_compact_threshold() {
        let mut session = Session::default();
        for _ in 0..100 {
            session
                .messages
                .push(ConversationMessage::user("a".repeat(400)));
        }
        assert!(should_compact(&session, 200_000, 0.05));
        assert!(!should_compact(&session, 200_000, 0.99));
    }

    #[test]
    fn truncate_safe_with_multibyte() {
        let t = truncate("hello\u{1f30d}world", 6);
        assert!(t.ends_with("..."));
    }
}
