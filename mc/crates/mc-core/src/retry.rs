use std::time::Duration;

use mc_provider::ProviderError;

/// Retry policy for mid-stream failures.
///
/// Provider-level retry handles connection/initial response.
/// This policy handles stream interruptions after partial data.
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub initial_backoff_ms: u64,
    pub max_backoff_ms: u64,
}

impl RetryPolicy {
    #[must_use]
    pub fn new(max_attempts: u32, initial_backoff_ms: u64, max_backoff_ms: u64) -> Self {
        Self {
            max_attempts,
            initial_backoff_ms,
            max_backoff_ms,
        }
    }

    #[must_use]
    pub fn should_retry(&self, error: &ProviderError, attempt: u32) -> bool {
        attempt < self.max_attempts && error.is_retryable()
    }

    #[must_use]
    pub fn backoff_duration(&self, attempt: u32) -> Duration {
        let multiplier = 1u64.checked_shl(attempt).unwrap_or(u64::MAX);
        let ms = self
            .initial_backoff_ms
            .saturating_mul(multiplier)
            .min(self.max_backoff_ms);
        Duration::from_millis(ms)
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 2,
            initial_backoff_ms: 500,
            max_backoff_ms: 5000,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_retry_retryable_error() {
        let policy = RetryPolicy::default();
        let err = ProviderError::Api {
            status: 429,
            error_type: None,
            message: "rate limit".into(),
            retryable: true,
        };
        assert!(policy.should_retry(&err, 0));
        assert!(policy.should_retry(&err, 1));
        assert!(!policy.should_retry(&err, 2)); // max_attempts = 2
    }

    #[test]
    fn should_not_retry_non_retryable() {
        let policy = RetryPolicy::default();
        let err = ProviderError::Api {
            status: 401,
            error_type: None,
            message: "unauthorized".into(),
            retryable: false,
        };
        assert!(!policy.should_retry(&err, 0));
    }

    #[test]
    fn backoff_exponential() {
        let policy = RetryPolicy::new(3, 500, 5000);
        assert_eq!(policy.backoff_duration(0), Duration::from_millis(500));
        assert_eq!(policy.backoff_duration(1), Duration::from_millis(1000));
        assert_eq!(policy.backoff_duration(2), Duration::from_millis(2000));
        assert_eq!(policy.backoff_duration(3), Duration::from_millis(4000));
        assert_eq!(policy.backoff_duration(4), Duration::from_millis(5000)); // capped
    }

    #[test]
    fn backoff_caps_at_max() {
        let policy = RetryPolicy::new(5, 1000, 3000);
        assert_eq!(policy.backoff_duration(10), Duration::from_millis(3000));
    }
}
