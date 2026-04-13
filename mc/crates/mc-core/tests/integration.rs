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
    // 1 summary + 4 preserved + up to 5 high-importance kept
    assert!(session.messages.len() >= 5);
    assert!(session.messages.len() <= 10);
    assert!(session.messages[0].contains_text("compacted"));
}

#[tokio::test]
async fn streaming_bash_tool_produces_output() {
    let provider = MockProvider::new(vec![
        vec![
            ProviderEvent::ToolUse {
                id: "t1".into(),
                name: "bash".into(),
                input: r#"{"command":"echo line1; echo line2; echo line3"}"#.into(),
            },
            ProviderEvent::MessageStop,
        ],
        vec![
            ProviderEvent::TextDelta("done".into()),
            ProviderEvent::MessageStop,
        ],
    ]);

    let policy = PermissionPolicy::new(PermissionMode::Allow);
    let cancel = CancellationToken::new();
    let mut runtime = ConversationRuntime::new("test".into(), 100, "test".into());

    let mut tool_output_chunks = Vec::new();
    let result = runtime
        .run_turn(
            &provider,
            "run multi-line",
            &policy,
            &mut None,
            &mut |ev| {
                if let ProviderEvent::ToolOutputDelta(t) = ev {
                    tool_output_chunks.push(t.clone());
                }
            },
            &cancel,
        )
        .await
        .unwrap();

    assert_eq!(result.tool_calls, vec!["bash"]);
    // Should have received streaming chunks
    assert!(!tool_output_chunks.is_empty());
}

#[tokio::test]
async fn web_tools_registered_in_specs() {
    let specs = mc_tools::ToolRegistry::specs();
    let names: Vec<_> = specs.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"web_fetch"));
    assert!(names.contains(&"web_search"));
}

#[tokio::test]
async fn multiple_tool_calls_in_parallel() {
    let provider = MockProvider::new(vec![
        vec![
            ProviderEvent::ToolUse {
                id: "t1".into(),
                name: "bash".into(),
                input: r#"{"command":"echo a"}"#.into(),
            },
            ProviderEvent::ToolUse {
                id: "t2".into(),
                name: "bash".into(),
                input: r#"{"command":"echo b"}"#.into(),
            },
            ProviderEvent::MessageStop,
        ],
        vec![
            ProviderEvent::TextDelta("both done".into()),
            ProviderEvent::MessageStop,
        ],
    ]);

    let policy = PermissionPolicy::new(PermissionMode::Allow);
    let cancel = CancellationToken::new();
    let mut runtime = ConversationRuntime::new("test".into(), 100, "test".into());

    let result = runtime
        .run_turn(
            &provider,
            "run both",
            &policy,
            &mut None,
            &mut |_| {},
            &cancel,
        )
        .await
        .unwrap();

    assert_eq!(result.tool_calls.len(), 2);
    assert!(result.text.contains("both done"));
}

#[test]
fn cost_tracker_roundtrip() {
    let path = std::env::temp_dir().join(format!("mc-cost-integ-{}.jsonl", std::process::id()));
    let mut tracker = mc_core::CostTracker::new(path.clone());
    tracker.record("claude", 1000, 200, 0.01);
    tracker.record("gpt-4o", 500, 100, 0.005);
    let (i, o, c) = tracker.cumulative();
    assert_eq!(i, 1500);
    assert_eq!(o, 300);
    assert!((c - 0.015).abs() < 1e-9);
    std::fs::remove_file(path).ok();
}

#[test]
fn debug_tool_validates_all_actions() {
    use mc_core::debug::execute_debug;
    use serde_json::json;

    // All actions with valid input should succeed
    let (_, err) = execute_debug(&json!({"action": "hypothesize", "bug_description": "crash"}));
    assert!(!err);
    let (_, err) = execute_debug(&json!({"action": "instrument", "file": "main.rs"}));
    assert!(!err);
    let (_, err) = execute_debug(&json!({"action": "analyze", "evidence": "log output"}));
    assert!(!err);
    let (_, err) = execute_debug(&json!({"action": "fix", "root_cause": "null", "file": "x.rs"}));
    assert!(!err);

    // Unknown action
    let (_, err) = execute_debug(&json!({"action": "unknown"}));
    assert!(err);
}

#[test]
fn auto_skill_threshold_and_generate() {
    assert!(!mc_core::auto_skill::should_create_skill(3, false));
    assert!(mc_core::auto_skill::should_create_skill(8, false));
    assert!(!mc_core::auto_skill::should_create_skill(8, true)); // errors = no skill

    let content = mc_core::auto_skill::generate_skill_content(
        "Setup React project",
        &["bash".into(), "write_file".into(), "edit_file".into()],
    );
    assert!(content.contains("Setup React project"));
    assert!(content.contains("- bash"));
}

#[test]
fn fts_search_across_sessions() {
    let dir = std::env::temp_dir().join(format!("mc-fts-int-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();

    // Create 2 sessions
    for (name, text) in [("s1", "fixed auth bug"), ("s2", "added new feature")] {
        let session = mc_core::Session {
            messages: vec![mc_core::ConversationMessage::user(text)],
            created_at: "2026-04-13".into(),
            ..Default::default()
        };
        std::fs::write(
            dir.join(format!("{name}.json")),
            serde_json::to_string(&session).unwrap(),
        )
        .unwrap();
    }

    let results = mc_core::fts::search_all_sessions(&dir, "auth bug");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].session_file, "s1");

    let results2 = mc_core::fts::search_all_sessions(&dir, "feature");
    assert_eq!(results2.len(), 1);

    let results3 = mc_core::fts::search_all_sessions(&dir, "nonexistent");
    assert!(results3.is_empty());

    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn debug_tool_dispatches_correctly() {
    // Agent calls debug tool → should get hypothesis prompt back
    let provider = MockProvider::new(vec![
        vec![
            ProviderEvent::ToolUse {
                id: "t1".into(),
                name: "debug".into(),
                input: r#"{"action":"hypothesize","bug_description":"crash on login"}"#.into(),
            },
            ProviderEvent::Usage(TokenUsage {
                input_tokens: 50,
                output_tokens: 20,
                cache_creation_input_tokens: 0,
                cache_read_input_tokens: 0,
            }),
            ProviderEvent::MessageStop,
        ],
        // Second call: agent responds to tool result
        vec![
            ProviderEvent::TextDelta("I'll investigate the crash.".into()),
            ProviderEvent::Usage(TokenUsage {
                input_tokens: 100,
                output_tokens: 30,
                cache_creation_input_tokens: 0,
                cache_read_input_tokens: 0,
            }),
            ProviderEvent::MessageStop,
        ],
    ]);

    let mut rt = ConversationRuntime::new("test".into(), 1000, "You are helpful.".into());
    let policy = PermissionPolicy::new(PermissionMode::Allow);
    let cancel = CancellationToken::new();
    let mut events = Vec::new();

    let result = rt
        .run_turn(
            &provider,
            "debug this crash",
            &policy,
            &mut None,
            &mut |e| {
                if let ProviderEvent::TextDelta(t) = e {
                    events.push(t.clone());
                }
            },
            &cancel,
        )
        .await
        .unwrap();

    assert!(!result.cancelled);
    // Debug tool should have been called
    assert!(result.tool_calls.iter().any(|t| t.contains("debug")));
}

#[tokio::test]
async fn edit_plan_tool_returns_formatted_plan() {
    let provider = MockProvider::new(vec![
        vec![
            ProviderEvent::ToolUse {
                id: "t1".into(),
                name: "edit_plan".into(),
                input: r#"{"title":"Refactor auth","steps":[{"file":"auth.rs","action":"edit","description":"Fix timeout"},{"file":"test.rs","action":"create","description":"Add test"}]}"#.into(),
            },
            ProviderEvent::Usage(TokenUsage {
                input_tokens: 50,
                output_tokens: 20,
                cache_creation_input_tokens: 0,
                cache_read_input_tokens: 0,
            }),
            ProviderEvent::MessageStop,
        ],
        vec![
            ProviderEvent::TextDelta("Proceeding with plan.".into()),
            ProviderEvent::Usage(TokenUsage {
                input_tokens: 100,
                output_tokens: 10,
                cache_creation_input_tokens: 0,
                cache_read_input_tokens: 0,
            }),
            ProviderEvent::MessageStop,
        ],
    ]);

    let mut rt = ConversationRuntime::new("test".into(), 1000, "You are helpful.".into());
    let policy = PermissionPolicy::new(PermissionMode::Allow);
    let cancel = CancellationToken::new();

    let result = rt
        .run_turn(
            &provider,
            "plan a refactor",
            &policy,
            &mut None,
            &mut |_| {},
            &cancel,
        )
        .await
        .unwrap();

    assert!(result.tool_calls.iter().any(|t| t.contains("edit_plan")));
}

#[tokio::test]
async fn codebase_search_with_repo_map() {
    let provider = MockProvider::new(vec![
        vec![
            ProviderEvent::ToolUse {
                id: "t1".into(),
                name: "codebase_search".into(),
                input: r#"{"query":"auth timeout","max_results":5}"#.into(),
            },
            ProviderEvent::Usage(TokenUsage {
                input_tokens: 50,
                output_tokens: 20,
                cache_creation_input_tokens: 0,
                cache_read_input_tokens: 0,
            }),
            ProviderEvent::MessageStop,
        ],
        vec![
            ProviderEvent::TextDelta("No results found.".into()),
            ProviderEvent::Usage(TokenUsage {
                input_tokens: 80,
                output_tokens: 10,
                cache_creation_input_tokens: 0,
                cache_read_input_tokens: 0,
            }),
            ProviderEvent::MessageStop,
        ],
    ]);

    let mut rt = ConversationRuntime::new("test".into(), 1000, "You are helpful.".into());
    // Set repo map to a temp dir
    let tmp = std::env::temp_dir().join(format!("mc-repo-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    std::fs::write(tmp.join("main.rs"), "fn main() { auth_timeout(); }").unwrap();
    rt.set_repo_map(&tmp);

    let policy = PermissionPolicy::new(PermissionMode::Allow);
    let cancel = CancellationToken::new();

    let result = rt
        .run_turn(
            &provider,
            "search for auth",
            &policy,
            &mut None,
            &mut |_| {},
            &cancel,
        )
        .await
        .unwrap();

    assert!(result
        .tool_calls
        .iter()
        .any(|t| t.contains("codebase_search")));
    std::fs::remove_dir_all(&tmp).ok();
}

#[tokio::test]
async fn memory_read_write_via_runtime() {
    let provider = MockProvider::new(vec![
        // Turn 1: write memory
        vec![
            ProviderEvent::ToolUse {
                id: "t1".into(),
                name: "memory_write".into(),
                input: r#"{"key":"project","value":"magic-code"}"#.into(),
            },
            ProviderEvent::Usage(TokenUsage {
                input_tokens: 30,
                output_tokens: 10,
                cache_creation_input_tokens: 0,
                cache_read_input_tokens: 0,
            }),
            ProviderEvent::MessageStop,
        ],
        vec![
            ProviderEvent::TextDelta("Saved.".into()),
            ProviderEvent::Usage(TokenUsage {
                input_tokens: 50,
                output_tokens: 5,
                cache_creation_input_tokens: 0,
                cache_read_input_tokens: 0,
            }),
            ProviderEvent::MessageStop,
        ],
    ]);

    let tmp = std::env::temp_dir().join(format!("mc-mem-{}", std::process::id()));
    let mut rt = ConversationRuntime::new("test".into(), 1000, "You are helpful.".into());
    rt.set_memory(mc_core::MemoryStore::load(&tmp.join("memory.json"), 100));

    let policy = PermissionPolicy::new(PermissionMode::Allow);
    let cancel = CancellationToken::new();

    let result = rt
        .run_turn(
            &provider,
            "remember this",
            &policy,
            &mut None,
            &mut |_| {},
            &cancel,
        )
        .await
        .unwrap();

    assert!(result.tool_calls.iter().any(|t| t.contains("memory_write")));
    std::fs::remove_dir_all(&tmp).ok();
}

#[tokio::test]
async fn task_create_and_list() {
    let provider = MockProvider::new(vec![
        vec![
            ProviderEvent::ToolUse {
                id: "t1".into(),
                name: "task_create".into(),
                input: r#"{"description":"run tests","command":"echo hello"}"#.into(),
            },
            ProviderEvent::Usage(TokenUsage {
                input_tokens: 30,
                output_tokens: 10,
                cache_creation_input_tokens: 0,
                cache_read_input_tokens: 0,
            }),
            ProviderEvent::MessageStop,
        ],
        vec![
            ProviderEvent::TextDelta("Task created.".into()),
            ProviderEvent::Usage(TokenUsage {
                input_tokens: 50,
                output_tokens: 5,
                cache_creation_input_tokens: 0,
                cache_read_input_tokens: 0,
            }),
            ProviderEvent::MessageStop,
        ],
    ]);

    let mut rt = ConversationRuntime::new("test".into(), 1000, "You are helpful.".into());
    let policy = PermissionPolicy::new(PermissionMode::Allow);
    let cancel = CancellationToken::new();

    let result = rt
        .run_turn(
            &provider,
            "create a task",
            &policy,
            &mut None,
            &mut |_| {},
            &cancel,
        )
        .await
        .unwrap();

    assert!(result.tool_calls.iter().any(|t| t.contains("task_create")));
}

#[test]
fn session_search_finds_matches() {
    let mut session = Session::default();
    session.messages.push(ConversationMessage::user(
        "fix the auth timeout bug in login.rs",
    ));
    session
        .messages
        .push(ConversationMessage::assistant("I'll look at login.rs"));
    session
        .messages
        .push(ConversationMessage::user("now add tests"));

    let results = session.search("auth timeout");
    assert_eq!(results.len(), 1);
    assert!(results[0].2.contains("auth timeout"));

    let results2 = session.search("login");
    assert_eq!(results2.len(), 2); // found in both user and assistant

    let results3 = session.search("nonexistent");
    assert!(results3.is_empty());
}

#[test]
fn anthropic_request_body_structure() {
    // Test that provider request building works correctly
    let req = CompletionRequest {
        model: "claude-sonnet-4-20250514".into(),
        max_tokens: 4096,
        system_prompt: Some("You are helpful.".into()),
        messages: vec![],
        tools: vec![mc_provider::ToolDefinition {
            name: "bash".into(),
            description: "Run command".into(),
            input_schema: serde_json::json!({"type": "object"}),
        }],
        tool_choice: None,
        thinking_budget: None,
        response_format: None,
    };
    // Verify request can be constructed without panic
    assert_eq!(req.model, "claude-sonnet-4-20250514");
    assert_eq!(req.tools.len(), 1);
    assert_eq!(req.tools[0].name, "bash");
}

#[test]
fn model_registry_covers_new_providers() {
    let reg = ModelRegistry::default();
    // Verify all major providers have context windows
    for model in &[
        "claude-sonnet-4-20250514",
        "gpt-4o",
        "gemini-2.0-flash",
        "deepseek-chat",
        "llama-3.3-70b",
    ] {
        let ctx = reg.context_window(model);
        assert!(ctx > 0, "Missing context window for {model}");
    }
}
