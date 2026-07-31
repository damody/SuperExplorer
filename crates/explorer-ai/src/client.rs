//! Asynchronous AI client contracts and deterministic implementations.

use std::{
    collections::VecDeque,
    future::Future,
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use crate::{AiError, AiErrorKind, AiRequest, AiResponse, AiResult};

/// Boxed provider operation that can be awaited by any executor.
pub type AiFuture<T> = Pin<Box<dyn Future<Output = AiResult<T>> + Send + 'static>>;

/// Streaming text callback.
pub type AiStreamCallback = Arc<dyn Fn(String) + Send + Sync + 'static>;

/// Cloneable cooperative cancellation shared by provider requests and callers.
#[derive(Clone, Debug, Default)]
pub struct AiCancellation {
    cancelled: Arc<AtomicBool>,
}

impl AiCancellation {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

/// Provider-neutral asynchronous AI client.
pub trait AiClient: Send + Sync {
    fn execute_cancellable(
        &self,
        request: AiRequest,
        cancellation: AiCancellation,
    ) -> AiFuture<AiResponse>;

    fn execute(&self, request: AiRequest) -> AiFuture<AiResponse> {
        self.execute_cancellable(request, AiCancellation::default())
    }

    fn execute_stream(
        &self,
        request: AiRequest,
        cancellation: AiCancellation,
        on_text: AiStreamCallback,
    ) -> AiFuture<AiResponse> {
        let future = self.execute_cancellable(request, cancellation);
        Box::pin(async move {
            let response = future.await?;
            on_text(response.text.clone());
            Ok(response)
        })
    }
}

/// Deterministic client that records requests and returns queued responses.
#[derive(Clone, Debug, Default)]
pub struct FakeAiClient {
    state: Arc<Mutex<FakeAiState>>,
}

#[derive(Debug, Default)]
struct FakeAiState {
    requests: Vec<AiRequest>,
    responses: VecDeque<AiResult<AiResponse>>,
}

impl FakeAiClient {
    /// Appends one response returned by the next request.
    ///
    /// # Errors
    ///
    /// Returns an internal error if another thread poisoned the fake state.
    pub fn push_response(&self, response: AiResult<AiResponse>) -> AiResult<()> {
        let mut state = self.state.lock().map_err(|_| poisoned_fake())?;
        state.responses.push_back(response);
        Ok(())
    }

    /// Returns a snapshot of submitted requests.
    ///
    /// # Errors
    ///
    /// Returns an internal error if another thread poisoned the fake state.
    pub fn requests(&self) -> AiResult<Vec<AiRequest>> {
        let state = self.state.lock().map_err(|_| poisoned_fake())?;
        Ok(state.requests.clone())
    }
}

impl AiClient for FakeAiClient {
    fn execute_cancellable(
        &self,
        request: AiRequest,
        cancellation: AiCancellation,
    ) -> AiFuture<AiResponse> {
        if cancellation.is_cancelled() {
            return Box::pin(async {
                Err(AiError::new(
                    AiErrorKind::Cancelled,
                    false,
                    "The AI request was cancelled",
                ))
            });
        }
        let result = self.state.lock().map_or_else(
            |_| Err(poisoned_fake()),
            |mut state| {
                state.requests.push(request);
                state.responses.pop_front().unwrap_or_else(|| {
                    Err(AiError::new(
                        AiErrorKind::Unavailable,
                        true,
                        "No deterministic AI response was configured",
                    ))
                })
            },
        );
        Box::pin(async move { result })
    }
}

fn poisoned_fake() -> AiError {
    AiError::new(
        AiErrorKind::Internal,
        false,
        "The deterministic AI client is unavailable",
    )
}

#[cfg(test)]
mod tests {
    use std::task::{Context, Poll, Waker};

    use crate::{AiOperation, AiRequest, AiResponse, AiUsage};

    use super::{AiClient, AiFuture, FakeAiClient};

    fn ready<T>(mut future: AiFuture<T>) -> crate::AiResult<T> {
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        match future.as_mut().poll(&mut context) {
            Poll::Ready(result) => result,
            Poll::Pending => panic!("deterministic AI future must be immediately ready"),
        }
    }

    #[test]
    fn fake_records_request_and_returns_queued_response() {
        let client = FakeAiClient::default();
        let response = AiResponse {
            provider: "deepseek".into(),
            model: "deepseek-v4-flash".into(),
            text: "summary".into(),
            usage: AiUsage::default(),
        };
        client
            .push_response(Ok(response.clone()))
            .expect("queue response");
        let request = AiRequest {
            operation: AiOperation::Summarize,
            provider: "deepseek".into(),
            model: "deepseek-v4-flash".into(),
            input: "private input".into(),
            system_prompt: None,
            timeout_ms: 1_000,
            correlation_id: "test".into(),
        };

        assert_eq!(ready(client.execute(request.clone())), Ok(response));
        assert_eq!(client.requests().expect("requests"), vec![request]);
    }
}
