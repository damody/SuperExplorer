//! Versioned, independent Windows bookmark storage.

use std::{
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{self, Read as _, Write as _},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use explorer_common::RoadmapLimits;
use explorer_model::{Bookmarks, SessionStoreError};
use serde::{Deserialize, Serialize};

const BOOKMARK_DIRECTORY: &str = "RustGpuiExplorer\\bookmarks\\v1";
const CURRENT_FILE: &str = "bookmarks.json";
const BACKUP_FILE: &str = "bookmarks.last-known-good.json";
const TEMP_FILE: &str = "bookmarks.pending.json";
const SCHEMA_VERSION: u16 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BookmarkLoadSource {
    Current,
    LastKnownGood,
    Missing,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BookmarkLoadOutcome {
    pub bookmarks: Option<Bookmarks>,
    pub source: BookmarkLoadSource,
    pub rejected_artifacts: usize,
    pub previously_initialized: bool,
    pub warning: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BookmarkResolution {
    pub bookmarks: Bookmarks,
    pub migration_performed: bool,
    pub warning: Option<String>,
}

pub trait BookmarkStore: Send + Sync {
    fn save(&self, bookmarks: &Bookmarks) -> Result<(), SessionStoreError>;
}

#[derive(Clone, Debug)]
pub struct WindowsBookmarkStore {
    root: PathBuf,
    limits: RoadmapLimits,
    #[cfg(test)]
    write_failure: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BookmarkEnvelope {
    schema_version: u16,
    bookmarks: Bookmarks,
}

impl WindowsBookmarkStore {
    /// Resolves the stable per-user bookmark root without creating it.
    pub fn from_environment(limits: RoadmapLimits) -> Result<Self, SessionStoreError> {
        let local = std::env::var_os("LOCALAPPDATA").ok_or_else(|| {
            SessionStoreError::Unavailable("LOCALAPPDATA is not available".to_owned())
        })?;
        Ok(Self::at_root(
            PathBuf::from(local).join(BOOKMARK_DIRECTORY),
            limits,
        ))
    }

    pub fn at_root(root: PathBuf, limits: RoadmapLimits) -> Self {
        Self {
            root,
            limits,
            #[cfg(test)]
            write_failure: false,
        }
    }

    fn current_path(&self) -> PathBuf {
        self.root.join(CURRENT_FILE)
    }

    fn backup_path(&self) -> PathBuf {
        self.root.join(BACKUP_FILE)
    }

    fn temporary_path(&self) -> PathBuf {
        self.root.join(TEMP_FILE)
    }

    fn previously_initialized(&self) -> bool {
        if self.current_path().exists()
            || self.backup_path().exists()
            || self.temporary_path().exists()
        {
            return true;
        }
        fs::read_dir(&self.root).is_ok_and(|entries| {
            entries.filter_map(Result::ok).any(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("bookmarks.")
            })
        })
    }

    fn read_document(&self, path: &Path) -> Result<Option<Bookmarks>, ReadFailure> {
        let mut file = match File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(ReadFailure::Io(error)),
        };
        let maximum = self.limits.max_state_payload_bytes;
        let mut bytes = Vec::with_capacity(maximum.min(64 * 1024));
        io::Read::by_ref(&mut file)
            .take(u64::try_from(maximum).unwrap_or(u64::MAX).saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(ReadFailure::Io)?;
        if bytes.len() > maximum {
            return Err(ReadFailure::Invalid);
        }
        let envelope: BookmarkEnvelope =
            serde_json::from_slice(&bytes).map_err(|_| ReadFailure::Invalid)?;
        if envelope.schema_version != SCHEMA_VERSION {
            return Err(ReadFailure::Invalid);
        }
        Ok(Some(envelope.bookmarks))
    }

    fn quarantine(&self, path: &Path) -> io::Result<()> {
        if !path.exists() {
            return Ok(());
        }
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let mut name = OsString::from(
            path.file_name()
                .unwrap_or_else(|| std::ffi::OsStr::new("bookmarks")),
        );
        name.push(format!(".corrupt.{timestamp}"));
        fs::rename(path, self.root.join(name))
    }

    pub fn load(&self) -> Result<BookmarkLoadOutcome, SessionStoreError> {
        let previously_initialized = self.previously_initialized();
        let mut rejected_artifacts = 0;
        for (path, source) in [
            (self.current_path(), BookmarkLoadSource::Current),
            (self.backup_path(), BookmarkLoadSource::LastKnownGood),
        ] {
            match self.read_document(&path) {
                Ok(Some(bookmarks)) => {
                    let warning = (source == BookmarkLoadSource::LastKnownGood)
                        .then(|| self.save(&bookmarks).err())
                        .flatten()
                        .map(|error| format!("bookmark current repair failed: {error}"));
                    return Ok(BookmarkLoadOutcome {
                        bookmarks: Some(bookmarks),
                        source,
                        rejected_artifacts,
                        previously_initialized,
                        warning,
                    });
                }
                Ok(None) => {}
                Err(ReadFailure::Invalid) => {
                    rejected_artifacts += 1;
                    let _ = self.quarantine(&path);
                }
                Err(ReadFailure::Io(error)) => return Err(map_io(error)),
            }
        }
        Ok(BookmarkLoadOutcome {
            bookmarks: None,
            source: BookmarkLoadSource::Missing,
            rejected_artifacts,
            previously_initialized,
            warning: None,
        })
    }

    /// Prefers independent data and migrates legacy session bookmarks only for a new store.
    pub fn load_or_migrate(&self, legacy: &Bookmarks) -> BookmarkResolution {
        match self.load() {
            Ok(outcome) => {
                if let Some(bookmarks) = outcome.bookmarks {
                    return BookmarkResolution {
                        bookmarks,
                        migration_performed: false,
                        warning: outcome.warning,
                    };
                }
                if outcome.previously_initialized {
                    return BookmarkResolution {
                        bookmarks: legacy.clone(),
                        migration_performed: false,
                        warning: (outcome.rejected_artifacts > 0)
                            .then(|| "bookmark artifacts failed validation".to_owned()),
                    };
                }
                match self.save(legacy) {
                    Ok(()) => BookmarkResolution {
                        bookmarks: legacy.clone(),
                        migration_performed: true,
                        warning: None,
                    },
                    Err(error) => BookmarkResolution {
                        bookmarks: legacy.clone(),
                        migration_performed: false,
                        warning: Some(format!("bookmark migration failed: {error}")),
                    },
                }
            }
            Err(error) => BookmarkResolution {
                bookmarks: legacy.clone(),
                migration_performed: false,
                warning: Some(format!("bookmark load failed: {error}")),
            },
        }
    }

    fn write_document(&self, bytes: &[u8]) -> Result<(), SessionStoreError> {
        #[cfg(test)]
        if self.write_failure {
            return Err(SessionStoreError::StorageFull);
        }
        fs::create_dir_all(&self.root).map_err(map_io)?;
        let temporary = self.temporary_path();
        let _ = fs::remove_file(&temporary);
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(map_io)?;
        file.write_all(bytes).map_err(map_io)?;
        file.sync_all().map_err(map_io)?;
        drop(file);
        crate::session_store::replace_with_backup(
            &temporary,
            &self.current_path(),
            &self.backup_path(),
        )
        .map_err(map_io)
    }

    #[cfg(test)]
    fn with_write_failure(mut self) -> Self {
        self.write_failure = true;
        self
    }
}

impl BookmarkStore for WindowsBookmarkStore {
    fn save(&self, bookmarks: &Bookmarks) -> Result<(), SessionStoreError> {
        let bytes = serde_json::to_vec_pretty(&BookmarkEnvelope {
            schema_version: SCHEMA_VERSION,
            bookmarks: bookmarks.clone(),
        })
        .map_err(|_| SessionStoreError::InvalidSnapshot("bookmark encoding failed".to_owned()))?;
        if bytes.len() > self.limits.max_state_payload_bytes {
            return Err(SessionStoreError::InvalidSnapshot(
                "bookmark payload exceeds persistence limit".to_owned(),
            ));
        }
        self.write_document(&bytes)
    }
}

#[derive(Debug)]
enum ReadFailure {
    Invalid,
    Io(io::Error),
}

fn map_io(error: io::Error) -> SessionStoreError {
    match error.kind() {
        io::ErrorKind::PermissionDenied => SessionStoreError::AccessDenied,
        io::ErrorKind::StorageFull => SessionStoreError::StorageFull,
        _ => SessionStoreError::Io(format!("kind={:?}", error.kind())),
    }
}

#[cfg(test)]
mod tests {
    use explorer_model::{BookmarkTarget, LocationDescriptor, SyntheticRoot};
    use tempfile::TempDir;

    use super::*;

    fn sample(name: &str) -> Bookmarks {
        let mut bookmarks = Bookmarks::default();
        let mutation = bookmarks.begin_add(
            name.to_owned(),
            BookmarkTarget::Folder {
                location: LocationDescriptor::synthetic(SyntheticRoot::Home),
            },
        );
        assert!(mutation.changed());
        bookmarks
    }

    #[test]
    fn independent_current_wins_over_legacy() {
        let directory = TempDir::new().expect("temporary directory");
        let store = WindowsBookmarkStore::at_root(
            directory.path().join("bookmarks"),
            RoadmapLimits::default(),
        );
        let independent = sample("independent");
        store.save(&independent).expect("save independent");
        let resolved = store.load_or_migrate(&sample("legacy"));
        assert_eq!(resolved.bookmarks, independent);
        assert!(!resolved.migration_performed);
    }

    #[test]
    fn empty_independent_document_is_authoritative() {
        let directory = TempDir::new().expect("temporary directory");
        let store = WindowsBookmarkStore::at_root(
            directory.path().join("bookmarks"),
            RoadmapLimits::default(),
        );
        store.save(&Bookmarks::default()).expect("save empty");
        let resolved = store.load_or_migrate(&sample("legacy"));
        assert_eq!(resolved.bookmarks, Bookmarks::default());
    }

    #[test]
    fn replacement_rotates_last_known_good() {
        let directory = TempDir::new().expect("temporary directory");
        let store = WindowsBookmarkStore::at_root(
            directory.path().join("bookmarks"),
            RoadmapLimits::default(),
        );
        let first = sample("first");
        let second = sample("second");
        store.save(&first).expect("first save");
        store.save(&second).expect("second save");
        assert_eq!(store.load().expect("load").bookmarks, Some(second));
        assert_eq!(
            store.read_document(&store.backup_path()).expect("backup"),
            Some(first)
        );
    }

    #[test]
    fn corrupt_current_recovers_backup_and_repairs_current() {
        let directory = TempDir::new().expect("temporary directory");
        let store = WindowsBookmarkStore::at_root(
            directory.path().join("bookmarks"),
            RoadmapLimits::default(),
        );
        let first = sample("first");
        store.save(&first).expect("first save");
        store.save(&sample("second")).expect("second save");
        fs::write(store.current_path(), b"truncated").expect("corrupt current");
        let loaded = store.load().expect("recover");
        assert_eq!(loaded.source, BookmarkLoadSource::LastKnownGood);
        assert_eq!(loaded.bookmarks, Some(first.clone()));
        assert_eq!(
            store
                .read_document(&store.current_path())
                .expect("repaired"),
            Some(first)
        );
    }

    #[test]
    fn backup_remains_usable_when_current_repair_fails() {
        let directory = TempDir::new().expect("temporary directory");
        let root = directory.path().join("bookmarks");
        let healthy = WindowsBookmarkStore::at_root(root.clone(), RoadmapLimits::default());
        let first = sample("first");
        healthy.save(&first).expect("first save");
        healthy.save(&sample("second")).expect("second save");
        fs::write(healthy.current_path(), b"truncated").expect("corrupt current");

        let failing =
            WindowsBookmarkStore::at_root(root, RoadmapLimits::default()).with_write_failure();
        let loaded = failing.load().expect("backup remains readable");
        assert_eq!(loaded.bookmarks, Some(first));
        assert_eq!(loaded.source, BookmarkLoadSource::LastKnownGood);
        assert!(loaded.warning.is_some());
    }

    #[test]
    fn oversized_current_falls_back_without_unbounded_read() {
        let directory = TempDir::new().expect("temporary directory");
        let mut limits = RoadmapLimits::default();
        limits.max_state_payload_bytes = 128;
        let store = WindowsBookmarkStore::at_root(directory.path().join("bookmarks"), limits);
        fs::create_dir_all(&store.root).expect("root");
        fs::write(store.current_path(), vec![b'x'; 129]).expect("oversized");
        let outcome = store.load().expect("bounded load");
        assert!(outcome.bookmarks.is_none());
        assert_eq!(outcome.rejected_artifacts, 1);
    }

    #[test]
    fn corrupt_artifacts_preserve_unrelated_files() {
        let directory = TempDir::new().expect("temporary directory");
        let store = WindowsBookmarkStore::at_root(
            directory.path().join("bookmarks"),
            RoadmapLimits::default(),
        );
        fs::create_dir_all(&store.root).expect("root");
        fs::write(store.current_path(), b"bad").expect("current");
        fs::write(store.backup_path(), b"also bad").expect("backup");
        let unrelated = store.root.join("keep.txt");
        fs::write(&unrelated, b"keep").expect("unrelated");
        let outcome = store.load().expect("load");
        assert_eq!(outcome.rejected_artifacts, 2);
        assert!(unrelated.exists());
    }

    #[test]
    fn missing_store_migrates_legacy_once() {
        let directory = TempDir::new().expect("temporary directory");
        let store = WindowsBookmarkStore::at_root(
            directory.path().join("bookmarks"),
            RoadmapLimits::default(),
        );
        let legacy = sample("legacy");
        let first = store.load_or_migrate(&legacy);
        assert!(first.migration_performed);
        let second = store.load_or_migrate(&sample("different"));
        assert!(!second.migration_performed);
        assert_eq!(second.bookmarks, legacy);
    }

    #[test]
    fn migration_failure_returns_legacy_without_mutation() {
        let directory = TempDir::new().expect("temporary directory");
        let blocking_file = directory.path().join("blocking-file");
        fs::write(&blocking_file, b"file").expect("blocking file");
        let store = WindowsBookmarkStore::at_root(
            blocking_file.join("bookmarks"),
            RoadmapLimits::default(),
        );
        let legacy = sample("legacy");
        let resolved = store.load_or_migrate(&legacy);
        assert_eq!(resolved.bookmarks, legacy);
        assert!(!resolved.migration_performed);
        assert!(resolved.warning.is_some());
        assert_eq!(fs::read(blocking_file).expect("unchanged"), b"file");
    }
}
