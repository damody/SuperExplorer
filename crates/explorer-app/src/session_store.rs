//! Versioned Windows session storage with bounded reads and recoverable atomic replacement.

use std::{
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{self, Read as _, Write as _},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use explorer_common::RoadmapLimits;
use explorer_model::{
    PersistedSessionEnvelope, SessionLoadOutcome, SessionLoadSource, SessionResetScope,
    SessionStore, SessionStoreError,
};

const STATE_DIRECTORY: &str = "RustGpuiExplorer\\state\\v1";
const CURRENT_FILE: &str = "session.json";
const BACKUP_FILE: &str = "session.last-known-good.json";
const TEMP_FILE: &str = "session.pending.json";

/// Production filesystem adapter. The root is owned by this application only.
#[derive(Clone, Debug)]
pub struct WindowsSessionStore {
    root: PathBuf,
    limits: RoadmapLimits,
    #[cfg(test)]
    fault: Option<SessionStoreFault>,
}

impl WindowsSessionStore {
    /// Resolves `%LOCALAPPDATA%\RustGpuiExplorer\state\v1` without creating it.
    ///
    /// # Errors
    ///
    /// Returns unavailable when the Windows per-user application-data root is absent.
    pub fn from_environment(limits: RoadmapLimits) -> Result<Self, SessionStoreError> {
        let local = std::env::var_os("LOCALAPPDATA").ok_or_else(|| {
            SessionStoreError::Unavailable("LOCALAPPDATA is not available".to_owned())
        })?;
        Ok(Self::at_root(
            PathBuf::from(local).join(STATE_DIRECTORY),
            limits,
        ))
    }

    /// Creates an adapter rooted at an explicitly application-owned directory.
    pub fn at_root(root: PathBuf, limits: RoadmapLimits) -> Self {
        Self {
            root,
            limits,
            #[cfg(test)]
            fault: None,
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

    fn read_snapshot(&self, path: &Path) -> Result<Option<LoadedSnapshot>, ReadFailure> {
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
        PersistedSessionEnvelope::decode_or_migrate(&bytes, self.limits)
            .map(|(envelope, migrated)| Some(LoadedSnapshot { envelope, migrated }))
            .map_err(|error| ReadFailure::Invalid(error.to_string()))
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
                .unwrap_or_else(|| std::ffi::OsStr::new("session")),
        );
        name.push(format!(".corrupt.{timestamp}"));
        fs::rename(path, self.root.join(name))
    }

    fn write_snapshot(&self, bytes: &[u8]) -> Result<(), SessionStoreError> {
        self.fail(SessionStoreFault::CreateDirectory)?;
        fs::create_dir_all(&self.root).map_err(map_io)?;
        let temporary = self.temporary_path();
        let _ = fs::remove_file(&temporary);

        self.fail(SessionStoreFault::OpenTemporary)?;
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(map_io)?;
        self.fail(SessionStoreFault::Write)?;
        if self.is_fault(SessionStoreFault::ShortWrite) {
            let midpoint = bytes.len().saturating_div(2);
            file.write_all(&bytes[..midpoint]).map_err(map_io)?;
            return Err(SessionStoreError::Io("short write".to_owned()));
        }
        file.write_all(bytes).map_err(map_io)?;
        self.fail(SessionStoreFault::Flush)?;
        file.sync_all().map_err(map_io)?;
        drop(file);
        self.fail(SessionStoreFault::Close)?;

        if self.current_path().exists() {
            self.fail(SessionStoreFault::BackupRotation)?;
        }
        self.fail(SessionStoreFault::Replace)?;
        replace_with_backup(&temporary, &self.current_path(), &self.backup_path()).map_err(map_io)
    }

    #[cfg(test)]
    fn with_fault(mut self, fault: SessionStoreFault) -> Self {
        self.fault = Some(fault);
        self
    }

    #[allow(
        clippy::unused_self,
        reason = "production and test builds intentionally share the same fault-check call sites"
    )]
    fn is_fault(&self, fault: SessionStoreFault) -> bool {
        #[cfg(test)]
        {
            self.fault == Some(fault)
        }
        #[cfg(not(test))]
        {
            let _ = fault;
            false
        }
    }

    fn fail(&self, fault: SessionStoreFault) -> Result<(), SessionStoreError> {
        if self.is_fault(fault) {
            return Err(match fault {
                SessionStoreFault::AccessDenied => SessionStoreError::AccessDenied,
                SessionStoreFault::StorageFull => SessionStoreError::StorageFull,
                _ => SessionStoreError::Io(fault.label().to_owned()),
            });
        }
        Ok(())
    }
}

impl SessionStore for WindowsSessionStore {
    fn load(&self) -> Result<SessionLoadOutcome, SessionStoreError> {
        let mut rejected = 0;
        for (path, source) in [
            (self.current_path(), SessionLoadSource::Current),
            (self.backup_path(), SessionLoadSource::LastKnownGood),
        ] {
            match self.read_snapshot(&path) {
                Ok(Some(loaded)) => {
                    if loaded.migrated {
                        self.save(&loaded.envelope)?;
                    }
                    return Ok(SessionLoadOutcome {
                        source,
                        envelope: Some(loaded.envelope),
                        rejected_artifacts: rejected,
                        migration_performed: loaded.migrated,
                    });
                }
                Ok(None) => {}
                Err(ReadFailure::Invalid(_reason)) => {
                    rejected += 1;
                    let _ = self.quarantine(&path);
                }
                Err(ReadFailure::Io(error)) => return Err(map_io(error)),
            }
        }
        Ok(SessionLoadOutcome {
            source: SessionLoadSource::Defaults,
            envelope: None,
            rejected_artifacts: rejected,
            migration_performed: false,
        })
    }

    fn save(&self, envelope: &PersistedSessionEnvelope) -> Result<(), SessionStoreError> {
        self.fail(SessionStoreFault::AccessDenied)?;
        self.fail(SessionStoreFault::StorageFull)?;
        let bytes = envelope
            .encode_pretty(self.limits)
            .map_err(|error| SessionStoreError::InvalidSnapshot(error.to_string()))?;
        self.write_snapshot(&bytes)
    }

    fn reset(&self, scope: SessionResetScope) -> Result<(), SessionStoreError> {
        match scope {
            SessionResetScope::Session | SessionResetScope::AllRoadmapState => {
                for path in [
                    self.current_path(),
                    self.backup_path(),
                    self.temporary_path(),
                ] {
                    match fs::remove_file(path) {
                        Ok(()) => {}
                        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                        Err(error) => return Err(map_io(error)),
                    }
                }
                Ok(())
            }
            SessionResetScope::ViewSettings | SessionResetScope::QuickAccess => {
                let Some(mut envelope) = self.load()?.envelope else {
                    return Ok(());
                };
                match scope {
                    SessionResetScope::ViewSettings => {
                        for tab in &mut envelope.payload.tabs {
                            tab.view_settings = explorer_model::PersistedViewSettings::default();
                        }
                    }
                    SessionResetScope::QuickAccess => envelope.payload.quick_access.clear(),
                    SessionResetScope::Session | SessionResetScope::AllRoadmapState => {}
                }
                envelope = PersistedSessionEnvelope::new(
                    envelope.write_generation.saturating_add(1),
                    envelope.provenance,
                    envelope.payload,
                    self.limits,
                )
                .map_err(|error| SessionStoreError::InvalidSnapshot(error.to_string()))?;
                self.save(&envelope)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SessionStoreFault {
    CreateDirectory,
    OpenTemporary,
    Write,
    ShortWrite,
    Flush,
    Close,
    BackupRotation,
    Replace,
    AccessDenied,
    StorageFull,
}

impl SessionStoreFault {
    const fn label(self) -> &'static str {
        match self {
            Self::CreateDirectory => "create directory",
            Self::OpenTemporary => "open temporary",
            Self::Write => "write",
            Self::ShortWrite => "short write",
            Self::Flush => "flush",
            Self::Close => "close",
            Self::BackupRotation => "backup rotation",
            Self::Replace => "replace",
            Self::AccessDenied => "access denied",
            Self::StorageFull => "storage full",
        }
    }
}

#[derive(Debug)]
enum ReadFailure {
    Invalid(String),
    Io(io::Error),
}

struct LoadedSnapshot {
    envelope: PersistedSessionEnvelope,
    migrated: bool,
}

#[allow(
    clippy::needless_pass_by_value,
    reason = "I/O Result::map_err supplies an owned error and this adapter is used directly"
)]
fn map_io(error: io::Error) -> SessionStoreError {
    match error.kind() {
        io::ErrorKind::PermissionDenied => SessionStoreError::AccessDenied,
        io::ErrorKind::StorageFull => SessionStoreError::StorageFull,
        _ => SessionStoreError::Io(format!("kind={:?}", error.kind())),
    }
}

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "Windows atomic replacement requires NUL-terminated path pointers for the duration of the FFI call"
)]
pub(crate) fn replace_with_backup(
    temporary: &Path,
    current: &Path,
    backup: &Path,
) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt as _;
    use windows::{
        Win32::Storage::FileSystem::{
            MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
            REPLACEFILE_WRITE_THROUGH, ReplaceFileW,
        },
        core::PCWSTR,
    };

    fn wide(path: &Path) -> Vec<u16> {
        path.as_os_str().encode_wide().chain(Some(0)).collect()
    }

    let temporary_wide = wide(temporary);
    let current_wide = wide(current);
    let backup_wide = wide(backup);
    let temporary_pcwstr = PCWSTR(temporary_wide.as_ptr());
    let current_pcwstr = PCWSTR(current_wide.as_ptr());
    if current.exists() {
        let backup_pcwstr = PCWSTR(backup_wide.as_ptr());
        // SAFETY: all pointers reference live, NUL-terminated UTF-16 buffers for this call.
        unsafe {
            ReplaceFileW(
                current_pcwstr,
                temporary_pcwstr,
                backup_pcwstr,
                REPLACEFILE_WRITE_THROUGH,
                None,
                None,
            )
        }
        .map_err(io::Error::other)
    } else {
        // SAFETY: both pointers reference live, NUL-terminated UTF-16 buffers for this call.
        unsafe {
            MoveFileExW(
                temporary_pcwstr,
                current_pcwstr,
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        }
        .map_err(io::Error::other)
    }
}

#[cfg(not(windows))]
pub(crate) fn replace_with_backup(
    temporary: &Path,
    current: &Path,
    backup: &Path,
) -> io::Result<()> {
    if current.exists() {
        let _ = fs::remove_file(backup);
        fs::rename(current, backup)?;
    }
    fs::rename(temporary, current)
}

#[cfg(test)]
mod tests {
    use explorer_model::{
        ExplorerWindowState, HistoryEntry, LocationDescriptor, PersistedQuickAccessPin,
        PersistedRect, PersistedWindowPlacement, SessionProvenance, SyntheticRoot,
    };
    use tempfile::TempDir;

    use super::*;

    fn snapshot(generation: u64) -> PersistedSessionEnvelope {
        let window = ExplorerWindowState::new(HistoryEntry::new(
            LocationDescriptor::synthetic(SyntheticRoot::Home),
            "Home",
        ));
        PersistedSessionEnvelope::project(
            &window,
            PersistedWindowPlacement {
                normal_bounds: PersistedRect {
                    left: 10,
                    top: 10,
                    width: 1000,
                    height: 700,
                },
                source_work_area: PersistedRect {
                    left: 0,
                    top: 0,
                    width: 1920,
                    height: 1080,
                },
                source_dpi: 96,
                maximized: false,
            },
            &[PersistedQuickAccessPin {
                location: LocationDescriptor::synthetic(SyntheticRoot::QuickAccess),
                display_name: "Quick Access".to_owned(),
                order: 0,
            }],
            true,
            generation,
            SessionProvenance {
                app_version: "test".to_owned(),
                app_revision: "fixture".to_owned(),
                windows_build: "test".to_owned(),
            },
            RoadmapLimits::default(),
        )
        .expect("valid fixture")
    }

    #[test]
    fn replacement_rotates_last_known_good_and_loads_current() {
        let directory = TempDir::new().expect("temporary directory");
        let store =
            WindowsSessionStore::at_root(directory.path().join("state"), RoadmapLimits::default());
        store.save(&snapshot(1)).expect("first save");
        store.save(&snapshot(2)).expect("second save");
        let loaded = store.load().expect("load");
        assert_eq!(loaded.source, SessionLoadSource::Current);
        assert_eq!(loaded.envelope.expect("snapshot").write_generation, 2);
        let backup = store
            .read_snapshot(&store.backup_path())
            .expect("backup read")
            .expect("backup");
        assert_eq!(backup.envelope.write_generation, 1);
    }

    #[test]
    fn corrupt_current_is_retained_and_backup_recovers() {
        let directory = TempDir::new().expect("temporary directory");
        let store =
            WindowsSessionStore::at_root(directory.path().join("state"), RoadmapLimits::default());
        store.save(&snapshot(1)).expect("first save");
        store.save(&snapshot(2)).expect("second save");
        fs::write(store.current_path(), b"truncated").expect("corrupt current");
        let loaded = store.load().expect("recover");
        assert_eq!(loaded.source, SessionLoadSource::LastKnownGood);
        assert_eq!(loaded.rejected_artifacts, 1);
        assert_eq!(loaded.envelope.expect("backup").write_generation, 1);
        let retained = fs::read_dir(&store.root)
            .expect("state listing")
            .filter_map(Result::ok)
            .any(|entry| entry.file_name().to_string_lossy().contains(".corrupt."));
        assert!(retained);
    }

    #[test]
    fn every_pre_replace_fault_preserves_last_valid_snapshot() {
        let faults = [
            SessionStoreFault::CreateDirectory,
            SessionStoreFault::OpenTemporary,
            SessionStoreFault::Write,
            SessionStoreFault::ShortWrite,
            SessionStoreFault::Flush,
            SessionStoreFault::Close,
            SessionStoreFault::BackupRotation,
            SessionStoreFault::Replace,
            SessionStoreFault::AccessDenied,
            SessionStoreFault::StorageFull,
        ];
        for fault in faults {
            let directory = TempDir::new().expect("temporary directory");
            let root = directory.path().join("state");
            let healthy = WindowsSessionStore::at_root(root.clone(), RoadmapLimits::default());
            healthy.save(&snapshot(1)).expect("baseline");
            let failing =
                WindowsSessionStore::at_root(root, RoadmapLimits::default()).with_fault(fault);
            assert!(failing.save(&snapshot(2)).is_err(), "{fault:?}");
            assert_eq!(
                healthy
                    .load()
                    .expect("load baseline")
                    .envelope
                    .expect("snapshot")
                    .write_generation,
                1,
                "{fault:?}"
            );
        }
    }

    #[test]
    fn reset_scopes_do_not_delete_unrelated_app_data_or_bookmarks() {
        let directory = TempDir::new().expect("temporary directory");
        let store =
            WindowsSessionStore::at_root(directory.path().join("state"), RoadmapLimits::default());
        store.save(&snapshot(1)).expect("save");
        let unrelated = directory.path().join("logs.txt");
        fs::write(&unrelated, b"keep").expect("unrelated");
        let bookmark = directory
            .path()
            .join("bookmarks")
            .join("v1")
            .join("bookmarks.json");
        fs::create_dir_all(bookmark.parent().expect("bookmark parent")).expect("bookmark root");
        fs::write(&bookmark, b"independent bookmarks").expect("bookmark fixture");
        store
            .reset(SessionResetScope::QuickAccess)
            .expect("reset pins");
        assert!(
            store
                .load()
                .expect("load")
                .envelope
                .expect("snapshot")
                .payload
                .quick_access
                .is_empty()
        );
        store
            .reset(SessionResetScope::Session)
            .expect("reset session");
        assert_eq!(
            store.load().expect("defaults").source,
            SessionLoadSource::Defaults
        );
        assert_eq!(
            fs::read(&bookmark).expect("bookmarks preserved"),
            b"independent bookmarks"
        );
        store.save(&snapshot(2)).expect("save after session reset");
        store
            .reset(SessionResetScope::AllRoadmapState)
            .expect("reset all state");
        assert_eq!(
            fs::read(&bookmark).expect("bookmarks preserved"),
            b"independent bookmarks"
        );
        assert!(unrelated.exists());
    }

    #[test]
    fn prior_schema_migrates_and_rewrites_current_exactly_once() {
        let directory = TempDir::new().expect("temporary directory");
        let store =
            WindowsSessionStore::at_root(directory.path().join("state"), RoadmapLimits::default());
        fs::create_dir_all(&store.root).expect("state root");
        fs::write(
            store.current_path(),
            include_bytes!("../../explorer-model/src/fixtures/session_v0.json"),
        )
        .expect("legacy state");

        let first = store.load().expect("migrate");
        assert!(first.migration_performed);
        assert_eq!(first.envelope.expect("migrated").write_generation, 8);
        let second = store.load().expect("load rewritten current");
        assert!(!second.migration_performed);
        assert_eq!(second.envelope.expect("current").write_generation, 8);
    }
}
