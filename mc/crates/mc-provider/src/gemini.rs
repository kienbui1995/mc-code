use std::time::Duration;

use crate::error::ProviderError;
use crate::types::{
    CompletionRequest, ContentBlock, MessageRole, ModelInfo, ProviderEvent, TokenUsage, ToolChoice,
};

const DEFAULT_BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta";

pub struct GeminiProvider {
    http: reqwest::Client,
    api_key: String,
    base_url: String,
    max_retries: u32,
}

impl GeminiProvider {
    pub fn from_env() -> Result<Self, ProviderError> {
        let api_key = std::env::var("GEMINI_API_KEY")
            .or_else(|_| std::env::var("GOOGLE_API_KEY"))
            .map_err(|_| ProviderError::MissingApiKey {
                env_var: "GEMINI_API_KEY".into(),
            })?;
        Ok(Self {
            http: reqwest::Client::new(),
            api_key,
            base_url: DEFAULT_BASE_URL.into(),
            max_retries: 2,
        })
    }

    #[must_use]
    pub fn model_info(model: &str) -> ModelInfo {
        ModelInfo {
            name: model.into(),
            provider: "gemini".into(),
            context_window: 1_000_000,
        }
    }

    #[must_use]
    pub fn stream(&self, request: &CompletionRequest) -> crate::ProviderStream {
        let body = build_body(request);
        let url = format!(
            "{}/models/{}:streamGenerateContent?alt=sse&key={}",
            self.base_url.trim_end_matches('/'),
            request.model,
            self.api_key
        );
        let http = self.http.clone();
        let max_retries = self.max_retries;

        Box::pin(async_stream::try_stream! {
            let response = send_with_retry(&http, &url, &body, max_retries).await?;
            let mut buf = String::new();
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
                            yield ProviderEvent::MessageStop;
                            return;
                        }

                        let v: serde_json::Value = match serde_json::from_str(data) {
                            Ok(v) => v,
                            Err(_) => continue,
                        };

                        // Parse Gemini response format
                        if let Some(candidates) = v.get("candidates").and_then(|c| c.as_array()) {
                            for candidate in candidates {
                                if let Some(content) = candidate.get("content") {
                                    if let Some(parts) = content.get("parts").and_then(|p| p.as_array()) {
                                        for part in parts {
                                            if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                                                if !text.is_empty() {
                                                    yield ProviderEvent::TextDelta(text.to_string());
                                                }
                                            }
                                            if let Some(fc) = part.get("functionCall") {
                                                let name = fc.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
                                                let args = fc.get("args").map_or(String::new(), std::string::ToString::to_string);
                                                let id = format!("gemini_{name}_{}", rand_id());
                                                yield ProviderEvent::ToolUse { id, name, input: args };
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Usage metadata
                        if let Some(meta) = v.get("usageMetadata") {
                            yield ProviderEvent::Usage(TokenUsage {
                                input_tokens: meta.get("promptTokenCount").and_then(serde_json::Value::as_u64).unwrap_or(0) as u32,
                                output_tokens: meta.get("candidatesTokenCount").and_then(serde_json::Value::as_u64).unwrap_or(0) as u32,
                                ..Default::default()
                            });
                        }
                    }
                }
            }

            yield ProviderEvent::MessageStop;
        })
    }
}

fn build_body(req: &CompletionRequest) -> serde_json::Value {
    let mut contents: Vec<serde_json::Value> = Vec::new();

    for msg in &req.messages {
        let role = match msg.role {
            MessageRole::User | MessageRole::Tool => "user",
            MessageRole::Assistant => "model",
        };

        let parts: Vec<serde_json::Value> = msg.content.iter().map(|block| match block {
            ContentBlock::Text { text } => serde_json::json!({"text": text}),
            ContentBlock::ToolUse { name, input, .. } => {
                let args: serde_json::Value = serde_json::from_str(input)
                    .unwrap_or(serde_json::json!({}));
                serde_json::json!({"functionCall": {"name": name, "args": args}})
            }
            ContentBlock::ToolResult { tool_use_id: _, output, .. } => {
                serde_json::json!({"functionResponse": {"name": "tool", "response": {"result": output}}})
            }
            ContentBlock::Image { data, media_type } => {
                serde_json::json!({"inlineData": {"mimeType": media_type, "data": data}})
            }
            ContentBlock::Thinking { .. } => serde_json::json!({}), // Gemini doesn't support thinking blocks
        }).collect();

        contents.push(serde_json::json!({"role": role, "parts": parts}));
    }

    let mut body = serde_json::json!({
        "contents": contents,
        "generationConfig": {
            "maxOutputTokens": req.max_tokens,
        }
    });

    if let Some(ref sys) = req.system_prompt {
        body["systemInstruction"] = serde_json::json!({
            "parts": [{"text": sys}]
        });
    }

    if !req.tools.is_empty() {
        let decls: Vec<serde_json::Value> = req
            .tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.input_schema,
                })
            })
            .collect();
        body["tools"] = serde_json::json!([{"functionDeclarations": decls}]);

        if let Some(ref choice) = req.tool_choice {
            body["toolConfig"] = match choice {
                ToolChoice::Auto => serde_json::json!({"functionCallingConfig": {"mode": "AUTO"}}),
                ToolChoice::Any => serde_json::json!({"functionCallingConfig": {"mode": "ANY"}}),
                ToolChoice::Tool { name } => {
                    serde_json::json!({"functionCallingConfig": {"mode": "ANY", "allowedFunctionNames": [name]}})
                }
            };
        }
    }

    body
}

async fn send_with_retry(
    http: &reqwest::Client,
    url: &str,
    body: &serde_json::Value,
    max_retries: u32,
) -> Result<reqwest::Response, ProviderError> {
    let mut last_err: Option<ProviderError> = None;
    for attempt in 0..=max_retries {
        match http.post(url).json(body).send().await {
            Ok(r) if r.status().is_success() => return Ok(r),
            Ok(r) => {
                let status = r.status().as_u16();
                let text = r.text().await.unwrap_or_default();
                let retryable = matches!(status, 429 | 500 | 502 | 503);
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

fn rand_id() -> u32 {
    use std::time::SystemTime;
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_body_with_tools() {
        use crate::types::{InputMessage, ToolDefinition};
        let req = CompletionRequest {
            model: "gemini-2.5-flash".into(),
            max_tokens: 1024,
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
        };
        let body = build_body(&req);
        assert!(body.get("systemInstruction").is_some());
        assert!(body.get("tools").is_some());
        assert_eq!(body["contents"][0]["role"], "user");
    }

    #[test]
    fn model_info_gemini() {
        let info = GeminiProvider::model_info("gemini-2.5-flash");
        assert_eq!(info.context_window, 1_000_000);
        assert_eq!(info.provider, "gemini");
    }
}
