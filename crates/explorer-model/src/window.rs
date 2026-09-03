//! Pure multi-tab window ownership and ordering rules.

use std::collections::HashSet;

use crate::{ExplorerEvent, HistoryEntry, TabId, TabSearchState, TabState};

/// A window invariant required by active-tab operations was unavailable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowInvariantError {
    MissingActiveTab,
}

impl std::fmt::Display for WindowInvariantError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("the active tab identity is not present in the window")
    }
}

impl std::error::Error for WindowInvariantError {}

/// Result of routing one owned service event to its correlated tab.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowEventOutcome {
    Applied,
    IgnoredStale,
    IgnoredUnrelated,
}

/// A non-empty ordered tab collection with exactly one active identity.
#[derive(Clone, Debug)]
pub struct ExplorerWindowState {
    tabs: Vec<TabState>,
    active_tab_id: TabId,
    recovery_tab: TabState,
    recovery_initial: HistoryEntry,
}

impl ExplorerWindowState {
    /// Creates a window with its required first tab.
    pub fn new(initial: HistoryEntry) -> Self {
        let recovery_initial = initial.clone();
        let tab = TabState::new(initial);
        let active_tab_id = tab.id;
        let recovery_tab = tab.clone();
        Self {
            tabs: vec![tab],
            active_tab_id,
            recovery_tab,
            recovery_initial,
        }
    }

    /// Reconstructs a validated non-empty tab collection from resolved durable state.
    ///
    /// # Errors
    ///
    /// Returns the precise empty, duplicate, or missing-active identity invariant.
    pub fn from_restored_tabs(
        tabs: Vec<TabState>,
        active_tab_id: TabId,
        fallback: HistoryEntry,
    ) -> Result<Self, TabStateInvariantError> {
        let recovery_tab = TabState::new(fallback.clone());
        let window = Self {
            tabs,
            active_tab_id,
            recovery_tab,
            recovery_initial: fallback,
        };
        window.validate()?;
        Ok(window)
    }

    /// Returns tabs in stable presentation order.
    pub fn tabs(&self) -> &[TabState] {
        &self.tabs
    }

    /// Returns the active tab identity.
    pub const fn active_tab_id(&self) -> TabId {
        self.active_tab_id
    }

    /// Returns the active tab, or an isolated recovery tab if an invariant was violated.
    pub fn active_tab(&self) -> &TabState {
        self.try_active_tab().unwrap_or(&self.recovery_tab)
    }

    /// Returns the active tab mutably, or an isolated recovery tab after invariant failure.
    pub fn active_tab_mut(&mut self) -> &mut TabState {
        let active_tab_id = self.active_tab_id;
        self.tabs
            .iter_mut()
            .find(|tab| tab.id == active_tab_id)
            .unwrap_or(&mut self.recovery_tab)
    }

    /// Returns the active tab through an explicit invariant boundary.
    ///
    /// # Errors
    ///
    /// Returns `MissingActiveTab` when the stored identity is absent.
    pub fn try_active_tab(&self) -> Result<&TabState, WindowInvariantError> {
        self.tabs
            .iter()
            .find(|tab| tab.id == self.active_tab_id)
            .ok_or(WindowInvariantError::MissingActiveTab)
    }

    /// Returns the mutable active tab through an explicit invariant boundary.
    ///
    /// # Errors
    ///
    /// Returns `MissingActiveTab` when the stored identity is absent.
    pub fn try_active_tab_mut(&mut self) -> Result<&mut TabState, WindowInvariantError> {
        self.tabs
            .iter_mut()
            .find(|tab| tab.id == self.active_tab_id)
            .ok_or(WindowInvariantError::MissingActiveTab)
    }

    /// Finds one tab for request routing without changing the active identity.
    pub fn tab_mut(&mut self, id: TabId) -> Option<&mut TabState> {
        self.tabs.iter_mut().find(|tab| tab.id == id)
    }

    /// Routes a request-scoped event by tab identity and full request correlation.
    #[allow(
        clippy::too_many_lines,
        reason = "one exhaustive event router keeps every correlated terminal and stale-event transition visible"
    )]
    pub fn apply_event(&mut self, event: ExplorerEvent) -> WindowEventOutcome {
        match event {
            ExplorerEvent::LocationResolved { context, metadata } => {
                let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == context.tab_id) else {
                    return WindowEventOutcome::IgnoredStale;
                };
                if tab.directory.accepts(&context).is_err() {
                    return WindowEventOutcome::IgnoredStale;
                }
                tab.location_can_write = metadata.can_write;
                if !tab.commit_resolved_location(
                    &context,
                    HistoryEntry::new(metadata.descriptor, metadata.display_title),
                ) {
                    return WindowEventOutcome::IgnoredStale;
                }
                WindowEventOutcome::Applied
            }
            ExplorerEvent::DirectoryBatch { context, entries } => {
                let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == context.tab_id) else {
                    return WindowEventOutcome::IgnoredStale;
                };
                if tab.directory.merge_batch(&context, entries).is_ok() {
                    WindowEventOutcome::Applied
                } else {
                    WindowEventOutcome::IgnoredStale
                }
            }
            ExplorerEvent::DirectoryFinished { context } => {
                let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == context.tab_id) else {
                    return WindowEventOutcome::IgnoredStale;
                };
                if tab.directory.finish(&context).is_err() {
                    return WindowEventOutcome::IgnoredStale;
                }
                if let Some(snapshot) = tab.directory.snapshot() {
                    tab.selection.reconcile(snapshot);
                }
                WindowEventOutcome::Applied
            }
            ExplorerEvent::SearchBatch {
                context,
                source,
                entries,
            } => {
                let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == context.tab_id) else {
                    return WindowEventOutcome::IgnoredStale;
                };
                if tab.merge_search_batch(&context, source, entries).is_ok() {
                    WindowEventOutcome::Applied
                } else {
                    WindowEventOutcome::IgnoredStale
                }
            }
            ExplorerEvent::SearchStatus { context, status } => {
                let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == context.tab_id) else {
                    return WindowEventOutcome::IgnoredStale;
                };
                if tab.update_search_status(&context, status).is_ok() {
                    WindowEventOutcome::Applied
                } else {
                    WindowEventOutcome::IgnoredStale
                }
            }
            ExplorerEvent::SearchFinished { context, outcome } => {
                let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == context.tab_id) else {
                    return WindowEventOutcome::IgnoredStale;
                };
                if tab.finish_search(&context, outcome).is_err() {
                    return WindowEventOutcome::IgnoredStale;
                }
                if let Some(snapshot) = tab.search_results().cloned() {
                    tab.selection.reconcile(&snapshot);
                }
                WindowEventOutcome::Applied
            }
            ExplorerEvent::Failed { context, error } => {
                let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == context.tab_id) else {
                    return WindowEventOutcome::IgnoredStale;
                };
                let navigation_error = error.user_message.clone();
                let search_failure = matches!(tab.search, TabSearchState::Loading { .. });
                let applied = if search_failure {
                    tab.finish_search(&context, crate::SearchTerminal::Failed(error))
                } else {
                    tab.reject_history_navigation(&context);
                    tab.directory.fail(&context, error)
                };
                if applied.is_ok() {
                    if !search_failure {
                        tab.view.address.navigation_failed(navigation_error);
                    }
                    WindowEventOutcome::Applied
                } else {
                    WindowEventOutcome::IgnoredStale
                }
            }
            ExplorerEvent::DirectoryChanged { .. }
            | ExplorerEvent::AncestryBatch { .. }
            | ExplorerEvent::AncestryFinished { .. }
            | ExplorerEvent::ChildContainersBatch { .. }
            | ExplorerEvent::ChildContainersFinished { .. }
            | ExplorerEvent::ClipboardChanged { .. }
            | ExplorerEvent::OperationProgress { .. }
            | ExplorerEvent::OperationFinished { .. }
            | ExplorerEvent::ContextMenuFinished { .. }
            | ExplorerEvent::ApkInstallStatus { .. }
            | ExplorerEvent::ShellIconLoaded { .. }
            | ExplorerEvent::ShellIconFailed { .. }
            | ExplorerEvent::ThumbnailFinished { .. }
            | ExplorerEvent::ThumbnailCacheCleared { .. }
            | ExplorerEvent::PreviewHostFinished { .. }
            | ExplorerEvent::LockOwnersDiscovered { .. }
            | ExplorerEvent::LockOwnersClosed { .. } => WindowEventOutcome::IgnoredUnrelated,
        }
    }

    /// Derives every active-tab surface value from one tab identity.
    pub fn active_presentation(&self) -> TabPresentationSnapshot {
        let tab = self.active_tab();
        let current = tab.history.current();
        TabPresentationSnapshot {
            tab_id: tab.id,
            address_title: matches!(
                tab.view.address.mode,
                crate::AddressBarMode::Editing | crate::AddressBarMode::NavigationError
            )
            .then(|| tab.view.address.draft.clone())
            .unwrap_or_else(|| {
                current
                    .map(|entry| entry.display_title.clone())
                    .unwrap_or_default()
            }),
            item_count: tab
                .visible_snapshot()
                .map_or(0, |snapshot| snapshot.entries().len()),
            selected_count: tab.selection.len(),
            search: tab.search.clone(),
            can_go_back: tab.history.can_go_back(),
            can_go_forward: tab.history.can_go_forward(),
            can_go_up: current
                .and_then(|entry| entry.location.path())
                .and_then(std::path::Path::parent)
                .is_some(),
            can_write: tab.location_can_write,
        }
    }

    /// Creates and activates a tab with an independent clone of the active committed history.
    pub fn new_tab(&mut self) -> TabId {
        let (history, settings) = self
            .try_active_tab()
            .ok()
            .and_then(|tab| {
                tab.history
                    .current()
                    .cloned()
                    .map(|_| (tab.history.clone(), tab.view.settings.clone()))
            })
            .unwrap_or_else(|| {
                (
                    crate::NavigationHistory::with_initial(self.recovery_initial.clone()),
                    self.recovery_tab.view.settings.clone(),
                )
            });
        let initial = history
            .current()
            .cloned()
            .unwrap_or_else(|| self.recovery_initial.clone());
        let mut tab = TabState::new(initial);
        tab.history = history;
        tab.view.settings = settings;
        let id = tab.id;
        self.tabs.push(tab);
        self.active_tab_id = id;
        debug_assert!(self.validate().is_ok());
        id
    }

    /// Activates an existing tab without cancelling work in background tabs.
    pub fn activate(&mut self, id: TabId) -> bool {
        if self.tabs.iter().any(|tab| tab.id == id) {
            self.active_tab_id = id;
            true
        } else {
            false
        }
    }

    /// Closes one tab or requests window close when it is the last tab.
    pub fn close(&mut self, id: TabId) -> TabCloseOutcome {
        let Some(index) = self.tabs.iter().position(|tab| tab.id == id) else {
            return TabCloseOutcome::NotFound;
        };
        if self.tabs.len() == 1 {
            return TabCloseOutcome::CloseWindow;
        }
        let mut removed = self.tabs.remove(index);
        removed.requests.cancel_all();
        removed.search = TabSearchState::Idle;
        if self.active_tab_id == id {
            let replacement_index = index.min(self.tabs.len() - 1);
            self.active_tab_id = self.tabs[replacement_index].id;
        }
        debug_assert!(self.validate().is_ok());
        TabCloseOutcome::Closed
    }

    /// Moves a tab to a new presentation index without changing its identity or state.
    pub fn reorder(&mut self, id: TabId, destination_index: usize) -> bool {
        let Some(source_index) = self.tabs.iter().position(|tab| tab.id == id) else {
            return false;
        };
        if destination_index >= self.tabs.len() {
            return false;
        }
        let tab = self.tabs.remove(source_index);
        self.tabs.insert(destination_index, tab);
        debug_assert!(self.validate().is_ok());
        true
    }

    /// Checks the non-empty, unique-ID, exactly-one-active tab invariant.
    ///
    /// # Errors
    ///
    /// Returns the precise structural invariant that is broken.
    pub fn validate(&self) -> Result<(), TabStateInvariantError> {
        if self.tabs.is_empty() {
            return Err(TabStateInvariantError::Empty);
        }
        let mut ids = HashSet::with_capacity(self.tabs.len());
        for tab in &self.tabs {
            if !ids.insert(tab.id) {
                return Err(TabStateInvariantError::DuplicateId(tab.id));
            }
        }
        let active_count = self
            .tabs
            .iter()
            .filter(|tab| tab.id == self.active_tab_id)
            .count();
        if active_count == 1 {
            Ok(())
        } else {
            Err(TabStateInvariantError::MissingActive(self.active_tab_id))
        }
    }
}

/// Result of applying the documented last-tab close rule.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TabCloseOutcome {
    Closed,
    CloseWindow,
    NotFound,
}

/// Values consumed by file view, status, address/search, and navigation availability.
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "this immutable projection exposes independent navigation and write capabilities"
)]
pub struct TabPresentationSnapshot {
    pub tab_id: TabId,
    pub address_title: String,
    pub item_count: usize,
    pub selected_count: usize,
    pub search: TabSearchState,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub can_go_up: bool,
    pub can_write: bool,
}

/// Invalid multi-tab structure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TabStateInvariantError {
    Empty,
    DuplicateId(TabId),
    MissingActive(TabId),
}

#[cfg(test)]
mod tests {
    use crate::{
        ExplorerEvent, FileEntry, Generation, LocationDescriptor, LocationMetadata, ShellItemId,
        TabSearchState,
    };

    use super::*;

    fn initial(name: &str) -> HistoryEntry {
        HistoryEntry::new(
            LocationDescriptor::file_system(format!(r"C:\fixture\{name}")),
            name,
        )
    }

    fn row(id: u8, name: &str) -> FileEntry {
        FileEntry {
            id: ShellItemId::from_provider_bytes([id]).expect("identity"),
            display_name: name.to_owned(),
            location: LocationDescriptor::file_system(format!(r"C:\fixture\{name}")),
            is_container: false,
            metadata: crate::FileEntryMetadata::default(),
        }
    }

    #[test]
    fn a_b_c_out_of_order_events_only_mutate_current_generation() {
        let mut window = ExplorerWindowState::new(initial("initial"));
        let a = window
            .active_tab_mut()
            .begin_navigation_request()
            .expect("request A");
        let b = window
            .active_tab_mut()
            .begin_navigation_request()
            .expect("request B");
        let c = window
            .active_tab_mut()
            .begin_navigation_request()
            .expect("request C");

        assert!(a.cancellation.is_cancelled());
        assert!(b.cancellation.is_cancelled());
        assert_eq!(
            window.apply_event(ExplorerEvent::DirectoryBatch {
                context: a,
                entries: vec![row(1, "late-a.txt")],
            }),
            WindowEventOutcome::IgnoredStale
        );
        assert_eq!(
            window.apply_event(ExplorerEvent::DirectoryFinished { context: b }),
            WindowEventOutcome::IgnoredStale
        );
        assert_eq!(
            window.apply_event(ExplorerEvent::LocationResolved {
                context: c.clone(),
                metadata: LocationMetadata {
                    descriptor: LocationDescriptor::file_system(r"C:\fixture\C"),
                    display_title: "C".to_owned(),
                    can_go_up: true,
                    can_write: true,
                },
            }),
            WindowEventOutcome::Applied
        );
        assert_eq!(
            window.apply_event(ExplorerEvent::DirectoryBatch {
                context: c.clone(),
                entries: vec![row(3, "current-c.txt")],
            }),
            WindowEventOutcome::Applied
        );
        assert_eq!(
            window.apply_event(ExplorerEvent::DirectoryFinished { context: c }),
            WindowEventOutcome::Applied
        );
        assert_eq!(window.active_presentation().address_title, "C");
        assert_eq!(window.active_presentation().item_count, 1);
        assert_eq!(
            window
                .active_tab()
                .directory
                .snapshot()
                .expect("snapshot")
                .entries()[0]
                .display_name,
            "current-c.txt"
        );
    }

    #[test]
    fn new_tabs_have_unique_independent_state_and_become_active() {
        let mut window = ExplorerWindowState::new(initial("first"));
        window
            .active_tab_mut()
            .history
            .commit_navigation(initial("second"));
        window
            .active_tab_mut()
            .history
            .commit_navigation(initial("third"));
        let _ = window.active_tab_mut().history.go_back();
        window.active_tab_mut().generation = Generation::new(7);
        window.active_tab_mut().search = TabSearchState::Editing("first query".to_owned());
        window.active_tab_mut().view.settings.mode = crate::ViewMode::LargeIcons;
        window.active_tab_mut().view.settings.hidden_items = true;
        let first = window.active_tab_id();
        let source_history = window.active_tab().history.clone();
        let second = window.new_tab();

        assert_ne!(first, second);
        assert_eq!(window.active_tab_id(), second);
        assert_eq!(window.tabs().len(), 2);
        assert_eq!(window.active_tab().generation, Generation::default());
        assert_eq!(window.active_tab().search, TabSearchState::Idle);
        assert_eq!(
            window.active_tab().view.settings.mode,
            crate::ViewMode::LargeIcons
        );
        assert!(window.active_tab().view.settings.hidden_items);
        assert_eq!(
            window.active_tab().history,
            source_history,
            "new tab inherits current, Back, and Forward stacks"
        );
        window
            .active_tab_mut()
            .history
            .commit_navigation(initial("independent"));
        assert_eq!(window.tabs()[0].history, source_history);
        assert_ne!(window.active_tab().history, source_history);
        assert_eq!(window.validate(), Ok(()));
    }

    #[test]
    fn missing_active_identity_uses_isolated_recovery_state_without_panicking() {
        let mut window = ExplorerWindowState::new(initial("first"));
        let original = window.active_tab_id();
        window.active_tab_id = TabId::new();

        assert!(matches!(
            window.try_active_tab(),
            Err(WindowInvariantError::MissingActiveTab)
        ));
        window.active_tab_mut().view.settings.hidden_items = true;
        assert!(window.active_tab().view.settings.hidden_items);
        assert_eq!(window.tabs()[0].id, original);
        assert!(!window.tabs()[0].view.settings.hidden_items);

        let recovered = window.new_tab();
        assert_eq!(window.active_tab_id(), recovered);
        assert!(window.try_active_tab().is_ok());
    }

    #[test]
    fn switching_reads_only_target_tab_state() {
        let mut window = ExplorerWindowState::new(initial("first"));
        let first = window.active_tab_id();
        let first_request = window
            .active_tab_mut()
            .begin_navigation_request()
            .expect("first request");
        let first_identity = ShellItemId::from_provider_bytes([1]).expect("first identity");
        window
            .active_tab_mut()
            .directory
            .merge_batch(
                &first_request,
                [FileEntry {
                    id: first_identity.clone(),
                    display_name: "first.txt".to_owned(),
                    location: LocationDescriptor::file_system(r"C:\fixture\first\first.txt"),
                    is_container: false,
                    metadata: crate::FileEntryMetadata::default(),
                }],
            )
            .expect("first batch");
        window
            .active_tab_mut()
            .directory
            .finish(&first_request)
            .expect("first finish");
        window
            .active_tab_mut()
            .selection
            .select_only(first_identity);
        window.active_tab_mut().search = TabSearchState::Editing("first".to_owned());
        let second = window.new_tab();
        window
            .active_tab_mut()
            .history
            .commit_navigation(initial("second"));
        window.active_tab_mut().search = TabSearchState::Editing("second".to_owned());
        assert!(window.activate(first));
        let first_presentation = window.active_presentation();
        assert_eq!(first_presentation.tab_id, first);
        assert_eq!(first_presentation.item_count, 1);
        assert_eq!(first_presentation.selected_count, 1);
        assert_eq!(first_presentation.address_title, "first");
        assert!(matches!(
            first_presentation.search,
            TabSearchState::Editing(input) if input == "first"
        ));
        assert!(window.activate(second));
        let second_presentation = window.active_presentation();
        assert_eq!(second_presentation.tab_id, second);
        assert_eq!(second_presentation.item_count, 0);
        assert_eq!(second_presentation.selected_count, 0);
        assert_eq!(second_presentation.address_title, "second");
        assert!(matches!(
            second_presentation.search,
            TabSearchState::Editing(input) if input == "second"
        ));
    }

    #[test]
    fn closing_background_active_and_last_tab_follows_product_rule() {
        let mut window = ExplorerWindowState::new(initial("first"));
        let first = window.active_tab_id();
        let second = window.new_tab();
        let third = window.new_tab();
        assert_eq!(window.close(second), TabCloseOutcome::Closed);
        assert_eq!(window.active_tab_id(), third);
        assert_eq!(window.close(third), TabCloseOutcome::Closed);
        assert_eq!(window.active_tab_id(), first);
        assert_eq!(window.close(first), TabCloseOutcome::CloseWindow);
        assert_eq!(window.tabs().len(), 1);
        assert_eq!(window.validate(), Ok(()));
    }

    #[test]
    fn closing_tab_cancels_navigation_search_and_operation_view_scopes() {
        let mut window = ExplorerWindowState::new(initial("first"));
        let closing = window.new_tab();
        let navigation = window
            .active_tab_mut()
            .begin_navigation_request()
            .expect("navigation")
            .cancellation;
        let search = crate::CancellationToken::new();
        let operation_view = crate::CancellationToken::new();
        window
            .active_tab_mut()
            .requests
            .replace_search(search.clone());
        window
            .active_tab_mut()
            .requests
            .replace_operation_view(operation_view.clone());
        assert_eq!(window.close(closing), TabCloseOutcome::Closed);
        assert!(navigation.is_cancelled());
        assert!(search.is_cancelled());
        assert!(operation_view.is_cancelled());
        assert!(!window.tabs().iter().any(|tab| tab.id == closing));
    }

    #[test]
    fn reorder_preserves_active_identity_and_per_tab_state() {
        let mut window = ExplorerWindowState::new(initial("first"));
        let first = window.active_tab_id();
        let second = window.new_tab();
        let identity = ShellItemId::from_provider_bytes([9]).expect("identity");
        window
            .active_tab_mut()
            .selection
            .select_only(identity.clone());
        assert!(window.reorder(second, 0));
        assert_eq!(window.active_tab_id(), second);
        assert_eq!(window.tabs()[0].id, second);
        assert!(window.tabs()[0].selection.contains(&identity));
        assert_eq!(window.tabs()[1].id, first);
        assert!(!window.tabs()[1].selection.contains(&identity));
    }

    #[test]
    fn active_address_edit_is_not_overwritten_by_background_tab_state() {
        let mut window = ExplorerWindowState::new(initial("first"));
        let first = window.active_tab_id();
        window.active_tab_mut().view.address.enter_editing();
        assert!(
            window
                .active_tab_mut()
                .view
                .address
                .update_draft("typing-not-submitted".to_owned())
        );
        let second = window.new_tab();
        window
            .active_tab_mut()
            .history
            .commit_navigation(initial("second"));
        assert_eq!(window.active_presentation().address_title, "second");

        let first_tab = window
            .tabs
            .iter_mut()
            .find(|tab| tab.id == first)
            .expect("first tab");
        first_tab
            .history
            .commit_navigation(initial("background-update"));
        assert_eq!(window.active_tab_id(), second);
        assert_eq!(window.active_presentation().address_title, "second");
        assert!(window.activate(first));
        assert_eq!(
            window.active_presentation().address_title,
            "typing-not-submitted"
        );
    }
}
