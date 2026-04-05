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
pub struct TurnResult {
    pub text: String,
    pub tool_calls: Vec<String>,
    pub usage: TokenUsage,
    pub iterations: usize,
    pub cancelled: bool,
}

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

pub struct ConversationRuntime {
    pub session: Session,
    pub usage: UsageTracker,
    pub plan_mode: bool,
    system_prompt: String,
    pub(crate) model: String,
    hook_engine: Option<Arc<HookEngine>>,
    model_registry: ModelRegistry,
    subagent: SubagentSpawner,
    audit_log: Option<Arc<AuditLog>>,
    tool_registry: Arc<ToolRegistry>,
    token_budget: TokenBudget,
    retry_policy: RetryPolicy,
    memory: Option<MemoryStore>,
    pending_image: Option<(String, String)>, // (path, media_type)
    thinking_budget: Option<u32>,
    context_resolver: Option<ContextResolver>,
    repo_map: Option<String>,
    undo_manager: UndoManager,
    tool_cache: ToolCache,
}

impl ConversationRuntime {
    #[must_use]
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
        }
    }

    pub fn set_hooks(&mut self, engine: HookEngine) {
        self.hook_engine = Some(Arc::new(engine));
    }
    pub fn set_tool_registry(&mut self, registry: ToolRegistry) {
        self.tool_registry = Arc::new(registry);
    }
    pub fn set_retry_policy(&mut self, policy: RetryPolicy) {
        self.retry_policy = policy;
    }
    pub fn set_token_budget(&mut self, budget: TokenBudget) {
        self.token_budget = budget;
    }
    pub fn set_memory(&mut self, memory: MemoryStore) {
        self.memory = Some(memory);
    }

    /// Attach an image to the next user message.
    pub fn attach_image(&mut self, path: String, media_type: String) {
        self.pending_image = Some((path, media_type));
    }

    pub fn set_thinking_budget(&mut self, budget: Option<u32>) {
        self.thinking_budget = budget;
    }
    pub fn set_context_resolver(&mut self, resolver: ContextResolver) {
        self.context_resolver = Some(resolver);
    }

    /// Build and set repo map from workspace root.
    pub fn set_repo_map(&mut self, root: &std::path::Path) {
        let map = RepoMap::build(root);
        if map.file_count() > 0 {
            self.repo_map = Some(map.to_prompt_section());
        }
    }

    /// Undo the last turn's file changes.
    pub fn undo_last_turn(&mut self) -> Result<Vec<String>, std::io::Error> {
        self.undo_manager.undo_last_turn()
    }

    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    #[allow(clippy::too_many_lines)]
    pub async fn run_turn(
        &mut self,
        provider: &dyn LlmProvider,
        user_input: &str,
        permission_policy: &PermissionPolicy,
        prompter: &mut Option<Box<dyn PermissionPrompter>>,
        on_event: &mut (dyn FnMut(&ProviderEvent) + Send),
        cancel: &CancellationToken,
    ) -> Result<TurnResult, ProviderError> {
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
                break;
            }

            // Split: subagent/memory tools run sequentially, rest run in parallel
            let mut sequential = Vec::new();
            let mut parallel = Vec::new();
            for tool in pending_tools {
                if matches!(tool.1.as_str(), "subagent" | "memory_read" | "memory_write") {
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
        }

        self.maybe_compact(provider).await;
        self.undo_manager.end_turn();
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

        let outcome = match prompter {
            Some(ref mut p) => policy.authorize(name, input, Some(&mut **p)),
            None => policy.authorize(name, input, None),
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

        // Snapshot files before write operations for undo
        if matches!(name, "write_file" | "edit_file") {
            if let Some(path) = input_val.get("path").and_then(|v| v.as_str()) {
                self.undo_manager
                    .snapshot_before_write(std::path::Path::new(path));
            }
        }

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
            let sub_prompt = if context.is_empty() {
                task.to_string()
            } else {
                format!("{task}\n\nContext:\n{context}")
            };
            match self
                .subagent
                .run_task(
                    provider,
                    &sub_prompt,
                    &self.system_prompt,
                    &self.tool_registry,
                )
                .await
            {
                Ok(out) => (out, false),
                Err(e) => (e.to_string(), true),
            }
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
    async fn maybe_compact(&mut self, provider: &dyn LlmProvider) {
        let ctx_window = self.model_registry.context_window(&self.model) as usize;
        if crate::compact::should_compact(&self.session, ctx_window, 0.8) {
            let preserve = 4;
            if let Err(e) =
                crate::compact::smart_compact(provider, &mut self.session, &self.model, preserve)
                    .await
            {
                tracing::warn!("smart compaction failed, using naive: {e}");
                crate::compact::compact_session(&mut self.session, preserve);
            }
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
                "{}{}\n\nYou are in PLAN MODE. Describe step-by-step what you would do. Do NOT use any tools.",
                self.system_prompt,
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
                    "{}{}{}",
                    self.system_prompt,
                    memory_section,
                    self.repo_map.as_deref().unwrap_or("")
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
        }
    }
}

pub(crate) async fn next_event(
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
    use std::io::Read;
    let mut file = std::fs::File::open(path).ok()?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).ok()?;
    Some(base64_encode(&buf))
}

fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = u32::from(chunk.get(1).copied().unwrap_or(0));
        let b2 = u32::from(chunk.get(2).copied().unwrap_or(0));
        let triple = (b0 << 16) | (b1 << 8) | b2;
        result.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARS[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}
