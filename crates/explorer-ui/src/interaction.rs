//! Shared visual interaction state for semantic controls.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PointerPhase {
    Idle,
    Hovered,
    Pressed,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DividerInteraction {
    dragging: bool,
    pointer_origin: f32,
    width_origin: LogicalPx,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DividerTerminal {
    PointerUp,
    PointerUpOutside,
    Cancelled,
    WindowBlur,
}

use crate::layout::{LayoutTokens, LogicalPx};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScrollbarKind {
    Navigation,
    FileView,
    FileViewHorizontal,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollbarDragSession {
    pub kind: ScrollbarKind,
    pub grab_offset_y: f32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScrollbarTerminal {
    PointerUp,
    PointerUpOutside,
    Escape,
    WindowBlur,
    CaptureLost,
    TabSwitch,
    ViewSwitch,
    WindowClose,
}

impl ScrollbarDragSession {
    pub fn new(kind: ScrollbarKind, grab_offset_y: f32) -> Option<Self> {
        (grab_offset_y.is_finite() && grab_offset_y >= 0.0).then_some(Self {
            kind,
            grab_offset_y,
        })
    }
}

pub fn scrollbar_thumb_height(viewport: f32, maximum: f32, minimum_thumb: f32) -> Option<f32> {
    if !viewport.is_finite()
        || !maximum.is_finite()
        || !minimum_thumb.is_finite()
        || viewport <= 0.0
        || maximum <= 0.0
    {
        return None;
    }
    Some(
        (viewport * viewport / (viewport + maximum))
            .max(minimum_thumb.max(0.0))
            .min(viewport),
    )
}

pub fn scrollbar_target_offset(
    viewport: f32,
    maximum: f32,
    minimum_thumb: f32,
    pointer_local_y: f32,
    grab_offset_y: f32,
) -> Option<f32> {
    if !pointer_local_y.is_finite() || !grab_offset_y.is_finite() {
        return None;
    }
    let thumb_height = scrollbar_thumb_height(viewport, maximum, minimum_thumb)?;
    let track = (viewport - thumb_height).max(1.0);
    Some(((pointer_local_y - grab_offset_y) / track * maximum).clamp(0.0, maximum))
}

impl Default for DividerInteraction {
    fn default() -> Self {
        Self {
            dragging: false,
            pointer_origin: 0.0,
            width_origin: LayoutTokens::WINDOWS_11.navigation_pane_default_width,
        }
    }
}

impl DividerInteraction {
    pub const fn is_dragging(self) -> bool {
        self.dragging
    }

    pub fn begin(&mut self, pointer_x: f32, current_width: LogicalPx) -> bool {
        if !pointer_x.is_finite() || !current_width.value().is_finite() {
            return false;
        }
        self.dragging = true;
        self.pointer_origin = pointer_x;
        self.width_origin = current_width;
        true
    }

    pub fn update(self, pointer_x: f32, tokens: LayoutTokens) -> Option<LogicalPx> {
        if !self.dragging || !pointer_x.is_finite() {
            return None;
        }
        let requested = self.width_origin.value() + pointer_x - self.pointer_origin;
        Some(LogicalPx::new(requested.clamp(
            tokens.navigation_pane_min_width.value(),
            tokens.navigation_pane_max_width.value(),
        )))
    }

    pub fn finish(&mut self) -> bool {
        std::mem::take(&mut self.dragging)
    }

    pub fn terminate(&mut self, _reason: DividerTerminal) -> bool {
        self.finish()
    }

    pub fn reset(&mut self, tokens: LayoutTokens) -> LogicalPx {
        self.dragging = false;
        tokens.navigation_pane_default_width
    }

    pub fn keyboard_adjust(width: LogicalPx, direction: i8, tokens: LayoutTokens) -> LogicalPx {
        let requested = width.value() + f32::from(direction) * tokens.divider_keyboard_step.value();
        LogicalPx::new(requested.clamp(
            tokens.navigation_pane_min_width.value(),
            tokens.navigation_pane_max_width.value(),
        ))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InteractionState {
    pub enabled: bool,
    pub active: bool,
    pub focused: bool,
    pub pointer: PointerPhase,
}

impl Default for InteractionState {
    fn default() -> Self {
        Self {
            enabled: true,
            active: false,
            focused: false,
            pointer: PointerPhase::Idle,
        }
    }
}

impl InteractionState {
    pub fn set_hovered(&mut self, hovered: bool) {
        self.pointer = if hovered {
            PointerPhase::Hovered
        } else {
            PointerPhase::Idle
        };
    }

    pub fn set_pressed(&mut self, pressed: bool) {
        self.pointer = if pressed {
            PointerPhase::Pressed
        } else {
            PointerPhase::Hovered
        };
    }
}

#[cfg(test)]
mod tests {
    use super::{InteractionState, PointerPhase};

    #[test]
    fn pointer_states_share_one_terminal_path() {
        let mut state = InteractionState::default();
        state.set_hovered(true);
        assert_eq!(state.pointer, PointerPhase::Hovered);
        state.set_pressed(true);
        assert_eq!(state.pointer, PointerPhase::Pressed);
        state.set_pressed(false);
        assert_eq!(state.pointer, PointerPhase::Hovered);
        state.set_hovered(false);
        assert_eq!(state.pointer, PointerPhase::Idle);
    }

    #[test]
    fn divider_drag_clamps_and_every_terminal_path_releases_capture() {
        use super::{DividerInteraction, DividerTerminal};
        use crate::layout::{LayoutTokens, LogicalPx};

        let tokens = LayoutTokens::WINDOWS_11;
        let mut divider = DividerInteraction::default();
        assert!(!divider.begin(f32::NAN, LogicalPx::new(240.0)));
        assert!(divider.begin(240.0, LogicalPx::new(240.0)));
        assert_eq!(
            divider.update(-1_000.0, tokens),
            Some(LogicalPx::new(180.0))
        );
        assert_eq!(
            divider.update(1_000.0, tokens),
            Some(tokens.navigation_pane_max_width)
        );
        for terminal in [
            DividerTerminal::PointerUp,
            DividerTerminal::PointerUpOutside,
            DividerTerminal::Cancelled,
            DividerTerminal::WindowBlur,
        ] {
            assert!(divider.terminate(terminal));
            assert!(!divider.is_dragging());
            assert!(!divider.terminate(terminal));
            assert!(divider.begin(240.0, LogicalPx::new(240.0)));
        }
        assert!(divider.finish());
        assert_eq!(divider.reset(tokens), tokens.navigation_pane_default_width);
        assert_eq!(
            DividerInteraction::keyboard_adjust(LogicalPx::new(180.0), -1, tokens),
            LogicalPx::new(180.0)
        );
        assert_eq!(
            DividerInteraction::keyboard_adjust(tokens.navigation_pane_max_width, 1, tokens),
            tokens.navigation_pane_max_width
        );
    }

    #[test]
    fn scrollbar_geometry_preserves_grab_offset_and_clamps_outside_track() {
        use super::{
            ScrollbarDragSession, ScrollbarKind, scrollbar_target_offset, scrollbar_thumb_height,
        };

        assert!(ScrollbarDragSession::new(ScrollbarKind::FileView, f32::NAN).is_none());
        assert!(ScrollbarDragSession::new(ScrollbarKind::Navigation, -1.0).is_none());
        assert!(scrollbar_thumb_height(0.0, 100.0, 20.0).is_none());
        assert!(scrollbar_thumb_height(100.0, 0.0, 20.0).is_none());
        assert_eq!(scrollbar_thumb_height(100.0, 300.0, 20.0), Some(25.0));
        assert_eq!(
            scrollbar_target_offset(100.0, 300.0, 20.0, 10.0, 10.0),
            Some(0.0)
        );
        assert_eq!(
            scrollbar_target_offset(100.0, 300.0, 20.0, 47.5, 10.0),
            Some(150.0)
        );
        assert_eq!(
            scrollbar_target_offset(100.0, 300.0, 20.0, 1_000.0, 10.0),
            Some(300.0)
        );
        assert_eq!(
            scrollbar_target_offset(200.0, 300.0, 20.0, 100.0, 10.0),
            Some(225.0)
        );
    }
}
