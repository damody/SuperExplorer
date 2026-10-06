//! Bounded close ordering shared by local tabs and saved windows from other processes.

use std::collections::{HashSet, VecDeque};

#[derive(Clone, Debug)]
pub(crate) struct ClosedTab {
    pub index: usize,
    pub history: explorer_model::NavigationHistory,
    pub settings: explorer_model::ViewSettings,
}

#[derive(Clone, Debug)]
pub(crate) enum ClosedItem {
    Tab(ClosedTab),
    Window(u64),
}

#[derive(Clone, Debug, Default)]
pub(crate) struct RecentlyClosed {
    items: VecDeque<ClosedItem>,
    known_windows: HashSet<u64>,
}

impl RecentlyClosed {
    pub fn sync_windows(&mut self, newest_first: impl IntoIterator<Item = u64>) {
        let ids = newest_first.into_iter().collect::<Vec<_>>();
        let available = ids.iter().copied().collect::<HashSet<_>>();
        self.items.retain(|item| match item {
            ClosedItem::Tab(_) => true,
            ClosedItem::Window(id) => available.contains(id),
        });
        for id in ids.into_iter().rev() {
            if !self.known_windows.contains(&id) {
                self.items.push_back(ClosedItem::Window(id));
            }
        }
        // Keep successful restores known until the new process acquires its
        // presence lease, preventing a fast second shortcut from reopening it.
        self.known_windows = available;
        self.bound();
    }

    pub fn push_tab(&mut self, tab: ClosedTab) {
        self.items.push_back(ClosedItem::Tab(tab));
        self.bound();
    }

    fn bound(&mut self) {
        while self
            .items
            .iter()
            .filter(|item| matches!(item, ClosedItem::Tab(_)))
            .count()
            > 20
        {
            if let Some(index) = self
                .items
                .iter()
                .position(|item| matches!(item, ClosedItem::Tab(_)))
            {
                self.items.remove(index);
            }
        }
        while self.items.len() > 40 {
            self.items.pop_front();
        }
    }

    pub fn peek(&self) -> Option<&ClosedItem> {
        self.items.back()
    }

    pub fn pop_tab(&mut self) -> Option<ClosedTab> {
        if matches!(self.peek(), Some(ClosedItem::Tab(_))) {
            if let Some(ClosedItem::Tab(tab)) = self.items.pop_back() {
                return Some(tab);
            }
        }
        None
    }

    pub fn remove_window(&mut self, id: u64) {
        self.items
            .retain(|item| !matches!(item, ClosedItem::Window(candidate) if *candidate == id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab() -> ClosedTab {
        let tab = explorer_model::TabState::new(explorer_model::HistoryEntry::new(
            explorer_model::LocationDescriptor::file_system(r"C:\"),
            "C:",
        ));
        ClosedTab {
            index: 0,
            history: tab.history,
            settings: tab.view.settings,
        }
    }

    #[test]
    fn windows_and_tabs_restore_in_close_order_without_spawn_duplicates() {
        let mut history = RecentlyClosed::default();
        history.sync_windows([1]);
        history.push_tab(tab());
        history.sync_windows([1]);
        assert!(matches!(history.peek(), Some(ClosedItem::Tab(_))));
        history.sync_windows([2, 1]);
        assert!(matches!(history.peek(), Some(ClosedItem::Window(2))));
        history.remove_window(2);
        history.sync_windows([2, 1]);
        assert!(matches!(history.peek(), Some(ClosedItem::Tab(_))));
        assert!(history.pop_tab().is_some());
        assert!(matches!(history.peek(), Some(ClosedItem::Window(1))));
        history.sync_windows([1]);
        history.sync_windows([2, 1]);
        assert!(matches!(history.peek(), Some(ClosedItem::Window(2))));
    }

    #[test]
    fn closed_tab_retention_is_bounded() {
        let mut history = RecentlyClosed::default();
        for _ in 0..100 {
            history.push_tab(tab());
        }
        assert_eq!(history.items.len(), 20);
    }
}
