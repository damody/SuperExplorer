//! Centralized Explorer chrome icons backed by official embedded Fluent System Icons SVG data.

use gpui::{IntoElement, div, prelude::*, px, svg};

use crate::{
    UiTokens, diagnostics::icon_probe, navigation_pane::NavigationIcon,
    theme::NavigationIconPalette,
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ExplorerIcon {
    Back,
    Forward,
    Up,
    Refresh,
    New,
    Add,
    Cut,
    Copy,
    Paste,
    Rename,
    Share,
    Delete,
    Sort,
    View,
    More,
    Details,
    Search,
    Close,
    Chevron,
    ChevronDown,
    Minimize,
    Maximize,
    Restore,
    Pin,
}

impl ExplorerIcon {
    pub const ALL: [Self; 24] = [
        Self::Back,
        Self::Forward,
        Self::Up,
        Self::Refresh,
        Self::New,
        Self::Add,
        Self::Cut,
        Self::Copy,
        Self::Paste,
        Self::Rename,
        Self::Share,
        Self::Delete,
        Self::Sort,
        Self::View,
        Self::More,
        Self::Details,
        Self::Search,
        Self::Close,
        Self::Chevron,
        Self::ChevronDown,
        Self::Minimize,
        Self::Maximize,
        Self::Restore,
        Self::Pin,
    ];

    pub const fn stable_name(self) -> &'static str {
        match self {
            Self::Back => "back",
            Self::Forward => "forward",
            Self::Up => "up",
            Self::Refresh => "refresh",
            Self::New => "new",
            Self::Add => "add",
            Self::Cut => "cut",
            Self::Copy => "copy",
            Self::Paste => "paste",
            Self::Rename => "rename",
            Self::Share => "share",
            Self::Delete => "delete",
            Self::Sort => "sort",
            Self::View => "view",
            Self::More => "more",
            Self::Details => "details",
            Self::Search => "search",
            Self::Close => "close",
            Self::Chevron => "chevron",
            Self::ChevronDown => "chevron-down",
            Self::Minimize => "minimize",
            Self::Maximize => "maximize",
            Self::Restore => "restore",
            Self::Pin => "pin",
        }
    }

    pub const fn source(self) -> &'static str {
        "microsoft/fluentui-system-icons, 20px regular SVG, MIT"
    }

    pub fn asset_path(self) -> String {
        format!("fluent/{}.svg", self.stable_name())
    }
}

pub fn chrome_icon(
    region_id: impl Into<String>,
    icon: ExplorerIcon,
    tokens: UiTokens,
) -> impl IntoElement {
    let region_id = region_id.into();
    let size = tokens.layout.maximum_visible_glyph.value().min(16.0);
    div()
        .relative()
        .w(px(size))
        .h(px(size))
        .flex_none()
        .child(icon_probe(region_id))
        .child(
            svg()
                .path(icon.asset_path())
                .size_full()
                .text_color(tokens.theme.colors.text_primary.to_gpui()),
        )
}

/// Back/Forward glyph treatment matching Explorer's availability affordance.
/// Disabled history stays a single light regular glyph; enabled history gains a
/// subpixel duplicate that makes the same Fluent path only slightly heavier.
pub const NAVIGATION_HISTORY_ENABLED_EMBOLDEN_OFFSET: f32 = 0.35;

pub(crate) fn navigation_history_icon_color(
    enabled: bool,
    tokens: UiTokens,
) -> crate::theme::Rgba8 {
    if enabled {
        tokens.theme.colors.text_primary
    } else {
        tokens.theme.colors.text_disabled
    }
}

pub fn navigation_history_icon(
    region_id: impl Into<String>,
    icon: ExplorerIcon,
    enabled: bool,
    tokens: UiTokens,
) -> impl IntoElement {
    let region_id = region_id.into();
    let size = tokens.layout.maximum_visible_glyph.value().min(16.0);
    let color = navigation_history_icon_color(enabled, tokens).to_gpui();
    let glyph = move || svg().path(icon.asset_path()).size_full().text_color(color);

    div()
        .relative()
        .w(px(size))
        .h(px(size))
        .flex_none()
        .child(icon_probe(region_id))
        .child(glyph())
        .when(enabled, |element| {
            element.child(
                div()
                    .absolute()
                    .top_0()
                    .left(px(NAVIGATION_HISTORY_ENABLED_EMBOLDEN_OFFSET))
                    .size_full()
                    .child(glyph()),
            )
        })
}

/// Geometry-stable colorful fallback for Shell-owned navigation icons. The
/// 20x20 box is also the exact box used by asynchronously supplied Shell
/// bitmaps, so loading never shifts labels.
pub fn navigation_icon(icon: NavigationIcon, tokens: UiTokens) -> impl IntoElement {
    let size = tokens.layout.navigation_icon_size.value();
    let colors = NavigationIconPalette::WINDOWS_11;
    let tile = |color| {
        div()
            .w(px(size * 0.9))
            .h(px(size * 0.75))
            .rounded(px(1.5))
            .bg(color)
    };
    let art = match icon {
        NavigationIcon::Home | NavigationIcon::QuickAccess => div()
            .w(px(size * 0.85))
            .h(px(size * 0.75))
            .mt(px(2.0))
            .rounded(px(2.0))
            .bg(colors.home.to_gpui()),
        NavigationIcon::Gallery | NavigationIcon::Pictures => div()
            .w(px(size * 0.9))
            .h(px(size * 0.85))
            .rounded(px(2.0))
            .border(px(1.0))
            .border_color(colors.gallery_border.to_gpui())
            .bg(colors.gallery.to_gpui()),
        NavigationIcon::OneDrive => div()
            .relative()
            .w(px(size * 0.95))
            .h(px(size * 0.7))
            .mt(px(3.0))
            .child(
                div()
                    .absolute()
                    .left_0()
                    .bottom_0()
                    .w(px(size * 0.65))
                    .h(px(size * 0.48))
                    .rounded(px(size * 0.25))
                    .bg(colors.one_drive.to_gpui()),
            )
            .child(
                div()
                    .absolute()
                    .left(px(size * 0.25))
                    .top_0()
                    .w(px(size * 0.55))
                    .h(px(size * 0.55))
                    .rounded(px(size * 0.3))
                    .bg(colors.one_drive.to_gpui()),
            )
            .child(
                div()
                    .absolute()
                    .right_0()
                    .bottom_0()
                    .w(px(size * 0.58))
                    .h(px(size * 0.46))
                    .rounded(px(size * 0.25))
                    .bg(colors.one_drive.to_gpui()),
            ),
        NavigationIcon::Desktop | NavigationIcon::Computer | NavigationIcon::Network => div()
            .w(px(size * 0.9))
            .h(px(size * 0.7))
            .rounded(px(1.0))
            .border(px(1.0))
            .border_color(colors.computer_border.to_gpui())
            .bg(colors.computer.to_gpui()),
        NavigationIcon::Downloads => div()
            .w(px(size * 0.2))
            .h(px(size * 0.9))
            .rounded(px(2.0))
            .bg(colors.downloads.to_gpui()),
        NavigationIcon::Documents => tile(colors.documents.to_gpui()),
        NavigationIcon::Music => div()
            .w(px(size * 0.9))
            .h(px(size * 0.9))
            .rounded(px(9.0))
            .bg(colors.music.to_gpui()),
        NavigationIcon::Videos => tile(colors.videos.to_gpui()),
        NavigationIcon::Folder | NavigationIcon::Libraries => div()
            .w(px(size * 0.95))
            .h(px(size * 0.7))
            .mt(px(3.0))
            .rounded(px(1.5))
            .bg(colors.folder.to_gpui()),
        NavigationIcon::Archive => div()
            .w(px(size * 0.8))
            .h(px(size * 0.9))
            .rounded(px(1.5))
            .border(px(1.0))
            .border_color(colors.gallery_border.to_gpui())
            .bg(colors.documents.to_gpui()),
        NavigationIcon::Drive | NavigationIcon::RecycleBin => div()
            .w(px(size * 0.95))
            .h(px(size * 0.4))
            .mt(px(6.0))
            .rounded(px(2.0))
            .border(px(1.0))
            .border_color(colors.drive_border.to_gpui())
            .bg(colors.drive.to_gpui()),
    };
    div()
        .w(px(size))
        .h(px(size))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .child(art)
}

/// Explorer-like unavailable marker used when an optional navigation provider is absent.
/// It reuses the embedded Fluent Close glyph and keeps the same geometry as Shell icons.
pub fn unavailable_navigation_icon(tokens: UiTokens) -> impl IntoElement {
    let size = tokens.layout.navigation_icon_size.value();
    div()
        .w(px(size))
        .h(px(size))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(size / 2.0))
        .border(px(1.5))
        .border_color(tokens.theme.colors.danger.to_gpui())
        .child(
            svg()
                .path(ExplorerIcon::Close.asset_path())
                .size(px(size * 0.62))
                .text_color(tokens.theme.colors.danger.to_gpui()),
        )
}

#[cfg(test)]
mod tests {
    use super::{
        ExplorerIcon, NAVIGATION_HISTORY_ENABLED_EMBOLDEN_OFFSET, navigation_history_icon_color,
    };
    use crate::{UiTokens, theme::ThemeTokens};

    #[test]
    fn icon_contract_has_unique_stable_names_and_sources() {
        let mut names = std::collections::HashSet::new();
        for icon in ExplorerIcon::ALL {
            assert!(names.insert(icon.stable_name()));
            assert!(!icon.source().is_empty());
        }
    }

    #[test]
    fn enabled_navigation_history_glyph_uses_only_a_subpixel_embolden_offset() {
        assert!(NAVIGATION_HISTORY_ENABLED_EMBOLDEN_OFFSET > 0.0);
        assert!(NAVIGATION_HISTORY_ENABLED_EMBOLDEN_OFFSET < 0.5);
    }

    #[test]
    fn navigation_history_glyph_uses_primary_when_enabled_and_disabled_semantic_color_otherwise() {
        for theme in [ThemeTokens::light(), ThemeTokens::dark()] {
            let tokens = UiTokens {
                theme,
                ..UiTokens::default()
            };
            assert_eq!(
                navigation_history_icon_color(true, tokens),
                theme.colors.text_primary
            );
            assert_eq!(
                navigation_history_icon_color(false, tokens),
                theme.colors.text_disabled
            );
            assert_ne!(
                navigation_history_icon_color(true, tokens),
                navigation_history_icon_color(false, tokens)
            );
        }
    }
}
