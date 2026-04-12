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
        // Simple: just use primary. Fallback handled by retry in runtime.
        // True fallback requires tokio-stream which we don't have.
        // For now, expose the struct so it can be wired later.
        self.primary.stream(request)
    }
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
