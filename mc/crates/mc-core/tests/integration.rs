use mc_core::{
    compact_session, should_compact, ConversationMessage, ConversationRuntime, LlmProvider,
    ModelRegistry, Role, Session,
};
use mc_provider::{CompletionRequest, ProviderEvent, ProviderStream, TokenUsage};
use mc_tools::{PermissionMode, PermissionPolicy};
use tokio_util::sync::CancellationToken;

/// A mock provider that returns canned responses.
struct MockProvider {
    responses: Vec<Vec<ProviderEvent>>,
    call_count: std::sync::atomic::AtomicUsize,
}

impl MockProvider {
    fn new(responses: Vec<Vec<ProviderEvent>>) -> Self {
        Self {
            responses,
            call_count: std::sync::atomic::AtomicUsize::new(0),
        }
    }
}

impl LlmProvider for MockProvider {
    fn stream(&self, _request: &CompletionRequest) -> ProviderStream {
        let idx = self
            .call_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let events = self
            .responses
            .get(idx)
            .cloned()
            .unwrap_or_else(|| vec![ProviderEvent::MessageStop]);

        Box::pin(async_stream::stream! {
            for event in events {
                yield Ok(event);
            }
        })
    }
}

#[tokio::test]
async fn simple_text_response() {
    let provider = MockProvider::new(vec![vec![
        ProviderEvent::TextDelta("Hello ".into()),
        ProviderEvent::TextDelta("world!".into()),
        ProviderEvent::Usage(TokenUsage {
            input_tokens: 10,
            output_tokens: 5,
            ..Default::default()
        }),
        ProviderEvent::MessageStop,
    ]]);

    let policy = PermissionPolicy::new(PermissionMode::Allow);
    let cancel = CancellationToken::new();
    let mut runtime = ConversationRuntime::new("test".into(), 100, "be helpful".into());

    let mut collected = String::new();
    let result = runtime
        .run_turn(
            &provider,
            "hi",
            &policy,
            &mut None,
            &mut |ev| {
                if let ProviderEvent::TextDelta(t) = ev {
                    collected.push_str(t);
                }
            },
            &cancel,
        )
        .await
        .unwrap();

    assert_eq!(collected, "Hello world!");
    assert_eq!(result.text, "Hello world!");
    assert!(!result.cancelled);
    assert_eq!(result.iterations, 1);
    assert!(result.tool_calls.is_empty());
}

#[tokio::test]
async fn tool_call_and_result() {
    let provider = MockProvider::new(vec![
        // First call: LLM requests a tool
        vec![
            ProviderEvent::ToolUse {
                id: "t1".into(),
                name: "bash".into(),
                input: r#"{"command":"echo hello"}"#.into(),
            },
            ProviderEvent::MessageStop,
        ],
        // Second call: LLM responds with text after seeing tool result
        vec![
            ProviderEvent::TextDelta("The command output: hello".into()),
            ProviderEvent::MessageStop,
        ],
    ]);

    let policy = PermissionPolicy::new(PermissionMode::Allow);
    let cancel = CancellationToken::new();
    let mut runtime = ConversationRuntime::new("test".into(), 100, "be helpful".into());

    let result = runtime
        .run_turn(
            &provider,
            "run echo hello",
            &policy,
            &mut None,
            &mut |_| {},
            &cancel,
        )
        .await
        .unwrap();

    assert_eq!(result.tool_calls, vec!["bash"]);
    assert!(result.text.contains("hello"));
    assert_eq!(result.iterations, 2);
}

#[tokio::test]
async fn cancellation_stops_turn() {
    let provider = MockProvider::new(vec![vec![
        ProviderEvent::TextDelta("start".into()),
        ProviderEvent::MessageStop,
    ]]);

    let policy = PermissionPolicy::new(PermissionMode::Allow);
    let cancel = CancellationToken::new();
    cancel.cancel(); // Cancel immediately

    let mut runtime = ConversationRuntime::new("test".into(), 100, "test".into());
    let result = runtime
        .run_turn(&provider, "hi", &policy, &mut None, &mut |_| {}, &cancel)
        .await
        .unwrap();

    assert!(result.cancelled);
}

#[tokio::test]
async fn session_save_load_preserves_state() {
    let path = std::env::temp_dir().join(format!("mc-integ-{}.json", std::process::id()));

    let mut session = Session::default();
    session.messages.push(ConversationMessage::user("hello"));
    session
        .messages
        .push(ConversationMessage::assistant("hi there"));
    session.input_tokens = 42;
    session.save(&path).unwrap();

    let loaded = Session::load(&path).unwrap();
    assert_eq!(loaded.messages.len(), 2);
    assert_eq!(loaded.input_tokens, 42);
    assert!(loaded.messages[0].contains_text("hello"));

    std::fs::remove_file(path).ok();
}

#[tokio::test]
async fn permission_deny_blocks_tool() {
    let provider = MockProvider::new(vec![
        vec![
            ProviderEvent::ToolUse {
                id: "t1".into(),
                name: "bash".into(),
                input: r#"{"command":"rm -rf /"}"#.into(),
            },
            ProviderEvent::MessageStop,
        ],
        vec![
            ProviderEvent::TextDelta("ok denied".into()),
            ProviderEvent::MessageStop,
        ],
    ]);

    let policy = PermissionPolicy::new(PermissionMode::Deny);
    let cancel = CancellationToken::new();
    let mut runtime = ConversationRuntime::new("test".into(), 100, "test".into());

    let result = runtime
        .run_turn(
            &provider,
            "delete everything",
            &policy,
            &mut None,
            &mut |_| {},
            &cancel,
        )
        .await
        .unwrap();

    // Tool was called but denied
    assert_eq!(result.tool_calls, vec!["bash"]);
    // Session should have a tool_result with denial
    let denied = runtime
        .session
        .messages
        .iter()
        .any(|m| m.role == Role::Tool && m.contains_text("denied"));
    assert!(denied);
}

#[test]
fn model_registry_covers_all_providers() {
    let r = ModelRegistry::default();
    // Anthropic
    assert!(r.context_window("claude-sonnet-4-20250514") > 0);
    // OpenAI
    assert!(r.context_window("gpt-4o") > 0);
    // Gemini
    assert!(r.context_window("gemini-2.5-flash") > 0);
    // Local
    assert!(r.context_window("llama3") > 0);
    // Unknown defaults
    assert!(r.context_window("unknown-model") > 0);
}

#[test]
fn compaction_works_end_to_end() {
    let mut session = Session::default();
    for i in 0..50 {
        session.messages.push(ConversationMessage::user(format!(
            "question {i} with some padding text to increase size"
        )));
        session
            .messages
            .push(ConversationMessage::assistant(format!(
                "answer {i} with detailed explanation"
            )));
    }
    assert_eq!(session.messages.len(), 100);

    // Should trigger compaction at low threshold
    assert!(should_compact(&session, 1000, 0.1));

    compact_session(&mut session, 4);
    assert_eq!(session.messages.len(), 5); // 1 summary + 4 preserved
    assert!(session.messages[0].contains_text("compacted"));
}
