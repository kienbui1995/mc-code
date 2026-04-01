#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("missing API key: set {env_var}")]
    MissingApiKey { env_var: String },

    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("api error {status}: {message}")]
    Api {
        status: u16,
        error_type: Option<String>,
        message: String,
        retryable: bool,
    },

    #[error("retries exhausted after {attempts} attempts: {last_message}")]
    RetriesExhausted { attempts: u32, last_message: String },

    #[error("invalid SSE frame: {0}")]
    InvalidSse(String),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

impl ProviderError {
    #[must_use] 
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Http(e) => e.is_connect() || e.is_timeout() || e.is_request(),
            Self::Api { retryable, .. } => *retryable,
            _ => false,
        }
    }
}
