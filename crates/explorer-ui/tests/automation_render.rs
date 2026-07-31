use std::sync::{Arc, Mutex};

use explorer_automation::ScriptRegistry;
use explorer_ui::automation::{
    AutomationManagerState, AutomationManagerView, SummaryPresentationMode,
};
use gpui::{AppContext as _, TestAppContext, px, size};

#[gpui::test]
fn automation_manager_and_loading_summary_render_without_blocking(cx: &mut TestAppContext) {
    let mut state = AutomationManagerState::new(Arc::new(Mutex::new(ScriptRegistry::default())));
    state.begin_summary("visual-test".into(), SummaryPresentationMode::Docked);
    let window = cx.open_window(size(px(480.0), px(320.0)), |_, _| AutomationManagerView {
        state,
    });
    let any_window = window.into();
    cx.update_window(any_window, |_, window, cx| window.draw(cx).clear())
        .expect("test window remains available");
}
