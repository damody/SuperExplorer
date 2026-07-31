//! Deterministic focus traversal independent of GPUI rendering.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FocusSurface {
    WindowChrome,
    TabStrip,
    CommandBar,
    AddressBar,
    Search,
    NavigationPane,
    FileView,
    PreviewPane,
    StatusBar,
}

impl FocusSurface {
    pub const ORDER: [Self; 9] = [
        Self::WindowChrome,
        Self::TabStrip,
        Self::CommandBar,
        Self::AddressBar,
        Self::Search,
        Self::NavigationPane,
        Self::FileView,
        Self::PreviewPane,
        Self::StatusBar,
    ];
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FocusDirection {
    Forward,
    Backward,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FocusCoordinator {
    current: FocusSurface,
    previous: Option<FocusSurface>,
}

impl Default for FocusCoordinator {
    fn default() -> Self {
        Self {
            current: FocusSurface::FileView,
            previous: None,
        }
    }
}

impl FocusCoordinator {
    pub const fn current(self) -> FocusSurface {
        self.current
    }

    pub const fn previous(self) -> Option<FocusSurface> {
        self.previous
    }

    pub fn focus(&mut self, surface: FocusSurface) {
        if self.current != surface {
            self.previous = Some(self.current);
            self.current = surface;
        }
    }

    /// Restores a tab-scoped focus target without carrying another tab's previous-focus stack.
    pub fn restore_context(&mut self, surface: FocusSurface) {
        self.current = surface;
        self.previous = None;
    }

    pub fn restore_previous(&mut self) -> bool {
        let Some(previous) = self.previous.take() else {
            return false;
        };
        self.current = previous;
        true
    }

    pub fn traverse(
        &mut self,
        direction: FocusDirection,
        is_enabled: impl Fn(FocusSurface) -> bool,
    ) -> bool {
        let Some(current_index) = FocusSurface::ORDER
            .iter()
            .position(|surface| *surface == self.current)
        else {
            return false;
        };
        let count = FocusSurface::ORDER.len();
        for offset in 1..=count {
            let index = match direction {
                FocusDirection::Forward => (current_index + offset) % count,
                FocusDirection::Backward => (current_index + count - (offset % count)) % count,
            };
            let candidate = FocusSurface::ORDER[index];
            if is_enabled(candidate) {
                self.focus(candidate);
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::{FocusCoordinator, FocusDirection, FocusSurface};

    #[test]
    fn traversal_skips_disabled_and_noninteractive_surfaces() {
        let mut focus = FocusCoordinator::default();
        focus.focus(FocusSurface::TabStrip);
        assert!(focus.traverse(FocusDirection::Forward, |surface| {
            !matches!(surface, FocusSurface::CommandBar | FocusSurface::AddressBar)
        }));
        assert_eq!(focus.current(), FocusSurface::Search);

        assert!(focus.traverse(FocusDirection::Backward, |surface| {
            !matches!(surface, FocusSurface::CommandBar | FocusSurface::AddressBar)
        }));
        assert_eq!(focus.current(), FocusSurface::TabStrip);
    }

    #[test]
    fn input_focus_restores_the_previous_surface_once() {
        let mut focus = FocusCoordinator::default();
        focus.focus(FocusSurface::AddressBar);
        assert_eq!(focus.previous(), Some(FocusSurface::FileView));
        assert!(focus.restore_previous());
        assert_eq!(focus.current(), FocusSurface::FileView);
        assert!(!focus.restore_previous());
    }
}
