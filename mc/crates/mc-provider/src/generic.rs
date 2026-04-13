use std::time::Duration;

use crate::error::ProviderError;
use crate::types::{
    CompletionRequest, ContentBlock, InputMessage, MessageRole, ModelInfo, ProviderEvent,
    TokenUsage, ToolChoice,
};

/// A provider that works with any OpenAI-compatible API endpoint.
/// Supports: `OpenAI`, `LiteLLM`, Ollama, vLLM, Together, Groq, Fireworks, etc.
pub struct GenericProvider {
    http: reqwest::Client,
    api_key: Option<String>,
    base_url: String,
    max_retries: u32,
}

impl GenericProvider {
    #[must_use]
    /// New.
    pub fn new(base_url: String, api_key: Option<String>) -> Self {
        Self {
            http: reqwest::Client::new(),
            api_key,
            base_url,
            max_retries: 2,
        }
    }

    /// Create from env vars. Checks `MAGIC_CODE_BASE_URL` / `MAGIC_CODE_API_KEY` first,
    /// then falls back to `OPENAI_BASE_URL` / `OPENAI_API_KEY`.
    pub fn from_env() -> Result<Self, ProviderError> {
        let base_url = std::env::var("MAGIC_CODE_BASE_URL")
            .or_else(|_| std::env::var("OPENAI_BASE_URL"))
            .unwrap_or_else(|_| "https://api.openai.com".to_string());

        let api_key = std::env::var("MAGIC_CODE_API_KEY")
            .or_else(|_| std::env::var("OPENAI_API_KEY"))
            .ok()
            .filter(|k| !k.is_empty());

        Ok(Self::new(base_url, api_key))
    }

    /// Create for a `LiteLLM` proxy.
    #[must_use]
    /// Litellm.
    pub fn litellm(base_url: String, api_key: Option<String>) -> Self {
        Self::new(base_url, api_key)
    }

    /// Create for local Ollama.
    #[must_use]
    /// Ollama.
    pub fn ollama() -> Self {
        let host =
            std::env::var("OLLAMA_HOST").unwrap_or_else(|_| "http://localhost:11434".to_string());
        Self::new(host, None)
    }

    #[must_use]
    /// Model info.
    pub fn model_info(model: &str, provider_name: &str) -> ModelInfo {
        ModelInfo {
            name: model.to_string(),
            provider: provider_name.to_string(),
            context_window: 128_000,
        }
    }

    /// Stream.
    pub fn stream(&self, request: &CompletionRequest) -> crate::ProviderStream {
        tracing::debug!(
            model = %request.model,
            base_url = %self.base_url,
            tools = request.tools.len(),
            "generic provider streaming"
        );
        let body = build_request_body(request);
        let http = self.http.clone();
        let api_key = self.api_key.clone();
        let base_url = self.base_url.clone();
        let max_retries = self.max_retries;

        Box::pin(async_stream::try_stream! {
            let response = send_with_retry_generic(
                &http, api_key.as_deref(), &base_url, max_retries, &body,
            ).await?;

            let mut buf = String::new();
            let mut pending_tools: std::collections::HashMap<usize, (String, String, String)> =
                std::collections::HashMap::new();
            let mut stream = response;

            while let Some(chunk) = stream.chunk().await? {
                buf.push_str(&String::from_utf8_lossy(&chunk));

                while let Some(pos) = buf.find("\n\n") {
                    let frame = buf[..pos].to_string();
                    buf = buf[pos + 2..].to_string();

                    for line in frame.lines() {
                        let data = match line.strip_prefix("data: ") {
                            Some(d) => d.trim(),
                            None => continue,
                        };
                        if data == "[DONE]" {
                            for ev in flush_pending_tools_vec(&mut pending_tools) { yield ev; }
                            yield ProviderEvent::MessageStop;
                            return;
                        }

                        let v: serde_json::Value = match serde_json::from_str(data) {
                            Ok(v) => v,
                            Err(_) => continue,
                        };

                        if let Some(usage) = v.get("usage") {
                            yield ProviderEvent::Usage(TokenUsage {
                                input_tokens: usage["prompt_tokens"].as_u64().unwrap_or(0) as u32,
                                output_tokens: usage["completion_tokens"].as_u64().unwrap_or(0) as u32,
                                ..Default::default()
                            });
                        }

                        if let Some(choices) = v.get("choices").and_then(|c| c.as_array()) {
                            for choice in choices {
                                if let Some(delta) = choice.get("delta") {
                                    if let Some(text) = delta.get("content").and_then(|c| c.as_str()) {
                                        if !text.is_empty() {
                                            yield ProviderEvent::TextDelta(text.to_string());
                                        }
                                    }
                                    if let Some(tcs) = delta.get("tool_calls").and_then(|t| t.as_array()) {
                                        for tc in tcs {
                                            let idx = tc.get("index").and_then(serde_json::Value::as_u64).unwrap_or(0) as usize;
                                            let entry = pending_tools.entry(idx).or_insert_with(|| {
                                                let id = tc.get("id").and_then(|i| i.as_str()).unwrap_or("").to_string();
                                                let name = tc.pointer("/function/name").and_then(|n| n.as_str()).unwrap_or("").to_string();
                                                (id, name, String::new())
                                            });
                                            if let Some(args) = tc.pointer("/function/arguments").and_then(|a| a.as_str()) {
                                                entry.2.push_str(args);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            for ev in flush_pending_tools_vec(&mut pending_tools) { yield ev; }
            yield ProviderEvent::MessageStop;
        })
    }
}

async fn send_with_retry_generic(
    http: &reqwest::Client,
    api_key: Option<&str>,
    base_url: &str,
    max_retries: u32,
    body: &serde_json::Value,
) -> Result<reqwest::Response, ProviderError> {
    let mut last_err: Option<ProviderError> = None;
    let url = format!("{}/v1/chat/completions", base_url.trim_end_matches('/'));
    for attempt in 0..=max_retries {
        let mut req = http.post(&url).header("content-type", "application/json");
        if let Some(key) = api_key {
            req = req.bearer_auth(key);
        }
        match req.json(body).send().await {
            Ok(r) if r.status().is_success() => return Ok(r),
            Ok(r) => {
                let status = r.status().as_u16();
                let text = r.text().await.unwrap_or_default();
                let retryable = matches!(status, 429 | 500 | 502 | 503 | 504);
                let err = ProviderError::Api {
                    status,
                    error_type: None,
                    message: text,
                    retryable,
                };
                if retryable && attempt < max_retries {
                    last_err = Some(err);
                    tokio::time::sleep(Duration::from_millis(200 * (1u64 << attempt))).await;
                    continue;
                }
                return Err(err);
            }
            Err(e) => {
                let err = ProviderError::from(e);
                if err.is_retryable() && attempt < max_retries {
                    last_err = Some(err);
                    tokio::time::sleep(Duration::from_millis(200 * (1u64 << attempt))).await;
                    continue;
                }
                return Err(err);
            }
        }
    }
    Err(ProviderError::RetriesExhausted {
        attempts: max_retries + 1,
        last_message: last_err.map_or("unknown".into(), |e| e.to_string()),
    })
}

fn flush_pending_tools_vec(
    pending: &mut std::collections::HashMap<usize, (String, String, String)>,
) -> Vec<ProviderEvent> {
    let mut indices: Vec<_> = pending.keys().copied().collect();
    indices.sort_unstable();
    indices
        .iter()
        .filter_map(|idx| {
            pending
                .remove(idx)
                .map(|(id, name, args)| ProviderEvent::ToolUse {
                    id,
                    name,
                    input: args,
                })
        })
        .collect()
}

fn build_request_body(req: &CompletionRequest) -> serde_json::Value {
    let messages: Vec<serde_json::Value> = std::iter::once(serde_json::json!({
        "role": "system",
        "content": req.system_prompt.as_deref().unwrap_or("")
    }))
    .chain(req.messages.iter().map(msg_to_json))
    .collect();

    let mut body = serde_json::json!({
        "model": req.model,
        "max_tokens": req.max_tokens,
        "messages": messages,
        "stream": true,
        "stream_options": {"include_usage": true},
    });

    if !req.tools.is_empty() {
        body["tools"] = serde_json::json!(req
            .tools
            .iter()
            .map(|t| serde_json::json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.input_schema,
                }
            }))
            .collect::<Vec<_>>());
        if let Some(choice) = &req.tool_choice {
            body["tool_choice"] = match choice {
                ToolChoice::Auto => serde_json::json!("auto"),
                ToolChoice::Any => serde_json::json!("required"),
                ToolChoice::Tool { name } => {
                    serde_json::json!({"type": "function", "function": {"name": name}})
                }
            };
        }
    }
    // Structured output / JSON mode
    if let Some(ref fmt) = req.response_format {
        match fmt {
            crate::types::ResponseFormat::Json => {
                body["response_format"] = serde_json::json!({"type": "json_object"});
            }
            crate::types::ResponseFormat::JsonSchema { name, schema } => {
                body["response_format"] = serde_json::json!({
                    "type": "json_schema",
                    "json_schema": {"name": name, "schema": schema}
                });
            }
        }
    }
    body
}

fn msg_to_json(msg: &InputMessage) -> serde_json::Value {
    let role = match msg.role {
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
        MessageRole::Tool => "tool",
    };

    if msg.role == MessageRole::Tool {
        if let Some(ContentBlock::ToolResult {
            tool_use_id,
            output,
            ..
        }) = msg.content.first()
        {
            return serde_json::json!({"role": "tool", "tool_call_id": tool_use_id, "content": output});
        }
    }

    if msg.role == MessageRole::Assistant {
        let tool_uses: Vec<_> = msg
            .content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::ToolUse { id, name, input } => Some(serde_json::json!({
                    "id": id, "type": "function", "function": {"name": name, "arguments": input}
                })),
                _ => None,
            })
            .collect();
        if !tool_uses.is_empty() {
            let text: String = msg
                .content
                .iter()
                .filter_map(|b| match b {
                    ContentBlock::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("");
            return serde_json::json!({
                "role": "assistant",
                "content": if text.is_empty() { serde_json::Value::Null } else { serde_json::json!(text) },
                "tool_calls": tool_uses,
            });
        }
    }

    let has_image = msg
        .content
        .iter()
        .any(|b| matches!(b, ContentBlock::Image { .. }));

    let text: String = msg
        .content
        .iter()
        .filter_map(|b| match b {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");

    if has_image {
        let mut parts: Vec<serde_json::Value> = Vec::new();
        for b in &msg.content {
            match b {
                ContentBlock::Text { text } => {
                    parts.push(serde_json::json!({"type": "text", "text": text}));
                }
                ContentBlock::Image { data, media_type } => parts.push(serde_json::json!({
                    "type": "image_url",
                    "image_url": {"url": format!("data:{media_type};base64,{data}")}
                })),
                _ => {}
            }
        }
        return serde_json::json!({"role": role, "content": parts});
    }

    serde_json::json!({"role": role, "content": text})
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ToolDefinition;

    #[test]
    fn builds_request_with_system_and_tools() {
        let req = CompletionRequest {
            model: "gpt-4o".into(),
            max_tokens: 100,
            system_prompt: Some("be helpful".into()),
            messages: vec![InputMessage {
                role: MessageRole::User,
                content: vec![ContentBlock::Text { text: "hi".into() }],
            }],
            tools: vec![ToolDefinition {
                name: "bash".into(),
                description: "run cmd".into(),
                input_schema: serde_json::json!({"type": "object"}),
            }],
            tool_choice: Some(ToolChoice::Auto),
            thinking_budget: None,
            response_format: None,
        };
        let body = build_request_body(&req);
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][1]["role"], "user");
        assert!(body.get("tools").is_some());
        assert_eq!(body["tool_choice"], "auto");
    }

    #[test]
    fn creates_from_explicit_config() {
        let p = GenericProvider::new("http://localhost:4000".into(), Some("sk-test".into()));
        assert_eq!(p.base_url, "http://localhost:4000");
        assert_eq!(p.api_key.as_deref(), Some("sk-test"));
    }

    #[test]
    fn creates_ollama_provider() {
        let p = GenericProvider::ollama();
        assert!(p.base_url.contains("11434"));
        assert!(p.api_key.is_none());
    }

    #[test]
    fn litellm_shortcut() {
        let p = GenericProvider::litellm("http://litellm:4000".into(), Some("key".into()));
        assert_eq!(p.base_url, "http://litellm:4000");
    }

    #[test]
    fn tool_result_message_format() {
        let msg = InputMessage {
            role: MessageRole::Tool,
            content: vec![ContentBlock::ToolResult {
                tool_use_id: "call_1".into(),
                output: "done".into(),
                is_error: false,
            }],
        };
        let json = msg_to_json(&msg);
        assert_eq!(json["role"], "tool");
        assert_eq!(json["tool_call_id"], "call_1");
    }
}

#[test]
fn response_format_json_mode() {
    let req = CompletionRequest {
        model: "gpt-4".into(),
        max_tokens: 100,
        system_prompt: None,
        messages: vec![],
        tools: vec![],
        tool_choice: None,
        thinking_budget: None,
        response_format: Some(crate::types::ResponseFormat::Json),
    };
    let body = build_request_body(&req);
    assert_eq!(body["response_format"]["type"], "json_object");
}

#[test]
fn response_format_json_schema() {
    let schema = serde_json::json!({"type": "object", "properties": {"name": {"type": "string"}}});
    let req = CompletionRequest {
        model: "gpt-4".into(),
        max_tokens: 100,
        system_prompt: None,
        messages: vec![],
        tools: vec![],
        tool_choice: None,
        thinking_budget: None,
        response_format: Some(crate::types::ResponseFormat::JsonSchema {
            name: "test".into(),
            schema: schema.clone(),
        }),
    };
    let body = build_request_body(&req);
    assert_eq!(body["response_format"]["type"], "json_schema");
    assert_eq!(body["response_format"]["json_schema"]["name"], "test");
}

#[test]
fn response_format_none_omitted() {
    let req = CompletionRequest {
        model: "gpt-4".into(),
        max_tokens: 100,
        system_prompt: None,
        messages: vec![],
        tools: vec![],
        tool_choice: None,
        thinking_budget: None,
        response_format: None,
    };
    let body = build_request_body(&req);
    assert!(body.get("response_format").is_none());
}
