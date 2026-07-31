#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )
)]
//! Test-only deterministic services and strictly owned real-folder fixtures.
#![allow(
    clippy::must_use_candidate,
    reason = "fixture factories and observations are intentionally convenient in tests"
)]

use std::{
    collections::{BTreeMap, VecDeque},
    fs, io,
    path::{Path, PathBuf},
    sync::Mutex,
};

use tempfile::TempDir;
use thiserror::Error;
use uuid::Uuid;

use explorer_model::{
    BreadcrumbIconHint, BreadcrumbMenuItem, BreadcrumbSegment, BreadcrumbSegmentId,
    BreadcrumbTerminal, ExplorerCommand, ExplorerEvent, FileEntry, FileEntryMetadata,
    LocationDescriptor, ShellItemId, TerminalLedger, TerminalViolation,
};

/// Builds a deterministic large directory model without touching the filesystem or Shell.
pub fn synthetic_directory_entries(count: usize) -> Vec<FileEntry> {
    (0..count)
        .filter_map(|index| {
            let mut identity = vec![b'L'];
            identity.extend_from_slice(&index.to_le_bytes());
            let id = ShellItemId::from_provider_bytes(identity)?;
            let numeric = u64::try_from(index).unwrap_or(u64::MAX);
            let is_container = index % 1_000 == 0;
            let display_name = if is_container {
                format!("folder-{index:08}")
            } else {
                format!("item-{index:08}.txt")
            };
            Some(FileEntry {
                id,
                display_name: display_name.clone(),
                location: LocationDescriptor::file_system(format!(
                    r"C:\model-fixture\{display_name}"
                )),
                is_container,
                metadata: FileEntryMetadata {
                    modified_display: Some("2026-07-28 12:00".to_owned()),
                    modified_sort_key: Some(numeric),
                    size_bytes: (!is_container).then_some(numeric.saturating_mul(17)),
                    type_display: Some(
                        if is_container {
                            "Folder"
                        } else {
                            "Text Document"
                        }
                        .to_owned(),
                    ),
                    ..FileEntryMetadata::default()
                },
            })
        })
        .collect()
}

/// Stable ancestry scenarios shared by fake and real Shell contract suites.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BreadcrumbContractFixture {
    pub name: &'static str,
    pub location: LocationDescriptor,
    pub expected_root_name: &'static str,
    pub expected_leaf_name: &'static str,
    pub requires_real_provider: bool,
}

/// Returns the complete location-shape matrix required by breadcrumb providers.
///
/// Parsing-name and opaque namespace cases are descriptors only: real-provider suites may skip
/// cases unavailable on the current Windows build, while the deterministic suite must still
/// exercise their identity and terminal behavior.
pub fn breadcrumb_contract_fixtures() -> Vec<BreadcrumbContractFixture> {
    vec![
        BreadcrumbContractFixture {
            name: "filesystem-root",
            location: LocationDescriptor::file_system(r"C:\"),
            expected_root_name: "本機",
            expected_leaf_name: "C:",
            requires_real_provider: false,
        },
        BreadcrumbContractFixture {
            name: "drive",
            location: LocationDescriptor::file_system(r"D:\"),
            expected_root_name: "本機",
            expected_leaf_name: "D:",
            requires_real_provider: false,
        },
        BreadcrumbContractFixture {
            name: "nested-path",
            location: LocationDescriptor::file_system(r"D:\fixture\巢狀"),
            expected_root_name: "本機",
            expected_leaf_name: "巢狀",
            requires_real_provider: false,
        },
        BreadcrumbContractFixture {
            name: "unc",
            location: LocationDescriptor::file_system(r"\\server\share\folder"),
            expected_root_name: "本機",
            expected_leaf_name: "folder",
            requires_real_provider: true,
        },
        BreadcrumbContractFixture {
            name: "this-pc",
            location: LocationDescriptor::ParsingName("shell:MyComputerFolder".to_owned()),
            expected_root_name: "本機",
            expected_leaf_name: "本機",
            requires_real_provider: false,
        },
        BreadcrumbContractFixture {
            name: "zip",
            location: LocationDescriptor::ParsingName(r"zipfldr:D:\fixture\archive.zip".to_owned()),
            expected_root_name: "本機",
            expected_leaf_name: "archive.zip",
            requires_real_provider: true,
        },
        BreadcrumbContractFixture {
            name: "libraries",
            location: LocationDescriptor::ParsingName("shell:Libraries".to_owned()),
            expected_root_name: "本機",
            expected_leaf_name: "媒體櫃",
            requires_real_provider: true,
        },
        BreadcrumbContractFixture {
            name: "fake-namespace",
            location: LocationDescriptor::ShellNamespace(b"fake-provider:item-7".to_vec()),
            expected_root_name: "本機",
            expected_leaf_name: "Fake item 7",
            requires_real_provider: false,
        },
    ]
}

/// Provider-neutral breadcrumb contract failure used by fake and real Shell suites.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum BreadcrumbContractViolation {
    #[error("ancestry emitted no batches")]
    MissingAncestry,
    #[error("ancestry identity/order changed after metadata enrichment")]
    UnstableAncestryIdentity,
    #[error("ancestry contains duplicate segment identities")]
    DuplicateAncestryIdentity,
    #[error("child enumeration returned a descendant instead of a direct child")]
    NonDirectChild,
    #[error("child enumeration returned a duplicate stable location")]
    DuplicateChild,
    #[error("request did not emit exactly one terminal event")]
    TerminalCount,
    #[error("provider did not release its request resources")]
    Cleanup,
}

/// Validates the behavior shared by deterministic and Windows Shell breadcrumb providers.
///
/// # Errors
///
/// Returns the first identity, ordering, direct-child, terminal, or cleanup contract violation.
pub fn validate_breadcrumb_contract(
    ancestry_batches: &[Vec<BreadcrumbSegment>],
    parent: &LocationDescriptor,
    child_batches: &[Vec<BreadcrumbMenuItem>],
    ancestry_terminal_count: usize,
    child_terminal_count: usize,
    cleanup_complete: bool,
) -> Result<(), BreadcrumbContractViolation> {
    let Some(first) = ancestry_batches.first() else {
        return Err(BreadcrumbContractViolation::MissingAncestry);
    };
    let expected_ids = first.iter().map(|segment| segment.id).collect::<Vec<_>>();
    let unique_ids = expected_ids
        .iter()
        .copied()
        .collect::<std::collections::HashSet<_>>();
    if unique_ids.len() != expected_ids.len() {
        return Err(BreadcrumbContractViolation::DuplicateAncestryIdentity);
    }
    if ancestry_batches.iter().any(|batch| {
        batch
            .iter()
            .map(|segment| segment.id)
            .ne(expected_ids.iter().copied())
    }) {
        return Err(BreadcrumbContractViolation::UnstableAncestryIdentity);
    }

    let mut child_locations = std::collections::HashSet::new();
    for child in child_batches.iter().flatten() {
        if !child_locations.insert(child.location.clone()) {
            return Err(BreadcrumbContractViolation::DuplicateChild);
        }
        if let (Some(parent), Some(path)) = (parent.path(), child.location.path())
            && path.parent() != Some(parent)
            && path
                .parent()
                .and_then(|value| value.canonicalize().ok())
                .as_deref()
                != parent.canonicalize().ok().as_deref()
        {
            return Err(BreadcrumbContractViolation::NonDirectChild);
        }
    }
    if ancestry_terminal_count != 1 || child_terminal_count != 1 {
        return Err(BreadcrumbContractViolation::TerminalCount);
    }
    if !cleanup_complete {
        return Err(BreadcrumbContractViolation::Cleanup);
    }
    Ok(())
}

/// Immediate thread-safe fake implementing the same production service endpoint as Windows.
#[derive(Debug, Default)]
pub struct ImmediateNavigationService {
    events: Mutex<VecDeque<ExplorerEvent>>,
}

impl explorer_model::ExplorerService for ImmediateNavigationService {
    fn submit(&self, command: ExplorerCommand) -> Result<(), explorer_model::ExplorerServiceError> {
        let mut events = self
            .events
            .lock()
            .map_err(|_| explorer_model::ExplorerServiceError::Internal)?;
        match command {
            ExplorerCommand::Navigate { context, location }
            | ExplorerCommand::Refresh { context, location } => {
                if context.cancellation.is_cancelled() {
                    events.push_back(ExplorerEvent::Failed {
                        context,
                        error: explorer_common::ExplorerError::new(
                            explorer_common::ExplorerErrorKind::Cancellation,
                            "navigate",
                            true,
                            "已取消資料夾載入。",
                            "deterministic cancellation",
                        ),
                    });
                    return Ok(());
                }
                events.push_back(ExplorerEvent::LocationResolved {
                    context: context.clone(),
                    metadata: explorer_model::LocationMetadata {
                        descriptor: location,
                        display_title: "fixture".to_owned(),
                        can_go_up: true,
                        can_write: true,
                    },
                });
                events.push_back(ExplorerEvent::DirectoryFinished { context });
                Ok(())
            }
            ExplorerCommand::ResolveAncestry { context, location } => {
                let outcome = if context.cancellation.is_cancelled() {
                    BreadcrumbTerminal::Cancelled
                } else {
                    let display_name = location
                        .path()
                        .and_then(|path| {
                            path.file_name().or_else(|| {
                                path.components()
                                    .next_back()
                                    .map(std::path::Component::as_os_str)
                            })
                        })
                        .map(|name| name.to_string_lossy().into_owned())
                        .filter(|name| !name.is_empty())
                        .unwrap_or_else(|| "本機".to_owned());
                    events.push_back(ExplorerEvent::AncestryBatch {
                        context: context.clone(),
                        segments: vec![BreadcrumbSegment {
                            id: BreadcrumbSegmentId(1),
                            display_name,
                            location,
                            icon_hint: BreadcrumbIconHint::Folder,
                            is_container: true,
                        }],
                    });
                    BreadcrumbTerminal::Finished
                };
                events.push_back(ExplorerEvent::AncestryFinished { context, outcome });
                Ok(())
            }
            ExplorerCommand::EnumerateChildContainers {
                context,
                segment_id,
                menu_generation,
                ..
            } => {
                let outcome = if context.cancellation.is_cancelled() {
                    BreadcrumbTerminal::Cancelled
                } else {
                    BreadcrumbTerminal::Empty
                };
                events.push_back(ExplorerEvent::ChildContainersFinished {
                    context,
                    segment_id,
                    menu_generation,
                    outcome,
                });
                Ok(())
            }
            _ => Err(explorer_model::ExplorerServiceError::Internal),
        }
    }

    fn try_recv(&self) -> Result<Option<ExplorerEvent>, explorer_model::ExplorerServiceError> {
        self.events
            .lock()
            .map_err(|_| explorer_model::ExplorerServiceError::Internal)
            .map(|mut events| events.pop_front())
    }
}

const MARKER_FILE: &str = ".explorer-test-fixture";

/// Safety error raised before a fixture can mutate an unowned path.
#[derive(Debug, Error)]
pub enum FixtureError {
    #[error("fixture path does not exist or cannot be resolved: {0}")]
    Unresolved(PathBuf),
    #[error("fixture refused unsafe root: {0}")]
    UnsafeRoot(PathBuf),
    #[error("target escapes the owned fixture root: {0}")]
    OutsideRoot(PathBuf),
    #[error("fixture root itself cannot be a destructive target")]
    RootTarget,
    #[error("fixture ownership marker is missing or invalid")]
    InvalidMarker,
    #[error("fixture I/O failed during {operation}: {source}")]
    Io {
        operation: &'static str,
        #[source]
        source: io::Error,
    },
}

/// Unique temporary directory whose mutation helpers enforce canonical ownership.
#[derive(Debug)]
pub struct OwnedTempFixture {
    directory: TempDir,
    resolved_root: PathBuf,
    marker: String,
}

/// Deterministic oracle for a generated flat large-folder dataset.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LargeDatasetOracle {
    pub root: PathBuf,
    pub item_count: usize,
}

impl LargeDatasetOracle {
    pub fn expected_name(index: usize) -> String {
        format!("item-{index:06}.dat")
    }
}

impl OwnedTempFixture {
    /// Creates a unique OS temporary directory and an ownership marker.
    ///
    /// # Errors
    ///
    /// Returns an I/O or root-safety error before exposing the fixture.
    pub fn new() -> Result<Self, FixtureError> {
        let directory = tempfile::Builder::new()
            .prefix("rust-gpui-explorer-")
            .tempdir()
            .map_err(|source| FixtureError::Io {
                operation: "create fixture root",
                source,
            })?;
        Self::from_temp_dir(directory)
    }

    /// Creates an owned fixture beneath an explicit existing base for cross-volume tests.
    ///
    /// # Errors
    ///
    /// Returns an I/O or root-safety error before exposing the fixture.
    pub fn new_in(base: impl AsRef<Path>) -> Result<Self, FixtureError> {
        let directory = tempfile::Builder::new()
            .prefix("rust-gpui-explorer-")
            .tempdir_in(base)
            .map_err(|source| FixtureError::Io {
                operation: "create fixture root in explicit base",
                source,
            })?;
        Self::from_temp_dir(directory)
    }

    fn from_temp_dir(directory: TempDir) -> Result<Self, FixtureError> {
        let resolved_root = directory
            .path()
            .canonicalize()
            .map_err(|_| FixtureError::Unresolved(directory.path().to_path_buf()))?;
        validate_safe_root(&resolved_root)?;
        let marker = Uuid::new_v4().to_string();
        fs::write(resolved_root.join(MARKER_FILE), marker.as_bytes()).map_err(|source| {
            FixtureError::Io {
                operation: "write fixture marker",
                source,
            }
        })?;
        Ok(Self {
            directory,
            resolved_root,
            marker,
        })
    }

    /// Returns the canonical root owned by this fixture.
    pub fn root(&self) -> &Path {
        debug_assert_eq!(
            self.directory.path().canonicalize().ok().as_deref(),
            Some(self.resolved_root.as_path())
        );
        &self.resolved_root
    }

    /// Creates an owned subdirectory beneath an already-resolved parent.
    ///
    /// # Errors
    ///
    /// Rejects escaping/unresolved parents and propagates filesystem failures.
    pub fn create_dir(&self, relative: impl AsRef<Path>) -> Result<PathBuf, FixtureError> {
        let destination = self.resolve_new_target(relative.as_ref())?;
        fs::create_dir(&destination).map_err(|source| FixtureError::Io {
            operation: "create directory",
            source,
        })?;
        destination
            .canonicalize()
            .map_err(|_| FixtureError::Unresolved(destination))
    }

    /// Creates or replaces an owned file with deterministic bytes.
    ///
    /// # Errors
    ///
    /// Rejects escaping/unresolved parents and propagates filesystem failures.
    pub fn create_file(
        &self,
        relative: impl AsRef<Path>,
        bytes: &[u8],
    ) -> Result<PathBuf, FixtureError> {
        let destination = self.resolve_new_target(relative.as_ref())?;
        fs::write(&destination, bytes).map_err(|source| FixtureError::Io {
            operation: "create file",
            source,
        })?;
        destination
            .canonicalize()
            .map_err(|_| FixtureError::Unresolved(destination))
    }

    /// Generates a deterministic flat dataset for high-volume enumeration measurements.
    ///
    /// # Errors
    ///
    /// Validates fixture ownership before generation and returns the first filesystem failure.
    pub fn generate_large_dataset(
        &self,
        item_count: usize,
    ) -> Result<LargeDatasetOracle, FixtureError> {
        self.verify_marker()?;
        let root = self.resolve_new_target(Path::new("large-dataset"))?;
        fs::create_dir(&root).map_err(|source| FixtureError::Io {
            operation: "create large dataset root",
            source,
        })?;
        for index in 0..item_count {
            fs::File::create(root.join(LargeDatasetOracle::expected_name(index))).map_err(
                |source| FixtureError::Io {
                    operation: "create large dataset item",
                    source,
                },
            )?;
        }
        Ok(LargeDatasetOracle { root, item_count })
    }

    /// Renames one existing owned item to a new owned destination.
    ///
    /// # Errors
    ///
    /// Rejects unresolved sources, unsafe destinations, and filesystem failures.
    pub fn rename(
        &self,
        source: impl AsRef<Path>,
        destination: impl AsRef<Path>,
    ) -> Result<PathBuf, FixtureError> {
        let source = self.resolve_existing_target(source.as_ref())?;
        let destination = self.resolve_new_target(destination.as_ref())?;
        fs::rename(&source, &destination).map_err(|source| FixtureError::Io {
            operation: "rename fixture item",
            source,
        })?;
        destination
            .canonicalize()
            .map_err(|_| FixtureError::Unresolved(destination))
    }

    /// Copies one existing owned file to a new owned destination.
    ///
    /// # Errors
    ///
    /// Rejects unresolved/escaping paths and filesystem failures.
    pub fn copy_file(
        &self,
        source: impl AsRef<Path>,
        destination: impl AsRef<Path>,
    ) -> Result<PathBuf, FixtureError> {
        let source = self.resolve_existing_target(source.as_ref())?;
        let destination = self.resolve_new_target(destination.as_ref())?;
        fs::copy(&source, &destination).map_err(|source| FixtureError::Io {
            operation: "copy fixture file",
            source,
        })?;
        destination
            .canonicalize()
            .map_err(|_| FixtureError::Unresolved(destination))
    }

    /// Moves one existing owned item to a new owned destination.
    ///
    /// # Errors
    ///
    /// Rejects unresolved/escaping paths and filesystem failures.
    pub fn move_item(
        &self,
        source: impl AsRef<Path>,
        destination: impl AsRef<Path>,
    ) -> Result<PathBuf, FixtureError> {
        self.rename(source, destination)
    }

    /// Deletes one existing owned file or directory tree after a final ownership check.
    ///
    /// # Errors
    ///
    /// Rejects the fixture root, unresolved paths, reparse escapes, or filesystem failures.
    pub fn delete(&self, target: impl AsRef<Path>) -> Result<(), FixtureError> {
        self.verify_marker()?;
        let target = self.resolve_existing_target(target.as_ref())?;
        let metadata = fs::symlink_metadata(&target).map_err(|source| FixtureError::Io {
            operation: "inspect delete target",
            source,
        })?;
        if metadata.is_dir() {
            fs::remove_dir_all(&target).map_err(|source| FixtureError::Io {
                operation: "delete fixture directory",
                source,
            })
        } else {
            fs::remove_file(&target).map_err(|source| FixtureError::Io {
                operation: "delete fixture file",
                source,
            })
        }
    }

    /// Re-resolves and verifies a destructive target immediately before an external adapter uses
    /// it. This is intentionally separate from deletion so native-operation tests share the same
    /// canonical containment guard.
    ///
    /// # Errors
    ///
    /// Rejects roots, unresolved targets, workspace/drive roots, and reparse escapes.
    pub fn verify_destructive_target(
        &self,
        target: impl AsRef<Path>,
    ) -> Result<PathBuf, FixtureError> {
        self.resolve_existing_target(target.as_ref())
    }

    fn verify_marker(&self) -> Result<(), FixtureError> {
        let marker = fs::read_to_string(self.resolved_root.join(MARKER_FILE))
            .map_err(|_| FixtureError::InvalidMarker)?;
        if marker == self.marker {
            Ok(())
        } else {
            Err(FixtureError::InvalidMarker)
        }
    }

    fn absolute_candidate(&self, path: &Path) -> PathBuf {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.resolved_root.join(path)
        }
    }

    fn resolve_existing_target(&self, path: &Path) -> Result<PathBuf, FixtureError> {
        self.verify_marker()?;
        let candidate = self.absolute_candidate(path);
        let resolved = candidate
            .canonicalize()
            .map_err(|_| FixtureError::Unresolved(candidate.clone()))?;
        self.verify_descendant(&resolved)?;
        Ok(resolved)
    }

    fn resolve_new_target(&self, path: &Path) -> Result<PathBuf, FixtureError> {
        self.verify_marker()?;
        let candidate = self.absolute_candidate(path);
        match fs::symlink_metadata(&candidate) {
            Ok(_) => return self.resolve_existing_target(&candidate),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(source) => {
                return Err(FixtureError::Io {
                    operation: "inspect destination",
                    source,
                });
            }
        }
        let file_name = candidate
            .file_name()
            .ok_or_else(|| FixtureError::OutsideRoot(candidate.clone()))?;
        let parent = candidate
            .parent()
            .ok_or_else(|| FixtureError::OutsideRoot(candidate.clone()))?;
        let resolved_parent = parent
            .canonicalize()
            .map_err(|_| FixtureError::Unresolved(parent.to_path_buf()))?;
        self.verify_root_or_descendant(&resolved_parent)?;
        Ok(resolved_parent.join(file_name))
    }

    fn verify_descendant(&self, resolved: &Path) -> Result<(), FixtureError> {
        if resolved == self.resolved_root {
            return Err(FixtureError::RootTarget);
        }
        self.verify_root_or_descendant(resolved)
    }

    fn verify_root_or_descendant(&self, resolved: &Path) -> Result<(), FixtureError> {
        if resolved.starts_with(&self.resolved_root) {
            Ok(())
        } else {
            Err(FixtureError::OutsideRoot(resolved.to_path_buf()))
        }
    }
}

fn validate_safe_root(root: &Path) -> Result<(), FixtureError> {
    if root.parent().is_none() {
        return Err(FixtureError::UnsafeRoot(root.to_path_buf()));
    }
    for variable in ["USERPROFILE", "HOME"] {
        if let Some(path) = std::env::var_os(variable)
            && Path::new(&path).canonicalize().ok().as_deref() == Some(root)
        {
            return Err(FixtureError::UnsafeRoot(root.to_path_buf()));
        }
    }
    if Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .and_then(|path| path.canonicalize().ok())
        .as_deref()
        == Some(root)
    {
        return Err(FixtureError::UnsafeRoot(root.to_path_buf()));
    }
    Ok(())
}

/// Tick-driven fake Shell service using production command/event domain values.
#[derive(Debug, Default)]
pub struct DeterministicShellService {
    tick: u64,
    commands: VecDeque<ExplorerCommand>,
    scheduled: BTreeMap<u64, VecDeque<ExplorerEvent>>,
    terminal_ledger: TerminalLedger,
}

impl DeterministicShellService {
    /// Submits the exact command type used by the production Shell endpoint.
    ///
    /// # Errors
    ///
    /// Rejects a duplicate request identity.
    pub fn submit(&mut self, command: ExplorerCommand) -> Result<(), TerminalViolation> {
        self.terminal_ledger.register(&command)?;
        self.commands.push_back(command);
        Ok(())
    }

    /// Removes the next command for deterministic fake processing assertions.
    pub fn pop_command(&mut self) -> Option<ExplorerCommand> {
        self.commands.pop_front()
    }

    /// Schedules a production event after a deterministic number of ticks.
    pub fn schedule(&mut self, ticks_from_now: u64, event: ExplorerEvent) {
        let due = self.tick.saturating_add(ticks_from_now);
        self.scheduled.entry(due).or_default().push_back(event);
    }

    /// Simulates the service endpoint closing while queued work is outstanding.
    ///
    /// Every queued request is converted into exactly one recoverable terminal event so tests can
    /// prove that channel teardown never leaves a spinner or protocol ledger entry behind.
    ///
    /// # Errors
    ///
    /// Returns a terminal-ledger violation if teardown would publish a duplicate terminal event.
    pub fn close_channel(&mut self) -> Result<Vec<ExplorerEvent>, TerminalViolation> {
        let mut events = Vec::new();
        while let Some(command) = self.commands.pop_front() {
            let Some(context) = command.context().cloned() else {
                continue;
            };
            let error = explorer_common::ExplorerError::new(
                explorer_common::ExplorerErrorKind::Availability,
                "fake Shell channel",
                true,
                "Shell 服務連線已關閉，請重試。",
                "deterministic channel close",
            );
            let event = match command {
                ExplorerCommand::ResolveAncestry { .. } => ExplorerEvent::AncestryFinished {
                    context,
                    outcome: BreadcrumbTerminal::Failed(error),
                },
                ExplorerCommand::EnumerateChildContainers {
                    segment_id,
                    menu_generation,
                    ..
                } => ExplorerEvent::ChildContainersFinished {
                    context,
                    segment_id,
                    menu_generation,
                    outcome: BreadcrumbTerminal::Failed(error),
                },
                ExplorerCommand::Cancel { .. } => continue,
                _ => ExplorerEvent::Failed { context, error },
            };
            self.terminal_ledger.record_event(&event)?;
            events.push(event);
        }
        self.scheduled.clear();
        Ok(events)
    }

    /// Advances one tick and returns all due events in insertion order.
    ///
    /// # Errors
    ///
    /// Rejects unknown request events or duplicate terminal events.
    pub fn advance(&mut self) -> Result<Vec<ExplorerEvent>, TerminalViolation> {
        self.tick = self.tick.saturating_add(1);
        let due_ticks: Vec<_> = self
            .scheduled
            .range(..=self.tick)
            .map(|(tick, _)| *tick)
            .collect();
        let mut events = Vec::new();
        for due_tick in due_ticks {
            if let Some(mut due) = self.scheduled.remove(&due_tick) {
                while let Some(event) = due.pop_front() {
                    self.terminal_ledger.record_event(&event)?;
                    events.push(event);
                }
            }
        }
        Ok(events)
    }

    /// Verifies that every submitted request received exactly one terminal event.
    ///
    /// # Errors
    ///
    /// Reports the number of outstanding requests.
    pub fn verify_drained(&self) -> Result<(), TerminalViolation> {
        self.terminal_ledger.verify_drained()
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use explorer_model::{
        BreadcrumbIconHint, BreadcrumbMenuItem, BreadcrumbSegment, BreadcrumbSegmentId,
        BreadcrumbTerminal, ExplorerCommand, ExplorerEvent, Generation, LocationDescriptor,
        RequestContext, TabId, TerminalViolation,
    };

    use super::{
        DeterministicShellService, FixtureError, OwnedTempFixture, breadcrumb_contract_fixtures,
        synthetic_directory_entries, validate_breadcrumb_contract, validate_safe_root,
    };

    #[test]
    fn synthetic_large_directory_is_deterministic_and_has_no_io_dependency() {
        let first = synthetic_directory_entries(100_000);
        let second = synthetic_directory_entries(100_000);
        assert_eq!(first.len(), 100_000);
        assert_eq!(first, second);
        assert!(first[0].is_container);
        assert_eq!(first[1].display_name, "item-00000001.txt");
    }

    #[test]
    fn breadcrumb_contract_fixture_matrix_covers_every_location_shape() {
        let fixtures = breadcrumb_contract_fixtures();
        let names = fixtures
            .iter()
            .map(|fixture| fixture.name)
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            names,
            std::collections::BTreeSet::from([
                "filesystem-root",
                "drive",
                "nested-path",
                "unc",
                "this-pc",
                "zip",
                "libraries",
                "fake-namespace",
            ])
        );
        assert_eq!(names.len(), fixtures.len(), "fixture names must be unique");
        assert!(fixtures.iter().all(|fixture| {
            !fixture.expected_root_name.is_empty() && !fixture.expected_leaf_name.is_empty()
        }));
        assert!(matches!(
            fixtures
                .iter()
                .find(|fixture| fixture.name == "fake-namespace")
                .map(|fixture| &fixture.location),
            Some(LocationDescriptor::ShellNamespace(_))
        ));
    }

    #[test]
    fn provider_neutral_breadcrumb_contract_accepts_fake_batches() {
        let parent = LocationDescriptor::file_system(r"D:\fixture");
        let location = LocationDescriptor::file_system(r"D:\fixture\child");
        let segment = BreadcrumbSegment {
            id: BreadcrumbSegmentId(41),
            display_name: "child".to_owned(),
            location: location.clone(),
            icon_hint: BreadcrumbIconHint::Folder,
            is_container: true,
        };
        let mut enriched = segment.clone();
        enriched.display_name = "Shell child".to_owned();
        let children = vec![BreadcrumbMenuItem {
            display_name: "child".to_owned(),
            location,
        }];
        assert_eq!(
            validate_breadcrumb_contract(
                &[vec![segment], vec![enriched]],
                &parent,
                &[children],
                1,
                1,
                true,
            ),
            Ok(())
        );
    }

    #[test]
    fn real_fixture_create_rename_copy_move_delete_stays_owned() -> Result<(), FixtureError> {
        let fixture = OwnedTempFixture::new()?;
        fixture.create_dir("source")?;
        let original = fixture.create_file(r"source\original.txt", b"actual bytes")?;
        let renamed = fixture.rename(&original, r"source\renamed.txt")?;
        let copied = fixture.copy_file(&renamed, "copied.txt")?;
        assert_eq!(
            std::fs::read(&copied).expect("read copied oracle"),
            b"actual bytes"
        );
        let moved = fixture.move_item(&copied, "moved.txt")?;
        assert!(!copied.exists());
        assert!(moved.exists());
        fixture.delete(&moved)?;
        fixture.delete("source")?;
        assert!(!moved.exists());
        Ok(())
    }

    #[test]
    fn destructive_guard_rejects_root_outside_and_unresolved_targets() -> Result<(), FixtureError> {
        let fixture = OwnedTempFixture::new()?;
        assert!(matches!(
            fixture.delete(fixture.root()),
            Err(FixtureError::RootTarget)
        ));
        assert!(matches!(
            fixture.delete(r"C:\"),
            Err(FixtureError::OutsideRoot(_))
        ));
        assert!(matches!(
            fixture.delete("missing.txt"),
            Err(FixtureError::Unresolved(_))
        ));
        assert!(matches!(
            fixture.create_file(r"missing-parent\file.txt", b"no"),
            Err(FixtureError::Unresolved(_))
        ));
        Ok(())
    }

    #[test]
    fn safe_root_guard_rejects_drive_workspace_and_user_profile() {
        assert!(matches!(
            validate_safe_root(Path::new(r"C:\")),
            Err(FixtureError::UnsafeRoot(_))
        ));
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("workspace root")
            .canonicalize()
            .expect("resolve workspace");
        assert!(matches!(
            validate_safe_root(&workspace),
            Err(FixtureError::UnsafeRoot(_))
        ));
        if let Some(profile) = std::env::var_os("USERPROFILE") {
            let profile = Path::new(&profile).canonicalize().expect("resolve profile");
            assert!(matches!(
                validate_safe_root(&profile),
                Err(FixtureError::UnsafeRoot(_))
            ));
        }
    }

    #[test]
    fn reparse_escape_is_rejected_when_windows_allows_test_symlink() -> Result<(), FixtureError> {
        let fixture = OwnedTempFixture::new()?;
        let outside = tempfile::tempdir().map_err(|source| FixtureError::Io {
            operation: "create outside oracle",
            source,
        })?;
        let link = fixture.root().join("escape-link");
        if std::os::windows::fs::symlink_dir(outside.path(), &link).is_err() {
            return Ok(());
        }
        assert!(matches!(
            fixture.delete(&link),
            Err(FixtureError::OutsideRoot(_))
        ));
        Ok(())
    }

    #[test]
    fn deterministic_fake_uses_production_protocol_and_terminal_contract() {
        let context = RequestContext::new(TabId::new(), Generation::new(1));
        let mut service = DeterministicShellService::default();
        service
            .submit(ExplorerCommand::Navigate {
                context: context.clone(),
                location: LocationDescriptor::file_system(r"C:\fixture"),
            })
            .expect("submit production command");
        assert!(matches!(
            service.pop_command(),
            Some(ExplorerCommand::Navigate { .. })
        ));
        service.schedule(
            2,
            ExplorerEvent::DirectoryFinished {
                context: context.clone(),
            },
        );
        assert!(service.advance().expect("tick one").is_empty());
        assert_eq!(service.advance().expect("tick two").len(), 1);
        assert_eq!(service.verify_drained(), Ok(()));

        service.schedule(1, ExplorerEvent::DirectoryFinished { context });
        assert!(matches!(
            service.advance(),
            Err(TerminalViolation::UnknownOrDuplicateTerminal(_))
        ));
    }

    #[test]
    fn deterministic_fake_models_delayed_breadcrumb_batches_partial_and_stale_events() {
        let context = RequestContext::new(TabId::new(), Generation::new(4));
        let mut service = DeterministicShellService::default();
        service
            .submit(ExplorerCommand::ResolveAncestry {
                context: context.clone(),
                location: LocationDescriptor::file_system(r"D:\fixture\nested"),
            })
            .expect("submit ancestry");
        service.schedule(
            1,
            ExplorerEvent::AncestryBatch {
                context: context.clone(),
                segments: vec![BreadcrumbSegment {
                    id: BreadcrumbSegmentId(7),
                    display_name: "nested".into(),
                    location: LocationDescriptor::file_system(r"D:\fixture\nested"),
                    icon_hint: BreadcrumbIconHint::Folder,
                    is_container: true,
                }],
            },
        );
        service.schedule(
            3,
            ExplorerEvent::AncestryBatch {
                context: context.clone(),
                segments: vec![BreadcrumbSegment {
                    id: BreadcrumbSegmentId(7),
                    display_name: "Shell nested".into(),
                    location: LocationDescriptor::file_system(r"D:\fixture\nested"),
                    icon_hint: BreadcrumbIconHint::Folder,
                    is_container: true,
                }],
            },
        );
        service.schedule(
            4,
            ExplorerEvent::AncestryFinished {
                context: context.clone(),
                outcome: BreadcrumbTerminal::Finished,
            },
        );
        assert_eq!(service.advance().expect("early batch").len(), 1);
        assert!(service.advance().expect("slow provider tick").is_empty());
        assert_eq!(service.advance().expect("metadata batch").len(), 1);
        assert_eq!(service.advance().expect("terminal").len(), 1);
        assert_eq!(service.verify_drained(), Ok(()));

        let menu_context = RequestContext::new(context.tab_id, context.generation);
        service
            .submit(ExplorerCommand::EnumerateChildContainers {
                context: menu_context.clone(),
                parent: LocationDescriptor::file_system(r"D:\fixture"),
                segment_id: BreadcrumbSegmentId(7),
                menu_generation: 2,
            })
            .expect("submit menu");
        service.schedule(
            1,
            ExplorerEvent::ChildContainersBatch {
                context: menu_context.clone(),
                segment_id: BreadcrumbSegmentId(7),
                menu_generation: 2,
                children: vec![BreadcrumbMenuItem {
                    display_name: "child".into(),
                    location: LocationDescriptor::file_system(r"D:\fixture\child"),
                }],
            },
        );
        service.schedule(
            2,
            ExplorerEvent::ChildContainersFinished {
                context: menu_context.clone(),
                segment_id: BreadcrumbSegmentId(7),
                menu_generation: 2,
                outcome: BreadcrumbTerminal::Partial(explorer_common::ExplorerError::new(
                    explorer_common::ExplorerErrorKind::Availability,
                    "fake slow provider",
                    true,
                    "部分資料夾無法列出。",
                    "deterministic partial failure",
                )),
            },
        );
        assert_eq!(service.advance().expect("child batch").len(), 1);
        assert_eq!(service.advance().expect("partial terminal").len(), 1);
        assert_eq!(service.verify_drained(), Ok(()));

        service.schedule(
            1,
            ExplorerEvent::ChildContainersFinished {
                context: menu_context,
                segment_id: BreadcrumbSegmentId(7),
                menu_generation: 2,
                outcome: BreadcrumbTerminal::Cancelled,
            },
        );
        assert!(matches!(
            service.advance(),
            Err(TerminalViolation::UnknownOrDuplicateTerminal(_))
        ));
    }

    #[test]
    fn deterministic_fake_channel_close_finishes_every_breadcrumb_request_once() {
        let tab_id = TabId::new();
        let generation = Generation::new(9);
        let ancestry = RequestContext::new(tab_id, generation);
        let menu = RequestContext::new(tab_id, generation);
        let mut service = DeterministicShellService::default();
        service
            .submit(ExplorerCommand::ResolveAncestry {
                context: ancestry.clone(),
                location: LocationDescriptor::file_system(r"D:\fixture"),
            })
            .expect("submit ancestry");
        service
            .submit(ExplorerCommand::EnumerateChildContainers {
                context: menu.clone(),
                parent: LocationDescriptor::file_system(r"D:\fixture"),
                segment_id: BreadcrumbSegmentId(11),
                menu_generation: 4,
            })
            .expect("submit menu");

        let events = service.close_channel().expect("close fake channel");
        assert_eq!(events.len(), 2);
        assert!(matches!(
            &events[0],
            ExplorerEvent::AncestryFinished { context, outcome: BreadcrumbTerminal::Failed(error) }
                if context.request_id == ancestry.request_id && error.recoverable
        ));
        assert!(matches!(
            &events[1],
            ExplorerEvent::ChildContainersFinished {
                context,
                segment_id: BreadcrumbSegmentId(11),
                menu_generation: 4,
                outcome: BreadcrumbTerminal::Failed(error),
            } if context.request_id == menu.request_id && error.recoverable
        ));
        assert_eq!(service.verify_drained(), Ok(()));
        assert!(
            service
                .close_channel()
                .expect("idempotent close")
                .is_empty()
        );
    }

    #[test]
    fn large_dataset_generator_matches_deterministic_oracle() -> Result<(), FixtureError> {
        let fixture = OwnedTempFixture::new()?;
        let oracle = fixture.generate_large_dataset(128)?;
        assert_eq!(oracle.item_count, 128);
        assert!(oracle.root.join("item-000000.dat").is_file());
        assert!(oracle.root.join("item-000127.dat").is_file());
        assert_eq!(
            std::fs::read_dir(&oracle.root).unwrap().count(),
            oracle.item_count
        );
        Ok(())
    }

    #[test]
    #[ignore = "explicit 100,000-item performance dataset; run on the dedicated soak machine"]
    fn generate_100k_real_dataset_for_soak() -> Result<(), FixtureError> {
        let fixture = OwnedTempFixture::new()?;
        let started = std::time::Instant::now();
        let oracle = fixture.generate_large_dataset(100_000)?;
        assert_eq!(std::fs::read_dir(&oracle.root).unwrap().count(), 100_000);
        eprintln!("generated 100k dataset in {:?}", started.elapsed());
        Ok(())
    }
}
