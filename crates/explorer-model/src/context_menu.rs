//! Apartment-neutral Shell context-menu request and session state.

use crate::ShellContextMenuTarget;

/// Provider-neutral logical metrics for the Windows-style context-menu presentation.
///
/// Local converts these values to physical pixels using the popup monitor DPI. GPUI already
/// renders in logical pixels, so ADB/SFTP consume the same values directly.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContextMenuVisualMetrics {
    pub row_height: u16,
    pub separator_height: u16,
    pub minimum_width: u16,
    pub maximum_width: u16,
    pub icon_gutter: u16,
    pub icon_size: u16,
    pub icon_left: u16,
    pub right_inset: u16,
    pub divider_right_inset: u16,
    pub outer_padding: u16,
    pub font_size: u16,
    pub right_shadow_extent: u16,
    pub bottom_shadow_extent: u16,
}

pub const WINDOWS_CONTEXT_MENU_VISUAL_METRICS: ContextMenuVisualMetrics =
    ContextMenuVisualMetrics {
        row_height: 23,
        separator_height: 7,
        minimum_width: 282,
        maximum_width: 520,
        icon_gutter: 42,
        icon_size: 16,
        icon_left: 13,
        right_inset: 24,
        divider_right_inset: 8,
        outer_padding: 3,
        font_size: 15,
        right_shadow_extent: 6,
        bottom_shadow_extent: 8,
    };

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContextMenuPaletteRgb8 {
    pub surface: [u8; 3],
    pub hover: [u8; 3],
    pub text: [u8; 3],
    pub disabled_text: [u8; 3],
    pub divider: [u8; 3],
}

pub const WINDOWS_CONTEXT_MENU_LIGHT_PALETTE: ContextMenuPaletteRgb8 = ContextMenuPaletteRgb8 {
    surface: [249, 249, 249],
    hover: [233, 233, 233],
    text: [26, 26, 26],
    disabled_text: [128, 128, 128],
    divider: [215, 215, 215],
};

pub const WINDOWS_CONTEXT_MENU_DARK_PALETTE: ContextMenuPaletteRgb8 = ContextMenuPaletteRgb8 {
    surface: [43, 43, 43],
    hover: [61, 61, 61],
    text: [242, 242, 242],
    disabled_text: [152, 152, 152],
    divider: [72, 72, 72],
};

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

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ContextMenuColorScheme {
    #[default]
    Light,
    Dark,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextMenuRequest {
    pub target: ShellContextMenuTarget,
    pub owner_window: u64,
    pub point: MenuPoint,
    pub keyboard_invoked: bool,
    pub invocation_profile: ContextMenuInvocationProfile,
    pub color_scheme: ContextMenuColorScheme,
    /// Enables the application-owned documented Win32/GDI popup host for this popup only.
    /// Canonical-verb requests leave this false because they do not display a menu.
    pub immersive_native_context_menus: bool,
    /// Application-owned clipboard state captured when the popup request is created.
    pub paste_available: bool,
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
    Paste,
    CopyPath,
    CreateShortcut,
    Delete,
    Rename,
    Share,
    PinToStart,
    ToggleQuickAccess,
    AddBookmark,
    Properties,
}

impl ContextMenuHostCommand {
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Cut => "cut",
            Self::Copy => "copy",
            Self::Paste => "paste",
            Self::CopyPath => "copyaspath",
            Self::CreateShortcut => "link",
            Self::Delete => "delete",
            Self::Rename => "rename",
            Self::Share => "windows.share",
            Self::PinToStart => "pintostartscreen",
            Self::ToggleQuickAccess => "togglequickaccess",
            Self::AddBookmark => "addbookmark",
            Self::Properties => "properties",
        }
    }

    pub fn from_wire_name(value: &str) -> Option<Self> {
        match value {
            "open" => Some(Self::Open),
            "cut" => Some(Self::Cut),
            "copy" => Some(Self::Copy),
            "paste" => Some(Self::Paste),
            "copyaspath" => Some(Self::CopyPath),
            "link" => Some(Self::CreateShortcut),
            "delete" => Some(Self::Delete),
            "rename" => Some(Self::Rename),
            "windows.share" => Some(Self::Share),
            "pintostartscreen" => Some(Self::PinToStart),
            "togglequickaccess" => Some(Self::ToggleQuickAccess),
            "addbookmark" => Some(Self::AddBookmark),
            "properties" => Some(Self::Properties),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContextMenuOutcome {
    Cancelled,
    ReplayRequested {
        x: i32,
        y: i32,
    },
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
            ContextMenuHostCommand::Paste,
            ContextMenuHostCommand::CopyPath,
            ContextMenuHostCommand::CreateShortcut,
            ContextMenuHostCommand::Delete,
            ContextMenuHostCommand::Rename,
            ContextMenuHostCommand::Share,
            ContextMenuHostCommand::PinToStart,
            ContextMenuHostCommand::ToggleQuickAccess,
            ContextMenuHostCommand::AddBookmark,
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

    #[test]
    fn windows_context_menu_contract_matches_the_accepted_local_baseline() {
        assert_eq!(WINDOWS_CONTEXT_MENU_VISUAL_METRICS.row_height, 23);
        assert_eq!(WINDOWS_CONTEXT_MENU_VISUAL_METRICS.minimum_width, 282);
        assert_eq!(WINDOWS_CONTEXT_MENU_VISUAL_METRICS.icon_gutter, 42);
        assert_eq!(WINDOWS_CONTEXT_MENU_VISUAL_METRICS.icon_left, 13);
        assert_eq!(WINDOWS_CONTEXT_MENU_VISUAL_METRICS.font_size, 15);
        assert_eq!(WINDOWS_CONTEXT_MENU_LIGHT_PALETTE.surface, [249; 3]);
        assert_eq!(WINDOWS_CONTEXT_MENU_LIGHT_PALETTE.divider, [215; 3]);
        assert_eq!(WINDOWS_CONTEXT_MENU_DARK_PALETTE.surface, [43; 3]);
    }
}
