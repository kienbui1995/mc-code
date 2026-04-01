use mc_provider::{
    CompletionRequest, InputMessage, ProviderError, ProviderEvent,
    ToolChoice, ToolDefinition,
};
use mc_provider::types::{ContentBlock, MessageRole};
use mc_tools::{PermissionOutcome, PermissionPolicy, ToolRegistry};

use crate::runtime::LlmProvider;
use crate::session::ConversationMessage;

const MAX_CONCURRENT_SUBAGENTS: usize = 4;
const MAX_SUBAGENT_ITERATIONS: usize = 8;

/// Spawns isolated subagent conversations for delegated tasks.
pub struct SubagentSpawner {
    model: String,
    max_tokens: u32,
    active_count: usize,
}

impl SubagentSpawner {
    #[must_use]
    pub fn new(model: String, max_tokens: u32) -> Self {
        Self { model, max_tokens, active_count: 0 }
    }

    /// Run a subagent with its own isolated context (no recursive subagent).
    pub async fn run_task(
        &mut self,
        provider: &dyn LlmProvider,
        task_prompt: &str,
        system_prompt: &str,
    ) -> Result<String, ProviderError> {
        if self.active_count >= MAX_CONCURRENT_SUBAGENTS {
            return Ok("[subagent limit reached, task queued]".to_string());
        }

        self.active_count += 1;
        tracing::debug!(task = task_prompt, "spawning subagent");

        let result = run_simple_agent(
            provider,
            &self.model,
            self.max_tokens,
            system_prompt,
            task_prompt,
        ).await;

        self.active_count -= 1;

        match result {
            Ok(output) => {
                if output.len() > 500 {
                    let end = output.char_indices().map(|(i,_)|i).take_while(|&i| i <= 500).last().unwrap_or(output.len());
                    Ok(format!("{}... [truncated]", &output[..end]))
                } else {
                    Ok(output)
                }
            }
            Err(e) => Err(e),
        }
    }

    #[must_use]
    pub fn active_count(&self) -> usize {
        self.active_count
    }
}

/// A simple agent loop without subagent capability (breaks recursion).
async fn run_simple_agent(
    provider: &dyn LlmProvider,
    model: &str,
    max_tokens: u32,
    system_prompt: &str,
    user_input: &str,
) -> Result<String, ProviderError> {
    let policy = PermissionPolicy::new(mc_tools::PermissionMode::Allow);
    let mut messages: Vec<ConversationMessage> = vec![ConversationMessage::user(user_input)];
    let mut output = String::new();

    // Tool specs without subagent (no recursion)
    let tools: Vec<ToolDefinition> = ToolRegistry::specs()
        .iter()
        .filter(|s| s.name != "subagent")
        .map(|s| ToolDefinition {
            name: s.name.clone(),
            description: s.description.clone(),
            input_schema: s.input_schema.clone(),
        })
        .collect();

    for _ in 0..MAX_SUBAGENT_ITERATIONS {
        let request = CompletionRequest {
            model: model.to_string(),
            max_tokens,
            system_prompt: Some(system_prompt.to_string()),
            messages: messages.iter().map(msg_to_input).collect(),
            tools: tools.clone(),
            tool_choice: Some(ToolChoice::Auto),
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
                    match ToolRegistry::execute(&name, &input_val).await {
                        Ok(out) => (out, false),
                        Err(e) => (e.to_string(), true),
                    }
                }
                PermissionOutcome::Deny { reason } => (reason, true),
            };
            messages.push(ConversationMessage::tool_result(&id, &name, &tool_output, is_error));
        }
    }

    Ok(output)
}

fn msg_to_input(msg: &ConversationMessage) -> InputMessage {
    match msg.role.as_str() {
        "assistant" if msg.tool_name.is_some() => InputMessage {
            role: MessageRole::Assistant,
            content: vec![ContentBlock::ToolUse {
                id: msg.tool_use_id.clone().unwrap_or_default(),
                name: msg.tool_name.clone().unwrap_or_default(),
                input: msg.content.clone(),
            }],
        },
        "assistant" => InputMessage {
            role: MessageRole::Assistant,
            content: vec![ContentBlock::Text { text: msg.content.clone() }],
        },
        "tool" => InputMessage {
            role: MessageRole::Tool,
            content: vec![ContentBlock::ToolResult {
                tool_use_id: msg.tool_use_id.clone().unwrap_or_default(),
                output: msg.content.clone(),
                is_error: msg.is_error.unwrap_or(false),
            }],
        },
        _ => InputMessage {
            role: MessageRole::User,
            content: vec![ContentBlock::Text { text: msg.content.clone() }],
        },
    }
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
