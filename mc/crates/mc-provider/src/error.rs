#[derive(Debug, thiserror::Error)]
/// Providererror.
pub enum ProviderError {
    #[error("[MC-E001] missing API key: set {env_var}")]
    MissingApiKey { env_var: String },

    #[error("[MC-E002] http error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("[MC-E003] api error {status}: {message}")]
    Api {
        status: u16,
        error_type: Option<String>,
        message: String,
        retryable: bool,
    },

    #[error("[MC-E004] retries exhausted after {attempts} attempts: {last_message}")]
    RetriesExhausted { attempts: u32, last_message: String },

    #[error("[MC-E005] invalid SSE frame: {0}")]
    InvalidSse(String),

    #[error("[MC-E006] json error: {0}")]
    Json(#[from] serde_json::Error),
}

impl ProviderError {
    #[must_use]
    /// Is retryable.
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Http(e) => e.is_connect() || e.is_timeout() || e.is_request(),
            Self::Api { retryable, .. } => *retryable,
            _ => false,
        }
    }

    /// Unique error ID for debugging.
    #[must_use]
    /// Error id.
    pub fn error_id(&self) -> &'static str {
        match self {
            Self::MissingApiKey { .. } => "MC-E001",
            Self::Http(_) => "MC-E002",
            Self::Api { .. } => "MC-E003",
            Self::RetriesExhausted { .. } => "MC-E004",
            Self::InvalidSse(_) => "MC-E005",
            Self::Json(_) => "MC-E006",
        }
    }
}
