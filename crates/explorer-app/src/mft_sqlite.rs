//! Crash-consistent per-volume SQLite persistence for the MFT service.

use std::time::Duration;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use rusqlite::{
    Connection, OpenFlags, OptionalExtension as _, Transaction, config::DbConfig, limits::Limit,
    params,
};

use crate::mft_journal::{MftChangeKindV2, MftChangeV2, PENDING_CHANGE_LIMIT, VolumeIdentityV2};
use crate::mft_persistence::JournalCursorV1;
use crate::mft_persistence::LifecycleBarrierV1;
use crate::mft_size_map::{MftAggregateV1, MftEntryV1, MftIndexV1};

const SCHEMA_VERSION: i64 = 1;
const BUSY_TIMEOUT: Duration = Duration::from_secs(2);
pub(crate) const WAL_MAINTENANCE_THRESHOLD_BYTES: u64 = 256 * 1024 * 1024;
pub(crate) const MAX_PENDING_BATCH_BYTES: u64 = 16 * 1024 * 1024;
// Each changed row can dirty more than its encoded payload. This multiplier is
// deliberately conservative and is verified against actual WAL growth in tests.
const WAL_FRAME_OVERHEAD_MULTIPLIER: u64 = 4;
const SQLITE_PAGE_BYTES: u64 = 4096;
const MIN_SQLITE_STORE_PAGES: u64 = 8;
const SQLITE_TEMP_TRANSACTION_OVERHEAD_BYTES: u64 = 1024 * 1024;
// A writer reopen can retain a WAL header and shared-memory index. Keep a
// conservative fixed allowance outside the rollback-mode main/journal build,
// then verify the exact canonical member bytes before returning success.
const SQLITE_WRITER_COMPANION_RESERVE_BYTES: u64 = 1024 * 1024;
const WAL_FRAME_HEADER_BYTES: u64 = 24;
// A tiny encoded delete/update can still dirty one distinct B-tree page. The
// count-derived branch therefore dominates the payload ratio for a maximally
// scattered batch and is measured by the fragmented-store regression below.
const MAX_SCATTERED_WAL_GROWTH_BYTES: u64 = (PENDING_CHANGE_LIMIT as u64)
    .saturating_mul(SQLITE_PAGE_BYTES + WAL_FRAME_HEADER_BYTES)
    .saturating_add(1024 * 1024);
const MAX_INDEX_ENTRIES: usize = 20_000_000;
const MAX_ENTRY_NAME_BYTES: usize = 64 * 1024;
const SQLITE_LENGTH_LIMIT_BYTES: i32 = (MAX_ENTRY_NAME_BYTES + 4096) as i32;
const MAX_INDEX_DECODED_BYTES: usize = 8 * 1024 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct StoreIdentityV1 {
    pub(crate) volume: VolumeIdentityV2,
    pub(crate) cursor: JournalCursorV1,
    pub(crate) complete: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct StoreTelemetryV1 {
    pub(crate) main_bytes: u64,
    pub(crate) wal_bytes: u64,
    pub(crate) transaction_attempts: u64,
    pub(crate) transaction_failures: u64,
    pub(crate) checkpoint_attempts: u64,
    pub(crate) checkpoint_failures: u64,
    pub(crate) transaction_last_outcome: u8,
    pub(crate) checkpoint_last_outcome: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CommitFailurePointV1 {
    None,
    BeforeMutation,
    BeforeCursor,
    BeforeCommit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MigrationFailurePointV1 {
    None,
    Build,
    TempCommit,
    Fsync,
    PreVerify,
    Promote,
    Reopen,
    PostVerify,
}

#[derive(Debug)]
pub(crate) struct MftSqliteStoreV1 {
    connection: Connection,
    path: PathBuf,
    identity: StoreIdentityV1,
    telemetry: StoreTelemetryV1,
}

impl MftSqliteStoreV1 {
    pub(crate) const fn schema_version() -> u8 {
        SCHEMA_VERSION as u8
    }

    pub(crate) fn file_bytes_for_path(path: &Path) -> (u64, u64) {
        let main = std::fs::metadata(path).map_or(0, |metadata| metadata.len());
        let wal = std::fs::metadata(PathBuf::from(format!("{}-wal", path.display())))
            .map_or(0, |metadata| metadata.len());
        (main, wal)
    }

    pub(crate) fn migrate_snapshot(
        temporary: &Path,
        canonical: &Path,
        fixed_root: &Path,
        identity: StoreIdentityV1,
        index: &MftIndexV1,
    ) -> Result<Self, String> {
        Self::migrate_snapshot_injected(
            temporary,
            canonical,
            fixed_root,
            identity,
            index,
            MigrationFailurePointV1::None,
            false,
            None,
            None,
            || true,
        )
    }

    pub(crate) fn migrate_snapshot_guarded(
        temporary: &Path,
        canonical: &Path,
        fixed_root: &Path,
        identity: StoreIdentityV1,
        index: &MftIndexV1,
        lifecycle_open: impl Fn() -> bool + Sync,
    ) -> Result<Self, String> {
        Self::migrate_snapshot_injected(
            temporary,
            canonical,
            fixed_root,
            identity,
            index,
            MigrationFailurePointV1::None,
            false,
            None,
            None,
            lifecycle_open,
        )
    }

    pub(crate) fn rebuild_snapshot_guarded(
        temporary: &Path,
        canonical: &Path,
        fixed_root: &Path,
        identity: StoreIdentityV1,
        index: &MftIndexV1,
        lifecycle_open: impl Fn() -> bool + Sync,
    ) -> Result<Self, String> {
        Self::migrate_snapshot_injected(
            temporary,
            canonical,
            fixed_root,
            identity,
            index,
            MigrationFailurePointV1::None,
            true,
            None,
            None,
            lifecycle_open,
        )
    }

    pub(crate) fn snapshot_linearized(
        temporary: &Path,
        canonical: &Path,
        fixed_root: &Path,
        identity: StoreIdentityV1,
        index: &MftIndexV1,
        replace_existing: bool,
        barrier: &LifecycleBarrierV1,
    ) -> Result<Self, String> {
        Self::migrate_snapshot_injected(
            temporary,
            canonical,
            fixed_root,
            identity,
            index,
            MigrationFailurePointV1::None,
            replace_existing,
            None,
            Some(barrier),
            || barrier.is_open(),
        )
    }

    pub(crate) fn snapshot_focused_linearized(
        temporary: &Path,
        canonical: &Path,
        fixed_root: &Path,
        identity: StoreIdentityV1,
        index: &MftIndexV1,
        replace_existing: bool,
        barrier: &LifecycleBarrierV1,
        focused_now: impl Fn() -> bool + Sync,
    ) -> Result<Self, String> {
        Self::migrate_snapshot_injected(
            temporary,
            canonical,
            fixed_root,
            identity,
            index,
            MigrationFailurePointV1::None,
            replace_existing,
            None,
            Some(barrier),
            || barrier.is_open() && focused_now(),
        )
    }

    pub(crate) fn snapshot_focused_bounded_linearized(
        temporary: &Path,
        canonical: &Path,
        fixed_root: &Path,
        identity: StoreIdentityV1,
        index: &MftIndexV1,
        replace_existing: bool,
        max_candidate_bytes: u64,
        barrier: &LifecycleBarrierV1,
        focused_now: impl Fn() -> bool + Sync,
    ) -> Result<Self, String> {
        if max_candidate_bytes
            < SQLITE_TEMP_TRANSACTION_OVERHEAD_BYTES
                + SQLITE_WRITER_COMPANION_RESERVE_BYTES
                + 2 * MIN_SQLITE_STORE_PAGES * SQLITE_PAGE_BYTES
        {
            return Err("MFT SQLite candidate budget is below the minimum store size".to_owned());
        }
        Self::migrate_snapshot_injected(
            temporary,
            canonical,
            fixed_root,
            identity,
            index,
            MigrationFailurePointV1::None,
            replace_existing,
            Some(max_candidate_bytes),
            Some(barrier),
            || barrier.is_open() && focused_now(),
        )
    }

    fn migrate_snapshot_injected(
        temporary: &Path,
        canonical: &Path,
        fixed_root: &Path,
        identity: StoreIdentityV1,
        index: &MftIndexV1,
        failure: MigrationFailurePointV1,
        replace_existing: bool,
        max_candidate_bytes: Option<u64>,
        barrier: Option<&LifecycleBarrierV1>,
        lifecycle_open: impl Fn() -> bool + Sync,
    ) -> Result<Self, String> {
        validate_store_path(canonical, fixed_root)?;
        validate_migration_temp_path(temporary, canonical, fixed_root)?;
        let canonical_exists = canonical.is_file();
        let companions_exist = Self::canonical_members(canonical)[1..]
            .iter()
            .any(|path| path.exists());
        if (companions_exist || canonical_exists) && !replace_existing {
            return Err("canonical MFT SQLite set appeared before migration".to_owned());
        }
        let temporary_members = [
            temporary.to_path_buf(),
            PathBuf::from(format!("{}-journal", temporary.display())),
            wal_path(temporary),
            PathBuf::from(format!("{}-shm", temporary.display())),
        ];
        if temporary_members.iter().any(|path| path.exists()) {
            let Some(barrier) = barrier else {
                return Err("MFT SQLite migration temporary already exists".to_owned());
            };
            barrier.invoke(|| {
                if !lifecycle_open() {
                    return Err(
                        "MFT SQLite lifecycle or focus gate closed before temp recovery".to_owned(),
                    );
                }
                for member in &temporary_members {
                    if member.exists() {
                        std::fs::remove_file(member).map_err(|error| error.to_string())?;
                    }
                }
                Ok(())
            })?;
        }
        if failure == MigrationFailurePointV1::Build {
            return Err("injected migration build failure".into());
        }
        if !lifecycle_open() {
            return Err("MFT SQLite lifecycle closed before migration build".to_owned());
        }
        let build = || {
            let connection = Connection::open(temporary).map_err(|error| error.to_string())?;
            connection
                .busy_timeout(BUSY_TIMEOUT)
                .map_err(|error| error.to_string())?;
            configure_page_size(&connection)?;
            if let Some(maximum) = max_candidate_bytes {
                // A rollback-journal build can temporarily hold both the main
                // pages and their journal images. Keep both plus headers under
                // the caller's persisted-cache allowance.
                let page_limit = maximum
                    .saturating_sub(SQLITE_TEMP_TRANSACTION_OVERHEAD_BYTES)
                    .saturating_sub(SQLITE_WRITER_COMPANION_RESERVE_BYTES)
                    / (2 * SQLITE_PAGE_BYTES);
                connection
                    .pragma_update(None, "max_page_count", page_limit)
                    .map_err(|error| error.to_string())?;
            }
            let mode: String = connection
                .pragma_update_and_check(None, "journal_mode", "DELETE", |row| row.get(0))
                .map_err(|error| error.to_string())?;
            if !mode.eq_ignore_ascii_case("delete") {
                return Err("migration SQLite refused rollback journal mode".to_owned());
            }
            connection
                .pragma_update(None, "synchronous", "FULL")
                .map_err(|error| error.to_string())?;
            initialize_schema(&connection, identity)?;
            if failure == MigrationFailurePointV1::TempCommit {
                return Err("injected migration temporary commit failure".into());
            }
            install_migration_entries_guarded(&connection, index, &lifecycle_open)?;
            Ok(connection)
        };
        let connection = if let Some(barrier) = barrier {
            barrier.invoke(build)?
        } else {
            build()?
        };
        verify_integrity(&connection)?;
        drop(connection);
        if wal_path(temporary).exists()
            || PathBuf::from(format!("{}-shm", temporary.display())).exists()
        {
            return Err("migration temporary has a live WAL/SHM set".to_owned());
        }
        if failure == MigrationFailurePointV1::Fsync {
            return Err("injected migration fsync failure".into());
        }
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(temporary)
            .and_then(|file| file.sync_all())
            .map_err(|error| error.to_string())?;
        if max_candidate_bytes.is_some_and(|limit| file_bytes(temporary) > limit) {
            return Err("MFT SQLite candidate exceeds the configured persisted budget".to_owned());
        }
        if failure == MigrationFailurePointV1::PreVerify {
            return Err("injected migration pre-verify failure".into());
        }
        let verify_connection = Connection::open_with_flags(
            temporary,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|error| error.to_string())?;
        verify_integrity(&verify_connection)?;
        let verified = read_identity(&verify_connection)?;
        let verified_index = load_index_from_connection(&verify_connection)?;
        drop(verify_connection);
        if verified != identity || verified_index.entries.len() != index.entries.len() {
            return Err("migration temporary verification mismatch".to_owned());
        }
        let canonical_still_exists = canonical.is_file();
        let companions_appeared = Self::canonical_members(canonical)[1..]
            .iter()
            .any(|path| path.exists());
        if (!replace_existing && companions_appeared)
            || (!replace_existing && canonical_still_exists)
            || (replace_existing && canonical_still_exists != canonical_exists)
        {
            return Err("canonical MFT SQLite set appeared during migration".to_owned());
        }
        if failure == MigrationFailurePointV1::Promote {
            return Err("injected migration promote failure".into());
        }
        if !lifecycle_open() {
            return Err("MFT SQLite lifecycle closed before migration promotion".to_owned());
        }
        let replacement_backup = replace_existing.then(|| Self::replacement_backup_path(canonical));
        if replacement_backup
            .as_ref()
            .is_some_and(|path| path.exists())
        {
            return Err("MFT SQLite replacement backup requires recovery".to_owned());
        }
        if let Some(backup) = replacement_backup.as_ref() {
            let open_source = || {
                if !lifecycle_open() {
                    return Err(
                        "MFT SQLite lifecycle or focus gate closed before safety copy".to_owned(),
                    );
                }
                let old = Connection::open(canonical).map_err(|error| error.to_string())?;
                configure(&old)?;
                Ok(old)
            };
            let old = if let Some(barrier) = barrier {
                barrier.invoke(open_source)?
            } else {
                open_source()?
            };
            run_cancellable_vacuum_into(&old, backup, &lifecycle_open)?;
            drop(old);
            let finish_replacement = || {
                if !lifecycle_open() {
                    return Err(
                        "MFT SQLite lifecycle or focus gate closed after safety copy".to_owned(),
                    );
                }
                sync_file(backup)?;
                let verified = Connection::open_with_flags(
                    backup,
                    OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
                )
                .map_err(|error| error.to_string())?;
                verify_integrity(&verified)?;
                let _ = read_identity(&verified)?;
                drop(verified);
                if !lifecycle_open() {
                    return Err(
                        "MFT SQLite lifecycle or focus gate closed before WAL disposition"
                            .to_owned(),
                    );
                }
                for companion in &Self::canonical_members(canonical)[1..] {
                    if companion.exists() {
                        std::fs::remove_file(companion).map_err(|error| error.to_string())?;
                    }
                }
                Ok(())
            };
            if let Some(barrier) = barrier {
                barrier.invoke(finish_replacement)?;
            } else {
                finish_replacement()?;
            }
        }
        if let Some(barrier) = barrier {
            barrier.invoke(|| {
                if !lifecycle_open() {
                    return Err("MFT SQLite lifecycle or focus gate closed at promotion".to_owned());
                }
                if replace_existing {
                    atomic_replace_file(temporary, canonical, None)
                } else {
                    std::fs::rename(temporary, canonical).map_err(|error| error.to_string())
                }
            })?;
        } else if replace_existing {
            atomic_replace_file(temporary, canonical, None)?;
        } else {
            std::fs::rename(temporary, canonical).map_err(|error| error.to_string())?;
        }
        let admission = (|| {
            if failure == MigrationFailurePointV1::Reopen {
                return Err("injected migration reopen failure".into());
            }
            let reopen = || {
                if !lifecycle_open() {
                    return Err(
                        "MFT SQLite lifecycle or focus gate closed before reopen".to_owned()
                    );
                }
                Self::open_expected_completeness(
                    canonical,
                    fixed_root,
                    identity.volume,
                    identity.cursor.journal_id,
                    identity.complete,
                )
            };
            let store = if let Some(barrier) = barrier {
                barrier.invoke(reopen)?
            } else {
                reopen()?
            };
            if failure == MigrationFailurePointV1::PostVerify {
                return Err("injected migration post-verify failure".into());
            }
            if store.identity() != identity || store.entry_count()? != index.entries.len() as u64 {
                return Err("promoted MFT SQLite verification mismatch".to_owned());
            }
            if max_candidate_bytes.is_some_and(|limit| {
                Self::canonical_members(canonical)
                    .iter()
                    .map(|member| std::fs::metadata(member).map_or(0, |metadata| metadata.len()))
                    .sum::<u64>()
                    > limit
            }) {
                return Err(
                    "promoted MFT SQLite set exceeds the configured persisted budget".to_owned(),
                );
            }
            Ok(store)
        })();
        match admission {
            Ok(store) => {
                if let Some(backup) = replacement_backup {
                    let cleanup = || {
                        if !lifecycle_open() {
                            return Err("MFT SQLite gate closed before backup cleanup".to_owned());
                        }
                        std::fs::remove_file(backup).map_err(|error| error.to_string())
                    };
                    if let Some(barrier) = barrier {
                        barrier.invoke(cleanup)?;
                    } else {
                        cleanup()?;
                    }
                }
                Ok(store)
            }
            Err(error) => {
                if let Some(backup) = replacement_backup
                    && backup.is_file()
                {
                    let restore = || {
                        if !lifecycle_open() {
                            return Err("MFT SQLite gate closed before backup restore".to_owned());
                        }
                        for companion in &Self::canonical_members(canonical)[1..] {
                            if companion.exists() {
                                std::fs::remove_file(companion)
                                    .map_err(|failure| failure.to_string())?;
                            }
                        }
                        atomic_replace_file(&backup, canonical, None)
                    };
                    if let Some(barrier) = barrier {
                        let _ = barrier.invoke(restore);
                    } else {
                        let _ = restore();
                    }
                }
                Err(error)
            }
        }
    }
    /// Admits and loads a canonical store without enabling WAL persistence or
    /// changing connection pragmas. This is the unfocused startup path.
    pub(crate) fn load_read_only(
        path: &Path,
        fixed_root: &Path,
        expected_volume: VolumeIdentityV2,
        expected_journal_id: u64,
    ) -> Result<(StoreIdentityV1, MftIndexV1), String> {
        validate_store_path(path, fixed_root)?;
        Self::load_read_only_unvalidated(path, expected_volume, expected_journal_id)
    }

    /// Computes one exact folder aggregate from an admitted durable store
    /// without materializing the whole volume index. The service verifies that
    /// `expected_cursor` still equals the NTFS journal before and after this
    /// read, so a budget-partial memory topology can never make stale SQLite
    /// data look exact.
    pub(crate) fn query_folder_aggregate_read_only(
        path: &Path,
        fixed_root: &Path,
        expected_volume: VolumeIdentityV2,
        expected_cursor: JournalCursorV1,
        reference: u64,
        possibly_changed_references: &HashSet<u64>,
    ) -> Result<MftAggregateV1, String> {
        validate_store_path(path, fixed_root)?;
        if !path.is_file() {
            return Err("MFT SQLite store is unavailable".to_owned());
        }
        let mut connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|error| error.to_string())?;
        connection
            .busy_timeout(BUSY_TIMEOUT)
            .map_err(|error| error.to_string())?;
        connection
            .pragma_update(None, "temp_store", "MEMORY")
            .map_err(|error| error.to_string())?;
        verify_page_size(&connection)?;
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        let identity = read_identity(&transaction)?;
        if !identity.complete
            || identity.volume != expected_volume
            || identity.cursor != expected_cursor
        {
            return Err("MFT SQLite aggregate identity is stale".to_owned());
        }
        validate_cursor(identity.cursor)?;
        let mut statement = transaction
            .prepare(
                "WITH RECURSIVE descendants(reference) AS (
                     SELECT ?1
                     UNION
                     SELECT entries.reference
                       FROM entries
                       JOIN descendants
                         ON entries.parent_reference = descendants.reference
                      WHERE entries.reference != entries.parent_reference
                 )
                 SELECT entries.logical_bytes, entries.allocated_bytes, entries.kind,
                        entries.reference
                   FROM entries
                   JOIN descendants ON descendants.reference = entries.reference",
            )
            .map_err(|error| error.to_string())?;
        let mut rows = statement
            .query([encode_u64(reference)])
            .map_err(|error| error.to_string())?;
        let mut aggregate = MftAggregateV1::default();
        let mut found = false;
        while let Some(row) = rows.next().map_err(|error| error.to_string())? {
            found = true;
            let row_reference =
                decode_u64(row.get::<_, i64>(3).map_err(|error| error.to_string())?);
            if possibly_changed_references.contains(&row_reference) {
                return Err("folder aggregate changed after the durable cursor".to_owned());
            }
            let logical = decode_u64(row.get::<_, i64>(0).map_err(|error| error.to_string())?);
            let allocated = decode_u64(row.get::<_, i64>(1).map_err(|error| error.to_string())?);
            let is_directory = row.get::<_, i64>(2).map_err(|error| error.to_string())? != 0;
            aggregate.logical_bytes = aggregate.logical_bytes.saturating_add(logical);
            aggregate.allocated_bytes = aggregate.allocated_bytes.saturating_add(allocated);
            aggregate.file_count = aggregate
                .file_count
                .saturating_add(u64::from(!is_directory));
            aggregate.directory_count = aggregate
                .directory_count
                .saturating_add(u64::from(is_directory));
        }
        drop(rows);
        drop(statement);
        transaction.commit().map_err(|error| error.to_string())?;
        if !found {
            return Err("folder aggregate is unavailable".to_owned());
        }
        Ok(aggregate)
    }

    /// Read-only startup admission with service live-memory ceilings. The SQL
    /// aggregate preflight avoids materializing an otherwise valid multi-GB
    /// canonical index merely to trim it after allocation.
    pub(crate) fn load_read_only_bounded(
        path: &Path,
        fixed_root: &Path,
        expected_volume: VolumeIdentityV2,
        expected_journal_id: u64,
        volume_limit_bytes: usize,
        file_limit_bytes: usize,
    ) -> Result<(StoreIdentityV1, MftIndexV1, bool), String> {
        validate_store_path(path, fixed_root)?;
        Self::load_read_only_bounded_unvalidated(
            path,
            expected_volume,
            expected_journal_id,
            volume_limit_bytes,
            file_limit_bytes,
        )
    }

    pub(crate) fn load_read_only_bounded_cancelled(
        path: &Path,
        fixed_root: &Path,
        expected_volume: VolumeIdentityV2,
        expected_journal_id: u64,
        volume_limit_bytes: usize,
        file_limit_bytes: usize,
        cancelled: fn() -> bool,
    ) -> Result<(StoreIdentityV1, MftIndexV1, bool), String> {
        validate_store_path(path, fixed_root)?;
        Self::load_read_only_bounded_unvalidated_cancelled(
            path,
            expected_volume,
            expected_journal_id,
            volume_limit_bytes,
            file_limit_bytes,
            cancelled,
        )
    }

    pub(crate) fn replacement_backup_path(canonical: &Path) -> PathBuf {
        PathBuf::from(format!("{}.replacement-backup", canonical.display()))
    }

    pub(crate) fn load_replacement_backup_read_only(
        backup: &Path,
        canonical: &Path,
        fixed_root: &Path,
        expected_volume: VolumeIdentityV2,
        expected_journal_id: u64,
    ) -> Result<(StoreIdentityV1, MftIndexV1), String> {
        validate_replacement_backup_path(backup, canonical, fixed_root)?;
        Self::load_read_only_unvalidated(backup, expected_volume, expected_journal_id)
    }

    pub(crate) fn load_replacement_backup_read_only_bounded(
        backup: &Path,
        canonical: &Path,
        fixed_root: &Path,
        expected_volume: VolumeIdentityV2,
        expected_journal_id: u64,
        volume_limit_bytes: usize,
        file_limit_bytes: usize,
    ) -> Result<(StoreIdentityV1, MftIndexV1, bool), String> {
        validate_replacement_backup_path(backup, canonical, fixed_root)?;
        Self::load_read_only_bounded_unvalidated(
            backup,
            expected_volume,
            expected_journal_id,
            volume_limit_bytes,
            file_limit_bytes,
        )
    }

    pub(crate) fn restore_replacement_backup_focused_linearized(
        backup: &Path,
        canonical: &Path,
        fixed_root: &Path,
        expected_volume: VolumeIdentityV2,
        expected_journal_id: u64,
        barrier: &LifecycleBarrierV1,
        focused_now: impl Fn() -> bool,
    ) -> Result<Self, String> {
        let (backup_identity, backup_index) = Self::load_replacement_backup_read_only(
            backup,
            canonical,
            fixed_root,
            expected_volume,
            expected_journal_id,
        )?;
        barrier.invoke(|| {
            if !barrier.is_open() || !focused_now() {
                return Err("MFT SQLite gate closed before backup recovery".to_owned());
            }
            for companion in &Self::canonical_members(canonical)[1..] {
                if companion.exists() {
                    std::fs::remove_file(companion).map_err(|error| error.to_string())?;
                }
            }
            if canonical.exists() {
                atomic_replace_file(backup, canonical, None)
            } else {
                std::fs::rename(backup, canonical).map_err(|error| error.to_string())
            }
        })?;
        let store = barrier.invoke(|| {
            if !barrier.is_open() || !focused_now() {
                return Err("MFT SQLite gate closed before recovered-store reopen".to_owned());
            }
            Self::open(canonical, fixed_root, expected_volume, expected_journal_id)
        })?;
        if store.identity() != backup_identity
            || store.entry_count()? != backup_index.entries.len() as u64
        {
            return Err("recovered MFT SQLite backup verification mismatch".to_owned());
        }
        Ok(store)
    }

    pub(crate) fn cleanup_replacement_backup_focused_linearized(
        backup: &Path,
        canonical: &Path,
        fixed_root: &Path,
        barrier: &LifecycleBarrierV1,
        focused_now: impl Fn() -> bool,
    ) -> Result<(), String> {
        validate_replacement_backup_path(backup, canonical, fixed_root)?;
        if !backup.exists() {
            return Ok(());
        }
        barrier.invoke(|| {
            if !barrier.is_open() || !focused_now() {
                return Err("MFT SQLite gate closed before backup cleanup".to_owned());
            }
            std::fs::remove_file(backup).map_err(|error| error.to_string())
        })
    }

    /// Admits and loads a canonical store without enabling WAL persistence or
    /// changing connection pragmas. This is the unfocused startup path.
    fn load_read_only_unvalidated(
        path: &Path,
        expected_volume: VolumeIdentityV2,
        expected_journal_id: u64,
    ) -> Result<(StoreIdentityV1, MftIndexV1), String> {
        if !path.is_file() {
            return Err("MFT SQLite store is unavailable".to_owned());
        }
        let connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|error| error.to_string())?;
        connection
            .busy_timeout(BUSY_TIMEOUT)
            .map_err(|error| error.to_string())?;
        verify_integrity(&connection)?;
        let identity = read_identity(&connection)?;
        if !identity.complete {
            return Err("MFT SQLite store is incomplete".to_owned());
        }
        if identity.volume != expected_volume || identity.cursor.journal_id != expected_journal_id {
            return Err("MFT SQLite volume or journal identity mismatch".to_owned());
        }
        validate_cursor(identity.cursor)?;
        let index = load_index_from_connection(&connection)?;
        Ok((identity, index))
    }

    fn load_read_only_bounded_unvalidated(
        path: &Path,
        expected_volume: VolumeIdentityV2,
        expected_journal_id: u64,
        volume_limit_bytes: usize,
        file_limit_bytes: usize,
    ) -> Result<(StoreIdentityV1, MftIndexV1, bool), String> {
        Self::load_read_only_bounded_unvalidated_cancelled(
            path,
            expected_volume,
            expected_journal_id,
            volume_limit_bytes,
            file_limit_bytes,
            never_cancelled,
        )
    }

    fn load_read_only_bounded_unvalidated_cancelled(
        path: &Path,
        expected_volume: VolumeIdentityV2,
        expected_journal_id: u64,
        volume_limit_bytes: usize,
        file_limit_bytes: usize,
        cancelled: fn() -> bool,
    ) -> Result<(StoreIdentityV1, MftIndexV1, bool), String> {
        if !path.is_file() {
            return Err("MFT SQLite store is unavailable".to_owned());
        }
        let connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|error| error.to_string())?;
        connection
            .busy_timeout(BUSY_TIMEOUT)
            .map_err(|error| error.to_string())?;
        connection.progress_handler(10_000, Some(move || cancelled()));
        verify_integrity(&connection)?;
        let identity = read_identity(&connection)?;
        if !identity.complete {
            return Err("MFT SQLite store is incomplete".to_owned());
        }
        if identity.volume != expected_volume || identity.cursor.journal_id != expected_journal_id {
            return Err("MFT SQLite volume or journal identity mismatch".to_owned());
        }
        validate_cursor(identity.cursor)?;
        let (index, complete) = load_index_from_connection_bounded(
            &connection,
            volume_limit_bytes,
            file_limit_bytes,
            cancelled,
        )?;
        Ok((identity, index, complete))
    }
    pub(crate) fn create(
        path: &Path,
        fixed_root: &Path,
        identity: StoreIdentityV1,
    ) -> Result<Self, String> {
        validate_store_path(path, fixed_root)?;
        if path.exists() {
            return Err("MFT SQLite store already exists".to_owned());
        }
        let connection = Connection::open(path).map_err(|error| error.to_string())?;
        configure(&connection)?;
        initialize_schema(&connection, identity)?;
        let mut store = Self {
            connection,
            path: path.to_path_buf(),
            identity,
            telemetry: StoreTelemetryV1::default(),
        };
        store.refresh_file_bytes();
        Ok(store)
    }

    pub(crate) fn open(
        path: &Path,
        fixed_root: &Path,
        expected_volume: VolumeIdentityV2,
        expected_journal_id: u64,
    ) -> Result<Self, String> {
        Self::open_expected_completeness(
            path,
            fixed_root,
            expected_volume,
            expected_journal_id,
            true,
        )
    }

    /// Reopens a just-promoted candidate and verifies the exact completeness
    /// bit that was written into the same SQLite transaction as its entries.
    /// Normal writer admission always calls `open`, which requires complete
    /// state; this path also supports deliberately partial persisted-budget
    /// candidates without ever admitting them as exact.
    fn open_expected_completeness(
        path: &Path,
        fixed_root: &Path,
        expected_volume: VolumeIdentityV2,
        expected_journal_id: u64,
        expected_complete: bool,
    ) -> Result<Self, String> {
        validate_store_path(path, fixed_root)?;
        if !path.is_file() {
            return Err("MFT SQLite store is unavailable".to_owned());
        }
        let connection = if expected_complete {
            let connection = Connection::open(path).map_err(|error| error.to_string())?;
            configure(&connection)?;
            connection
        } else {
            let connection = Connection::open_with_flags(
                path,
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )
            .map_err(|error| error.to_string())?;
            connection
                .busy_timeout(BUSY_TIMEOUT)
                .map_err(|error| error.to_string())?;
            connection
        };
        verify_integrity(&connection)?;
        let identity = read_identity(&connection)?;
        if identity.complete != expected_complete {
            return Err("MFT SQLite store completeness mismatch".to_owned());
        }
        if identity.volume != expected_volume || identity.cursor.journal_id != expected_journal_id {
            return Err("MFT SQLite volume or journal identity mismatch".to_owned());
        }
        validate_cursor(identity.cursor)?;
        let mut store = Self {
            connection,
            path: path.to_path_buf(),
            identity,
            telemetry: StoreTelemetryV1::default(),
        };
        store.refresh_file_bytes();
        Ok(store)
    }

    pub(crate) const fn identity(&self) -> StoreIdentityV1 {
        self.identity
    }

    pub(crate) fn telemetry(&mut self) -> StoreTelemetryV1 {
        self.refresh_file_bytes();
        self.telemetry
    }

    pub(crate) const fn telemetry_cached(&self) -> StoreTelemetryV1 {
        self.telemetry
    }

    pub(crate) fn commit_changes(
        &mut self,
        changes: &[MftChangeV2],
        next: JournalCursorV1,
        failure: CommitFailurePointV1,
    ) -> Result<(), String> {
        self.commit_changes_guarded(changes, next, failure, || true)
    }

    pub(crate) fn commit_changes_guarded(
        &mut self,
        changes: &[MftChangeV2],
        next: JournalCursorV1,
        failure: CommitFailurePointV1,
        lifecycle_open: impl Fn() -> bool,
    ) -> Result<(), String> {
        if next.journal_id != self.identity.cursor.journal_id
            || next.next_usn < self.identity.cursor.next_usn
            || next.generation <= self.identity.cursor.generation
        {
            return Err("MFT SQLite commit cursor is not contiguous".to_owned());
        }
        if !self.wal_allows(MAX_PENDING_BATCH_BYTES) {
            return Err("MFT SQLite WAL hard bound requires foreground maintenance".to_owned());
        }
        self.telemetry.transaction_attempts = self.telemetry.transaction_attempts.saturating_add(1);
        if !lifecycle_open() {
            self.telemetry.transaction_failures =
                self.telemetry.transaction_failures.saturating_add(1);
            self.telemetry.transaction_last_outcome = 2;
            return Err("MFT SQLite lifecycle closed before BEGIN".to_owned());
        }
        let result =
            commit_transaction(&mut self.connection, changes, next, failure, lifecycle_open);
        if result.is_err() {
            self.telemetry.transaction_failures =
                self.telemetry.transaction_failures.saturating_add(1);
            self.telemetry.transaction_last_outcome = 2;
        } else {
            self.telemetry.transaction_last_outcome = 1;
            self.identity.cursor = next;
        }
        self.refresh_file_bytes();
        result
    }

    pub(crate) fn commit_changes_linearized(
        &mut self,
        changes: &[MftChangeV2],
        next: JournalCursorV1,
        barrier: &LifecycleBarrierV1,
    ) -> Result<(), String> {
        self.commit_changes_focused_linearized(changes, next, barrier, || true)
    }

    pub(crate) fn commit_changes_focused_linearized(
        &mut self,
        changes: &[MftChangeV2],
        next: JournalCursorV1,
        barrier: &LifecycleBarrierV1,
        focused_now: impl Fn() -> bool,
    ) -> Result<(), String> {
        if next.journal_id != self.identity.cursor.journal_id
            || next.next_usn < self.identity.cursor.next_usn
            || next.generation <= self.identity.cursor.generation
        {
            return Err("MFT SQLite commit cursor is not contiguous".to_owned());
        }
        if !self.wal_allows(MAX_PENDING_BATCH_BYTES) {
            return Err("MFT SQLite WAL hard bound requires foreground maintenance".to_owned());
        }
        self.telemetry.transaction_attempts = self.telemetry.transaction_attempts.saturating_add(1);
        let result = barrier.invoke(|| {
            if !focused_now() {
                return Err("MFT SQLite focus lease expired before BEGIN".to_owned());
            }
            let transaction = self
                .connection
                .transaction()
                .map_err(|error| error.to_string())?;
            apply_changes(&transaction, changes)?;
            transaction
                .execute(
                    "UPDATE metadata SET next_usn=?1, generation=?2 WHERE singleton=1",
                    params![next.next_usn, encode_u64(next.generation)],
                )
                .map_err(|error| error.to_string())?;
            if !barrier.is_open() {
                return Err("MFT SQLite lifecycle closed before COMMIT".to_owned());
            }
            if !focused_now() {
                return Err("MFT SQLite focus lease expired before COMMIT".to_owned());
            }
            transaction.commit().map_err(|error| error.to_string())
        });
        if result.is_err() {
            self.telemetry.transaction_failures =
                self.telemetry.transaction_failures.saturating_add(1);
            self.telemetry.transaction_last_outcome = 2;
        } else {
            self.telemetry.transaction_last_outcome = 1;
            self.identity.cursor = next;
        }
        self.refresh_file_bytes();
        result
    }

    pub(crate) fn entry_count(&self) -> Result<u64, String> {
        self.connection
            .query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))
            .map_err(|error| error.to_string())
    }

    pub(crate) fn load_index(&self) -> Result<MftIndexV1, String> {
        load_index_from_connection(&self.connection)
    }

    pub(crate) fn canonical_members(path: &Path) -> [PathBuf; 3] {
        [
            path.to_path_buf(),
            wal_path(path),
            PathBuf::from(format!("{}-shm", path.display())),
        ]
    }

    pub(crate) fn prune_persisted_store_focused_linearized(
        canonical: &Path,
        fixed_root: &Path,
        incomplete_marker: &Path,
        identity: StoreIdentityV1,
        index: &MftIndexV1,
        max_candidate_bytes: u64,
        barrier: &LifecycleBarrierV1,
        focused_now: impl Fn() -> bool + Sync,
    ) -> Result<Self, String> {
        validate_store_path(canonical, fixed_root)?;
        if incomplete_marker.parent() != Some(fixed_root)
            || incomplete_marker
                .extension()
                .and_then(|value| value.to_str())
                != Some("persisted-partial")
        {
            return Err("MFT persisted eviction marker escapes the fixed root".to_owned());
        }
        let temporary_marker = incomplete_marker.with_extension("persisted-partial.tmp");
        if !incomplete_marker.exists() {
            barrier.invoke(|| {
                if !focused_now() {
                    return Err("MFT focus lease expired before persisted eviction".to_owned());
                }
                if temporary_marker.exists() {
                    std::fs::remove_file(&temporary_marker).map_err(|error| error.to_string())?;
                }
                use std::io::Write as _;
                let mut marker = std::fs::OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(&temporary_marker)
                    .map_err(|error| error.to_string())?;
                marker
                    .write_all(b"SEMFTPARTIAL2\nreason=persisted-budget\n")
                    .map_err(|error| error.to_string())?;
                marker.sync_all().map_err(|error| error.to_string())
            })?;
            barrier.invoke(|| {
                if !focused_now() {
                    return Err(
                        "MFT focus lease expired before persisted eviction intent".to_owned()
                    );
                }
                std::fs::rename(&temporary_marker, incomplete_marker)
                    .map_err(|error| error.to_string())
            })?;
        }
        let temporary = fixed_root.join(format!(
            "{}.migration-tmp",
            canonical
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| "MFT SQLite canonical filename is invalid".to_owned())?
        ));
        Self::snapshot_focused_bounded_linearized(
            &temporary,
            canonical,
            fixed_root,
            identity,
            index,
            true,
            max_candidate_bytes,
            barrier,
            focused_now,
        )
    }
}

fn validate_migration_temp_path(
    temporary: &Path,
    canonical: &Path,
    fixed_root: &Path,
) -> Result<(), String> {
    let root = fixed_root
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let parent = temporary
        .parent()
        .ok_or_else(|| "migration temporary has no parent".to_owned())?
        .canonicalize()
        .map_err(|error| error.to_string())?;
    if parent != root || temporary == canonical {
        return Err("migration temporary escapes the fixed cache root".to_owned());
    }
    let expected_prefix = canonical
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "canonical MFT SQLite filename is invalid".to_owned())?;
    let name = temporary
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "migration temporary filename is invalid".to_owned())?;
    if (!cfg!(test) && name != format!("{expected_prefix}.migration-tmp"))
        || (cfg!(test) && (!name.starts_with(expected_prefix) || !name.ends_with(".migration-tmp")))
    {
        return Err("migration temporary filename is not recognized".to_owned());
    }
    Ok(())
}

fn install_migration_entries_guarded(
    connection: &Connection,
    index: &MftIndexV1,
    lifecycle_open: &impl Fn() -> bool,
) -> Result<(), String> {
    let transaction = connection
        .unchecked_transaction()
        .map_err(|error| error.to_string())?;
    {
        let mut insert = transaction
            .prepare_cached(
                "INSERT INTO entries
             (reference, parent_reference, name, kind, logical_bytes, allocated_bytes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )
            .map_err(|error| error.to_string())?;
        for entry in index.entries.values() {
            if !lifecycle_open() {
                return Err("MFT SQLite lifecycle closed during migration build".to_owned());
            }
            insert
                .execute(params![
                    encode_u64(entry.reference),
                    encode_u64(entry.parent_reference),
                    entry.name,
                    i64::from(entry.is_directory),
                    encode_u64(entry.logical_bytes),
                    encode_u64(entry.allocated_bytes)
                ])
                .map_err(|error| error.to_string())?;
        }
    }
    if !lifecycle_open() {
        return Err("MFT SQLite lifecycle closed before migration COMMIT".to_owned());
    }
    transaction.commit().map_err(|error| error.to_string())
}

fn sync_file(path: &Path) -> Result<(), String> {
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .and_then(|file| file.sync_all())
        .map_err(|error| error.to_string())
}

fn run_cancellable_vacuum_into(
    connection: &Connection,
    destination: &Path,
    lifecycle_open: &(impl Fn() -> bool + Sync),
) -> Result<(), String> {
    use std::sync::atomic::{AtomicBool, Ordering};

    if !lifecycle_open() {
        return Err("MFT SQLite lifecycle or focus gate closed before safety copy".to_owned());
    }
    let finished = AtomicBool::new(false);
    let interrupt = connection.get_interrupt_handle();
    let result = std::thread::scope(|scope| {
        let monitor = scope.spawn(|| {
            while !finished.load(Ordering::Acquire) {
                if !lifecycle_open() {
                    interrupt.interrupt();
                    break;
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        });
        let result = connection
            .execute(
                "VACUUM INTO ?1",
                params![destination.to_string_lossy().as_ref()],
            )
            .map_err(|error| error.to_string());
        finished.store(true, Ordering::Release);
        let _ = monitor.join();
        result
    });
    if !lifecycle_open() {
        return Err("MFT SQLite safety copy cancelled by lifecycle or focus gate".to_owned());
    }
    result.map(|_| ())
}

#[cfg(windows)]
#[expect(
    unsafe_code,
    reason = "atomic SQLite store replacement with backup requires Win32 ReplaceFileW"
)]
// SAFETY: The declaration matches kernel32; source, destination, and optional
// backup buffers remain NUL-terminated and live for the synchronous call.
fn atomic_replace_file(
    source: &Path,
    destination: &Path,
    backup: Option<&Path>,
) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt as _;
    unsafe extern "system" {
        fn ReplaceFileW(
            replaced: *const u16,
            replacement: *const u16,
            backup: *const u16,
            flags: u32,
            exclude: *mut std::ffi::c_void,
            reserved: *mut std::ffi::c_void,
        ) -> i32;
    }
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let backup = backup.map(|path| {
        path.as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>()
    });
    let backup_ptr = backup
        .as_ref()
        .map_or(std::ptr::null(), |path| path.as_ptr());
    if unsafe {
        ReplaceFileW(
            destination.as_ptr(),
            source.as_ptr(),
            backup_ptr,
            0x2,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(())
}

#[cfg(not(windows))]
fn atomic_replace_file(
    source: &Path,
    destination: &Path,
    backup: Option<&Path>,
) -> Result<(), String> {
    if let Some(backup) = backup {
        std::fs::rename(destination, backup).map_err(|error| error.to_string())?;
    }
    std::fs::rename(source, destination).map_err(|error| error.to_string())
}

fn load_index_from_connection(connection: &Connection) -> Result<MftIndexV1, String> {
    connection.set_limit(Limit::SQLITE_LIMIT_LENGTH, SQLITE_LENGTH_LIMIT_BYTES);
    let mut statement = connection
        .prepare(
            "SELECT reference, parent_reference, name, length(CAST(name AS BLOB)), kind,
                    logical_bytes, allocated_bytes
                 FROM entries ORDER BY reference",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            let name_bytes = row.get::<_, i64>(3)?;
            if !(0..=MAX_ENTRY_NAME_BYTES as i64).contains(&name_bytes) {
                return Err(rusqlite::Error::FromSqlConversionFailure(
                    2,
                    rusqlite::types::Type::Text,
                    "MFT SQLite entry name exceeds the admission bound".into(),
                ));
            }
            Ok(MftEntryV1 {
                reference: decode_u64(row.get(0)?),
                parent_reference: decode_u64(row.get(1)?),
                name: row.get(2)?,
                is_directory: row.get::<_, i64>(4)? != 0,
                logical_bytes: decode_u64(row.get(5)?),
                allocated_bytes: decode_u64(row.get(6)?),
            })
        })
        .map_err(|error| error.to_string())?;
    let mut entries = std::collections::BTreeMap::new();
    let mut decoded_bytes = 0_usize;
    for row in rows {
        let entry = row.map_err(|error| error.to_string())?;
        if entries.len() >= MAX_INDEX_ENTRIES {
            return Err("MFT SQLite entry count exceeds the admission bound".to_owned());
        }
        if entry.name.len() > MAX_ENTRY_NAME_BYTES {
            return Err("MFT SQLite entry name exceeds the admission bound".to_owned());
        }
        decoded_bytes = decoded_bytes
            .checked_add(49_usize.saturating_add(entry.name.len()))
            .ok_or_else(|| "MFT SQLite decoded size overflow".to_owned())?;
        if decoded_bytes > MAX_INDEX_DECODED_BYTES {
            return Err("MFT SQLite decoded size exceeds the admission bound".to_owned());
        }
        entries.insert(entry.reference, entry);
    }
    MftIndexV1::try_from_entries(entries)
}

fn load_index_from_connection_bounded(
    connection: &Connection,
    volume_limit_bytes: usize,
    file_limit_bytes: usize,
    cancelled: fn() -> bool,
) -> Result<(MftIndexV1, bool), String> {
    connection.set_limit(Limit::SQLITE_LIMIT_LENGTH, SQLITE_LENGTH_LIMIT_BYTES);
    let mut statement = connection
        .prepare(
            "SELECT reference, parent_reference, name, length(CAST(name AS BLOB)), kind,
                    logical_bytes, allocated_bytes
                 FROM entries ORDER BY reference",
        )
        .map_err(|error| error.to_string())?;
    let mut rows = statement.query([]).map_err(|error| error.to_string())?;
    let maximum_entries =
        crate::mft_size_map::maximum_entries_for_volume_budget(volume_limit_bytes);
    let mut entries = std::collections::BTreeMap::new();
    let mut file_bytes = 0_usize;
    let mut volume_complete = true;
    let mut file_complete = true;
    while let Some(row) = rows.next().map_err(|error| error.to_string())? {
        if cancelled() {
            return Err("MFT SQLite bounded load cancelled".to_owned());
        }
        if entries.len() >= maximum_entries {
            volume_complete = false;
            break;
        }
        let name_bytes = row.get::<_, i64>(3).map_err(|error| error.to_string())?;
        if !(0..=MAX_ENTRY_NAME_BYTES as i64).contains(&name_bytes) {
            return Err("MFT SQLite entry name exceeds the admission bound".to_owned());
        }
        let name_bytes = usize::try_from(name_bytes)
            .map_err(|_| "MFT SQLite entry name length is invalid".to_owned())?;
        let name = if file_bytes.saturating_add(name_bytes) <= file_limit_bytes {
            let name = row.get::<_, String>(2).map_err(|error| error.to_string())?;
            file_bytes = file_bytes.saturating_add(name.capacity());
            name
        } else {
            file_complete = false;
            String::new()
        };
        entries.insert(
            decode_u64(row.get(0).map_err(|error| error.to_string())?),
            MftEntryV1 {
                reference: decode_u64(row.get(0).map_err(|error| error.to_string())?),
                parent_reference: decode_u64(row.get(1).map_err(|error| error.to_string())?),
                name,
                is_directory: row.get::<_, i64>(4).map_err(|error| error.to_string())? != 0,
                logical_bytes: decode_u64(row.get(5).map_err(|error| error.to_string())?),
                allocated_bytes: decode_u64(row.get(6).map_err(|error| error.to_string())?),
            },
        );
    }
    let mut index = MftIndexV1::try_from_entries_cancelled(entries, cancelled)?;
    let memory = index.memory_breakdown();
    if memory.volume_index_bytes > volume_limit_bytes {
        volume_complete = false;
        index.trim_volume_index_to_bytes(volume_limit_bytes);
    }
    if memory.file_data_bytes > file_limit_bytes {
        file_complete = false;
        index.trim_file_data_to_bytes(file_limit_bytes);
    }
    Ok((index, volume_complete && file_complete))
}

fn never_cancelled() -> bool {
    false
}

impl MftSqliteStoreV1 {
    /// Installs a complete in-memory snapshot as the durable base.  The
    /// completeness bit and cursor are promoted in the same transaction as
    /// the rows, so an interrupted initial build is never admitted on restart.
    pub(crate) fn install_snapshot(
        &mut self,
        index: &MftIndexV1,
        cursor: JournalCursorV1,
    ) -> Result<(), String> {
        if self.identity.complete {
            return Err("MFT SQLite snapshot is already complete".to_owned());
        }
        if cursor.journal_id != self.identity.cursor.journal_id
            || cursor.next_usn < self.identity.cursor.next_usn
            || cursor.generation <= self.identity.cursor.generation
        {
            return Err("MFT SQLite snapshot cursor is not contiguous".to_owned());
        }
        self.telemetry.transaction_attempts = self.telemetry.transaction_attempts.saturating_add(1);
        let result = install_snapshot_transaction(&mut self.connection, index, cursor);
        if result.is_err() {
            self.telemetry.transaction_failures =
                self.telemetry.transaction_failures.saturating_add(1);
            self.telemetry.transaction_last_outcome = 2;
        } else {
            self.telemetry.transaction_last_outcome = 1;
            self.identity.cursor = cursor;
            self.identity.complete = true;
        }
        self.refresh_file_bytes();
        result
    }

    pub(crate) fn wal_checkpoint_eligible(
        &mut self,
        focused: bool,
        conflicting_work: bool,
    ) -> bool {
        self.refresh_file_bytes();
        checkpoint_eligible(self.telemetry.wal_bytes, focused, conflicting_work)
    }

    pub(crate) fn truncate_wal(
        &mut self,
        focused: bool,
        conflicting_work: bool,
    ) -> Result<bool, String> {
        self.truncate_wal_guarded(focused, conflicting_work, || true)
    }

    pub(crate) fn truncate_wal_guarded(
        &mut self,
        focused: bool,
        conflicting_work: bool,
        lifecycle_open: impl Fn() -> bool,
    ) -> Result<bool, String> {
        if !self.wal_checkpoint_eligible(focused, conflicting_work) {
            return Ok(false);
        }
        if !lifecycle_open() {
            return Err("MFT SQLite lifecycle closed before checkpoint".to_owned());
        }
        self.telemetry.checkpoint_attempts = self.telemetry.checkpoint_attempts.saturating_add(1);
        let result = run_truncate_checkpoint(&self.connection);
        if result.is_err() {
            self.telemetry.checkpoint_failures =
                self.telemetry.checkpoint_failures.saturating_add(1);
            self.telemetry.checkpoint_last_outcome = 2;
        } else {
            self.telemetry.checkpoint_last_outcome = 1;
        }
        self.refresh_file_bytes();
        result.map(|()| true)
    }

    pub(crate) fn truncate_wal_linearized(
        &mut self,
        focused: bool,
        conflicting_work: bool,
        barrier: &LifecycleBarrierV1,
    ) -> Result<bool, String> {
        self.truncate_wal_focused_linearized(conflicting_work, barrier, || focused)
    }

    pub(crate) fn truncate_wal_focused_linearized(
        &mut self,
        conflicting_work: bool,
        barrier: &LifecycleBarrierV1,
        focused_now: impl Fn() -> bool,
    ) -> Result<bool, String> {
        self.truncate_wal_ready_linearized(barrier, focused_now, || conflicting_work)
    }

    pub(crate) fn truncate_wal_ready_linearized(
        &mut self,
        barrier: &LifecycleBarrierV1,
        focused_now: impl Fn() -> bool,
        conflicting_work_now: impl Fn() -> bool,
    ) -> Result<bool, String> {
        self.refresh_file_bytes();
        if self.telemetry.wal_bytes <= WAL_MAINTENANCE_THRESHOLD_BYTES || conflicting_work_now() {
            return Ok(false);
        }
        self.telemetry.checkpoint_attempts = self.telemetry.checkpoint_attempts.saturating_add(1);
        let result = barrier.invoke(|| {
            if !focused_now() {
                return Err("MFT SQLite focus lease expired before checkpoint".to_owned());
            }
            if conflicting_work_now() {
                return Err("MFT SQLite query-critical work began before checkpoint".to_owned());
            }
            run_truncate_checkpoint(&self.connection)
        });
        if result.is_err() {
            self.telemetry.checkpoint_failures =
                self.telemetry.checkpoint_failures.saturating_add(1);
            self.telemetry.checkpoint_last_outcome = 2;
        } else {
            self.telemetry.checkpoint_last_outcome = 1;
        }
        self.refresh_file_bytes();
        result.map(|()| true)
    }

    pub(crate) fn wal_allows(&mut self, incoming_encoded_bytes: u64) -> bool {
        self.refresh_file_bytes();
        wal_admission(self.telemetry.wal_bytes, incoming_encoded_bytes)
    }

    fn refresh_file_bytes(&mut self) {
        self.telemetry.main_bytes = file_bytes(&self.path);
        self.telemetry.wal_bytes = file_bytes(&wal_path(&self.path));
    }
}

fn run_truncate_checkpoint(connection: &Connection) -> Result<(), String> {
    let (busy, _log_frames, _checkpointed): (i64, i64, i64) = connection
        .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .map_err(|error| error.to_string())?;
    if busy != 0 {
        return Err("MFT SQLite truncate checkpoint is busy".to_owned());
    }
    Ok(())
}

fn install_snapshot_transaction(
    connection: &mut Connection,
    index: &MftIndexV1,
    cursor: JournalCursorV1,
) -> Result<(), String> {
    let transaction = connection
        .transaction()
        .map_err(|error| error.to_string())?;
    {
        let mut insert = transaction
            .prepare_cached(
                "INSERT INTO entries
                 (reference, parent_reference, name, kind, logical_bytes, allocated_bytes)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )
            .map_err(|error| error.to_string())?;
        for entry in index.entries.values() {
            insert
                .execute(params![
                    encode_u64(entry.reference),
                    encode_u64(entry.parent_reference),
                    entry.name,
                    i64::from(entry.is_directory),
                    encode_u64(entry.logical_bytes),
                    encode_u64(entry.allocated_bytes),
                ])
                .map_err(|error| error.to_string())?;
        }
    }
    transaction
        .execute(
            "UPDATE metadata
             SET next_usn=?1, generation=?2, complete=1
             WHERE singleton=1 AND complete=0",
            params![cursor.next_usn, encode_u64(cursor.generation)],
        )
        .map_err(|error| error.to_string())?;
    transaction.commit().map_err(|error| error.to_string())
}

fn configure(connection: &Connection) -> Result<(), String> {
    connection.set_limit(Limit::SQLITE_LIMIT_LENGTH, SQLITE_LENGTH_LIMIT_BYTES);
    configure_page_size(connection)?;
    connection
        .busy_timeout(BUSY_TIMEOUT)
        .map_err(|error| error.to_string())?;
    let mode: String = connection
        .pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get(0))
        .map_err(|error| error.to_string())?;
    if !mode.eq_ignore_ascii_case("wal") {
        return Err("MFT SQLite refused WAL mode".to_owned());
    }
    connection
        .pragma_update(None, "synchronous", "NORMAL")
        .map_err(|error| error.to_string())?;
    connection
        .pragma_update(None, "wal_autocheckpoint", 0_i64)
        .map_err(|error| error.to_string())?;
    connection
        .set_db_config(DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE, true)
        .map_err(|error| error.to_string())?;
    if !connection
        .db_config(DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE)
        .map_err(|error| error.to_string())?
    {
        return Err("MFT SQLite no-checkpoint-on-close is disabled".to_owned());
    }
    enable_persistent_wal_files(connection)?;
    Ok(())
}

#[expect(
    unsafe_code,
    reason = "enabling persistent WAL requires SQLite's raw file-control interface"
)]
fn enable_persistent_wal_files(connection: &Connection) -> Result<(), String> {
    let mut enabled = 1_i32;
    // SAFETY: the connection is open for the duration of the call, `main` is a
    // terminated static database name, and SQLite expects a writable int.
    let result = unsafe {
        rusqlite::ffi::sqlite3_file_control(
            connection.handle(),
            c"main".as_ptr(),
            rusqlite::ffi::SQLITE_FCNTL_PERSIST_WAL,
            (&raw mut enabled).cast(),
        )
    };
    if result != rusqlite::ffi::SQLITE_OK {
        return Err(format!("MFT SQLite persistent WAL setup failed ({result})"));
    }
    Ok(())
}

const fn checkpoint_eligible(wal_bytes: u64, focused: bool, conflicting_work: bool) -> bool {
    focused && !conflicting_work && wal_bytes > WAL_MAINTENANCE_THRESHOLD_BYTES
}

const fn wal_admission(current_wal_bytes: u64, incoming_encoded_bytes: u64) -> bool {
    let bounded_incoming = if incoming_encoded_bytes > MAX_PENDING_BATCH_BYTES {
        MAX_PENDING_BATCH_BYTES
    } else {
        incoming_encoded_bytes
    };
    let worst_case_growth = maximum_wal_batch_growth_bytes(bounded_incoming);
    let hard_bound = WAL_MAINTENANCE_THRESHOLD_BYTES.saturating_add(worst_case_growth);
    current_wal_bytes.saturating_add(worst_case_growth) <= hard_bound
}

pub(crate) const fn maximum_wal_batch_growth_bytes(incoming_encoded_bytes: u64) -> u64 {
    let bounded_incoming = if incoming_encoded_bytes > MAX_PENDING_BATCH_BYTES {
        MAX_PENDING_BATCH_BYTES
    } else {
        incoming_encoded_bytes
    };
    let payload_growth = bounded_incoming
        .saturating_mul(WAL_FRAME_OVERHEAD_MULTIPLIER)
        .saturating_add(1024 * 1024);
    if payload_growth > MAX_SCATTERED_WAL_GROWTH_BYTES {
        payload_growth
    } else {
        MAX_SCATTERED_WAL_GROWTH_BYTES
    }
}

fn initialize_schema(connection: &Connection, identity: StoreIdentityV1) -> Result<(), String> {
    validate_cursor(identity.cursor)?;
    connection
        .execute_batch(
            "BEGIN IMMEDIATE;
             CREATE TABLE metadata (
                 singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
                 schema_version INTEGER NOT NULL,
                 volume_serial INTEGER NOT NULL,
                 journal_id INTEGER NOT NULL,
                 next_usn INTEGER NOT NULL,
                 generation INTEGER NOT NULL,
                 complete INTEGER NOT NULL CHECK (complete IN (0, 1))
             );
             CREATE TABLE entries (
                 reference INTEGER PRIMARY KEY,
                 parent_reference INTEGER NOT NULL,
                 name TEXT NOT NULL,
                 kind INTEGER NOT NULL CHECK (kind IN (0, 1)),
                 logical_bytes INTEGER NOT NULL,
                 allocated_bytes INTEGER NOT NULL
             );
             CREATE INDEX entries_parent ON entries(parent_reference);
             COMMIT;",
        )
        .map_err(|error| error.to_string())?;
    connection
        .execute(
            "INSERT INTO metadata
             (singleton, schema_version, volume_serial, journal_id, next_usn, generation, complete)
             VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                SCHEMA_VERSION,
                encode_u64(identity.volume.serial),
                encode_u64(identity.cursor.journal_id),
                identity.cursor.next_usn,
                encode_u64(identity.cursor.generation),
                i64::from(identity.complete),
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn commit_transaction(
    connection: &mut Connection,
    changes: &[MftChangeV2],
    next: JournalCursorV1,
    failure: CommitFailurePointV1,
    lifecycle_open: impl Fn() -> bool,
) -> Result<(), String> {
    let transaction = connection
        .transaction()
        .map_err(|error| error.to_string())?;
    if failure == CommitFailurePointV1::BeforeMutation {
        return Err("injected MFT SQLite failure before mutation".to_owned());
    }
    apply_changes(&transaction, changes)?;
    if failure == CommitFailurePointV1::BeforeCursor {
        return Err("injected MFT SQLite failure before cursor".to_owned());
    }
    transaction
        .execute(
            "UPDATE metadata SET next_usn=?1, generation=?2 WHERE singleton=1",
            params![next.next_usn, encode_u64(next.generation)],
        )
        .map_err(|error| error.to_string())?;
    if failure == CommitFailurePointV1::BeforeCommit {
        return Err("injected MFT SQLite failure before commit".to_owned());
    }
    if !lifecycle_open() {
        return Err("MFT SQLite lifecycle closed before COMMIT invocation".to_owned());
    }
    // The call below is the linearization boundary. Once invoked, shutdown may
    // wait for SQLite to finish but cannot turn this transaction into a later
    // durability operation.
    transaction.commit().map_err(|error| error.to_string())
}

fn apply_changes(transaction: &Transaction<'_>, changes: &[MftChangeV2]) -> Result<(), String> {
    for change in changes {
        match change.kind {
            MftChangeKindV2::Upsert => {
                transaction
                    .execute(
                        "INSERT INTO entries
                         (reference, parent_reference, name, kind, logical_bytes, allocated_bytes)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                         ON CONFLICT(reference) DO UPDATE SET
                           parent_reference=excluded.parent_reference,
                           name=excluded.name,
                           kind=excluded.kind,
                           logical_bytes=excluded.logical_bytes,
                           allocated_bytes=excluded.allocated_bytes",
                        params![
                            encode_u64(change.reference),
                            encode_u64(change.parent_reference),
                            change.name,
                            i64::from(change.is_directory),
                            encode_u64(change.logical_bytes),
                            encode_u64(change.allocated_bytes),
                        ],
                    )
                    .map_err(|error| error.to_string())?;
            }
            MftChangeKindV2::Delete => {
                transaction
                    .execute(
                        "DELETE FROM entries WHERE reference=?1",
                        [encode_u64(change.reference)],
                    )
                    .map_err(|error| error.to_string())?;
            }
            MftChangeKindV2::Invalidate => {
                return Err("invalidating MFT change cannot be persisted as exact".to_owned());
            }
        }
    }
    Ok(())
}

fn read_identity(connection: &Connection) -> Result<StoreIdentityV1, String> {
    connection
        .query_row(
            "SELECT schema_version, volume_serial, journal_id, next_usn, generation, complete
             FROM metadata WHERE singleton=1",
            [],
            |row| {
                let schema: i64 = row.get(0)?;
                if schema != SCHEMA_VERSION {
                    return Err(rusqlite::Error::InvalidQuery);
                }
                Ok(StoreIdentityV1 {
                    volume: VolumeIdentityV2 {
                        serial: decode_u64(row.get(1)?),
                    },
                    cursor: JournalCursorV1 {
                        journal_id: decode_u64(row.get(2)?),
                        next_usn: row.get(3)?,
                        generation: decode_u64(row.get(4)?),
                    },
                    complete: row.get::<_, i64>(5)? != 0,
                })
            },
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "MFT SQLite metadata is unavailable".to_owned())
}

fn verify_integrity(connection: &Connection) -> Result<(), String> {
    verify_page_size(connection)?;
    let result: String = connection
        .query_row("PRAGMA quick_check(1)", [], |row| row.get(0))
        .map_err(|error| error.to_string())?;
    if result != "ok" {
        return Err(format!("MFT SQLite integrity check failed: {result}"));
    }
    Ok(())
}

fn configure_page_size(connection: &Connection) -> Result<(), String> {
    connection
        .pragma_update(None, "page_size", SQLITE_PAGE_BYTES)
        .map_err(|error| error.to_string())?;
    verify_page_size(connection)
}

fn verify_page_size(connection: &Connection) -> Result<(), String> {
    let page_size: u64 = connection
        .pragma_query_value(None, "page_size", |row| row.get(0))
        .map_err(|error| error.to_string())?;
    if page_size != SQLITE_PAGE_BYTES {
        return Err(format!(
            "MFT SQLite page size {page_size} is incompatible with the WAL admission bound"
        ));
    }
    Ok(())
}

fn validate_cursor(cursor: JournalCursorV1) -> Result<(), String> {
    if cursor.journal_id == 0 || cursor.next_usn < 0 {
        return Err("MFT SQLite cursor is invalid".to_owned());
    }
    Ok(())
}

const fn encode_u64(value: u64) -> i64 {
    i64::from_ne_bytes(value.to_ne_bytes())
}

const fn decode_u64(value: i64) -> u64 {
    u64::from_ne_bytes(value.to_ne_bytes())
}

fn validate_store_path(path: &Path, fixed_root: &Path) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "MFT SQLite path has no parent".to_owned())?;
    let root = fixed_root
        .canonicalize()
        .map_err(|error| format!("MFT SQLite fixed root is unavailable: {error}"))?;
    let parent = parent
        .canonicalize()
        .map_err(|error| format!("MFT SQLite parent is unavailable: {error}"))?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt as _;
        if fixed_root
            .symlink_metadata()
            .map_err(|error| error.to_string())?
            .file_attributes()
            & 0x400
            != 0
        {
            return Err("MFT SQLite fixed root must not be a reparse point".to_owned());
        }
        if path.exists()
            && path
                .symlink_metadata()
                .map_err(|error| error.to_string())?
                .file_attributes()
                & 0x400
                != 0
        {
            return Err("MFT SQLite store must not be a reparse point".to_owned());
        }
    }
    let name = path.file_name().and_then(|value| value.to_str());
    let canonical_name = name.is_some_and(|name| {
        let bytes = name.as_bytes();
        bytes.len() == 13 && bytes[0].is_ascii_uppercase() && bytes[1..] == *b".mft.sqlite3"
    });
    if parent != root || (!cfg!(test) && !canonical_name) {
        return Err("MFT SQLite path is outside the fixed cache root".to_owned());
    }
    Ok(())
}

fn validate_replacement_backup_path(
    backup: &Path,
    canonical: &Path,
    fixed_root: &Path,
) -> Result<(), String> {
    validate_store_path(canonical, fixed_root)?;
    if backup != MftSqliteStoreV1::replacement_backup_path(canonical)
        || backup.parent() != canonical.parent()
    {
        return Err("MFT SQLite replacement backup path is invalid".to_owned());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt as _;
        if backup.exists()
            && backup
                .symlink_metadata()
                .map_err(|error| error.to_string())?
                .file_attributes()
                & 0x400
                != 0
        {
            return Err("MFT SQLite replacement backup must not be a reparse point".to_owned());
        }
    }
    Ok(())
}

fn wal_path(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}-wal", path.display()))
}

fn file_bytes(path: &Path) -> u64 {
    std::fs::metadata(path).map_or(0, |metadata| metadata.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mft_size_map::MftEntryV1;
    use std::cell::Cell;
    use std::collections::BTreeMap;
    use tempfile::TempDir;

    fn identity() -> StoreIdentityV1 {
        StoreIdentityV1 {
            volume: VolumeIdentityV2 { serial: 7 },
            cursor: JournalCursorV1 {
                journal_id: 11,
                next_usn: 100,
                generation: 0,
            },
            complete: true,
        }
    }

    fn change(reference: u64) -> MftChangeV2 {
        MftChangeV2 {
            kind: MftChangeKindV2::Upsert,
            reference,
            parent_reference: 5,
            name: format!("entry-{reference}"),
            logical_bytes: 8,
            allocated_bytes: 4096,
            is_directory: false,
            reason: 1,
        }
    }

    fn next(generation: u64) -> JournalCursorV1 {
        JournalCursorV1 {
            journal_id: 11,
            next_usn: 100 + generation as i64,
            generation,
        }
    }

    #[test]
    fn schema_round_trip_and_fixed_root_admission() {
        let root = TempDir::new().unwrap();
        let path = root.path().join("C.mft.sqlite3");
        let store = MftSqliteStoreV1::create(&path, root.path(), identity()).unwrap();
        assert_eq!(store.identity(), identity());
        drop(store);
        let reopened = MftSqliteStoreV1::open(
            &path,
            root.path(),
            identity().volume,
            identity().cursor.journal_id,
        )
        .unwrap();
        assert_eq!(reopened.identity(), identity());
        assert!(
            MftSqliteStoreV1::open(
                &path,
                &root.path().join("other"),
                identity().volume,
                identity().cursor.journal_id,
            )
            .is_err()
        );
    }

    #[test]
    fn read_only_folder_aggregate_is_exact_and_preserves_fixed_file_set() {
        let root = TempDir::new().unwrap();
        let path = root.path().join("C.mft.sqlite3");
        let mut store = MftSqliteStoreV1::create(&path, root.path(), identity()).unwrap();
        let entry = |reference, parent_reference, logical_bytes, is_directory| MftChangeV2 {
            kind: MftChangeKindV2::Upsert,
            reference,
            parent_reference,
            name: format!("entry-{reference}"),
            logical_bytes,
            allocated_bytes: logical_bytes,
            is_directory,
            reason: 1,
        };
        store
            .commit_changes(
                &[
                    entry(5, 5, 0, true),
                    entry(10, 5, 0, true),
                    entry(11, 10, 20, false),
                    entry(12, 5, 7, false),
                ],
                next(1),
                CommitFailurePointV1::None,
            )
            .unwrap();
        let before = MftSqliteStoreV1::canonical_members(&path)
            .into_iter()
            .filter(|member| member.exists())
            .map(|member| (member.clone(), std::fs::metadata(member).unwrap().len()))
            .collect::<Vec<_>>();

        let aggregate = MftSqliteStoreV1::query_folder_aggregate_read_only(
            &path,
            root.path(),
            identity().volume,
            next(1),
            10,
            &HashSet::new(),
        )
        .unwrap();
        assert_eq!(aggregate.logical_bytes, 20);
        assert_eq!(aggregate.allocated_bytes, 20);
        assert_eq!(aggregate.file_count, 1);
        assert_eq!(aggregate.directory_count, 1);
        assert!(
            MftSqliteStoreV1::query_folder_aggregate_read_only(
                &path,
                root.path(),
                identity().volume,
                next(1),
                10,
                &HashSet::from([11]),
            )
            .is_err(),
            "a changed descendant invalidates the durable aggregate"
        );
        assert!(
            MftSqliteStoreV1::query_folder_aggregate_read_only(
                &path,
                root.path(),
                identity().volume,
                next(1),
                10,
                &HashSet::from([12]),
            )
            .is_ok(),
            "an unrelated changed reference does not invalidate the subtree"
        );
        assert!(
            MftSqliteStoreV1::query_folder_aggregate_read_only(
                &path,
                root.path(),
                identity().volume,
                identity().cursor,
                10,
                &HashSet::new(),
            )
            .is_err(),
            "a stale cursor must never be reported as exact"
        );

        let after = MftSqliteStoreV1::canonical_members(&path)
            .into_iter()
            .filter(|member| member.exists())
            .map(|member| (member.clone(), std::fs::metadata(member).unwrap().len()))
            .collect::<Vec<_>>();
        assert_eq!(
            after, before,
            "read-only aggregate must not mutate the store"
        );
    }

    #[test]
    fn persisted_budget_prune_is_focused_atomic_and_leaves_typed_marker() {
        let root = TempDir::new().unwrap();
        let path = root.path().join("C.mft.sqlite3");
        drop(MftSqliteStoreV1::create(&path, root.path(), identity()).unwrap());
        let marker = root.path().join("C.persisted-partial");
        let barrier = LifecycleBarrierV1::new();
        let mut partial_identity = identity();
        partial_identity.complete = false;
        let index = MftIndexV1::from_entries(BTreeMap::from([(
            5,
            MftEntryV1 {
                reference: 5,
                parent_reference: 5,
                name: "known-partial-root".into(),
                logical_bytes: 0,
                allocated_bytes: 0,
                is_directory: true,
            },
        )]));

        assert!(
            MftSqliteStoreV1::prune_persisted_store_focused_linearized(
                &path,
                root.path(),
                &marker,
                partial_identity,
                &index,
                8 * 1024 * 1024,
                &barrier,
                || false,
            )
            .is_err()
        );
        assert!(path.exists());
        assert!(!marker.exists());

        let focus_checks = std::sync::atomic::AtomicU8::new(0);
        assert!(
            MftSqliteStoreV1::prune_persisted_store_focused_linearized(
                &path,
                root.path(),
                &marker,
                partial_identity,
                &index,
                8 * 1024 * 1024,
                &barrier,
                || {
                    let check = focus_checks.fetch_add(1, std::sync::atomic::Ordering::AcqRel) + 1;
                    check <= 2
                },
            )
            .is_err()
        );
        assert!(marker.exists());
        assert!(
            path.exists(),
            "focus loss before delete preserves the store"
        );

        let pruned = MftSqliteStoreV1::prune_persisted_store_focused_linearized(
            &path,
            root.path(),
            &marker,
            partial_identity,
            &index,
            8 * 1024 * 1024,
            &barrier,
            || true,
        )
        .unwrap();
        assert!(marker.exists());
        assert!(path.exists());
        assert!(!pruned.identity().complete);
        assert_eq!(pruned.entry_count().unwrap(), 1);
        assert!(
            MftSqliteStoreV1::canonical_members(&path)[1..]
                .iter()
                .all(|member| !member.exists()),
            "partial verification must not create writer WAL/SHM companions"
        );
    }

    #[test]
    fn actual_candidate_bytes_are_checked_before_promotion() {
        let root = TempDir::new().unwrap();
        let canonical = root.path().join("C.mft.sqlite3");
        let temporary = root.path().join("C.mft.sqlite3.bounded.migration-tmp");
        let index = MftIndexV1::from_entries(BTreeMap::from([(
            5,
            MftEntryV1 {
                reference: 5,
                parent_reference: 5,
                name: "root".into(),
                logical_bytes: 0,
                allocated_bytes: 0,
                is_directory: true,
            },
        )]));
        let barrier = LifecycleBarrierV1::new();
        assert!(
            MftSqliteStoreV1::snapshot_focused_bounded_linearized(
                &temporary,
                &canonical,
                root.path(),
                identity(),
                &index,
                false,
                1,
                &barrier,
                || true,
            )
            .is_err()
        );
        assert!(!canonical.exists());

        let temporary = root.path().join("C.mft.sqlite3.fitting.migration-tmp");
        let limit = 4 * 1024 * 1024;
        let store = MftSqliteStoreV1::snapshot_focused_bounded_linearized(
            &temporary,
            &canonical,
            root.path(),
            identity(),
            &index,
            false,
            limit,
            &barrier,
            || true,
        )
        .unwrap();
        let retained = MftSqliteStoreV1::canonical_members(&canonical)
            .iter()
            .map(|member| std::fs::metadata(member).map_or(0, |metadata| metadata.len()))
            .sum::<u64>();
        assert!(retained <= limit);
        drop(store);
    }

    #[test]
    fn admission_rejects_oversized_names_and_cyclic_parent_graphs() {
        let root = TempDir::new().unwrap();

        let wide_page_path = root.path().join("P.mft.sqlite3");
        let connection = Connection::open(&wide_page_path).unwrap();
        connection
            .pragma_update(None, "page_size", 65_536_u64)
            .unwrap();
        initialize_schema(&connection, identity()).unwrap();
        drop(connection);
        assert!(
            MftSqliteStoreV1::load_read_only(
                &wide_page_path,
                root.path(),
                identity().volume,
                identity().cursor.journal_id,
            )
            .unwrap_err()
            .contains("page size")
        );

        let oversized_path = root.path().join("O.mft.sqlite3");
        drop(MftSqliteStoreV1::create(&oversized_path, root.path(), identity()).unwrap());
        let connection = Connection::open(&oversized_path).unwrap();
        connection
            .execute(
                "INSERT INTO entries
                 (reference,parent_reference,name,kind,logical_bytes,allocated_bytes)
                 VALUES (5,5,?1,1,0,0)",
                params!["x".repeat(MAX_ENTRY_NAME_BYTES + 1)],
            )
            .unwrap();
        drop(connection);
        assert!(
            MftSqliteStoreV1::load_read_only(
                &oversized_path,
                root.path(),
                identity().volume,
                identity().cursor.journal_id,
            )
            .is_err()
        );

        let cyclic_path = root.path().join("Y.mft.sqlite3");
        drop(MftSqliteStoreV1::create(&cyclic_path, root.path(), identity()).unwrap());
        let connection = Connection::open(&cyclic_path).unwrap();
        connection
            .execute_batch(
                "INSERT INTO entries VALUES (1,2,'one',1,0,0);
                 INSERT INTO entries VALUES (2,1,'two',1,0,0);",
            )
            .unwrap();
        drop(connection);
        assert!(
            MftSqliteStoreV1::load_read_only(
                &cyclic_path,
                root.path(),
                identity().volume,
                identity().cursor.journal_id,
            )
            .is_err()
        );
    }

    #[test]
    fn metadata_and_entry_keys_round_trip_full_u64_domain() {
        let root = TempDir::new().unwrap();
        let path = root.path().join("C.mft.sqlite3");
        for journal_id in [i64::MAX as u64 + 1, u64::MAX] {
            let candidate = StoreIdentityV1 {
                volume: VolumeIdentityV2 { serial: u64::MAX },
                cursor: JournalCursorV1 {
                    journal_id,
                    next_usn: 100,
                    generation: u64::MAX,
                },
                complete: true,
            };
            let candidate_path = path.with_extension(format!("{journal_id}.sqlite3"));
            let store = MftSqliteStoreV1::create(&candidate_path, root.path(), candidate).unwrap();
            assert_eq!(store.identity(), candidate);
            drop(store);
            let reopened =
                MftSqliteStoreV1::open(&candidate_path, root.path(), candidate.volume, journal_id)
                    .unwrap();
            assert_eq!(reopened.identity(), candidate);
        }
    }

    #[test]
    fn transaction_atomically_applies_entries_and_cursor() {
        let root = TempDir::new().unwrap();
        let path = root.path().join("D.mft.sqlite3");
        let mut store = MftSqliteStoreV1::create(&path, root.path(), identity()).unwrap();
        store
            .commit_changes(&[change(20)], next(1), CommitFailurePointV1::None)
            .unwrap();
        assert_eq!(store.entry_count().unwrap(), 1);
        assert_eq!(store.identity().cursor, next(1));
    }

    #[test]
    fn incomplete_snapshot_is_promoted_atomically() {
        let root = TempDir::new().unwrap();
        let path = root.path().join("snapshot.mft.sqlite3");
        let mut initial = identity();
        initial.complete = false;
        let mut store = MftSqliteStoreV1::create(&path, root.path(), initial).unwrap();
        let entries = BTreeMap::from([(
            5,
            MftEntryV1 {
                reference: 5,
                parent_reference: 5,
                name: "root".to_owned(),
                logical_bytes: 0,
                allocated_bytes: 0,
                is_directory: true,
            },
        )]);
        let index = MftIndexV1::from_entries(entries);
        store.install_snapshot(&index, next(1)).unwrap();
        assert_eq!(store.entry_count().unwrap(), 1);
        assert!(store.identity().complete);
        assert_eq!(store.identity().cursor, next(1));
        assert!(store.install_snapshot(&index, next(2)).is_err());
    }

    #[test]
    fn rollback_journal_migration_promotes_one_verified_main_file() {
        let root = TempDir::new().unwrap();
        let canonical = root.path().join("M.mft.sqlite3");
        let temporary = root.path().join("M.mft.sqlite3.123.migration-tmp");
        let index = MftIndexV1::from_entries(BTreeMap::from([(
            5,
            MftEntryV1 {
                reference: 5,
                parent_reference: 5,
                name: "root".into(),
                logical_bytes: 0,
                allocated_bytes: 0,
                is_directory: true,
            },
        )]));
        let store = MftSqliteStoreV1::migrate_snapshot(
            &temporary,
            &canonical,
            root.path(),
            identity(),
            &index,
        )
        .unwrap();
        assert_eq!(store.identity(), identity());
        assert_eq!(store.entry_count().unwrap(), 1);
        assert!(!temporary.exists());
        assert!(canonical.is_file());
        assert!(
            MftSqliteStoreV1::migrate_snapshot(
                &root.path().join("M.mft.sqlite3.456.migration-tmp"),
                &canonical,
                root.path(),
                identity(),
                &index,
            )
            .is_err()
        );
    }

    #[test]
    fn migration_fault_matrix_never_promotes_before_atomic_boundary() {
        let index = MftIndexV1::from_entries(BTreeMap::from([(
            5,
            MftEntryV1 {
                reference: 5,
                parent_reference: 5,
                name: "root".into(),
                logical_bytes: 0,
                allocated_bytes: 0,
                is_directory: true,
            },
        )]));
        for failure in [
            MigrationFailurePointV1::Build,
            MigrationFailurePointV1::TempCommit,
            MigrationFailurePointV1::Fsync,
            MigrationFailurePointV1::PreVerify,
            MigrationFailurePointV1::Promote,
            MigrationFailurePointV1::Reopen,
            MigrationFailurePointV1::PostVerify,
        ] {
            let root = TempDir::new().unwrap();
            let canonical = root.path().join("Z.mft.sqlite3");
            let temporary = root.path().join("Z.mft.sqlite3.fault.migration-tmp");
            assert!(
                MftSqliteStoreV1::migrate_snapshot_injected(
                    &temporary,
                    &canonical,
                    root.path(),
                    identity(),
                    &index,
                    failure,
                    false,
                    None,
                    None,
                    || true,
                )
                .is_err()
            );
            if matches!(
                failure,
                MigrationFailurePointV1::Build
                    | MigrationFailurePointV1::TempCommit
                    | MigrationFailurePointV1::Fsync
                    | MigrationFailurePointV1::PreVerify
                    | MigrationFailurePointV1::Promote
            ) {
                assert!(!canonical.exists(), "{failure:?} promoted too early");
            } else {
                let (admitted, loaded) = MftSqliteStoreV1::load_read_only(
                    &canonical,
                    root.path(),
                    identity().volume,
                    identity().cursor.journal_id,
                )
                .unwrap();
                assert_eq!(admitted, identity());
                assert_eq!(loaded.entries.len(), 1);
            }
        }
    }

    #[test]
    fn failed_rebuild_reopen_or_verification_restores_previous_durable_store() {
        for failure in [
            MigrationFailurePointV1::Reopen,
            MigrationFailurePointV1::PostVerify,
        ] {
            let root = TempDir::new().unwrap();
            let canonical = root.path().join("R.mft.sqlite3");
            let temporary = root.path().join("R.mft.sqlite3.migration-tmp");
            let mut old = MftSqliteStoreV1::create(&canonical, root.path(), identity()).unwrap();
            old.commit_changes(&[change(20)], next(1), CommitFailurePointV1::None)
                .unwrap();
            drop(old);

            let candidate_identity = StoreIdentityV1 {
                cursor: next(2),
                ..identity()
            };
            let candidate = MftIndexV1::from_entries(BTreeMap::from([(
                99,
                MftEntryV1 {
                    reference: 99,
                    parent_reference: 99,
                    name: "replacement-root".into(),
                    logical_bytes: 0,
                    allocated_bytes: 0,
                    is_directory: true,
                },
            )]));
            assert!(
                MftSqliteStoreV1::migrate_snapshot_injected(
                    &temporary,
                    &canonical,
                    root.path(),
                    candidate_identity,
                    &candidate,
                    failure,
                    true,
                    None,
                    None,
                    || true,
                )
                .is_err()
            );

            let (restored_identity, restored) = MftSqliteStoreV1::load_read_only(
                &canonical,
                root.path(),
                identity().volume,
                identity().cursor.journal_id,
            )
            .unwrap();
            assert_eq!(restored_identity.cursor, next(1), "{failure:?}");
            assert!(restored.entries.contains_key(&20), "{failure:?}");
            assert!(!restored.entries.contains_key(&99), "{failure:?}");
            assert!(
                !PathBuf::from(format!("{}.replacement-backup", canonical.display())).exists(),
                "{failure:?} left an unmanaged safety copy"
            );
        }
    }

    #[test]
    fn startup_recovery_restores_a_verified_backup_only_with_focus_and_open_barrier() {
        let root = TempDir::new().unwrap();
        let canonical = root.path().join("S.mft.sqlite3");
        let backup = MftSqliteStoreV1::replacement_backup_path(&canonical);
        let mut old = MftSqliteStoreV1::create(&canonical, root.path(), identity()).unwrap();
        old.commit_changes(&[change(42)], next(1), CommitFailurePointV1::None)
            .unwrap();
        drop(old);
        let connection = Connection::open(&canonical).unwrap();
        connection
            .execute("VACUUM INTO ?1", params![backup.to_string_lossy().as_ref()])
            .unwrap();
        drop(connection);
        for companion in &MftSqliteStoreV1::canonical_members(&canonical)[1..] {
            if companion.exists() {
                std::fs::remove_file(companion).unwrap();
            }
        }
        std::fs::write(&canonical, b"invalid replacement").unwrap();

        let barrier = LifecycleBarrierV1::new();
        assert!(
            MftSqliteStoreV1::restore_replacement_backup_focused_linearized(
                &backup,
                &canonical,
                root.path(),
                identity().volume,
                identity().cursor.journal_id,
                &barrier,
                || false,
            )
            .is_err()
        );
        assert!(backup.is_file());
        let recovered = MftSqliteStoreV1::restore_replacement_backup_focused_linearized(
            &backup,
            &canonical,
            root.path(),
            identity().volume,
            identity().cursor.journal_id,
            &barrier,
            || true,
        )
        .unwrap();
        assert_eq!(recovered.identity().cursor, next(1));
        assert!(recovered.load_index().unwrap().entries.contains_key(&42));
        assert!(!backup.exists());
    }

    #[test]
    fn backup_promotion_remains_read_only_admissible_if_reopen_loses_focus() {
        let root = TempDir::new().unwrap();
        let canonical = root.path().join("T.mft.sqlite3");
        let backup = MftSqliteStoreV1::replacement_backup_path(&canonical);
        let mut old = MftSqliteStoreV1::create(&canonical, root.path(), identity()).unwrap();
        old.commit_changes(&[change(43)], next(1), CommitFailurePointV1::None)
            .unwrap();
        drop(old);
        let connection = Connection::open(&canonical).unwrap();
        connection
            .execute("VACUUM INTO ?1", params![backup.to_string_lossy().as_ref()])
            .unwrap();
        drop(connection);
        std::fs::write(&canonical, b"invalid replacement").unwrap();

        let focus_checks = std::sync::atomic::AtomicU8::new(0);
        let barrier = LifecycleBarrierV1::new();
        assert!(
            MftSqliteStoreV1::restore_replacement_backup_focused_linearized(
                &backup,
                &canonical,
                root.path(),
                identity().volume,
                identity().cursor.journal_id,
                &barrier,
                || focus_checks.fetch_add(1, std::sync::atomic::Ordering::AcqRel) == 0,
            )
            .is_err()
        );
        assert!(!backup.exists(), "promotion consumed the verified backup");
        let (admitted, index) = MftSqliteStoreV1::load_read_only(
            &canonical,
            root.path(),
            identity().volume,
            identity().cursor.journal_id,
        )
        .unwrap();
        assert_eq!(admitted.cursor, next(1));
        assert!(index.entries.contains_key(&43));
    }

    #[test]
    fn migration_rejects_live_temp_wal_path_escape_and_identity_change() {
        let root = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        let canonical = root.path().join("Y.mft.sqlite3");
        let index = MftIndexV1::from_entries(BTreeMap::new());
        assert!(
            MftSqliteStoreV1::migrate_snapshot(
                &outside.path().join("Y.mft.sqlite3.x.migration-tmp"),
                &canonical,
                root.path(),
                identity(),
                &index
            )
            .is_err()
        );
        let temporary = root.path().join("Y.mft.sqlite3.x.migration-tmp");
        std::fs::write(&temporary, b"occupied").unwrap();
        std::fs::write(wal_path(&temporary), b"live").unwrap();
        assert!(
            MftSqliteStoreV1::migrate_snapshot(
                &temporary,
                &canonical,
                root.path(),
                identity(),
                &index
            )
            .is_err()
        );
        let mut wrong = identity();
        wrong.volume.serial = 999;
        std::fs::remove_file(&temporary).unwrap();
        std::fs::remove_file(wal_path(&temporary)).unwrap();
        let store =
            MftSqliteStoreV1::migrate_snapshot(&temporary, &canonical, root.path(), wrong, &index)
                .unwrap();
        drop(store);
        assert!(
            MftSqliteStoreV1::load_read_only(
                &canonical,
                root.path(),
                identity().volume,
                identity().cursor.journal_id
            )
            .is_err()
        );
    }

    #[test]
    fn restart_replay_is_idempotent_without_immediate_persistence() {
        let root = TempDir::new().unwrap();
        let path = root.path().join("replay.mft.sqlite3");
        let mut initial = identity();
        initial.complete = false;
        let mut store = MftSqliteStoreV1::create(&path, root.path(), initial).unwrap();
        let base = MftIndexV1::from_entries(BTreeMap::from([(
            5,
            MftEntryV1 {
                reference: 5,
                parent_reference: 5,
                name: "root".to_owned(),
                logical_bytes: 0,
                allocated_bytes: 0,
                is_directory: true,
            },
        )]));
        store.install_snapshot(&base, next(1)).unwrap();
        drop(store);

        let replay = [change(20), change(21)];
        for _ in 0..2 {
            let store = MftSqliteStoreV1::open(
                &path,
                root.path(),
                identity().volume,
                identity().cursor.journal_id,
            )
            .unwrap();
            let mut memory = store.load_index().unwrap();
            for change in &replay {
                memory.apply_change(change).unwrap();
            }
            assert_eq!(memory.entries.len(), 3);
            assert_eq!(store.identity().cursor, next(1));
            assert_eq!(store.entry_count().unwrap(), 1);
        }
    }

    #[test]
    fn failures_never_advance_entries_or_cursor() {
        for point in [
            CommitFailurePointV1::BeforeMutation,
            CommitFailurePointV1::BeforeCursor,
            CommitFailurePointV1::BeforeCommit,
        ] {
            let root = TempDir::new().unwrap();
            let path = root.path().join("E.mft.sqlite3");
            let mut store = MftSqliteStoreV1::create(&path, root.path(), identity()).unwrap();
            assert!(store.commit_changes(&[change(30)], next(1), point).is_err());
            drop(store);
            let reopened = MftSqliteStoreV1::open(
                &path,
                root.path(),
                identity().volume,
                identity().cursor.journal_id,
            )
            .unwrap();
            assert_eq!(reopened.entry_count().unwrap(), 0);
            assert_eq!(reopened.identity().cursor, identity().cursor);
        }
    }

    #[test]
    fn lifecycle_barrier_rolls_back_before_begin_or_commit_invocation() {
        for close_on_check in [1_u32, 2] {
            let root = TempDir::new().unwrap();
            let path = root
                .path()
                .join(format!("stop-{close_on_check}.mft.sqlite3"));
            let mut store = MftSqliteStoreV1::create(&path, root.path(), identity()).unwrap();
            let checks = Cell::new(0_u32);
            let result = store.commit_changes_guarded(
                &[change(60)],
                next(1),
                CommitFailurePointV1::None,
                || {
                    checks.set(checks.get() + 1);
                    checks.get() < close_on_check
                },
            );
            assert!(result.is_err());
            assert_eq!(store.entry_count().unwrap(), 0);
            assert_eq!(store.identity().cursor, identity().cursor);
        }
    }

    #[test]
    fn shutdown_linearization_matrix_allows_only_invoked_commit() {
        // Mutation/pre-cursor/pre-commit failures all roll back entries and cursor.
        for (ordinal, point) in [
            CommitFailurePointV1::BeforeMutation,
            CommitFailurePointV1::BeforeCursor,
            CommitFailurePointV1::BeforeCommit,
        ]
        .into_iter()
        .enumerate()
        {
            let root = TempDir::new().unwrap();
            let path = root.path().join(format!("matrix-{ordinal}.mft.sqlite3"));
            let mut store = MftSqliteStoreV1::create(&path, root.path(), identity()).unwrap();
            assert!(
                store
                    .commit_changes_guarded(&[change(90)], next(1), point, || true)
                    .is_err()
            );
            assert_eq!(store.entry_count().unwrap(), 0);
            assert_eq!(store.identity().cursor, identity().cursor);
        }
        // Once both lifecycle checks pass, COMMIT has been invoked and remains
        // the atomic linearization point even if shutdown is observed afterward.
        let root = TempDir::new().unwrap();
        let path = root.path().join("matrix-invoked.mft.sqlite3");
        let mut store = MftSqliteStoreV1::create(&path, root.path(), identity()).unwrap();
        let checks = Cell::new(0_u32);
        store
            .commit_changes_guarded(&[change(91)], next(1), CommitFailurePointV1::None, || {
                checks.set(checks.get() + 1);
                checks.get() <= 2
            })
            .unwrap();
        assert_eq!(checks.get(), 2);
        assert_eq!(store.entry_count().unwrap(), 1);
        assert_eq!(store.identity().cursor, next(1));
    }

    #[test]
    fn invalid_identity_corruption_and_cursor_regression_are_rejected() {
        let root = TempDir::new().unwrap();
        let path = root.path().join("F.mft.sqlite3");
        let mut store = MftSqliteStoreV1::create(&path, root.path(), identity()).unwrap();
        assert!(
            store
                .commit_changes(&[], identity().cursor, CommitFailurePointV1::None)
                .is_err()
        );
        drop(store);
        assert!(
            MftSqliteStoreV1::open(
                &path,
                root.path(),
                VolumeIdentityV2 { serial: 8 },
                identity().cursor.journal_id,
            )
            .is_err()
        );
        let corrupt = root.path().join("corrupt.mft.sqlite3");
        std::fs::write(&corrupt, b"not sqlite").unwrap();
        assert!(
            MftSqliteStoreV1::open(
                &corrupt,
                root.path(),
                identity().volume,
                identity().cursor.journal_id,
            )
            .is_err()
        );
    }

    #[test]
    fn wal_policy_never_checkpoints_without_threshold_and_focus() {
        let root = TempDir::new().unwrap();
        let path = root.path().join("G.mft.sqlite3");
        let mut store = MftSqliteStoreV1::create(&path, root.path(), identity()).unwrap();
        assert!(!store.wal_checkpoint_eligible(true, false));
        assert!(!store.truncate_wal(false, false).unwrap());
        assert!(store.wal_allows(MAX_PENDING_BATCH_BYTES));
        let telemetry = store.telemetry();
        assert!(telemetry.main_bytes > 0);
        assert!(telemetry.wal_bytes < WAL_MAINTENANCE_THRESHOLD_BYTES);
    }

    #[test]
    fn wal_threshold_and_hard_admission_boundaries_are_exact() {
        assert!(!checkpoint_eligible(
            WAL_MAINTENANCE_THRESHOLD_BYTES,
            true,
            false
        ));
        assert!(checkpoint_eligible(
            WAL_MAINTENANCE_THRESHOLD_BYTES + 1,
            true,
            false
        ));
        assert!(!checkpoint_eligible(
            WAL_MAINTENANCE_THRESHOLD_BYTES + 1,
            false,
            false
        ));
        assert!(!checkpoint_eligible(
            WAL_MAINTENANCE_THRESHOLD_BYTES + 1,
            true,
            true
        ));
        assert!(wal_admission(
            WAL_MAINTENANCE_THRESHOLD_BYTES,
            MAX_PENDING_BATCH_BYTES
        ));
        assert!(!wal_admission(
            WAL_MAINTENANCE_THRESHOLD_BYTES + 1,
            MAX_PENDING_BATCH_BYTES
        ));
    }

    #[test]
    fn maximum_encoded_batch_and_repeated_precommit_failures_obey_measured_wal_bound() {
        const CHANGE_COUNT: usize = 65_536;
        const NAME_BYTES: usize = 207;
        let root = TempDir::new().unwrap();
        let path = root.path().join("W.mft.sqlite3");
        let mut store = MftSqliteStoreV1::create(&path, root.path(), identity()).unwrap();
        let make_batch = |round: usize| {
            (0..CHANGE_COUNT)
                .map(|index| {
                    let prefix = format!("{round:02}-{index:05}-");
                    let name = format!("{prefix}{}", "x".repeat(NAME_BYTES - prefix.len()));
                    MftChangeV2 {
                        kind: MftChangeKindV2::Upsert,
                        reference: 10_000 + index as u64,
                        parent_reference: 5,
                        name,
                        logical_bytes: 8,
                        allocated_bytes: 4_096,
                        is_directory: false,
                        reason: 1,
                    }
                })
                .collect::<Vec<_>>()
        };
        let batch = make_batch(0);
        assert_eq!(
            batch
                .iter()
                .map(|change| 49_usize + change.name.len())
                .sum::<usize>(),
            MAX_PENDING_BATCH_BYTES as usize
        );
        let before = file_bytes(&wal_path(&path));
        store
            .commit_changes(&batch, next(1), CommitFailurePointV1::None)
            .unwrap();
        let one_batch_growth = file_bytes(&wal_path(&path)).saturating_sub(before);
        let measured_allowance = MAX_SCATTERED_WAL_GROWTH_BYTES
            .max(MAX_PENDING_BATCH_BYTES * WAL_FRAME_OVERHEAD_MULTIPLIER + 1024 * 1024);
        assert!(one_batch_growth <= measured_allowance);

        for round in 1..=3 {
            assert!(
                store
                    .commit_changes(
                        &make_batch(round),
                        next(1 + round as u64),
                        CommitFailurePointV1::BeforeCommit,
                    )
                    .is_err()
            );
            assert_eq!(store.identity().cursor, next(1));
            assert!(
                file_bytes(&wal_path(&path))
                    <= WAL_MAINTENANCE_THRESHOLD_BYTES.saturating_add(measured_allowance)
            );
        }
    }

    #[test]
    fn busy_checkpoint_preserves_reads_and_later_retry_truncates() {
        let root = TempDir::new().unwrap();
        let path = root.path().join("busy.mft.sqlite3");
        let mut store = MftSqliteStoreV1::create(&path, root.path(), identity()).unwrap();
        store
            .commit_changes(&[change(80)], next(1), CommitFailurePointV1::None)
            .unwrap();
        let reader = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        reader.execute_batch("BEGIN").unwrap();
        assert_eq!(
            reader
                .query_row("SELECT COUNT(*) FROM entries", [], |row| row
                    .get::<_, u64>(0))
                .unwrap(),
            1
        );
        store
            .commit_changes(&[change(81)], next(2), CommitFailurePointV1::None)
            .unwrap();
        assert!(run_truncate_checkpoint(&store.connection).is_err());
        assert_eq!(store.entry_count().unwrap(), 2);
        assert!(
            file_bytes(&wal_path(&path))
                <= WAL_MAINTENANCE_THRESHOLD_BYTES + MAX_SCATTERED_WAL_GROWTH_BYTES
        );
        reader.execute_batch("ROLLBACK").unwrap();
        drop(reader);
        run_truncate_checkpoint(&store.connection).unwrap();
        assert_eq!(file_bytes(&wal_path(&path)), 0);
        assert_eq!(store.entry_count().unwrap(), 2);
    }

    #[test]
    fn fragmented_prefilled_store_scattered_deletes_fit_count_derived_wal_bound() {
        let root = TempDir::new().unwrap();
        let path = root.path().join("fragmented.mft.sqlite3");
        let mut store = MftSqliteStoreV1::create(&path, root.path(), identity()).unwrap();
        let large_name = "x".repeat(3_800);
        {
            let transaction = store.connection.transaction().unwrap();
            transaction
                .execute(
                    "INSERT INTO entries
                     (reference,parent_reference,name,kind,logical_bytes,allocated_bytes)
                     VALUES (5,5,'root',1,0,0)",
                    [],
                )
                .unwrap();
            {
                let mut insert = transaction
                    .prepare(
                        "INSERT INTO entries
                         (reference,parent_reference,name,kind,logical_bytes,allocated_bytes)
                         VALUES (?1,5,?2,0,1,4096)",
                    )
                    .unwrap();
                for index in 0..PENDING_CHANGE_LIMIT {
                    insert
                        .execute(params![encode_u64(10_000 + index as u64), &large_name])
                        .unwrap();
                }
            }
            transaction.commit().unwrap();
        }
        run_truncate_checkpoint(&store.connection).unwrap();
        let before = file_bytes(&wal_path(&path));
        let deletes = (0..PENDING_CHANGE_LIMIT)
            .map(|index| MftChangeV2 {
                kind: MftChangeKindV2::Delete,
                reference: 10_000 + index as u64,
                parent_reference: 5,
                name: String::new(),
                logical_bytes: 0,
                allocated_bytes: 0,
                is_directory: false,
                reason: 1,
            })
            .collect::<Vec<_>>();
        store
            .commit_changes(&deletes, next(1), CommitFailurePointV1::None)
            .unwrap();
        let growth = file_bytes(&wal_path(&path)).saturating_sub(before);
        assert!(
            growth <= MAX_SCATTERED_WAL_GROWTH_BYTES,
            "scattered WAL growth {growth} exceeded {}",
            MAX_SCATTERED_WAL_GROWTH_BYTES
        );
    }

    #[test]
    fn replacement_safety_copy_is_interrupted_when_gate_closes_mid_vacuum() {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        };

        let root = TempDir::new().unwrap();
        let path = root.path().join("cancel-source.mft.sqlite3");
        let backup = root.path().join("cancel-backup.mft.sqlite3");
        let mut store = MftSqliteStoreV1::create(&path, root.path(), identity()).unwrap();
        let large_name = "x".repeat(3_800);
        {
            let transaction = store.connection.transaction().unwrap();
            {
                let mut insert = transaction
                    .prepare(
                        "INSERT INTO entries
                         (reference,parent_reference,name,kind,logical_bytes,allocated_bytes)
                         VALUES (?1,?1,?2,1,0,0)",
                    )
                    .unwrap();
                for index in 0..20_000_u64 {
                    insert.execute(params![index + 1, &large_name]).unwrap();
                }
            }
            transaction.commit().unwrap();
        }
        let gate = Arc::new(AtomicBool::new(true));
        let closing_gate = Arc::clone(&gate);
        let closer = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(5));
            closing_gate.store(false, Ordering::Release);
        });
        let result = run_cancellable_vacuum_into(&store.connection, &backup, &|| {
            gate.load(Ordering::Acquire)
        });
        closer.join().unwrap();
        assert!(result.is_err());
        assert!(!gate.load(Ordering::Acquire));
    }

    #[test]
    fn last_close_preserves_wal_and_does_not_backfill_main_database() {
        let root = TempDir::new().unwrap();
        let path = root.path().join("H.mft.sqlite3");
        let wal = wal_path(&path);
        let shm = PathBuf::from(format!("{}-shm", path.display()));
        let mut store = MftSqliteStoreV1::create(&path, root.path(), identity()).unwrap();
        store
            .commit_changes(&[change(40)], next(1), CommitFailurePointV1::None)
            .unwrap();
        let main_before = std::fs::read(&path).unwrap();
        let wal_before = std::fs::read(&wal).unwrap();
        assert!(!wal_before.is_empty());
        assert!(shm.is_file());
        drop(store);
        assert_eq!(std::fs::read(&path).unwrap(), main_before);
        assert_eq!(std::fs::read(&wal).unwrap(), wal_before);
        assert!(shm.is_file());
    }

    #[test]
    fn unfocused_read_only_admission_preserves_durable_bytes_and_file_set() {
        let root = TempDir::new().unwrap();
        let path = root.path().join("startup.mft.sqlite3");
        let mut store = MftSqliteStoreV1::create(&path, root.path(), identity()).unwrap();
        store
            .commit_changes(&[change(70)], next(1), CommitFailurePointV1::None)
            .unwrap();
        drop(store);
        let members = MftSqliteStoreV1::canonical_members(&path);
        let before_names = std::fs::read_dir(root.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        let main_before = std::fs::read(&members[0]).unwrap();
        let wal_before = std::fs::read(&members[1]).unwrap();
        let (admitted, index) = MftSqliteStoreV1::load_read_only(
            &path,
            root.path(),
            identity().volume,
            identity().cursor.journal_id,
        )
        .unwrap();
        assert_eq!(admitted.cursor, next(1));
        assert!(index.entries.contains_key(&70));
        let after_names = std::fs::read_dir(root.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        assert_eq!(std::fs::read(&members[0]).unwrap(), main_before);
        assert_eq!(std::fs::read(&members[1]).unwrap(), wal_before);
        assert_eq!(after_names, before_names);
    }
}
