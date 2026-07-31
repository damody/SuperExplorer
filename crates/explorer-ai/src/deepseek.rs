//! `DeepSeek` OpenAI-compatible chat completion client.

use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll, Waker},
    thread,
    time::Duration,
};

use reqwest::StatusCode;
use serde::{Deserialize, Serialize};

use crate::{
    AiCancellation, AiClient, AiError, AiErrorKind, AiFuture, AiRequest, AiResponse, AiUsage,
};

pub const DEEPSEEK_DEFAULT_MODEL: &str = "deepseek-v4-flash";
const DEEPSEEK_CHAT_URL: &str = "https://api.deepseek.com/chat/completions";

/// Privacy-safe `DeepSeek` client; debug output never exposes its API key.
#[derive(Clone)]
pub struct DeepSeekClient {
    http: reqwest::blocking::Client,
    api_key: String,
    endpoint: String,
    max_retries: usize,
}

impl DeepSeekClient {
    /// Creates a production client. The key remains only in owned process memory.
    ///
    /// # Errors
    ///
    /// Returns `MissingCredential` for an empty key.
    pub fn new(api_key: impl Into<String>) -> Result<Self, AiError> {
        let api_key = api_key.into();
        if api_key.trim().is_empty() {
            return Err(AiError::new(
                AiErrorKind::MissingCredential,
                false,
                "A DeepSeek API credential is required",
            ));
        }
        Ok(Self {
            http: reqwest::blocking::Client::new(),
            api_key,
            endpoint: DEEPSEEK_CHAT_URL.into(),
            max_retries: 2,
        })
    }

    #[cfg(test)]
    fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = endpoint.into();
        self
    }
}

impl std::fmt::Debug for DeepSeekClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DeepSeekClient")
            .field("endpoint", &self.endpoint)
            .field("max_retries", &self.max_retries)
            .finish_non_exhaustive()
    }
}

impl AiClient for DeepSeekClient {
    fn execute_cancellable(
        &self,
        request: AiRequest,
        cancellation: AiCancellation,
    ) -> AiFuture<AiResponse> {
        DeepSeekFuture::spawn(self.clone(), request, cancellation)
    }
}

struct DeepSeekFutureState {
    result: Option<Result<AiResponse, AiError>>,
    waker: Option<Waker>,
}

struct DeepSeekFuture {
    state: Arc<Mutex<DeepSeekFutureState>>,
}

impl DeepSeekFuture {
    fn spawn(
        client: DeepSeekClient,
        request: AiRequest,
        cancellation: AiCancellation,
    ) -> AiFuture<AiResponse> {
        let state = Arc::new(Mutex::new(DeepSeekFutureState {
            result: None,
            waker: None,
        }));
        let worker_state = Arc::clone(&state);
        thread::spawn(move || {
            let result = client.run_request(&request, &cancellation);
            if let Ok(mut state) = worker_state.lock() {
                state.result = Some(result);
                if let Some(waker) = state.waker.take() {
                    waker.wake();
                }
            }
        });
        Box::pin(Self { state })
    }
}

impl Future for DeepSeekFuture {
    type Output = Result<AiResponse, AiError>;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let Ok(mut state) = self.state.lock() else {
            return Poll::Ready(Err(AiError::new(
                AiErrorKind::Internal,
                false,
                "The DeepSeek request state is unavailable",
            )));
        };
        if let Some(result) = state.result.take() {
            Poll::Ready(result)
        } else {
            state.waker = Some(context.waker().clone());
            Poll::Pending
        }
    }
}

impl DeepSeekClient {
    fn run_request(
        &self,
        request: &AiRequest,
        cancellation: &AiCancellation,
    ) -> Result<AiResponse, AiError> {
        if cancellation.is_cancelled() {
            return Err(cancelled());
        }
        if request.provider != "deepseek" || request.input.trim().is_empty() {
            return Err(AiError::new(
                AiErrorKind::InvalidInput,
                false,
                "The DeepSeek request is invalid",
            ));
        }
        let model = if request.model.trim().is_empty() {
            DEEPSEEK_DEFAULT_MODEL
        } else {
            request.model.as_str()
        };
        let payload = ChatRequest::from_request(request, model);
        for attempt in 0..=self.max_retries {
            if cancellation.is_cancelled() {
                return Err(cancelled());
            }
            let response = self
                .http
                .post(&self.endpoint)
                .bearer_auth(&self.api_key)
                .timeout(Duration::from_millis(request.timeout_ms.max(1)))
                .json(&payload)
                .send()
                .map_err(|error| map_transport(&error))?;
            let status = response.status();
            if status.is_success() {
                let body: ChatResponse = response.json().map_err(|_| {
                    AiError::new(
                        AiErrorKind::InvalidResponse,
                        false,
                        "DeepSeek returned an invalid response",
                    )
                })?;
                if cancellation.is_cancelled() {
                    return Err(cancelled());
                }
                return body.into_ai_response();
            }
            let error = map_status(status);
            if !error.recoverable || attempt == self.max_retries {
                return Err(error);
            }
        }
        Err(AiError::new(
            AiErrorKind::Internal,
            false,
            "The DeepSeek request did not complete",
        ))
    }
}

#[derive(Debug, Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<ChatMessage<'a>>,
    stream: bool,
    thinking: ThinkingMode,
}

impl<'a> ChatRequest<'a> {
    fn from_request(request: &'a AiRequest, model: &'a str) -> Self {
        let mut messages = Vec::with_capacity(2);
        if let Some(system) = request.system_prompt.as_deref() {
            messages.push(ChatMessage {
                role: "system",
                content: system,
            });
        }
        messages.push(ChatMessage {
            role: "user",
            content: &request.input,
        });
        Self {
            model,
            messages,
            stream: false,
            thinking: ThinkingMode { kind: "disabled" },
        }
    }
}

#[derive(Debug, Serialize)]
struct ChatMessage<'a> {
    role: &'static str,
    content: &'a str,
}

#[derive(Debug, Serialize)]
struct ThinkingMode {
    #[serde(rename = "type")]
    kind: &'static str,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    model: String,
    choices: Vec<ChatChoice>,
    #[serde(default)]
    usage: ChatUsage,
}

impl ChatResponse {
    fn into_ai_response(self) -> Result<AiResponse, AiError> {
        let text = self
            .choices
            .into_iter()
            .next()
            .and_then(|choice| choice.message.content)
            .filter(|text| !text.is_empty())
            .ok_or_else(|| {
                AiError::new(
                    AiErrorKind::InvalidResponse,
                    false,
                    "DeepSeek returned no summary text",
                )
            })?;
        Ok(AiResponse {
            provider: "deepseek".into(),
            model: self.model,
            text,
            usage: AiUsage {
                input_tokens: self.usage.prompt,
                output_tokens: self.usage.completion,
                cached_input_tokens: self.usage.prompt_cache_hit,
            },
        })
    }
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatResponseMessage,
}

#[derive(Debug, Deserialize)]
struct ChatResponseMessage {
    content: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ChatUsage {
    #[serde(rename = "prompt_tokens")]
    prompt: Option<u64>,
    #[serde(rename = "completion_tokens")]
    completion: Option<u64>,
    #[serde(rename = "prompt_cache_hit_tokens")]
    prompt_cache_hit: Option<u64>,
}

fn map_transport(error: &reqwest::Error) -> AiError {
    if error.is_timeout() {
        AiError::new(AiErrorKind::Timeout, true, "The DeepSeek request timed out")
    } else {
        AiError::new(
            AiErrorKind::Unavailable,
            true,
            "DeepSeek is currently unavailable",
        )
    }
}

fn map_status(status: StatusCode) -> AiError {
    let (kind, recoverable, message) = match status.as_u16() {
        401 | 403 => (
            AiErrorKind::Authentication,
            false,
            "DeepSeek rejected the credential",
        ),
        429 => (
            AiErrorKind::RateLimited,
            true,
            "DeepSeek rate limited the request",
        ),
        500..=599 => (
            AiErrorKind::Unavailable,
            true,
            "DeepSeek is currently unavailable",
        ),
        _ => (
            AiErrorKind::InvalidInput,
            false,
            "DeepSeek rejected the request",
        ),
    };
    let mut error = AiError::new(kind, recoverable, message);
    error.provider = Some("deepseek".into());
    error.status_code = Some(status.as_u16());
    error
}

fn cancelled() -> AiError {
    AiError::new(
        AiErrorKind::Cancelled,
        false,
        "The DeepSeek request was cancelled",
    )
}

#[cfg(test)]
mod tests {
    use super::{DeepSeekClient, map_status};
    use crate::AiErrorKind;

    #[test]
    fn diagnostics_never_expose_credentials() {
        let client = DeepSeekClient::new("super-secret")
            .expect("client")
            .with_endpoint("http://127.0.0.1:1");
        assert!(!format!("{client:?}").contains("super-secret"));
    }

    #[test]
    fn retryable_and_permanent_statuses_are_typed() {
        assert_eq!(
            map_status(reqwest::StatusCode::TOO_MANY_REQUESTS).kind,
            AiErrorKind::RateLimited
        );
        assert!(map_status(reqwest::StatusCode::BAD_GATEWAY).recoverable);
        assert!(!map_status(reqwest::StatusCode::UNAUTHORIZED).recoverable);
    }
}
