//! Exact-directory `super_explorer.lua` ownership and reload coordination.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    hash::{DefaultHasher, Hash as _, Hasher as _},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::SystemTime,
};

use explorer_model::TabId;

use crate::{AutomationError, AutomationErrorKind, AutomationResult, ScriptRegistry};

/// The only filename recognized for directory-local automation.
pub const FOLDER_SCRIPT_FILENAME: &str = "super_explorer.lua";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FolderScriptState {
    Missing,
    Active,
    ReloadError,
}

/// User-facing state for one directory currently owned by at least one tab.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FolderScriptSnapshot {
    pub directory: PathBuf,
    pub script_path: PathBuf,
    pub owner_count: usize,
    pub state: FolderScriptState,
    pub diagnostic: Option<AutomationError>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ScriptFingerprint {
    modified: Option<SystemTime>,
    size: u64,
    content_hash: u64,
}

#[derive(Debug)]
struct DirectoryEntry {
    owners: HashSet<TabId>,
    fingerprint: Option<ScriptFingerprint>,
    state: FolderScriptState,
    diagnostic: Option<AutomationError>,
}

/// Owns exact-directory script VMs and their tab reference counts.
#[derive(Default)]
pub struct FolderScriptCoordinator {
    registry: ScriptRegistry,
    directories: BTreeMap<PathBuf, DirectoryEntry>,
    tab_directories: HashMap<TabId, PathBuf>,
}

impl FolderScriptCoordinator {
    /// Associates a tab with an exact filesystem directory.
    ///
    /// A missing or invalid script is represented in the returned snapshot and never blocks
    /// navigation. Passing `None` detaches the tab for a non-filesystem Shell location.
    pub fn enter_directory(
        &mut self,
        tab_id: TabId,
        directory: Option<&Path>,
    ) -> Option<FolderScriptSnapshot> {
        let destination = directory.map(canonical_directory);
        if self.tab_directories.get(&tab_id) == destination.as_ref() {
            if let Some(directory) = destination.as_deref() {
                self.refresh_directory(directory);
                return self.snapshot(directory);
            }
            return None;
        }

        if let Some(directory) = destination.as_ref() {
            self.directories
                .entry(directory.clone())
                .or_insert_with(|| DirectoryEntry {
                    owners: HashSet::new(),
                    fingerprint: None,
                    state: FolderScriptState::Missing,
                    diagnostic: None,
                })
                .owners
                .insert(tab_id);
            self.refresh_directory(directory);
        }

        let previous = match destination.clone() {
            Some(directory) => self.tab_directories.insert(tab_id, directory),
            None => self.tab_directories.remove(&tab_id),
        };
        if let Some(previous) = previous {
            self.release_owner(tab_id, &previous);
        }
        destination.and_then(|directory| self.snapshot(&directory))
    }

    /// Releases a closed tab and unloads the VM after the final owner leaves.
    pub fn close_tab(&mut self, tab_id: TabId) {
        if let Some(directory) = self.tab_directories.remove(&tab_id) {
            self.release_owner(tab_id, &directory);
        }
    }

    /// Polls active directory script metadata and performs atomic reload transitions.
    pub fn refresh_changed(&mut self) {
        let directories = self.directories.keys().cloned().collect::<Vec<_>>();
        for directory in directories {
            self.refresh_directory(&directory);
        }
    }

    /// Returns active directory entries in stable path order.
    #[must_use]
    pub fn snapshots(&self) -> Vec<FolderScriptSnapshot> {
        self.directories
            .keys()
            .filter_map(|directory| self.snapshot(directory))
            .collect()
    }

    /// Stops all directory scripts and clears every tab association.
    pub fn shutdown(&mut self) {
        self.registry.shutdown();
        self.directories.clear();
        self.tab_directories.clear();
    }

    #[must_use]
    pub fn registry(&self) -> &ScriptRegistry {
        &self.registry
    }

    fn refresh_directory(&mut self, directory: &Path) {
        let script_path = directory.join(FOLDER_SCRIPT_FILENAME);
        let current = script_fingerprint(&script_path);
        let previous = self
            .directories
            .get(directory)
            .and_then(|entry| entry.fingerprint);
        if current == previous {
            return;
        }

        let (state, diagnostic) = if current.is_none() {
            self.registry.remove(&script_path);
            (FolderScriptState::Missing, None)
        } else {
            let result = if self.registry.active_vm(&script_path).is_some() {
                self.registry.reload(&script_path)
            } else {
                self.registry.enable(&script_path)
            };
            match result {
                Ok(()) => (FolderScriptState::Active, None),
                Err(error) => (FolderScriptState::ReloadError, Some(error)),
            }
        };
        if let Some(entry) = self.directories.get_mut(directory) {
            entry.fingerprint = current;
            entry.state = state;
            entry.diagnostic = diagnostic;
        }
    }

    fn release_owner(&mut self, tab_id: TabId, directory: &Path) {
        let remove = self.directories.get_mut(directory).is_some_and(|entry| {
            entry.owners.remove(&tab_id);
            entry.owners.is_empty()
        });
        if remove {
            self.registry
                .remove(&directory.join(FOLDER_SCRIPT_FILENAME));
            self.directories.remove(directory);
        }
    }

    fn snapshot(&self, directory: &Path) -> Option<FolderScriptSnapshot> {
        self.directories
            .get(directory)
            .map(|entry| FolderScriptSnapshot {
                directory: directory.to_path_buf(),
                script_path: directory.join(FOLDER_SCRIPT_FILENAME),
                owner_count: entry.owners.len(),
                state: entry.state,
                diagnostic: entry.diagnostic.clone(),
            })
    }
}

impl std::fmt::Debug for FolderScriptCoordinator {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FolderScriptCoordinator")
            .field("directory_count", &self.directories.len())
            .field("tab_count", &self.tab_directories.len())
            .finish_non_exhaustive()
    }
}

/// Cloneable application/UI boundary around the coordinator.
#[derive(Clone, Default)]
pub struct FolderScriptHandle {
    inner: Arc<Mutex<FolderScriptCoordinator>>,
}

impl FolderScriptHandle {
    #[must_use]
    pub fn new(coordinator: FolderScriptCoordinator) -> Self {
        Self {
            inner: Arc::new(Mutex::new(coordinator)),
        }
    }

    /// Associates one tab with an exact filesystem directory.
    ///
    /// # Errors
    ///
    /// Returns an internal availability error if the shared coordinator lock is poisoned.
    pub fn enter_directory(
        &self,
        tab_id: TabId,
        directory: Option<&Path>,
    ) -> AutomationResult<Option<FolderScriptSnapshot>> {
        self.with_coordinator(|coordinator| coordinator.enter_directory(tab_id, directory))
    }

    /// Releases automation ownership for a closed tab.
    ///
    /// # Errors
    ///
    /// Returns an internal availability error if the shared coordinator lock is poisoned.
    pub fn close_tab(&self, tab_id: TabId) -> AutomationResult<()> {
        self.with_coordinator(|coordinator| coordinator.close_tab(tab_id))
    }

    /// Refreshes every directory-local script whose file content changed.
    ///
    /// # Errors
    ///
    /// Returns an internal availability error if the shared coordinator lock is poisoned.
    pub fn refresh_changed(&self) -> AutomationResult<()> {
        self.with_coordinator(FolderScriptCoordinator::refresh_changed)
    }

    /// Returns stable snapshots for directories currently owned by tabs.
    ///
    /// # Errors
    ///
    /// Returns an internal availability error if the shared coordinator lock is poisoned.
    pub fn snapshots(&self) -> AutomationResult<Vec<FolderScriptSnapshot>> {
        self.with_coordinator(|coordinator| coordinator.snapshots())
    }

    /// Stops all scripts and clears every tab association.
    ///
    /// # Errors
    ///
    /// Returns an internal availability error if the shared coordinator lock is poisoned.
    pub fn shutdown(&self) -> AutomationResult<()> {
        self.with_coordinator(FolderScriptCoordinator::shutdown)
    }

    fn with_coordinator<T>(
        &self,
        operation: impl FnOnce(&mut FolderScriptCoordinator) -> T,
    ) -> AutomationResult<T> {
        self.inner
            .lock()
            .map(|mut coordinator| operation(&mut coordinator))
            .map_err(|_| {
                AutomationError::new(
                    AutomationErrorKind::Internal,
                    "folder_script.lock",
                    true,
                    "The folder automation service is temporarily unavailable",
                )
            })
    }
}

impl std::fmt::Debug for FolderScriptHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("FolderScriptHandle")
    }
}

fn canonical_directory(directory: &Path) -> PathBuf {
    fs::canonicalize(directory).unwrap_or_else(|_| directory.to_path_buf())
}

fn script_fingerprint(path: &Path) -> Option<ScriptFingerprint> {
    let metadata = fs::metadata(path).ok()?;
    if !metadata.is_file() {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    Some(ScriptFingerprint {
        modified: metadata.modified().ok(),
        size: metadata.len(),
        content_hash: hasher.finish(),
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use explorer_model::TabId;
    use tempfile::tempdir;

    use super::{FOLDER_SCRIPT_FILENAME, FolderScriptCoordinator, FolderScriptState};

    const SCRIPT: &str = "script.configure { activation = 'temporary' }";

    #[test]
    fn parent_script_is_not_inherited_by_child_directory() {
        let root = tempdir().expect("tempdir");
        let child = root.path().join("child");
        fs::create_dir(&child).expect("child");
        fs::write(root.path().join(FOLDER_SCRIPT_FILENAME), SCRIPT).expect("script");
        let mut coordinator = FolderScriptCoordinator::default();

        let snapshot = coordinator
            .enter_directory(TabId::new(), Some(&child))
            .expect("snapshot");

        assert_eq!(snapshot.state, FolderScriptState::Missing);
        assert!(
            coordinator
                .registry()
                .active_vm(&snapshot.script_path)
                .is_none()
        );
    }

    #[test]
    fn tabs_share_one_vm_and_the_last_departure_unloads_it() {
        let root = tempdir().expect("tempdir");
        let script = root.path().join(FOLDER_SCRIPT_FILENAME);
        fs::write(&script, SCRIPT).expect("script");
        let first = TabId::new();
        let second = TabId::new();
        let mut coordinator = FolderScriptCoordinator::default();

        let initial = coordinator
            .enter_directory(first, Some(root.path()))
            .expect("initial");
        let shared = coordinator
            .enter_directory(second, Some(root.path()))
            .expect("shared");
        assert_eq!(shared.owner_count, 2);
        assert!(
            coordinator
                .registry()
                .active_vm(&initial.script_path)
                .is_some()
        );

        coordinator.close_tab(first);
        assert_eq!(coordinator.snapshots()[0].owner_count, 1);
        assert!(
            coordinator
                .registry()
                .active_vm(&initial.script_path)
                .is_some()
        );
        coordinator.close_tab(second);
        assert!(coordinator.snapshots().is_empty());
        assert!(
            coordinator
                .registry()
                .active_vm(&initial.script_path)
                .is_none()
        );
    }

    #[test]
    fn invalid_reload_keeps_working_vm_and_later_valid_edit_recovers() {
        let root = tempdir().expect("tempdir");
        let script = root.path().join(FOLDER_SCRIPT_FILENAME);
        fs::write(&script, SCRIPT).expect("script");
        let mut coordinator = FolderScriptCoordinator::default();
        let initial = coordinator
            .enter_directory(TabId::new(), Some(root.path()))
            .expect("initial");

        fs::write(&script, "function broken(").expect("invalid");
        coordinator.refresh_changed();
        assert_eq!(
            coordinator.snapshots()[0].state,
            FolderScriptState::ReloadError
        );
        assert!(
            coordinator
                .registry()
                .active_vm(&initial.script_path)
                .is_some()
        );

        fs::write(&script, "script.configure { name = 'recovered' }").expect("valid");
        coordinator.refresh_changed();
        assert_eq!(coordinator.snapshots()[0].state, FolderScriptState::Active);
    }

    #[test]
    fn remove_and_recreate_tracks_tabs_that_remain_in_directory() {
        let root = tempdir().expect("tempdir");
        let script = root.path().join(FOLDER_SCRIPT_FILENAME);
        fs::write(&script, SCRIPT).expect("script");
        let mut coordinator = FolderScriptCoordinator::default();
        coordinator.enter_directory(TabId::new(), Some(root.path()));

        fs::remove_file(&script).expect("remove");
        coordinator.refresh_changed();
        assert_eq!(coordinator.snapshots()[0].state, FolderScriptState::Missing);

        fs::write(&script, SCRIPT).expect("recreate");
        coordinator.refresh_changed();
        assert_eq!(coordinator.snapshots()[0].state, FolderScriptState::Active);
    }
}
