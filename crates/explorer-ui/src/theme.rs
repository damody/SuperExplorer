//! Typed semantic colors shared by every Explorer UI surface.

/// An sRGB color with an explicit alpha channel.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rgba8 {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

impl Rgba8 {
    pub const fn opaque(red: u8, green: u8, blue: u8) -> Self {
        Self {
            red,
            green,
            blue,
            alpha: u8::MAX,
        }
    }

    pub fn to_gpui(self) -> gpui::Rgba {
        gpui::Rgba {
            r: f32::from(self.red) / 255.0,
            g: f32::from(self.green) / 255.0,
            b: f32::from(self.blue) / 255.0,
            a: f32::from(self.alpha) / 255.0,
        }
    }
}

/// Shared modal backdrop used by Explorer-style dialogs.
pub const MODAL_BACKDROP: Rgba8 = Rgba8 {
    red: 0,
    green: 0,
    blue: 0,
    alpha: 51,
};

/// Shell-owned colors used only by the geometry-stable navigation fallback.
/// Native Shell bitmap payloads replace this palette as they arrive.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NavigationIconPalette {
    pub home: Rgba8,
    pub gallery: Rgba8,
    pub gallery_border: Rgba8,
    pub one_drive: Rgba8,
    pub computer: Rgba8,
    pub computer_border: Rgba8,
    pub downloads: Rgba8,
    pub documents: Rgba8,
    pub music: Rgba8,
    pub videos: Rgba8,
    pub folder: Rgba8,
    pub drive: Rgba8,
    pub drive_border: Rgba8,
}

impl NavigationIconPalette {
    pub const WINDOWS_11: Self = Self {
        home: Rgba8::opaque(242, 140, 40),
        gallery: Rgba8::opaque(26, 163, 232),
        gallery_border: Rgba8::opaque(8, 124, 193),
        one_drive: Rgba8::opaque(21, 155, 228),
        computer: Rgba8::opaque(55, 185, 212),
        computer_border: Rgba8::opaque(36, 103, 124),
        downloads: Rgba8::opaque(0, 183, 160),
        documents: Rgba8::opaque(142, 175, 208),
        music: Rgba8::opaque(231, 121, 114),
        videos: Rgba8::opaque(138, 43, 226),
        folder: Rgba8::opaque(255, 200, 61),
        drive: Rgba8::opaque(216, 216, 216),
        drive_border: Rgba8::opaque(122, 122, 122),
    };
}

/// Stable identifiers used by diagnostics and contract tests.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SemanticColorSlot {
    Surface,
    SubtleSurface,
    ControlFill,
    ControlHover,
    ControlPressed,
    SelectedActive,
    SelectedText,
    SelectedInactive,
    Divider,
    TextPrimary,
    TextSecondary,
    TextDisabled,
    Focus,
    Danger,
    Accent,
    ToolbarFill,
    AddressFill,
    SearchFill,
    RowHover,
    MenuFill,
    CaptionHover,
}

impl SemanticColorSlot {
    pub const ALL: [Self; 21] = [
        Self::Surface,
        Self::SubtleSurface,
        Self::ControlFill,
        Self::ControlHover,
        Self::ControlPressed,
        Self::SelectedActive,
        Self::SelectedText,
        Self::SelectedInactive,
        Self::Divider,
        Self::TextPrimary,
        Self::TextSecondary,
        Self::TextDisabled,
        Self::Focus,
        Self::Danger,
        Self::Accent,
        Self::ToolbarFill,
        Self::AddressFill,
        Self::SearchFill,
        Self::RowHover,
        Self::MenuFill,
        Self::CaptionHover,
    ];
}

/// Complete semantic palette. Adding a slot requires updating every constructor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SemanticColors {
    pub surface: Rgba8,
    pub subtle_surface: Rgba8,
    pub control_fill: Rgba8,
    pub control_hover: Rgba8,
    pub control_pressed: Rgba8,
    pub selected_active: Rgba8,
    pub selected_text: Rgba8,
    pub selected_inactive: Rgba8,
    pub divider: Rgba8,
    pub text_primary: Rgba8,
    pub text_secondary: Rgba8,
    pub text_disabled: Rgba8,
    pub focus: Rgba8,
    pub danger: Rgba8,
    pub accent: Rgba8,
    pub toolbar_fill: Rgba8,
    pub address_fill: Rgba8,
    pub search_fill: Rgba8,
    pub row_hover: Rgba8,
    pub menu_fill: Rgba8,
    pub caption_hover: Rgba8,
}

impl SemanticColors {
    pub const fn get(self, slot: SemanticColorSlot) -> Rgba8 {
        match slot {
            SemanticColorSlot::Surface => self.surface,
            SemanticColorSlot::SubtleSurface => self.subtle_surface,
            SemanticColorSlot::ControlFill => self.control_fill,
            SemanticColorSlot::ControlHover => self.control_hover,
            SemanticColorSlot::ControlPressed => self.control_pressed,
            SemanticColorSlot::SelectedActive => self.selected_active,
            SemanticColorSlot::SelectedText => self.selected_text,
            SemanticColorSlot::SelectedInactive => self.selected_inactive,
            SemanticColorSlot::Divider => self.divider,
            SemanticColorSlot::TextPrimary => self.text_primary,
            SemanticColorSlot::TextSecondary => self.text_secondary,
            SemanticColorSlot::TextDisabled => self.text_disabled,
            SemanticColorSlot::Focus => self.focus,
            SemanticColorSlot::Danger => self.danger,
            SemanticColorSlot::Accent => self.accent,
            SemanticColorSlot::ToolbarFill => self.toolbar_fill,
            SemanticColorSlot::AddressFill => self.address_fill,
            SemanticColorSlot::SearchFill => self.search_fill,
            SemanticColorSlot::RowHover => self.row_hover,
            SemanticColorSlot::MenuFill => self.menu_fill,
            SemanticColorSlot::CaptionHover => self.caption_hover,
        }
    }
}

/// User-selectable application theme.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThemeMode {
    Light,
    Dark,
}

/// Windows semantic system colors used when high contrast is active.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SystemColorRole {
    Window,
    WindowText,
    ButtonFace,
    GrayText,
    Highlight,
    HighlightText,
    Hotlight,
}

/// Complete high-contrast mapping without assuming fixed RGB values.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HighContrastMappings {
    pub surface: SystemColorRole,
    pub subtle_surface: SystemColorRole,
    pub control_fill: SystemColorRole,
    pub control_hover: SystemColorRole,
    pub control_pressed: SystemColorRole,
    pub selected_active: SystemColorRole,
    pub selected_text: SystemColorRole,
    pub selected_inactive: SystemColorRole,
    pub divider: SystemColorRole,
    pub text_primary: SystemColorRole,
    pub text_secondary: SystemColorRole,
    pub text_disabled: SystemColorRole,
    pub focus: SystemColorRole,
    pub danger: SystemColorRole,
    pub accent: SystemColorRole,
    pub toolbar_fill: SystemColorRole,
    pub address_fill: SystemColorRole,
    pub search_fill: SystemColorRole,
    pub row_hover: SystemColorRole,
    pub menu_fill: SystemColorRole,
    pub caption_hover: SystemColorRole,
}

impl HighContrastMappings {
    pub const WINDOWS: Self = Self {
        surface: SystemColorRole::Window,
        subtle_surface: SystemColorRole::Window,
        control_fill: SystemColorRole::ButtonFace,
        control_hover: SystemColorRole::Highlight,
        control_pressed: SystemColorRole::Highlight,
        selected_active: SystemColorRole::Highlight,
        selected_text: SystemColorRole::HighlightText,
        selected_inactive: SystemColorRole::ButtonFace,
        divider: SystemColorRole::WindowText,
        text_primary: SystemColorRole::WindowText,
        text_secondary: SystemColorRole::WindowText,
        text_disabled: SystemColorRole::GrayText,
        focus: SystemColorRole::Highlight,
        danger: SystemColorRole::Hotlight,
        accent: SystemColorRole::Highlight,
        toolbar_fill: SystemColorRole::ButtonFace,
        address_fill: SystemColorRole::Window,
        search_fill: SystemColorRole::Window,
        row_hover: SystemColorRole::Highlight,
        menu_fill: SystemColorRole::ButtonFace,
        caption_hover: SystemColorRole::Highlight,
    };

    pub const fn get(self, slot: SemanticColorSlot) -> SystemColorRole {
        match slot {
            SemanticColorSlot::Surface => self.surface,
            SemanticColorSlot::SubtleSurface => self.subtle_surface,
            SemanticColorSlot::ControlFill => self.control_fill,
            SemanticColorSlot::ControlHover => self.control_hover,
            SemanticColorSlot::ControlPressed => self.control_pressed,
            SemanticColorSlot::SelectedActive => self.selected_active,
            SemanticColorSlot::SelectedText => self.selected_text,
            SemanticColorSlot::SelectedInactive => self.selected_inactive,
            SemanticColorSlot::Divider => self.divider,
            SemanticColorSlot::TextPrimary => self.text_primary,
            SemanticColorSlot::TextSecondary => self.text_secondary,
            SemanticColorSlot::TextDisabled => self.text_disabled,
            SemanticColorSlot::Focus => self.focus,
            SemanticColorSlot::Danger => self.danger,
            SemanticColorSlot::Accent => self.accent,
            SemanticColorSlot::ToolbarFill => self.toolbar_fill,
            SemanticColorSlot::AddressFill => self.address_fill,
            SemanticColorSlot::SearchFill => self.search_fill,
            SemanticColorSlot::RowHover => self.row_hover,
            SemanticColorSlot::MenuFill => self.menu_fill,
            SemanticColorSlot::CaptionHover => self.caption_hover,
        }
    }
}

/// All theme data injected at the Explorer root.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ThemeTokens {
    pub mode: ThemeMode,
    pub colors: SemanticColors,
    pub high_contrast: HighContrastMappings,
    pub high_contrast_active: bool,
}

impl ThemeTokens {
    pub const fn light() -> Self {
        Self {
            mode: ThemeMode::Light,
            colors: SemanticColors {
                surface: Rgba8::opaque(255, 255, 255),
                subtle_surface: Rgba8::opaque(232, 232, 232),
                control_fill: Rgba8::opaque(253, 253, 253),
                control_hover: Rgba8::opaque(248, 248, 248),
                control_pressed: Rgba8::opaque(240, 240, 240),
                selected_active: Rgba8::opaque(0, 120, 212),
                selected_text: Rgba8::opaque(255, 255, 255),
                selected_inactive: Rgba8::opaque(240, 240, 240),
                divider: Rgba8::opaque(214, 214, 214),
                text_primary: Rgba8::opaque(26, 26, 26),
                text_secondary: Rgba8::opaque(90, 90, 90),
                text_disabled: Rgba8::opaque(158, 158, 158),
                focus: Rgba8::opaque(0, 120, 212),
                danger: Rgba8::opaque(196, 43, 28),
                accent: Rgba8::opaque(0, 120, 212),
                toolbar_fill: Rgba8::opaque(248, 248, 248),
                address_fill: Rgba8::opaque(253, 253, 253),
                search_fill: Rgba8::opaque(253, 253, 253),
                row_hover: Rgba8::opaque(245, 245, 245),
                menu_fill: Rgba8::opaque(249, 249, 249),
                caption_hover: Rgba8::opaque(224, 224, 224),
            },
            high_contrast: HighContrastMappings::WINDOWS,
            high_contrast_active: false,
        }
    }

    pub const fn dark() -> Self {
        Self {
            mode: ThemeMode::Dark,
            colors: SemanticColors {
                surface: Rgba8::opaque(32, 32, 32),
                subtle_surface: Rgba8::opaque(39, 39, 39),
                control_fill: Rgba8::opaque(45, 45, 45),
                control_hover: Rgba8::opaque(58, 58, 58),
                control_pressed: Rgba8::opaque(69, 69, 69),
                selected_active: Rgba8::opaque(0, 78, 140),
                selected_text: Rgba8::opaque(255, 255, 255),
                selected_inactive: Rgba8::opaque(59, 59, 59),
                divider: Rgba8::opaque(63, 63, 63),
                text_primary: Rgba8::opaque(255, 255, 255),
                text_secondary: Rgba8::opaque(199, 199, 199),
                text_disabled: Rgba8::opaque(119, 119, 119),
                focus: Rgba8::opaque(96, 205, 255),
                danger: Rgba8::opaque(255, 153, 164),
                accent: Rgba8::opaque(96, 205, 255),
                toolbar_fill: Rgba8::opaque(39, 39, 39),
                address_fill: Rgba8::opaque(45, 45, 45),
                search_fill: Rgba8::opaque(45, 45, 45),
                row_hover: Rgba8::opaque(47, 47, 47),
                menu_fill: Rgba8::opaque(44, 44, 44),
                caption_hover: Rgba8::opaque(58, 58, 58),
            },
            high_contrast: HighContrastMappings::WINDOWS,
            high_contrast_active: false,
        }
    }

    /// Resolves every visual slot through the active Windows high-contrast
    /// system-color table. The mode remains light/dark-neutral because the
    /// operating system, rather than the application's theme toggle, owns it.
    pub fn windows_high_contrast(resolve: impl Fn(SystemColorRole) -> Rgba8) -> Self {
        let mapping = HighContrastMappings::WINDOWS;
        Self {
            mode: ThemeMode::Light,
            colors: SemanticColors {
                surface: resolve(mapping.surface),
                subtle_surface: resolve(mapping.subtle_surface),
                control_fill: resolve(mapping.control_fill),
                control_hover: resolve(mapping.control_hover),
                control_pressed: resolve(mapping.control_pressed),
                selected_active: resolve(mapping.selected_active),
                selected_text: resolve(mapping.selected_text),
                selected_inactive: resolve(mapping.selected_inactive),
                divider: resolve(mapping.divider),
                text_primary: resolve(mapping.text_primary),
                text_secondary: resolve(mapping.text_secondary),
                text_disabled: resolve(mapping.text_disabled),
                focus: resolve(mapping.focus),
                danger: resolve(mapping.danger),
                accent: resolve(mapping.accent),
                toolbar_fill: resolve(mapping.toolbar_fill),
                address_fill: resolve(mapping.address_fill),
                search_fill: resolve(mapping.search_fill),
                row_hover: resolve(mapping.row_hover),
                menu_fill: resolve(mapping.menu_fill),
                caption_hover: resolve(mapping.caption_hover),
            },
            high_contrast: mapping,
            high_contrast_active: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Rgba8, SemanticColorSlot, SemanticColors, SystemColorRole, ThemeTokens};

    const DISTINCT_CONTRACT_COLORS: SemanticColors = SemanticColors {
        surface: Rgba8::opaque(1, 0, 0),
        subtle_surface: Rgba8::opaque(2, 0, 0),
        control_fill: Rgba8::opaque(3, 0, 0),
        control_hover: Rgba8::opaque(4, 0, 0),
        control_pressed: Rgba8::opaque(5, 0, 0),
        selected_active: Rgba8::opaque(6, 0, 0),
        selected_text: Rgba8::opaque(7, 0, 0),
        selected_inactive: Rgba8::opaque(8, 0, 0),
        divider: Rgba8::opaque(9, 0, 0),
        text_primary: Rgba8::opaque(10, 0, 0),
        text_secondary: Rgba8::opaque(11, 0, 0),
        text_disabled: Rgba8::opaque(12, 0, 0),
        focus: Rgba8::opaque(13, 0, 0),
        danger: Rgba8::opaque(14, 0, 0),
        accent: Rgba8::opaque(15, 0, 0),
        toolbar_fill: Rgba8::opaque(16, 0, 0),
        address_fill: Rgba8::opaque(17, 0, 0),
        search_fill: Rgba8::opaque(18, 0, 0),
        row_hover: Rgba8::opaque(19, 0, 0),
        menu_fill: Rgba8::opaque(20, 0, 0),
        caption_hover: Rgba8::opaque(21, 0, 0),
    };

    #[test]
    fn semantic_color_contract_contains_every_required_slot_once() {
        assert_eq!(SemanticColorSlot::ALL.len(), 21);
        for (index, slot) in SemanticColorSlot::ALL.into_iter().enumerate() {
            assert_eq!(
                DISTINCT_CONTRACT_COLORS.get(slot).red,
                u8::try_from(index + 1).expect("contract index fits in u8")
            );
        }
    }

    #[test]
    fn light_and_dark_define_every_semantic_slot_independently() {
        let light = ThemeTokens::light();
        let dark = ThemeTokens::dark();
        for slot in SemanticColorSlot::ALL {
            if slot != SemanticColorSlot::SelectedText {
                assert_ne!(light.colors.get(slot), dark.colors.get(slot));
            }
        }
        assert_ne!(
            dark.colors.surface.red,
            u8::MAX - light.colors.surface.red,
            "dark palette must not be generated by channel inversion"
        );
    }

    #[test]
    fn high_contrast_mapping_uses_windows_semantic_roles_for_every_slot() {
        let mappings = ThemeTokens::light().high_contrast;
        for slot in SemanticColorSlot::ALL {
            let role = mappings.get(slot);
            assert!(matches!(
                role,
                SystemColorRole::Window
                    | SystemColorRole::WindowText
                    | SystemColorRole::ButtonFace
                    | SystemColorRole::GrayText
                    | SystemColorRole::Highlight
                    | SystemColorRole::HighlightText
                    | SystemColorRole::Hotlight
            ));
        }
        assert_eq!(
            mappings.get(SemanticColorSlot::SelectedActive),
            SystemColorRole::Highlight
        );
        assert_eq!(
            mappings.get(SemanticColorSlot::TextDisabled),
            SystemColorRole::GrayText
        );
    }

    #[test]
    fn high_contrast_palette_resolves_system_roles_without_alpha_only_states() {
        let palette = ThemeTokens::windows_high_contrast(|role| match role {
            SystemColorRole::Window | SystemColorRole::HighlightText => Rgba8::opaque(0, 0, 0),
            SystemColorRole::WindowText => Rgba8::opaque(255, 255, 255),
            SystemColorRole::ButtonFace => Rgba8::opaque(32, 32, 32),
            SystemColorRole::GrayText => Rgba8::opaque(128, 128, 128),
            SystemColorRole::Highlight => Rgba8::opaque(255, 255, 0),
            SystemColorRole::Hotlight => Rgba8::opaque(0, 255, 255),
        });
        assert_eq!(palette.colors.surface, Rgba8::opaque(0, 0, 0));
        assert_eq!(palette.colors.text_primary, Rgba8::opaque(255, 255, 255));
        assert_ne!(palette.colors.selected_active, palette.colors.surface);
        assert_ne!(palette.colors.text_disabled, palette.colors.text_primary);
        assert!(
            SemanticColorSlot::ALL
                .into_iter()
                .all(|slot| palette.colors.get(slot).alpha == u8::MAX)
        );
    }
}
