mod anthropic;
mod error;
mod gemini;
mod generic;
mod sse;
pub mod types;

pub use anthropic::AnthropicProvider;
pub use error::ProviderError;
pub use gemini::GeminiProvider;
pub use generic::GenericProvider;
pub use types::{
    CompletionRequest, ContentBlock, InputMessage, MessageRole, ModelInfo, ProviderEvent,
    ResponseFormat, TokenUsage, ToolChoice, ToolDefinition,
};

use std::pin::Pin;

/// A stream of provider events.
pub type ProviderStream =
    Pin<Box<dyn futures_core::Stream<Item = Result<ProviderEvent, ProviderError>> + Send>>;
