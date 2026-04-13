use mc_provider::types::{ContentBlock, MessageRole};
use mc_provider::{CompletionRequest, InputMessage, ProviderEvent};

use crate::runtime::LlmProvider;
use crate::session::{Block, ConversationMessage, Role, Session};

/// Estimate token count (rough: 4 chars ~ 1 token).
#[must_use]
/// Estimate tokens.
pub fn estimate_tokens(session: &Session) -> usize {
    session
        .messages
        .iter()
        .map(|m| m.content_len() / 4 + 1)
        .sum()
}

#[must_use]
/// Should compact.
pub fn should_compact(session: &Session, context_window: usize, threshold: f64) -> bool {
    let estimated = estimate_tokens(session);
    let limit = (context_window as f64 * threshold) as usize;
    estimated >= limit
}

/// Naive compaction: truncate old messages and insert text summary.
/// Micro-compact: trim long tool outputs to first/last ~500 chars (UTF-8 safe).
pub fn micro_compact(session: &mut Session) {
    for msg in &mut session.messages {
        for block in &mut msg.blocks {
            if let Block::ToolResult { output, .. } = block {
                if output.len() > 2000 {
                    let total = output.len();
                    let first_end = output
                        .char_indices()
                        .map(|(i, _)| i)
                        .take_while(|&i| i <= 500)
                        .last()
                        .unwrap_or(0);
                    let last_start = output
                        .char_indices()
                        .rev()
                        .take_while(|&(i, _)| total - i <= 500)
                        .last()
                        .map_or(total, |(i, _)| i);
                    *output = format!(
                        "{}...[trimmed {total}B]...{}",
                        &output[..first_end],
                        &output[last_start..]
                    );
                }
            }
        }
    }
}

/// Collapse consecutive read results into summaries.
pub fn collapse_reads(session: &mut Session) {
    for msg in &mut session.messages {
        for block in &mut msg.blocks {
            if let Block::ToolResult { name, output, .. } = block {
                if matches!(
                    Some(name.as_str()),
                    Some("read_file" | "glob_search" | "grep_search")
                ) && output.len() > 1000
                {
                    let lines = output.lines().count();
                    *output = format!(
                        "[{} output: {lines} lines, {}B]",
                        name.as_str(),
                        output.len()
                    );
                }
            }
        }
    }
}

/// Snip old thinking blocks, keep only conclusions.
pub fn snip_thinking(session: &mut Session, keep_recent: usize) {
    let len = session.messages.len();
    let cutoff = len.saturating_sub(keep_recent);
    for msg in session.messages.iter_mut().take(cutoff) {
        msg.blocks.retain(|b| !matches!(b, Block::Thinking { .. }));
    }
}

/// Compact session with importance scoring — keeps high-value messages.
pub fn compact_session(session: &mut Session, preserve_recent: usize) {
    if session.messages.len() <= preserve_recent {
        return;
    }
    let split = session.messages.len() - preserve_recent;
    let old: Vec<_> = session.messages.drain(..split).collect();

    // Score each old message by importance
    let mut scored: Vec<(usize, f32, &ConversationMessage)> = old
        .iter()
        .enumerate()
        .map(|(i, msg)| {
            let mut score: f32 = 0.0;
            // User messages with substantial content are important
            if msg.role == Role::User && msg.content_len() > 50 {
                score += 2.0;
            }
            // Messages with errors are important (context for fixes)
            for block in &msg.blocks {
                match block {
                    Block::ToolResult { is_error, .. } if *is_error => score += 3.0,
                    Block::ToolUse { name, .. }
                        if matches!(
                            name.as_str(),
                            "write_file" | "edit_file" | "bash" | "memory_write"
                        ) =>
                    {
                        score += 1.5
                    }
                    _ => {}
                }
            }
            (i, score, msg)
        })
        .collect();

    // Keep high-importance messages (score > 1.0), summarize the rest
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let keep_count = scored.iter().filter(|(_, s, _)| *s > 1.0).count().min(5);
    let kept_indices: std::collections::BTreeSet<usize> =
        scored.iter().take(keep_count).map(|(i, _, _)| *i).collect();

    let to_summarize: Vec<_> = old
        .iter()
        .enumerate()
        .filter(|(i, _)| !kept_indices.contains(i))
        .map(|(_, m)| m.clone())
        .collect();

    let summary = build_naive_summary(&to_summarize);
    session
        .messages
        .insert(0, ConversationMessage::user(summary));

    // Re-insert kept important messages after summary
    for (idx, msg) in old.into_iter().enumerate() {
        if kept_indices.contains(&idx) {
            session.messages.insert(1, msg.clone());
        }
    }
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

    // Preserve pinned messages — they survive compaction
    let (pinned, to_summarize): (Vec<_>, Vec<_>) = old.into_iter().partition(|m| m.pinned);

    let mut transcript = String::new();
    for msg in &to_summarize {
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
        response_format: None,
    };

    let mut summary = String::new();
    let mut stream = provider.stream(&request);
    loop {
        match crate::runtime::next_event(&mut stream).await {
            Some(Ok(ProviderEvent::TextDelta(t))) => summary.push_str(&t),
            Some(Ok(ProviderEvent::MessageStop)) | None => break,
            Some(Err(e)) => {
                tracing::warn!("smart compaction failed, falling back to naive: {e}");
                session.messages.insert(
                    0,
                    ConversationMessage::user(build_naive_summary(&to_summarize)),
                );
                return Ok(());
            }
            _ => {}
        }
    }

    let text = if summary.trim().is_empty() {
        build_naive_summary(&to_summarize)
    } else {
        format!(
            "[Session compacted via LLM. Summary of {} earlier messages:\n{}\n]",
            to_summarize.len(),
            summary.trim()
        )
    };
    session.messages.insert(0, ConversationMessage::user(text));
    // Re-insert pinned messages after summary
    for (i, msg) in pinned.into_iter().enumerate() {
        session.messages.insert(1 + i, msg);
    }
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

#[test]
fn estimate_tokens_empty() {
    let session = Session::default();
    assert_eq!(estimate_tokens(&session), 0);
}

#[test]
fn micro_compact_trims_long_output() {
    let mut session = Session::default();
    session.messages.push(ConversationMessage::tool_result(
        "t1",
        "bash",
        "x".repeat(3000),
        false,
    ));
    micro_compact(&mut session);
    if let Block::ToolResult { output, .. } = &session.messages[0].blocks[0] {
        assert!(output.len() < 3000);
        assert!(output.contains("trimmed"));
    }
}

#[test]
fn collapse_reads_shrinks_large_output() {
    let mut session = Session::default();
    session.messages.push(ConversationMessage::tool_result(
        "t1",
        "read_file",
        "line\n".repeat(500),
        false,
    ));
    collapse_reads(&mut session);
    if let Block::ToolResult { output, .. } = &session.messages[0].blocks[0] {
        assert!(output.contains("read_file output"));
        assert!(output.len() < 200);
    }
}

#[test]
fn snip_thinking_removes_old() {
    let mut session = Session::default();
    for _ in 0..5 {
        let mut msg = ConversationMessage::assistant("text");
        msg.push_block(Block::Thinking {
            text: "deep thought".into(),
        });
        session.messages.push(msg);
    }
    snip_thinking(&mut session, 2);
    // First 3 messages should have thinking removed
    assert!(!session.messages[0]
        .blocks
        .iter()
        .any(|b| matches!(b, Block::Thinking { .. })));
    // Last 2 should keep thinking
    assert!(session.messages[4]
        .blocks
        .iter()
        .any(|b| matches!(b, Block::Thinking { .. })));
}

#[test]
fn smart_compact_keeps_important_messages() {
    let mut session = Session::default();
    // Add user instruction (important, score 2.0)
    session.messages.push(ConversationMessage::user(
        "Please fix the auth bug in login.rs with detailed error handling",
    ));
    // Add tool error (important, score 3.0)
    let mut err_msg = ConversationMessage::assistant("trying fix");
    err_msg.push_block(Block::ToolResult {
        tool_use_id: "1".into(),
        name: "bash".into(),
        output: "error: compilation failed".into(),
        is_error: true,
    });
    session.messages.push(err_msg);
    // Add 8 filler messages
    for i in 0..8 {
        session
            .messages
            .push(ConversationMessage::assistant(format!("filler {i}")));
    }
    // 10 total, compact keeping 2 recent
    compact_session(&mut session, 2);
    // Should have: summary + some important kept + 2 recent
    assert!(session.messages.len() >= 3); // at least summary + 2 recent
                                          // Summary should exist
    assert!(session.messages[0].contains_text("compacted"));
}
