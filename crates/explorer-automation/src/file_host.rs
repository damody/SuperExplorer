//! Task-relative file APIs and an atomic native implementation.

#![allow(clippy::missing_errors_doc)]

use std::{
    fs::{self, OpenOptions},
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
    sync::Arc,
};

use tempfile::NamedTempFile;

use crate::{
    AutomationError, AutomationErrorKind, AutomationFuture, AutomationResult, FileHost,
    FileWriteMode, HostEffect, ScriptId, TaskContext, UiHost,
};

/// Native file adapter. Operations are executor-neutral and return ready boxed futures.
#[derive(Clone, Copy, Debug, Default)]
pub struct NativeFileHost;

impl FileHost for NativeFileHost {
    fn read(&self, path: PathBuf) -> AutomationFuture<Vec<u8>> {
        Box::pin(async move {
            let mut file = fs::File::open(path).map_err(|_| file_error("file.read"))?;
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)
                .map_err(|_| file_error("file.read"))?;
            Ok(bytes)
        })
    }

    fn write(
        &self,
        path: PathBuf,
        bytes: Vec<u8>,
        mode: FileWriteMode,
    ) -> AutomationFuture<PathBuf> {
        Box::pin(async move {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|_| file_error("file.write"))?;
            }
            match mode {
                FileWriteMode::CreateNew => {
                    let mut file = OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&path)
                        .map_err(|_| file_error("file.write"))?;
                    file.write_all(&bytes)
                        .and_then(|()| file.sync_all())
                        .map_err(|_| file_error("file.write"))?;
                }
                FileWriteMode::Append => {
                    let mut file = OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&path)
                        .map_err(|_| file_error("file.append"))?;
                    file.write_all(&bytes)
                        .and_then(|()| file.sync_all())
                        .map_err(|_| file_error("file.append"))?;
                }
                FileWriteMode::AtomicReplace => atomic_replace(&path, &bytes)?,
            }
            Ok(path)
        })
    }

    fn remove(&self, _script_id: ScriptId, path: PathBuf) -> AutomationFuture<()> {
        Box::pin(async move {
            if path.is_dir() {
                fs::remove_dir_all(path).map_err(|_| file_error("file.remove"))
            } else {
                fs::remove_file(path).map_err(|_| file_error("file.remove"))
            }
        })
    }
}

/// Enforces the sole mandatory user confirmation: file or directory removal.
#[derive(Clone)]
pub struct ConfirmingFileHost {
    inner: Arc<dyn FileHost>,
    ui: Arc<dyn UiHost>,
}

impl ConfirmingFileHost {
    #[must_use]
    pub fn new(inner: Arc<dyn FileHost>, ui: Arc<dyn UiHost>) -> Self {
        Self { inner, ui }
    }
}

impl std::fmt::Debug for ConfirmingFileHost {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ConfirmingFileHost")
            .finish_non_exhaustive()
    }
}

impl FileHost for ConfirmingFileHost {
    fn read(&self, path: PathBuf) -> AutomationFuture<Vec<u8>> {
        self.inner.read(path)
    }

    fn write(
        &self,
        path: PathBuf,
        bytes: Vec<u8>,
        mode: FileWriteMode,
    ) -> AutomationFuture<PathBuf> {
        self.inner.write(path, bytes, mode)
    }

    fn remove(&self, script_id: ScriptId, path: PathBuf) -> AutomationFuture<()> {
        let ui = Arc::clone(&self.ui);
        let inner = Arc::clone(&self.inner);
        Box::pin(async move {
            let approved = ui
                .present(HostEffect::ConfirmDeletion {
                    script_id,
                    paths: vec![path.clone()],
                })
                .await?;
            if !approved {
                return Err(AutomationError::new(
                    AutomationErrorKind::DeletionDenied,
                    "file.remove",
                    false,
                    "File removal was not approved",
                ));
            }
            inner.remove(script_id, path).await
        })
    }
}

/// High-level UTF-8, bytes, and JSON helpers bound to one immutable task cwd.
#[derive(Clone)]
pub struct TaskFiles {
    host: Arc<dyn FileHost>,
    task: TaskContext,
}

impl TaskFiles {
    #[must_use]
    pub fn new(host: Arc<dyn FileHost>, task: TaskContext) -> Self {
        Self { host, task }
    }

    /// Resolves against the cwd captured when the event created this task.
    #[must_use]
    pub fn resolve(&self, path: impl AsRef<Path>) -> PathBuf {
        self.task.resolve_path(path)
    }

    pub async fn read_bytes(&self, path: impl AsRef<Path>) -> AutomationResult<Vec<u8>> {
        self.task.ensure_active(self.task.created_unix_ms)?;
        self.host.read(self.resolve(path)).await
    }

    pub async fn read_text(&self, path: impl AsRef<Path>) -> AutomationResult<String> {
        String::from_utf8(self.read_bytes(path).await?).map_err(|_| {
            AutomationError::new(
                AutomationErrorKind::FileSystem,
                "file.read_text",
                false,
                "The file is not valid UTF-8 text",
            )
        })
    }

    pub async fn read_json(&self, path: impl AsRef<Path>) -> AutomationResult<serde_json::Value> {
        serde_json::from_slice(&self.read_bytes(path).await?).map_err(|_| {
            AutomationError::new(
                AutomationErrorKind::FileSystem,
                "file.read_json",
                false,
                "The file does not contain valid JSON",
            )
        })
    }

    pub async fn write_bytes(
        &self,
        path: impl AsRef<Path>,
        bytes: Vec<u8>,
        mode: FileWriteMode,
    ) -> AutomationResult<PathBuf> {
        self.task.ensure_active(self.task.created_unix_ms)?;
        self.host.write(self.resolve(path), bytes, mode).await
    }

    pub async fn write_text(
        &self,
        path: impl AsRef<Path>,
        text: impl Into<String>,
        mode: FileWriteMode,
    ) -> AutomationResult<PathBuf> {
        self.write_bytes(path, text.into().into_bytes(), mode).await
    }

    pub async fn write_json(
        &self,
        path: impl AsRef<Path>,
        value: &serde_json::Value,
        mode: FileWriteMode,
    ) -> AutomationResult<PathBuf> {
        let bytes = serde_json::to_vec_pretty(value).map_err(|_| {
            AutomationError::new(
                AutomationErrorKind::InvalidInput,
                "file.write_json",
                false,
                "The value could not be encoded as JSON",
            )
        })?;
        self.write_bytes(path, bytes, mode).await
    }
}

impl std::fmt::Debug for TaskFiles {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TaskFiles")
            .field("cwd", &self.task.cwd)
            .finish_non_exhaustive()
    }
}

fn atomic_replace(path: &Path, bytes: &[u8]) -> AutomationResult<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut temporary = NamedTempFile::new_in(parent).map_err(|_| file_error("file.write"))?;
    temporary
        .write_all(bytes)
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|_| file_error("file.write"))?;
    temporary
        .persist(path)
        .map_err(|_| file_error("file.write"))?;
    Ok(())
}

fn file_error(operation: &'static str) -> AutomationError {
    AutomationError::new(
        AutomationErrorKind::FileSystem,
        operation,
        true,
        "The file operation could not be completed",
    )
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        future::Future,
        sync::Arc,
        task::{Context, Poll, Waker},
    };

    use tempfile::tempdir;

    use crate::{
        AutomationErrorKind, ConfirmingFileHost, FileHost, FileWriteMode, NativeFileHost, ScriptId,
        TaskFiles,
        fakes::{FakeFileHost, FakeUiHost},
        task::tests_support,
    };

    fn ready<F: Future>(future: F) -> F::Output {
        let mut future = std::pin::pin!(future);
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => value,
            Poll::Pending => panic!("native file test future must be immediately ready"),
        }
    }

    #[test]
    fn native_atomic_replace_and_task_relative_json_are_complete() {
        let root = tempdir().expect("tempdir");
        let handler = crate::HandlerId::new();
        let task = tests_support::task_for_runtime_test(handler, &root.path().to_string_lossy());
        let files = TaskFiles::new(Arc::new(NativeFileHost), task);
        let value = serde_json::json!({ "summary": "完成" });
        ready(files.write_json("out/summary.txt", &value, FileWriteMode::AtomicReplace))
            .expect("write json");
        let decoded = ready(files.read_json("out/summary.txt")).expect("read json");
        assert_eq!(decoded, value);
        assert_eq!(
            fs::read_to_string(root.path().join("out/summary.txt")).expect("text"),
            "{\n  \"summary\": \"完成\"\n}"
        );
        assert_eq!(
            fs::read_dir(root.path().join("out"))
                .expect("directory")
                .count(),
            1
        );
    }

    #[test]
    fn removal_requires_confirmation_and_reports_denial() {
        let files = Arc::new(FakeFileHost::default());
        let ui = Arc::new(FakeUiHost::default());
        ui.push_answer(false).expect("answer");
        let host = ConfirmingFileHost::new(files.clone(), ui.clone());
        let path = std::path::PathBuf::from("protected.txt");
        ready(files.write(path.clone(), b"keep".to_vec(), FileWriteMode::AtomicReplace))
            .expect("seed");
        let error = ready(host.remove(ScriptId::new(), path.clone())).expect_err("denied");
        assert_eq!(error.kind, AutomationErrorKind::DeletionDenied);
        assert!(files.file(&path).expect("file").is_some());
        assert_eq!(ui.effects().expect("effects").len(), 1);
    }
}
