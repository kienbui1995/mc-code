use futures_core::Stream;
use std::pin::Pin;
use tokio_util::sync::CancellationToken;

use mc_provider::types::{ContentBlock, MessageRole};
use mc_provider::{
    CompletionRequest, InputMessage, ProviderError, ProviderEvent, ProviderStream, TokenUsage,
    ToolChoice, ToolDefinition,
};
use mc_tools::{AuditEntry, AuditLog, HookEngine, HookEvent, PermissionOutcome, PermissionPolicy, ToolRegistry};

use crate::model_registry::ModelRegistry;
use crate::session::{ConversationMessage, Session};
use crate::subagent::SubagentSpawner;
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

/// Trait for any LLM provider — returns a stream of events.
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
    model: String,
    max_tokens: u32,
    hook_engine: Option<HookEngine>,
    model_registry: ModelRegistry,
    subagent: SubagentSpawner,
    audit_log: Option<AuditLog>,
}

impl ConversationRuntime {
    #[must_use]
    pub fn new(model: String, max_tokens: u32, system_prompt: String) -> Self {
        let subagent = SubagentSpawner::new(model.clone(), max_tokens);
        let audit_log = AuditLog::default_path().map(AuditLog::new);
        Self {
            session: Session::default(),
            usage: UsageTracker::default(),
            plan_mode: false,
            system_prompt,
            model,
            max_tokens,
            hook_engine: None,
            model_registry: ModelRegistry::default(),
            subagent,
            audit_log,
        }
    }

    pub fn set_hooks(&mut self, engine: HookEngine) {
        self.hook_engine = Some(engine);
    }

    #[allow(clippy::too_many_lines)]
    pub async fn run_turn(
        &mut self,
        provider: &dyn LlmProvider,
        user_input: &str,
        permission_policy: &PermissionPolicy,
        on_event: &mut (dyn FnMut(&ProviderEvent) + Send),
        cancel: &CancellationToken,
    ) -> Result<TurnResult, ProviderError> {
        self.session
            .messages
            .push(ConversationMessage::user(user_input));

        let mut final_text = String::new();
        let mut tool_calls = Vec::new();
        let mut turn_usage = TokenUsage::default();
        let mut iterations = 0;

        loop {
            if cancel.is_cancelled() {
                return Ok(TurnResult {
                    text: final_text,
                    tool_calls,
                    usage: turn_usage,
                    iterations,
                    cancelled: true,
                });
            }

            iterations += 1;
            if iterations > MAX_ITERATIONS {
                break;
            }

            let request = self.build_request();
            let mut stream = provider.stream(&request);

            let mut text_buf = String::new();
            let mut pending_tools: Vec<(String, String, String)> = Vec::new();

            // Consume the stream event by event
            loop {
                let next = tokio::select! {
                    () = cancel.cancelled() => {
                        return Ok(TurnResult {
                            text: final_text,
                            tool_calls,
                            usage: turn_usage,
                            iterations,
                            cancelled: true,
                        });
                    }
                    item = next_event(&mut stream) => item,
                };

                match next {
                    Some(Ok(event)) => {
                        on_event(&event);
                        match &event {
                            ProviderEvent::TextDelta(t) => text_buf.push_str(t),
                            ProviderEvent::ToolUse { id, name, input } => {
                                pending_tools.push((id.clone(), name.clone(), input.clone()));
                            }
                            ProviderEvent::Usage(u) => {
                                turn_usage = u.clone();
                                self.usage.record(u);
                                self.session.input_tokens += u.input_tokens;
                                self.session.output_tokens += u.output_tokens;
                            }
                            ProviderEvent::MessageStop => {}
                        }
                    }
                    Some(Err(e)) => return Err(e),
                    None => break,
                }
            }

            if !text_buf.is_empty() {
                self.session
                    .messages
                    .push(ConversationMessage::assistant(&text_buf));
                final_text.push_str(&text_buf);
            }

            if pending_tools.is_empty() {
                break;
            }

            for (id, name, input) in pending_tools {
                if cancel.is_cancelled() {
                    return Ok(TurnResult {
                        text: final_text,
                        tool_calls,
                        usage: turn_usage,
                        iterations,
                        cancelled: true,
                    });
                }

                tool_calls.push(name.clone());
                self.session
                    .messages
                    .push(ConversationMessage::tool_use(&id, &name, &input));

                // Pre-tool hook
                if let Some(ref engine) = self.hook_engine {
                    if let Err(e) = engine.fire(
                        &HookEvent::PreToolCall,
                        Some(&name),
                        &[("tool_name", &name)],
                    ) {
                        self.session.messages.push(ConversationMessage::tool_result(
                            &id,
                            &name,
                            e.to_string(),
                            true,
                        ));
                        continue;
                    }
                }

                let outcome = permission_policy.authorize(&name, &input, None);

                let timer = AuditLog::start_timer();
                let (output, is_error) = match outcome {
                    PermissionOutcome::Allow => {
                        let input_val: serde_json::Value = serde_json::from_str(&input)
                            .unwrap_or_else(|_| serde_json::json!({"raw": input}));

                        // Subagent is handled by runtime, not ToolRegistry
                        let result = if name == "subagent" {
                            let task = input_val.get("task").and_then(|v| v.as_str()).unwrap_or("");
                            let context = input_val.get("context").and_then(|v| v.as_str()).unwrap_or("");
                            let sub_prompt = if context.is_empty() {
                                task.to_string()
                            } else {
                                format!("{task}\n\nContext:\n{context}")
                            };
                            match self.subagent.run_task(provider, &sub_prompt, &self.system_prompt).await {
                                Ok(out) => (out, false),
                                Err(e) => (e.to_string(), true),
                            }
                        } else {
                            match ToolRegistry::execute(&name, &input_val).await {
                                Ok(out) => (out, false),
                                Err(e) => (e.to_string(), true),
                            }
                        };

                        if let Some(ref log) = self.audit_log {
                            log.log(&AuditEntry {
                                tool: name.clone(),
                                input_summary: input.clone(),
                                output_len: result.0.len(),
                                is_error: result.1,
                                duration_ms: timer.elapsed().as_millis() as u64,
                                allowed: true,
                            });
                        }
                        result
                    }
                    PermissionOutcome::Deny { reason } => {
                        if let Some(ref log) = self.audit_log {
                            log.log(&AuditEntry {
                                tool: name.clone(),
                                input_summary: input.clone(),
                                output_len: 0,
                                is_error: true,
                                duration_ms: timer.elapsed().as_millis() as u64,
                                allowed: false,
                            });
                        }
                        (reason, true)
                    }
                };

                // Post-tool hook
                if let Some(ref engine) = self.hook_engine {
                    let _ = engine.fire(
                        &HookEvent::PostToolCall,
                        Some(&name),
                        &[("tool_name", &name)],
                    );
                }

                self.session.messages.push(ConversationMessage::tool_result(
                    &id, &name, &output, is_error,
                ));
            }
        }

        // Auto-compact if needed — use model registry for context window
        let ctx_window = self.model_registry.context_window(&self.model) as usize;
        if crate::compact::should_compact(&self.session, ctx_window, 0.8) {
            let preserve = 4;
            if let Err(e) = crate::compact::smart_compact(
                provider, &mut self.session, &self.model, preserve,
            ).await {
                tracing::warn!("smart compaction failed, using naive: {e}");
                crate::compact::compact_session(&mut self.session, preserve);
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

    fn build_request(&self) -> CompletionRequest {
        let mut messages = Vec::new();

        for msg in &self.session.messages {
            match msg.role.as_str() {
                "user" => {
                    messages.push(InputMessage {
                        role: MessageRole::User,
                        content: vec![ContentBlock::Text {
                            text: msg.content.clone(),
                        }],
                    });
                }
                "assistant" if msg.tool_name.is_some() => {
                    messages.push(InputMessage {
                        role: MessageRole::Assistant,
                        content: vec![ContentBlock::ToolUse {
                            id: msg.tool_use_id.clone().unwrap_or_default(),
                            name: msg.tool_name.clone().unwrap_or_default(),
                            input: msg.content.clone(),
                        }],
                    });
                }
                "assistant" => {
                    messages.push(InputMessage {
                        role: MessageRole::Assistant,
                        content: vec![ContentBlock::Text {
                            text: msg.content.clone(),
                        }],
                    });
                }
                "tool" => {
                    messages.push(InputMessage {
                        role: MessageRole::Tool,
                        content: vec![ContentBlock::ToolResult {
                            tool_use_id: msg.tool_use_id.clone().unwrap_or_default(),
                            output: msg.content.clone(),
                            is_error: msg.is_error.unwrap_or(false),
                        }],
                    });
                }
                _ => {}
            }
        }

        let (tools, tool_choice, system) = if self.plan_mode {
            (
                Vec::new(),
                None,
                format!(
                    "{}\n\nYou are in PLAN MODE. Describe step-by-step what you would do. Do NOT use any tools.",
                    self.system_prompt
                ),
            )
        } else {
            let tools: Vec<ToolDefinition> = ToolRegistry::specs()
                .iter()
                .map(|s| ToolDefinition {
                    name: s.name.clone(),
                    description: s.description.clone(),
                    input_schema: s.input_schema.clone(),
                })
                .collect();
            (tools, Some(ToolChoice::Auto), self.system_prompt.clone())
        };

        CompletionRequest {
            model: self.model.clone(),
            max_tokens: self.max_tokens,
            system_prompt: Some(system),
            messages,
            tools,
            tool_choice,
        }
    }
}

/// Helper to get next item from a pinned boxed stream.
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
