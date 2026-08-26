//! Cross-filesystem copy/move using bounded scoped staging.

use std::{
    fs,
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
    Partial { diagnostic: String },
    Failed { diagnostic: String },
    Cancelled,
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
        let result = if cancellation.is_cancelled() {
            TransferResult::Cancelled
        } else {
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                self.copy(&source, &destination, conflict, cancellation)
            })) {
                Err(_) => TransferResult::Failed {
                    diagnostic: "transfer provider panicked".to_owned(),
                },
                Ok(copy_result) => match copy_result {
                    Ok(false) => TransferResult::Skipped,
                    Ok(true) if mode == TransferMode::Copy => TransferResult::Succeeded,
                    Ok(true) => match self.delete_source(&source, cancellation) {
                        Ok(()) => TransferResult::Succeeded,
                        Err(_) => TransferResult::Partial {
                            diagnostic: "copy completed but source deletion failed".to_owned(),
                        },
                    },
                    Err(_error) if cancellation.is_cancelled() => TransferResult::Cancelled,
                    Err(_) => TransferResult::Failed {
                        diagnostic: "transfer failed".to_owned(),
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
    ) -> Result<bool> {
        match (source, destination) {
            (
                LocationDescriptor::FileSystem(source),
                LocationDescriptor::FileSystem(destination),
            ) => copy_local_with_conflict(source, destination, conflict),
            (LocationDescriptor::FileSystem(source), LocationDescriptor::Virtual(destination)) => {
                let name = source
                    .file_name()
                    .and_then(|name| name.to_str())
                    .context("source has no UTF-8 file name")?;
                let plan =
                    self.remote_destination_plan(destination, name, conflict, cancellation)?;
                if matches!(plan, ConflictPlan::Skip) {
                    return Ok(false);
                }
                let renamed;
                let upload_source = if let ConflictPlan::KeepBoth(name) = plan {
                    renamed = staged_with_name(source, &name)?;
                    renamed.1.as_path()
                } else {
                    source
                };
                self.providers
                    .resolve(&LocationDescriptor::Virtual(destination.clone()))?
                    .upload(upload_source, destination, cancellation)?;
                Ok(true)
            }
            (LocationDescriptor::Virtual(source), LocationDescriptor::FileSystem(destination)) => {
                let target = if destination.is_dir() {
                    let name = source
                        .components
                        .last()
                        .context("remote source has no final component")?;
                    crate::provider::validate_windows_component(name)?;
                    destination.join(name)
                } else {
                    destination.clone()
                };
                if !local_destination_allows(&target, conflict)? {
                    return Ok(false);
                }
                self.providers
                    .resolve(&LocationDescriptor::Virtual(source.clone()))?
                    .download(source, &target, cancellation)?;
                Ok(true)
            }
            (LocationDescriptor::Virtual(source), LocationDescriptor::Virtual(destination)) => {
                let staging = tempfile::Builder::new()
                    .prefix("superexplorer-remote-transfer-")
                    .tempdir()
                    .context("create scoped transfer staging")?;
                ensure_staging_free_space(staging.path())?;
                let name = source
                    .components
                    .last()
                    .context("remote source has no final component")?;
                crate::provider::validate_windows_component(name)?;
                let plan =
                    self.remote_destination_plan(destination, name, conflict, cancellation)?;
                if matches!(plan, ConflictPlan::Skip) {
                    return Ok(false);
                }
                let staged = staging.path().join(name);
                self.providers
                    .resolve(&LocationDescriptor::Virtual(source.clone()))?
                    .download(source, &staged, cancellation)?;
                ensure_owned_staging_containment(staging.path(), &staged)?;
                let staged_bytes = staged_tree_bytes(&staged)?;
                if staged_bytes > crate::provider::MAX_OPERATION_STAGING_BYTES {
                    bail!("operation staging quota exceeded");
                }
                let _reservation = StagingReservation::acquire(staged_bytes)?;
                if cancellation.is_cancelled() {
                    bail!("transfer cancelled");
                }
                let renamed;
                let upload_source = if let ConflictPlan::KeepBoth(name) = plan {
                    renamed = staged_with_name(&staged, &name)?;
                    renamed.1.as_path()
                } else {
                    staged.as_path()
                };
                self.providers
                    .resolve(&LocationDescriptor::Virtual(destination.clone()))?
                    .upload(upload_source, destination, cancellation)?;
                Ok(true)
            }
            _ => bail!("unsupported Shell location in remote transfer"),
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

fn copy_local(source: &Path, destination: &Path) -> Result<()> {
    let target = if destination.is_dir() {
        destination.join(source.file_name().context("source has no file name")?)
    } else {
        PathBuf::from(destination)
    };
    if source.is_dir() {
        return copy_local_tree(source, &target);
    }
    fs::copy(source, &target)
        .with_context(|| format!("copy {} to {}", source.display(), target.display()))?;
    Ok(())
}

fn copy_local_with_conflict(
    source: &Path,
    destination: &Path,
    conflict: ConflictDecision,
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
                    copy_local_tree(source, &candidate)?;
                } else {
                    fs::copy(source, candidate)?;
                }
                return Ok(true);
            }
        }
        bail!("keep-both destination name limit exceeded");
    }
    if !local_destination_allows(&target, conflict)? {
        return Ok(false);
    }
    copy_local(source, destination)?;
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

fn staged_tree_bytes(root: &Path) -> Result<u64> {
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
                fs::copy(entry.path(), &child_target)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering as AtomicOrdering},
    };

    use super::*;
    use crate::{RemoteEntry, RemoteProvider};
    use explorer_model::VirtualLocationDescriptor;

    struct FakeProvider {
        fail_delete: bool,
        delete_calls: Arc<AtomicUsize>,
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
        assert!(matches!(outcome.result, TransferResult::Partial { .. }));
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
}
