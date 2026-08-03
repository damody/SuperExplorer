//! Bounded import of the canonical P0 `.sepack` ZIP subset.
//!
//! P0 accepts only a store-only, UTF-8 ZIP with mandatory `manifest.json` and
//! `plugin/plugin.dll` runtime entries. Rejecting compression, data descriptors, ZIP64, and
//! directory records makes the on-disk format deterministic and avoids a
//! decompressor or archive-path trust boundary in the host.  The importer does
//! not trust the manifest: it extracts bytes only into host-private scratch
//! space and callers must still use [`PackageValidatorV1`].

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt, fs,
    fs::{File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};

use ring::rand::{SecureRandom as _, SystemRandom};
use sha2::{Digest as _, Sha256};
use thiserror::Error;

use crate::PackageValidationCancellationV1;

const MAX_ARCHIVE_BYTES_V1: u64 = 512 * 1024 * 1024;
const MAX_ENTRY_BYTES_V1: u64 = 512 * 1024 * 1024;
const MAX_ENTRY_COUNT_V1: usize = 1_024;
const MAX_ENTRY_DEPTH_V1: usize = 32;
const MAX_ENTRY_NAME_BYTES_V1: usize = 1_024;
const MAX_CENTRAL_DIRECTORY_BYTES_V1: usize = 16 * 1024 * 1024;
const MAX_STAGING_ROOT_ENTRY_SCAN_V1: usize = 256;
const MAX_STAGING_ENTRY_COUNT_V1: usize = 1_024;
const MINIMUM_STAGING_AGE_V1: Duration = Duration::from_mins(15);
const STAGING_SCAVENGE_TIMEOUT_V1: Duration = Duration::from_secs(1);
const IMPORT_TIMEOUT_V1: Duration = Duration::from_secs(30);
const ZIP_END_OF_CENTRAL_DIRECTORY_SIGNATURE: u32 = 0x0605_4b50;
const ZIP_CENTRAL_DIRECTORY_SIGNATURE: u32 = 0x0201_4b50;
const ZIP_LOCAL_FILE_SIGNATURE: u32 = 0x0403_4b50;
const ZIP_UTF8_FLAG: u16 = 1 << 11;

#[cfg(test)]
static FORCED_CLEANUP_FAILURES_V1: std::sync::Mutex<BTreeSet<PathBuf>> =
    std::sync::Mutex::new(BTreeSet::new());

/// Host-owned importer for the strict P0 `.sepack` ZIP format.
#[derive(Clone)]
pub(crate) struct SePackImporterV1 {
    staging_root: Arc<StagingRootV1>,
}

struct StagingRootV1 {
    path: PathBuf,
    #[cfg(windows)]
    handle: DirectoryHandleV1,
}

/// Shared cancellation/deadline gate for every bounded importer phase.
///
/// The importer checks this before and after each bounded read, parse step,
/// extraction write, synchronization, and publication handoff. A blocking
/// kernel I/O call itself cannot be interrupted by synchronous `std::fs`.
struct ImportBudgetV1<'a> {
    deadline: Instant,
    cancelled: &'a PackageValidationCancellationV1,
    #[cfg(test)]
    cancel_at_commit_for_test: bool,
}

impl ImportBudgetV1<'_> {
    fn check(&self) -> Result<(), SePackImportErrorV1> {
        if self.cancelled.cancelled() {
            return Err(SePackImportErrorV1::ImportCancelled);
        }
        if Instant::now() >= self.deadline {
            return Err(SePackImportErrorV1::ImportTimedOut);
        }
        Ok(())
    }
}

impl fmt::Debug for SePackImporterV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SePackImporterV1 { source_root: <redacted> }")
    }
}

impl SePackImporterV1 {
    /// Creates an importer rooted in a host-controlled local package source.
    ///
    /// # Errors
    ///
    /// Returns an error when the source root or its private staging directory
    /// is a symlink, junction, reparse point, or otherwise unsafe directory.
    pub(crate) fn new(source_root: &Path) -> Result<Self, SePackImportErrorV1> {
        fs::create_dir_all(source_root).map_err(|source| SePackImportErrorV1::Io {
            path: source_root.to_path_buf(),
            source,
        })?;
        let source_root = verify_safe_directory(source_root)?;
        let staging_root = source_root.join(".sepack-staging");
        if !staging_root.exists() {
            fs::create_dir(&staging_root).map_err(|source| SePackImportErrorV1::Io {
                path: staging_root.clone(),
                source,
            })?;
        }
        let staging_root = verify_safe_directory(&staging_root)?;
        if staging_root.parent() != Some(source_root.as_path()) {
            return Err(SePackImportErrorV1::UnsafeSourceRoot { path: source_root });
        }
        scavenge_stale_staging_directories(&staging_root);
        Ok(Self {
            staging_root: Arc::new(StagingRootV1::open(staging_root)?),
        })
    }

    /// Extracts one canonical `.sepack` into private staging and atomically
    /// exposes it as a private, immutable validation candidate.
    ///
    /// The returned path is nested below `.sepack-staging`, so no package source
    /// can discover it. Loading still requires validation into the sealed store.
    ///
    /// # Errors
    ///
    /// Returns an error without publishing any partial package when the archive
    /// is not the strict P0 ZIP subset, violates a resource bound, or extraction
    /// encounters an I/O failure.
    #[allow(dead_code, reason = "compatibility wrapper for internal host callers")]
    pub(crate) fn import_archive(
        &self,
        archive: &Path,
    ) -> Result<ImportedSePackV1, SePackImportErrorV1> {
        let cancelled = PackageValidationCancellationV1::new();
        let budget = ImportBudgetV1 {
            deadline: Instant::now() + IMPORT_TIMEOUT_V1,
            cancelled: &cancelled,
            #[cfg(test)]
            cancel_at_commit_for_test: false,
        };
        self.import_archive_with_budget(archive, &budget)
    }

    /// Imports using a caller-owned cancellation token. Cancellation is
    /// checked through every extraction and before publication.
    pub(crate) fn import_archive_with_cancellation(
        &self,
        archive: &Path,
        cancellation: &PackageValidationCancellationV1,
    ) -> Result<ImportedSePackV1, SePackImportErrorV1> {
        let budget = ImportBudgetV1 {
            deadline: Instant::now() + IMPORT_TIMEOUT_V1,
            cancelled: cancellation,
            #[cfg(test)]
            cancel_at_commit_for_test: false,
        };
        self.import_archive_with_budget(archive, &budget)
    }

    fn import_archive_with_budget(
        &self,
        archive: &Path,
        budget: &ImportBudgetV1<'_>,
    ) -> Result<ImportedSePackV1, SePackImportErrorV1> {
        budget.check()?;
        let mut archive_file = open_safe_archive(archive)?;
        let archive_size = archive_file
            .metadata()
            .map_err(|source| SePackImportErrorV1::Io {
                path: archive.to_path_buf(),
                source,
            })?
            .len();
        if archive_size > MAX_ARCHIVE_BYTES_V1 {
            return Err(SePackImportErrorV1::ArchiveTooLarge {
                actual: archive_size,
                maximum: MAX_ARCHIVE_BYTES_V1,
            });
        }
        let archive_sha256 = sha256_file(&mut archive_file, archive, archive_size, budget)?;
        let entries = parse_canonical_zip(&mut archive_file, archive, archive_size, budget)?;
        let mut staging = StagingDirectory::new(self.staging_root.as_ref())?;
        extract_entries(&mut archive_file, archive, &entries, &mut staging, budget)?;
        staging.sync_all(budget)?;
        budget.check()?;
        staging.verify_identity()?;
        #[cfg(test)]
        if budget.cancel_at_commit_for_test {
            budget.cancelled.cancel();
        }
        budget.check()?;
        let root = staging.path().to_path_buf();
        staging.disarm();
        Ok(ImportedSePackV1 {
            root,
            archive_sha256,
        })
    }

    pub(crate) fn discard_import(
        &self,
        imported: &ImportedSePackV1,
    ) -> Result<(), SePackImportErrorV1> {
        if imported.root.parent() != Some(self.staging_root.path.as_path())
            || imported.archive_sha256.len() != 64
            || !imported
                .archive_sha256
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        {
            return Err(SePackImportErrorV1::UnsafeSourceRoot {
                path: imported.root.clone(),
            });
        }
        verify_safe_directory(&imported.root)?;
        #[cfg(test)]
        if forced_cleanup_failure(&imported.root) {
            return Err(SePackImportErrorV1::Io {
                path: imported.root.clone(),
                source: io::Error::other("test-only injected scratch cleanup failure"),
            });
        }
        fs::remove_dir_all(&imported.root).map_err(|source| SePackImportErrorV1::Io {
            path: imported.root.clone(),
            source,
        })
    }

    #[cfg(test)]
    pub(crate) fn force_cleanup_failure_for_test(imported: &ImportedSePackV1) {
        let mut failures = FORCED_CLEANUP_FAILURES_V1
            .lock()
            .expect("test cleanup-failure set lock");
        failures.insert(imported.root.clone());
    }
}

/// A completed archive import awaiting ordinary package-source discovery.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ImportedSePackV1 {
    root: PathBuf,
    archive_sha256: String,
}

impl ImportedSePackV1 {
    /// Returns the host-owned package root that was atomically published.
    #[must_use]
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    /// Returns the SHA-256 of the exact imported archive bytes.
    #[must_use]
    #[cfg(test)]
    pub(crate) fn archive_sha256(&self) -> &str {
        &self.archive_sha256
    }
}

/// Typed failure while parsing or extracting an untrusted `.sepack`.
#[derive(Debug, Error)]
pub enum SePackImportErrorV1 {
    /// A filesystem operation failed.
    #[error("could not access sepack path {path}: {source}")]
    Io {
        /// The path being accessed.
        path: PathBuf,
        /// The operating-system error.
        #[source]
        source: io::Error,
    },
    /// The archive file is a symlink or Windows reparse point.
    #[error("sepack archive is a symlink or reparse point: {path}")]
    ArchiveReparsePoint {
        /// The unsafe archive path.
        path: PathBuf,
    },
    /// The configured host source or staging root is unsafe.
    #[error("sepack source root is unsafe: {path}")]
    UnsafeSourceRoot {
        /// The unsafe host-owned path.
        path: PathBuf,
    },
    /// The archive exceeds the fixed import budget.
    #[error("sepack archive is {actual} bytes, exceeding the {maximum} byte limit")]
    ArchiveTooLarge {
        /// Observed archive byte length.
        actual: u64,
        /// Fixed maximum archive byte length.
        maximum: u64,
    },
    /// The ZIP representation is not the strict canonical P0 subset.
    #[error("sepack archive is not the strict canonical ZIP format: {reason}")]
    InvalidArchive {
        /// Stable explanation of the rejected format feature.
        reason: &'static str,
    },
    /// An archive member name is unsafe or not part of the P0 runtime inventory.
    #[error("sepack archive has an invalid runtime entry name: {entry}")]
    InvalidEntryName {
        /// The rejected member name.
        entry: String,
    },
    /// An archive repeats a member name after Windows case folding.
    #[error("sepack archive has duplicate or case-colliding entry: {entry}")]
    DuplicateEntry {
        /// The duplicate member name.
        entry: String,
    },
    /// The archive does not have the exact P0 runtime inventory.
    #[error("sepack archive is missing required runtime entry: {entry}")]
    MissingRuntimeEntry {
        /// Required runtime entry name.
        entry: &'static str,
    },
    /// One member exceeds the fixed byte budget.
    #[error("sepack entry {entry} is {actual} bytes, exceeding the {maximum} byte limit")]
    EntryTooLarge {
        /// Entry name.
        entry: String,
        /// Observed uncompressed size.
        actual: u64,
        /// Fixed maximum entry byte length.
        maximum: u64,
    },
    /// A concurrent or previous import already published this archive digest.
    #[error("sepack import destination already exists: {path}")]
    DestinationAlreadyExists {
        /// Existing destination path.
        path: PathBuf,
    },
    /// The bounded importer deadline elapsed before private staging could publish.
    #[error("sepack import exceeded its bounded deadline")]
    ImportTimedOut,
    /// The caller cancelled the import before private staging could publish.
    #[error("sepack import was cancelled")]
    ImportCancelled,
    /// The archive member's stored bytes differ from its ZIP CRC-32.
    #[error("sepack archive CRC mismatch for {entry}")]
    CrcMismatch {
        /// The corrupted member name.
        entry: String,
    },
}

#[derive(Clone, Debug)]
struct ZipEntry {
    name: String,
    crc32: u32,
    size: u64,
    local_offset: u64,
}

fn open_safe_archive(path: &Path) -> Result<File, SePackImportErrorV1> {
    if has_reparse_ancestor(path)? {
        return Err(SePackImportErrorV1::ArchiveReparsePoint {
            path: path.to_path_buf(),
        });
    }
    let metadata = fs::symlink_metadata(path).map_err(|source| SePackImportErrorV1::Io {
        path: path.to_path_buf(),
        source,
    })?;
    if metadata_is_reparse_point(&metadata) {
        return Err(SePackImportErrorV1::ArchiveReparsePoint {
            path: path.to_path_buf(),
        });
    }
    if !metadata.is_file() {
        return Err(SePackImportErrorV1::InvalidArchive {
            reason: "archive is not a regular file",
        });
    }
    let canonical = fs::canonicalize(path).map_err(|source| SePackImportErrorV1::Io {
        path: path.to_path_buf(),
        source,
    })?;
    #[cfg(windows)]
    let file = {
        use std::os::windows::fs::OpenOptionsExt as _;

        // Hold the archive bytes stable while copying them into private staging.
        // Open the link object itself and deny write/delete sharing. A reparse
        // swap therefore cannot silently redirect the handle after inspection.
        OpenOptions::new()
            .read(true)
            .share_mode(0x0000_0001)
            .custom_flags(0x0020_0000) // FILE_FLAG_OPEN_REPARSE_POINT
            .open(&canonical)
    };
    #[cfg(not(windows))]
    let file = OpenOptions::new().read(true).open(&canonical);
    let file = file.map_err(|source| SePackImportErrorV1::Io {
        path: path.to_path_buf(),
        source,
    })?;
    if metadata_is_reparse_point(&file.metadata().map_err(|source| SePackImportErrorV1::Io {
        path: path.to_path_buf(),
        source,
    })?) {
        return Err(SePackImportErrorV1::ArchiveReparsePoint {
            path: path.to_path_buf(),
        });
    }
    Ok(file)
}

fn verify_safe_directory(path: &Path) -> Result<PathBuf, SePackImportErrorV1> {
    if has_reparse_ancestor(path)? {
        return Err(SePackImportErrorV1::UnsafeSourceRoot {
            path: path.to_path_buf(),
        });
    }
    let metadata = fs::symlink_metadata(path).map_err(|source| SePackImportErrorV1::Io {
        path: path.to_path_buf(),
        source,
    })?;
    if !metadata.is_dir() || metadata_is_reparse_point(&metadata) {
        return Err(SePackImportErrorV1::UnsafeSourceRoot {
            path: path.to_path_buf(),
        });
    }
    fs::canonicalize(path).map_err(|source| SePackImportErrorV1::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn has_reparse_ancestor(path: &Path) -> Result<bool, SePackImportErrorV1> {
    let mut current = path.to_path_buf();
    loop {
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata_is_reparse_point(&metadata) => return Ok(true),
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(source) => {
                return Err(SePackImportErrorV1::Io {
                    path: current,
                    source,
                });
            }
        }
        let Some(parent) = current.parent() else {
            return Ok(false);
        };
        if parent == current {
            return Ok(false);
        }
        current = parent.to_path_buf();
    }
}

fn metadata_is_reparse_point(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt as _;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}

impl StagingRootV1 {
    fn open(path: PathBuf) -> Result<Self, SePackImportErrorV1> {
        #[cfg(windows)]
        let handle = DirectoryHandleV1::open_path(&path)?;
        Ok(Self {
            path,
            #[cfg(windows)]
            handle,
        })
    }
}

fn sha256_file(
    file: &mut File,
    path: &Path,
    expected_size: u64,
    budget: &ImportBudgetV1<'_>,
) -> Result<String, SePackImportErrorV1> {
    budget.check()?;
    file.seek(SeekFrom::Start(0))
        .map_err(|source| SePackImportErrorV1::Io {
            path: path.to_path_buf(),
            source,
        })?;
    let mut hasher = Sha256::new();
    let mut read_total = 0_u64;
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    loop {
        budget.check()?;
        let read = file
            .read(&mut buffer)
            .map_err(|source| SePackImportErrorV1::Io {
                path: path.to_path_buf(),
                source,
            })?;
        if read == 0 {
            break;
        }
        budget.check()?;
        read_total =
            read_total
                .checked_add(read as u64)
                .ok_or(SePackImportErrorV1::InvalidArchive {
                    reason: "archive byte count overflowed",
                })?;
        if read_total > expected_size {
            return Err(SePackImportErrorV1::InvalidArchive {
                reason: "archive changed while being imported",
            });
        }
        hasher.update(&buffer[..read]);
    }
    budget.check()?;
    if read_total != expected_size {
        return Err(SePackImportErrorV1::InvalidArchive {
            reason: "archive changed while being imported",
        });
    }
    file.seek(SeekFrom::Start(0))
        .map_err(|source| SePackImportErrorV1::Io {
            path: path.to_path_buf(),
            source,
        })?;
    Ok(hex_digest(&hasher.finalize()))
}

fn parse_canonical_zip(
    file: &mut File,
    path: &Path,
    archive_size: u64,
    budget: &ImportBudgetV1<'_>,
) -> Result<Vec<ZipEntry>, SePackImportErrorV1> {
    budget.check()?;
    if archive_size < 22 {
        return Err(SePackImportErrorV1::InvalidArchive {
            reason: "missing end of central directory",
        });
    }
    let tail_size = usize::try_from(archive_size.min(22 + u64::from(u16::MAX))).map_err(|_| {
        SePackImportErrorV1::InvalidArchive {
            reason: "archive tail length cannot be represented",
        }
    })?;
    file.seek(SeekFrom::End(
        -(i64::try_from(tail_size).map_err(|_| SePackImportErrorV1::InvalidArchive {
            reason: "archive tail offset cannot be represented",
        })?),
    ))
    .map_err(|source| SePackImportErrorV1::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut tail = vec![0_u8; tail_size];
    read_exact(file, &mut tail, path, budget)?;
    let eocd_index = tail
        .windows(4)
        .rposition(|bytes| read_u32(bytes) == Some(ZIP_END_OF_CENTRAL_DIRECTORY_SIGNATURE))
        .ok_or(SePackImportErrorV1::InvalidArchive {
            reason: "missing end of central directory",
        })?;
    let record =
        tail.get(eocd_index..eocd_index + 22)
            .ok_or(SePackImportErrorV1::InvalidArchive {
                reason: "truncated end of central directory",
            })?;
    if read_u16(&record[4..]) != Some(0)
        || read_u16(&record[6..]) != Some(0)
        || read_u16(&record[8..]) != read_u16(&record[10..])
        || read_u16(&record[20..]) != Some(0)
    {
        return Err(SePackImportErrorV1::InvalidArchive {
            reason: "multi-disk, ZIP64, or commented archives are unsupported",
        });
    }
    let entry_count = usize::from(read_u16(&record[10..]).unwrap_or_default());
    if entry_count > MAX_ENTRY_COUNT_V1 {
        return Err(SePackImportErrorV1::InvalidArchive {
            reason: "entry count exceeds the fixed import limit",
        });
    }
    let central_size = usize::try_from(u64::from(read_u32(&record[12..]).unwrap_or_default()))
        .map_err(|_| SePackImportErrorV1::InvalidArchive {
            reason: "central directory size cannot be represented",
        })?;
    if central_size > MAX_CENTRAL_DIRECTORY_BYTES_V1 {
        return Err(SePackImportErrorV1::InvalidArchive {
            reason: "central directory exceeds the fixed import limit",
        });
    }
    let central_offset = u64::from(read_u32(&record[16..]).unwrap_or_default());
    let eocd_offset = archive_size - u64::try_from(tail_size).unwrap_or_default()
        + u64::try_from(eocd_index).unwrap_or_default();
    if eocd_offset.checked_add(22) != Some(archive_size) {
        return Err(SePackImportErrorV1::InvalidArchive {
            reason: "archive has trailing bytes after the end record",
        });
    }
    if central_offset.checked_add(u64::try_from(central_size).unwrap_or(u64::MAX))
        != Some(eocd_offset)
    {
        return Err(SePackImportErrorV1::InvalidArchive {
            reason: "central directory does not immediately precede the end record",
        });
    }
    file.seek(SeekFrom::Start(central_offset))
        .map_err(|source| SePackImportErrorV1::Io {
            path: path.to_path_buf(),
            source,
        })?;
    let mut central = vec![0_u8; central_size];
    read_exact(file, &mut central, path, budget)?;
    parse_central_directory(&central, entry_count, central_offset, budget)
}

fn parse_central_directory(
    central: &[u8],
    entry_count: usize,
    central_offset: u64,
    budget: &ImportBudgetV1<'_>,
) -> Result<Vec<ZipEntry>, SePackImportErrorV1> {
    let mut position = 0_usize;
    let mut entries = Vec::with_capacity(entry_count);
    let mut folded_names = BTreeSet::new();
    let mut total_size = 0_u64;
    let mut expected_local_offset = 0_u64;
    for _ in 0..entry_count {
        budget.check()?;
        let header =
            central
                .get(position..position + 46)
                .ok_or(SePackImportErrorV1::InvalidArchive {
                    reason: "truncated central directory entry",
                })?;
        if read_u32(header) != Some(ZIP_CENTRAL_DIRECTORY_SIGNATURE) {
            return Err(SePackImportErrorV1::InvalidArchive {
                reason: "invalid central directory entry signature",
            });
        }
        let made_by = read_u16(&header[4..]).unwrap_or_default();
        let flags = read_u16(&header[8..]).unwrap_or_default();
        let method = read_u16(&header[10..]).unwrap_or_default();
        let crc32 = read_u32(&header[16..]).unwrap_or_default();
        let compressed_size = u64::from(read_u32(&header[20..]).unwrap_or_default());
        let size = u64::from(read_u32(&header[24..]).unwrap_or_default());
        let name_length = usize::from(read_u16(&header[28..]).unwrap_or_default());
        let extra_length = usize::from(read_u16(&header[30..]).unwrap_or_default());
        let comment_length = usize::from(read_u16(&header[32..]).unwrap_or_default());
        let disk_start = read_u16(&header[34..]).unwrap_or_default();
        let external_attributes = read_u32(&header[38..]).unwrap_or_default();
        let local_offset = u64::from(read_u32(&header[42..]).unwrap_or_default());
        if flags != ZIP_UTF8_FLAG
            || method != 0
            || compressed_size != size
            || extra_length != 0
            || comment_length != 0
            || disk_start != 0
        {
            return Err(SePackImportErrorV1::InvalidArchive {
                reason: "entries must be UTF-8, store-only, and have no extras or descriptors",
            });
        }
        let unix_mode = if (made_by >> 8) == 3 {
            (external_attributes >> 16) & 0o170_000
        } else {
            0
        };
        if matches!(unix_mode, 0o120_000 | 0o040_000) {
            return Err(SePackImportErrorV1::InvalidArchive {
                reason: "symlink or directory archive entries are forbidden",
            });
        }
        let entry_end = position
            .checked_add(46)
            .and_then(|value| value.checked_add(name_length))
            .and_then(|value| value.checked_add(extra_length))
            .and_then(|value| value.checked_add(comment_length))
            .ok_or(SePackImportErrorV1::InvalidArchive {
                reason: "central directory entry length overflowed",
            })?;
        let name_bytes = central
            .get(position + 46..position + 46 + name_length)
            .ok_or(SePackImportErrorV1::InvalidArchive {
                reason: "truncated central directory filename",
            })?;
        let name = std::str::from_utf8(name_bytes)
            .map_err(|_| SePackImportErrorV1::InvalidArchive {
                reason: "entry names must be UTF-8",
            })?
            .to_owned();
        validate_entry_name(&name)?;
        if !folded_names.insert(name.to_ascii_lowercase()) {
            return Err(SePackImportErrorV1::DuplicateEntry { entry: name });
        }
        if size > MAX_ENTRY_BYTES_V1 {
            return Err(SePackImportErrorV1::EntryTooLarge {
                entry: name,
                actual: size,
                maximum: MAX_ENTRY_BYTES_V1,
            });
        }
        total_size = total_size
            .checked_add(size)
            .ok_or(SePackImportErrorV1::InvalidArchive {
                reason: "total entry byte count overflowed",
            })?;
        let local_end = local_offset
            .checked_add(30)
            .and_then(|value| value.checked_add(u64::try_from(name_length).ok()?))
            .and_then(|value| value.checked_add(size))
            .ok_or(SePackImportErrorV1::InvalidArchive {
                reason: "local entry byte range overflowed",
            })?;
        if total_size > MAX_ARCHIVE_BYTES_V1
            || local_offset != expected_local_offset
            || local_end > central_offset
        {
            return Err(SePackImportErrorV1::InvalidArchive {
                reason: "local entry data has a prefix, gap, overlap, or escapes the fixed import region",
            });
        }
        expected_local_offset = local_end;
        entries.push(ZipEntry {
            name,
            crc32,
            size,
            local_offset,
        });
        position = entry_end;
    }
    if position != central.len() {
        return Err(SePackImportErrorV1::InvalidArchive {
            reason: "central directory has trailing data",
        });
    }
    if expected_local_offset != central_offset {
        return Err(SePackImportErrorV1::InvalidArchive {
            reason: "local entry data does not exactly precede the central directory",
        });
    }
    for required in ["manifest.json", "plugin/plugin.dll"] {
        if !folded_names.contains(required) {
            return Err(SePackImportErrorV1::MissingRuntimeEntry { entry: required });
        }
    }
    Ok(entries)
}

fn validate_entry_name(name: &str) -> Result<(), SePackImportErrorV1> {
    if name.len() > MAX_ENTRY_NAME_BYTES_V1
        || !name.is_ascii()
        || name.starts_with('/')
        || name.contains('\\')
        || name.contains(':')
        || name.split('/').count() > MAX_ENTRY_DEPTH_V1
        || name.split('/').any(|segment| {
            segment.is_empty()
                || matches!(segment, "." | "..")
                || segment.ends_with([' ', '.'])
                || segment.bytes().any(|byte| byte.is_ascii_control())
        })
    {
        return Err(SePackImportErrorV1::InvalidEntryName {
            entry: name.to_owned(),
        });
    }
    Ok(())
}

fn extract_entries(
    archive: &mut File,
    archive_path: &Path,
    entries: &[ZipEntry],
    staging: &mut StagingDirectory,
    budget: &ImportBudgetV1<'_>,
) -> Result<(), SePackImportErrorV1> {
    for entry in entries {
        budget.check()?;
        archive
            .seek(SeekFrom::Start(entry.local_offset))
            .map_err(|source| SePackImportErrorV1::Io {
                path: archive_path.to_path_buf(),
                source,
            })?;
        let mut header = [0_u8; 30];
        read_exact(archive, &mut header, archive_path, budget)?;
        if read_u32(&header) != Some(ZIP_LOCAL_FILE_SIGNATURE)
            || read_u16(&header[6..]) != Some(ZIP_UTF8_FLAG)
            || read_u16(&header[8..]) != Some(0)
            || u64::from(read_u32(&header[18..]).unwrap_or_default()) != entry.size
            || u64::from(read_u32(&header[22..]).unwrap_or_default()) != entry.size
            || read_u32(&header[14..]) != Some(entry.crc32)
        {
            return Err(SePackImportErrorV1::InvalidArchive {
                reason: "local entry disagrees with canonical central directory",
            });
        }
        let name_length = usize::from(read_u16(&header[26..]).unwrap_or_default());
        let extra_length = usize::from(read_u16(&header[28..]).unwrap_or_default());
        if extra_length != 0 || name_length > MAX_ENTRY_NAME_BYTES_V1 {
            return Err(SePackImportErrorV1::InvalidArchive {
                reason: "local entry has unsupported extra data",
            });
        }
        let mut name = vec![0_u8; name_length];
        read_exact(archive, &mut name, archive_path, budget)?;
        if std::str::from_utf8(&name).ok() != Some(entry.name.as_str()) {
            return Err(SePackImportErrorV1::InvalidArchive {
                reason: "local entry name disagrees with central directory",
            });
        }
        let (destination, mut output) = staging.create_output(&entry.name)?;
        copy_stored_entry(archive, archive_path, &mut output, entry, budget)?;
        budget.check()?;
        output
            .sync_all()
            .map_err(|source| SePackImportErrorV1::Io {
                path: destination,
                source,
            })?;
        budget.check()?;
    }
    Ok(())
}

#[cfg(not(windows))]
fn stage_destination(staging: &Path, name: &str) -> Result<PathBuf, SePackImportErrorV1> {
    let destination = name
        .split('/')
        .fold(staging.to_path_buf(), |path, component| {
            path.join(component)
        });
    if destination.parent().is_none() || !destination.starts_with(staging) {
        return Err(SePackImportErrorV1::InvalidEntryName {
            entry: name.to_owned(),
        });
    }
    Ok(destination)
}

fn copy_stored_entry(
    archive: &mut File,
    archive_path: &Path,
    output: &mut File,
    entry: &ZipEntry,
    budget: &ImportBudgetV1<'_>,
) -> Result<(), SePackImportErrorV1> {
    let mut remaining = entry.size;
    let mut crc = Crc32::new();
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    while remaining > 0 {
        budget.check()?;
        let wanted = usize::try_from(remaining.min(buffer.len() as u64)).map_err(|_| {
            SePackImportErrorV1::InvalidArchive {
                reason: "entry byte count cannot be represented",
            }
        })?;
        let read =
            archive
                .read(&mut buffer[..wanted])
                .map_err(|source| SePackImportErrorV1::Io {
                    path: archive_path.to_path_buf(),
                    source,
                })?;
        if read == 0 {
            return Err(SePackImportErrorV1::InvalidArchive {
                reason: "stored entry is truncated",
            });
        }
        budget.check()?;
        output
            .write_all(&buffer[..read])
            .map_err(|source| SePackImportErrorV1::Io {
                path: archive_path.to_path_buf(),
                source,
            })?;
        crc.update(&buffer[..read]);
        remaining -= read as u64;
    }
    budget.check()?;
    if crc.finish() != entry.crc32 {
        return Err(SePackImportErrorV1::CrcMismatch {
            entry: entry.name.clone(),
        });
    }
    Ok(())
}

struct StagingDirectory {
    path: PathBuf,
    armed: bool,
    #[cfg(windows)]
    root_handle: DirectoryHandleV1,
    #[cfg(windows)]
    directory_handles: BTreeMap<String, DirectoryHandleV1>,
    #[cfg(windows)]
    files: Vec<(Option<String>, String)>,
}

/// Best-effort cleanup for staging left by a dead process. The staging namespace
/// is never discoverable or loadable, so this intentionally skips anything that
/// cannot be proven old, bounded, and free of reparse points.
fn scavenge_stale_staging_directories(staging_root: &Path) {
    let deadline = Instant::now() + STAGING_SCAVENGE_TIMEOUT_V1;
    scavenge_stale_staging_directories_with_limits(
        staging_root,
        MAX_STAGING_ROOT_ENTRY_SCAN_V1,
        deadline,
        MINIMUM_STAGING_AGE_V1,
    );
}

fn scavenge_stale_staging_directories_with_limits(
    staging_root: &Path,
    root_entry_limit: usize,
    deadline: Instant,
    minimum_age: Duration,
) {
    let Ok(entries) = fs::read_dir(staging_root) else {
        return;
    };
    for (index, entry) in entries.enumerate() {
        if index >= root_entry_limit || Instant::now() >= deadline {
            return;
        }
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let Some(owner_pid) = staging_owner_pid(&name) else {
            continue;
        };
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            continue;
        };
        if metadata_is_reparse_point(&metadata)
            || !metadata.is_dir()
            || !staging_is_old_enough(&metadata, minimum_age)
            || staging_owner_is_active(owner_pid)
        {
            continue;
        }
        if !matches!(staging_tree_is_safe(&path, deadline), Ok(true)) {
            continue;
        }
        let _ = fs::remove_dir_all(path);
    }
}

fn staging_owner_pid(name: &str) -> Option<u32> {
    let (pid, nonce) = name.split_once('-')?;
    if nonce.len() != 32 || !nonce.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    pid.parse().ok()
}

fn staging_is_old_enough(metadata: &fs::Metadata, minimum_age: Duration) -> bool {
    metadata
        .modified()
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .is_some_and(|age| age >= minimum_age)
}

fn staging_owner_is_active(pid: u32) -> bool {
    if pid == std::process::id() {
        return true;
    }
    #[cfg(windows)]
    {
        process_is_running(pid)
    }
    #[cfg(not(windows))]
    {
        // Only Windows owns this production namespace. Non-Windows test builds
        // use the conservative age boundary but do not need a platform process
        // query to exercise bounded traversal.
        let _ = pid;
        false
    }
}

#[cfg(windows)]
#[allow(unsafe_code, reason = "Windows has no safe process-liveness API")]
fn process_is_running(pid: u32) -> bool {
    const SYNCHRONIZE: u32 = 0x0010_0000;
    const WAIT_TIMEOUT: u32 = 258;
    let handle = unsafe { open_process(SYNCHRONIZE, 0, pid) };
    if handle == 0 {
        // ERROR_INVALID_PARAMETER means the PID is absent. Access denial and
        // every other error are treated as active so cleanup remains fail-closed.
        return io::Error::last_os_error().raw_os_error() != Some(87);
    }
    // SAFETY: `handle` came from OpenProcess and is closed exactly once.
    let running = unsafe { wait_for_single_object(handle, 0) == WAIT_TIMEOUT };
    unsafe {
        let _ = close_handle(handle);
    }
    running
}

#[cfg(windows)]
#[allow(unsafe_code, reason = "Windows process handles require direct FFI")]
#[link(name = "kernel32")]
unsafe extern "system" {
    #[link_name = "OpenProcess"]
    fn open_process(desired_access: u32, inherit_handle: i32, process_id: u32) -> isize;
    #[link_name = "WaitForSingleObject"]
    fn wait_for_single_object(handle: isize, milliseconds: u32) -> u32;
    #[link_name = "CloseHandle"]
    fn close_handle(handle: isize) -> i32;
}

fn staging_tree_is_safe(root: &Path, deadline: Instant) -> Result<bool, SePackImportErrorV1> {
    let mut pending = vec![(root.to_path_buf(), 0_usize)];
    let mut entries_seen = 0_usize;
    while let Some((directory, depth)) = pending.pop() {
        if Instant::now() >= deadline {
            return Ok(false);
        }
        for entry in fs::read_dir(&directory).map_err(|source| SePackImportErrorV1::Io {
            path: directory.clone(),
            source,
        })? {
            if Instant::now() >= deadline {
                return Ok(false);
            }
            entries_seen = match entries_seen.checked_add(1) {
                Some(value) if value <= MAX_STAGING_ENTRY_COUNT_V1 => value,
                _ => return Ok(false),
            };
            let entry = entry.map_err(|source| SePackImportErrorV1::Io {
                path: directory.clone(),
                source,
            })?;
            let path = entry.path();
            let metadata =
                fs::symlink_metadata(&path).map_err(|source| SePackImportErrorV1::Io {
                    path: path.clone(),
                    source,
                })?;
            if metadata_is_reparse_point(&metadata) {
                return Ok(false);
            }
            if metadata.is_dir() {
                let Some(next_depth) = depth.checked_add(1) else {
                    return Ok(false);
                };
                if next_depth > MAX_ENTRY_DEPTH_V1 {
                    return Ok(false);
                }
                pending.push((path, next_depth));
            } else if !metadata.is_file() {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

#[cfg(windows)]
struct DirectoryHandleV1 {
    handle: isize,
    identity: FileIdentityV1,
}

#[cfg(windows)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FileIdentityV1 {
    volume_serial_number: u32,
    file_index: u64,
}

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "owned Windows handles require direct conversion and cleanup"
)]
impl DirectoryHandleV1 {
    fn open_path(path: &Path) -> Result<Self, SePackImportErrorV1> {
        let handle = open_directory_path_handle(path)?;
        Self::from_directory_handle(handle, path)
    }

    fn create_relative(
        parent: &Self,
        name: &str,
        path: &Path,
    ) -> Result<Self, SePackImportErrorV1> {
        let handle = nt_create_relative(parent.raw(), name, NtCreateKindV1::CreateDirectory, path)?;
        Self::from_directory_handle(handle, path)
    }

    fn open_or_create_relative(
        parent: isize,
        name: &str,
        path: &Path,
    ) -> Result<Self, SePackImportErrorV1> {
        let handle = nt_create_relative(parent, name, NtCreateKindV1::OpenIfDirectory, path)?;
        Self::from_directory_handle(handle, path)
    }

    fn create_file_relative(
        parent: isize,
        name: &str,
        path: &Path,
    ) -> Result<File, SePackImportErrorV1> {
        let handle = nt_create_relative(parent, name, NtCreateKindV1::CreateFile, path)?;
        if let Err(error) = verify_file_handle(handle, path, false) {
            // SAFETY: this function owns `handle` until conversion into `File`.
            unsafe {
                let _ = close_handle_v1(handle);
            }
            return Err(error);
        }
        // SAFETY: `NtCreateFile` returned a fresh owned synchronous file handle.
        Ok(unsafe {
            std::os::windows::io::FromRawHandle::from_raw_handle(
                handle as std::os::windows::io::RawHandle,
            )
        })
    }

    fn delete_file_relative(
        parent: isize,
        name: &str,
        path: &Path,
    ) -> Result<(), SePackImportErrorV1> {
        let handle = nt_create_relative(parent, name, NtCreateKindV1::DeleteFile, path)?;
        // SAFETY: DeleteFile opens the entry with FILE_DELETE_ON_CLOSE.
        unsafe {
            let _ = close_handle_v1(handle);
        }
        Ok(())
    }

    const fn raw(&self) -> isize {
        self.handle
    }

    fn from_directory_handle(handle: isize, path: &Path) -> Result<Self, SePackImportErrorV1> {
        match verify_file_handle(handle, path, true) {
            Ok(identity) => Ok(Self { handle, identity }),
            Err(error) => {
                // SAFETY: this function owns `handle` until it returns it in Self.
                unsafe {
                    let _ = close_handle_v1(handle);
                }
                Err(error)
            }
        }
    }

    fn verify_directory_identity(&self, path: &Path) -> Result<(), SePackImportErrorV1> {
        let current = verify_file_handle(self.handle, path, true)?;
        if current != self.identity {
            return Err(SePackImportErrorV1::UnsafeSourceRoot {
                path: path.to_path_buf(),
            });
        }
        Ok(())
    }

    fn sync(&self, path: &Path) -> Result<(), SePackImportErrorV1> {
        // SAFETY: the handle is a live directory handle owned by this lease.
        if unsafe { flush_file_buffers_v1(self.handle) } == 0 {
            return Err(SePackImportErrorV1::Io {
                path: path.to_path_buf(),
                source: io::Error::last_os_error(),
            });
        }
        Ok(())
    }

    fn mark_delete_on_close(&mut self, path: &Path) -> Result<(), SePackImportErrorV1> {
        #[repr(C)]
        struct IoStatusBlock {
            status: isize,
            information: usize,
        }
        #[repr(C)]
        struct FileDispositionInformation {
            delete_file: u8,
        }
        const FILE_DISPOSITION_INFORMATION: u32 = 13;
        let mut io_status = IoStatusBlock {
            status: 0,
            information: 0,
        };
        let mut disposition = FileDispositionInformation { delete_file: 1 };
        // SAFETY: the handle is owned, and both output structures outlive the synchronous call.
        let status = unsafe {
            nt_set_information_file_v1(
                self.handle,
                std::ptr::from_mut(&mut io_status).cast(),
                std::ptr::from_mut(&mut disposition).cast(),
                u32::try_from(size_of::<FileDispositionInformation>()).unwrap_or(u32::MAX),
                FILE_DISPOSITION_INFORMATION,
            )
        };
        if status < 0 {
            return Err(SePackImportErrorV1::Io {
                path: path.to_path_buf(),
                source: io::Error::other(format!(
                    "NtSetInformationFile(FileDispositionInformation) failed with NTSTATUS {status:#x}"
                )),
            });
        }
        Ok(())
    }
}

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "owned Windows directory handle must close exactly once"
)]
impl Drop for DirectoryHandleV1 {
    fn drop(&mut self) {
        // SAFETY: `handle` was returned by CreateFileW/NtCreateFile and is owned once.
        unsafe {
            let _ = close_handle_v1(self.handle);
        }
    }
}

#[cfg(windows)]
#[derive(Clone, Copy)]
enum NtCreateKindV1 {
    CreateDirectory,
    OpenIfDirectory,
    CreateFile,
    DeleteFile,
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn open_directory_path_handle(path: &Path) -> Result<isize, SePackImportErrorV1> {
    use std::{ffi::c_void, iter, os::windows::ffi::OsStrExt as _};

    const FILE_LIST_DIRECTORY: u32 = 0x0000_0001;
    const FILE_ADD_FILE: u32 = 0x0000_0002;
    const FILE_ADD_SUBDIRECTORY: u32 = 0x0000_0004;
    const FILE_READ_ATTRIBUTES: u32 = 0x0000_0080;
    const SYNCHRONIZE: u32 = 0x0010_0000;
    const FILE_SHARE_READ: u32 = 0x0000_0001;
    const OPEN_EXISTING: u32 = 3;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const INVALID_HANDLE_VALUE: isize = -1;
    let wide_path: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(iter::once(0))
        .collect();
    // SAFETY: the path is NUL terminated and all optional pointers are null.
    let handle = unsafe {
        create_file_w_v1(
            wide_path.as_ptr(),
            FILE_LIST_DIRECTORY
                | FILE_ADD_FILE
                | FILE_ADD_SUBDIRECTORY
                | FILE_READ_ATTRIBUTES
                | SYNCHRONIZE,
            FILE_SHARE_READ,
            std::ptr::null_mut::<c_void>(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            0,
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(SePackImportErrorV1::Io {
            path: path.to_path_buf(),
            source: io::Error::last_os_error(),
        });
    }
    Ok(handle)
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn nt_create_relative(
    parent: isize,
    name: &str,
    kind: NtCreateKindV1,
    path: &Path,
) -> Result<isize, SePackImportErrorV1> {
    use std::{mem::size_of, os::windows::ffi::OsStrExt as _};

    const FILE_LIST_DIRECTORY: u32 = 0x0000_0001;
    const FILE_ADD_FILE: u32 = 0x0000_0002;
    const FILE_ADD_SUBDIRECTORY: u32 = 0x0000_0004;
    const FILE_WRITE_DATA: u32 = 0x0000_0002;
    const FILE_READ_ATTRIBUTES: u32 = 0x0000_0080;
    const FILE_WRITE_ATTRIBUTES: u32 = 0x0000_0100;
    const DELETE: u32 = 0x0001_0000;
    const SYNCHRONIZE: u32 = 0x0010_0000;
    const FILE_SHARE_READ: u32 = 0x0000_0001;
    const FILE_ATTRIBUTE_NORMAL: u32 = 0x0000_0080;
    const FILE_CREATE: u32 = 2;
    const FILE_OPEN_IF: u32 = 3;
    const FILE_DIRECTORY_FILE: u32 = 0x0000_0001;
    const FILE_NON_DIRECTORY_FILE: u32 = 0x0000_0040;
    const FILE_SYNCHRONOUS_IO_NONALERT: u32 = 0x0000_0020;
    const FILE_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const FILE_DELETE_ON_CLOSE: u32 = 0x0000_1000;
    const OBJ_CASE_INSENSITIVE: u32 = 0x0000_0040;
    const OBJ_DONT_REPARSE: u32 = 0x0000_1000;
    const STATUS_OBJECT_NAME_COLLISION: i32 = 0xc000_0035_u32.cast_signed();

    #[repr(C)]
    struct UnicodeString {
        length: u16,
        maximum_length: u16,
        buffer: *mut u16,
    }
    #[repr(C)]
    struct ObjectAttributes {
        length: u32,
        root_directory: isize,
        object_name: *mut UnicodeString,
        attributes: u32,
        security_descriptor: *mut std::ffi::c_void,
        security_quality_of_service: *mut std::ffi::c_void,
    }
    #[repr(C)]
    struct IoStatusBlock {
        status: isize,
        information: usize,
    }

    let mut wide_name: Vec<u16> = std::ffi::OsStr::new(name).encode_wide().collect();
    let byte_length = u16::try_from(wide_name.len().saturating_mul(2)).map_err(|_| {
        SePackImportErrorV1::InvalidArchive {
            reason: "staging entry name cannot be represented by Windows",
        }
    })?;
    let mut unicode_name = UnicodeString {
        length: byte_length,
        maximum_length: byte_length,
        buffer: wide_name.as_mut_ptr(),
    };
    let mut object_attributes = ObjectAttributes {
        length: u32::try_from(size_of::<ObjectAttributes>()).unwrap_or(u32::MAX),
        root_directory: parent,
        object_name: &raw mut unicode_name,
        attributes: OBJ_CASE_INSENSITIVE | OBJ_DONT_REPARSE,
        security_descriptor: std::ptr::null_mut(),
        security_quality_of_service: std::ptr::null_mut(),
    };
    let (desired_access, disposition, options) = match kind {
        NtCreateKindV1::CreateDirectory => (
            FILE_LIST_DIRECTORY
                | FILE_ADD_FILE
                | FILE_ADD_SUBDIRECTORY
                | FILE_READ_ATTRIBUTES
                | DELETE
                | SYNCHRONIZE,
            FILE_CREATE,
            FILE_DIRECTORY_FILE | FILE_SYNCHRONOUS_IO_NONALERT | FILE_OPEN_REPARSE_POINT,
        ),
        NtCreateKindV1::OpenIfDirectory => (
            FILE_LIST_DIRECTORY
                | FILE_ADD_FILE
                | FILE_ADD_SUBDIRECTORY
                | FILE_READ_ATTRIBUTES
                | DELETE
                | SYNCHRONIZE,
            FILE_OPEN_IF,
            FILE_DIRECTORY_FILE | FILE_SYNCHRONOUS_IO_NONALERT | FILE_OPEN_REPARSE_POINT,
        ),
        NtCreateKindV1::CreateFile => (
            FILE_WRITE_DATA | FILE_READ_ATTRIBUTES | FILE_WRITE_ATTRIBUTES | SYNCHRONIZE,
            FILE_CREATE,
            FILE_NON_DIRECTORY_FILE | FILE_SYNCHRONOUS_IO_NONALERT | FILE_OPEN_REPARSE_POINT,
        ),
        NtCreateKindV1::DeleteFile => (
            DELETE | SYNCHRONIZE,
            1,
            FILE_NON_DIRECTORY_FILE
                | FILE_SYNCHRONOUS_IO_NONALERT
                | FILE_OPEN_REPARSE_POINT
                | FILE_DELETE_ON_CLOSE,
        ),
    };
    let mut handle = 0_isize;
    let mut io_status = IoStatusBlock {
        status: 0,
        information: 0,
    };
    // SAFETY: structures and name buffer outlive the synchronous native call.
    let status = unsafe {
        nt_create_file_v1(
            &raw mut handle,
            desired_access,
            std::ptr::from_mut(&mut object_attributes).cast(),
            std::ptr::from_mut(&mut io_status).cast(),
            std::ptr::null_mut(),
            FILE_ATTRIBUTE_NORMAL,
            FILE_SHARE_READ,
            disposition,
            options,
            std::ptr::null_mut(),
            0,
        )
    };
    if status < 0 {
        let source = if status == STATUS_OBJECT_NAME_COLLISION {
            io::Error::from(io::ErrorKind::AlreadyExists)
        } else {
            io::Error::other(format!("NtCreateFile failed with NTSTATUS {status:#x}"))
        };
        return Err(SePackImportErrorV1::Io {
            path: path.to_path_buf(),
            source,
        });
    }
    Ok(handle)
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn verify_file_handle(
    handle: isize,
    path: &Path,
    expected_directory: bool,
) -> Result<FileIdentityV1, SePackImportErrorV1> {
    const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x0000_0010;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    #[repr(C)]
    struct FileTime {
        low_date_time: u32,
        high_date_time: u32,
    }
    #[repr(C)]
    struct ByHandleFileInformation {
        file_attributes: u32,
        creation_time: FileTime,
        last_access_time: FileTime,
        last_write_time: FileTime,
        volume_serial_number: u32,
        file_size_high: u32,
        file_size_low: u32,
        number_of_links: u32,
        file_index_high: u32,
        file_index_low: u32,
    }
    let mut info = std::mem::MaybeUninit::<ByHandleFileInformation>::uninit();
    // SAFETY: `info` is writable storage for the Win32 result.
    if unsafe { get_file_information_by_handle_v1(handle, info.as_mut_ptr().cast()) } == 0 {
        return Err(SePackImportErrorV1::Io {
            path: path.to_path_buf(),
            source: io::Error::last_os_error(),
        });
    }
    // SAFETY: the API returned success and initialized `info`.
    let info = unsafe { info.assume_init() };
    if (info.file_attributes & FILE_ATTRIBUTE_DIRECTORY != 0) != expected_directory
        || info.file_attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
    {
        return Err(SePackImportErrorV1::UnsafeSourceRoot {
            path: path.to_path_buf(),
        });
    }
    Ok(FileIdentityV1 {
        volume_serial_number: info.volume_serial_number,
        file_index: u64::from(info.file_index_high) << 32 | u64::from(info.file_index_low),
    })
}

#[cfg(windows)]
#[allow(unsafe_code)]
#[link(name = "kernel32")]
unsafe extern "system" {
    #[link_name = "CreateFileW"]
    fn create_file_w_v1(
        file_name: *const u16,
        desired_access: u32,
        share_mode: u32,
        security_attributes: *mut std::ffi::c_void,
        creation_disposition: u32,
        flags_and_attributes: u32,
        template_file: isize,
    ) -> isize;
    #[link_name = "CloseHandle"]
    fn close_handle_v1(handle: isize) -> i32;
    #[link_name = "GetFileInformationByHandle"]
    fn get_file_information_by_handle_v1(handle: isize, info: *mut std::ffi::c_void) -> i32;
    #[link_name = "FlushFileBuffers"]
    fn flush_file_buffers_v1(handle: isize) -> i32;
}

#[cfg(windows)]
#[allow(unsafe_code)]
#[link(name = "ntdll")]
unsafe extern "system" {
    #[link_name = "NtCreateFile"]
    fn nt_create_file_v1(
        file_handle: *mut isize,
        desired_access: u32,
        object_attributes: *mut std::ffi::c_void,
        io_status_block: *mut std::ffi::c_void,
        allocation_size: *mut i64,
        file_attributes: u32,
        share_access: u32,
        create_disposition: u32,
        create_options: u32,
        ea_buffer: *mut std::ffi::c_void,
        ea_length: u32,
    ) -> i32;
    #[link_name = "NtSetInformationFile"]
    fn nt_set_information_file_v1(
        file_handle: isize,
        io_status_block: *mut std::ffi::c_void,
        file_information: *mut std::ffi::c_void,
        length: u32,
        file_information_class: u32,
    ) -> i32;
}

#[cfg(test)]
fn forced_cleanup_failure(path: &Path) -> bool {
    let mut failures = FORCED_CLEANUP_FAILURES_V1
        .lock()
        .expect("test cleanup-failure set lock");
    failures.remove(path)
}

impl StagingDirectory {
    fn new(root: &StagingRootV1) -> Result<Self, SePackImportErrorV1> {
        for _ in 0..32 {
            let leaf = random_staging_leaf()?;
            let path = root.path.join(&leaf);
            #[cfg(windows)]
            match DirectoryHandleV1::create_relative(&root.handle, &leaf, &path) {
                Ok(handle) => {
                    return Ok(Self {
                        path,
                        armed: true,
                        root_handle: handle,
                        directory_handles: BTreeMap::new(),
                        files: Vec::new(),
                    });
                }
                Err(SePackImportErrorV1::Io { source, .. })
                    if source.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
            #[cfg(not(windows))]
            match fs::create_dir(&path) {
                Ok(()) => {
                    verify_safe_directory(&path)?;
                    return Ok(Self { path, armed: true });
                }
                Err(source) if source.kind() == io::ErrorKind::AlreadyExists => {}
                Err(source) => return Err(SePackImportErrorV1::Io { path, source }),
            }
        }
        Err(SePackImportErrorV1::InvalidArchive {
            reason: "could not allocate private import staging",
        })
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn verify_identity(&self) -> Result<(), SePackImportErrorV1> {
        #[cfg(windows)]
        {
            self.root_handle.verify_directory_identity(&self.path)?;
            Ok(())
        }
        #[cfg(not(windows))]
        verify_safe_directory(&self.path).map(|_| ())
    }

    fn sync_all(&self, budget: &ImportBudgetV1<'_>) -> Result<(), SePackImportErrorV1> {
        budget.check()?;
        #[cfg(windows)]
        {
            self.root_handle.sync(&self.path)?;
            for (relative, handle) in &self.directory_handles {
                budget.check()?;
                handle.sync(&self.path.join(relative))?;
            }
        }
        #[cfg(not(windows))]
        {
            let directory = File::open(&self.path).map_err(|source| SePackImportErrorV1::Io {
                path: self.path.clone(),
                source,
            })?;
            directory
                .sync_all()
                .map_err(|source| SePackImportErrorV1::Io {
                    path: self.path.clone(),
                    source,
                })?;
        }
        budget.check()
    }

    #[cfg(windows)]
    fn create_output(&mut self, name: &str) -> Result<(PathBuf, File), SePackImportErrorV1> {
        let components: Vec<&str> = name.split('/').collect();
        let (file_name, directories) =
            components
                .split_last()
                .ok_or(SePackImportErrorV1::InvalidEntryName {
                    entry: name.to_owned(),
                })?;
        let mut parent_key = String::new();
        let mut current_handle = self.root_handle.raw();
        let mut current_path = self.path.clone();
        for directory in directories {
            current_path.push(directory);
            let key = if parent_key.is_empty() {
                (*directory).to_owned()
            } else {
                format!("{parent_key}/{directory}")
            };
            if !self.directory_handles.contains_key(&key) {
                let handle = DirectoryHandleV1::open_or_create_relative(
                    current_handle,
                    directory,
                    &current_path,
                )?;
                self.directory_handles.insert(key.clone(), handle);
            }
            current_handle = self
                .directory_handles
                .get(&key)
                .ok_or(SePackImportErrorV1::InvalidArchive {
                    reason: "private staging directory handle cache was lost",
                })?
                .raw();
            parent_key = key;
        }
        let destination = current_path.join(file_name);
        let output =
            DirectoryHandleV1::create_file_relative(current_handle, file_name, &destination)?;
        self.files.push((
            (!parent_key.is_empty()).then_some(parent_key),
            (*file_name).to_owned(),
        ));
        Ok((destination, output))
    }

    #[cfg(windows)]
    fn cleanup_windows(&mut self) -> Result<(), SePackImportErrorV1> {
        for (parent_key, file_name) in self.files.drain(..).rev() {
            let parent = match parent_key.as_deref() {
                Some(key) => self
                    .directory_handles
                    .get(key)
                    .ok_or(SePackImportErrorV1::InvalidArchive {
                        reason: "private staging file parent handle cache was lost",
                    })?
                    .raw(),
                None => self.root_handle.raw(),
            };
            DirectoryHandleV1::delete_file_relative(parent, &file_name, &self.path)?;
        }
        let mut directories: Vec<String> = self.directory_handles.keys().cloned().collect();
        directories.sort_by_key(|path| std::cmp::Reverse(path.matches('/').count()));
        for relative in directories {
            let mut handle = self.directory_handles.remove(&relative).ok_or(
                SePackImportErrorV1::InvalidArchive {
                    reason: "private staging directory handle cache was lost",
                },
            )?;
            handle.mark_delete_on_close(&self.path.join(&relative))?;
            drop(handle);
        }
        self.root_handle.mark_delete_on_close(&self.path)
    }

    #[cfg(not(windows))]
    fn create_output(&mut self, name: &str) -> Result<(PathBuf, File), SePackImportErrorV1> {
        let destination = stage_destination(&self.path, name)?;
        let parent = destination
            .parent()
            .ok_or(SePackImportErrorV1::InvalidArchive {
                reason: "entry has no destination parent",
            })?;
        fs::create_dir_all(parent).map_err(|source| SePackImportErrorV1::Io {
            path: parent.to_path_buf(),
            source,
        })?;
        verify_safe_directory(parent)?;
        let output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination)
            .map_err(|source| SePackImportErrorV1::Io {
                path: destination.clone(),
                source,
            })?;
        Ok((destination, output))
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

fn random_staging_leaf() -> Result<String, SePackImportErrorV1> {
    let mut nonce = [0_u8; 16];
    SystemRandom::new()
        .fill(&mut nonce)
        .map_err(|_| SePackImportErrorV1::InvalidArchive {
            reason: "operating-system entropy is unavailable for private import staging",
        })?;
    Ok(format!("{}-{}", std::process::id(), hex_digest(&nonce)))
}

impl Drop for StagingDirectory {
    fn drop(&mut self) {
        if self.armed {
            #[cfg(windows)]
            {
                let _ = self.cleanup_windows();
            }
            #[cfg(not(windows))]
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

struct Crc32(u32);

impl Crc32 {
    const fn new() -> Self {
        Self(0xffff_ffff)
    }

    fn update(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u32::from(*byte);
            for _ in 0..8 {
                self.0 = if self.0 & 1 == 0 {
                    self.0 >> 1
                } else {
                    (self.0 >> 1) ^ 0xedb8_8320
                };
            }
        }
    }

    const fn finish(self) -> u32 {
        !self.0
    }
}

fn read_exact(
    file: &mut File,
    bytes: &mut [u8],
    path: &Path,
    budget: &ImportBudgetV1<'_>,
) -> Result<(), SePackImportErrorV1> {
    budget.check()?;
    file.read_exact(bytes)
        .map_err(|source| SePackImportErrorV1::Io {
            path: path.to_path_buf(),
            source,
        })?;
    budget.check()
}

fn read_u16(bytes: &[u8]) -> Option<u16> {
    bytes
        .get(..2)
        .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_u32(bytes: &[u8]) -> Option<u32> {
    bytes
        .get(..4)
        .map(|bytes| u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn hex_digest(digest: &[u8]) -> String {
    let mut value = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut value, "{byte:02x}");
    }
    value
}

#[cfg(test)]
mod tests {
    use std::{
        fmt::Write as _,
        fs,
        time::{Duration, Instant},
    };

    use serde_json::json;
    use sha2::{Digest as _, Sha256};
    use tempfile::TempDir;

    use super::{
        Crc32, ImportBudgetV1, ImportedSePackV1, SePackImportErrorV1, SePackImporterV1,
        ZIP_CENTRAL_DIRECTORY_SIGNATURE, ZIP_END_OF_CENTRAL_DIRECTORY_SIGNATURE,
        ZIP_LOCAL_FILE_SIGNATURE, ZIP_UTF8_FLAG,
    };
    use crate::{
        LocalDeveloperPackageSourceV1, PackageSourceV1, PackageValidationCancellationV1,
        PackageValidationRequestV1, PackageValidatorV1, SealedPackageStoreV1,
        TrustedPublisherKeyStoreV1,
        package_source::{LocalDeveloperPackageStoreErrorV1, LocalDeveloperPackageStoreV1},
        package_validation::LocalDeveloperAuthorizationV1,
    };

    fn package_manifest(payload: &[u8]) -> Vec<u8> {
        let mut hash = String::new();
        for byte in Sha256::digest(payload) {
            write!(&mut hash, "{byte:02x}").expect("write digest");
        }
        json!({
            "manifest_version": 1,
            "package": { "id": "example.sepack", "version": "1.0.0" },
            "publisher": {
                "id": "example.publisher", "display_name": "Example Publisher",
                "contacts": [{ "kind": "email", "value": "support@example.invalid", "purposes": ["support"] }]
            },
            "sdk": {
                "bundle_id": "dev.20260802", "target": "x86_64-pc-windows-msvc",
                "abi_schema": 1, "gpui": false, "ui_abi_fingerprint": null
            },
            "rust": [{ "id": "example.plugin", "entrypoint": "plugin/plugin.dll", "root_contract_id": { "namespace": 1_397_030_913, "value": 1 }, "sdk_major": 1 }],
            "lua": [], "skins": [], "locales": [], "tools": [],
            "features": [{ "id": "main", "capabilities": ["abi"], "dependencies": [] }],
            "dependencies": [],
            "payloads": [{ "path": "plugin/plugin.dll", "size": payload.len(), "sha256": hash, "kind": "rust_dll" }],
            "signature": { "kind": "unsigned" }, "data_version": 1
        })
        .to_string()
        .into_bytes()
    }

    fn push_u16(bytes: &mut Vec<u8>, value: u16) {
        bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn push_u32(bytes: &mut Vec<u8>, value: u32) {
        bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn stored_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut archive = Vec::new();
        let mut central = Vec::new();
        for (name, content) in entries {
            let offset = u32::try_from(archive.len()).expect("test archive offset");
            let mut crc = Crc32::new();
            crc.update(content);
            let crc = crc.finish();
            let name = name.as_bytes();
            let length = u32::try_from(content.len()).expect("test entry size");
            push_u32(&mut archive, ZIP_LOCAL_FILE_SIGNATURE);
            push_u16(&mut archive, 20);
            push_u16(&mut archive, ZIP_UTF8_FLAG);
            push_u16(&mut archive, 0);
            push_u16(&mut archive, 0);
            push_u16(&mut archive, 0);
            push_u32(&mut archive, crc);
            push_u32(&mut archive, length);
            push_u32(&mut archive, length);
            push_u16(
                &mut archive,
                u16::try_from(name.len()).expect("test name size"),
            );
            push_u16(&mut archive, 0);
            archive.extend_from_slice(name);
            archive.extend_from_slice(content);

            push_u32(&mut central, ZIP_CENTRAL_DIRECTORY_SIGNATURE);
            push_u16(&mut central, (3 << 8) | 0x0014);
            push_u16(&mut central, 20);
            push_u16(&mut central, ZIP_UTF8_FLAG);
            push_u16(&mut central, 0);
            push_u16(&mut central, 0);
            push_u16(&mut central, 0);
            push_u32(&mut central, crc);
            push_u32(&mut central, length);
            push_u32(&mut central, length);
            push_u16(
                &mut central,
                u16::try_from(name.len()).expect("test name size"),
            );
            push_u16(&mut central, 0);
            push_u16(&mut central, 0);
            push_u16(&mut central, 0);
            push_u16(&mut central, 0);
            push_u32(&mut central, 0o100_644 << 16);
            push_u32(&mut central, offset);
            central.extend_from_slice(name);
        }
        let central_offset = u32::try_from(archive.len()).expect("test central offset");
        archive.extend_from_slice(&central);
        push_u32(&mut archive, ZIP_END_OF_CENTRAL_DIRECTORY_SIGNATURE);
        push_u16(&mut archive, 0);
        push_u16(&mut archive, 0);
        push_u16(
            &mut archive,
            u16::try_from(entries.len()).expect("test entry count"),
        );
        push_u16(
            &mut archive,
            u16::try_from(entries.len()).expect("test entry count"),
        );
        push_u32(
            &mut archive,
            u32::try_from(central.len()).expect("test central size"),
        );
        push_u32(&mut archive, central_offset);
        push_u16(&mut archive, 0);
        archive
    }

    fn import_archive(archive: &[u8]) -> (TempDir, ImportedSePackV1) {
        let source = tempfile::tempdir().expect("temporary source root");
        let archive_path = source.path().join("input.sepack");
        fs::write(&archive_path, archive).expect("write archive");
        let importer = SePackImporterV1::new(source.path()).expect("create importer");
        let imported_package = importer
            .import_archive(&archive_path)
            .expect("import archive");
        (source, imported_package)
    }

    #[test]
    fn canonical_archive_imports_then_local_developer_validation_parses_manifest() {
        let dll = b"P0 ABI DLL bytes";
        let manifest = package_manifest(dll);
        let archive = stored_zip(&[
            ("manifest.json", manifest.as_slice()),
            ("plugin/plugin.dll", dll),
        ]);
        let (source, imported_package) = import_archive(&archive);
        let sealed = tempfile::tempdir().expect("temporary sealed store");
        let validator = PackageValidatorV1::new(
            TrustedPublisherKeyStoreV1::default(),
            SealedPackageStoreV1::new(sealed.path()).expect("sealed store"),
        );
        let source_packages = LocalDeveloperPackageSourceV1::new(source.path().to_path_buf());
        assert!(
            source_packages
                .discover()
                .expect("inspect package source")
                .is_empty(),
            "private import scratch must never be discoverable as a package source"
        );
        let request = PackageValidationRequestV1::new(imported_package.root().to_path_buf())
            .with_local_developer_authorization(LocalDeveloperAuthorizationV1::issue());
        let validated = validator
            .validate(&request)
            .expect("local package validation");

        assert_eq!(validated.manifest_digest.len(), 64);
        assert_eq!(
            fs::read(imported_package.root().join("plugin/plugin.dll")).expect("read imported DLL"),
            dll
        );
        assert!(imported_package.root().join("manifest.json").is_file());
        assert!(
            imported_package
                .archive_sha256()
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        );
    }

    #[test]
    fn importing_the_same_archive_uses_distinct_private_scratch_generations() {
        let source = tempfile::tempdir().expect("temporary source root");
        let archive_path = source.path().join("same.sepack");
        let dll = b"immutable P0 ABI DLL bytes";
        let manifest = package_manifest(dll);
        fs::write(
            &archive_path,
            stored_zip(&[
                ("manifest.json", manifest.as_slice()),
                ("plugin/plugin.dll", dll),
            ]),
        )
        .expect("write archive");
        let importer = SePackImporterV1::new(source.path()).expect("create importer");
        let first = importer
            .import_archive(&archive_path)
            .expect("first import succeeds");
        let first_dll =
            fs::read(first.root().join("plugin/plugin.dll")).expect("read first published DLL");

        let second = importer
            .import_archive(&archive_path)
            .expect("retry uses a fresh scratch generation");
        assert_ne!(first.root(), second.root());
        assert_eq!(first.archive_sha256(), second.archive_sha256());
        assert_eq!(
            fs::read(first.root().join("plugin/plugin.dll"))
                .expect("read first generation after retry"),
            first_dll
        );
    }

    #[test]
    fn cryptographic_staging_name_ignores_precreated_pid_counter_guesses() {
        let source = tempfile::tempdir().expect("temporary source root");
        let staging = source.path().join(".sepack-staging");
        fs::create_dir(&staging).expect("create staging root");
        for guessed_counter in 0..32 {
            fs::create_dir(staging.join(format!("{}-{guessed_counter}", std::process::id())))
                .expect("precreate obsolete predictable staging name");
        }
        let archive_path = source.path().join("input.sepack");
        fs::write(
            &archive_path,
            stored_zip(&[("manifest.json", b"{}"), ("plugin/plugin.dll", b"dll")]),
        )
        .expect("write archive");
        let importer = SePackImporterV1::new(source.path()).expect("create importer");

        let imported_package = importer
            .import_archive(&archive_path)
            .expect("import archive");
        let leaf = imported_package
            .root()
            .file_name()
            .and_then(|name| name.to_str())
            .expect("staging leaf");
        assert!(
            !leaf.ends_with("-0") && !leaf.ends_with("-31"),
            "staging must not use a predictable process-local counter"
        );
        assert!(
            leaf.rsplit_once('-')
                .is_some_and(|(_, nonce)| nonce.len() == 32
                    && nonce.bytes().all(|byte| byte.is_ascii_hexdigit())),
            "staging leaf must contain a 128-bit hexadecimal random nonce"
        );
    }

    #[cfg(windows)]
    #[test]
    fn repeated_entries_reuse_the_same_held_parent_directory_handle() {
        let source = tempfile::tempdir().expect("temporary source root");
        let archive_path = source.path().join("input.sepack");
        fs::write(
            &archive_path,
            stored_zip(&[
                ("manifest.json", b"{}"),
                ("plugin/plugin.dll", b"dll"),
                ("plugin/first.data", b"first"),
                ("plugin/second.data", b"second"),
            ]),
        )
        .expect("write archive");
        let importer = SePackImporterV1::new(source.path()).expect("create importer");

        let imported_package = importer
            .import_archive(&archive_path)
            .expect("import archive");
        assert_eq!(
            fs::read(imported_package.root().join("plugin/first.data")).expect("read first file"),
            b"first"
        );
        assert_eq!(
            fs::read(imported_package.root().join("plugin/second.data")).expect("read second file"),
            b"second"
        );
    }

    #[test]
    fn expired_or_cancelled_import_never_publishes_private_content() {
        let source = tempfile::tempdir().expect("temporary source root");
        let archive_path = source.path().join("input.sepack");
        fs::write(
            &archive_path,
            stored_zip(&[("manifest.json", b"{}"), ("plugin/plugin.dll", b"dll")]),
        )
        .expect("write archive");
        let importer = SePackImporterV1::new(source.path()).expect("create importer");
        let cancelled = PackageValidationCancellationV1::new();
        let expired = ImportBudgetV1 {
            deadline: Instant::now()
                .checked_sub(Duration::from_millis(1))
                .unwrap_or_else(Instant::now),
            cancelled: &cancelled,
            cancel_at_commit_for_test: false,
        };
        assert!(matches!(
            importer.import_archive_with_budget(&archive_path, &expired),
            Err(SePackImportErrorV1::ImportTimedOut)
        ));
        cancelled.cancel();
        let active_deadline = ImportBudgetV1 {
            deadline: Instant::now() + Duration::from_secs(1),
            cancelled: &cancelled,
            cancel_at_commit_for_test: false,
        };
        assert!(matches!(
            importer.import_archive_with_budget(&archive_path, &active_deadline),
            Err(SePackImportErrorV1::ImportCancelled)
        ));
        assert!(
            fs::read_dir(source.path())
                .expect("read source root")
                .filter_map(Result::ok)
                .all(|entry| entry.file_name() == ".sepack-staging" || entry.path().is_file()),
            "timeout or cancellation must leave no private package publication"
        );
    }

    #[test]
    fn cancellation_at_the_commit_boundary_cleans_private_staging() {
        let source = tempfile::tempdir().expect("temporary source root");
        let archive_path = source.path().join("input.sepack");
        fs::write(
            &archive_path,
            stored_zip(&[("manifest.json", b"{}"), ("plugin/plugin.dll", b"dll")]),
        )
        .expect("write archive");
        let importer = SePackImporterV1::new(source.path()).expect("create importer");
        let cancelled = PackageValidationCancellationV1::new();
        let budget = ImportBudgetV1 {
            deadline: Instant::now() + Duration::from_secs(1),
            cancelled: &cancelled,
            cancel_at_commit_for_test: true,
        };

        assert!(matches!(
            importer.import_archive_with_budget(&archive_path, &budget),
            Err(SePackImportErrorV1::ImportCancelled)
        ));
        let staging = source.path().join(".sepack-staging");
        assert!(
            fs::read_dir(staging)
                .expect("read private staging")
                .next()
                .is_none(),
            "a cancellation before disarm must clean the private staging generation"
        );
    }

    #[cfg(windows)]
    #[test]
    fn held_staging_parent_handle_rejects_adversarial_parent_swap() {
        let source = tempfile::tempdir().expect("temporary source root");
        let importer = SePackImporterV1::new(source.path()).expect("create importer");
        let staging = source.path().join(".sepack-staging");
        let replacement = source.path().join("replacement-staging");
        fs::create_dir(&replacement).expect("create replacement directory");

        assert!(
            fs::rename(&staging, source.path().join("staging-moved")).is_err(),
            "the held no-delete staging handle must reject a parent swap"
        );
        drop(importer);
    }

    #[test]
    fn production_local_store_seals_valid_packages_and_cleans_import_sources() {
        let source = tempfile::tempdir().expect("temporary source root");
        let sealed = tempfile::tempdir().expect("temporary sealed root");
        let archive_path = source.path().join("valid.sepack");
        let dll = b"production store P0 ABI DLL bytes";
        let manifest = package_manifest(dll);
        fs::write(
            &archive_path,
            stored_zip(&[
                ("manifest.json", manifest.as_slice()),
                ("plugin/plugin.dll", dll),
            ]),
        )
        .expect("write valid archive");
        let validator = PackageValidatorV1::new(
            TrustedPublisherKeyStoreV1::default(),
            SealedPackageStoreV1::new(sealed.path()).expect("sealed store"),
        );
        let store = LocalDeveloperPackageStoreV1::new(source.path()).expect("local store");

        let result = store
            .import_and_validate(&archive_path, &validator)
            .expect("import and seal valid package");

        assert_eq!(result.manifest_digest.len(), 64);
        assert!(
            fs::read_dir(source.path())
                .expect("read source root")
                .filter_map(Result::ok)
                .all(|entry| entry.file_name() == ".sepack-staging" || entry.path().is_file()),
            "the transient imported source must be removed after sealing"
        );
    }

    #[test]
    fn production_local_store_cleans_sources_after_validation_failure() {
        let source = tempfile::tempdir().expect("temporary source root");
        let sealed = tempfile::tempdir().expect("temporary sealed root");
        let archive_path = source.path().join("invalid.sepack");
        fs::write(
            &archive_path,
            stored_zip(&[("manifest.json", b"{}"), ("plugin/plugin.dll", b"invalid")]),
        )
        .expect("write invalid archive");
        let validator = PackageValidatorV1::new(
            TrustedPublisherKeyStoreV1::default(),
            SealedPackageStoreV1::new(sealed.path()).expect("sealed store"),
        );
        let store = LocalDeveloperPackageStoreV1::new(source.path()).expect("local store");

        assert!(matches!(
            store.import_and_validate(&archive_path, &validator),
            Err(LocalDeveloperPackageStoreErrorV1::Validation(_))
        ));
        assert!(
            fs::read_dir(source.path())
                .expect("read source root")
                .filter_map(Result::ok)
                .all(|entry| entry.file_name() == ".sepack-staging" || entry.path().is_file()),
            "an invalid imported source must not remain discoverable"
        );
    }

    #[test]
    fn caller_owned_cancellation_prevents_local_import_publication_and_leaves_staging_empty() {
        let source = tempfile::tempdir().expect("temporary source root");
        let sealed = tempfile::tempdir().expect("temporary sealed root");
        let archive_path = source.path().join("cancelled.sepack");
        let dll = b"cancelled import bytes";
        let manifest = package_manifest(dll);
        fs::write(
            &archive_path,
            stored_zip(&[
                ("manifest.json", manifest.as_slice()),
                ("plugin/plugin.dll", dll),
            ]),
        )
        .expect("write archive");
        let validator = PackageValidatorV1::new(
            TrustedPublisherKeyStoreV1::default(),
            SealedPackageStoreV1::new(sealed.path()).expect("sealed store"),
        );
        let store = LocalDeveloperPackageStoreV1::new(source.path()).expect("local store");
        let cancellation = PackageValidationCancellationV1::new();
        cancellation.cancel();

        assert!(matches!(
            store.import_and_validate_with_cancellation(&archive_path, &validator, &cancellation),
            Err(LocalDeveloperPackageStoreErrorV1::Import(
                SePackImportErrorV1::ImportCancelled
            ))
        ));
        assert!(
            fs::read_dir(source.path().join(".sepack-staging"))
                .expect("private staging root")
                .next()
                .is_none(),
            "cancelled import must not publish a private scratch generation"
        );
    }

    #[test]
    fn scratch_cleanup_failure_never_rolls_back_a_sealed_package_and_retry_is_unique() {
        let source = tempfile::tempdir().expect("temporary source root");
        let sealed = tempfile::tempdir().expect("temporary sealed root");
        let archive_path = source.path().join("retry.sepack");
        let dll = b"sealed before cleanup failure";
        let manifest = package_manifest(dll);
        fs::write(
            &archive_path,
            stored_zip(&[
                ("manifest.json", manifest.as_slice()),
                ("plugin/plugin.dll", dll),
            ]),
        )
        .expect("write archive");
        let validator = PackageValidatorV1::new(
            TrustedPublisherKeyStoreV1::default(),
            SealedPackageStoreV1::new(sealed.path()).expect("sealed store"),
        );
        let store = LocalDeveloperPackageStoreV1::new(source.path()).expect("local store");

        store.force_next_cleanup_failure_for_test();
        let committed = store
            .import_and_validate(&archive_path, &validator)
            .expect("cleanup failure must not hide a sealed package");
        assert_eq!(committed.manifest_digest.len(), 64);
        let telemetry = store.telemetry();
        assert_eq!(telemetry.cleanup_failure_count(), 1);
        assert!(
            !format!("{telemetry:?}").contains(&source.path().display().to_string()),
            "scratch telemetry must not reveal local paths"
        );

        let retry = store
            .import_and_validate(&archive_path, &validator)
            .expect("a retry must allocate a fresh scratch generation");
        assert_eq!(retry.manifest_digest, committed.manifest_digest);
        assert_eq!(store.telemetry().cleanup_failure_count(), 1);
    }

    #[test]
    fn scratch_cleanup_failure_preserves_the_validation_root_cause() {
        let source = tempfile::tempdir().expect("temporary source root");
        let sealed = tempfile::tempdir().expect("temporary sealed root");
        let archive_path = source.path().join("invalid-retry.sepack");
        fs::write(
            &archive_path,
            stored_zip(&[("manifest.json", b"{}"), ("plugin/plugin.dll", b"invalid")]),
        )
        .expect("write invalid archive");
        let validator = PackageValidatorV1::new(
            TrustedPublisherKeyStoreV1::default(),
            SealedPackageStoreV1::new(sealed.path()).expect("sealed store"),
        );
        let store = LocalDeveloperPackageStoreV1::new(source.path()).expect("local store");

        store.force_next_cleanup_failure_for_test();
        assert!(matches!(
            store.import_and_validate(&archive_path, &validator),
            Err(LocalDeveloperPackageStoreErrorV1::Validation(_))
        ));
        assert_eq!(store.telemetry().cleanup_failure_count(), 1);
    }

    #[test]
    fn stale_scratch_scavenger_is_bounded_and_skips_unsafe_or_oversized_trees() {
        let source = tempfile::tempdir().expect("temporary source root");
        let staging = source.path().join(".sepack-staging");
        fs::create_dir(&staging).expect("create staging root");
        let stale_empty = staging.join("0-00000000000000000000000000000000");
        fs::create_dir(&stale_empty).expect("create stale empty scratch");
        let oversized = staging.join("0-11111111111111111111111111111111");
        fs::create_dir(&oversized).expect("create oversized scratch");
        for index in 0..=super::MAX_STAGING_ENTRY_COUNT_V1 {
            fs::write(oversized.join(format!("{index}.tmp")), b"x")
                .expect("write oversized scratch entry");
        }

        super::scavenge_stale_staging_directories_with_limits(
            &staging,
            8,
            Instant::now() + Duration::from_secs(1),
            Duration::ZERO,
        );
        assert!(
            !stale_empty.exists(),
            "stale safe scratch must be scavenged"
        );
        assert!(
            oversized.exists(),
            "bounded scavenger must skip oversized trees"
        );
    }

    #[test]
    #[ignore = "invoked by sdk/tests/plugin-tooling-self-test.ps1 with a real package artifact"]
    fn script_produced_sepack_reaches_production_native_lifecycle() {
        let archive = std::env::var_os("SUPEREXPLORER_TEST_SEPACK_PATH")
            .map(std::path::PathBuf::from)
            .expect("SUPEREXPLORER_TEST_SEPACK_PATH must identify the script-produced archive");
        let config = crate::ExtensionHostConfigV1 {
            local_developer_mode: crate::LocalDeveloperModeV1::Enabled,
            ..crate::ExtensionHostConfigV1::default()
        }
        .with_local_developer_archives([archive]);
        let mut host = crate::ExtensionHost::with_config(config);
        host.start().expect(
            "script-produced package must traverse ExtensionHost import, validation, resolution, and native lifecycle",
        );
        let [admission] = host.startup_admissions() else {
            panic!("script-produced package must produce exactly one host startup admission");
        };
        assert_eq!(admission.root_count, 1);
        assert_eq!(admission.activated_feature_count, 1);
        host.shutdown();
    }

    #[test]
    fn traversal_duplicate_and_noncanonical_inventory_never_publish_partial_content() {
        let source = tempfile::tempdir().expect("temporary source root");
        let importer = SePackImporterV1::new(source.path()).expect("create importer");
        let archive_path = source.path().join("unsafe.sepack");
        fs::write(
            &archive_path,
            stored_zip(&[("manifest.json", b"{}"), ("plugin/../plugin.dll", b"dll")]),
        )
        .expect("write traversal archive");

        assert!(matches!(
            importer.import_archive(&archive_path),
            Err(SePackImportErrorV1::InvalidEntryName { .. })
        ));
        assert_eq!(
            fs::read_dir(source.path())
                .expect("read source root")
                .filter_map(Result::ok)
                .filter(|entry| entry.file_name() != ".sepack-staging")
                .count(),
            1,
            "only the input archive remains; no partial package is published"
        );
    }

    #[test]
    fn malformed_zip_features_are_rejected_before_any_package_handoff() {
        let dll = b"P0 ABI DLL bytes";
        let manifest = package_manifest(dll);
        let canonical = stored_zip(&[
            ("manifest.json", manifest.as_slice()),
            ("plugin/plugin.dll", dll),
        ]);
        let central = canonical
            .windows(4)
            .rposition(|bytes| {
                u32::from_le_bytes(bytes.try_into().expect("signature"))
                    == ZIP_CENTRAL_DIRECTORY_SIGNATURE
            })
            .expect("central directory");
        let mut compressed = canonical.clone();
        compressed[8] = 8;
        compressed[central + 10] = 8;
        let mut descriptor = canonical.clone();
        descriptor[6] |= 0b1000;
        descriptor[central + 8] |= 0b1000;
        let mut crc_mismatch = canonical.clone();
        crc_mismatch[30 + "manifest.json".len()] ^= 1;
        let mut zip64 = canonical.clone();
        let eocd = zip64.len() - 22;
        zip64[eocd + 8..eocd + 10].copy_from_slice(&u16::MAX.to_le_bytes());
        let duplicate = stored_zip(&[
            ("manifest.json", manifest.as_slice()),
            ("manifest.json", manifest.as_slice()),
            ("plugin/plugin.dll", dll),
        ]);
        let mut truncated = canonical.clone();
        truncated.pop();
        let mut appended = canonical.clone();
        appended.extend_from_slice(b"polyglot-trailing-bytes");

        for (name, archive) in [
            ("compressed", compressed),
            ("descriptor", descriptor),
            ("crc", crc_mismatch),
            ("zip64", zip64),
            ("duplicate", duplicate),
            ("truncated", truncated),
            ("appended", appended),
        ] {
            let source = tempfile::tempdir().expect("temporary source root");
            let archive_path = source.path().join(format!("{name}.sepack"));
            fs::write(&archive_path, archive).expect("write malformed archive");
            let importer = SePackImporterV1::new(source.path()).expect("create importer");

            assert!(importer.import_archive(&archive_path).is_err(), "{name}");
            assert!(
                fs::read_dir(source.path())
                    .expect("read source root")
                    .filter_map(Result::ok)
                    .all(|entry| entry.file_name() == ".sepack-staging" || entry.path().is_file()),
                "{name} must not publish a package directory"
            );
        }
    }
}
