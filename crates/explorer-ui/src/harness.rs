//! Deterministic UI behavior harness with no GPUI window or Shell service.

use crate::{
    UiTokens,
    actions::{ActionSource, ActionTrace, ExplorerAction, dispatch_action},
    state::AppViewState,
    theme::{SemanticColorSlot, ThemeMode, ThemeTokens},
};

#[derive(Debug, Default)]
pub struct UiTestHarness {
    state: AppViewState,
    tokens: UiTokens,
    traces: Vec<ActionTrace>,
}

impl UiTestHarness {
    pub const fn state(&self) -> &AppViewState {
        &self.state
    }

    pub const fn tokens(&self) -> &UiTokens {
        &self.tokens
    }

    pub fn dispatch(&mut self, action: ExplorerAction, source: ActionSource) -> ActionTrace {
        let trace = dispatch_action(&mut self.state, action, source);
        self.tokens.theme = match self.state.current_theme() {
            ThemeMode::Light => ThemeTokens::light(),
            ThemeMode::Dark => ThemeTokens::dark(),
        };
        self.traces.push(trace);
        trace
    }

    pub fn traces(&self) -> &[ActionTrace] {
        &self.traces
    }

    pub fn semantic_snapshot(&self) -> Vec<(SemanticColorSlot, crate::theme::Rgba8)> {
        SemanticColorSlot::ALL
            .into_iter()
            .map(|slot| (slot, self.tokens.theme.colors.get(slot)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::UiTestHarness;
    use crate::{
        actions::{ActionOutcome, ActionSource, ExplorerAction},
        focus::FocusSurface,
        theme::{SemanticColorSlot, ThemeMode, ThemeTokens},
    };

    #[test]
    fn disabled_navigation_clicks_and_shortcuts_never_fake_state() {
        let mut harness = UiTestHarness::default();
        let initial_theme = harness.state().current_theme();
        let initial_pane = harness.state().navigation_pane_width();
        let initial_focus = harness.state().focused_surface();
        let initial_tab = harness.state().tabs().active_tab_id();
        for source in [ActionSource::Mouse, ActionSource::Keyboard] {
            for action in [
                ExplorerAction::Back,
                ExplorerAction::Forward,
                ExplorerAction::Up,
            ] {
                assert_eq!(
                    harness.dispatch(action, source).outcome,
                    ActionOutcome::Disabled
                );
                assert_eq!(harness.state().current_theme(), initial_theme);
                assert_eq!(harness.state().navigation_pane_width(), initial_pane);
                assert_eq!(harness.state().focused_surface(), initial_focus);
                assert_eq!(harness.state().tabs().active_tab_id(), initial_tab);
            }
        }
        assert_eq!(harness.traces().len(), 6);
    }

    #[test]
    fn address_search_and_restore_are_owned_by_one_surface_each() {
        let mut harness = UiTestHarness::default();
        harness.dispatch(ExplorerAction::FocusAddress, ActionSource::Keyboard);
        assert_eq!(harness.state().focused_surface(), FocusSurface::AddressBar);
        harness.dispatch(ExplorerAction::RestorePreviousFocus, ActionSource::Keyboard);
        assert_eq!(harness.state().focused_surface(), FocusSurface::FileView);
        harness.dispatch(ExplorerAction::FocusSearch, ActionSource::Mouse);
        assert_eq!(harness.state().focused_surface(), FocusSurface::Search);
        harness.dispatch(ExplorerAction::RestorePreviousFocus, ActionSource::Keyboard);
        assert_eq!(harness.state().focused_surface(), FocusSurface::FileView);
        assert_eq!(harness.traces().len(), 4);
    }

    #[test]
    fn semantic_snapshot_is_stable_and_switches_as_one_provider() {
        let mut harness = UiTestHarness::default();
        let light = harness.semantic_snapshot();
        assert_eq!(light.len(), SemanticColorSlot::ALL.len());
        harness.dispatch(ExplorerAction::ToggleTheme, ActionSource::Programmatic);
        let dark = harness.semantic_snapshot();
        assert_eq!(harness.tokens().theme.mode, ThemeMode::Dark);
        for ((light_slot, light_color), (dark_slot, dark_color)) in light.into_iter().zip(dark) {
            assert_eq!(light_slot, dark_slot);
            if light_slot != SemanticColorSlot::SelectedText {
                assert_ne!(light_color, dark_color);
            }
            assert_eq!(dark_color, ThemeTokens::dark().colors.get(dark_slot));
        }
    }
}
