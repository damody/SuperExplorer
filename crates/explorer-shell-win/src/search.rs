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
    ExplorerEvent, LocationDescriptor, RequestContext, SearchBackend, SearchEngineAvailability,
    SearchEngineFacts, SearchEnginePreference, SearchInput, SearchSourcePhase, SearchSourceStatus,
    SearchTerminal,
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
pub fn probe_search_engine_availability(location: &LocationDescriptor) -> SearchEngineAvailability {
    let Some(path) = location.path() else {
        return SearchEngineAvailability::all_unavailable().for_location(location);
    };
    if explorer_model::network_unc_parts(path).is_some() {
        return SearchEngineAvailability::all_unavailable().for_location(location);
    }
    let indexed_drive = indexable_drive_letter(path);
    SearchEngineAvailability::from_facts(SearchEngineFacts {
        has_local_filesystem_path: true,
        everything_available: indexed_drive.is_some()
            && crate::everything::EverythingProvider::open_adjacent().is_ok(),
        mft_index_available: indexed_drive
            .is_some_and(|letter| explorer_mft::mft_sqlite_path(letter).is_file()),
    })
}

fn indexable_volume_letter(
    requested: char,
    volume_root: &str,
    filesystem: &str,
    drive_type: u32,
) -> Option<char> {
    // Folder-mounted volumes and redirected drives cannot use the containing drive's MFT.
    (matches!(drive_type, 2 | 3)
        && filesystem.eq_ignore_ascii_case("NTFS")
        && volume_root.eq_ignore_ascii_case(&format!("{requested}:\\")))
    .then_some(requested)
}

fn indexable_drive_letter(path: &Path) -> Option<char> {
    use std::os::windows::ffi::OsStrExt as _;
    use windows::{
        Win32::Storage::FileSystem::{GetDriveTypeW, GetVolumeInformationW, GetVolumePathNameW},
        core::PCWSTR,
    };
    let requested = explorer_mft::volume_drive_letter(path)?;
    let wide: Vec<_> = path.as_os_str().encode_wide().chain([0]).collect();
    let mut volume = vec![0_u16; 32_768];
    let mut filesystem = [0_u16; 32];
    // SAFETY: the input is NUL-terminated and both output buffers remain live for each call.
    unsafe {
        GetVolumePathNameW(PCWSTR(wide.as_ptr()), &mut volume).ok()?;
    }
    // Reject redirected/offline drives before asking their server for volume metadata.
    // SAFETY: the successful call above wrote a NUL-terminated root into this live buffer.
    let drive_type = unsafe { GetDriveTypeW(PCWSTR(volume.as_ptr())) };
    if !matches!(drive_type, 2 | 3) {
        return None;
    }
    // SAFETY: the root and writable filesystem-name buffer remain live for the call.
    unsafe {
        GetVolumeInformationW(
            PCWSTR(volume.as_ptr()),
            None,
            None,
            None,
            None,
            Some(&mut filesystem),
        )
        .ok()?;
    }
    let decode = |value: &[u16]| {
        String::from_utf16_lossy(
            &value[..value
                .iter()
                .position(|unit| *unit == 0)
                .unwrap_or(value.len())],
        )
    };
    indexable_volume_letter(
        requested,
        &decode(&volume),
        &decode(&filesystem),
        drive_type,
    )
}

pub(crate) fn execute_with_terminals<P: RequiredTerminalPublisher>(
    context: &RequestContext,
    location: &LocationDescriptor,
    input: &SearchInput,
    engine: SearchEnginePreference,
    events: &SyncSender<ExplorerEvent>,
    terminals: &P,
) -> Result<(), ExplorerError> {
    let expression = parse(input.as_str()).map_err(|error| {
        ExplorerError::new(
            ExplorerErrorKind::Input,
            "parse search query",
            true,
            format!("搜尋條件有誤：{}", error.message),
            error.to_string(),
        )
    })?;
    let Some(root) = location.path() else {
        return execute_namespace_search(context, location, expression, events, terminals);
    };
    let availability = probe_search_engine_availability(location);
    let engine = availability.resolve(engine);
    if !availability.support(engine).is_available() {
        let backend = engine.as_backend();
        publish_status(
            events,
            context,
            backend,
            SearchSourcePhase::Unavailable,
            Some(engine.unsupported_hint_id().to_owned()),
        )?;
        terminals.publish_terminal(ExplorerEvent::SearchFinished {
            context: context.clone(),
            outcome: SearchTerminal::Failed(search_error(
                ExplorerErrorKind::Availability,
                "目前位置無法使用所選的搜尋引擎。",
                engine.unsupported_hint_id(),
            )),
        });
        return Ok(());
    }

    match engine {
        SearchEnginePreference::Everything => {
            return execute_everything_only(context, root, &expression, events, terminals);
        }
        SearchEnginePreference::Mft => {
            return execute_mft_only(context, root, &expression, events, terminals);
        }
        SearchEnginePreference::FileEnumeration => {
            return execute_file_enumeration(context, root, expression, events, terminals);
        }
    }
}

fn execute_namespace_search<P: RequiredTerminalPublisher>(
    context: &RequestContext,
    location: &LocationDescriptor,
    expression: Expr,
    events: &SyncSender<ExplorerEvent>,
    terminals: &P,
) -> Result<(), ExplorerError> {
    publish_status(
        events,
        context,
        SearchBackend::FileSystemFallback,
        SearchSourcePhase::Active,
        None,
    )?;
    let outcome = explorer_search::search_listing(
        location,
        &expression,
        &context.cancellation,
        FallbackConfig::default(),
        |directory| {
            let resolved = crate::navigation::resolve_location(directory)
                .map_err(|_| "Unable to read a Shell search directory".to_owned())?;
            let mut rows = Vec::new();
            let complete =
                crate::navigation::enumerate_directory(context, &resolved, |event| {
                    if let ExplorerEvent::DirectoryBatch { entries, .. } = event {
                        rows.extend(entries.into_iter().map(|entry| {
                            explorer_search::ListingEntry {
                                recurse: entry.is_container
                                    && entry.metadata.filesystem_attributes & 0x400 == 0,
                                entry,
                            }
                        }));
                    }
                    rows.len() <= 65_536
                })
                .map_err(|_| "Unable to enumerate a Shell search directory".to_owned())?;
            if !complete && !context.cancellation.is_cancelled() {
                return Err("Shell directory search limit reached".to_owned());
            }
            Ok(rows)
        },
        |entries| {
            events
                .send(ExplorerEvent::SearchBatch {
                    context: context.clone(),
                    source: SearchBackend::FileSystemFallback,
                    entries,
                })
                .map_err(|_| ())
        },
    );
    let terminal = match outcome {
        SearchOutcome::Finished(_) => SearchTerminal::Finished,
        SearchOutcome::Cancelled(_) => SearchTerminal::Cancelled,
        SearchOutcome::Partial { diagnostic, .. } => SearchTerminal::Partial(search_error(
            ExplorerErrorKind::Availability,
            "搜尋只完成部分位置。",
            diagnostic.detail,
        )),
        SearchOutcome::Failed(diagnostic) => SearchTerminal::Failed(search_error(
            ExplorerErrorKind::Availability,
            "搜尋無法完成。",
            diagnostic.detail,
        )),
    };
    let phase = match &terminal {
        SearchTerminal::Finished => SearchSourcePhase::Complete,
        SearchTerminal::Cancelled => SearchSourcePhase::Cancelled,
        SearchTerminal::Partial(_) => SearchSourcePhase::Partial,
        SearchTerminal::Failed(_) => SearchSourcePhase::Failed,
    };
    publish_status(
        events,
        context,
        SearchBackend::FileSystemFallback,
        phase,
        None,
    )?;
    terminals.publish_terminal(ExplorerEvent::SearchFinished {
        context: context.clone(),
        outcome: terminal,
    });
    Ok(())
}

fn execute_everything_only<P: RequiredTerminalPublisher>(
    context: &RequestContext,
    root: &Path,
    expression: &Expr,
    events: &SyncSender<ExplorerEvent>,
    terminals: &P,
) -> Result<(), ExplorerError> {
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
                Ok(result_count) => {
                    publish_status(
                        events,
                        context,
                        SearchBackend::Everything,
                        SearchSourcePhase::Complete,
                        None,
                    )?;
                    if result_count == 0 {
                        // A working IPC endpoint does not prove this scope is indexed.
                        return execute_file_enumeration(
                            context,
                            root,
                            expression.clone(),
                            events,
                            terminals,
                        );
                    }
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
                Err(detail) => {
                    publish_status(
                        events,
                        context,
                        SearchBackend::Everything,
                        SearchSourcePhase::Unavailable,
                        Some(detail.clone()),
                    )?;
                    execute_file_enumeration(context, root, expression.clone(), events, terminals)
                }
            }
        }
        Err(detail) => {
            publish_status(
                events,
                context,
                SearchBackend::Everything,
                SearchSourcePhase::Unavailable,
                Some(detail.clone()),
            )?;
            execute_file_enumeration(context, root, expression.clone(), events, terminals)
        }
    }
}

fn execute_file_enumeration<P: RequiredTerminalPublisher>(
    context: &RequestContext,
    root: &Path,
    expression: Expr,
    events: &SyncSender<ExplorerEvent>,
    terminals: &P,
) -> Result<(), ExplorerError> {
    let request = SearchRequest {
        root: root.to_owned(),
        expression,
        cancellation: context.cancellation.clone(),
    };
    publish_status(
        events,
        context,
        SearchBackend::FileSystemFallback,
        SearchSourcePhase::Active,
        None,
    )?;
    let outcome = search_filesystem(
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
                    source: SearchBackend::FileSystemFallback,
                    entries: batch.hits.into_iter().map(|hit| hit.entry).collect(),
                })
                .map_err(|_| ())
        },
    );
    let terminal = match outcome {
        SearchOutcome::Finished(_) => {
            publish_status(
                events,
                context,
                SearchBackend::FileSystemFallback,
                SearchSourcePhase::Complete,
                None,
            )?;
            SearchTerminal::Finished
        }
        SearchOutcome::Cancelled(_) => {
            publish_status(
                events,
                context,
                SearchBackend::FileSystemFallback,
                SearchSourcePhase::Cancelled,
                None,
            )?;
            SearchTerminal::Cancelled
        }
        SearchOutcome::Partial { diagnostic, .. } => {
            publish_status(
                events,
                context,
                SearchBackend::FileSystemFallback,
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
                SearchBackend::FileSystemFallback,
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

fn execute_mft_only<P: RequiredTerminalPublisher>(
    context: &RequestContext,
    root: &Path,
    expression: &Expr,
    events: &SyncSender<ExplorerEvent>,
    terminals: &P,
) -> Result<(), ExplorerError> {
    publish_status(
        events,
        context,
        SearchBackend::Mft,
        SearchSourcePhase::Active,
        None,
    )?;
    let outcome = match load_mft_index_for_search(root) {
        Ok(index) => search_mft_index(context, root, expression, &index, events),
        Err(detail) => {
            publish_status(
                events,
                context,
                SearchBackend::Mft,
                SearchSourcePhase::Unavailable,
                Some(detail.clone()),
            )?;
            return execute_file_enumeration(context, root, expression.clone(), events, terminals);
        }
    };
    if matches!(&outcome, SearchOutcome::Finished(metrics) if metrics.matched == 0) {
        publish_status(
            events,
            context,
            SearchBackend::Mft,
            SearchSourcePhase::Complete,
            None,
        )?;
        return execute_file_enumeration(context, root, expression.clone(), events, terminals);
    }
    let terminal = match outcome {
        SearchOutcome::Finished(_) => {
            publish_status(
                events,
                context,
                SearchBackend::Mft,
                SearchSourcePhase::Complete,
                None,
            )?;
            SearchTerminal::Finished
        }
        SearchOutcome::Cancelled(_) => {
            publish_status(
                events,
                context,
                SearchBackend::Mft,
                SearchSourcePhase::Cancelled,
                None,
            )?;
            SearchTerminal::Cancelled
        }
        SearchOutcome::Partial { diagnostic, .. } => {
            publish_status(
                events,
                context,
                SearchBackend::Mft,
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
                SearchBackend::Mft,
                SearchSourcePhase::Failed,
                Some(diagnostic.detail.clone()),
            )?;
            SearchTerminal::Failed(search_error(
                ExplorerErrorKind::Availability,
                "MFT 搜尋無法完成。",
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

fn load_mft_index_for_search(
    root: &Path,
) -> Result<explorer_mft::mft_size_map::MftIndexV1, String> {
    let letter = indexable_drive_letter(root)
        .ok_or_else(|| "MFT search requires a local drive-letter path".to_owned())?;
    let path = explorer_mft::mft_sqlite_path(letter);
    explorer_mft::mft_sqlite::MftSqliteStoreV1::load_index_read_only_for_search(
        &path,
        &explorer_mft::mft_index_cache_root(),
    )
}

fn search_mft_index(
    context: &RequestContext,
    root: &Path,
    expression: &Expr,
    index: &explorer_mft::mft_size_map::MftIndexV1,
    events: &SyncSender<ExplorerEvent>,
) -> SearchOutcome {
    let Some(letter) = explorer_mft::volume_drive_letter(root) else {
        return SearchOutcome::Failed(explorer_search::BackendDiagnostic {
            source: explorer_search::SearchSource::FileSystemFallback,
            state: explorer_search::SearchSourceState::Failed,
            detail: "MFT search requires a local drive-letter path".to_owned(),
        });
    };
    let volume_root = PathBuf::from(format!("{}:\\", letter));
    let mut batch = Vec::new();
    let mut matched = 0usize;
    for (ordinal, entry) in index.entries().values().enumerate() {
        if context.cancellation.is_cancelled() {
            return SearchOutcome::Cancelled(explorer_search::SearchMetrics::default());
        }
        if ordinal % 4_096 == 0 && context.cancellation.is_cancelled() {
            return SearchOutcome::Cancelled(explorer_search::SearchMetrics::default());
        }
        let Some(path) = index.reconstructed_path(entry.reference, &volume_root) else {
            continue;
        };
        if !path_is_under(&path, root) {
            continue;
        }
        let bytes = crate::navigation::filesystem_identity(&path, entry.is_directory)
            .unwrap_or_else(|_| crate::navigation::fallback_filesystem_identity(&path));
        let Some(id) = explorer_model::ShellItemId::from_provider_bytes(bytes) else {
            continue;
        };
        let file_entry = explorer_model::FileEntry {
            id,
            display_name: entry.name.clone(),
            location: LocationDescriptor::FileSystem(path),
            is_container: entry.is_directory,
            metadata: explorer_model::FileEntryMetadata {
                size_bytes: (!entry.is_directory).then_some(entry.logical_bytes),
                ..Default::default()
            },
        };
        if !explorer_search::matches_entry(expression, &file_entry) {
            continue;
        }
        batch.push(file_entry);
        matched = matched.saturating_add(1);
        if batch.len() >= 64 {
            if events
                .send(ExplorerEvent::SearchBatch {
                    context: context.clone(),
                    source: SearchBackend::Mft,
                    entries: std::mem::take(&mut batch),
                })
                .is_err()
            {
                return SearchOutcome::Failed(explorer_search::BackendDiagnostic {
                    source: explorer_search::SearchSource::FileSystemFallback,
                    state: explorer_search::SearchSourceState::Failed,
                    detail: "result channel closed".to_owned(),
                });
            }
        }
    }
    if !batch.is_empty()
        && events
            .send(ExplorerEvent::SearchBatch {
                context: context.clone(),
                source: SearchBackend::Mft,
                entries: batch,
            })
            .is_err()
    {
        return SearchOutcome::Failed(explorer_search::BackendDiagnostic {
            source: explorer_search::SearchSource::FileSystemFallback,
            state: explorer_search::SearchSourceState::Failed,
            detail: "result channel closed".to_owned(),
        });
    }
    let mut metrics = explorer_search::SearchMetrics::default();
    metrics.matched = matched;
    SearchOutcome::Finished(metrics)
}

fn path_is_under(path: &Path, root: &Path) -> bool {
    let path = normalize_search_path(path);
    let root = normalize_search_path(root);
    path != root && (path.starts_with(&(root.clone() + "\\")) || path.starts_with(&(root + "/")))
}

fn normalize_search_path(path: &Path) -> String {
    path.to_string_lossy()
        .trim_end_matches(['\\', '/'])
        .to_lowercase()
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
            Ok(EverythingWorkerEvent::Batch(mut entries)) => {
                if cancellation.is_cancelled() {
                    return Err("cancelled".to_owned());
                }
                delivered = delivered.saturating_add(entries.len());
                for entry in &mut entries {
                    if let Some(path) = entry.location.path() {
                        let bytes =
                            crate::navigation::filesystem_identity(path, entry.is_container)
                                .unwrap_or_else(|_| {
                                    crate::navigation::fallback_filesystem_identity(path)
                                });
                        if let Some(id) = explorer_model::ShellItemId::from_provider_bytes(bytes) {
                            entry.id = id;
                        }
                    }
                }
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
    #[ignore = "requires SUPEREXPLORER_TEST_UNC pointing to a readable populated network folder"]
    fn live_unc_search_accepts_filesystem_and_shell_parsing_names() {
        let root = std::env::var("SUPEREXPLORER_TEST_UNC").expect("set the live UNC test root");
        let query = std::env::var("SUPEREXPLORER_TEST_QUERY").unwrap_or_else(|_| "*".to_owned());
        assert!(root.starts_with(r"\\"));
        for location in [
            LocationDescriptor::file_system(&root),
            LocationDescriptor::ParsingName(root.clone()),
        ] {
            let context = RequestContext::new(
                explorer_model::TabId::new(),
                explorer_model::Generation::new(1),
            );
            let (sender, receiver) = mpsc::sync_channel(32);
            let worker_context = context.clone();
            let worker_query = query.clone();
            let worker = std::thread::spawn(move || {
                let _apartment = crate::sta::ApartmentGuard::initialize().unwrap();
                execute_with_terminals(
                    &worker_context,
                    &location,
                    &SearchInput::new(worker_query),
                    SearchEnginePreference::Mft,
                    &sender,
                    &sender,
                )
                .unwrap();
            });
            let mut received_results = false;
            loop {
                match receiver.recv_timeout(Duration::from_secs(30)) {
                    Ok(ExplorerEvent::SearchBatch { entries, source, .. }) => {
                        assert_eq!(source, SearchBackend::FileSystemFallback);
                        assert!(!entries.is_empty());
                        assert!(entries.iter().all(|entry| explorer_search::matches_entry(
                            &parse(&query).unwrap(), entry
                        )));
                        received_results = true;
                        context.cancellation.cancel();
                    }
                    Ok(ExplorerEvent::SearchFinished { outcome, .. }) => {
                        assert!(received_results, "search ended without results: {outcome:?}");
                        assert!(matches!(
                            outcome,
                            SearchTerminal::Cancelled | SearchTerminal::Finished
                        ));
                        break;
                    }
                    Ok(_) => {}
                    Err(error) => {
                        context.cancellation.cancel();
                        panic!("live UNC search did not respond: {error}");
                    }
                }
            }
            worker.join().unwrap();
        }
    }

    #[test]
    fn index_capabilities_reject_non_ntfs_redirected_and_folder_mounted_volumes() {
        for filesystem in ["FAT", "FAT32", "exFAT", "ReFS", "CDFS", ""] {
            assert_eq!(
                indexable_volume_letter('E', r"E:\", filesystem, 3),
                None,
                "{filesystem}"
            );
        }
        for kind in [0, 1, 4, 5, 6] {
            assert_eq!(indexable_volume_letter('E', r"E:\", "NTFS", kind), None);
        }
        assert_eq!(
            indexable_volume_letter('C', r"C:\mounted-volume\", "NTFS", 3),
            None
        );
        assert_eq!(indexable_volume_letter('C', r"D:\", "NTFS", 3), None);
        assert_eq!(indexable_volume_letter('E', r"E:\", "NTFS", 2), Some('E'));
        assert_eq!(indexable_volume_letter('C', r"C:\", "ntfs", 3), Some('C'));
    }

    #[test]
    fn unavailable_index_engines_fall_back_and_emit_one_terminal() {
        let _lock = crate::clipboard::CLIPBOARD_TEST_LOCK.lock().unwrap();
        let _everything = crate::everything::force_unavailable_for_test();
        let fixture = tempfile::tempdir().unwrap();
        std::fs::write(fixture.path().join("fallback-match.txt"), b"fixture").unwrap();
        // Extended paths are enumerable even though they are not drive-letter MFT scopes.
        let root = fixture.path().canonicalize().unwrap();
        for engine in [
            SearchEnginePreference::Everything,
            SearchEnginePreference::Mft,
        ] {
            let context = RequestContext::new(
                explorer_model::TabId::new(),
                explorer_model::Generation::new(1),
            );
            let (sender, receiver) = mpsc::sync_channel(32);
            let expression = parse("fallback-match").unwrap();
            match engine {
                SearchEnginePreference::Everything => {
                    execute_everything_only(&context, &root, &expression, &sender, &sender)
                }
                _ => execute_mft_only(&context, &root, &expression, &sender, &sender),
            }
            .unwrap();
            let events: Vec<_> = receiver.try_iter().collect();
            assert!(events.iter().any(|event| matches!(event, ExplorerEvent::SearchBatch { source: SearchBackend::FileSystemFallback, entries, .. } if entries.len() == 1)));
            assert_eq!(
                events
                    .iter()
                    .filter(|event| matches!(event, ExplorerEvent::SearchFinished { .. }))
                    .count(),
                1
            );
            assert!(matches!(
                events.last(),
                Some(ExplorerEvent::SearchFinished {
                    outcome: SearchTerminal::Finished,
                    ..
                })
            ));
        }
    }

    #[test]
    fn shell_namespace_search_uses_listing_instead_of_requiring_a_path() {
        let _apartment = crate::sta::ApartmentGuard::initialize().unwrap();
        let fixture = tempfile::tempdir().unwrap();
        std::fs::create_dir(fixture.path().join("nested")).unwrap();
        std::fs::write(
            fixture.path().join("nested").join("namespace-match.txt"),
            b"fixture",
        )
        .unwrap();
        let location =
            LocationDescriptor::ParsingName(fixture.path().to_string_lossy().into_owned());
        let context = RequestContext::new(
            explorer_model::TabId::new(),
            explorer_model::Generation::new(1),
        );
        let (sender, receiver) = mpsc::sync_channel(32);
        execute_with_terminals(
            &context,
            &location,
            &SearchInput::new("namespace-match"),
            SearchEnginePreference::Mft,
            &sender,
            &sender,
        )
        .unwrap();
        let events: Vec<_> = receiver.try_iter().collect();
        assert!(events.iter().any(|event| matches!(event, ExplorerEvent::SearchBatch { entries, .. } if entries.iter().any(|entry| entry.display_name == "namespace-match.txt"))));
        assert!(matches!(
            events.last(),
            Some(ExplorerEvent::SearchFinished {
                outcome: SearchTerminal::Finished,
                ..
            })
        ));
    }

    #[test]
    fn unc_search_uses_file_enumeration_without_local_indexes() {
        for path in [
            r"\\damody\gdrive\動畫",
            r"\\?\UNC\damody\gdrive\動畫",
            r"\\192.168.0.239\share",
        ] {
            let availability =
                probe_search_engine_availability(&LocationDescriptor::file_system(path));
            assert!(!availability.everything.is_available());
            assert!(!availability.mft.is_available());
            for selected in [
                SearchEnginePreference::Everything,
                SearchEnginePreference::Mft,
                SearchEnginePreference::FileEnumeration,
            ] {
                assert_eq!(
                    availability.resolve(selected),
                    SearchEnginePreference::FileEnumeration
                );
            }
        }
    }

    #[test]
    fn file_enumeration_search_returns_nested_unicode_results_without_an_index() {
        let fixture = tempfile::tempdir().unwrap();
        let folder = fixture.path().join("動畫");
        std::fs::create_dir(&folder).unwrap();
        let target = folder.join("測試動畫.mp4");
        std::fs::write(&target, b"fixture").unwrap();
        std::fs::write(folder.join("unrelated.txt"), b"fixture").unwrap();
        let context = RequestContext::new(
            explorer_model::TabId::new(),
            explorer_model::Generation::new(1),
        );
        let (events, receiver) = mpsc::sync_channel(16);
        execute_file_enumeration(
            &context,
            fixture.path(),
            parse("測試").unwrap(),
            &events,
            &events,
        )
        .unwrap();
        drop(events);
        let received: Vec<_> = receiver.try_iter().collect();
        let matches: Vec<_> = received
            .iter()
            .filter_map(|event| match event {
                ExplorerEvent::SearchBatch { entries, .. } => Some(entries),
                _ => None,
            })
            .flatten()
            .collect();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].location.path(), Some(target.as_path()));
        assert!(matches!(
            received.last(),
            Some(ExplorerEvent::SearchFinished {
                outcome: SearchTerminal::Finished,
                ..
            })
        ));
    }

    #[test]
    fn mft_index_search_matches_reconstructed_filenames_under_the_root() {
        use explorer_mft::mft_size_map::{MftEntryV1, MftIndexV1};
        use std::collections::BTreeMap;
        let root = MftEntryV1 {
            reference: 5,
            parent_reference: 5,
            name: String::new(),
            logical_bytes: 0,
            allocated_bytes: 0,
            is_directory: true,
        };
        let users = MftEntryV1 {
            reference: 12,
            parent_reference: 5,
            name: "Users".to_owned(),
            logical_bytes: 0,
            allocated_bytes: 0,
            is_directory: true,
        };
        let hit = MftEntryV1 {
            reference: 34,
            parent_reference: 12,
            name: "報告.txt".to_owned(),
            logical_bytes: 8,
            allocated_bytes: 4096,
            is_directory: false,
        };
        let miss = MftEntryV1 {
            reference: 35,
            parent_reference: 12,
            name: "other.bin".to_owned(),
            logical_bytes: 4,
            allocated_bytes: 4096,
            is_directory: false,
        };
        let index = MftIndexV1::from_entries(BTreeMap::from([
            (root.reference, root),
            (users.reference, users),
            (hit.reference, hit),
            (miss.reference, miss),
        ]));
        let context = RequestContext::new(
            explorer_model::TabId::new(),
            explorer_model::Generation::new(1),
        );
        let (events, receiver) = mpsc::sync_channel(8);
        let expression = parse("報告").unwrap();
        let outcome = search_mft_index(
            &context,
            Path::new(r"C:\Users"),
            &expression,
            &index,
            &events,
        );
        assert!(matches!(outcome, SearchOutcome::Finished(_)));
        let mut names = Vec::new();
        while let Ok(event) = receiver.try_recv() {
            if let ExplorerEvent::SearchBatch {
                entries, source, ..
            } = event
            {
                assert_eq!(source, SearchBackend::Mft);
                names.extend(entries.into_iter().map(|entry| entry.display_name));
            }
        }
        assert_eq!(names, ["報告.txt"]);
    }

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
