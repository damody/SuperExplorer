//! Owned AI request, response, usage, and error models.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// High-level operation independent from a provider wire format.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiOperation {
    Summarize,
    Chat,
}

/// Complete owned request submitted to an AI provider.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AiRequest {
    pub operation: AiOperation,
    pub provider: String,
    pub model: String,
    pub input: String,
    pub system_prompt: Option<String>,
    pub timeout_ms: u64,
    pub correlation_id: String,
}

/// Provider token accounting when returned by the API.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct AiUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cached_input_tokens: Option<u64>,
}

/// Provider-neutral final text result.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AiResponse {
    pub provider: String,
    pub model: String,
    pub text: String,
    pub usage: AiUsage,
}

/// Stable error categories used for retries and UI behavior.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiErrorKind {
    InvalidInput,
    MissingCredential,
    Authentication,
    RateLimited,
    Unavailable,
    Timeout,
    Cancelled,
    InvalidResponse,
    Internal,
}

/// Error that deliberately excludes prompt, response, and credential content.
#[derive(Clone, Debug, Eq, Error, PartialEq, Serialize, Deserialize)]
#[error("{user_message}")]
pub struct AiError {
    pub kind: AiErrorKind,
    pub recoverable: bool,
    pub user_message: String,
    pub provider: Option<String>,
    pub status_code: Option<u16>,
}

impl AiError {
    /// Creates a privacy-safe provider error.
    pub fn new(kind: AiErrorKind, recoverable: bool, user_message: impl Into<String>) -> Self {
        Self {
            kind,
            recoverable,
            user_message: user_message.into(),
            provider: None,
            status_code: None,
        }
    }
}

/// AI client result.
pub type AiResult<T> = Result<T, AiError>;
