use crate::compact::estimate_tokens;
use crate::session::Session;

/// Manages token allocation within a model's context window.
///
/// Anthropic `max_tokens` = max OUTPUT tokens. This budget ensures
/// we don't over-fill the context and dynamically adjusts response size.
pub struct TokenBudget {
    context_window: usize,
    response_reserve: usize,
}

impl TokenBudget {
    #[must_use]
    /// New.
    pub fn new(context_window: usize, response_reserve: usize) -> Self {
        Self {
            context_window,
            response_reserve,
        }
    }

    /// Tokens available for conversation history.
    /// `context_window - system_tokens - tool_schema_tokens - response_reserve`
    #[must_use]
    /// Available for messages.
    pub fn available_for_messages(&self, system_tokens: usize, tool_schema_tokens: usize) -> usize {
        self.context_window
            .saturating_sub(system_tokens)
            .saturating_sub(tool_schema_tokens)
            .saturating_sub(self.response_reserve)
    }

    /// Dynamically compute max output tokens based on how much context is used.
    /// Returns `min(response_reserve, context_window - used_context)`, clamped to ≥1.
    #[must_use]
    /// Effective max tokens.
    pub fn effective_max_tokens(&self, used_context: usize) -> u32 {
        let remaining = self.context_window.saturating_sub(used_context);
        let effective = remaining.min(self.response_reserve);
        u32::try_from(effective.max(1)).unwrap_or(u32::MAX)
    }

    /// Estimate tokens used by a session's message history.
    #[must_use]
    /// Session tokens.
    pub fn session_tokens(session: &Session) -> usize {
        estimate_tokens(session)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn available_for_messages_basic() {
        let b = TokenBudget::new(200_000, 8192);
        // 200k - 2k system - 1k tools - 8192 reserve = 188808
        assert_eq!(b.available_for_messages(2000, 1000), 188_808);
    }

    #[test]
    fn available_saturates_to_zero() {
        let b = TokenBudget::new(1000, 8192);
        assert_eq!(b.available_for_messages(500, 600), 0);
    }

    #[test]
    fn effective_max_tokens_normal() {
        let b = TokenBudget::new(200_000, 8192);
        // 200k - 100k used = 100k remaining, min(100k, 8192) = 8192
        assert_eq!(b.effective_max_tokens(100_000), 8192);
    }

    #[test]
    fn effective_max_tokens_tight_context() {
        let b = TokenBudget::new(200_000, 8192);
        // 200k - 198k = 2k remaining, min(2k, 8192) = 2000
        assert_eq!(b.effective_max_tokens(198_000), 2000);
    }

    #[test]
    fn effective_max_tokens_over_budget() {
        let b = TokenBudget::new(200_000, 8192);
        // used > context_window → remaining = 0, clamped to 1
        assert_eq!(b.effective_max_tokens(300_000), 1);
    }
}
