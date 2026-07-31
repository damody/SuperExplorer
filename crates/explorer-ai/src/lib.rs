#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )
)]
//! Provider-neutral AI boundary for Explorer automation.

pub mod client;
pub mod deepseek;
pub mod model;

pub use client::{AiCancellation, AiClient, AiFuture, AiStreamCallback, FakeAiClient};
pub use deepseek::{DEEPSEEK_DEFAULT_MODEL, DeepSeekClient};
pub use model::{AiError, AiErrorKind, AiOperation, AiRequest, AiResponse, AiResult, AiUsage};
