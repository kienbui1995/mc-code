use anyhow::{bail, Context, Result};
use mc_core::LlmProvider;

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
    } else if m.starts_with("llama")
        || m.starts_with("codellama")
        || m.starts_with("deepseek")
        || m.starts_with("mistral")
        || m.starts_with("phi")
        || m.starts_with("qwen")
    {
        Some("ollama".into())
    } else if m.contains('/') {
        // Catch-all for litellm-style "provider/model" format
        Some("litellm".into())
    } else {
        None
    }
}

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
