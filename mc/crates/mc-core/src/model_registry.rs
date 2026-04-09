use std::collections::HashMap;

/// Known model metadata for context window sizing and cost estimation.
pub struct ModelRegistry {
    models: HashMap<String, ModelMeta>,
}

#[derive(Debug, Clone)]
/// Modelmeta.
pub struct ModelMeta {
    pub context_window: u32,
    pub supports_tools: bool,
    /// Cost per million tokens (input, output).
    pub cost_per_mtok: (f64, f64),
}

impl Default for ModelRegistry {
    fn default() -> Self {
        let mut models = HashMap::new();
        let m = |ctx, tools, input_cost, output_cost| ModelMeta {
            context_window: ctx,
            supports_tools: tools,
            cost_per_mtok: (input_cost, output_cost),
        };

        // Anthropic
        models.insert(
            "claude-opus-4-20250514".into(),
            m(200_000, true, 15.0, 75.0),
        );
        models.insert(
            "claude-sonnet-4-20250514".into(),
            m(200_000, true, 3.0, 15.0),
        );
        models.insert(
            "claude-haiku-3-5-20241022".into(),
            m(200_000, true, 0.8, 4.0),
        );

        // OpenAI
        models.insert("gpt-4o".into(), m(128_000, true, 2.5, 10.0));
        models.insert("gpt-4o-mini".into(), m(128_000, true, 0.15, 0.6));
        models.insert("o3".into(), m(200_000, true, 10.0, 40.0));
        models.insert("o3-mini".into(), m(200_000, true, 1.1, 4.4));
        models.insert("o4-mini".into(), m(200_000, true, 1.1, 4.4));

        // Gemini
        models.insert("gemini-2.5-pro".into(), m(1_000_000, true, 1.25, 10.0));
        models.insert("gemini-2.5-flash".into(), m(1_000_000, true, 0.15, 0.6));

        // Local / open
        models.insert("llama3".into(), m(8_192, true, 0.0, 0.0));
        models.insert("llama3:70b".into(), m(8_192, true, 0.0, 0.0));
        models.insert("codellama".into(), m(16_384, false, 0.0, 0.0));
        models.insert("deepseek-coder".into(), m(128_000, true, 0.0, 0.0));

        // Groq
        models.insert("llama-3.3-70b-versatile".into(), m(128_000, true, 0.59, 0.79));
        models.insert("llama-3.1-8b-instant".into(), m(128_000, true, 0.05, 0.08));

        // DeepSeek
        models.insert("deepseek-chat".into(), m(128_000, true, 0.14, 0.28));
        models.insert("deepseek-reasoner".into(), m(128_000, true, 0.55, 2.19));

        // Mistral
        models.insert("mistral-large-latest".into(), m(128_000, true, 2.0, 6.0));
        models.insert("mistral-small-latest".into(), m(128_000, true, 0.1, 0.3));

        // xAI
        models.insert("grok-2".into(), m(131_072, true, 2.0, 10.0));
        models.insert("grok-3-mini".into(), m(131_072, true, 0.3, 0.5));

        // OpenRouter (pass-through, use provider pricing)
        models.insert("anthropic/claude-sonnet-4".into(), m(200_000, true, 3.0, 15.0));
        models.insert("meta-llama/llama-3.3-70b-instruct".into(), m(128_000, true, 0.59, 0.79));

        // Together
        models.insert("meta-llama/Llama-3.3-70B-Instruct-Turbo".into(), m(128_000, true, 0.88, 0.88));

        // Perplexity
        models.insert("sonar-pro".into(), m(200_000, true, 3.0, 15.0));
        models.insert("sonar".into(), m(128_000, true, 1.0, 1.0));

        // Cohere
        models.insert("command-r-plus".into(), m(128_000, true, 2.5, 10.0));
        models.insert("command-r".into(), m(128_000, true, 0.15, 0.6));

        // Cerebras
        models.insert("llama3.1-70b".into(), m(128_000, true, 0.0, 0.0));

        Self { models }
    }
}

impl ModelRegistry {
    #[must_use]
    /// Context window.
    pub fn context_window(&self, model: &str) -> u32 {
        self.lookup(model).map_or(128_000, |m| m.context_window)
    }

    #[must_use]
    /// Supports tools.
    pub fn supports_tools(&self, model: &str) -> bool {
        self.lookup(model).is_none_or(|m| m.supports_tools)
    }

    /// Returns (`input_cost`, `output_cost`) per million tokens.
    #[must_use]
    /// Cost per mtok.
    pub fn cost_per_mtok(&self, model: &str) -> (f64, f64) {
        self.lookup(model).map_or((0.0, 0.0), |m| m.cost_per_mtok)
    }

    /// Estimate cost in USD for given token counts.
    #[must_use]
    /// Estimate cost.
    pub fn estimate_cost(&self, model: &str, input_tokens: u32, output_tokens: u32) -> f64 {
        let (ic, oc) = self.cost_per_mtok(model);
        f64::from(input_tokens) * ic / 1_000_000.0 + f64::from(output_tokens) * oc / 1_000_000.0
    }

    fn lookup(&self, model: &str) -> Option<&ModelMeta> {
        self.models.get(model).or_else(|| {
            // Fuzzy match: "claude-sonnet-4-20250514" should match if user passes "claude-sonnet-4-20250514"
            // Also try prefix match for versioned models
            self.models
                .iter()
                .find(|(k, _)| model.starts_with(k.as_str()))
                .map(|(_, v)| v)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_models() {
        let r = ModelRegistry::default();
        assert_eq!(r.context_window("claude-sonnet-4-20250514"), 200_000);
        assert_eq!(r.context_window("gpt-4o"), 128_000);
        assert_eq!(r.context_window("llama3"), 8_192);
    }

    #[test]
    fn unknown_model_defaults() {
        let r = ModelRegistry::default();
        assert_eq!(r.context_window("some-unknown-model"), 128_000);
        assert!(r.supports_tools("some-unknown-model"));
    }

    #[test]
    fn cost_estimation() {
        let r = ModelRegistry::default();
        let cost = r.estimate_cost("claude-sonnet-4-20250514", 1_000_000, 100_000);
        // 1M input * $3/M + 100K output * $15/M = $3 + $1.5 = $4.5
        assert!((cost - 4.5).abs() < 0.01);
    }
}
