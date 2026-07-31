//! AI summary composition with atomic task-relative text output.

use std::{path::Path, sync::Arc};

use explorer_ai::{AiCancellation, AiClient, AiRequest, AiResponse};

use crate::{
    AutomationError, AutomationErrorKind, AutomationResult, FileHost, FileWriteMode, TaskContext,
    TaskFiles,
};

/// Composes a provider-neutral AI client with the task-scoped filesystem API.
#[derive(Clone)]
pub struct SummaryService {
    ai: Arc<dyn AiClient>,
    files: Arc<dyn FileHost>,
}

impl SummaryService {
    #[must_use]
    pub fn new(ai: Arc<dyn AiClient>, files: Arc<dyn FileHost>) -> Self {
        Self { ai, files }
    }

    /// Summarizes text and atomically writes the returned UTF-8 text.
    ///
    /// # Errors
    ///
    /// Returns typed cancellation, provider, encoding, or filesystem errors.
    pub async fn summarize_to_text(
        &self,
        task: &TaskContext,
        input: String,
        output: impl AsRef<Path>,
        system_prompt: Option<String>,
    ) -> AutomationResult<AiResponse> {
        task.ensure_active(task.created_unix_ms)?;
        let cancellation = AiCancellation::default();
        if task.cancellation.is_cancelled() {
            cancellation.cancel();
        }
        let request = AiRequest {
            operation: explorer_ai::AiOperation::Summarize,
            provider: "deepseek".into(),
            model: explorer_ai::DEEPSEEK_DEFAULT_MODEL.into(),
            input,
            system_prompt,
            timeout_ms: task.deadline_unix_ms.map_or(90_000, |deadline| {
                deadline.saturating_sub(task.created_unix_ms)
            }),
            correlation_id: task.correlation_id.as_uuid().to_string(),
        };
        let response = self
            .ai
            .execute_cancellable(request, cancellation)
            .await
            .map_err(map_ai_error)?;
        task.ensure_active(task.created_unix_ms)?;
        TaskFiles::new(Arc::clone(&self.files), task.clone())
            .write_text(output, response.text.clone(), FileWriteMode::AtomicReplace)
            .await?;
        Ok(response)
    }
}

impl std::fmt::Debug for SummaryService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SummaryService")
            .finish_non_exhaustive()
    }
}

fn map_ai_error(error: explorer_ai::AiError) -> AutomationError {
    let kind = match error.kind {
        explorer_ai::AiErrorKind::Cancelled => AutomationErrorKind::Cancelled,
        explorer_ai::AiErrorKind::Timeout => AutomationErrorKind::Timeout,
        explorer_ai::AiErrorKind::RateLimited => AutomationErrorKind::Overloaded,
        explorer_ai::AiErrorKind::Authentication | explorer_ai::AiErrorKind::MissingCredential => {
            AutomationErrorKind::Authorization
        }
        _ => AutomationErrorKind::Ai,
    };
    AutomationError::new(kind, "ai.summarize", error.recoverable, error.user_message)
}

#[cfg(test)]
mod tests {
    use std::{
        future::Future,
        sync::Arc,
        task::{Context, Poll, Waker},
    };

    use explorer_ai::{AiResponse, AiUsage, FakeAiClient};

    use crate::{HandlerId, SummaryService, fakes::FakeFileHost, task::tests_support};

    fn ready<F: Future>(future: F) -> F::Output {
        let mut future = std::pin::pin!(future);
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => value,
            Poll::Pending => panic!("fake summary future must be ready"),
        }
    }

    #[test]
    fn summary_is_written_atomically_relative_to_captured_cwd() {
        let ai = Arc::new(FakeAiClient::default());
        ai.push_response(Ok(AiResponse {
            provider: "deepseek".into(),
            model: "deepseek-v4-flash".into(),
            text: "精簡摘要".into(),
            usage: AiUsage::default(),
        }))
        .expect("response");
        let files = Arc::new(FakeFileHost::default());
        let service = SummaryService::new(ai, files.clone());
        let task = tests_support::task_for_runtime_test(HandlerId::new(), r"D:\Notes");
        ready(service.summarize_to_text(&task, "long text".into(), "summary.txt", None))
            .expect("summary");
        assert_eq!(
            files
                .file(&std::path::PathBuf::from(r"D:\Notes\summary.txt"))
                .expect("file"),
            Some("精簡摘要".as_bytes().to_vec())
        );
    }
}
