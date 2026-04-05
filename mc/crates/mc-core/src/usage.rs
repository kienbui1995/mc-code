use mc_provider::TokenUsage;

#[derive(Debug, Clone, Default)]
pub struct UsageTracker {
    turns: usize,
    total: TokenUsage,
}

impl UsageTracker {
    pub fn record(&mut self, usage: &TokenUsage) {
        self.turns += 1;
        self.total.input_tokens += usage.input_tokens;
        self.total.output_tokens += usage.output_tokens;
        self.total.cache_creation_input_tokens += usage.cache_creation_input_tokens;
        self.total.cache_read_input_tokens += usage.cache_read_input_tokens;
    }

    #[must_use]
    pub fn turns(&self) -> usize {
        self.turns
    }

    #[must_use]
    pub fn total(&self) -> &TokenUsage {
        &self.total
    }

    /// Tokens served from prompt cache (90% cost savings on these).
    #[must_use]
    pub fn cache_read_tokens(&self) -> u32 {
        self.total.cache_read_input_tokens
    }

    /// Tokens written to prompt cache (25% surcharge on first write).
    #[must_use]
    pub fn cache_creation_tokens(&self) -> u32 {
        self.total.cache_creation_input_tokens
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_cumulative_usage() {
        let mut tracker = UsageTracker::default();
        tracker.record(&TokenUsage {
            input_tokens: 10,
            output_tokens: 5,
            ..Default::default()
        });
        tracker.record(&TokenUsage {
            input_tokens: 20,
            output_tokens: 15,
            ..Default::default()
        });
        assert_eq!(tracker.turns(), 2);
        assert_eq!(tracker.total().input_tokens, 30);
        assert_eq!(tracker.total().output_tokens, 20);
    }
}
