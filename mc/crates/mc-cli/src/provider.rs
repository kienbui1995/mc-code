use anyhow::{bail, Context, Result};
use mc_core::LlmProvider;
use std::sync::Arc;

/// Provider that falls back to a secondary when primary fails before producing data.
pub struct FallbackProvider {
    primary: Arc<dyn LlmProvider>,
    fallback: Arc<dyn LlmProvider>,
}

impl FallbackProvider {
    pub fn new(primary: Box<dyn LlmProvider>, fallback: Box<dyn LlmProvider>) -> Self {
        Self {
            primary: Arc::from(primary),
            fallback: Arc::from(fallback),
        }
    }
}

impl LlmProvider for FallbackProvider {
    fn stream(&self, request: &mc_provider::CompletionRequest) -> mc_provider::ProviderStream {
        let primary = Arc::clone(&self.primary);
        let fallback = Arc::clone(&self.fallback);
        let req = request.clone();
        Box::pin(async_stream::stream! {
            let mut primary_stream = primary.stream(&req);
            match next_item(&mut primary_stream).await {
                Some(Err(e)) if e.is_retryable() => {
                    drop(primary_stream);
                    let mut fb_stream = fallback.stream(&req);
                    while let Some(item) = next_item(&mut fb_stream).await {
                        yield item;
                    }
                }
                Some(item) => {
                    yield item;
                    while let Some(item) = next_item(&mut primary_stream).await {
                        yield item;
                    }
                }
                None => {}
            }
        })
    }
}

async fn next_item(
    stream: &mut mc_provider::ProviderStream,
) -> Option<Result<mc_provider::ProviderEvent, mc_provider::ProviderError>> {
    std::future::poll_fn(|cx| stream.as_mut().poll_next(cx)).await
}

/// Auto-detect provider from model name.
pub fn detect_provider(model: &str) -> Option<String> {
    let m = model.to_lowercase();
    // Order matters: more specific prefixes first
    if m.starts_with("claude") || m.starts_with("bedrock/claude") {
        Some("anthropic".into())
    } else if m.starts_with("gpt") || m.starts_with("o3") || m.starts_with("o4") {
        Some("openai".into())
    } else if m.starts_with("gemini") {
        Some("gemini".into())
    } else if m.starts_with("deepseek") {
        Some("deepseek".into())
    } else if m.starts_with("mistral") {
        Some("mistral".into())
    } else if m.starts_with("grok") {
        Some("xai".into())
    } else if m.starts_with("command-r") || m.starts_with("command-a") {
        Some("cohere".into())
    } else if m.starts_with("sonar") {
        Some("perplexity".into())
    } else if m.starts_with("llama")
        || m.starts_with("codellama")
        || m.starts_with("phi")
        || m.starts_with("qwen")
    {
        Some("ollama".into())
    } else if m.contains('/') {
        Some("openrouter".into())
    } else {
        None
    }
}

/// Resolve api key.
pub fn resolve_api_key(config: &mc_config::ProviderConfig) -> Option<String> {
    if config.api_key_env.is_empty() {
        return None;
    }
    std::env::var(&config.api_key_env)
        .ok()
        .filter(|k| !k.is_empty())
}

/// Create a provider from name + config. Returns a boxed trait object.
pub fn create_provider(
    name: &str,
    config: &mc_config::ProviderConfig,
    cli_base_url: Option<&str>,
    cli_api_key: Option<&str>,
) -> Result<Box<dyn LlmProvider>> {
    if let Some(base_url) = cli_base_url {
        return Ok(Box::new(mc_provider::GenericProvider::new(
            base_url.to_string(),
            cli_api_key.map(String::from),
        )));
    }

    let format = config.format.as_deref().unwrap_or("");

    match name {
        "anthropic" if format != "openai-compatible" => Ok(Box::new(
            mc_provider::AnthropicProvider::from_env().context("set ANTHROPIC_API_KEY")?,
        )),
        "openai" => {
            let key = cli_api_key
                .map(String::from)
                .or_else(|| {
                    std::env::var("OPENAI_API_KEY")
                        .ok()
                        .filter(|k| !k.is_empty())
                })
                .context("set OPENAI_API_KEY")?;
            Ok(Box::new(mc_provider::GenericProvider::new(
                "https://api.openai.com".into(),
                Some(key),
            )))
        }
        "ollama" => Ok(Box::new(mc_provider::GenericProvider::ollama())),
        "gemini" => Ok(Box::new(
            mc_provider::GeminiProvider::from_env().context("set GEMINI_API_KEY")?,
        )),
        "litellm" => {
            let base = config
                .base_url
                .clone()
                .or_else(|| std::env::var("LITELLM_BASE_URL").ok())
                .unwrap_or_else(|| "http://localhost:4000".to_string());
            let key = cli_api_key
                .map(String::from)
                .or_else(|| resolve_api_key(config));
            Ok(Box::new(mc_provider::GenericProvider::new(base, key)))
        }
        "groq" => {
            let key = cli_api_key
                .map(String::from)
                .or_else(|| std::env::var("GROQ_API_KEY").ok().filter(|k| !k.is_empty()))
                .context("set GROQ_API_KEY")?;
            Ok(Box::new(mc_provider::GenericProvider::new(
                "https://api.groq.com/openai".into(),
                Some(key),
            )))
        }
        "deepseek" => {
            let key = cli_api_key
                .map(String::from)
                .or_else(|| {
                    std::env::var("DEEPSEEK_API_KEY")
                        .ok()
                        .filter(|k| !k.is_empty())
                })
                .context("set DEEPSEEK_API_KEY")?;
            Ok(Box::new(mc_provider::GenericProvider::new(
                "https://api.deepseek.com".into(),
                Some(key),
            )))
        }
        "mistral" => {
            let key = cli_api_key
                .map(String::from)
                .or_else(|| {
                    std::env::var("MISTRAL_API_KEY")
                        .ok()
                        .filter(|k| !k.is_empty())
                })
                .context("set MISTRAL_API_KEY")?;
            Ok(Box::new(mc_provider::GenericProvider::new(
                "https://api.mistral.ai".into(),
                Some(key),
            )))
        }
        "xai" => {
            let key = cli_api_key
                .map(String::from)
                .or_else(|| std::env::var("XAI_API_KEY").ok().filter(|k| !k.is_empty()))
                .context("set XAI_API_KEY")?;
            Ok(Box::new(mc_provider::GenericProvider::new(
                "https://api.x.ai".into(),
                Some(key),
            )))
        }
        "openrouter" => {
            let key = cli_api_key
                .map(String::from)
                .or_else(|| {
                    std::env::var("OPENROUTER_API_KEY")
                        .ok()
                        .filter(|k| !k.is_empty())
                })
                .context("set OPENROUTER_API_KEY")?;
            Ok(Box::new(mc_provider::GenericProvider::new(
                "https://openrouter.ai/api".into(),
                Some(key),
            )))
        }
        "together" => {
            let key = cli_api_key
                .map(String::from)
                .or_else(|| {
                    std::env::var("TOGETHER_API_KEY")
                        .ok()
                        .filter(|k| !k.is_empty())
                })
                .context("set TOGETHER_API_KEY")?;
            Ok(Box::new(mc_provider::GenericProvider::new(
                "https://api.together.xyz".into(),
                Some(key),
            )))
        }
        "perplexity" => {
            let key = cli_api_key
                .map(String::from)
                .or_else(|| {
                    std::env::var("PERPLEXITY_API_KEY")
                        .ok()
                        .filter(|k| !k.is_empty())
                })
                .context("set PERPLEXITY_API_KEY")?;
            Ok(Box::new(mc_provider::GenericProvider::new(
                "https://api.perplexity.ai".into(),
                Some(key),
            )))
        }
        "cohere" => {
            let key = cli_api_key
                .map(String::from)
                .or_else(|| {
                    std::env::var("COHERE_API_KEY")
                        .ok()
                        .filter(|k| !k.is_empty())
                })
                .context("set COHERE_API_KEY")?;
            Ok(Box::new(mc_provider::GenericProvider::new(
                "https://api.cohere.com/v2".into(),
                Some(key),
            )))
        }
        "cerebras" => {
            let key = cli_api_key
                .map(String::from)
                .or_else(|| {
                    std::env::var("CEREBRAS_API_KEY")
                        .ok()
                        .filter(|k| !k.is_empty())
                })
                .context("set CEREBRAS_API_KEY")?;
            Ok(Box::new(mc_provider::GenericProvider::new(
                "https://api.cerebras.ai".into(),
                Some(key),
            )))
        }
        "lmstudio" => {
            let host =
                std::env::var("LM_STUDIO_HOST").unwrap_or_else(|_| "http://localhost:1234".into());
            Ok(Box::new(mc_provider::GenericProvider::new(host, None)))
        }
        "llamacpp" => {
            let host =
                std::env::var("LLAMA_CPP_HOST").unwrap_or_else(|_| "http://localhost:8080".into());
            Ok(Box::new(mc_provider::GenericProvider::new(host, None)))
        }
        _ => {
            if let Some(base) = &config.base_url {
                let key = cli_api_key
                    .map(String::from)
                    .or_else(|| resolve_api_key(config));
                Ok(Box::new(mc_provider::GenericProvider::new(
                    base.clone(),
                    key,
                )))
            } else {
                bail!("unknown provider '{name}'. Set base_url in config or use --base-url flag.")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mc_provider::{CompletionRequest, ProviderError, ProviderEvent, ProviderStream};
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    struct MockProvider {
        scripts: Mutex<VecDeque<Vec<Result<ProviderEvent, ProviderError>>>>,
        calls: AtomicUsize,
    }

    impl MockProvider {
        fn new(scripts: Vec<Vec<Result<ProviderEvent, ProviderError>>>) -> Self {
            Self {
                scripts: Mutex::new(scripts.into()),
                calls: AtomicUsize::new(0),
            }
        }

        fn call_count(&self) -> usize {
            self.calls.load(Ordering::Relaxed)
        }
    }

    impl LlmProvider for MockProvider {
        fn stream(&self, _request: &CompletionRequest) -> ProviderStream {
            self.calls.fetch_add(1, Ordering::Relaxed);
            let events = self
                .scripts
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| vec![Ok(ProviderEvent::MessageStop)]);
            Box::pin(async_stream::stream! {
                for ev in events {
                    yield ev;
                }
            })
        }
    }

    fn empty_request() -> CompletionRequest {
        CompletionRequest {
            model: "test".into(),
            max_tokens: 16,
            system_prompt: None,
            messages: Vec::new(),
            tools: Vec::new(),
            tool_choice: None,
            thinking_budget: None,
            response_format: None,
        }
    }

    async fn collect(stream: ProviderStream) -> Vec<Result<ProviderEvent, ProviderError>> {
        let mut stream = stream;
        let mut out = Vec::new();
        while let Some(item) = next_item(&mut stream).await {
            out.push(item);
        }
        out
    }

    fn retryable_api_err() -> ProviderError {
        ProviderError::Api {
            status: 503,
            error_type: None,
            message: "overloaded".into(),
            retryable: true,
        }
    }

    #[tokio::test]
    async fn fallback_used_when_primary_errors_first() {
        let primary = Arc::new(MockProvider::new(vec![vec![Err(retryable_api_err())]]));
        let fallback = Arc::new(MockProvider::new(vec![vec![
            Ok(ProviderEvent::TextDelta("from-fallback".into())),
            Ok(ProviderEvent::MessageStop),
        ]]));
        let fp = FallbackProvider {
            primary: primary.clone() as Arc<dyn LlmProvider>,
            fallback: fallback.clone() as Arc<dyn LlmProvider>,
        };
        let events = collect(fp.stream(&empty_request())).await;
        let texts: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                Ok(ProviderEvent::TextDelta(s)) => Some(s.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(texts, vec!["from-fallback"]);
        assert_eq!(primary.call_count(), 1);
        assert_eq!(fallback.call_count(), 1);
    }

    #[tokio::test]
    async fn primary_used_when_it_succeeds() {
        let primary = Arc::new(MockProvider::new(vec![vec![
            Ok(ProviderEvent::TextDelta("from-primary".into())),
            Ok(ProviderEvent::MessageStop),
        ]]));
        let fallback = Arc::new(MockProvider::new(vec![vec![Ok(ProviderEvent::TextDelta(
            "should-not-appear".into(),
        ))]]));
        let fp = FallbackProvider {
            primary: primary.clone() as Arc<dyn LlmProvider>,
            fallback: fallback.clone() as Arc<dyn LlmProvider>,
        };
        let events = collect(fp.stream(&empty_request())).await;
        let texts: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                Ok(ProviderEvent::TextDelta(s)) => Some(s.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(texts, vec!["from-primary"]);
        assert_eq!(primary.call_count(), 1);
        assert_eq!(fallback.call_count(), 0);
    }

    #[tokio::test]
    async fn non_retryable_error_propagates() {
        let primary = Arc::new(MockProvider::new(vec![vec![Err(
            ProviderError::MissingApiKey {
                env_var: "NONE".into(),
            },
        )]]));
        let fallback = Arc::new(MockProvider::new(vec![vec![Ok(ProviderEvent::TextDelta(
            "should-not-appear".into(),
        ))]]));
        let fp = FallbackProvider {
            primary: primary.clone() as Arc<dyn LlmProvider>,
            fallback: fallback.clone() as Arc<dyn LlmProvider>,
        };
        let events = collect(fp.stream(&empty_request())).await;
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0],
            Err(ProviderError::MissingApiKey { .. })
        ));
        assert_eq!(fallback.call_count(), 0);
    }
}
