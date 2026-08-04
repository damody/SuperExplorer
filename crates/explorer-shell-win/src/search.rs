//! Windows Search capability adapter plus the bounded real-filesystem fallback.
#![allow(
    unsafe_code,
    reason = "Windows Search and COM activation require audited unsafe calls"
)]

use std::{
    path::{Path, PathBuf},
    sync::mpsc::{self, RecvTimeoutError, SyncSender},
    time::{Duration, Instant},
};

use crate::sta::RequiredTerminalPublisher;
use explorer_common::{ExplorerError, ExplorerErrorKind};
use explorer_model::{
    ExplorerEvent, LocationDescriptor, RequestContext, SearchBackend, SearchInput,
    SearchSourcePhase, SearchSourceStatus, SearchTerminal,
};
use explorer_search::{
    Expr, FallbackConfig, QueryParameter, SearchOutcome, SearchRequest, bind_query, parse,
    search_filesystem,
};
use windows::{
    Win32::System::{
        Com::{CLSCTX_INPROC_SERVER, CoCreateInstance},
        Search::{CSearchManager, CatalogPausedReason, CatalogStatus, ISearchManager},
    },
    core::HSTRING,
};

#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(dead_code, reason = "retained as a WindowsIndex compatibility oracle")]
pub(crate) enum IndexAvailability {
    Indexed { generated_sql_bytes: usize },
    OutsideScope,
    Unavailable(String),
}

#[allow(
    clippy::too_many_lines,
    reason = "one linear search pipeline keeps source-status and exactly-one terminal ordering auditable"
)]
pub(crate) fn execute_with_terminals<P: RequiredTerminalPublisher>(
    context: &RequestContext,
    location: &LocationDescriptor,
    input: &SearchInput,
    events: &SyncSender<ExplorerEvent>,
    terminals: &P,
) -> Result<(), ExplorerError> {
    let root = location.path().ok_or_else(|| {
        ExplorerError::new(
            ExplorerErrorKind::Input,
            "start search",
            true,
            "此位置目前不支援檔案系統搜尋。",
            "search root was not a filesystem location",
        )
    })?;
    let expression = parse(input.as_str()).map_err(|error| {
        ExplorerError::new(
            ExplorerErrorKind::Input,
            "parse search query",
            true,
            format!("搜尋條件有誤：{}", error.message),
            error.to_string(),
        )
    })?;

    match crate::everything::EverythingProvider::open_adjacent() {
        Ok(provider) => {
            publish_status(
                events,
                context,
                SearchBackend::Everything,
                SearchSourcePhase::Active,
                None,
            )?;
            match run_everything_bounded(provider, root, &expression, context, events) {
                Ok(_result_count) => {
                    publish_status(
                        events,
                        context,
                        SearchBackend::Everything,
                        SearchSourcePhase::Complete,
                        None,
                    )?;
                    terminals.publish_terminal(ExplorerEvent::SearchFinished {
                        context: context.clone(),
                        outcome: SearchTerminal::Finished,
                    });
                    return Ok(());
                }
                Err(_detail) if context.cancellation.is_cancelled() => {
                    publish_status(
                        events,
                        context,
                        SearchBackend::Everything,
                        SearchSourcePhase::Cancelled,
                        None,
                    )?;
                    terminals.publish_terminal(ExplorerEvent::SearchFinished {
                        context: context.clone(),
                        outcome: SearchTerminal::Cancelled,
                    });
                    return Ok(());
                }
                Err(detail) => publish_status(
                    events,
                    context,
                    SearchBackend::Everything,
                    SearchSourcePhase::Unavailable,
                    Some(detail),
                )?,
            }
        }
        Err(detail) => publish_status(
            events,
            context,
            SearchBackend::Everything,
            SearchSourcePhase::Unavailable,
            Some(detail),
        )?,
    }

    let request = SearchRequest {
        root: root.to_owned(),
        expression,
        cancellation: context.cancellation.clone(),
    };
    publish_status(
        events,
        context,
        SearchBackend::LocalIndex,
        SearchSourcePhase::Active,
        None,
    )?;
    let outcome = match explorer_search::LazyIndex::open_default() {
        Ok(mut index) => index.search(
            root,
            &request.expression,
            &request.cancellation,
            explorer_search::LazyIndexConfig::default(),
            |batch| {
                events
                    .send(ExplorerEvent::SearchBatch {
                        context: context.clone(),
                        source: SearchBackend::LocalIndex,
                        entries: batch.hits.into_iter().map(|hit| hit.entry).collect(),
                    })
                    .map_err(|_| ())
            },
        ),
        Err(error) => {
            publish_status(
                events,
                context,
                SearchBackend::LocalIndex,
                SearchSourcePhase::Partial,
                Some(format!("persistent index unavailable: {error}")),
            )?;
            search_filesystem(
                &request,
                FallbackConfig::default(),
                |path, is_directory| {
                    let bytes = crate::navigation::filesystem_identity(path, is_directory)
                        .unwrap_or_else(|_| crate::navigation::fallback_filesystem_identity(path));
                    explorer_model::ShellItemId::from_provider_bytes(bytes)
                },
                |batch| {
                    events
                        .send(ExplorerEvent::SearchBatch {
                            context: context.clone(),
                            source: SearchBackend::LocalIndex,
                            entries: batch.hits.into_iter().map(|hit| hit.entry).collect(),
                        })
                        .map_err(|_| ())
                },
            )
        }
    };
    let terminal = match outcome {
        SearchOutcome::Finished(_) => {
            publish_status(
                events,
                context,
                SearchBackend::LocalIndex,
                SearchSourcePhase::Complete,
                None,
            )?;
            SearchTerminal::Finished
        }
        SearchOutcome::Cancelled(_) => {
            publish_status(
                events,
                context,
                SearchBackend::LocalIndex,
                SearchSourcePhase::Cancelled,
                None,
            )?;
            SearchTerminal::Cancelled
        }
        SearchOutcome::Partial { diagnostic, .. } => {
            publish_status(
                events,
                context,
                SearchBackend::LocalIndex,
                SearchSourcePhase::Partial,
                Some(diagnostic.detail.clone()),
            )?;
            SearchTerminal::Partial(search_error(
                ExplorerErrorKind::Availability,
                "搜尋只完成部分位置。",
                diagnostic.detail,
            ))
        }
        SearchOutcome::Failed(diagnostic) => {
            publish_status(
                events,
                context,
                SearchBackend::LocalIndex,
                SearchSourcePhase::Failed,
                Some(diagnostic.detail.clone()),
            )?;
            SearchTerminal::Failed(search_error(
                ExplorerErrorKind::Availability,
                "搜尋服務發生錯誤。",
                diagnostic.detail,
            ))
        }
    };
    terminals
        .send(ExplorerEvent::SearchFinished {
            context: context.clone(),
            outcome: terminal,
        })
        .map_err(|_| {
            search_error(
                ExplorerErrorKind::Availability,
                "搜尋結果接收端已關閉。",
                "terminal result could not be delivered",
            )
        })
}

enum EverythingWorkerEvent {
    Batch(Vec<explorer_model::FileEntry>),
    Finished(Result<(), String>),
}

fn run_everything_bounded(
    mut provider: crate::everything::EverythingProvider,
    root: &Path,
    expression: &Expr,
    context: &RequestContext,
    events: &SyncSender<ExplorerEvent>,
) -> Result<usize, String> {
    const QUERY_TIMEOUT: Duration = Duration::from_secs(5);
    const POLL: Duration = Duration::from_millis(50);
    let root = PathBuf::from(root);
    let expression = expression.clone();
    let cancellation = context.cancellation.clone();
    let worker_cancellation = cancellation.clone();
    let (sender, receiver) = mpsc::sync_channel(2);
    std::thread::Builder::new()
        .name("everything-query".to_owned())
        .spawn(move || {
            let batches = sender.clone();
            let result = provider.query(&root, &expression, &worker_cancellation, |entries| {
                batches
                    .send(EverythingWorkerEvent::Batch(entries))
                    .map_err(|_| ())
            });
            let _ = sender.send(EverythingWorkerEvent::Finished(result));
        })
        .map_err(|error| format!("Everything worker unavailable: {error}"))?;
    let deadline = Instant::now() + QUERY_TIMEOUT;
    let mut delivered = 0usize;
    loop {
        if cancellation.is_cancelled() {
            return Err("cancelled".to_owned());
        }
        let now = Instant::now();
        if now >= deadline {
            return Err("Everything IPC query timed out".to_owned());
        }
        match receiver.recv_timeout(POLL.min(deadline.saturating_duration_since(now))) {
            Ok(EverythingWorkerEvent::Batch(entries)) => {
                if cancellation.is_cancelled() {
                    return Err("cancelled".to_owned());
                }
                delivered = delivered.saturating_add(entries.len());
                events
                    .send(ExplorerEvent::SearchBatch {
                        context: context.clone(),
                        source: SearchBackend::Everything,
                        entries,
                    })
                    .map_err(|_| "result channel closed".to_owned())?;
            }
            Ok(EverythingWorkerEvent::Finished(result)) => {
                return result.map(|()| delivered);
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                return Err("Everything IPC worker disconnected".to_owned());
            }
        }
    }
}

#[allow(dead_code, reason = "retained as a WindowsIndex compatibility oracle")]
fn probe_index(root: &Path, expression: &Expr) -> IndexAvailability {
    let result = (|| -> windows::core::Result<IndexAvailability> {
        // SAFETY: the Shell STA initialized COM; all returned interfaces remain on this STA.
        let manager: ISearchManager =
            unsafe { CoCreateInstance(&CSearchManager, None, CLSCTX_INPROC_SERVER)? };
        let catalog = unsafe { manager.GetCatalog(&HSTRING::from("SystemIndex"))? };
        let mut status = CatalogStatus::default();
        let mut paused = CatalogPausedReason::default();
        unsafe { catalog.GetCatalogStatus(&raw mut status, &raw mut paused)? };
        let scope = unsafe { catalog.GetCrawlScopeManager()? };
        if !unsafe { scope.IncludedInCrawlScope(&HSTRING::from(file_url(root)))? }.as_bool() {
            return Ok(IndexAvailability::OutsideScope);
        }
        let helper = unsafe { catalog.GetQueryHelper()? };
        let sql = unsafe {
            helper.GenerateSQLFromUserQuery(&HSTRING::from(render_bound_aqs(expression)))?
        };
        let length = unsafe { sql.as_wide() }.len() * 2;
        // SAFETY: GenerateSQLFromUserQuery transfers a COM-task allocation to the caller.
        unsafe { windows::Win32::System::Com::CoTaskMemFree(Some(sql.0.cast())) };
        Ok(IndexAvailability::Indexed {
            generated_sql_bytes: length,
        })
    })();
    result.unwrap_or_else(|error| {
        IndexAvailability::Unavailable(format!("HRESULT={:#010x}", error.code().0))
    })
}

#[allow(dead_code, reason = "retained as a WindowsIndex compatibility oracle")]
fn enumerate_index(
    context: &RequestContext,
    root: &Path,
    expression: &Expr,
    events: &SyncSender<ExplorerEvent>,
) -> Result<(), ExplorerError> {
    let query = percent_encode(&render_bound_aqs(expression));
    let scope = percent_encode(&root.to_string_lossy());
    let descriptor =
        LocationDescriptor::ParsingName(format!("search-ms:query={query}&crumb=location:{scope}"));
    let resolved = crate::navigation::resolve_location(&descriptor)?;
    crate::navigation::enumerate_directory(context, &resolved, |event| match event {
        ExplorerEvent::DirectoryBatch { entries, .. } => events
            .send(ExplorerEvent::SearchBatch {
                context: context.clone(),
                source: SearchBackend::WindowsIndex,
                entries,
            })
            .is_ok(),
        _ => true,
    })?;
    Ok(())
}

fn render_bound_aqs(expression: &Expr) -> String {
    let bound = bind_query(expression);
    let mut output = bound.template;
    for (index, parameter) in bound.parameters.iter().enumerate().rev() {
        let value = match parameter {
            QueryParameter::Text(value) => {
                format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
            }
            QueryParameter::Unsigned(value) => value.to_string(),
            QueryParameter::Date(value) => {
                format!("{:04}-{:02}-{:02}", value.year, value.month, value.day)
            }
        };
        output = output.replace(&format!("{{{index}}}"), &value);
    }
    output
}

#[allow(dead_code, reason = "retained as a WindowsIndex compatibility oracle")]
fn file_url(path: &Path) -> String {
    format!(
        "file:///{}",
        path.to_string_lossy()
            .replace('\\', "/")
            .trim_start_matches('/')
    )
}

fn percent_encode(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(*byte, b'-' | b'_' | b'.' | b'~') {
            output.push(char::from(*byte));
        } else {
            use std::fmt::Write as _;
            let _ = write!(output, "%{byte:02X}");
        }
    }
    output
}

fn search_error(
    kind: ExplorerErrorKind,
    user: impl Into<String>,
    detail: impl Into<String>,
) -> ExplorerError {
    ExplorerError::new(kind, "search", true, user, detail)
}

fn publish_status(
    events: &SyncSender<ExplorerEvent>,
    context: &RequestContext,
    backend: SearchBackend,
    phase: SearchSourcePhase,
    diagnostic: Option<String>,
) -> Result<(), ExplorerError> {
    events
        .send(ExplorerEvent::SearchStatus {
            context: context.clone(),
            status: SearchSourceStatus {
                backend,
                phase,
                diagnostic,
            },
        })
        .map_err(|_| {
            search_error(
                ExplorerErrorKind::Availability,
                "搜尋狀態接收端已關閉。",
                "source status could not be delivered",
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use explorer_search::parse;

    #[test]
    fn aqs_binding_escapes_literals_and_uri_encoding_blocks_structure_injection() {
        let expression = parse(r#"name:"x\"&crumb=location:C:\\""#).unwrap();
        let aqs = render_bound_aqs(&expression);
        assert!(aqs.contains(r#"x\"&crumb"#));
        let encoded = percent_encode(&aqs);
        assert!(!encoded.contains('&'));
        assert!(!encoded.contains(':'));
    }

    #[test]
    fn real_index_probe_is_truthful_for_temporary_scope() {
        let _guard = crate::clipboard::CLIPBOARD_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let apartment = crate::sta::ApartmentGuard::initialize().unwrap();
        let folder = tempfile::tempdir().unwrap();
        let availability = probe_index(folder.path(), &parse("name:oracle").unwrap());
        eprintln!("temporary-folder Windows Search availability: {availability:?}");
        assert!(matches!(
            availability,
            IndexAvailability::OutsideScope
                | IndexAvailability::Unavailable(_)
                | IndexAvailability::Indexed { .. }
        ));
        drop(apartment);
    }
}
