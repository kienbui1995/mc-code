use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use mc_provider::types::{ContentBlock, MessageRole};
use mc_provider::{
    CompletionRequest, InputMessage, ProviderError, ProviderEvent, ToolChoice, ToolDefinition,
};
use mc_tools::{PermissionOutcome, PermissionPolicy, ToolRegistry};

use crate::runtime::LlmProvider;
use crate::session::{Block, ConversationMessage, Role};

const MAX_CONCURRENT_SUBAGENTS: usize = 4;
const MAX_SUBAGENT_ITERATIONS: usize = 8;

/// Shared context board for inter-subagent communication.
#[derive(Debug, Clone, Default)]
pub struct SharedContext {
    entries: Arc<Mutex<HashMap<String, String>>>,
}

impl SharedContext {
    /// Write a key-value pair visible to all subagents.
    pub fn set(&self, key: &str, value: &str) {
        if let Ok(mut map) = self.entries.lock() {
            map.insert(key.to_string(), value.to_string());
        }
    }

    /// Read a value by key.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<String> {
        self.entries.lock().ok()?.get(key).cloned()
    }

    /// Get all entries as a formatted string for injection into subagent context.
    #[must_use]
    pub fn summary(&self) -> String {
        let map = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        if map.is_empty() {
            return String::new();
        }
        let mut out = String::from("\n## Shared Context (from other agents)\n");
        for (k, v) in map.iter() {
            out.push_str(&format!(
                "- **{k}**: {}\n",
                crate::session::truncate(v, 200)
            ));
        }
        out
    }
}

/// Spawns isolated subagent conversations for delegated tasks.
pub struct SubagentSpawner {
    model: String,
    max_tokens: u32,
    active_count: usize,
    pub shared_context: SharedContext,
    background_results: Arc<Mutex<HashMap<String, Option<String>>>>,
}

impl SubagentSpawner {
    #[must_use]
    /// New.
    pub fn new(model: String, max_tokens: u32) -> Self {
        Self {
            model,
            max_tokens,
            active_count: 0,
            shared_context: SharedContext::default(),
            background_results: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Poll a background agent's result. Returns None if still running.
    #[must_use]
    pub fn poll_background(&self, agent_id: &str) -> Option<Option<String>> {
        let results = self
            .background_results
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        results.get(agent_id).cloned()
    }

    /// Run a subagent with its own isolated context (no recursive subagent).
    pub async fn run_task(
        &mut self,
        provider: &dyn LlmProvider,
        task_prompt: &str,
        system_prompt: &str,
        tool_registry: &ToolRegistry,
        model_override: Option<&str>,
        allowed_tools: Option<&[String]>,
        max_turns: Option<usize>,
    ) -> Result<String, ProviderError> {
        if self.active_count >= MAX_CONCURRENT_SUBAGENTS {
            return Ok("[subagent limit reached, task queued]".to_string());
        }

        self.active_count += 1;
        let effective_model = model_override
            .filter(|m| !m.trim().is_empty())
            .unwrap_or(&self.model);
        tracing::debug!(
            task = task_prompt,
            model = effective_model,
            "spawning subagent"
        );

        // Inject shared context from other agents
        let shared = self.shared_context.summary();
        let enriched_prompt = if shared.is_empty() {
            system_prompt.to_string()
        } else {
            format!("{system_prompt}{shared}")
        };

        let result = run_simple_agent(
            provider,
            effective_model,
            self.max_tokens,
            &enriched_prompt,
            task_prompt,
            tool_registry,
            allowed_tools,
            max_turns,
        )
        .await;

        self.active_count -= 1;

        // Store result in shared context for other agents
        let task_key = task_prompt.chars().take(50).collect::<String>();
        match &result {
            Ok(output) => self.shared_context.set(&task_key, output),
            Err(e) => self.shared_context.set(&task_key, &format!("error: {e}")),
        }

        match result {
            Ok(output) => {
                if output.len() > 500 {
                    let end = output
                        .char_indices()
                        .map(|(i, _)| i)
                        .take_while(|&i| i <= 500)
                        .last()
                        .unwrap_or(output.len());
                    Ok(format!("{}... [truncated]", &output[..end]))
                } else {
                    Ok(output)
                }
            }
            Err(e) => Err(e),
        }
    }

    #[must_use]
    /// Active count.
    pub fn active_count(&self) -> usize {
        self.active_count
    }

    /// List background agent IDs and their status.
    #[must_use]
    pub fn list_background(&self) -> Vec<(String, bool)> {
        let results = self
            .background_results
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        results
            .iter()
            .map(|(id, r)| (id.clone(), r.is_some()))
            .collect()
    }
}

/// A simple agent loop without subagent capability (breaks recursion).
async fn run_simple_agent(
    provider: &dyn LlmProvider,
    model: &str,
    max_tokens: u32,
    system_prompt: &str,
    user_input: &str,
    tool_registry: &ToolRegistry,
    allowed_tools: Option<&[String]>,
    max_turns: Option<usize>,
) -> Result<String, ProviderError> {
    let policy = PermissionPolicy::new(mc_tools::PermissionMode::Allow);
    let mut messages: Vec<ConversationMessage> = vec![ConversationMessage::user(user_input)];
    let mut output = String::new();

    let max_iters = max_turns.unwrap_or(MAX_SUBAGENT_ITERATIONS);

    // Tool specs without subagent (no recursion), optionally filtered
    let tools: Vec<ToolDefinition> = tool_registry
        .all_specs()
        .iter()
        .filter(|s| s.name != "subagent")
        .filter(|s| allowed_tools.map_or(true, |at| at.iter().any(|a| a == &s.name)))
        .map(|s| ToolDefinition {
            name: s.name.clone(),
            description: s.description.clone(),
            input_schema: s.input_schema.clone(),
        })
        .collect();

    for _ in 0..max_iters {
        let request = CompletionRequest {
            model: model.to_string(),
            max_tokens,
            system_prompt: Some(system_prompt.to_string()),
            messages: messages.iter().map(msg_to_input).collect(),
            tools: tools.clone(),
            tool_choice: Some(ToolChoice::Auto),
            thinking_budget: None,
        };

        let mut stream = provider.stream(&request);
        let mut text_buf = String::new();
        let mut pending_tools: Vec<(String, String, String)> = Vec::new();

        loop {
            match crate::runtime::next_event(&mut stream).await {
                Some(Ok(ProviderEvent::TextDelta(t))) => text_buf.push_str(&t),
                Some(Ok(ProviderEvent::ToolUse { id, name, input })) => {
                    pending_tools.push((id, name, input));
                }
                _ => break,
            }
        }

        if !text_buf.is_empty() {
            messages.push(ConversationMessage::assistant(&text_buf));
            output.push_str(&text_buf);
        }

        if pending_tools.is_empty() {
            break;
        }

        for (id, name, input) in pending_tools {
            messages.push(ConversationMessage::tool_use(&id, &name, &input));
            let outcome = policy.authorize(&name, &input, None);
            let (tool_output, is_error) = match outcome {
                PermissionOutcome::Allow => {
                    let input_val: serde_json::Value = serde_json::from_str(&input)
                        .unwrap_or_else(|_| serde_json::json!({"raw": input}));
                    match tool_registry.execute(&name, &input_val).await {
                        Ok(out) => (out, false),
                        Err(e) => (e.to_string(), true),
                    }
                }
                PermissionOutcome::Deny { reason } => (reason, true),
            };
            messages.push(ConversationMessage::tool_result(
                &id,
                &name,
                &tool_output,
                is_error,
            ));
        }
    }

    Ok(output)
}

fn msg_to_input(msg: &ConversationMessage) -> InputMessage {
    let role = match msg.role {
        Role::User => MessageRole::User,
        Role::Assistant => MessageRole::Assistant,
        Role::Tool => MessageRole::Tool,
    };
    let content = msg
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
            Block::Image { .. } | Block::Thinking { .. } => None, // subagents don't handle images/thinking
        })
        .collect();
    InputMessage { role, content }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_spawner() {
        let spawner = SubagentSpawner::new("test".into(), 1000);
        assert_eq!(spawner.active_count(), 0);
    }
}
