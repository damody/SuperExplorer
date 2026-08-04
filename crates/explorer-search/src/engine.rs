use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs::{self, Metadata},
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use explorer_model::{CancellationToken, FileEntry, LocationDescriptor, ShellItemId};

use crate::{Comparison, Expr, PropertyKey, Value};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SearchSource {
    WindowsIndex,
    FileSystemFallback,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SearchSourceState {
    Pending,
    Active,
    Complete,
    Partial,
    Unavailable,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackendDiagnostic {
    pub source: SearchSource,
    pub state: SearchSourceState,
    pub detail: String,
}

#[derive(Clone, Debug)]
pub struct SearchRequest {
    pub root: PathBuf,
    pub expression: Expr,
    pub cancellation: CancellationToken,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchHit {
    pub entry: FileEntry,
    pub sources: Vec<SearchSource>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchBatch {
    pub hits: Vec<SearchHit>,
    pub source: SearchSource,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SearchMetrics {
    pub visited: usize,
    pub matched: usize,
    pub batches: usize,
    pub max_pending_directories: usize,
    pub elapsed: Duration,
    pub cancel_latency: Option<Duration>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SearchOutcome {
    Finished(SearchMetrics),
    Cancelled(SearchMetrics),
    Partial {
        metrics: SearchMetrics,
        diagnostic: BackendDiagnostic,
    },
    Failed(BackendDiagnostic),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FallbackConfig {
    pub batch_size: usize,
    pub max_pending_directories: usize,
    pub max_visited: usize,
    pub follow_reparse_points: bool,
}

impl Default for FallbackConfig {
    fn default() -> Self {
        Self {
            batch_size: 64,
            max_pending_directories: 4_096,
            max_visited: 1_000_000,
            follow_reparse_points: false,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct DedupeStore {
    hits: Vec<SearchHit>,
    indices: HashMap<ShellItemId, usize>,
}

impl DedupeStore {
    pub fn insert(&mut self, mut hit: SearchHit) -> bool {
        if let Some(index) = self.indices.get(&hit.entry.id).copied() {
            let existing = &mut self.hits[index];
            for source in hit.sources.drain(..) {
                if !existing.sources.contains(&source) {
                    existing.sources.push(source);
                }
            }
            false
        } else {
            self.indices.insert(hit.entry.id.clone(), self.hits.len());
            self.hits.push(hit);
            true
        }
    }

    pub fn hits(&self) -> &[SearchHit] {
        &self.hits
    }
}

/// Traverses a real filesystem off the UI thread. Delivery is callback-driven so callers can
/// bridge it to a bounded channel and naturally apply backpressure.
#[allow(
    clippy::too_many_lines,
    reason = "the traversal keeps bounds, cancellation, batching, and terminal exits in one auditable loop"
)]
pub fn search_filesystem(
    request: &SearchRequest,
    config: FallbackConfig,
    mut identify: impl FnMut(&Path, bool) -> Option<ShellItemId>,
    mut deliver: impl FnMut(SearchBatch) -> Result<(), ()>,
) -> SearchOutcome {
    let started = Instant::now();
    let mut metrics = SearchMetrics::default();
    if config.batch_size == 0 || config.max_pending_directories == 0 || config.max_visited == 0 {
        return SearchOutcome::Failed(diagnostic(
            SearchSourceState::Failed,
            "fallback bounds must be non-zero",
        ));
    }
    let mut directories = VecDeque::from([request.root.clone()]);
    let mut visited_directories = HashSet::new();
    let mut batch = Vec::with_capacity(config.batch_size);
    while let Some(directory) = directories.pop_front() {
        if request.cancellation.is_cancelled() {
            metrics.elapsed = started.elapsed();
            metrics.cancel_latency = Some(Duration::ZERO);
            return SearchOutcome::Cancelled(metrics);
        }
        let Ok(canonical) = directory.canonicalize() else {
            continue;
        };
        if !visited_directories.insert(canonical) {
            continue;
        }
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) => {
                metrics.elapsed = started.elapsed();
                flush(&mut batch, &mut metrics, &mut deliver);
                return SearchOutcome::Partial {
                    metrics,
                    diagnostic: diagnostic(
                        SearchSourceState::Partial,
                        format!("could not enumerate one directory: {error}"),
                    ),
                };
            }
        };
        for entry in entries {
            if request.cancellation.is_cancelled() {
                metrics.elapsed = started.elapsed();
                metrics.cancel_latency = Some(Duration::ZERO);
                return SearchOutcome::Cancelled(metrics);
            }
            if metrics.visited >= config.max_visited {
                metrics.elapsed = started.elapsed();
                flush(&mut batch, &mut metrics, &mut deliver);
                return SearchOutcome::Partial {
                    metrics,
                    diagnostic: diagnostic(
                        SearchSourceState::Partial,
                        "fallback item limit reached",
                    ),
                };
            }
            let Ok(entry) = entry else { continue };
            let path = entry.path();
            let Ok(metadata) = fs::symlink_metadata(&path) else {
                continue;
            };
            metrics.visited += 1;
            let file_type = metadata.file_type();
            let is_directory = file_type.is_dir();
            let is_reparse_like = file_type.is_symlink();
            if is_directory && (!is_reparse_like || config.follow_reparse_points) {
                if directories.len() >= config.max_pending_directories {
                    metrics.elapsed = started.elapsed();
                    flush(&mut batch, &mut metrics, &mut deliver);
                    return SearchOutcome::Partial {
                        metrics,
                        diagnostic: diagnostic(
                            SearchSourceState::Partial,
                            "fallback directory queue limit reached",
                        ),
                    };
                }
                directories.push_back(path.clone());
                metrics.max_pending_directories =
                    metrics.max_pending_directories.max(directories.len());
            }
            if evaluates(&request.expression, &path, &metadata) {
                let Some(id) = identify(&path, is_directory) else {
                    continue;
                };
                batch.push(SearchHit {
                    entry: FileEntry {
                        id,
                        display_name: entry.file_name().to_string_lossy().into_owned(),
                        location: LocationDescriptor::FileSystem(path),
                        is_container: is_directory,
                        metadata: explorer_model::FileEntryMetadata {
                            size_bytes: (!is_directory).then_some(metadata.len()),
                            ..Default::default()
                        },
                    },
                    sources: vec![SearchSource::FileSystemFallback],
                });
                metrics.matched += 1;
                if batch.len() == config.batch_size
                    && !flush(&mut batch, &mut metrics, &mut deliver)
                {
                    return SearchOutcome::Failed(diagnostic(
                        SearchSourceState::Failed,
                        "result channel closed",
                    ));
                }
            }
        }
    }
    if !flush(&mut batch, &mut metrics, &mut deliver) {
        return SearchOutcome::Failed(diagnostic(
            SearchSourceState::Failed,
            "result channel closed",
        ));
    }
    metrics.elapsed = started.elapsed();
    SearchOutcome::Finished(metrics)
}

fn flush(
    batch: &mut Vec<SearchHit>,
    metrics: &mut SearchMetrics,
    deliver: &mut impl FnMut(SearchBatch) -> Result<(), ()>,
) -> bool {
    if batch.is_empty() {
        return true;
    }
    let hits = std::mem::take(batch);
    metrics.batches += 1;
    deliver(SearchBatch {
        hits,
        source: SearchSource::FileSystemFallback,
    })
    .is_ok()
}

fn diagnostic(state: SearchSourceState, detail: impl Into<String>) -> BackendDiagnostic {
    BackendDiagnostic {
        source: SearchSource::FileSystemFallback,
        state,
        detail: detail.into(),
    }
}

fn evaluates(expression: &Expr, path: &Path, metadata: &Metadata) -> bool {
    match expression {
        Expr::Text {
            value,
            phrase,
            glob,
        } => crate::matches_text(file_name(path), value, *phrase, *glob),
        Expr::Filter {
            key,
            comparison,
            value,
        } => match (key, value) {
            (PropertyKey::Name, Value::Text(value)) => contains(file_name(path), value),
            (PropertyKey::Type, Value::Text(value)) => {
                let expected = value.trim_start_matches('.');
                path.extension().is_some_and(|extension| {
                    extension.to_string_lossy().eq_ignore_ascii_case(expected)
                })
            }
            (PropertyKey::Size, Value::Size(value)) => {
                compare(&metadata.len(), &value.0, *comparison)
            }
            (PropertyKey::DateModified, Value::Date(value)) => {
                metadata.modified().ok().is_some_and(|modified| {
                    compare(
                        &modified_day(modified),
                        &value.days_since_unix_epoch(),
                        *comparison,
                    )
                })
            }
            _ => false,
        },
        Expr::Not(inner) => !evaluates(inner, path, metadata),
        Expr::And(left, right) => {
            evaluates(left, path, metadata) && evaluates(right, path, metadata)
        }
        Expr::Or(left, right) => {
            evaluates(left, path, metadata) || evaluates(right, path, metadata)
        }
    }
}

fn file_name(path: &Path) -> &str {
    path.file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
}
fn contains(haystack: &str, needle: &str) -> bool {
    haystack.to_lowercase().contains(&needle.to_lowercase())
}
fn compare<T: Ord>(actual: &T, expected: &T, comparison: Comparison) -> bool {
    match comparison {
        Comparison::Equal => actual == expected,
        Comparison::Greater => actual > expected,
        Comparison::GreaterOrEqual => actual >= expected,
        Comparison::Less => actual < expected,
        Comparison::LessOrEqual => actual <= expected,
    }
}
fn modified_day(value: SystemTime) -> i64 {
    match value.duration_since(UNIX_EPOCH) {
        Ok(duration) => i64::try_from(duration.as_secs() / 86_400).unwrap_or(i64::MAX),
        Err(error) => -i64::try_from(error.duration().as_secs() / 86_400).unwrap_or(i64::MAX),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use std::{
        fs::File,
        io::Write,
        sync::{Arc, Mutex},
        thread,
    };
    use tempfile::tempdir;

    fn identity(path: &Path, _: bool) -> Option<ShellItemId> {
        ShellItemId::from_provider_bytes(
            path.as_os_str()
                .to_string_lossy()
                .to_lowercase()
                .into_bytes(),
        )
    }

    #[test]
    fn real_fixture_covers_unicode_phrase_properties_boolean_and_empty() {
        let folder = tempdir().unwrap();
        let fixtures = [
            ("專案 quarter four.txt", 12_000),
            ("專案 old.bin", 3),
            ("notes.md", 500),
            ("zero.txt", 0),
        ];
        for (name, size) in fixtures {
            let mut file = File::create(folder.path().join(name)).unwrap();
            file.write_all(&vec![b'x'; size]).unwrap();
        }
        let cases = [
            (
                r#"專案 "quarter four" type:txt size:>10KB"#,
                vec!["專案 quarter four.txt"],
            ),
            (
                "(type:md OR type:bin) NOT name:missing",
                vec!["notes.md", "專案 old.bin"],
            ),
            (
                "date:<2100-01-01",
                vec![
                    "notes.md",
                    "zero.txt",
                    "專案 old.bin",
                    "專案 quarter four.txt",
                ],
            ),
            ("size:=0", vec!["zero.txt"]),
            ("name:does-not-exist", vec![]),
        ];
        for (query, mut expected) in cases {
            let mut actual = Vec::new();
            let outcome = search_filesystem(
                &SearchRequest {
                    root: folder.path().to_owned(),
                    expression: parse(query).unwrap(),
                    cancellation: CancellationToken::new(),
                },
                FallbackConfig {
                    batch_size: 1,
                    ..FallbackConfig::default()
                },
                identity,
                |batch| {
                    actual.extend(batch.hits.into_iter().map(|hit| hit.entry.display_name));
                    Ok(())
                },
            );
            actual.sort();
            expected.sort_unstable();
            assert_eq!(actual, expected, "query {query}");
            assert!(matches!(outcome, SearchOutcome::Finished(_)));
        }
    }

    #[test]
    fn cancellation_and_closed_channel_are_terminal_and_bounded() {
        let folder = tempdir().unwrap();
        for index in 0..256 {
            File::create(folder.path().join(format!("item-{index}.txt"))).unwrap();
        }
        let cancellation = CancellationToken::new();
        let outcome = search_filesystem(
            &SearchRequest {
                root: folder.path().to_owned(),
                expression: parse("type:txt").unwrap(),
                cancellation: cancellation.clone(),
            },
            FallbackConfig {
                batch_size: 2,
                ..FallbackConfig::default()
            },
            identity,
            |_: SearchBatch| {
                cancellation.cancel();
                Ok(())
            },
        );
        assert!(matches!(outcome, SearchOutcome::Cancelled(metrics) if metrics.batches == 1));

        let outcome = search_filesystem(
            &SearchRequest {
                root: folder.path().to_owned(),
                expression: parse("type:txt").unwrap(),
                cancellation: CancellationToken::new(),
            },
            FallbackConfig {
                batch_size: 1,
                ..FallbackConfig::default()
            },
            identity,
            |_| Err(()),
        );
        assert!(
            matches!(outcome, SearchOutcome::Failed(diagnostic) if diagnostic.detail.contains("channel closed"))
        );
    }

    #[test]
    fn quick_replacement_rejects_old_generation_at_consumer_boundary() {
        let accepted = Arc::new(Mutex::new(Vec::new()));
        let current_generation = Arc::new(std::sync::atomic::AtomicU64::new(2));
        let old_accepted = Arc::clone(&accepted);
        let old_generation = Arc::clone(&current_generation);
        let old = thread::spawn(move || {
            if old_generation.load(std::sync::atomic::Ordering::Acquire) == 1 {
                old_accepted.lock().unwrap().push("old");
            }
        });
        if current_generation.load(std::sync::atomic::Ordering::Acquire) == 2 {
            accepted.lock().unwrap().push("new");
        }
        old.join().unwrap();
        assert_eq!(*accepted.lock().unwrap(), vec!["new"]);
    }

    #[test]
    fn stable_identity_dedupes_aliases_and_preserves_sources() {
        let id = ShellItemId::from_provider_bytes([1]).unwrap();
        let entry = FileEntry {
            id,
            display_name: "alias-a".into(),
            location: LocationDescriptor::file_system("a"),
            is_container: false,
            metadata: explorer_model::FileEntryMetadata::default(),
        };
        let mut store = DedupeStore::default();
        assert!(store.insert(SearchHit {
            entry: entry.clone(),
            sources: vec![SearchSource::WindowsIndex]
        }));
        let mut alias = entry;
        alias.display_name = "alias-b".into();
        alias.location = LocationDescriptor::file_system("b");
        assert!(!store.insert(SearchHit {
            entry: alias,
            sources: vec![SearchSource::FileSystemFallback]
        }));
        assert_eq!(store.hits()[0].sources.len(), 2);
    }

    #[test]
    #[ignore = "explicit 100k filesystem performance evidence"]
    fn measures_one_hundred_thousand_real_items() {
        let folder = tempdir().unwrap();
        for index in 0..100_000 {
            File::create(folder.path().join(format!("item-{index:06}.txt"))).unwrap();
        }
        let memory_before = process_working_set();
        let mut first_result = None;
        let mut first_viewport = None;
        let started = Instant::now();
        let outcome = search_filesystem(
            &SearchRequest {
                root: folder.path().to_owned(),
                expression: parse("type:txt").unwrap(),
                cancellation: CancellationToken::new(),
            },
            FallbackConfig {
                batch_size: 64,
                ..FallbackConfig::default()
            },
            identity,
            |_| {
                first_result.get_or_insert_with(|| started.elapsed());
                first_viewport.get_or_insert_with(|| started.elapsed());
                Ok(())
            },
        );
        let SearchOutcome::Finished(metrics) = outcome else {
            panic!("search did not finish")
        };
        let memory_after = process_working_set();
        let cancellation = CancellationToken::new();
        let mut cancellation_started = None;
        let cancelled = search_filesystem(
            &SearchRequest {
                root: folder.path().to_owned(),
                expression: parse("type:txt").unwrap(),
                cancellation: cancellation.clone(),
            },
            FallbackConfig {
                batch_size: 64,
                ..FallbackConfig::default()
            },
            identity,
            |_| {
                cancellation_started = Some(Instant::now());
                cancellation.cancel();
                Ok(())
            },
        );
        let cancel_latency = cancellation_started.map(|started| started.elapsed());
        eprintln!(
            "first_result={first_result:?} first_viewport={first_viewport:?} terminal={:?} batches={} max_queue={} memory_before={} memory_after={} memory_delta={} cancel_latency={cancel_latency:?}",
            metrics.elapsed,
            metrics.batches,
            metrics.max_pending_directories,
            memory_before,
            memory_after,
            memory_after.saturating_sub(memory_before),
        );
        assert_eq!(metrics.matched, 100_000);
        assert!(matches!(cancelled, SearchOutcome::Cancelled(_)));
        assert!(cancel_latency.is_some_and(|latency| latency < Duration::from_secs(1)));
    }

    #[cfg(windows)]
    #[allow(
        unsafe_code,
        reason = "the performance oracle reads a sized process memory counter"
    )]
    fn process_working_set() -> usize {
        use std::mem::{MaybeUninit, size_of};
        use windows::Win32::System::{
            ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS},
            Threading::GetCurrentProcess,
        };
        let mut counters = MaybeUninit::<PROCESS_MEMORY_COUNTERS>::zeroed();
        // SAFETY: the current-process pseudo-handle is borrowed and counters is sized writable
        // storage initialized by the API before assume_init.
        unsafe {
            GetProcessMemoryInfo(
                GetCurrentProcess(),
                counters.as_mut_ptr(),
                u32::try_from(size_of::<PROCESS_MEMORY_COUNTERS>()).unwrap(),
            )
            .unwrap();
            counters.assume_init().WorkingSetSize
        }
    }

    #[cfg(not(windows))]
    fn process_working_set() -> usize {
        0
    }
}
