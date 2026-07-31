//! Apartment-neutral OLE drag-and-drop session and effect negotiation domain.

use crate::{LocationDescriptor, RequestContext, TransferEffects};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DragButton {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DragEffect {
    None,
    Copy,
    Move,
    Link,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DragModifiers {
    pub control: bool,
    pub shift: bool,
    pub alt: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DropTargetKind {
    FileView,
    FolderItem,
    NavigationItem,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AutoScrollDirection {
    Up,
    Down,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum DragSessionState {
    #[default]
    Idle,
    Candidate {
        origin_x: f32,
        origin_y: f32,
        button: DragButton,
    },
    Dragging {
        button: DragButton,
        effect: DragEffect,
        target: Option<DropTargetKind>,
        auto_scroll: Option<AutoScrollDirection>,
    },
    Dropped(DragEffect),
    Cancelled,
    Failed,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DragSession {
    state: DragSessionState,
    threshold_x: f32,
    threshold_y: f32,
}

impl DragSession {
    pub fn new(threshold_x: f32, threshold_y: f32) -> Self {
        Self {
            state: DragSessionState::Idle,
            threshold_x: threshold_x.max(1.0),
            threshold_y: threshold_y.max(1.0),
        }
    }

    pub const fn state(&self) -> &DragSessionState {
        &self.state
    }

    pub fn begin_candidate(&mut self, x: f32, y: f32, button: DragButton) -> bool {
        if matches!(
            self.state,
            DragSessionState::Dropped(_) | DragSessionState::Cancelled | DragSessionState::Failed
        ) {
            self.reset();
        }
        if !matches!(self.state, DragSessionState::Idle) || !x.is_finite() || !y.is_finite() {
            return false;
        }
        self.state = DragSessionState::Candidate {
            origin_x: x,
            origin_y: y,
            button,
        };
        true
    }

    pub fn update_pointer(&mut self, x: f32, y: f32) -> bool {
        let DragSessionState::Candidate {
            origin_x,
            origin_y,
            button,
        } = self.state
        else {
            return false;
        };
        if (x - origin_x).abs() < self.threshold_x && (y - origin_y).abs() < self.threshold_y {
            return false;
        }
        self.state = DragSessionState::Dragging {
            button,
            effect: DragEffect::None,
            target: None,
            auto_scroll: None,
        };
        true
    }

    pub fn update_target(
        &mut self,
        target: DropTargetKind,
        effect: DragEffect,
        auto_scroll: Option<AutoScrollDirection>,
    ) -> bool {
        let DragSessionState::Dragging { button, .. } = self.state else {
            return false;
        };
        self.state = DragSessionState::Dragging {
            button,
            effect,
            target: Some(target),
            auto_scroll,
        };
        true
    }

    pub fn begin_external(
        &mut self,
        target: DropTargetKind,
        effect: DragEffect,
        auto_scroll: Option<AutoScrollDirection>,
    ) {
        self.state = DragSessionState::Dragging {
            button: DragButton::Left,
            effect,
            target: Some(target),
            auto_scroll,
        };
    }

    pub fn leave_target(&mut self) -> bool {
        let DragSessionState::Dragging { button, .. } = self.state else {
            return false;
        };
        self.state = DragSessionState::Dragging {
            button,
            effect: DragEffect::None,
            target: None,
            auto_scroll: None,
        };
        true
    }

    pub fn finish(&mut self, terminal: DragSessionState) -> bool {
        if !matches!(
            terminal,
            DragSessionState::Dropped(_) | DragSessionState::Cancelled | DragSessionState::Failed
        ) || !matches!(
            self.state,
            DragSessionState::Candidate { .. } | DragSessionState::Dragging { .. }
        ) {
            return false;
        }
        self.state = terminal;
        true
    }

    pub fn reset(&mut self) {
        self.state = DragSessionState::Idle;
    }
}

#[derive(Clone, Debug)]
pub struct DropTargetSnapshot {
    pub context: RequestContext,
    pub destination: LocationDescriptor,
    pub can_write: bool,
    pub target: DropTargetKind,
    pub generation: u64,
}

pub const fn negotiate_effect(
    allowed: TransferEffects,
    preferred: DragEffect,
    modifiers: DragModifiers,
    target_can_write: bool,
) -> DragEffect {
    if !target_can_write {
        return DragEffect::None;
    }
    if modifiers.alt && allowed.link {
        return DragEffect::Link;
    }
    if modifiers.control && allowed.copy {
        return DragEffect::Copy;
    }
    if modifiers.shift && allowed.move_item {
        return DragEffect::Move;
    }
    let preferred_allowed = match preferred {
        DragEffect::Copy => allowed.copy,
        DragEffect::Move => allowed.move_item,
        DragEffect::Link => allowed.link,
        DragEffect::None => false,
    };
    if preferred_allowed {
        return preferred;
    }
    if allowed.copy {
        DragEffect::Copy
    } else if allowed.move_item {
        DragEffect::Move
    } else if allowed.link {
        DragEffect::Link
    } else {
        DragEffect::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn threshold_and_every_terminal_path_clean_transient_state() {
        for terminal in [
            DragSessionState::Dropped(DragEffect::Copy),
            DragSessionState::Cancelled,
            DragSessionState::Failed,
        ] {
            let mut session = DragSession::new(4.0, 4.0);
            assert!(session.begin_candidate(10.0, 10.0, DragButton::Left));
            assert!(!session.update_pointer(13.0, 13.0));
            assert!(session.update_pointer(14.0, 10.0));
            assert!(session.update_target(
                DropTargetKind::FileView,
                DragEffect::Copy,
                Some(AutoScrollDirection::Down)
            ));
            assert!(session.finish(terminal.clone()));
            assert_eq!(session.state(), &terminal);
            assert!(session.begin_candidate(20.0, 20.0, DragButton::Right));
            assert!(matches!(
                session.state(),
                DragSessionState::Candidate {
                    button: DragButton::Right,
                    ..
                }
            ));
        }
    }

    #[test]
    fn modifiers_preferred_effect_and_capability_are_deterministic() {
        let all = TransferEffects {
            copy: true,
            move_item: true,
            link: true,
        };
        assert_eq!(
            negotiate_effect(all, DragEffect::Move, DragModifiers::default(), true),
            DragEffect::Move
        );
        assert_eq!(
            negotiate_effect(
                all,
                DragEffect::Move,
                DragModifiers {
                    control: true,
                    ..DragModifiers::default()
                },
                true
            ),
            DragEffect::Copy
        );
        assert_eq!(
            negotiate_effect(all, DragEffect::Move, DragModifiers::default(), false),
            DragEffect::None
        );
    }
}
