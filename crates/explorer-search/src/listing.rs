//! Bounded recursive search for providers which expose directory listings instead of indexes.
use crate::{
    BackendDiagnostic, Expr, FallbackConfig, SearchMetrics, SearchOutcome, SearchSource,
    SearchSourceState, matches_entry, parse,
};
use explorer_model::{
    CancellationToken, ExplorerEvent, FileEntry, LocationDescriptor, RequestContext, SearchBackend,
    SearchInput, SearchSourcePhase, SearchSourceStatus, SearchTerminal,
};
use std::{
    collections::{HashSet, VecDeque},
    sync::mpsc::SyncSender,
    time::Instant,
};

pub struct ListingEntry {
    pub entry: FileEntry,
    /// False for directory symlinks: matching a link must not follow it outside the scope.
    pub recurse: bool,
}

pub fn search_listing(
    root: &LocationDescriptor,
    expression: &Expr,
    cancellation: &CancellationToken,
    config: FallbackConfig,
    mut list: impl FnMut(&LocationDescriptor) -> Result<Vec<ListingEntry>, String>,
    mut deliver: impl FnMut(Vec<FileEntry>) -> Result<(), ()>,
) -> SearchOutcome {
    let started = Instant::now();
    let mut metrics = SearchMetrics::default();
    let diagnostic = |detail: String| BackendDiagnostic {
        source: SearchSource::FileSystemFallback,
        state: SearchSourceState::Failed,
        detail,
    };
    if config.batch_size == 0 || config.max_visited == 0 || config.max_pending_directories == 0 {
        return SearchOutcome::Failed(diagnostic("Invalid search limits".to_owned()));
    }
    let mut pending = VecDeque::from([(root.clone(), 0_usize)]);
    let mut seen = HashSet::new();
    let mut batch = Vec::new();
    let mut problem = None;
    while let Some((directory, depth)) = pending.pop_front() {
        if cancellation.is_cancelled() {
            return SearchOutcome::Cancelled(metrics);
        }
        if !seen.insert(directory.clone()) {
            continue;
        }
        let entries = match list(&directory) {
            Ok(entries) => entries,
            Err(detail) => {
                problem.get_or_insert(detail);
                continue;
            }
        };
        for item in entries {
            if cancellation.is_cancelled() {
                return SearchOutcome::Cancelled(metrics);
            }
            if metrics.visited >= config.max_visited {
                problem = Some("Search item limit reached".to_owned());
                pending.clear();
                break;
            }
            metrics.visited += 1;
            if item.recurse && item.entry.is_container && !seen.contains(&item.entry.location) {
                if depth >= 64 || pending.len() >= config.max_pending_directories {
                    problem.get_or_insert_with(|| "Search directory limit reached".to_owned());
                } else {
                    pending.push_back((item.entry.location.clone(), depth + 1));
                    metrics.max_pending_directories =
                        metrics.max_pending_directories.max(pending.len());
                }
            }
            if matches_entry(expression, &item.entry) {
                metrics.matched += 1;
                batch.push(item.entry);
                if batch.len() >= config.batch_size {
                    if deliver(std::mem::take(&mut batch)).is_err() {
                        return SearchOutcome::Failed(diagnostic(
                            "Search receiver closed".to_owned(),
                        ));
                    }
                    metrics.batches += 1;
                }
            }
        }
    }
    if cancellation.is_cancelled() {
        return SearchOutcome::Cancelled(metrics);
    }
    if !batch.is_empty() {
        if deliver(batch).is_err() {
            return SearchOutcome::Failed(diagnostic("Search receiver closed".to_owned()));
        }
        metrics.batches += 1;
    }
    metrics.elapsed = started.elapsed();
    match problem {
        Some(detail) if metrics.visited == 0 => SearchOutcome::Failed(diagnostic(detail)),
        Some(detail) => {
            let mut diagnostic = diagnostic(detail);
            diagnostic.state = SearchSourceState::Partial;
            SearchOutcome::Partial {
                metrics,
                diagnostic,
            }
        }
        None => SearchOutcome::Finished(metrics),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn location(provider: &str, parts: &[&str]) -> LocationDescriptor {
        LocationDescriptor::try_virtual(
            provider,
            [1; 16],
            1,
            None,
            parts.iter().map(|part| (*part).to_owned()).collect(),
        )
        .unwrap()
    }
    fn item(provider: &str, parts: &[&str], directory: bool, recurse: bool) -> ListingEntry {
        let name = *parts.last().unwrap();
        ListingEntry {
            recurse,
            entry: FileEntry {
                id: explorer_model::ShellItemId::from_provider_bytes(parts.join("/").into_bytes())
                    .unwrap(),
                display_name: name.to_owned(),
                location: location(provider, parts),
                is_container: directory,
                metadata: explorer_model::FileEntryMetadata {
                    size_bytes: Some(12000),
                    ..Default::default()
                },
            },
        }
    }
    #[test]
    fn provider_listing_search_recurses_matches_properties_and_keeps_readable_siblings() {
        for provider in ["sftp", "ftp", "adb", "gdrive", "rust-7z:resource"] {
            let root = location(provider, &[]);
            let mut visited = Vec::new();
            let mut hits = Vec::new();
            let outcome = search_listing(
                &root,
                &parse("動畫 type:mp4 size:>10KB").unwrap(),
                &CancellationToken::new(),
                FallbackConfig::default(),
                |directory| {
                    visited.push(directory.clone());
                    if directory == &root {
                        Ok(vec![
                            item(provider, &["nested"], true, true),
                            item(provider, &["denied"], true, true),
                            item(provider, &["link"], true, false),
                        ])
                    } else if directory == &location(provider, &["denied"]) {
                        Err("fixture access denied".to_owned())
                    } else {
                        Ok(vec![item(
                            provider,
                            &["nested", "測試動畫.mp4"],
                            false,
                            false,
                        )])
                    }
                },
                |batch| {
                    hits.extend(batch);
                    Ok(())
                },
            );
            assert!(matches!(outcome, SearchOutcome::Partial { .. }));
            assert_eq!(hits.len(), 1);
            assert_eq!(hits[0].display_name, "測試動畫.mp4");
            assert!(!visited.contains(&location(provider, &["link"])));
        }
    }
    #[test]
    fn provider_search_stops_during_delivery_and_reports_root_read_errors() {
        let root = location("sftp", &[]);
        let token = CancellationToken::new();
        let mut delivered = 0;
        let outcome = search_listing(
            &root,
            &parse("file").unwrap(),
            &token,
            FallbackConfig {
                batch_size: 1,
                ..Default::default()
            },
            |_| {
                Ok(vec![
                    item("sftp", &["file.txt"], false, false),
                    item("sftp", &["file2.txt"], false, false),
                ])
            },
            |_| {
                delivered += 1;
                token.cancel();
                Ok(())
            },
        );
        assert!(matches!(outcome, SearchOutcome::Cancelled(_)));
        assert_eq!(delivered, 1);
        let outcome = search_listing(
            &root,
            &parse("file").unwrap(),
            &CancellationToken::new(),
            FallbackConfig::default(),
            |_| Err("fixture inaccessible".to_owned()),
            |_| panic!("failed root must not deliver matches"),
        );
        assert!(matches!(outcome, SearchOutcome::Failed(_)));
    }

    #[test]
    fn listing_search_bounds_cancellation_and_terminal_count() {
        let root = location("adb", &[]);
        let outcome = search_listing(
            &root,
            &parse("file").unwrap(),
            &CancellationToken::new(),
            FallbackConfig {
                max_visited: 1,
                ..Default::default()
            },
            |_| {
                Ok(vec![
                    item("adb", &["file.txt"], false, false),
                    item("adb", &["file2.txt"], false, false),
                ])
            },
            |_| Ok(()),
        );
        assert!(matches!(outcome, SearchOutcome::Partial { .. }));
        for invalid in [false, true] {
            let context = RequestContext::new(
                explorer_model::TabId::new(),
                explorer_model::Generation::new(1),
            );
            if !invalid {
                context.cancellation.cancel();
            }
            let (sender, receiver) = std::sync::mpsc::sync_channel(16);
            execute_listing_search(
                &context,
                &root,
                &SearchInput::new(if invalid { "size:invalid" } else { "file" }),
                &sender,
                |_| panic!("cancelled/invalid search must not list"),
            );
            let events: Vec<_> = receiver.try_iter().collect();
            assert_eq!(
                events
                    .iter()
                    .filter(|event| matches!(event, ExplorerEvent::SearchFinished { .. }))
                    .count(),
                1
            );
            assert!(
                matches!(
                    events.last(),
                    Some(ExplorerEvent::SearchFinished {
                        outcome: SearchTerminal::Failed(_),
                        ..
                    })
                ) == invalid
            );
        }
    }
}

/// App-provider bridge: exactly one search terminal, including cancellation and parse errors.
pub fn execute_listing_search(
    context: &RequestContext,
    root: &LocationDescriptor,
    input: &SearchInput,
    events: &SyncSender<ExplorerEvent>,
    list: impl FnMut(&LocationDescriptor) -> Result<Vec<ListingEntry>, String>,
) {
    let error = |detail: String| {
        explorer_common::ExplorerError::new(
            explorer_common::ExplorerErrorKind::Availability,
            "search directory listing",
            true,
            "搜尋未能完成所有位置。",
            detail,
        )
    };
    let parsed = parse(input.as_str());
    let terminal = match parsed {
        Err(detail) => SearchTerminal::Failed(explorer_common::ExplorerError::new(
            explorer_common::ExplorerErrorKind::Input,
            "parse search query",
            true,
            format!("搜尋條件有誤：{}", detail.message),
            detail.to_string(),
        )),
        Ok(expression) => {
            let _ = events.send(ExplorerEvent::SearchStatus {
                context: context.clone(),
                status: SearchSourceStatus {
                    backend: SearchBackend::FileSystemFallback,
                    phase: SearchSourcePhase::Active,
                    diagnostic: None,
                },
            });
            match search_listing(
                root,
                &expression,
                &context.cancellation,
                FallbackConfig::default(),
                list,
                |entries| {
                    events
                        .send(ExplorerEvent::SearchBatch {
                            context: context.clone(),
                            source: SearchBackend::FileSystemFallback,
                            entries,
                        })
                        .map_err(|_| ())
                },
            ) {
                SearchOutcome::Finished(_) => SearchTerminal::Finished,
                SearchOutcome::Cancelled(_) => SearchTerminal::Cancelled,
                SearchOutcome::Partial { diagnostic, .. } => {
                    SearchTerminal::Partial(error(diagnostic.detail))
                }
                SearchOutcome::Failed(diagnostic) => {
                    SearchTerminal::Failed(error(diagnostic.detail))
                }
            }
        }
    };
    let phase = match &terminal {
        SearchTerminal::Finished => SearchSourcePhase::Complete,
        SearchTerminal::Cancelled => SearchSourcePhase::Cancelled,
        SearchTerminal::Partial(_) => SearchSourcePhase::Partial,
        SearchTerminal::Failed(_) => SearchSourcePhase::Failed,
    };
    let _ = events.send(ExplorerEvent::SearchStatus {
        context: context.clone(),
        status: SearchSourceStatus {
            backend: SearchBackend::FileSystemFallback,
            phase,
            diagnostic: None,
        },
    });
    let _ = events.send(ExplorerEvent::SearchFinished {
        context: context.clone(),
        outcome: terminal,
    });
}
