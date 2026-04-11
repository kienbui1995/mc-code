use std::time::Duration;

use crate::error::ProviderError;
use crate::sse::SseParser;
use crate::types::{
    AnthropicContentBlock, AnthropicDelta, AnthropicImageSource, AnthropicInputMessage,
    AnthropicOutputBlock, AnthropicRequest, AnthropicStreamEvent, AnthropicSystemBlock,
    AnthropicThinking, AnthropicToolChoice, AnthropicToolDef, AnthropicToolResultContent,
    CacheControl, CompletionRequest, ContentBlock, InputMessage, MessageRole, ModelInfo,
    ProviderEvent, TokenUsage, ToolChoice, ToolDefinition,
};

const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const DEFAULT_MAX_RETRIES: u32 = 2;
const DEFAULT_INITIAL_BACKOFF: Duration = Duration::from_millis(200);
const DEFAULT_MAX_BACKOFF: Duration = Duration::from_secs(2);

/// Anthropicprovider.
pub struct AnthropicProvider {
    http: reqwest::Client,
    api_key: String,
    base_url: String,
    max_retries: u32,
    initial_backoff: Duration,
    max_backoff: Duration,
}

impl AnthropicProvider {
    /// From env.
    pub fn from_env() -> Result<Self, ProviderError> {
        let api_key = read_env_key("ANTHROPIC_API_KEY")?;
        let base_url =
            std::env::var("ANTHROPIC_BASE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.to_string());

        Ok(Self {
            http: reqwest::Client::new(),
            api_key,
            base_url,
            max_retries: DEFAULT_MAX_RETRIES,
            initial_backoff: DEFAULT_INITIAL_BACKOFF,
            max_backoff: DEFAULT_MAX_BACKOFF,
        })
    }

    #[must_use]
    /// With config.
    pub fn with_config(api_key: String, base_url: Option<String>, max_retries: u32) -> Self {
        Self {
            http: reqwest::Client::new(),
            api_key,
            base_url: base_url.unwrap_or_else(|| DEFAULT_BASE_URL.to_string()),
            max_retries,
            initial_backoff: DEFAULT_INITIAL_BACKOFF,
            max_backoff: DEFAULT_MAX_BACKOFF,
        }
    }

    #[must_use]
    /// Model info.
    pub fn model_info(model: &str) -> ModelInfo {
        let context_window = match model {
            m if m.contains("opus") => 200_000,
            m if m.contains("sonnet") => 200_000,
            m if m.contains("haiku") => 200_000,
            _ => 200_000,
        };
        ModelInfo {
            name: model.to_string(),
            provider: "anthropic".to_string(),
            context_window,
        }
    }

    /// Stream a completion request, yielding events as they arrive.
    pub fn stream(&self, request: &CompletionRequest) -> crate::ProviderStream {
        let wire = Self::to_wire_request(request);
        let http = self.http.clone();
        let api_key = self.api_key.clone();
        let base_url = self.base_url.clone();
        let max_retries = self.max_retries;
        let initial_backoff = self.initial_backoff;
        let max_backoff = self.max_backoff;

        tracing::debug!(model = %request.model, tools = request.tools.len(), "streaming request");

        Box::pin(async_stream::try_stream! {
            let response = send_with_retry_static(
                &http, &api_key, &base_url, max_retries, initial_backoff, max_backoff, &wire,
            ).await?;

            let mut parser = SseParser::default();
            let mut pending_tool: Option<(String, String, String)> = None;
            let mut stream = response;

            #[allow(clippy::single_match_else)]
            loop {
                match stream.chunk().await? {
                    Some(chunk) => {
                        for sse_event in parser.push(&chunk)? {
                            for ev in process_sse_event_vec(sse_event, &mut pending_tool) {
                                yield ev;
                            }
                        }
                    }
                    None => {
                        for sse_event in parser.finish()? {
                            for ev in process_sse_event_vec(sse_event, &mut pending_tool) {
                                yield ev;
                            }
                        }
                        break;
                    }
                }
            }

            if let Some((id, name, input)) = pending_tool.take() {
                yield ProviderEvent::ToolUse { id, name, input };
            }
            yield ProviderEvent::MessageStop;
        })
    }

    fn to_wire_request(req: &CompletionRequest) -> AnthropicRequest {
        let system = req.system_prompt.as_ref().map(|text| {
            vec![AnthropicSystemBlock {
                r#type: "text".into(),
                text: text.clone(),
                cache_control: Some(CacheControl {
                    r#type: "ephemeral".into(),
                }),
            }]
        });

        let tools = if req.tools.is_empty() {
            None
        } else {
            let mut wire_tools: Vec<AnthropicToolDef> =
                req.tools.iter().map(to_wire_tool).collect();
            // Mark last tool with cache_control for prompt caching
            if let Some(last) = wire_tools.last_mut() {
                last.cache_control = Some(CacheControl {
                    r#type: "ephemeral".into(),
                });
            }
            Some(wire_tools)
        };

        let thinking = req.thinking_budget.map(|budget| AnthropicThinking {
            r#type: "enabled".into(),
            budget_tokens: budget,
        });

        AnthropicRequest {
            model: req.model.clone(),
            max_tokens: req.max_tokens,
            system,
            messages: req.messages.iter().map(to_wire_message).collect(),
            tools,
            tool_choice: req.tool_choice.as_ref().map(to_wire_tool_choice),
            thinking,
            stream: true,
        }
    }
}

/// Process a single SSE event into a vec of provider events.
fn process_sse_event_vec(
    event: AnthropicStreamEvent,
    pending_tool: &mut Option<(String, String, String)>,
) -> Vec<ProviderEvent> {
    let mut out = Vec::new();
    match event {
        AnthropicStreamEvent::MessageStart { message } => {
            for block in message.content {
                push_output_block(block, &mut out, pending_tool);
            }
        }
        AnthropicStreamEvent::ContentBlockStart { content_block } => {
            push_output_block(content_block, &mut out, pending_tool);
        }
        AnthropicStreamEvent::ContentBlockDelta { delta } => match delta {
            AnthropicDelta::TextDelta { text } => {
                if !text.is_empty() {
                    out.push(ProviderEvent::TextDelta(text));
                }
            }
            AnthropicDelta::InputJsonDelta { partial_json } => {
                if let Some((_, ref name, input)) = pending_tool.as_mut() {
                    input.push_str(&partial_json);
                    if matches!(name.as_str(), "write_file" | "edit_file") {
                        out.push(ProviderEvent::ToolInputDelta {
                            name: name.clone(),
                            partial: partial_json,
                        });
                    }
                }
            }
            AnthropicDelta::ThinkingDelta { thinking } => {
                if !thinking.is_empty() {
                    out.push(ProviderEvent::ThinkingDelta(thinking));
                }
            }
        },
        AnthropicStreamEvent::ContentBlockStop {} => {
            if let Some((id, name, input)) = pending_tool.take() {
                out.push(ProviderEvent::ToolUse { id, name, input });
            }
        }
        AnthropicStreamEvent::MessageDelta { usage } => {
            out.push(ProviderEvent::Usage(TokenUsage {
                input_tokens: usage.input_tokens,
                output_tokens: usage.output_tokens,
                cache_creation_input_tokens: usage.cache_creation_input_tokens,
                cache_read_input_tokens: usage.cache_read_input_tokens,
            }));
        }
        AnthropicStreamEvent::MessageStop {} => {
            out.push(ProviderEvent::MessageStop);
        }
    }
    out
}

fn push_output_block(
    block: AnthropicOutputBlock,
    events: &mut Vec<ProviderEvent>,
    pending_tool: &mut Option<(String, String, String)>,
) {
    match block {
        AnthropicOutputBlock::Text { text } => {
            if !text.is_empty() {
                events.push(ProviderEvent::TextDelta(text));
            }
        }
        AnthropicOutputBlock::ToolUse { id, name, input } => {
            *pending_tool = Some((id, name, input.to_string()));
        }
        AnthropicOutputBlock::Thinking { thinking } => {
            if !thinking.is_empty() {
                events.push(ProviderEvent::ThinkingDelta(thinking));
            }
        }
    }
}

async fn send_with_retry_static(
    http: &reqwest::Client,
    api_key: &str,
    base_url: &str,
    max_retries: u32,
    initial_backoff: Duration,
    max_backoff: Duration,
    request: &AnthropicRequest,
) -> Result<reqwest::Response, ProviderError> {
    let mut last_error: Option<ProviderError> = None;

    for attempt in 0..=max_retries {
        match send_raw_static(http, api_key, base_url, request).await {
            Ok(resp) => match check_status(resp).await {
                Ok(resp) => return Ok(resp),
                Err(e) if e.is_retryable() && attempt < max_retries => {
                    last_error = Some(e);
                }
                Err(e) => return Err(e),
            },
            Err(e) if e.is_retryable() && attempt < max_retries => {
                last_error = Some(e);
            }
            Err(e) => return Err(e),
        }

        let multiplier = 1u64.checked_shl(attempt).unwrap_or(u64::MAX);
        let delay = initial_backoff
            .checked_mul(multiplier as u32)
            .map_or(max_backoff, |d| d.min(max_backoff));
        tokio::time::sleep(delay).await;
    }

    Err(ProviderError::RetriesExhausted {
        attempts: max_retries + 1,
        last_message: last_error.map_or_else(|| "unknown".into(), |e| e.to_string()),
    })
}

async fn send_raw_static(
    http: &reqwest::Client,
    api_key: &str,
    base_url: &str,
    request: &AnthropicRequest,
) -> Result<reqwest::Response, ProviderError> {
    let url = format!("{}/v1/messages", base_url.trim_end_matches('/'));
    http.post(&url)
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .header("content-type", "application/json")
        .json(request)
        .send()
        .await
        .map_err(ProviderError::from)
}

fn read_env_key(var: &str) -> Result<String, ProviderError> {
    match std::env::var(var) {
        Ok(v) if !v.is_empty() => Ok(v),
        _ => Err(ProviderError::MissingApiKey {
            env_var: var.to_string(),
        }),
    }
}

async fn check_status(response: reqwest::Response) -> Result<reqwest::Response, ProviderError> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let body = response.text().await.unwrap_or_default();
    let retryable = matches!(status.as_u16(), 408 | 429 | 500 | 502 | 503 | 504);
    Err(ProviderError::Api {
        status: status.as_u16(),
        error_type: None,
        message: body,
        retryable,
    })
}

fn to_wire_message(msg: &InputMessage) -> AnthropicInputMessage {
    let role = match msg.role {
        MessageRole::User | MessageRole::Tool => "user",
        MessageRole::Assistant => "assistant",
    };
    AnthropicInputMessage {
        role: role.to_string(),
        content: msg.content.iter().map(to_wire_content).collect(),
    }
}

fn to_wire_content(block: &ContentBlock) -> AnthropicContentBlock {
    match block {
        ContentBlock::Text { text } => AnthropicContentBlock::Text { text: text.clone() },
        ContentBlock::ToolUse { id, name, input } => AnthropicContentBlock::ToolUse {
            id: id.clone(),
            name: name.clone(),
            input: serde_json::from_str(input)
                .unwrap_or_else(|_| serde_json::json!({ "raw": input })),
        },
        ContentBlock::ToolResult {
            tool_use_id,
            output,
            is_error,
        } => AnthropicContentBlock::ToolResult {
            tool_use_id: tool_use_id.clone(),
            content: vec![AnthropicToolResultContent::Text {
                text: output.clone(),
            }],
            is_error: *is_error,
        },
        ContentBlock::Image { data, media_type } => AnthropicContentBlock::Image {
            source: AnthropicImageSource {
                r#type: "base64".into(),
                media_type: media_type.clone(),
                data: data.clone(),
            },
        },
        ContentBlock::Thinking { text } => AnthropicContentBlock::Thinking {
            thinking: text.clone(),
        },
    }
}

fn to_wire_tool(tool: &ToolDefinition) -> AnthropicToolDef {
    AnthropicToolDef {
        name: tool.name.clone(),
        description: Some(tool.description.clone()),
        input_schema: tool.input_schema.clone(),
        cache_control: None,
    }
}

fn to_wire_tool_choice(choice: &ToolChoice) -> AnthropicToolChoice {
    match choice {
        ToolChoice::Auto => AnthropicToolChoice::Auto,
        ToolChoice::Any => AnthropicToolChoice::Any,
        ToolChoice::Tool { name } => AnthropicToolChoice::Tool { name: name.clone() },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_messages_to_wire_format() {
        let msg = InputMessage {
            role: MessageRole::User,
            content: vec![ContentBlock::Text {
                text: "hello".into(),
            }],
        };
        let wire = to_wire_message(&msg);
        assert_eq!(wire.role, "user");
        assert_eq!(wire.content.len(), 1);
    }

    #[test]
    fn tool_result_converts_correctly() {
        let msg = InputMessage {
            role: MessageRole::Tool,
            content: vec![ContentBlock::ToolResult {
                tool_use_id: "t1".into(),
                output: "ok".into(),
                is_error: false,
            }],
        };
        let wire = to_wire_message(&msg);
        assert_eq!(wire.role, "user"); // tool results sent as user role
    }

    #[test]
    fn model_info_returns_context_window() {
        let info = AnthropicProvider::model_info("claude-sonnet-4-20250514");
        assert_eq!(info.context_window, 200_000);
        assert_eq!(info.provider, "anthropic");
    }
}
