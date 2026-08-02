//! Apartment-neutral OLE drag-and-drop session and effect negotiation domain.

use std::path::{Path, Prefix};

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

/// Resolves Explorer's unmodified filesystem drag default from real source and destination
/// volumes, while preserving the effects actually advertised by the OLE source.
pub fn default_filesystem_drop_effect(
    sources: &[std::path::PathBuf],
    destination: &Path,
    allowed: TransferEffects,
) -> DragEffect {
    let destination_volume = windows_volume_prefix(destination);
    let same_volume = destination_volume
        .as_ref()
        .is_some_and(|destination_volume| {
            !sources.is_empty()
                && sources.iter().all(|source| {
                    windows_volume_prefix(source).as_ref() == Some(destination_volume)
                })
        });
    if same_volume && allowed.move_item {
        DragEffect::Move
    } else if allowed.copy {
        DragEffect::Copy
    } else if allowed.move_item {
        DragEffect::Move
    } else if allowed.link {
        DragEffect::Link
    } else {
        DragEffect::None
    }
}

/// Negotiates one filesystem drop using live modifiers, source preference, and Explorer's
/// same-volume/cross-volume default when the source intentionally supplies no preference.
pub fn negotiate_filesystem_drop_effect(
    allowed: TransferEffects,
    preferred: DragEffect,
    modifiers: DragModifiers,
    target_can_write: bool,
    sources: &[std::path::PathBuf],
    destination: &Path,
) -> DragEffect {
    if !target_can_write || modifiers.alt || modifiers.control || modifiers.shift {
        return negotiate_effect(allowed, preferred, modifiers, target_can_write);
    }
    let preferred = if preferred == DragEffect::None {
        default_filesystem_drop_effect(sources, destination, allowed)
    } else {
        preferred
    };
    negotiate_effect(allowed, preferred, modifiers, target_can_write)
}

/// Rejects targets that Explorer cannot safely use for a filesystem drop.
pub fn filesystem_drop_destination_is_valid(
    sources: &[std::path::PathBuf],
    destination: &Path,
    effect: DragEffect,
) -> bool {
    if sources.is_empty() || effect == DragEffect::None || !destination.is_absolute() {
        return false;
    }
    let destination = normalized_windows_path(destination);
    sources.iter().all(|source| {
        if !source.is_absolute() {
            return false;
        }
        let normalized_source = normalized_windows_path(source);
        if destination == normalized_source
            || destination
                .strip_prefix(&normalized_source)
                .is_some_and(|suffix| suffix.starts_with('\\'))
        {
            return false;
        }
        effect != DragEffect::Move
            || source
                .parent()
                .is_none_or(|parent| normalized_windows_path(parent) != destination)
    })
}

fn normalized_windows_path(path: &Path) -> String {
    let normalized = path
        .as_os_str()
        .to_string_lossy()
        .replace('/', "\\")
        .to_ascii_lowercase();
    normalized
        .strip_suffix('\\')
        .unwrap_or(&normalized)
        .to_owned()
}

fn windows_volume_prefix(path: &Path) -> Option<String> {
    let std::path::Component::Prefix(prefix) = path.components().next()? else {
        return None;
    };
    let identity = match prefix.kind() {
        Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => {
            format!("disk:{}", char::from(letter).to_ascii_lowercase())
        }
        Prefix::UNC(server, share) | Prefix::VerbatimUNC(server, share) => format!(
            "unc:{}\\{}",
            server.to_string_lossy().to_ascii_lowercase(),
            share.to_string_lossy().to_ascii_lowercase()
        ),
        Prefix::DeviceNS(device) => {
            format!("device:{}", device.to_string_lossy().to_ascii_lowercase())
        }
        Prefix::Verbatim(value) => {
            format!("verbatim:{}", value.to_string_lossy().to_ascii_lowercase())
        }
    };
    Some(identity)
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

    #[test]
    fn left_drag_filesystem_default_matches_explorer_same_and_cross_volume_semantics() {
        let allowed = TransferEffects {
            copy: true,
            move_item: true,
            link: false,
        };
        assert_eq!(
            default_filesystem_drop_effect(
                &[std::path::PathBuf::from(r"C:\source\one.txt")],
                Path::new(r"C:\destination"),
                allowed,
            ),
            DragEffect::Move
        );
        assert_eq!(
            default_filesystem_drop_effect(
                &[std::path::PathBuf::from(r"C:\source\one.txt")],
                Path::new(r"D:\destination"),
                allowed,
            ),
            DragEffect::Copy
        );
        assert_eq!(
            default_filesystem_drop_effect(
                &[std::path::PathBuf::from(r"\\server\share\source\one.txt")],
                Path::new(r"\\SERVER\SHARE\destination"),
                allowed,
            ),
            DragEffect::Move
        );
    }

    #[test]
    fn left_drag_live_ctrl_and_shift_override_filesystem_default_and_source_preference() {
        let allowed = TransferEffects {
            copy: true,
            move_item: true,
            link: false,
        };
        let sources = [std::path::PathBuf::from(r"C:\source\one.txt")];
        let destination = Path::new(r"D:\destination");
        assert_eq!(
            negotiate_filesystem_drop_effect(
                allowed,
                DragEffect::Move,
                DragModifiers {
                    control: true,
                    ..DragModifiers::default()
                },
                true,
                &sources,
                destination,
            ),
            DragEffect::Copy
        );
        assert_eq!(
            negotiate_filesystem_drop_effect(
                allowed,
                DragEffect::Copy,
                DragModifiers {
                    shift: true,
                    ..DragModifiers::default()
                },
                true,
                &sources,
                destination,
            ),
            DragEffect::Move
        );
    }

    #[test]
    fn left_drag_rejects_self_descendant_and_same_parent_move_but_allows_copy() {
        let folder = std::path::PathBuf::from(r"C:\source\folder");
        assert!(!filesystem_drop_destination_is_valid(
            std::slice::from_ref(&folder),
            Path::new(r"C:\source\folder"),
            DragEffect::Move,
        ));
        assert!(!filesystem_drop_destination_is_valid(
            std::slice::from_ref(&folder),
            Path::new(r"C:\source\folder\child"),
            DragEffect::Copy,
        ));
        let file = std::path::PathBuf::from(r"C:\source\one.txt");
        assert!(!filesystem_drop_destination_is_valid(
            std::slice::from_ref(&file),
            Path::new(r"C:\source"),
            DragEffect::Move,
        ));
        assert!(filesystem_drop_destination_is_valid(
            std::slice::from_ref(&file),
            Path::new(r"C:\source"),
            DragEffect::Copy,
        ));
        assert!(filesystem_drop_destination_is_valid(
            &[file],
            Path::new(r"C:\destination"),
            DragEffect::Move,
        ));
    }
}
