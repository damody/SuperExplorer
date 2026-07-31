//! Directory-local automation lifecycle composed without global script discovery.

use std::path::Path;

use anyhow::Error;
use explorer_automation::{FolderScriptHandle, FolderScriptSnapshot};
use explorer_model::TabId;

/// Owns the shared exact-directory automation coordinator.
#[derive(Clone, Debug, Default)]
pub struct AutomationComposition {
    handle: FolderScriptHandle,
}

impl AutomationComposition {
    /// Starts inertly. Scripts are discovered only after a tab resolves a filesystem directory.
    ///
    /// # Errors
    ///
    /// Reserved for future host-adapter initialization failures.
    pub fn start() -> Result<Self, Error> {
        Ok(Self::default())
    }

    #[must_use]
    pub fn handle(&self) -> FolderScriptHandle {
        self.handle.clone()
    }

    /// Associates a tab with its newly resolved exact directory.
    ///
    /// # Errors
    ///
    /// Returns an automation coordination error if the shared service is unavailable.
    pub fn enter_directory(
        &self,
        tab_id: TabId,
        directory: Option<&Path>,
    ) -> Result<Option<FolderScriptSnapshot>, Error> {
        self.handle
            .enter_directory(tab_id, directory)
            .map_err(Error::msg)
    }

    /// Returns directory-local automation state for diagnostics and UI presentation.
    ///
    /// # Errors
    ///
    /// Returns an automation coordination error if the shared service is unavailable.
    pub fn snapshots(&self) -> Result<Vec<FolderScriptSnapshot>, Error> {
        self.handle.snapshots().map_err(Error::msg)
    }

    /// Cancels all directory-owned resources before process services stop.
    pub fn shutdown(&mut self) {
        if let Err(error) = self.handle.shutdown() {
            tracing::warn!(%error, "folder automation shutdown failed");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use explorer_automation::{FOLDER_SCRIPT_FILENAME, FolderScriptState};
    use explorer_model::TabId;
    use tempfile::tempdir;

    use super::AutomationComposition;

    #[test]
    fn starts_inert_and_loads_only_the_entered_directory_script() {
        let root = tempdir().expect("tempdir");
        let child = root.path().join("child");
        fs::create_dir(&child).expect("child");
        fs::write(
            root.path().join(FOLDER_SCRIPT_FILENAME),
            "script.configure { activation = 'temporary' }",
        )
        .expect("script");
        let mut service = AutomationComposition::start().expect("composition");
        assert!(service.snapshots().expect("snapshots").is_empty());

        let child_state = service
            .enter_directory(TabId::new(), Some(&child))
            .expect("child transition")
            .expect("child snapshot");
        assert_eq!(child_state.state, FolderScriptState::Missing);

        let root_state = service
            .enter_directory(TabId::new(), Some(root.path()))
            .expect("root transition")
            .expect("root snapshot");
        assert_eq!(root_state.state, FolderScriptState::Active);
        service.shutdown();
        assert!(service.snapshots().expect("snapshots").is_empty());
    }
}
