use std::{
    collections::{HashSet, VecDeque},
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use explorer_model::{
    CancellationToken, FileEntry, FileEntryMetadata, LocationDescriptor, ShellItemId,
};
use rusqlite::{Connection, OptionalExtension as _, params};

use crate::{
    BackendDiagnostic, Comparison, Expr, PropertyKey, SearchBatch, SearchHit, SearchMetrics,
    SearchOutcome, SearchSource, SearchSourceState, Value,
};

const SCHEMA_VERSION: i64 = 1;

#[derive(Clone, Copy, Debug)]
pub struct LazyIndexConfig {
    pub batch_size: usize,
    pub max_pending_directories: usize,
    pub max_visited: usize,
    pub max_results: usize,
    pub max_index_rows: usize,
    pub max_path_bytes: usize,
}

impl Default for LazyIndexConfig {
    fn default() -> Self {
        Self {
            batch_size: 64,
            max_pending_directories: 4096,
            max_visited: 1_000_000,
            max_results: 100_000,
            max_index_rows: 1_000_000,
            max_path_bytes: 32_768,
        }
    }
}

pub fn default_index_path() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .map(|root| {
            root.join("RustGpuiExplorer")
                .join("search-index")
                .join("v1")
                .join("index.sqlite3")
        })
}

pub struct LazyIndex {
    connection: Connection,
}

#[allow(
    clippy::missing_errors_doc,
    reason = "SQLite errors are preserved verbatim by this internal cache boundary"
)]
impl LazyIndex {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|_| rusqlite::Error::InvalidPath(parent.to_owned()))?;
        }
        let connection = Connection::open(path)?;
        connection.busy_timeout(Duration::from_millis(500))?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.execute_batch("CREATE TABLE IF NOT EXISTS meta(version INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS entries(path TEXT PRIMARY KEY, parent TEXT NOT NULL, name TEXT NOT NULL, is_dir INTEGER NOT NULL, size INTEGER, modified INTEGER, item_type TEXT); CREATE INDEX IF NOT EXISTS entries_parent ON entries(parent); CREATE INDEX IF NOT EXISTS entries_name ON entries(name);")?;
        let version: Option<i64> = connection
            .query_row("SELECT version FROM meta LIMIT 1", [], |row| row.get(0))
            .optional()?;
        if version.is_none() {
            connection.execute("INSERT INTO meta(version) VALUES(?1)", [SCHEMA_VERSION])?;
        } else if version != Some(SCHEMA_VERSION) {
            return Err(rusqlite::Error::InvalidQuery);
        }
        let integrity: String = connection.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
        if integrity != "ok" {
            return Err(rusqlite::Error::InvalidQuery);
        }
        Ok(Self { connection })
    }

    pub fn open_default() -> rusqlite::Result<Self> {
        let path = default_index_path()
            .ok_or_else(|| rusqlite::Error::InvalidPath(PathBuf::from("LOCALAPPDATA")))?;
        Self::open_resilient(&path)
    }

    pub fn open_resilient(path: &Path) -> rusqlite::Result<Self> {
        match Self::open(path) {
            Ok(index) => Ok(index),
            Err(original) if path.exists() => {
                let suffix = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |value| value.as_secs());
                let quarantine = path.with_extension(format!("corrupt-{suffix}.sqlite3"));
                fs::rename(path, quarantine).map_err(|_| original)?;
                Self::open(path)
            }
            Err(error) => Err(error),
        }
    }

    pub fn row_count(&self) -> rusqlite::Result<u64> {
        self.connection
            .query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))
    }

    fn cached_entries(&self, root: &Path, maximum: usize) -> rusqlite::Result<Vec<FileEntry>> {
        let root = canonical_text(root);
        let prefix = format!("{root}\\");
        let mut statement = self.connection.prepare("SELECT path,name,is_dir,size,modified,item_type FROM entries WHERE path=?1 OR substr(path,1,length(?2))=?2 LIMIT ?3")?;
        let rows = statement.query_map(
            params![root, prefix, i64::try_from(maximum).unwrap_or(i64::MAX)],
            |row| {
                let path: String = row.get(0)?;
                let path = PathBuf::from(path);
                Ok(FileEntry {
                    id: identity(&path).ok_or(rusqlite::Error::InvalidQuery)?,
                    display_name: row.get(1)?,
                    location: LocationDescriptor::FileSystem(path),
                    is_container: row.get(2)?,
                    metadata: FileEntryMetadata {
                        size_bytes: row.get(3)?,
                        modified_sort_key: row.get(4)?,
                        type_display: row.get(5)?,
                        ..Default::default()
                    },
                })
            },
        )?;
        rows.collect()
    }

    pub fn observe_directory(
        &mut self,
        parent: &Path,
        entries: &[FileEntry],
    ) -> rusqlite::Result<()> {
        self.observe_directory_bounded(
            parent,
            entries,
            LazyIndexConfig::default().max_index_rows,
            LazyIndexConfig::default().max_path_bytes,
        )
        .map(|_| ())
    }

    fn observe_directory_bounded(
        &mut self,
        parent: &Path,
        entries: &[FileEntry],
        max_rows: usize,
        max_path_bytes: usize,
    ) -> rusqlite::Result<bool> {
        let parent = canonical_text(parent);
        let transaction = self.connection.transaction()?;
        transaction.execute("DELETE FROM entries WHERE parent=?1", [&parent])?;
        let existing: u64 =
            transaction.query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))?;
        let mut remaining = max_rows.saturating_sub(existing.try_into().unwrap_or(usize::MAX));
        let mut bounded = false;
        {
            let mut statement = transaction.prepare("INSERT OR REPLACE INTO entries(path,parent,name,is_dir,size,modified,item_type) VALUES(?1,?2,?3,?4,?5,?6,?7)")?;
            for entry in entries {
                let Some(path) = entry.location.path() else {
                    continue;
                };
                let path = canonical_text(path);
                if path.len() > max_path_bytes || remaining == 0 {
                    bounded = true;
                    continue;
                }
                statement.execute(params![
                    path,
                    parent,
                    entry.display_name,
                    entry.is_container,
                    entry.metadata.size_bytes,
                    entry.metadata.modified_sort_key,
                    entry.metadata.type_display
                ])?;
                remaining -= 1;
            }
        }
        transaction.commit()?;
        Ok(bounded)
    }

    pub fn search(
        &mut self,
        root: &Path,
        expression: &Expr,
        cancellation: &CancellationToken,
        config: LazyIndexConfig,
        mut deliver: impl FnMut(SearchBatch) -> Result<(), ()>,
    ) -> SearchOutcome {
        let started = Instant::now();
        let mut metrics = SearchMetrics::default();
        let mut directories = VecDeque::from([root.to_owned()]);
        let mut batch = Vec::with_capacity(config.batch_size);
        let mut emitted = HashSet::new();
        if let Ok(cached) = self.cached_entries(root, config.max_results) {
            for entry in cached {
                if cancellation.is_cancelled() {
                    return cancelled(started, metrics);
                }
                if matches_entry(expression, &entry)
                    && emitted.insert(canonical_text(entry.location.path().unwrap_or(root)))
                {
                    batch.push(SearchHit {
                        entry,
                        sources: vec![SearchSource::FileSystemFallback],
                    });
                    metrics.matched += 1;
                    if batch.len() >= config.batch_size
                        && !flush(&mut batch, &mut metrics, &mut deliver)
                    {
                        return failed("local index result channel closed");
                    }
                }
            }
            if !flush(&mut batch, &mut metrics, &mut deliver) {
                return failed("local index result channel closed");
            }
        }
        while let Some(directory) = directories.pop_front() {
            if cancellation.is_cancelled() {
                return cancelled(started, metrics);
            }
            let read = match fs::read_dir(&directory) {
                Ok(value) => value,
                Err(error) => return partial(started, metrics, error.to_string()),
            };
            let mut observed = Vec::new();
            for item in read {
                if cancellation.is_cancelled() {
                    return cancelled(started, metrics);
                }
                if metrics.visited >= config.max_visited || metrics.matched >= config.max_results {
                    return partial(started, metrics, "local index bound reached");
                }
                let Ok(item) = item else { continue };
                let path = item.path();
                let Ok(metadata) = fs::symlink_metadata(&path) else {
                    continue;
                };
                let is_directory = metadata.file_type().is_dir();
                let Some(id) = identity(&path) else { continue };
                let entry = FileEntry {
                    id,
                    display_name: item.file_name().to_string_lossy().into_owned(),
                    location: LocationDescriptor::FileSystem(path.clone()),
                    is_container: is_directory,
                    metadata: FileEntryMetadata {
                        size_bytes: (!is_directory).then_some(metadata.len()),
                        ..Default::default()
                    },
                };
                observed.push(entry.clone());
                metrics.visited += 1;
                if is_directory && !metadata.file_type().is_symlink() {
                    if directories.len() >= config.max_pending_directories {
                        return partial(started, metrics, "local index directory bound reached");
                    }
                    directories.push_back(path.clone());
                    metrics.max_pending_directories =
                        metrics.max_pending_directories.max(directories.len());
                }
                if matches_entry(expression, &entry) && emitted.insert(canonical_text(&path)) {
                    batch.push(SearchHit {
                        entry,
                        sources: vec![SearchSource::FileSystemFallback],
                    });
                    metrics.matched += 1;
                }
                if batch.len() >= config.batch_size
                    && !flush(&mut batch, &mut metrics, &mut deliver)
                {
                    return failed("local index result channel closed");
                }
            }
            if cancellation.is_cancelled() {
                return cancelled(started, metrics);
            }
            match self.observe_directory_bounded(
                &directory,
                &observed,
                config.max_index_rows,
                config.max_path_bytes,
            ) {
                Ok(true) => {
                    if !flush(&mut batch, &mut metrics, &mut deliver) {
                        return failed("local index result channel closed");
                    }
                    return partial(started, metrics, "local index storage bound reached");
                }
                Ok(false) => {}
                Err(_) => {
                    if !flush(&mut batch, &mut metrics, &mut deliver) {
                        return failed("local index result channel closed");
                    }
                    return partial(started, metrics, "local index write failed");
                }
            }
        }
        if !flush(&mut batch, &mut metrics, &mut deliver) {
            return failed("local index result channel closed");
        }
        metrics.elapsed = started.elapsed();
        SearchOutcome::Finished(metrics)
    }
}

fn identity(path: &Path) -> Option<ShellItemId> {
    ShellItemId::from_provider_bytes(path.to_string_lossy().to_lowercase().into_bytes())
}
fn canonical_text(path: &Path) -> String {
    let text = path
        .canonicalize()
        .unwrap_or_else(|_| path.to_owned())
        .to_string_lossy()
        .trim_end_matches(['\\', '/'])
        .to_lowercase();
    if let Some(unc) = text.strip_prefix(r"\\?\unc\") {
        format!(r"\\{unc}")
    } else {
        text.strip_prefix(r"\\?\").unwrap_or(&text).to_owned()
    }
}
/// Applies the app's bounded query semantics to one already-materialized entry.
///
/// Native index providers are treated as candidate generators; their results are passed through
/// this predicate before they reach the UI so provider syntax or version differences cannot
/// broaden the visible result set.
pub fn matches_entry(expression: &Expr, entry: &FileEntry) -> bool {
    match expression {
        Expr::Text {
            value,
            phrase,
            glob,
        } => matches_text(&entry.display_name, value, *phrase, *glob),
        Expr::Filter {
            key,
            comparison,
            value,
        } => match (key, value) {
            (PropertyKey::Name, Value::Text(v)) => contains(&entry.display_name, v),
            (PropertyKey::Type, Value::Text(v)) => entry
                .location
                .path()
                .and_then(Path::extension)
                .is_some_and(|x| {
                    x.to_string_lossy()
                        .eq_ignore_ascii_case(v.trim_start_matches('.'))
                }),
            (PropertyKey::Size, Value::Size(v)) => {
                compare(&entry.metadata.size_bytes.unwrap_or(0), &v.0, *comparison)
            }
            _ => false,
        },
        Expr::Not(v) => !matches_entry(v, entry),
        Expr::And(a, b) => matches_entry(a, entry) && matches_entry(b, entry),
        Expr::Or(a, b) => matches_entry(a, entry) || matches_entry(b, entry),
    }
}
fn contains(a: &str, b: &str) -> bool {
    a.to_lowercase().contains(&b.to_lowercase())
}

/// Matches unqualified search text against one complete filename.
///
/// Plain text retains substring behavior. Glob text is matched against the complete filename;
/// backslash escapes `*`, `?`, and backslash itself.
pub fn matches_text(filename: &str, pattern: &str, phrase: bool, glob: bool) -> bool {
    if phrase {
        return contains(filename, pattern);
    }
    let tokens = glob_tokens(pattern);
    if !glob {
        let literal: String = tokens
            .into_iter()
            .map(|token| match token {
                GlobToken::Literal(character) => character,
                GlobToken::AnyMany => '*',
                GlobToken::AnyOne => '?',
            })
            .collect();
        return contains(filename, &literal);
    }
    glob_matches(filename, &tokens)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GlobToken {
    Literal(char),
    AnyMany,
    AnyOne,
}

fn glob_tokens(pattern: &str) -> Vec<GlobToken> {
    let mut tokens = Vec::with_capacity(pattern.chars().count());
    let mut characters = pattern.chars();
    while let Some(character) = characters.next() {
        if character == '\\' {
            match characters.next() {
                Some(escaped @ ('*' | '?' | '\\')) => tokens.push(GlobToken::Literal(escaped)),
                Some(other) => {
                    tokens.push(GlobToken::Literal('\\'));
                    tokens.push(GlobToken::Literal(other));
                }
                None => tokens.push(GlobToken::Literal('\\')),
            }
        } else {
            tokens.push(match character {
                '*' => GlobToken::AnyMany,
                '?' => GlobToken::AnyOne,
                literal => GlobToken::Literal(literal),
            });
        }
    }
    tokens
}

fn glob_matches(filename: &str, tokens: &[GlobToken]) -> bool {
    let filename: Vec<char> = filename.chars().flat_map(char::to_lowercase).collect();
    let tokens: Vec<GlobToken> = tokens
        .iter()
        .flat_map(|token| match token {
            GlobToken::Literal(character) => character
                .to_lowercase()
                .map(GlobToken::Literal)
                .collect::<Vec<_>>(),
            other => vec![*other],
        })
        .collect();
    let (mut name_index, mut pattern_index) = (0, 0);
    let (mut star_index, mut star_match) = (None, 0);
    while name_index < filename.len() {
        match tokens.get(pattern_index) {
            Some(GlobToken::Literal(expected)) if *expected == filename[name_index] => {
                name_index += 1;
                pattern_index += 1;
            }
            Some(GlobToken::AnyOne) => {
                name_index += 1;
                pattern_index += 1;
            }
            Some(GlobToken::AnyMany) => {
                star_index = Some(pattern_index);
                pattern_index += 1;
                star_match = name_index;
            }
            _ => {
                let Some(star) = star_index else { return false };
                star_match += 1;
                name_index = star_match;
                pattern_index = star + 1;
            }
        }
    }
    tokens[pattern_index..]
        .iter()
        .all(|token| *token == GlobToken::AnyMany)
}
fn compare<T: Ord>(a: &T, b: &T, c: Comparison) -> bool {
    match c {
        Comparison::Equal => a == b,
        Comparison::Greater => a > b,
        Comparison::GreaterOrEqual => a >= b,
        Comparison::Less => a < b,
        Comparison::LessOrEqual => a <= b,
    }
}
fn flush(
    batch: &mut Vec<SearchHit>,
    metrics: &mut SearchMetrics,
    deliver: &mut impl FnMut(SearchBatch) -> Result<(), ()>,
) -> bool {
    if batch.is_empty() {
        return true;
    }
    metrics.batches += 1;
    deliver(SearchBatch {
        hits: std::mem::take(batch),
        source: SearchSource::FileSystemFallback,
    })
    .is_ok()
}
fn cancelled(started: Instant, mut metrics: SearchMetrics) -> SearchOutcome {
    metrics.elapsed = started.elapsed();
    metrics.cancel_latency = Some(Duration::ZERO);
    SearchOutcome::Cancelled(metrics)
}
fn partial(
    started: Instant,
    mut metrics: SearchMetrics,
    detail: impl Into<String>,
) -> SearchOutcome {
    metrics.elapsed = started.elapsed();
    SearchOutcome::Partial {
        metrics,
        diagnostic: BackendDiagnostic {
            source: SearchSource::FileSystemFallback,
            state: SearchSourceState::Partial,
            detail: detail.into(),
        },
    }
}
fn failed(detail: impl Into<String>) -> SearchOutcome {
    SearchOutcome::Failed(BackendDiagnostic {
        source: SearchSource::FileSystemFallback,
        state: SearchSourceState::Failed,
        detail: detail.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filename_globs_and_plain_text_share_one_matcher() {
        assert!(matches_text("src.rs", "*.rs", false, true));
        assert!(matches_text("SRC.RS", "*.rs", false, true));
        assert!(matches_text("foo-test.rs", "foo*.rs", false, true));
        assert!(matches_text("unit-test-data", "*test*", false, true));
        assert!(matches_text("file1.rs", "file?.rs", false, true));
        assert!(!matches_text("file10.rs", "file?.rs", false, true));
        assert!(matches_text("測試.rs", "測?.rs", false, true));
        assert!(matches_text("quarter-report.txt", "report", false, false));
        assert!(matches_text("literal*star", r"literal\*star", false, false));
        assert!(matches_text(
            "literal*star1.rs",
            r"literal\*star?.rs",
            false,
            true
        ));
        assert!(!matches_text("other.rs", r"*.rs", true, false));
    }
    use crate::parse;
    use tempfile::tempdir;

    #[test]
    fn indexes_only_active_scope_and_stops_on_cancel() {
        let folder = tempdir().unwrap();
        fs::create_dir(folder.path().join("child")).unwrap();
        fs::write(folder.path().join("one.txt"), b"1").unwrap();
        fs::write(folder.path().join("child").join("two.txt"), b"2").unwrap();
        let mut index = LazyIndex::open(&folder.path().join("index.sqlite3")).unwrap();
        let cancel = CancellationToken::new();
        let mut names = Vec::new();
        let outcome = index.search(
            folder.path(),
            &parse("type:txt").unwrap(),
            &cancel,
            LazyIndexConfig::default(),
            |batch| {
                names.extend(batch.hits.into_iter().map(|x| x.entry.display_name));
                Ok(())
            },
        );
        assert!(matches!(outcome, SearchOutcome::Finished(_)));
        assert_eq!(names.len(), 2);
        assert!(index.row_count().unwrap() >= 3);
        cancel.cancel();
        let before = index.row_count().unwrap();
        let outcome = index.search(
            folder.path(),
            &parse("txt").unwrap(),
            &cancel,
            LazyIndexConfig::default(),
            |_| Ok(()),
        );
        assert!(matches!(outcome, SearchOutcome::Cancelled(_)));
        assert_eq!(index.row_count().unwrap(), before);
    }

    #[test]
    fn corrupt_database_is_quarantined_and_rebuilt() {
        let folder = tempdir().unwrap();
        let path = folder.path().join("index.sqlite3");
        fs::write(&path, b"not sqlite").unwrap();
        let index = LazyIndex::open_resilient(&path).unwrap();
        assert_eq!(index.row_count().unwrap(), 0);
        assert!(
            fs::read_dir(folder.path())
                .unwrap()
                .filter_map(Result::ok)
                .any(|entry| entry.file_name().to_string_lossy().contains("corrupt-"))
        );
    }

    #[test]
    fn cancellation_during_delivery_stops_database_growth() {
        let folder = tempdir().unwrap();
        for directory in 0..8 {
            let path = folder.path().join(format!("dir-{directory}"));
            fs::create_dir(&path).unwrap();
            for file in 0..8 {
                fs::write(path.join(format!("match-{file}.txt")), b"fixture").unwrap();
            }
        }
        let mut index = LazyIndex::open(&folder.path().join("index.sqlite3")).unwrap();
        let cancel = CancellationToken::new();
        let outcome = index.search(
            folder.path(),
            &parse("match").unwrap(),
            &cancel,
            LazyIndexConfig {
                batch_size: 1,
                ..LazyIndexConfig::default()
            },
            |_| {
                cancel.cancel();
                Ok(())
            },
        );
        assert!(matches!(outcome, SearchOutcome::Cancelled(_)));
        let stopped_at = index.row_count().unwrap();
        std::thread::sleep(Duration::from_millis(25));
        assert_eq!(index.row_count().unwrap(), stopped_at);
        assert!(stopped_at < 72);
    }

    #[test]
    fn cached_scope_does_not_include_similar_sibling_prefix() {
        let folder = tempdir().unwrap();
        let foo = folder.path().join("foo");
        let foobar = folder.path().join("foobar");
        fs::create_dir_all(&foo).unwrap();
        fs::create_dir_all(&foobar).unwrap();
        let mut index = LazyIndex::open(&folder.path().join("index.sqlite3")).unwrap();
        let entry = |path: PathBuf| FileEntry {
            id: identity(&path).unwrap(),
            display_name: "hit.txt".into(),
            location: LocationDescriptor::FileSystem(path),
            is_container: false,
            metadata: FileEntryMetadata::default(),
        };
        index
            .observe_directory(&foo, &[entry(foo.join("hit.txt"))])
            .unwrap();
        index
            .observe_directory(&foobar, &[entry(foobar.join("hit.txt"))])
            .unwrap();
        let cached = index.cached_entries(&foo, 10).unwrap();
        assert_eq!(cached.len(), 1);
        assert!(
            canonical_text(cached[0].location.path().unwrap()).starts_with(&canonical_text(&foo))
        );
    }

    #[test]
    fn shallow_refresh_deletes_missing_children_and_storage_bounds_are_partial() {
        let folder = tempdir().unwrap();
        let parent = folder.path().join("viewed");
        fs::create_dir(&parent).unwrap();
        let make_entry = |name: &str| {
            let path = parent.join(name);
            fs::write(&path, name.as_bytes()).unwrap();
            FileEntry {
                id: identity(&path).unwrap(),
                display_name: name.to_owned(),
                location: LocationDescriptor::FileSystem(path),
                is_container: false,
                metadata: FileEntryMetadata::default(),
            }
        };
        let first = make_entry("first.txt");
        let second = make_entry("second.txt");
        let mut index = LazyIndex::open(&folder.path().join("index.sqlite3")).unwrap();
        index
            .observe_directory(&parent, &[first.clone(), second])
            .unwrap();
        assert_eq!(index.cached_entries(&parent, 10).unwrap().len(), 2);
        index
            .observe_directory(&parent, std::slice::from_ref(&first))
            .unwrap();
        let cached = index.cached_entries(&parent, 10).unwrap();
        assert_eq!(cached.len(), 1);
        assert_eq!(cached[0].display_name, "first.txt");

        let bounded = index
            .observe_directory_bounded(&parent, &[first], 0, 8)
            .unwrap();
        assert!(bounded);
        assert!(index.cached_entries(&parent, 10).unwrap().is_empty());

        let mut delivered = Vec::new();
        let outcome = index.search(
            &parent,
            &parse("first").unwrap(),
            &CancellationToken::new(),
            LazyIndexConfig {
                max_index_rows: 0,
                ..LazyIndexConfig::default()
            },
            |batch| {
                delivered.extend(batch.hits.into_iter().map(|hit| hit.entry.display_name));
                Ok(())
            },
        );
        assert!(matches!(outcome, SearchOutcome::Partial { .. }));
        assert_eq!(delivered, ["first.txt"]);
    }

    #[cfg(windows)]
    #[test]
    fn active_search_does_not_follow_directory_symlinks() {
        let folder = tempdir().unwrap();
        let outside = tempdir().unwrap();
        fs::write(outside.path().join("needle.txt"), b"needle").unwrap();
        let link = folder.path().join("linked");
        if std::os::windows::fs::symlink_dir(outside.path(), &link).is_err() {
            return;
        }
        let mut index = LazyIndex::open(&folder.path().join("index.sqlite3")).unwrap();
        let mut names = Vec::new();
        let outcome = index.search(
            folder.path(),
            &parse("needle").unwrap(),
            &CancellationToken::new(),
            LazyIndexConfig::default(),
            |batch| {
                names.extend(batch.hits.into_iter().map(|hit| hit.entry.display_name));
                Ok(())
            },
        );
        assert!(matches!(outcome, SearchOutcome::Finished(_)));
        assert!(names.is_empty());
    }

    #[test]
    fn ten_cycle_two_root_cancellation_oracle() {
        let root_a = std::env::var_os("EXPLORER_SEARCH_CANCEL_ROOT_A")
            .map_or_else(std::env::temp_dir, PathBuf::from);
        let root_b = std::env::var_os("EXPLORER_SEARCH_CANCEL_ROOT_B")
            .map_or_else(std::env::temp_dir, PathBuf::from);
        if !root_a.is_dir() || !root_b.is_dir() {
            return;
        }
        for cycle in 0..10 {
            let fixture_a = tempfile::Builder::new()
                .prefix("explorer-cancel-a-")
                .tempdir_in(&root_a)
                .unwrap();
            let fixture_b = tempfile::Builder::new()
                .prefix("explorer-cancel-b-")
                .tempdir_in(&root_b)
                .unwrap();
            for fixture in [&fixture_a, &fixture_b] {
                for directory in 0..4 {
                    let path = fixture.path().join(format!("dir-{directory}"));
                    fs::create_dir(&path).unwrap();
                    for file in 0..4 {
                        fs::write(path.join(format!("needle-{file}.txt")), b"fixture").unwrap();
                    }
                }
                let mut index = LazyIndex::open(&fixture.path().join("index.sqlite3")).unwrap();
                let cancel = CancellationToken::new();
                let from_delivery = cancel.clone();
                let outcome = index.search(
                    fixture.path(),
                    &parse("needle").unwrap(),
                    &cancel,
                    LazyIndexConfig {
                        batch_size: 1,
                        ..LazyIndexConfig::default()
                    },
                    |_| {
                        from_delivery.cancel();
                        Ok(())
                    },
                );
                assert!(matches!(outcome, SearchOutcome::Cancelled(_)));
                let rows = index.row_count().unwrap();
                std::thread::sleep(Duration::from_millis(5));
                assert_eq!(index.row_count().unwrap(), rows);
            }
            eprintln!("cancellation-cycle={} roots=2 stable=true", cycle + 1);
        }
    }
}
