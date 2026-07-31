//! Process diagnostics, safe log redaction, and panic reporting.

use std::{
    any::Any,
    error::Error as StdError,
    fs::{self, File, OpenOptions},
    io::{self, Write as _},
    panic::PanicHookInfo,
    path::PathBuf,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use thiserror::Error;

/// Immutable process diagnostics configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiagnosticsConfig {
    pub app_name: String,
    pub app_version: String,
    pub log_directory: PathBuf,
    /// Full candidate paths for the dedicated append-only error log, in priority order.
    pub error_log_candidates: Vec<PathBuf>,
    pub sensitive_roots: Vec<PathBuf>,
}

impl DiagnosticsConfig {
    /// Builds the production configuration, with an opt-in test log location.
    pub fn from_environment(app_version: impl Into<String>) -> Self {
        let configured_log_directory = std::env::var_os("EXPLORER_LOG_DIR").map(PathBuf::from);
        let local_log_directory = std::env::var_os("LOCALAPPDATA").map_or_else(
            || std::env::temp_dir().join("RustGpuiExplorer").join("logs"),
            |root| PathBuf::from(root).join("RustGpuiExplorer").join("logs"),
        );
        let log_directory = configured_log_directory
            .clone()
            .unwrap_or_else(|| local_log_directory.clone());
        let error_log_candidates = configured_log_directory.map_or_else(
            || production_error_log_candidates(&local_log_directory),
            |directory| vec![directory.join("error.log")],
        );
        let sensitive_roots = std::env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .into_iter()
            .collect();

        Self {
            app_name: "rust-gpui-windows-explorer".to_owned(),
            app_version: app_version.into(),
            log_directory,
            error_log_candidates,
            sensitive_roots,
        }
    }

    /// Returns the deterministic log path for this process.
    #[must_use]
    pub fn log_file_path(&self) -> PathBuf {
        self.log_directory.join("explorer.log")
    }

    /// Creates a deterministic configuration for tests and embedded hosts.
    #[must_use]
    pub fn with_error_log_candidates(mut self, candidates: Vec<PathBuf>) -> Self {
        self.error_log_candidates = candidates;
        self
    }

    fn equivalent_to(&self, other: &Self) -> bool {
        self.app_name == other.app_name
            && self.app_version == other.app_version
            && self.log_file_path() == other.log_file_path()
            && self.error_log_candidates == other.error_log_candidates
    }
}

fn production_error_log_candidates(local_log_directory: &std::path::Path) -> Vec<PathBuf> {
    let mut candidates = Vec::with_capacity(3);
    if let Ok(executable) = std::env::current_exe()
        && let Some(directory) = executable.parent()
    {
        candidates.push(directory.join("error.log"));
    }
    candidates.push(local_log_directory.join("error.log"));
    let temporary = std::env::temp_dir()
        .join("RustGpuiExplorer")
        .join("logs")
        .join("error.log");
    if !candidates.contains(&temporary) {
        candidates.push(temporary);
    }
    candidates
}

/// Diagnostics initialization and write failures.
#[derive(Debug, Error)]
pub enum DiagnosticsError {
    #[error("failed to create diagnostics directory {path}: {source}")]
    CreateDirectory { path: PathBuf, source: io::Error },
    #[error("failed to open diagnostics log {path}: {source}")]
    OpenLog { path: PathBuf, source: io::Error },
    #[error("diagnostics were already initialized with a different configuration")]
    ConfigurationMismatch,
    #[error("diagnostics log mutex was poisoned")]
    Poisoned,
    #[error("failed to write diagnostics log: {0}")]
    Write(#[from] io::Error),
}

#[derive(Debug)]
struct DiagnosticsInner {
    config: DiagnosticsConfig,
    file: Mutex<File>,
    error_sink: Mutex<ErrorSink>,
    shutdown: AtomicBool,
}

#[derive(Debug)]
struct ErrorSink {
    file: Option<File>,
    path: Option<PathBuf>,
}

/// Severity attached to an `error.log` record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorSeverity {
    Warning,
    Error,
    Critical,
}

impl ErrorSeverity {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Warning => "warning",
            Self::Error => "error",
            Self::Critical => "critical",
        }
    }
}

/// Cloneable process diagnostics session.
#[derive(Clone, Debug)]
pub struct DiagnosticsSession(Arc<DiagnosticsInner>);

impl DiagnosticsSession {
    /// Returns the active configuration.
    #[must_use]
    pub fn config(&self) -> &DiagnosticsConfig {
        &self.0.config
    }

    /// Returns the selected `error.log` path, or `None` when every candidate failed.
    #[must_use]
    pub fn error_log_path(&self) -> Option<PathBuf> {
        let sink = self
            .0
            .error_sink
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        sink.path.clone()
    }

    /// Writes one structured event after redacting configured sensitive roots.
    ///
    /// # Errors
    ///
    /// Returns an error when the log mutex is poisoned or the event cannot be written/flushed.
    pub fn record_event(
        &self,
        event: &str,
        fields: &[(&str, &str)],
    ) -> Result<(), DiagnosticsError> {
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let mut line = format!("timestamp_ms={timestamp_ms} event={}", escape_value(event));
        for (key, value) in fields {
            let redacted = redact_text(value, &self.0.config.sensitive_roots);
            line.push(' ');
            line.push_str(key);
            line.push('=');
            line.push_str(&escape_value(&redacted));
        }
        line.push('\n');

        let mut file = self.0.file.lock().map_err(|_| DiagnosticsError::Poisoned)?;
        file.write_all(line.as_bytes())?;
        file.flush()?;
        Ok(())
    }

    /// Appends a structured error record without allowing diagnostic failure to escape.
    pub fn record_error(
        &self,
        severity: ErrorSeverity,
        subsystem: &str,
        operation: &str,
        error: &(dyn StdError + 'static),
        source_location: Option<&str>,
    ) {
        let error_chain = format_error_chain(error);
        self.record_error_message(
            severity,
            subsystem,
            operation,
            &error_chain,
            source_location,
        );
    }

    /// Appends a structured error record for a preformatted failure or panic report.
    pub fn record_error_message(
        &self,
        severity: ErrorSeverity,
        subsystem: &str,
        operation: &str,
        message: &str,
        source_location: Option<&str>,
    ) {
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let thread = std::thread::current();
        let thread_name = thread.name().unwrap_or("unnamed");
        let redacted_message = redact_text(message, &self.0.config.sensitive_roots);
        let mut line = format!(
            "timestamp_ms={timestamp_ms} severity={} subsystem={} operation={} error={} thread={} version={}",
            severity.as_str(),
            escape_value(subsystem),
            escape_value(operation),
            escape_value(&redacted_message),
            escape_value(thread_name),
            escape_value(&self.0.config.app_version),
        );
        if let Some(source) = source_location {
            let redacted_source = redact_text(source, &self.0.config.sensitive_roots);
            line.push_str(" source=");
            line.push_str(&escape_value(&redacted_source));
        }
        line.push('\n');

        let mut sink = self
            .0
            .error_sink
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let write_result = sink.file.as_mut().map_or_else(
            || {
                Err(io::Error::new(
                    io::ErrorKind::NotConnected,
                    "error log unavailable",
                ))
            },
            |file| file.write_all(line.as_bytes()).and_then(|()| file.flush()),
        );
        if let Err(write_error) = write_result {
            sink.file = None;
            sink.path = None;
            drop(sink);
            fallback_error_report(&line, &write_error);
        }
    }

    /// Records clean shutdown and flushes at most once.
    ///
    /// # Errors
    ///
    /// Returns an error when the terminal event cannot be written or flushed.
    pub fn shutdown(&self) -> Result<(), DiagnosticsError> {
        if self.0.shutdown.swap(true, Ordering::AcqRel) {
            return Ok(());
        }
        self.record_event("clean_shutdown", &[])
    }
}

/// Injectable registry that makes repeated initialization deterministic in tests.
#[derive(Debug, Default)]
pub struct DiagnosticsRegistry {
    session: OnceLock<DiagnosticsSession>,
}

impl DiagnosticsRegistry {
    /// Creates an empty registry.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            session: OnceLock::new(),
        }
    }

    /// Initializes diagnostics or returns the existing equivalent session.
    ///
    /// # Errors
    ///
    /// Returns an error for a conflicting repeated configuration or when the log cannot be opened.
    pub fn initialize(
        &self,
        config: DiagnosticsConfig,
    ) -> Result<DiagnosticsSession, DiagnosticsError> {
        if let Some(existing) = self.session.get() {
            return if existing.config().equivalent_to(&config) {
                Ok(existing.clone())
            } else {
                Err(DiagnosticsError::ConfigurationMismatch)
            };
        }

        fs::create_dir_all(&config.log_directory).map_err(|source| {
            DiagnosticsError::CreateDirectory {
                path: config.log_directory.clone(),
                source,
            }
        })?;
        let log_path = config.log_file_path();
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .map_err(|source| DiagnosticsError::OpenLog {
                path: log_path,
                source,
            })?;
        let error_sink = open_error_sink(&config.error_log_candidates);
        let session = DiagnosticsSession(Arc::new(DiagnosticsInner {
            config,
            file: Mutex::new(file),
            error_sink: Mutex::new(error_sink),
            shutdown: AtomicBool::new(false),
        }));
        let _ = self.session.set(session.clone());
        Ok(session)
    }
}

fn open_error_sink(candidates: &[PathBuf]) -> ErrorSink {
    for path in candidates {
        let Some(directory) = path.parent() else {
            continue;
        };
        if fs::create_dir_all(directory).is_err() {
            continue;
        }
        if let Ok(file) = OpenOptions::new().create(true).append(true).open(path) {
            return ErrorSink {
                file: Some(file),
                path: Some(path.clone()),
            };
        }
    }
    ErrorSink {
        file: None,
        path: None,
    }
}

fn format_error_chain(error: &(dyn StdError + 'static)) -> String {
    let mut output = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        output.push_str(": ");
        output.push_str(&cause.to_string());
        source = cause.source();
    }
    output
}

fn fallback_error_report(line: &str, write_error: &io::Error) {
    eprintln!(
        "Explorer error-log failure ({write_error}): {}",
        line.trim_end()
    );
}

static PROCESS_DIAGNOSTICS: DiagnosticsRegistry = DiagnosticsRegistry::new();
static PANIC_HOOK_INSTALLED: AtomicBool = AtomicBool::new(false);

/// Records an error through the process diagnostics session when one is available.
pub fn record_process_error(
    severity: ErrorSeverity,
    subsystem: &str,
    operation: &str,
    error: &(dyn StdError + 'static),
    source_location: Option<&str>,
) {
    if let Some(session) = PROCESS_DIAGNOSTICS.session.get() {
        session.record_error(severity, subsystem, operation, error, source_location);
    } else {
        eprintln!(
            "Explorer error before diagnostics initialization: subsystem={subsystem} operation={operation} error={error}"
        );
    }
}

/// Records a preformatted error through the process diagnostics session when available.
pub fn record_process_error_message(
    severity: ErrorSeverity,
    subsystem: &str,
    operation: &str,
    message: &str,
    source_location: Option<&str>,
) {
    if let Some(session) = PROCESS_DIAGNOSTICS.session.get() {
        session.record_error_message(severity, subsystem, operation, message, source_location);
    } else {
        eprintln!(
            "Explorer error before diagnostics initialization: subsystem={subsystem} operation={operation} error={message}"
        );
    }
}

/// Converts a Rust panic payload into a stable diagnostic message.
#[must_use]
pub fn panic_payload_message(payload: &(dyn Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .copied()
        .map(str::to_owned)
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "non-string panic payload".to_owned())
}

/// Initializes the process-global diagnostics session.
///
/// # Errors
///
/// Returns an error when the configured log directory or initial event cannot be written.
pub fn initialize_diagnostics(
    config: DiagnosticsConfig,
) -> Result<DiagnosticsSession, DiagnosticsError> {
    let fallback_config = config.clone();
    let session = match PROCESS_DIAGNOSTICS.initialize(config) {
        Ok(session) => session,
        Err(error) => {
            record_bootstrap_failure(&fallback_config, &error);
            return Err(error);
        }
    };
    // The application is commonly launched with stdout/stderr redirected to a background log
    // viewer. Such writers do not interpret terminal escape sequences, so keep the process log
    // plain text instead of leaking values such as `\x1b[2m` into the visible output.
    let _ = tracing_subscriber::fmt()
        .with_target(false)
        .with_ansi(false)
        .try_init();
    session.record_event(
        "diagnostics_initialized",
        &[
            ("app", &session.config().app_name),
            ("version", &session.config().app_version),
        ],
    )?;
    Ok(session)
}

fn record_bootstrap_failure(config: &DiagnosticsConfig, error: &DiagnosticsError) {
    let timestamp_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let message = redact_text(&error.to_string(), &config.sensitive_roots);
    let line = format!(
        "timestamp_ms={timestamp_ms} severity=critical subsystem=\"diagnostics\" operation=\"initialize\" error={} thread=\"main\" version={}\n",
        escape_value(&message),
        escape_value(&config.app_version),
    );
    let mut sink = open_error_sink(&config.error_log_candidates);
    let result = sink.file.as_mut().map_or_else(
        || {
            Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "error log unavailable",
            ))
        },
        |file| file.write_all(line.as_bytes()).and_then(|()| file.flush()),
    );
    if let Err(write_error) = result {
        fallback_error_report(&line, &write_error);
    }
}

/// Installs one process panic hook that writes a redacted report, then calls the previous hook.
pub fn install_panic_hook(session: DiagnosticsSession) {
    if PANIC_HOOK_INSTALLED.swap(true, Ordering::AcqRel) {
        return;
    }
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let report = format_panic_report(info, &session.0.config);
        let _ = session.record_event("panic", &[("report", &report)]);
        session.record_error_message(
            ErrorSeverity::Critical,
            "process",
            "panic",
            &report,
            info.location().map(std::panic::Location::file),
        );
        previous(info);
    }));
}

/// Formats a panic report without exposing configured sensitive path prefixes.
#[must_use]
pub fn format_panic_report(info: &PanicHookInfo<'_>, config: &DiagnosticsConfig) -> String {
    let thread = std::thread::current();
    let thread_name = thread.name().unwrap_or("unnamed");
    let message = panic_payload_message(info.payload());
    let location = info.location().map_or_else(
        || "unknown".to_owned(),
        |location| {
            format!(
                "{}:{}:{}",
                location.file(),
                location.line(),
                location.column()
            )
        },
    );
    let report = format!(
        "version={} thread={} location={} backtrace_available={} message={}",
        config.app_version,
        thread_name,
        location,
        std::env::var_os("RUST_BACKTRACE").is_some(),
        message
    );
    redact_text(&report, &config.sensitive_roots)
}

fn redact_text(input: &str, sensitive_roots: &[PathBuf]) -> String {
    sensitive_roots
        .iter()
        .fold(input.to_owned(), |redacted, root| {
            let root = root.to_string_lossy();
            if root.is_empty() {
                redacted
            } else {
                redacted.replace(root.as_ref(), "%REDACTED_ROOT%")
            }
        })
}

fn escape_value(value: &str) -> String {
    format!("{value:?}")
}

#[cfg(test)]
mod tests {
    use super::{
        DiagnosticsConfig, DiagnosticsError, DiagnosticsRegistry, ErrorSeverity, ErrorSink,
    };

    fn config(root: &Path, sensitive_root: &Path) -> DiagnosticsConfig {
        DiagnosticsConfig {
            app_name: "test-explorer".to_owned(),
            app_version: "1.2.3".to_owned(),
            log_directory: root.to_path_buf(),
            error_log_candidates: vec![root.join("error.log")],
            sensitive_roots: vec![sensitive_root.to_path_buf()],
        }
    }

    use std::{fs, path::Path};

    #[test]
    fn background_tracing_format_is_plain_text() {
        use std::sync::{Arc, Mutex};

        #[derive(Clone, Default)]
        struct Buffer(Arc<Mutex<Vec<u8>>>);

        struct BufferWriter(Buffer);

        impl std::io::Write for BufferWriter {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.0
                    .0
                    .lock()
                    .expect("buffer lock")
                    .extend_from_slice(bytes);
                Ok(bytes.len())
            }

            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        impl<'writer> tracing_subscriber::fmt::MakeWriter<'writer> for Buffer {
            type Writer = BufferWriter;

            fn make_writer(&'writer self) -> Self::Writer {
                BufferWriter(self.clone())
            }
        }

        let buffer = Buffer::default();
        let subscriber = tracing_subscriber::fmt()
            .with_target(false)
            .with_ansi(false)
            .with_writer(buffer.clone())
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(request_id = 42, "terminal event");
        });
        let bytes = buffer.0.lock().expect("buffer lock").clone();
        let output = String::from_utf8(bytes).expect("UTF-8 tracing output");

        assert!(output.contains("terminal event"));
        assert!(output.contains("request_id=42"));
        assert!(!output.contains('\u{1b}'));
    }

    #[test]
    fn repeated_equivalent_initialization_returns_the_same_log() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let registry = DiagnosticsRegistry::new();
        let config = config(temp.path(), Path::new(r"C:\Users\Sensitive"));

        let first = registry.initialize(config.clone()).expect("first init");
        let second = registry.initialize(config).expect("second init");

        assert_eq!(
            first.config().log_file_path(),
            second.config().log_file_path()
        );
    }

    #[test]
    fn different_repeated_configuration_is_rejected() {
        let first_root = tempfile::tempdir().expect("first temporary directory");
        let second_root = tempfile::tempdir().expect("second temporary directory");
        let registry = DiagnosticsRegistry::new();
        registry
            .initialize(config(first_root.path(), Path::new("secret")))
            .expect("first init");

        let result = registry.initialize(config(second_root.path(), Path::new("secret")));
        assert!(matches!(
            result,
            Err(DiagnosticsError::ConfigurationMismatch)
        ));
    }

    #[test]
    fn event_fields_are_redacted() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let registry = DiagnosticsRegistry::new();
        let sensitive = Path::new(r"C:\Users\Sensitive");
        let session = registry
            .initialize(config(temp.path(), sensitive))
            .expect("init");

        session
            .record_event("failure", &[("path", r"C:\Users\Sensitive\private.txt")])
            .expect("record event");

        let log = fs::read_to_string(session.config().log_file_path()).expect("read log");
        assert!(!log.contains("C:\\Users\\Sensitive"));
        assert!(log.contains("%REDACTED_ROOT%"));
    }

    #[test]
    fn shutdown_is_idempotent() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let registry = DiagnosticsRegistry::new();
        let session = registry
            .initialize(config(temp.path(), Path::new("secret")))
            .expect("init");

        session.shutdown().expect("first shutdown");
        session.shutdown().expect("second shutdown");

        let log = fs::read_to_string(session.config().log_file_path()).expect("read log");
        assert_eq!(log.matches("clean_shutdown").count(), 1);
    }

    #[test]
    fn error_log_uses_first_writable_candidate_and_appends_redacted_records() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let blocked = temp.path().join("blocked");
        fs::create_dir(&blocked).expect("blocked candidate directory");
        let selected = temp.path().join("fallback").join("error.log");
        let sensitive = Path::new(r"C:\Users\Sensitive");
        let registry = DiagnosticsRegistry::new();
        let session = registry
            .initialize(
                config(temp.path(), sensitive)
                    .with_error_log_candidates(vec![blocked, selected.clone()]),
            )
            .expect("initialize diagnostics");

        session.record_error_message(
            ErrorSeverity::Error,
            "shell",
            "enumerate",
            r"failed at C:\Users\Sensitive\private.txt",
            Some("watcher.rs:10"),
        );
        session.record_error_message(ErrorSeverity::Warning, "ui", "retry", "second record", None);

        assert_eq!(session.error_log_path(), Some(selected.clone()));
        let log = fs::read_to_string(selected).expect("read error log");
        assert!(log.contains("severity=error"));
        assert!(log.contains("subsystem=\"shell\""));
        assert!(log.contains("operation=\"enumerate\""));
        assert!(log.contains("%REDACTED_ROOT%"));
        assert!(!log.contains(r"C:\Users\Sensitive"));
        assert_eq!(log.lines().count(), 2);
    }

    #[test]
    fn every_error_candidate_may_fail_without_failing_diagnostics() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let first = temp.path().join("first-directory");
        let second = temp.path().join("second-directory");
        fs::create_dir(&first).expect("first directory");
        fs::create_dir(&second).expect("second directory");
        let registry = DiagnosticsRegistry::new();
        let session = registry
            .initialize(
                config(temp.path(), Path::new("secret"))
                    .with_error_log_candidates(vec![first, second]),
            )
            .expect("general diagnostics remain available");

        session.record_error_message(
            ErrorSeverity::Error,
            "diagnostics",
            "probe",
            "no error file",
            None,
        );

        assert_eq!(session.error_log_path(), None);
    }

    #[test]
    fn poisoned_error_sink_mutex_is_recovered() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let registry = DiagnosticsRegistry::new();
        let session = registry
            .initialize(config(temp.path(), Path::new("secret")))
            .expect("initialize diagnostics");
        let inner = session.0.clone();
        let _ = std::panic::catch_unwind(move || {
            let _guard = inner.error_sink.lock().expect("lock error sink");
            panic!("poison error sink for test");
        });

        session.record_error_message(
            ErrorSeverity::Error,
            "diagnostics",
            "poison_recovery",
            "recovered",
            None,
        );

        let path = session.error_log_path().expect("selected error log");
        let log = fs::read_to_string(path).expect("read recovered log");
        assert!(log.contains("poison_recovery"));
    }

    #[test]
    fn disabled_error_sink_state_is_non_panicking() {
        let sink = ErrorSink {
            file: None,
            path: None,
        };
        assert!(sink.file.is_none());
        assert!(sink.path.is_none());
    }
}
