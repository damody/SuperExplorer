//! Cross-filesystem copy/move using bounded scoped staging.

use std::{
    fs,
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use anyhow::{Context as _, Result, bail};
use explorer_model::{CancellationToken, ConflictDecision, LocationDescriptor};

use crate::RemoteProviderRegistry;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransferMode {
    Copy,
    Move,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TransferResult {
    Succeeded,
    Skipped,
    Partial {
        stage: TransferStage,
        diagnostic: String,
    },
    Failed {
        stage: TransferStage,
        diagnostic: String,
    },
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransferStage {
    ConflictInspection,
    LocalCopy,
    SourceDownload,
    DestinationUpload,
    SourceDelete,
    ProviderPanic,
}

impl TransferStage {
    pub const fn user_label(self) -> &'static str {
        match self {
            Self::ConflictInspection => "目的地衝突檢查",
            Self::LocalCopy => "本機複製",
            Self::SourceDownload => "來源下載",
            Self::DestinationUpload => "目的地上傳",
            Self::SourceDelete => "移動後刪除來源",
            Self::ProviderPanic => "傳輸提供者異常",
        }
    }
}

#[derive(Debug)]
struct TransferFailure {
    stage: TransferStage,
    error: anyhow::Error,
}

impl TransferFailure {
    fn new(stage: TransferStage, error: impl Into<anyhow::Error>) -> Self {
        Self {
            stage,
            error: error.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransferItemOutcome {
    pub source: LocationDescriptor,
    pub destination: LocationDescriptor,
    pub result: TransferResult,
}

pub struct TransferEngine<'a> {
    providers: &'a RemoteProviderRegistry,
}

enum ConflictPlan {
    Proceed,
    Skip,
    KeepBoth(String),
}

static PROCESS_STAGING_BYTES: AtomicU64 = AtomicU64::new(0);

pub fn sanitize_transfer_diagnostic(diagnostic: &str) -> String {
    let trimmed = diagnostic.trim();
    if trimmed.is_empty() {
        return "未提供底層錯誤".to_owned();
    }
    let mut sanitized = redact_uri_userinfo(trimmed);
    for key in ["password", "passwd", "token", "secret"] {
        sanitized = redact_assignment_values(&sanitized, key);
    }
    sanitized
}

fn redact_uri_userinfo(value: &str) -> String {
    let mut output = value.to_owned();
    let mut search_from = 0;
    loop {
        let lower = output.to_ascii_lowercase();
        let Some(relative_scheme) = lower[search_from..].find("://") else {
            break;
        };
        let authority_start = search_from + relative_scheme + 3;
        let authority_end = output[authority_start..]
            .find(['/', ' ', '\t', '\r', '\n'])
            .map_or(output.len(), |offset| authority_start + offset);
        let Some(relative_at) = output[authority_start..authority_end].rfind('@') else {
            search_from = authority_end;
            continue;
        };
        let userinfo_end = authority_start + relative_at;
        output.replace_range(authority_start..userinfo_end, "[已隱藏]");
        search_from = authority_start + "[已隱藏]@".len();
    }
    output
}

fn redact_assignment_values(value: &str, key: &str) -> String {
    let mut output = value.to_owned();
    let mut search_from = 0;
    loop {
        let lower = output.to_ascii_lowercase();
        let Some(relative_key) = lower[search_from..].find(key) else {
            break;
        };
        let key_start = search_from + relative_key;
        let mut separator = key_start + key.len();
        while output.as_bytes().get(separator) == Some(&b' ') {
            separator += 1;
        }
        if !matches!(output.as_bytes().get(separator), Some(b'=') | Some(b':')) {
            search_from = key_start + key.len();
            continue;
        }
        let mut value_start = separator + 1;
        while output.as_bytes().get(value_start) == Some(&b' ') {
            value_start += 1;
        }
        let value_end = output[value_start..]
            .find([',', ';', ' ', '\t', '\r', '\n'])
            .map_or(output.len(), |offset| value_start + offset);
        output.replace_range(value_start..value_end, "[已隱藏]");
        search_from = value_start + "[已隱藏]".len();
    }
    output
}

struct StagingReservation(u64);

impl StagingReservation {
    fn acquire(bytes: u64) -> Result<Self> {
        let result =
            PROCESS_STAGING_BYTES.fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                current
                    .checked_add(bytes)
                    .filter(|next| *next <= crate::provider::MAX_PROCESS_STAGING_BYTES)
            });
        result
            .map(|_| Self(bytes))
            .map_err(|_| anyhow::anyhow!("process staging quota exceeded"))
    }
}

impl Drop for StagingReservation {
    fn drop(&mut self) {
        PROCESS_STAGING_BYTES.fetch_sub(self.0, Ordering::AcqRel);
    }
}

impl<'a> TransferEngine<'a> {
    pub const fn new(providers: &'a RemoteProviderRegistry) -> Self {
        Self { providers }
    }

    pub fn transfer(
        &self,
        source: LocationDescriptor,
        destination: LocationDescriptor,
        mode: TransferMode,
        cancellation: &CancellationToken,
    ) -> TransferItemOutcome {
        self.transfer_with_conflict(
            source,
            destination,
            mode,
            ConflictDecision::Replace,
            cancellation,
        )
    }

    pub fn transfer_with_conflict(
        &self,
        source: LocationDescriptor,
        destination: LocationDescriptor,
        mode: TransferMode,
        conflict: ConflictDecision,
        cancellation: &CancellationToken,
    ) -> TransferItemOutcome {
        self.transfer_with_conflict_and_progress(
            source,
            destination,
            mode,
            conflict,
            cancellation,
            &|_| {},
        )
    }

    pub fn transfer_with_conflict_and_progress(
        &self,
        source: LocationDescriptor,
        destination: LocationDescriptor,
        mode: TransferMode,
        conflict: ConflictDecision,
        cancellation: &CancellationToken,
        progress: &(dyn Fn(u64) + Send + Sync),
    ) -> TransferItemOutcome {
        let result = if cancellation.is_cancelled() {
            TransferResult::Cancelled
        } else {
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                self.copy(&source, &destination, conflict, cancellation, progress)
            })) {
                Err(_) => TransferResult::Failed {
                    stage: TransferStage::ProviderPanic,
                    diagnostic: "transfer provider panicked".to_owned(),
                },
                Ok(copy_result) => match copy_result {
                    Ok(false) => TransferResult::Skipped,
                    Ok(true) if mode == TransferMode::Copy => TransferResult::Succeeded,
                    Ok(true) => match self.delete_source(&source, cancellation) {
                        Ok(()) => TransferResult::Succeeded,
                        Err(error) => TransferResult::Partial {
                            stage: TransferStage::SourceDelete,
                            diagnostic: sanitize_transfer_diagnostic(&format!("{error:#}")),
                        },
                    },
                    Err(_error) if cancellation.is_cancelled() => TransferResult::Cancelled,
                    Err(failure) => TransferResult::Failed {
                        stage: failure.stage,
                        diagnostic: sanitize_transfer_diagnostic(&format!("{:#}", failure.error)),
                    },
                },
            }
        };
        TransferItemOutcome {
            source,
            destination,
            result,
        }
    }

    fn copy(
        &self,
        source: &LocationDescriptor,
        destination: &LocationDescriptor,
        conflict: ConflictDecision,
        cancellation: &CancellationToken,
        progress: &(dyn Fn(u64) + Send + Sync),
    ) -> Result<bool, TransferFailure> {
        match (source, destination) {
            (
                LocationDescriptor::FileSystem(source),
                LocationDescriptor::FileSystem(destination),
            ) => copy_local_with_conflict(source, destination, conflict, progress)
                .map_err(|error| TransferFailure::new(TransferStage::LocalCopy, error)),
            (LocationDescriptor::FileSystem(source), LocationDescriptor::Virtual(destination)) => {
                let name = source
                    .file_name()
                    .and_then(|name| name.to_str())
                    .context("source has no UTF-8 file name")
                    .map_err(|error| TransferFailure::new(TransferStage::LocalCopy, error))?;
                let plan = self
                    .remote_destination_plan(destination, name, conflict, cancellation)
                    .map_err(|error| {
                        TransferFailure::new(TransferStage::ConflictInspection, error)
                    })?;
                if matches!(plan, ConflictPlan::Skip) {
                    return Ok(false);
                }
                let renamed;
                let upload_source = if let ConflictPlan::KeepBoth(name) = plan {
                    renamed = staged_with_name(source, &name)
                        .map_err(|error| TransferFailure::new(TransferStage::LocalCopy, error))?;
                    renamed.1.as_path()
                } else {
                    source
                };
                self.providers
                    .resolve(&LocationDescriptor::Virtual(destination.clone()))
                    .map_err(|error| TransferFailure::new(TransferStage::DestinationUpload, error))?
                    .upload_with_progress(upload_source, destination, cancellation, progress)
                    .with_context(|| {
                        format!(
                            "upload to {}",
                            LocationDescriptor::Virtual(destination.clone()).editable_text()
                        )
                    })
                    .map_err(|error| {
                        TransferFailure::new(TransferStage::DestinationUpload, error)
                    })?;
                Ok(true)
            }
            (LocationDescriptor::Virtual(source), LocationDescriptor::FileSystem(destination)) => {
                let target = if destination.is_dir() {
                    let name = source
                        .components
                        .last()
                        .context("remote source has no final component")
                        .map_err(|error| {
                            TransferFailure::new(TransferStage::SourceDownload, error)
                        })?;
                    crate::provider::validate_windows_component(name).map_err(|error| {
                        TransferFailure::new(TransferStage::SourceDownload, error)
                    })?;
                    destination.join(name)
                } else {
                    destination.clone()
                };
                if !local_destination_allows(&target, conflict).map_err(|error| {
                    TransferFailure::new(TransferStage::ConflictInspection, error)
                })? {
                    return Ok(false);
                }
                self.providers
                    .resolve(&LocationDescriptor::Virtual(source.clone()))
                    .map_err(|error| TransferFailure::new(TransferStage::SourceDownload, error))?
                    .download_with_progress(source, &target, cancellation, progress)
                    .with_context(|| {
                        format!(
                            "download from {}",
                            LocationDescriptor::Virtual(source.clone()).editable_text()
                        )
                    })
                    .map_err(|error| TransferFailure::new(TransferStage::SourceDownload, error))?;
                Ok(true)
            }
            (LocationDescriptor::Virtual(source), LocationDescriptor::Virtual(destination)) => {
                let staging = tempfile::Builder::new()
                    .prefix("superexplorer-remote-transfer-")
                    .tempdir()
                    .context("create scoped transfer staging")
                    .map_err(|error| TransferFailure::new(TransferStage::LocalCopy, error))?;
                ensure_staging_free_space(staging.path())
                    .map_err(|error| TransferFailure::new(TransferStage::LocalCopy, error))?;
                let name = source
                    .components
                    .last()
                    .context("remote source has no final component")
                    .map_err(|error| TransferFailure::new(TransferStage::SourceDownload, error))?;
                crate::provider::validate_windows_component(name)
                    .map_err(|error| TransferFailure::new(TransferStage::SourceDownload, error))?;
                let plan = self
                    .remote_destination_plan(destination, name, conflict, cancellation)
                    .map_err(|error| {
                        TransferFailure::new(TransferStage::ConflictInspection, error)
                    })?;
                if matches!(plan, ConflictPlan::Skip) {
                    return Ok(false);
                }
                let staged = staging.path().join(name);
                self.providers
                    .resolve(&LocationDescriptor::Virtual(source.clone()))
                    .map_err(|error| TransferFailure::new(TransferStage::SourceDownload, error))?
                    .download_with_progress(source, &staged, cancellation, progress)
                    .with_context(|| {
                        format!(
                            "download from {}",
                            LocationDescriptor::Virtual(source.clone()).editable_text()
                        )
                    })
                    .map_err(|error| TransferFailure::new(TransferStage::SourceDownload, error))?;
                ensure_owned_staging_containment(staging.path(), &staged)
                    .map_err(|error| TransferFailure::new(TransferStage::LocalCopy, error))?;
                let staged_bytes = local_tree_bytes(&staged)
                    .map_err(|error| TransferFailure::new(TransferStage::LocalCopy, error))?;
                if staged_bytes > crate::provider::MAX_OPERATION_STAGING_BYTES {
                    return Err(TransferFailure::new(
                        TransferStage::LocalCopy,
                        anyhow::anyhow!("operation staging quota exceeded"),
                    ));
                }
                let _reservation = StagingReservation::acquire(staged_bytes)
                    .map_err(|error| TransferFailure::new(TransferStage::LocalCopy, error))?;
                if cancellation.is_cancelled() {
                    return Err(TransferFailure::new(
                        TransferStage::LocalCopy,
                        anyhow::anyhow!("transfer cancelled"),
                    ));
                }
                let renamed;
                let upload_source = if let ConflictPlan::KeepBoth(name) = plan {
                    renamed = staged_with_name(&staged, &name)
                        .map_err(|error| TransferFailure::new(TransferStage::LocalCopy, error))?;
                    renamed.1.as_path()
                } else {
                    staged.as_path()
                };
                self.providers
                    .resolve(&LocationDescriptor::Virtual(destination.clone()))
                    .map_err(|error| TransferFailure::new(TransferStage::DestinationUpload, error))?
                    .upload_with_progress(upload_source, destination, cancellation, progress)
                    .with_context(|| {
                        format!(
                            "upload to {}",
                            LocationDescriptor::Virtual(destination.clone()).editable_text()
                        )
                    })
                    .map_err(|error| {
                        TransferFailure::new(TransferStage::DestinationUpload, error)
                    })?;
                Ok(true)
            }
            _ => Err(TransferFailure::new(
                TransferStage::LocalCopy,
                anyhow::anyhow!("unsupported Shell location in remote transfer"),
            )),
        }
    }

    /// Best-effort byte work estimate. Remote-to-remote transfers perform both a download and
    /// upload, hence twice the source size. Directory totals remain unknown unless the provider
    /// can report an authoritative aggregate without following links.
    pub fn estimate_work_bytes(
        &self,
        source: &LocationDescriptor,
        destination: &LocationDescriptor,
        cancellation: &CancellationToken,
    ) -> Option<u64> {
        let bytes = match source {
            LocationDescriptor::FileSystem(path) => local_tree_bytes(path).ok()?,
            LocationDescriptor::Virtual(remote) => {
                let provider = self.providers.resolve(source).ok()?;
                estimate_remote_tree_bytes(provider.as_ref(), remote, cancellation)?
            }
            _ => return None,
        };
        if matches!(source, LocationDescriptor::Virtual(_))
            && matches!(destination, LocationDescriptor::Virtual(_))
        {
            bytes.checked_mul(2)
        } else {
            Some(bytes)
        }
    }

    fn remote_destination_plan(
        &self,
        destination: &explorer_model::VirtualLocationDescriptor,
        name: &str,
        conflict: ConflictDecision,
        cancellation: &CancellationToken,
    ) -> Result<ConflictPlan> {
        let location = LocationDescriptor::Virtual(destination.clone());
        let exists = self
            .providers
            .resolve(&location)?
            .list(destination, cancellation)?
            .iter()
            .any(|entry| entry.name.eq_ignore_ascii_case(name));
        if !exists {
            return Ok(ConflictPlan::Proceed);
        }
        match conflict {
            ConflictDecision::Skip => Ok(ConflictPlan::Skip),
            ConflictDecision::Replace => Ok(ConflictPlan::Proceed),
            ConflictDecision::Prompt => bail!("destination conflict requires a user decision"),
            ConflictDecision::KeepBoth => {
                let entries = self
                    .providers
                    .resolve(&location)?
                    .list(destination, cancellation)?;
                for suffix in 2..=10_000_u32 {
                    let candidate = keep_both_name(name, suffix);
                    if !entries
                        .iter()
                        .any(|entry| entry.name.eq_ignore_ascii_case(&candidate))
                    {
                        return Ok(ConflictPlan::KeepBoth(candidate));
                    }
                }
                bail!("keep-both destination name limit exceeded")
            }
        }
    }

    fn delete_source(
        &self,
        source: &LocationDescriptor,
        cancellation: &CancellationToken,
    ) -> Result<()> {
        match source {
            LocationDescriptor::FileSystem(path) if path.is_dir() => fs::remove_dir_all(path)
                .with_context(|| format!("remove moved directory {}", path.display())),
            LocationDescriptor::FileSystem(path) => fs::remove_file(path)
                .with_context(|| format!("remove moved file {}", path.display())),
            LocationDescriptor::Virtual(location) => {
                self.providers
                    .resolve(source)?
                    .delete(location, true, cancellation)
            }
            _ => bail!("unsupported Shell source in remote transfer"),
        }
    }
}

fn estimate_remote_tree_bytes(
    provider: &dyn crate::RemoteProvider,
    root: &explorer_model::VirtualLocationDescriptor,
    cancellation: &CancellationToken,
) -> Option<u64> {
    let metadata = provider.metadata(root, cancellation).ok()?;
    match metadata.kind {
        crate::RemoteEntryKind::File => return metadata.size,
        crate::RemoteEntryKind::Directory => {}
        crate::RemoteEntryKind::FileSymlink
        | crate::RemoteEntryKind::DirectorySymlink
        | crate::RemoteEntryKind::BrokenSymlink
        | crate::RemoteEntryKind::CircularSymlink => return None,
    }
    let mut total = 0_u64;
    let mut nodes = 0_usize;
    let mut pending = vec![(root.clone(), 0_usize)];
    while let Some((directory, depth)) = pending.pop() {
        if cancellation.is_cancelled() {
            return None;
        }
        for entry in provider.list(&directory, cancellation).ok()? {
            nodes = nodes.checked_add(1)?;
            if !crate::provider::transfer_tree_within_limits(depth, nodes) {
                return None;
            }
            match entry.kind {
                crate::RemoteEntryKind::File => {
                    let bytes = entry.size?;
                    if !crate::provider::transfer_bytes_within_limits(bytes, total) {
                        return None;
                    }
                    total = total.checked_add(bytes)?;
                    if !crate::provider::transfer_bytes_within_limits(bytes, total) {
                        return None;
                    }
                }
                crate::RemoteEntryKind::Directory => {
                    let LocationDescriptor::Virtual(child) = entry.location else {
                        return None;
                    };
                    pending.push((child, depth.checked_add(1)?));
                }
                crate::RemoteEntryKind::FileSymlink
                | crate::RemoteEntryKind::DirectorySymlink
                | crate::RemoteEntryKind::BrokenSymlink
                | crate::RemoteEntryKind::CircularSymlink => return None,
            }
        }
    }
    Some(total)
}

fn copy_local(
    source: &Path,
    destination: &Path,
    progress: &(dyn Fn(u64) + Send + Sync),
) -> Result<()> {
    let target = if destination.is_dir() {
        destination.join(source.file_name().context("source has no file name")?)
    } else {
        PathBuf::from(destination)
    };
    if source.is_dir() {
        return copy_local_tree_progress(source, &target, progress);
    }
    copy_local_file_progress(source, &target, progress)?;
    Ok(())
}

fn copy_local_with_conflict(
    source: &Path,
    destination: &Path,
    conflict: ConflictDecision,
    progress: &(dyn Fn(u64) + Send + Sync),
) -> Result<bool> {
    let target = if destination.is_dir() {
        destination.join(source.file_name().context("source has no file name")?)
    } else {
        destination.to_path_buf()
    };
    if target.exists() && conflict == ConflictDecision::KeepBoth {
        for suffix in 2..=10_000_u32 {
            let name = source
                .file_name()
                .and_then(|name| name.to_str())
                .context("source has no UTF-8 file name")?;
            let candidate = target.with_file_name(keep_both_name(name, suffix));
            if !candidate.exists() {
                if source.is_dir() {
                    copy_local_tree_progress(source, &candidate, progress)?;
                } else {
                    copy_local_file_progress(source, &candidate, progress)?;
                }
                return Ok(true);
            }
        }
        bail!("keep-both destination name limit exceeded");
    }
    if !local_destination_allows(&target, conflict)? {
        return Ok(false);
    }
    copy_local(source, destination, progress)?;
    Ok(true)
}

fn local_destination_allows(target: &Path, conflict: ConflictDecision) -> Result<bool> {
    if !target.exists() {
        return Ok(true);
    }
    match conflict {
        ConflictDecision::Skip => Ok(false),
        ConflictDecision::Replace => Ok(true),
        ConflictDecision::Prompt => bail!("destination conflict requires a user decision"),
        ConflictDecision::KeepBoth => Ok(true),
    }
}

fn keep_both_name(name: &str, suffix: u32) -> String {
    let path = Path::new(name);
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(name);
    match path.extension().and_then(|value| value.to_str()) {
        Some(extension) => format!("{stem} ({suffix}).{extension}"),
        None => format!("{stem} ({suffix})"),
    }
}

fn staged_with_name(source: &Path, name: &str) -> Result<(tempfile::TempDir, PathBuf)> {
    crate::provider::validate_windows_component(name)?;
    let root = tempfile::Builder::new()
        .prefix("superexplorer-conflict-")
        .tempdir()?;
    let target = root.path().join(name);
    if source.is_dir() {
        copy_local_tree(source, &target)?;
    } else {
        fs::copy(source, &target)?;
    }
    Ok((root, target))
}

pub fn local_tree_bytes(root: &Path) -> Result<u64> {
    let metadata = fs::symlink_metadata(root)?;
    if metadata.file_type().is_symlink() {
        bail!("staging tree contains a symbolic link");
    }
    if metadata.is_file() {
        if metadata.len() > crate::provider::MAX_TRANSFER_FILE_BYTES {
            bail!("staged file exceeds quota");
        }
        return Ok(metadata.len());
    }
    let mut total = 0_u64;
    let mut pending = vec![(root.to_path_buf(), 0_usize)];
    let mut nodes = 0_usize;
    while let Some((directory, depth)) = pending.pop() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            nodes = nodes.saturating_add(1);
            if !crate::provider::transfer_tree_within_limits(depth, nodes) {
                bail!("staging tree exceeds traversal limits");
            }
            let metadata = entry.file_type()?;
            if metadata.is_symlink() {
                bail!("staging tree contains a symbolic link");
            }
            if metadata.is_dir() {
                pending.push((entry.path(), depth + 1));
            } else {
                let bytes = entry.metadata()?.len();
                if bytes > crate::provider::MAX_TRANSFER_FILE_BYTES {
                    bail!("staged file exceeds quota");
                }
                total = total
                    .checked_add(bytes)
                    .context("staging byte count overflow")?;
                if total > crate::provider::MAX_OPERATION_STAGING_BYTES {
                    bail!("operation staging quota exceeded");
                }
            }
        }
    }
    Ok(total)
}

fn ensure_owned_staging_containment(root: &Path, target: &Path) -> Result<()> {
    let canonical_root = fs::canonicalize(root).context("canonicalize owned staging root")?;
    let canonical_target = fs::canonicalize(target).context("canonicalize staged target")?;
    if !canonical_target.starts_with(&canonical_root) {
        bail!("staged target escaped its owned root");
    }
    let relative = canonical_target
        .strip_prefix(&canonical_root)
        .context("derive staged target containment")?;
    if relative
        .components()
        .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        bail!("staged target containment is invalid");
    }
    Ok(())
}

#[cfg(windows)]
#[expect(
    unsafe_code,
    reason = "staging admission queries the containing Windows volume"
)]
fn ensure_staging_free_space(path: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt as _;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetDiskFreeSpaceExW(
            directory: *const u16,
            available: *mut u64,
            total: *mut u64,
            free: *mut u64,
        ) -> i32;
    }
    let mut wide = path.as_os_str().encode_wide().collect::<Vec<_>>();
    wide.push(0);
    let mut available = 0_u64;
    let mut total = 0_u64;
    let mut free = 0_u64;
    // SAFETY: all output pointers are valid and the input is a live NUL-terminated UTF-16 path.
    if unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &raw mut available,
            &raw mut total,
            &raw mut free,
        )
    } == 0
    {
        bail!("staging volume capacity is unavailable");
    }
    let reserve = crate::provider::MINIMUM_FREE_SPACE_RESERVE_BYTES.max(total / 20);
    if available <= reserve {
        bail!("staging volume free-space reserve would be violated");
    }
    Ok(())
}

#[cfg(not(windows))]
fn ensure_staging_free_space(_: &Path) -> Result<()> {
    Ok(())
}

fn copy_local_tree(source: &Path, target: &Path) -> Result<()> {
    copy_local_tree_progress(source, target, &|_| {})
}

fn copy_local_file_progress(
    source: &Path,
    target: &Path,
    progress: &(dyn Fn(u64) + Send + Sync),
) -> Result<()> {
    let mut input =
        fs::File::open(source).with_context(|| format!("open copy source {}", source.display()))?;
    let mut output = fs::File::create(target)
        .with_context(|| format!("create copy destination {}", target.display()))?;
    let mut buffer = vec![0_u8; 256 * 1024];
    loop {
        let read = input.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        output.write_all(&buffer[..read])?;
        progress(read as u64);
    }
    output.flush()?;
    Ok(())
}

fn copy_local_tree_progress(
    source: &Path,
    target: &Path,
    progress: &(dyn Fn(u64) + Send + Sync),
) -> Result<()> {
    fs::create_dir_all(target)
        .with_context(|| format!("create copied directory {}", target.display()))?;
    let mut pending = vec![(source.to_path_buf(), target.to_path_buf(), 0_usize)];
    let mut visited = 0_usize;
    while let Some((from, to, depth)) = pending.pop() {
        visited = visited.saturating_add(1);
        if visited > 100_000 || depth > 64 {
            bail!("local copy tree exceeds safety limits");
        }
        for entry in fs::read_dir(&from)
            .with_context(|| format!("enumerate copied directory {}", from.display()))?
        {
            let entry = entry?;
            let metadata = entry.file_type()?;
            let child_target = to.join(entry.file_name());
            if metadata.is_symlink() {
                bail!("local symbolic links are not followed during remote transfer");
            }
            if metadata.is_dir() {
                fs::create_dir_all(&child_target)?;
                pending.push((entry.path(), child_target, depth + 1));
            } else {
                copy_local_file_progress(&entry.path(), &child_target, progress)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicU64 as TestAtomicU64, AtomicUsize, Ordering as AtomicOrdering},
    };

    use super::*;
    use crate::{RemoteEntry, RemoteEntryKind, RemoteMetadata, RemoteProvider};
    use explorer_model::VirtualLocationDescriptor;

    struct FakeProvider {
        fail_delete: bool,
        delete_calls: Arc<AtomicUsize>,
    }

    struct FailingProvider;

    struct TreeProvider {
        unknown_leaf: bool,
    }

    impl RemoteProvider for TreeProvider {
        fn provider_id(&self) -> &'static str {
            "tree"
        }
        fn list(
            &self,
            location: &VirtualLocationDescriptor,
            cancellation: &CancellationToken,
        ) -> Result<Vec<RemoteEntry>> {
            if cancellation.is_cancelled() {
                bail!("cancelled");
            }
            let child = |name: &str, kind, size| {
                let mut descriptor = location.clone();
                descriptor.components.push(name.to_owned());
                RemoteEntry {
                    name: name.to_owned(),
                    location: LocationDescriptor::Virtual(descriptor),
                    kind,
                    size,
                    unix_mode: None,
                }
            };
            Ok(if location.components == ["root"] {
                vec![
                    child("first.bin", RemoteEntryKind::File, Some(10)),
                    child("nested", RemoteEntryKind::Directory, None),
                ]
            } else if location.components == ["root", "nested"] {
                vec![child(
                    "second.bin",
                    RemoteEntryKind::File,
                    (!self.unknown_leaf).then_some(20),
                )]
            } else {
                Vec::new()
            })
        }
        fn metadata(
            &self,
            location: &VirtualLocationDescriptor,
            _: &CancellationToken,
        ) -> Result<RemoteMetadata> {
            Ok(RemoteMetadata {
                location: LocationDescriptor::Virtual(location.clone()),
                kind: RemoteEntryKind::Directory,
                size: None,
                unix_mode: None,
                modified_unix_seconds: None,
            })
        }
        fn download(
            &self,
            _: &VirtualLocationDescriptor,
            _: &Path,
            _: &CancellationToken,
        ) -> Result<()> {
            Ok(())
        }
        fn upload(
            &self,
            _: &Path,
            _: &VirtualLocationDescriptor,
            _: &CancellationToken,
        ) -> Result<()> {
            Ok(())
        }
        fn create_directory(
            &self,
            _: &VirtualLocationDescriptor,
            _: &CancellationToken,
        ) -> Result<()> {
            Ok(())
        }
        fn rename(
            &self,
            _: &VirtualLocationDescriptor,
            _: &VirtualLocationDescriptor,
            _: &CancellationToken,
        ) -> Result<()> {
            Ok(())
        }
        fn delete(
            &self,
            _: &VirtualLocationDescriptor,
            _: bool,
            _: &CancellationToken,
        ) -> Result<()> {
            Ok(())
        }
    }

    impl RemoteProvider for FailingProvider {
        fn provider_id(&self) -> &'static str {
            "fail"
        }
        fn list(
            &self,
            _: &VirtualLocationDescriptor,
            _: &CancellationToken,
        ) -> Result<Vec<RemoteEntry>> {
            Ok(Vec::new())
        }
        fn download(
            &self,
            _: &VirtualLocationDescriptor,
            _: &Path,
            _: &CancellationToken,
        ) -> Result<()> {
            bail!("adb pull failed: device offline")
        }
        fn upload(
            &self,
            _: &Path,
            _: &VirtualLocationDescriptor,
            _: &CancellationToken,
        ) -> Result<()> {
            bail!("sftp upload denied password=should-not-leak")
        }
        fn create_directory(
            &self,
            _: &VirtualLocationDescriptor,
            _: &CancellationToken,
        ) -> Result<()> {
            Ok(())
        }
        fn rename(
            &self,
            _: &VirtualLocationDescriptor,
            _: &VirtualLocationDescriptor,
            _: &CancellationToken,
        ) -> Result<()> {
            Ok(())
        }
        fn delete(
            &self,
            _: &VirtualLocationDescriptor,
            _: bool,
            _: &CancellationToken,
        ) -> Result<()> {
            Ok(())
        }
    }

    impl RemoteProvider for FakeProvider {
        fn provider_id(&self) -> &'static str {
            "fake"
        }
        fn list(
            &self,
            _: &VirtualLocationDescriptor,
            _: &CancellationToken,
        ) -> Result<Vec<RemoteEntry>> {
            Ok(Vec::new())
        }
        fn download(
            &self,
            _: &VirtualLocationDescriptor,
            local: &Path,
            _: &CancellationToken,
        ) -> Result<()> {
            fs::write(local, b"remote")?;
            Ok(())
        }
        fn upload(
            &self,
            local: &Path,
            _: &VirtualLocationDescriptor,
            _: &CancellationToken,
        ) -> Result<()> {
            let _ = fs::read(local)?;
            Ok(())
        }
        fn create_directory(
            &self,
            _: &VirtualLocationDescriptor,
            _: &CancellationToken,
        ) -> Result<()> {
            Ok(())
        }
        fn rename(
            &self,
            _: &VirtualLocationDescriptor,
            _: &VirtualLocationDescriptor,
            _: &CancellationToken,
        ) -> Result<()> {
            Ok(())
        }
        fn delete(
            &self,
            _: &VirtualLocationDescriptor,
            _: bool,
            _: &CancellationToken,
        ) -> Result<()> {
            self.delete_calls.fetch_add(1, AtomicOrdering::AcqRel);
            if self.fail_delete {
                bail!("fixture delete failure")
            } else {
                Ok(())
            }
        }
    }

    fn remote(name: &str) -> LocationDescriptor {
        LocationDescriptor::try_virtual("fake", [1; 16], 1, None, vec![name.into()]).unwrap()
    }

    fn tree_root() -> LocationDescriptor {
        LocationDescriptor::try_virtual("tree", [3; 16], 1, None, vec!["root".into()]).unwrap()
    }

    #[test]
    fn remote_tree_estimator_recurses_and_degrades_unknown_or_cancelled() {
        let destination = LocationDescriptor::file_system(r"C:\destination");
        for (unknown_leaf, expected) in [(false, Some(30)), (true, None)] {
            let mut registry = RemoteProviderRegistry::default();
            registry
                .register(Arc::new(TreeProvider { unknown_leaf }))
                .unwrap();
            assert_eq!(
                TransferEngine::new(&registry).estimate_work_bytes(
                    &tree_root(),
                    &destination,
                    &CancellationToken::new(),
                ),
                expected
            );
        }
        let mut registry = RemoteProviderRegistry::default();
        registry
            .register(Arc::new(TreeProvider {
                unknown_leaf: false,
            }))
            .unwrap();
        let cancellation = CancellationToken::new();
        cancellation.cancel();
        assert_eq!(
            TransferEngine::new(&registry).estimate_work_bytes(
                &tree_root(),
                &destination,
                &cancellation,
            ),
            None
        );
    }

    fn failing_remote(name: &str) -> LocationDescriptor {
        LocationDescriptor::try_virtual("fail", [2; 16], 1, None, vec![name.into()]).unwrap()
    }

    #[test]
    fn remote_to_remote_copy_uses_scoped_staging() {
        let mut registry = RemoteProviderRegistry::default();
        registry
            .register(Arc::new(FakeProvider {
                fail_delete: false,
                delete_calls: Arc::new(AtomicUsize::new(0)),
            }))
            .unwrap();
        let outcome = TransferEngine::new(&registry).transfer(
            remote("a"),
            remote("b"),
            TransferMode::Copy,
            &CancellationToken::new(),
        );
        assert_eq!(outcome.result, TransferResult::Succeeded);
    }

    #[test]
    fn local_copy_reports_successfully_written_byte_chunks() {
        let registry = RemoteProviderRegistry::default();
        let source_root = tempfile::tempdir().unwrap();
        let destination = tempfile::tempdir().unwrap();
        let source = source_root.path().join("large.bin");
        let bytes = vec![0x5a; 1024 * 1024 + 17];
        fs::write(&source, &bytes).unwrap();
        let reported = TestAtomicU64::new(0);

        let outcome = TransferEngine::new(&registry).transfer_with_conflict_and_progress(
            LocationDescriptor::file_system(source),
            LocationDescriptor::file_system(destination.path().to_path_buf()),
            TransferMode::Copy,
            ConflictDecision::Replace,
            &CancellationToken::new(),
            &|delta| {
                reported.fetch_add(delta, AtomicOrdering::AcqRel);
            },
        );

        assert_eq!(outcome.result, TransferResult::Succeeded);
        assert_eq!(reported.load(AtomicOrdering::Acquire), bytes.len() as u64);
        assert_eq!(
            fs::read(destination.path().join("large.bin")).unwrap(),
            bytes
        );
    }

    #[test]
    fn move_reports_partial_when_source_delete_fails() {
        let mut registry = RemoteProviderRegistry::default();
        registry
            .register(Arc::new(FakeProvider {
                fail_delete: true,
                delete_calls: Arc::new(AtomicUsize::new(0)),
            }))
            .unwrap();
        let outcome = TransferEngine::new(&registry).transfer(
            remote("a"),
            remote("b"),
            TransferMode::Move,
            &CancellationToken::new(),
        );
        let TransferResult::Partial { stage, diagnostic } = outcome.result else {
            panic!("move delete failure must be partial")
        };
        assert_eq!(stage, TransferStage::SourceDelete);
        assert!(diagnostic.contains("fixture delete failure"));
    }

    #[test]
    fn skipped_conflict_never_deletes_move_source_and_keep_both_is_bounded() {
        let registry = RemoteProviderRegistry::default();
        let source_root = tempfile::tempdir().unwrap();
        let destination = tempfile::tempdir().unwrap();
        let source = source_root.path().join("report.txt");
        fs::write(&source, b"new").unwrap();
        fs::write(destination.path().join("report.txt"), b"old").unwrap();
        let skipped = TransferEngine::new(&registry).transfer_with_conflict(
            LocationDescriptor::file_system(source.clone()),
            LocationDescriptor::file_system(destination.path().to_path_buf()),
            TransferMode::Move,
            ConflictDecision::Skip,
            &CancellationToken::new(),
        );
        assert_eq!(skipped.result, TransferResult::Skipped);
        assert!(source.exists());

        let copied = TransferEngine::new(&registry).transfer_with_conflict(
            LocationDescriptor::file_system(source),
            LocationDescriptor::file_system(destination.path().to_path_buf()),
            TransferMode::Copy,
            ConflictDecision::KeepBoth,
            &CancellationToken::new(),
        );
        assert_eq!(copied.result, TransferResult::Succeeded);
        assert_eq!(
            fs::read(destination.path().join("report (2).txt")).unwrap(),
            b"new"
        );
    }

    #[test]
    fn process_staging_quota_accepts_boundary_and_releases_exactly_once() {
        PROCESS_STAGING_BYTES.store(0, Ordering::Release);
        let reservation = StagingReservation::acquire(crate::provider::MAX_PROCESS_STAGING_BYTES)
            .expect("exact process boundary");
        assert!(StagingReservation::acquire(1).is_err());
        drop(reservation);
        assert_eq!(PROCESS_STAGING_BYTES.load(Ordering::Acquire), 0);
    }

    #[test]
    fn cancellation_before_copy_never_reaches_source_delete() {
        let delete_calls = Arc::new(AtomicUsize::new(0));
        let mut registry = RemoteProviderRegistry::default();
        registry
            .register(Arc::new(FakeProvider {
                fail_delete: false,
                delete_calls: Arc::clone(&delete_calls),
            }))
            .unwrap();
        let cancellation = CancellationToken::new();
        cancellation.cancel();
        let outcome = TransferEngine::new(&registry).transfer(
            remote("a"),
            remote("b"),
            TransferMode::Move,
            &cancellation,
        );
        assert_eq!(outcome.result, TransferResult::Cancelled);
        assert_eq!(delete_calls.load(AtomicOrdering::Acquire), 0);
    }

    #[test]
    fn transfer_failure_retains_stage_provider_reason_and_redacts_credentials() {
        let mut registry = RemoteProviderRegistry::default();
        registry.register(Arc::new(FailingProvider)).unwrap();
        let source_root = tempfile::tempdir().unwrap();
        let source = source_root.path().join("report.txt");
        fs::write(&source, b"report").unwrap();

        let outcome = TransferEngine::new(&registry).transfer(
            LocationDescriptor::file_system(source),
            failing_remote("Download"),
            TransferMode::Copy,
            &CancellationToken::new(),
        );
        let TransferResult::Failed { stage, diagnostic } = outcome.result else {
            panic!("upload must fail")
        };
        assert_eq!(stage, TransferStage::DestinationUpload);
        assert!(diagnostic.contains("sftp upload denied"));
        assert!(diagnostic.contains("password=[已隱藏]"));
        assert!(!diagnostic.contains("should-not-leak"));
    }

    #[test]
    fn remote_download_failure_retains_source_stage_and_reason() {
        let mut registry = RemoteProviderRegistry::default();
        registry.register(Arc::new(FailingProvider)).unwrap();
        let destination = tempfile::tempdir().unwrap();
        let outcome = TransferEngine::new(&registry).transfer(
            failing_remote("report.txt"),
            LocationDescriptor::file_system(destination.path()),
            TransferMode::Copy,
            &CancellationToken::new(),
        );
        let TransferResult::Failed { stage, diagnostic } = outcome.result else {
            panic!("download must fail")
        };
        assert_eq!(stage, TransferStage::SourceDownload);
        assert!(diagnostic.contains("device offline"));
        assert!(!diagnostic.contains("superexplorer-remote-transfer"));
    }

    #[test]
    fn diagnostic_sanitizer_redacts_uri_userinfo_and_empty_reason() {
        assert_eq!(sanitize_transfer_diagnostic("  "), "未提供底層錯誤");
        let sanitized = sanitize_transfer_diagnostic(
            "connect sftp://root:secret@example.test/home token=abc123 refused",
        );
        assert!(sanitized.contains("sftp://[已隱藏]@example.test/home"));
        assert!(sanitized.contains("token=[已隱藏]"));
        assert!(!sanitized.contains("root:secret"));
        assert!(!sanitized.contains("abc123"));
    }
}
