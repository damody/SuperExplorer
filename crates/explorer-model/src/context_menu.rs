//! Apartment-neutral Shell context-menu request and session state.

use crate::ShellContextMenuTarget;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MenuPoint {
    pub x: i32,
    pub y: i32,
}

/// Per-session Shell menu profile. The target kind still determines whether item-only flags are
/// legal; this value carries only the user-visible normal versus Shift-extended policy.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ContextMenuInvocationProfile {
    #[default]
    Explorer,
    ExplorerExtended,
}

impl ContextMenuInvocationProfile {
    pub const fn extended_verbs(self) -> bool {
        matches!(self, Self::ExplorerExtended)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextMenuRequest {
    pub target: ShellContextMenuTarget,
    pub owner_window: u64,
    pub point: MenuPoint,
    pub keyboard_invoked: bool,
    pub invocation_profile: ContextMenuInvocationProfile,
    /// When present, invoke this canonical Shell verb without showing the popup menu.
    pub requested_verb: Option<String>,
    pub deadline_ms: u32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ContextMenuSessionState {
    #[default]
    Idle,
    Resolving,
    Querying,
    Showing,
    Invoking,
    Cancelled,
    Finished,
    Failed,
    Released,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ContextMenuSession {
    state: ContextMenuSessionState,
    cleanup_count: u8,
}

impl ContextMenuSession {
    pub const fn state(&self) -> ContextMenuSessionState {
        self.state
    }
    pub const fn cleanup_count(&self) -> u8 {
        self.cleanup_count
    }

    pub fn transition(&mut self, next: ContextMenuSessionState) -> bool {
        let valid = matches!(
            (self.state, next),
            (
                ContextMenuSessionState::Idle,
                ContextMenuSessionState::Resolving
            ) | (
                ContextMenuSessionState::Resolving,
                ContextMenuSessionState::Querying | ContextMenuSessionState::Failed
            ) | (
                ContextMenuSessionState::Querying,
                ContextMenuSessionState::Showing | ContextMenuSessionState::Failed
            ) | (
                ContextMenuSessionState::Showing,
                ContextMenuSessionState::Invoking
                    | ContextMenuSessionState::Cancelled
                    | ContextMenuSessionState::Failed
            ) | (
                ContextMenuSessionState::Invoking,
                ContextMenuSessionState::Finished | ContextMenuSessionState::Failed
            ) | (
                ContextMenuSessionState::Cancelled
                    | ContextMenuSessionState::Finished
                    | ContextMenuSessionState::Failed,
                ContextMenuSessionState::Released
            )
        );
        if valid {
            self.state = next;
        }
        valid
    }

    pub fn release(&mut self) -> bool {
        if self.transition(ContextMenuSessionState::Released) {
            self.cleanup_count = self.cleanup_count.saturating_add(1);
            true
        } else {
            false
        }
    }
}

/// Built-in Shell verbs that must be completed by the long-lived application process.
///
/// The native popup is still populated by `IContextMenu` in the isolated worker, but these
/// commands depend on application-owned selection, clipboard, refresh, or editor lifetime.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContextMenuHostCommand {
    Open,
    Cut,
    Copy,
    CopyPath,
    CreateShortcut,
    Delete,
    Rename,
    Share,
    PinToStart,
    ToggleQuickAccess,
    Properties,
}

impl ContextMenuHostCommand {
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Cut => "cut",
            Self::Copy => "copy",
            Self::CopyPath => "copyaspath",
            Self::CreateShortcut => "link",
            Self::Delete => "delete",
            Self::Rename => "rename",
            Self::Share => "windows.share",
            Self::PinToStart => "pintostartscreen",
            Self::ToggleQuickAccess => "togglequickaccess",
            Self::Properties => "properties",
        }
    }

    pub fn from_wire_name(value: &str) -> Option<Self> {
        match value {
            "open" => Some(Self::Open),
            "cut" => Some(Self::Cut),
            "copy" => Some(Self::Copy),
            "copyaspath" => Some(Self::CopyPath),
            "link" => Some(Self::CreateShortcut),
            "delete" => Some(Self::Delete),
            "rename" => Some(Self::Rename),
            "windows.share" => Some(Self::Share),
            "pintostartscreen" => Some(Self::PinToStart),
            "togglequickaccess" => Some(Self::ToggleQuickAccess),
            "properties" => Some(Self::Properties),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContextMenuOutcome {
    Cancelled,
    Invoked {
        command_offset: u32,
    },
    Delegated {
        command_offset: u32,
        command: ContextMenuHostCommand,
        target: ShellContextMenuTarget,
    },
    Failed {
        error: explorer_common::ExplorerError,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_context_command_wire_names_round_trip_without_localized_labels() {
        for command in [
            ContextMenuHostCommand::Open,
            ContextMenuHostCommand::Cut,
            ContextMenuHostCommand::Copy,
            ContextMenuHostCommand::CopyPath,
            ContextMenuHostCommand::CreateShortcut,
            ContextMenuHostCommand::Delete,
            ContextMenuHostCommand::Rename,
            ContextMenuHostCommand::Share,
            ContextMenuHostCommand::PinToStart,
            ContextMenuHostCommand::ToggleQuickAccess,
            ContextMenuHostCommand::Properties,
        ] {
            assert_eq!(
                ContextMenuHostCommand::from_wire_name(command.wire_name()),
                Some(command)
            );
        }
        assert_eq!(
            ContextMenuHostCommand::from_wire_name("provider.command"),
            None
        );
    }

    #[test]
    fn every_terminal_path_releases_exactly_once() {
        for terminal in [
            ContextMenuSessionState::Cancelled,
            ContextMenuSessionState::Failed,
        ] {
            let mut session = ContextMenuSession::default();
            assert!(session.transition(ContextMenuSessionState::Resolving));
            assert!(session.transition(ContextMenuSessionState::Querying));
            assert!(session.transition(ContextMenuSessionState::Showing));
            assert!(session.transition(terminal));
            assert!(session.release());
            assert!(!session.release());
            assert_eq!(session.cleanup_count(), 1);
        }
    }

    #[test]
    fn invocation_profile_is_ordinary_by_default_and_extended_is_session_local() {
        assert!(!ContextMenuInvocationProfile::default().extended_verbs());
        assert!(ContextMenuInvocationProfile::ExplorerExtended.extended_verbs());
    }
}
