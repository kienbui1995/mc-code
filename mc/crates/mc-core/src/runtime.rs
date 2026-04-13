use base64::Engine;
use futures_core::Stream;
use std::pin::Pin;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

use mc_provider::types::{ContentBlock, MessageRole};
use mc_provider::{
    CompletionRequest, InputMessage, ProviderError, ProviderEvent, ProviderStream, TokenUsage,
    ToolChoice, ToolDefinition,
};
use mc_tools::{
    AuditEntry, AuditLog, HookEngine, HookEvent, PermissionOutcome, PermissionPolicy,
    PermissionPrompter, ToolRegistry,
};

use crate::context_resolver::ContextResolver;
use crate::memory::MemoryStore;
use crate::model_registry::ModelRegistry;
use crate::repo_map::RepoMap;
use crate::retry::RetryPolicy;
use crate::session::{Block, ConversationMessage, Role, Session};
use crate::subagent::SubagentSpawner;
use crate::token_budget::TokenBudget;
use crate::tool_cache::ToolCache;
use crate::undo::UndoManager;
use crate::usage::UsageTracker;

const MAX_ITERATIONS: usize = 16;

#[derive(Debug)]
/// Result of a single conversation turn.
pub struct TurnResult {
    pub text: String,
    pub tool_calls: Vec<String>,
    pub usage: TokenUsage,
    pub iterations: usize,
    pub cancelled: bool,
}

/// Llmprovider.
pub trait LlmProvider: Send + Sync {
    fn stream(&self, request: &CompletionRequest) -> ProviderStream;
}

impl LlmProvider for mc_provider::AnthropicProvider {
    fn stream(&self, request: &CompletionRequest) -> ProviderStream {
        self.stream(request)
    }
}
impl LlmProvider for mc_provider::GenericProvider {
    fn stream(&self, request: &CompletionRequest) -> ProviderStream {
        self.stream(request)
    }
}
impl LlmProvider for mc_provider::GeminiProvider {
    fn stream(&self, request: &CompletionRequest) -> ProviderStream {
        self.stream(request)
    }
}

/// Main orchestrator: manages conversation, tools, providers, and session state.
pub struct ConversationRuntime {
    pub session: Session,
    pub usage: UsageTracker,
    pub plan_mode: bool,
    system_prompt: String,
    pub(crate) model: String,
    hook_engine: Option<Arc<HookEngine>>,
    model_registry: ModelRegistry,
    subagent: SubagentSpawner,
    agents: Vec<crate::agents::AgentDef>,
    audit_log: Option<Arc<AuditLog>>,
    tool_registry: Arc<ToolRegistry>,
    token_budget: TokenBudget,
    retry_policy: RetryPolicy,
    memory: Option<MemoryStore>,
    pending_image: Option<(String, String)>, // (path, media_type)
    thinking_budget: Option<u32>,
    context_resolver: Option<ContextResolver>,
    repo_map: Option<RepoMap>,
    undo_manager: UndoManager,
    tool_cache: ToolCache,
    cost_tracker: Option<crate::cost::CostTracker>,
    task_manager: crate::tasks::TaskManager,
    hierarchical_instructions: Option<String>,
    /// Auto-test: command to run after write tools. If set, test failures are fed back to LLM.
    pub auto_test_cmd: Option<String>,
    /// Auto-commit: if true, auto git add+commit after write tools with LLM-generated message.
    pub auto_commit: bool,
}

impl ConversationRuntime {
    #[must_use]
    /// New.
    pub fn new(model: String, max_tokens: u32, system_prompt: String) -> Self {
        let subagent = SubagentSpawner::new(model.clone(), max_tokens);
        let audit_log = AuditLog::default_path().map(AuditLog::new).map(Arc::new);
        let model_registry = ModelRegistry::default();
        let context_window = model_registry.context_window(&model) as usize;
        let token_budget = TokenBudget::new(context_window, max_tokens as usize);
        Self {
            session: Session::default(),
            usage: UsageTracker::default(),
            plan_mode: false,
            system_prompt,
            model,
            hook_engine: None,
            model_registry,
            subagent,
            agents: Vec::new(),
            audit_log,
            tool_registry: Arc::new(ToolRegistry::new()),
            token_budget,
            retry_policy: RetryPolicy::default(),
            memory: None,
            pending_image: None,
            thinking_budget: None,
            context_resolver: None,
            repo_map: None,
            undo_manager: UndoManager::new(10),
            tool_cache: ToolCache::default(),
            cost_tracker: crate::cost::CostTracker::default_path()
                .map(crate::cost::CostTracker::new),
            task_manager: crate::tasks::TaskManager::new(),
            hierarchical_instructions: None,
            auto_test_cmd: None,
            auto_commit: false,
        }
    }

    /// Set hooks.
    pub fn set_hooks(&mut self, engine: HookEngine) {
        self.hook_engine = Some(Arc::new(engine));
    }
    /// Set tool registry.
    pub fn set_tool_registry(&mut self, registry: ToolRegistry) {
        self.tool_registry = Arc::new(registry);
    }
    /// Set `review_writes` flag on the tool registry.
    pub fn set_review_writes(&self, enabled: bool) {
        self.tool_registry
            .review_writes
            .store(enabled, std::sync::atomic::Ordering::Relaxed);
    }
    /// Set max concurrent subagents.
    pub fn set_max_concurrent_agents(&mut self, n: usize) {
        self.subagent.set_max_concurrent(n);
    }
    /// Set named agent definitions.
    pub fn set_agents(&mut self, agents: Vec<crate::agents::AgentDef>) {
        self.agents = agents;
    }
    /// Set subagent permission mode (inherit from parent).
    pub fn set_subagent_permission_mode(&mut self, mode: mc_tools::PermissionMode) {
        self.subagent.set_permission_mode(mode);
    }
    /// Set subagent budget.
    pub fn set_subagent_budget(&mut self, budget: Option<f64>) {
        self.subagent.set_budget(budget);
    }
    /// Set retry policy.
    pub fn set_retry_policy(&mut self, policy: RetryPolicy) {
        self.retry_policy = policy;
    }
    /// Set token budget.
    pub fn set_token_budget(&mut self, budget: TokenBudget) {
        self.token_budget = budget;
    }
    /// Set memory.
    pub fn set_memory(&mut self, memory: MemoryStore) {
        self.memory = Some(memory);
    }

    /// Attach an image to the next user message.
    pub fn attach_image(&mut self, path: String, media_type: String) {
        self.pending_image = Some((path, media_type));
    }

    /// Set thinking budget.
    pub fn set_thinking_budget(&mut self, budget: Option<u32>) {
        self.thinking_budget = budget;
    }
    /// Set context resolver.
    pub fn set_context_resolver(&mut self, resolver: ContextResolver) {
        self.context_resolver = Some(resolver);
    }

    /// Build and set repo map from workspace root.
    pub fn set_repo_map(&mut self, root: &std::path::Path) {
        let map = RepoMap::build(root);
        if map.file_count() > 0 {
            self.repo_map = Some(map);
        }
    }

    /// Set hierarchical instructions.
    pub fn set_hierarchical_instructions(&mut self, instructions: String) {
        self.hierarchical_instructions = Some(instructions);
    }

    /// Undo the last turn's file changes.
    pub fn undo_last_turn(&mut self) -> Result<Vec<String>, std::io::Error> {
        self.undo_manager.undo_last_turn()
    }

    #[must_use]
    /// Model.
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Switch model mid-session.
    pub fn set_model(&mut self, model: String) {
        self.model = model;
    }

    /// Ask LLM to generate a commit message from a diff.
    pub async fn generate_commit_message(&self, provider: &dyn LlmProvider, diff: &str) -> String {
        let truncated = if diff.len() > 4000 {
            &diff[..4000]
        } else {
            diff
        };
        let prompt = format!(
            "Generate a concise git commit message (conventional commits format) for this diff. \
             Reply with ONLY the commit message, no explanation.\n\n```diff\n{truncated}\n```"
        );
        let request = CompletionRequest {
            model: self.model.clone(),
            max_tokens: 100,
            system_prompt: None,
            messages: vec![InputMessage {
                role: MessageRole::User,
                content: vec![ContentBlock::Text { text: prompt }],
            }],
            tools: Vec::new(),
            tool_choice: None,
            thinking_budget: None,
            response_format: None,
        };
        let mut stream = provider.stream(&request);
        let mut msg = String::new();
        while let Some(Ok(event)) = crate::runtime::next_event(&mut stream).await {
            if let ProviderEvent::TextDelta(t) = event {
                msg.push_str(&t);
            }
        }
        let trimmed = msg.trim().trim_matches('"').trim_matches('`').trim();
        if trimmed.is_empty() {
            "chore: update files".into()
        } else {
            trimmed.to_string()
        }
    }

    /// Cumulative cost across all sessions from disk.
    #[must_use]
    /// Cumulative cost.
    pub fn cumulative_cost(&self) -> (u64, u64, f64) {
        self.cost_tracker
            .as_ref()
            .map_or((0, 0, 0.0), crate::cost::CostTracker::cumulative)
    }

    #[allow(clippy::too_many_lines)]
    /// Run turn.
    pub async fn run_turn(
        &mut self,
        provider: &dyn LlmProvider,
        user_input: &str,
        permission_policy: &PermissionPolicy,
        prompter: &mut Option<Box<dyn PermissionPrompter>>,
        on_event: &mut (dyn FnMut(&ProviderEvent) + Send),
        cancel: &CancellationToken,
    ) -> Result<TurnResult, ProviderError> {
        // Context window preflight: reject if session already exceeds context window
        let ctx_window = self.model_registry.context_window(&self.model) as usize;
        let estimated = crate::compact::estimate_tokens(&self.session);
        if estimated > ctx_window {
            tracing::warn!(
                "context window preflight: {estimated} tokens > {ctx_window} limit, auto-compacting"
            );
            let preserve = 4;
            if let Err(e) =
                crate::compact::smart_compact(provider, &mut self.session, &self.model, preserve)
                    .await
            {
                tracing::warn!("preflight compact failed: {e}");
                crate::compact::compact_session(&mut self.session, preserve);
            }
        }

        // Resolve @-mentions in user input
        let effective_input = if let Some(ref resolver) = self.context_resolver {
            let (cleaned, contexts) = resolver.resolve(user_input);
            if contexts.is_empty() {
                user_input.to_string()
            } else {
                ContextResolver::build_message(&cleaned, &contexts)
            }
        } else {
            user_input.to_string()
        };

        self.session
            .messages
            .push(ConversationMessage::user(&effective_input));

        // Attach pending image to the user message
        if let Some((path, media_type)) = self.pending_image.take() {
            if let Some(msg) = self.session.messages.last_mut() {
                msg.push_block(Block::Image {
                    source: crate::session::ImageSource::Path { path },
                    media_type,
                });
            }
        }

        let mut final_text = String::new();
        let mut tool_calls = Vec::new();
        let mut turn_usage = TokenUsage::default();
        let mut iterations = 0;
        let mut recent_patterns: Vec<(usize, Vec<String>)> = Vec::new();

        loop {
            if cancel.is_cancelled() {
                return Ok(Self::cancelled_result(
                    final_text, tool_calls, turn_usage, iterations,
                ));
            }
            iterations += 1;
            if iterations > MAX_ITERATIONS {
                break;
            }

            let (text_buf, thinking_buf, pending_tools) = self
                .stream_with_retry(provider, on_event, &mut turn_usage, cancel)
                .await?;

            // Check if cancelled during streaming
            if cancel.is_cancelled() {
                return Ok(Self::cancelled_result(
                    final_text, tool_calls, turn_usage, iterations,
                ));
            }

            self.store_response(&text_buf, &thinking_buf, &pending_tools, &mut final_text);

            if pending_tools.is_empty() {
                // Auto-continue: if output appears cut off by token limit
                let trimmed = text_buf.trim_end();
                let looks_cut_off = !trimmed.is_empty()
                    && trimmed.len() > 200
                    && !trimmed.ends_with('.')
                    && !trimmed.ends_with('!')
                    && !trimmed.ends_with('?')
                    && !trimmed.ends_with("```")
                    && !trimmed.ends_with('\n')
                    && !trimmed.ends_with('}')
                    && !trimmed.ends_with(')')
                    && !trimmed.ends_with(']')
                    && !trimmed.ends_with(';')
                    && iterations < MAX_ITERATIONS - 1
                    && iterations < 2; // only auto-continue once
                if looks_cut_off {
                    tracing::debug!("auto-continue: output appears truncated");
                    self.session
                        .messages
                        .push(ConversationMessage::user("continue"));
                    iterations += 1;
                    continue;
                }
                break;
            }

            // Diminishing returns detection
            let pattern = (
                text_buf.len(),
                pending_tools
                    .iter()
                    .map(|t| t.1.clone())
                    .collect::<Vec<_>>(),
            );
            recent_patterns.push(pattern);
            if recent_patterns.len() >= 3 {
                let last3 = &recent_patterns[recent_patterns.len() - 3..];
                if last3[0].1 == last3[1].1
                    && last3[1].1 == last3[2].1
                    && (last3[0].0.abs_diff(last3[2].0)) < 50
                {
                    tracing::warn!("diminishing returns detected after {iterations} iterations");
                    final_text.push_str("\n[Stopped: repeated pattern detected]");
                    break;
                }
            }

            // Split: subagent/memory tools run sequentially, rest run in parallel
            let mut sequential = Vec::new();
            let mut parallel = Vec::new();
            let review_writes = self
                .tool_registry
                .review_writes
                .load(std::sync::atomic::Ordering::Relaxed);
            for tool in pending_tools {
                if matches!(
                    tool.1.as_str(),
                    "subagent"
                        | "memory_read"
                        | "memory_write"
                        | "ask_user"
                        | "sleep"
                        | "edit_plan"
                        | "debug"
                        | "browser"
                ) || (review_writes
                    && matches!(
                        tool.1.as_str(),
                        "write_file" | "edit_file" | "batch_edit" | "apply_patch"
                    ))
                {
                    sequential.push(tool);
                } else {
                    // Snapshot for undo before write operations
                    if matches!(tool.1.as_str(), "write_file" | "edit_file") {
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&tool.2) {
                            if let Some(p) = v.get("path").and_then(|v| v.as_str()) {
                                self.undo_manager
                                    .snapshot_before_write(std::path::Path::new(p));
                            }
                        }
                    }
                    parallel.push(tool);
                }
            }

            // Execute parallel batch with streaming output
            let (tool_output_tx, mut tool_output_rx) =
                tokio::sync::mpsc::unbounded_channel::<String>();
            let batch_results = {
                let batch_fut = crate::parallel_tools::execute_batch(
                    parallel,
                    &self.tool_registry,
                    &self.hook_engine,
                    &self.audit_log,
                    permission_policy,
                    cancel,
                    4,
                    Some(&tool_output_tx),
                );
                tokio::pin!(batch_fut);
                let mut done = false;
                let mut results = Vec::new();
                loop {
                    tokio::select! {
                        biased;
                        chunk = tool_output_rx.recv(), if !done => {
                            if let Some(c) = chunk {
                                on_event(&ProviderEvent::ToolOutputDelta(c));
                            }
                        }
                        r = &mut batch_fut, if !done => {
                            results = r;
                            done = true;
                        }
                    }
                    if done {
                        break;
                    }
                }
                results
            };
            // batch_fut and tool_output_tx borrow are now dropped
            drop(tool_output_tx);
            while let Ok(c) = tool_output_rx.try_recv() {
                on_event(&ProviderEvent::ToolOutputDelta(c));
            }
            for r in batch_results {
                tool_calls.push(r.name.clone());
                self.session.messages.push(ConversationMessage::tool_result(
                    &r.id, &r.name, &r.output, r.is_error,
                ));
            }

            // Execute sequential tools (subagent, memory)
            for (id, name, input) in sequential {
                if cancel.is_cancelled() {
                    return Ok(Self::cancelled_result(
                        final_text, tool_calls, turn_usage, iterations,
                    ));
                }
                let (output, is_error) = self
                    .execute_tool(provider, &name, &input, permission_policy, prompter)
                    .await;
                tool_calls.push(name.clone());
                self.session.messages.push(ConversationMessage::tool_result(
                    &id, &name, &output, is_error,
                ));
            }

            // Auto-verify: quick syntax check after writes (always on, no config needed)
            {
                let had_writes = tool_calls.iter().any(|t| {
                    matches!(
                        t.as_str(),
                        "write_file" | "edit_file" | "batch_edit" | "apply_patch"
                    )
                });
                if had_writes {
                    let mut paths_to_check: Vec<String> = Vec::new();
                    if let Some(msg) = self.session.messages.iter().rev().next() {
                        for block in &msg.blocks {
                            if let Block::ToolUse {
                                name: tool_name,
                                input,
                                ..
                            } = block
                            {
                                if matches!(tool_name.as_str(), "write_file" | "edit_file") {
                                    if let Ok(val) =
                                        serde_json::from_str::<serde_json::Value>(input)
                                    {
                                        if let Some(path) = val
                                            .get("file_path")
                                            .or(val.get("path"))
                                            .and_then(|v| v.as_str())
                                        {
                                            paths_to_check.push(path.to_string());
                                        }
                                    }
                                }
                            }
                        }
                    }
                    for path in &paths_to_check {
                        let check = match path.rsplit('.').next() {
                            Some("py") => Some(format!("python3 -c \"import ast; ast.parse(open(\'{path}\').read())\" 2>&1")),
                            Some("json") => Some(format!("python3 -c \"import json; json.load(open(\'{path}\'))\" 2>&1")),
                            _ => None,
                        };
                        if let Some(cmd) = check {
                            if let Ok(out) = std::process::Command::new("sh")
                                .arg("-c")
                                .arg(&cmd)
                                .output()
                            {
                                if !out.status.success() {
                                    let err = String::from_utf8_lossy(&out.stderr);
                                    let stdout = String::from_utf8_lossy(&out.stdout);
                                    let verify_msg = format!(
                                        "⚠️ Syntax error in `{path}`:
```
{stdout}{err}
```
Fix this before continuing."
                                    );
                                    self.session
                                        .messages
                                        .push(ConversationMessage::user(&verify_msg));
                                    on_event(&ProviderEvent::ToolOutputDelta(format!(
                                        "⚠️ Syntax error in {path}
"
                                    )));
                                }
                            }
                        }
                    }
                }
            }

            // Auto-test: run tests after write tools, feed failures back to LLM
            if let Some(ref test_cmd) = self.auto_test_cmd {
                let had_writes = tool_calls.iter().any(|t| {
                    matches!(
                        t.as_str(),
                        "write_file" | "edit_file" | "batch_edit" | "apply_patch"
                    )
                });
                if had_writes {
                    on_event(&ProviderEvent::ToolOutputDelta(
                        "\n🧪 Running tests...\n".into(),
                    ));
                    if let Ok(output) = tokio::process::Command::new("sh")
                        .arg("-c")
                        .arg(test_cmd)
                        .output()
                        .await
                    {
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        if !output.status.success() {
                            let fail_msg = format!(
                                "Tests failed after code changes. Fix the errors:\n```\n{}{}\n```",
                                &stdout[..stdout.len().min(2000)],
                                if stderr.is_empty() {
                                    String::new()
                                } else {
                                    format!("\nSTDERR:\n{}", &stderr[..stderr.len().min(500)])
                                }
                            );
                            on_event(&ProviderEvent::ToolOutputDelta("❌ Tests failed\n".into()));
                            self.session
                                .messages
                                .push(ConversationMessage::user(&fail_msg));
                            // Continue the loop — LLM will see the failure and try to fix
                            continue;
                        }
                        on_event(&ProviderEvent::ToolOutputDelta("✅ Tests passed\n".into()));
                    }
                }
            }

            // Auto-commit: stage and commit with LLM-generated message
            if self.auto_commit {
                let had_writes = tool_calls.iter().any(|t| {
                    matches!(
                        t.as_str(),
                        "write_file" | "edit_file" | "batch_edit" | "apply_patch"
                    )
                });
                if had_writes {
                    let _ = std::process::Command::new("git")
                        .args(["add", "-A"])
                        .output();
                    if let Ok(diff) = std::process::Command::new("git")
                        .args(["diff", "--cached", "--stat"])
                        .output()
                    {
                        let stat = String::from_utf8_lossy(&diff.stdout);
                        if !stat.trim().is_empty() {
                            on_event(&ProviderEvent::ToolOutputDelta(
                                "📦 Auto-committing...\n".into(),
                            ));
                            let msg = self.generate_commit_message(provider, &stat).await;
                            match std::process::Command::new("git")
                                .args(["commit", "-m", &msg])
                                .output()
                            {
                                Ok(o) => {
                                    let out = String::from_utf8_lossy(&o.stdout);
                                    on_event(&ProviderEvent::ToolOutputDelta(format!(
                                        "✓ {}\n",
                                        out.trim()
                                    )));
                                }
                                Err(e) => on_event(&ProviderEvent::ToolOutputDelta(format!(
                                    "commit error: {e}\n"
                                ))),
                            }
                        }
                    }
                }
            }
        }

        self.maybe_compact(provider).await;
        self.undo_manager.end_turn();
        // Persist cost
        if let Some(ref mut tracker) = self.cost_tracker {
            let cost = self.model_registry.estimate_cost(
                &self.model,
                turn_usage.input_tokens,
                turn_usage.output_tokens,
            );
            tracker.record(
                &self.model,
                turn_usage.input_tokens,
                turn_usage.output_tokens,
                cost,
            );
        }
        // Auto-memory: save useful facts from the response
        self.auto_save_memory(&final_text);

        // Auto-skill: create skill from complex successful turns
        let had_errors = tool_calls.iter().any(|t| t.contains("error"));
        if crate::auto_skill::should_create_skill(tool_calls.len(), had_errors) {
            let skills_dir = std::env::var_os("HOME")
                .map(|h| std::path::PathBuf::from(h).join(".config/magic-code/skills"));
            if let Some(dir) = skills_dir {
                let summary = final_text.chars().take(200).collect::<String>();
                let content = crate::auto_skill::generate_skill_content(&summary, &tool_calls);
                if let Some(path) = crate::auto_skill::save_auto_skill(
                    &dir,
                    &format!(
                        "auto-{iterations}t-{}-{}",
                        tool_calls.len(),
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map_or(0, |d| d.as_secs())
                    ),
                    &content,
                ) {
                    on_event(&ProviderEvent::TextDelta(format!(
                        "\n💡 Auto-skill saved: {path}\n"
                    )));
                }
            }
        }

        Ok(TurnResult {
            text: final_text,
            tool_calls,
            usage: turn_usage,
            iterations,
            cancelled: false,
        })
    }

    /// Stream with retry on mid-stream failures.
    async fn stream_with_retry(
        &mut self,
        provider: &dyn LlmProvider,
        on_event: &mut (dyn FnMut(&ProviderEvent) + Send),
        turn_usage: &mut TokenUsage,
        cancel: &CancellationToken,
    ) -> Result<(String, String, Vec<(String, String, String)>), ProviderError> {
        let mut last_error = None;
        for attempt in 0..=self.retry_policy.max_attempts {
            match self
                .stream_response(provider, on_event, turn_usage, cancel)
                .await
            {
                Ok(result) => return Ok(result),
                Err(e) if self.retry_policy.should_retry(&e, attempt) => {
                    let reason = e.to_string();
                    let backoff = self.retry_policy.backoff_duration(attempt);
                    on_event(&ProviderEvent::StreamReset);
                    on_event(&ProviderEvent::RetryAttempt {
                        attempt: attempt + 1,
                        max: self.retry_policy.max_attempts,
                        reason,
                    });
                    tokio::select! {
                        () = cancel.cancelled() => return Err(e),
                        () = tokio::time::sleep(backoff) => {}
                    }
                    last_error = Some(e);
                }
                Err(e) => return Err(e),
            }
        }
        Err(
            last_error.unwrap_or_else(|| ProviderError::RetriesExhausted {
                attempts: self.retry_policy.max_attempts,
                last_message: "unknown".into(),
            }),
        )
    }

    /// Stream one LLM response, collecting text and tool calls.
    async fn stream_response(
        &mut self,
        provider: &dyn LlmProvider,
        on_event: &mut (dyn FnMut(&ProviderEvent) + Send),
        turn_usage: &mut TokenUsage,
        cancel: &CancellationToken,
    ) -> Result<(String, String, Vec<(String, String, String)>), ProviderError> {
        let request = self.build_request();
        let mut stream = provider.stream(&request);
        let mut text_buf = String::new();
        let mut thinking_buf = String::new();
        let mut pending_tools: Vec<(String, String, String)> = Vec::new();

        loop {
            let next = tokio::select! {
                () = cancel.cancelled() => return Ok((text_buf, thinking_buf, pending_tools)),
                item = next_event(&mut stream) => item,
            };
            match next {
                Some(Ok(event)) => {
                    on_event(&event);
                    match &event {
                        ProviderEvent::TextDelta(t) => text_buf.push_str(t),
                        ProviderEvent::ThinkingDelta(t) => thinking_buf.push_str(t),
                        ProviderEvent::ToolUse { id, name, input } => {
                            pending_tools.push((id.clone(), name.clone(), input.clone()));
                        }
                        ProviderEvent::Usage(u) => {
                            *turn_usage = u.clone();
                            self.usage.record(u);
                            self.session.input_tokens += u.input_tokens;
                            self.session.output_tokens += u.output_tokens;
                        }
                        ProviderEvent::MessageStop
                        | ProviderEvent::RetryAttempt { .. }
                        | ProviderEvent::StreamReset
                        | ProviderEvent::ToolOutputDelta(_) => {}
                        evt @ ProviderEvent::ToolInputDelta { .. } => {
                            on_event(&evt);
                        }
                    }
                }
                Some(Err(e)) => return Err(e),
                None => break,
            }
        }
        Ok((text_buf, thinking_buf, pending_tools))
    }

    /// Store assistant text + thinking + `tool_use` blocks as a single message.
    fn store_response(
        &mut self,
        text_buf: &str,
        thinking_buf: &str,
        pending_tools: &[(String, String, String)],
        final_text: &mut String,
    ) {
        if text_buf.is_empty() && thinking_buf.is_empty() && pending_tools.is_empty() {
            return;
        }

        let mut msg = if text_buf.is_empty() {
            let mut m = ConversationMessage::assistant(String::new());
            m.blocks.clear();
            m
        } else {
            final_text.push_str(text_buf);
            ConversationMessage::assistant(text_buf)
        };

        if !thinking_buf.is_empty() {
            msg.blocks.insert(
                0,
                Block::Thinking {
                    text: thinking_buf.to_string(),
                },
            );
        }

        for (id, name, input) in pending_tools {
            msg.push_block(Block::ToolUse {
                id: id.clone(),
                name: name.clone(),
                input: input.clone(),
            });
        }

        if !msg.blocks.is_empty() {
            self.session.messages.push(msg);
        }
    }

    /// Execute a single tool: hooks → permission → dispatch → audit.
    async fn execute_tool(
        &mut self,
        provider: &dyn LlmProvider,
        name: &str,
        input: &str,
        policy: &PermissionPolicy,
        prompter: &mut Option<Box<dyn PermissionPrompter>>,
    ) -> (String, bool) {
        // Pre-tool hook
        if let Some(ref engine) = self.hook_engine {
            if let Err(e) = engine.fire(&HookEvent::PreToolCall, Some(name), &[("tool_name", name)])
            {
                return (e.to_string(), true);
            }
        }

        let outcome = {
            let is_reviewed = self
                .tool_registry
                .review_writes
                .load(std::sync::atomic::Ordering::Relaxed)
                && matches!(
                    name,
                    "write_file" | "edit_file" | "batch_edit" | "apply_patch"
                );
            if is_reviewed {
                let diff_summary = crate::parallel_tools::diff_preview_summary(name, input);
                match prompter {
                    Some(ref mut p) => p.decide(&mc_tools::PermissionRequest {
                        tool_name: name.to_string(),
                        input_summary: diff_summary,
                    }),
                    None => PermissionOutcome::Deny {
                        reason: "diff preview requires interactive mode".into(),
                    },
                }
            } else {
                match prompter {
                    Some(ref mut p) => policy.authorize(name, input, Some(&mut **p)),
                    None => policy.authorize(name, input, None),
                }
            }
        };
        let timer = AuditLog::start_timer();

        let result = match outcome {
            PermissionOutcome::Allow => {
                let r = self.dispatch_tool(provider, name, input).await;
                self.log_audit(name, input, r.0.len(), r.1, timer, true);
                r
            }
            PermissionOutcome::Deny { reason } => {
                self.log_audit(name, input, 0, true, timer, false);
                (reason, true)
            }
        };

        // Post-tool hook
        if let Some(ref engine) = self.hook_engine {
            let _ = engine.fire(&HookEvent::PostToolCall, Some(name), &[("tool_name", name)]);
        }

        result
    }

    /// Dispatch to memory, subagent, or tool registry.
    async fn dispatch_tool(
        &mut self,
        provider: &dyn LlmProvider,
        name: &str,
        input: &str,
    ) -> (String, bool) {
        let input_val: serde_json::Value =
            serde_json::from_str(input).unwrap_or_else(|_| serde_json::json!({"raw": input}));

        if name == "memory_read" {
            return match self.memory {
                Some(ref store) => (store.handle_read(&input_val), false),
                None => ("Memory not configured".into(), true),
            };
        }
        if name == "memory_write" {
            return match self.memory {
                Some(ref mut store) => {
                    let out = store.handle_write(&input_val);
                    let _ = store.save();
                    (out, false)
                }
                None => ("Memory not configured".into(), true),
            };
        }

        if name == "task_create" {
            let desc = input_val
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let cmd = input_val
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let id = self.task_manager.create(desc, cmd).await;
            return (format!("Task created: {id}"), false);
        }
        if name == "task_get" {
            let id = input_val
                .get("task_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            return match self.task_manager.get(id).await {
                Some(t) => (serde_json::json!({"id": t.id, "status": format!("{:?}", t.status), "output": t.output, "exit_code": t.exit_code}).to_string(), false),
                None => (format!("Task not found: {id}"), true),
            };
        }
        if name == "task_list" {
            let tasks = self.task_manager.list().await;
            let list: Vec<serde_json::Value> = tasks.iter().map(|t| serde_json::json!({"id": t.id, "description": t.description, "status": format!("{:?}", t.status)})).collect();
            return (
                serde_json::to_string(&list).unwrap_or_else(|_| "[]".into()),
                false,
            );
        }
        if name == "task_stop" {
            let id = input_val
                .get("task_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            return if self.task_manager.stop(id).await {
                (format!("Task {id} stopped"), false)
            } else {
                (format!("Task {id} not found or not running"), true)
            };
        }

        if name == "edit_plan" {
            let title = input_val
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("Edit Plan");
            let steps = input_val.get("steps").and_then(|v| v.as_array());
            if let Some(steps) = steps {
                let mut plan = format!("📋 **{title}** ({} files)\n\n", steps.len());
                for (i, step) in steps.iter().enumerate() {
                    let file = step.get("file").and_then(|v| v.as_str()).unwrap_or("?");
                    let action = step
                        .get("action")
                        .and_then(|v| v.as_str())
                        .unwrap_or("edit");
                    let desc = step
                        .get("description")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let icon = match action {
                        "create" => "✨",
                        "delete" => "🗑️",
                        _ => "✏️",
                    };
                    plan.push_str(&format!(
                        "{}. {icon} **{action}** `{file}`\n   {desc}\n\n",
                        i + 1
                    ));
                }
                plan.push_str("Proceed with this plan? (The agent will now execute each step)");
                return (plan, false);
            }
            return ("Invalid plan format: steps array required".into(), true);
        }

        if name == "codebase_search" {
            let query = input_val
                .get("query")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let max = input_val
                .get("max_results")
                .and_then(|v| v.as_u64())
                .unwrap_or(10) as usize;
            if let Some(ref map) = self.repo_map {
                let results = map.search(query, max);
                if results.is_empty() {
                    return ("No matching files or symbols found.".into(), false);
                }
                let out: Vec<String> = results
                    .iter()
                    .map(|r| {
                        if r.symbols.is_empty() {
                            format!("📄 {} (score: {:.1})", r.path, r.score)
                        } else {
                            format!(
                                "📄 {} (score: {:.1})\n   {}",
                                r.path,
                                r.score,
                                r.symbols.join(", ")
                            )
                        }
                    })
                    .collect();
                return (out.join("\n"), false);
            }
            return (
                "Repo map not initialized. Run from a project directory.".into(),
                true,
            );
        }

        if name == "todo_write" {
            let todos = input_val.get("todos").and_then(|v| v.as_array());
            if let Some(items) = todos {
                let summary: Vec<String> = items
                    .iter()
                    .filter_map(|t| {
                        let status = t
                            .get("status")
                            .and_then(|s| s.as_str())
                            .unwrap_or("pending");
                        let content = t.get("content").and_then(|s| s.as_str()).unwrap_or("");
                        let icon = match status {
                            "completed" => "✓",
                            "in_progress" => "◐",
                            _ => "○",
                        };
                        Some(format!("{icon} {content}"))
                    })
                    .collect();
                return (
                    format!(
                        "TODO list updated ({} items):\n{}",
                        items.len(),
                        summary.join("\n")
                    ),
                    false,
                );
            }
            return ("Invalid todos format".into(), true);
        }

        if name == "worktree_enter" {
            let branch = input_val
                .get("branch")
                .and_then(|v| v.as_str())
                .unwrap_or("temp");
            let wt_path = format!(".worktrees/{branch}");
            let output = tokio::process::Command::new("git")
                .args(["worktree", "add", &wt_path, "-b", branch])
                .output()
                .await;
            return match output {
                Ok(o) if o.status.success() => {
                    let abs = std::fs::canonicalize(&wt_path)
                        .unwrap_or_else(|_| std::path::PathBuf::from(&wt_path));
                    (format!("Worktree created at {}", abs.display()), false)
                }
                Ok(o) => {
                    // Branch might already exist, try without -b
                    let output2 = tokio::process::Command::new("git")
                        .args(["worktree", "add", &wt_path, branch])
                        .output()
                        .await;
                    match output2 {
                        Ok(o2) if o2.status.success() => {
                            let abs = std::fs::canonicalize(&wt_path)
                                .unwrap_or_else(|_| std::path::PathBuf::from(&wt_path));
                            (format!("Worktree created at {}", abs.display()), false)
                        }
                        _ => (String::from_utf8_lossy(&o.stderr).to_string(), true),
                    }
                }
                Err(e) => (format!("git worktree: {e}"), true),
            };
        }
        if name == "worktree_exit" {
            // Find and remove worktrees
            let output = tokio::process::Command::new("git")
                .args(["worktree", "list", "--porcelain"])
                .output()
                .await;
            if let Ok(o) = output {
                let text = String::from_utf8_lossy(&o.stdout);
                let worktrees: Vec<&str> = text
                    .lines()
                    .filter(|l| l.starts_with("worktree ") && l.contains(".worktrees/"))
                    .filter_map(|l| l.strip_prefix("worktree "))
                    .collect();
                for wt in &worktrees {
                    let _ = tokio::process::Command::new("git")
                        .args(["worktree", "remove", "--force", wt])
                        .output()
                        .await;
                }
                return (format!("Removed {} worktree(s)", worktrees.len()), false);
            }
            return ("No worktrees found".into(), false);
        }

        if name == "batch_edit" {
            // Enforce read-before-write for batch edits
            if let Some(edits) = input_val.get("edits").and_then(|v| v.as_array()) {
                for edit in edits {
                    if let Some(p) = edit.get("path").and_then(|v| v.as_str()) {
                        self.undo_manager
                            .snapshot_before_write(std::path::Path::new(p));
                    }
                }
            }
            // Fall through to tool_registry for actual execution
        }

        // Snapshot for undo before write operations
        if matches!(name, "write_file" | "edit_file") {
            if let Some(p) = input_val.get("path").and_then(|v| v.as_str()) {
                self.undo_manager
                    .snapshot_before_write(std::path::Path::new(p));
            }
        }

        // Check cache for read-only tools
        if let Some(cached) = self.tool_cache.get(name, &input_val) {
            return (cached.to_string(), false);
        }

        if name == "subagent" {
            let task = input_val.get("task").and_then(|v| v.as_str()).unwrap_or("");
            let context = input_val
                .get("context")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let agent_name = input_val.get("agent_name").and_then(|v| v.as_str());
            let agent_def = agent_name.and_then(|n| self.agents.iter().find(|a| a.name == n));
            let model_override = input_val
                .get("model")
                .and_then(|v| v.as_str())
                .map(String::from)
                .or_else(|| agent_def.and_then(|a| a.model.clone()));
            let allowed_tools: Option<Vec<String>> = input_val
                .get("tools")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .or_else(|| {
                    agent_def
                        .filter(|a| !a.allowed_tools.is_empty())
                        .map(|a| a.allowed_tools.clone())
                });
            let max_turns = input_val
                .get("max_turns")
                .and_then(serde_json::Value::as_u64)
                .map(|v| v as usize);
            // Build prompt: agent instructions + task + context
            let mut sub_prompt = String::new();
            if let Some(def) = agent_def {
                if !def.instructions.is_empty() {
                    sub_prompt.push_str(&def.instructions);
                    sub_prompt.push_str("\n\n");
                }
            }
            sub_prompt.push_str(task);
            if !context.is_empty() {
                sub_prompt.push_str("\n\nContext:\n");
                sub_prompt.push_str(context);
            }
            // Poll background agent
            if let Some(agent_id) = input_val.get("poll_agent_id").and_then(|v| v.as_str()) {
                return match self.subagent.poll_background(agent_id) {
                    Some(Some(result)) => (result, false),
                    Some(None) => ("Agent completed with no output".into(), false),
                    None => (
                        format!("{{\"agent_id\":\"{agent_id}\",\"status\":\"running\"}}"),
                        false,
                    ),
                };
            }
            match self
                .subagent
                .run_task(
                    provider,
                    &sub_prompt,
                    &self.system_prompt,
                    &self.tool_registry,
                    model_override.as_deref(),
                    allowed_tools.as_deref(),
                    max_turns,
                )
                .await
            {
                Ok(out) => (out, false),
                Err(e) => (e.to_string(), true),
            }
        } else if name == "browser" {
            return execute_browser(&input_val).await;
        } else if name == "debug" {
            return crate::debug::execute_debug(&input_val);
        } else {
            let result = match self.tool_registry.execute(name, &input_val).await {
                Ok(out) => (out, false),
                Err(e) => (e.to_string(), true),
            };
            // Cache read-only results, invalidate on writes
            if !result.1 {
                self.tool_cache.put(name, &input_val, result.0.clone());
            }
            if matches!(name, "write_file" | "edit_file" | "bash") {
                self.tool_cache.invalidate_all();
            }
            result
        }
    }

    fn log_audit(
        &self,
        tool: &str,
        input: &str,
        output_len: usize,
        is_error: bool,
        timer: std::time::Instant,
        allowed: bool,
    ) {
        if let Some(ref log) = self.audit_log {
            let entry = AuditEntry {
                tool: tool.to_string(),
                input_summary: input.to_string(),
                output_len,
                is_error,
                duration_ms: timer.elapsed().as_millis() as u64,
                allowed,
            };
            // Non-blocking: audit should never slow down tool execution
            // Audit: single-line append, negligible I/O — acceptable on async path
            log.log(&entry);
        }
    }

    /// Auto-compact if approaching context window limit.
    /// Auto-save useful facts from LLM response to memory.
    fn auto_save_memory(&mut self, text: &str) {
        let Some(ref mut memory) = self.memory else {
            return;
        };
        // Heuristic: save lines that look like project facts
        for line in text.lines() {
            let trimmed = line.trim();
            if (trimmed.starts_with("Note:")
                || trimmed.starts_with("Remember:")
                || trimmed.contains("convention is"))
                && trimmed.len() > 20
                && trimmed.len() < 200
            {
                let key = format!(
                    "auto_{}_{}",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis(),
                    trimmed.len(),
                );
                memory.set(&key, trimmed);
                let _ = memory.save();
            }
        }
    }

    async fn maybe_compact(&mut self, provider: &dyn LlmProvider) {
        let ctx_window = self.model_registry.context_window(&self.model) as usize;
        let est = crate::compact::estimate_tokens(&self.session);
        let usage_pct = (est * 100) / ctx_window.max(1);

        if usage_pct > 90 {
            // Full smart compact
            let preserve = 4;
            if let Err(e) =
                crate::compact::smart_compact(provider, &mut self.session, &self.model, preserve)
                    .await
            {
                tracing::warn!("smart compaction failed, using naive: {e}");
                crate::compact::compact_session(&mut self.session, preserve);
            }
        } else if usage_pct > 80 {
            // Collapse reads + snip thinking
            crate::compact::collapse_reads(&mut self.session);
            crate::compact::snip_thinking(&mut self.session, 6);
        } else if usage_pct > 60 {
            // Micro-compact only
            crate::compact::micro_compact(&mut self.session);
        }
    }

    fn cancelled_result(
        text: String,
        tool_calls: Vec<String>,
        usage: TokenUsage,
        iterations: usize,
    ) -> TurnResult {
        TurnResult {
            text,
            tool_calls,
            usage,
            iterations,
            cancelled: true,
        }
    }

    #[allow(clippy::too_many_lines)]
    fn build_request(&self) -> CompletionRequest {
        let mut messages: Vec<InputMessage> = Vec::new();

        for msg in &self.session.messages {
            let role = match msg.role {
                Role::User => MessageRole::User,
                Role::Assistant => MessageRole::Assistant,
                Role::Tool => MessageRole::Tool,
            };
            let content: Vec<ContentBlock> = msg
                .blocks
                .iter()
                .filter_map(|b| match b {
                    Block::Text { text } => Some(ContentBlock::Text { text: text.clone() }),
                    Block::ToolUse { id, name, input } => Some(ContentBlock::ToolUse {
                        id: id.clone(),
                        name: name.clone(),
                        input: input.clone(),
                    }),
                    Block::ToolResult {
                        tool_use_id,
                        output,
                        is_error,
                        ..
                    } => Some(ContentBlock::ToolResult {
                        tool_use_id: tool_use_id.clone(),
                        output: output.clone(),
                        is_error: *is_error,
                    }),
                    Block::Image { source, media_type } => {
                        let data = match source {
                            crate::session::ImageSource::Base64 { data } => data.clone(),
                            crate::session::ImageSource::Path { path } => {
                                resolve_image_base64(path).unwrap_or_default()
                            }
                        };
                        if data.is_empty() {
                            None
                        } else {
                            Some(ContentBlock::Image {
                                data,
                                media_type: media_type.clone(),
                            })
                        }
                    }
                    Block::Thinking { text } => Some(ContentBlock::Thinking { text: text.clone() }),
                })
                .collect();

            if let Some(last) = messages.last_mut() {
                if last.role == role && role == MessageRole::Tool {
                    last.content.extend(content);
                    continue;
                }
            }
            messages.push(InputMessage { role, content });
        }

        // Estimate used context and compute dynamic max_tokens
        let system_tokens = self.system_prompt.len().div_ceil(4);
        let tool_schema_tokens: usize = self
            .tool_registry
            .all_specs()
            .iter()
            .map(|s| s.input_schema.to_string().len().div_ceil(4) + s.description.len().div_ceil(4))
            .sum();
        let session_tokens = TokenBudget::session_tokens(&self.session);
        let used_context = system_tokens + tool_schema_tokens + session_tokens;
        let effective_max = self.token_budget.effective_max_tokens(used_context);

        let available = self
            .token_budget
            .available_for_messages(system_tokens, tool_schema_tokens);
        if session_tokens > 0 && available > 0 && session_tokens * 10 > available * 9 {
            tracing::warn!(
                session_tokens,
                available,
                "context pressure: history using >90% of available space"
            );
        }

        let (tools, tool_choice, system) = if self.plan_mode {
            (Vec::new(), None, format!(
                "{}{}{}\n\nYou are in PLAN MODE. Describe step-by-step what you would do. Do NOT use any tools.",
                self.system_prompt,
                self.hierarchical_instructions.as_deref().unwrap_or(""),
                self.memory.as_ref().map_or(String::new(), MemoryStore::to_prompt_section),
            ))
        } else {
            let memory_section = self
                .memory
                .as_ref()
                .map_or(String::new(), MemoryStore::to_prompt_section);
            let tools: Vec<ToolDefinition> = self
                .tool_registry
                .all_specs()
                .iter()
                .map(|s| ToolDefinition {
                    name: s.name.clone(),
                    description: s.description.clone(),
                    input_schema: s.input_schema.clone(),
                })
                .collect();
            (
                tools,
                Some(ToolChoice::Auto),
                format!(
                    "{}{}{}{}",
                    self.system_prompt,
                    self.hierarchical_instructions.as_deref().unwrap_or(""),
                    memory_section,
                    self.repo_map
                        .as_ref()
                        .map_or(String::new(), |m| m.to_prompt_section())
                ),
            )
        };

        CompletionRequest {
            model: self.model.clone(),
            max_tokens: effective_max,
            system_prompt: Some(system),
            messages,
            tools,
            tool_choice,
            thinking_budget: self.thinking_budget,
            response_format: None,
        }
    }
}

/// Next event.
pub async fn next_event(
    stream: &mut Pin<Box<dyn Stream<Item = Result<ProviderEvent, ProviderError>> + Send>>,
) -> Option<Result<ProviderEvent, ProviderError>> {
    use std::future::poll_fn;
    use std::task::Poll;
    poll_fn(|cx| match stream.as_mut().poll_next(cx) {
        Poll::Ready(item) => Poll::Ready(item),
        Poll::Pending => Poll::Pending,
    })
    .await
}

fn resolve_image_base64(path: &str) -> Option<String> {
    const MAX_IMAGE_SIZE: u64 = 10 * 1024 * 1024; // 10MB
    let metadata = std::fs::metadata(path).ok()?;
    if metadata.len() > MAX_IMAGE_SIZE {
        return None;
    }
    let buf = std::fs::read(path).ok()?;
    Some(base64_encode(&buf))
}

fn base64_encode(data: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(data)
}

/// Execute browser automation via playwright CLI.
async fn execute_browser(input: &serde_json::Value) -> (String, bool) {
    let action = input.get("action").and_then(|v| v.as_str()).unwrap_or("");

    // Build a playwright script based on action
    let script = match action {
        "navigate" => {
            let url = input.get("url").and_then(|v| v.as_str()).unwrap_or("");
            if url.is_empty() {
                return ("url is required for navigate".into(), true);
            }
            format!(
                r#"
const {{ chromium }} = require('playwright');
(async () => {{
    const b = await chromium.launch({{ headless: true }});
    const p = await b.newPage();
    await p.goto('{url}', {{ waitUntil: 'domcontentloaded', timeout: 15000 }});
    const title = await p.title();
    const text = await p.innerText('body').catch(() => '');
    const truncated = text.substring(0, 5000);
    await p.screenshot({{ path: '/tmp/mc_browser.png', fullPage: false }});
    console.log(JSON.stringify({{ title, text: truncated, screenshot: '/tmp/mc_browser.png' }}));
    await b.close();
}})();
"#,
                url = url.replace('\'', "\\'")
            )
        }
        "screenshot" => {
            let url = input.get("url").and_then(|v| v.as_str()).unwrap_or("");
            if url.is_empty() {
                return ("url is required for screenshot".into(), true);
            }
            format!(
                r#"
const {{ chromium }} = require('playwright');
(async () => {{
    const b = await chromium.launch({{ headless: true }});
    const p = await b.newPage();
    await p.goto('{url}', {{ waitUntil: 'domcontentloaded', timeout: 15000 }});
    await p.screenshot({{ path: '/tmp/mc_browser.png', fullPage: true }});
    console.log(JSON.stringify({{ screenshot: '/tmp/mc_browser.png' }}));
    await b.close();
}})();
"#,
                url = url.replace('\'', "\\'")
            )
        }
        "click" => {
            let url = input.get("url").and_then(|v| v.as_str()).unwrap_or("");
            let sel = input.get("selector").and_then(|v| v.as_str()).unwrap_or("");
            if sel.is_empty() {
                return ("selector is required for click".into(), true);
            }
            format!(
                r#"
const {{ chromium }} = require('playwright');
(async () => {{
    const b = await chromium.launch({{ headless: true }});
    const p = await b.newPage();
    if ('{url}') await p.goto('{url}', {{ waitUntil: 'domcontentloaded', timeout: 15000 }});
    await p.click('{sel}', {{ timeout: 5000 }});
    await p.waitForTimeout(1000);
    const text = await p.innerText('body').catch(() => '');
    await p.screenshot({{ path: '/tmp/mc_browser.png' }});
    console.log(JSON.stringify({{ clicked: '{sel}', text: text.substring(0, 3000), screenshot: '/tmp/mc_browser.png' }}));
    await b.close();
}})();
"#,
                url = url.replace('\'', "\\'"),
                sel = sel.replace('\'', "\\'")
            )
        }
        "type" => {
            let url = input.get("url").and_then(|v| v.as_str()).unwrap_or("");
            let sel = input.get("selector").and_then(|v| v.as_str()).unwrap_or("");
            let text = input.get("text").and_then(|v| v.as_str()).unwrap_or("");
            if sel.is_empty() {
                return ("selector is required for type".into(), true);
            }
            format!(
                r#"
const {{ chromium }} = require('playwright');
(async () => {{
    const b = await chromium.launch({{ headless: true }});
    const p = await b.newPage();
    if ('{url}') await p.goto('{url}', {{ waitUntil: 'domcontentloaded', timeout: 15000 }});
    await p.fill('{sel}', '{text}');
    await p.screenshot({{ path: '/tmp/mc_browser.png' }});
    console.log(JSON.stringify({{ filled: '{sel}', screenshot: '/tmp/mc_browser.png' }}));
    await b.close();
}})();
"#,
                url = url.replace('\'', "\\'"),
                sel = sel.replace('\'', "\\'"),
                text = text.replace('\'', "\\'")
            )
        }
        "evaluate" => {
            let url = input.get("url").and_then(|v| v.as_str()).unwrap_or("");
            let js = input.get("script").and_then(|v| v.as_str()).unwrap_or("");
            if js.is_empty() {
                return ("script is required for evaluate".into(), true);
            }
            format!(
                r#"
const {{ chromium }} = require('playwright');
(async () => {{
    const b = await chromium.launch({{ headless: true }});
    const p = await b.newPage();
    if ('{url}') await p.goto('{url}', {{ waitUntil: 'domcontentloaded', timeout: 15000 }});
    const result = await p.evaluate(() => {{ {js} }});
    console.log(JSON.stringify({{ result }}));
    await b.close();
}})();
"#,
                url = url.replace('\'', "\\'"),
                js = js
            )
        }
        _ => return (format!("Unknown browser action: {action}"), true),
    };

    // Write script to temp file and execute
    let script_path = "/tmp/mc_browser_script.js";
    if let Err(e) = tokio::fs::write(script_path, &script).await {
        return (format!("Failed to write script: {e}"), true);
    }

    let output = tokio::process::Command::new("node")
        .arg(script_path)
        .output()
        .await;

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            if out.status.success() {
                (stdout, false)
            } else if stderr.contains("Cannot find module 'playwright'") {
                ("Playwright not installed. Run: npm i -g playwright && npx playwright install chromium".into(), true)
            } else {
                (format!("Browser error: {stderr}"), true)
            }
        }
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                (
                    "Node.js not found. Install Node.js to use browser tool.".into(),
                    true,
                )
            } else {
                (format!("Failed to run browser: {e}"), true)
            }
        }
    }
}
