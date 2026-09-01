//! Centralized Explorer chrome icons backed by official embedded Fluent System Icons SVG data.

use gpui::{IntoElement, div, prelude::*, px, svg};

use crate::{
    UiTokens,
    diagnostics::icon_probe,
    navigation_pane::NavigationIcon,
    theme::{NavigationIconPalette, Rgba8},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RemoteFileIconSpec {
    pub glyph_name: &'static str,
    pub accessible_label: &'static str,
    pub accent: Rgba8,
    pub monochrome: bool,
}

impl RemoteFileIconSpec {
    pub fn asset_path(self) -> String {
        format!("remote-file/{}.svg", self.glyph_name)
    }
}

pub const fn remote_file_icon_spec(kind: explorer_model::RemoteFileIconKind) -> RemoteFileIconSpec {
    use explorer_model::RemoteFileIconKind as Kind;
    let mut spec = match kind {
        Kind::Generic => RemoteFileIconSpec {
            glyph_name: "generic",
            accessible_label: "file",
            accent: Rgba8::opaque(116, 151, 184),
            monochrome: false,
        },
        Kind::Pdf => RemoteFileIconSpec {
            glyph_name: "pdf",
            accessible_label: "PDF file",
            accent: Rgba8::opaque(218, 55, 62),
            monochrome: true,
        },
        Kind::Text => RemoteFileIconSpec {
            glyph_name: "text",
            accessible_label: "text file",
            accent: Rgba8::opaque(48, 126, 190),
            monochrome: false,
        },
        Kind::Settings => RemoteFileIconSpec {
            glyph_name: "settings",
            accessible_label: "settings file",
            accent: Rgba8::opaque(88, 104, 122),
            monochrome: false,
        },
        Kind::Image => RemoteFileIconSpec {
            glyph_name: "image",
            accessible_label: "image file",
            accent: Rgba8::opaque(26, 148, 104),
            monochrome: false,
        },
        Kind::Archive => RemoteFileIconSpec {
            glyph_name: "archive",
            accessible_label: "archive file",
            accent: Rgba8::opaque(181, 129, 5),
            monochrome: true,
        },
        Kind::Audio => RemoteFileIconSpec {
            glyph_name: "audio",
            accessible_label: "audio file",
            accent: Rgba8::opaque(190, 72, 132),
            monochrome: false,
        },
        Kind::Video => RemoteFileIconSpec {
            glyph_name: "video",
            accessible_label: "video file",
            accent: Rgba8::opaque(116, 77, 169),
            monochrome: false,
        },
        Kind::Code => RemoteFileIconSpec {
            glyph_name: "code",
            accessible_label: "code file",
            accent: Rgba8::opaque(47, 111, 117),
            monochrome: false,
        },
        Kind::Script => RemoteFileIconSpec {
            glyph_name: "script",
            accessible_label: "script file",
            accent: Rgba8::opaque(45, 125, 91),
            monochrome: true,
        },
        Kind::Executable => RemoteFileIconSpec {
            glyph_name: "executable",
            accessible_label: "executable or binary file",
            accent: Rgba8::opaque(91, 96, 105),
            monochrome: false,
        },
        Kind::AndroidPackage => RemoteFileIconSpec {
            glyph_name: "android",
            accessible_label: "Android package",
            accent: Rgba8::opaque(61, 220, 132),
            monochrome: false,
        },
        Kind::Word => RemoteFileIconSpec {
            glyph_name: "word",
            accessible_label: "word-processing document",
            accent: Rgba8::opaque(42, 94, 171),
            monochrome: true,
        },
        Kind::Spreadsheet => RemoteFileIconSpec {
            glyph_name: "spreadsheet",
            accessible_label: "spreadsheet",
            accent: Rgba8::opaque(33, 115, 70),
            monochrome: false,
        },
        Kind::Presentation => RemoteFileIconSpec {
            glyph_name: "presentation",
            accessible_label: "presentation",
            accent: Rgba8::opaque(210, 71, 38),
            monochrome: true,
        },
        Kind::Notebook => RemoteFileIconSpec {
            glyph_name: "notebook",
            accessible_label: "notebook",
            accent: Rgba8::opaque(119, 61, 126),
            monochrome: false,
        },
        Kind::Database => RemoteFileIconSpec {
            glyph_name: "database",
            accessible_label: "database",
            accent: Rgba8::opaque(156, 100, 37),
            monochrome: false,
        },
        Kind::Mail => RemoteFileIconSpec {
            glyph_name: "mail",
            accessible_label: "mail data file",
            accent: Rgba8::opaque(30, 111, 180),
            monochrome: false,
        },
        Kind::Font => RemoteFileIconSpec {
            glyph_name: "font",
            accessible_label: "font file",
            accent: Rgba8::opaque(74, 74, 74),
            monochrome: true,
        },
        Kind::Certificate => RemoteFileIconSpec {
            glyph_name: "certificate",
            accessible_label: "certificate or key file",
            accent: Rgba8::opaque(186, 124, 14),
            monochrome: false,
        },
        Kind::DiskImage => RemoteFileIconSpec {
            glyph_name: "disk-image",
            accessible_label: "disk image",
            accent: Rgba8::opaque(83, 101, 118),
            monochrome: true,
        },
        Kind::Web => RemoteFileIconSpec {
            glyph_name: "web",
            accessible_label: "web file",
            accent: Rgba8::opaque(0, 120, 212),
            monochrome: false,
        },
        Kind::Data => RemoteFileIconSpec {
            glyph_name: "data",
            accessible_label: "data file",
            accent: Rgba8::opaque(92, 72, 169),
            monochrome: false,
        },
        Kind::Markup => RemoteFileIconSpec {
            glyph_name: "markup",
            accessible_label: "markup file",
            accent: Rgba8::opaque(44, 136, 153),
            monochrome: false,
        },
    };
    // GPUI's SVG path supports the official Filled subset through currentColor.
    spec.monochrome = true;
    spec
}

/// Scalable, dependency-free file tile used only when an ADB/SFTP item has no bitmap.
pub fn remote_file_icon(
    kind: explorer_model::RemoteFileIconKind,
    size: f32,
    _tokens: UiTokens,
) -> impl IntoElement {
    let spec = remote_file_icon_spec(kind);
    let glyph_size = size * 0.94;
    let glyph = svg()
        .path(spec.asset_path())
        .size(px(glyph_size))
        .text_color(spec.accent.to_gpui());
    div()
        .w(px(size))
        .h(px(size))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .child(glyph)
}

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

pub(crate) fn navigation_history_icon_color(enabled: bool, tokens: UiTokens) -> Rgba8 {
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
        NavigationIcon::Phone => div()
            .w(px(size * 0.58))
            .h(px(size * 0.92))
            .rounded(px(3.0))
            .border(px(1.5))
            .border_color(colors.computer_border.to_gpui())
            .bg(colors.computer.to_gpui()),
        NavigationIcon::Server => div()
            .w(px(size * 0.9))
            .h(px(size * 0.72))
            .rounded(px(2.0))
            .border(px(1.0))
            .border_color(colors.drive_border.to_gpui())
            .bg(colors.drive.to_gpui()),
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
        remote_file_icon_spec,
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

    #[test]
    fn every_remote_file_icon_kind_has_stable_accessible_visual_metadata() {
        use explorer_model::RemoteFileIconKind as Kind;
        let kinds = [
            Kind::Generic,
            Kind::Pdf,
            Kind::Text,
            Kind::Settings,
            Kind::Image,
            Kind::Archive,
            Kind::Audio,
            Kind::Video,
            Kind::Code,
            Kind::Script,
            Kind::Executable,
            Kind::AndroidPackage,
            Kind::Word,
            Kind::Spreadsheet,
            Kind::Presentation,
            Kind::Notebook,
            Kind::Database,
            Kind::Mail,
            Kind::Font,
            Kind::Certificate,
            Kind::DiskImage,
            Kind::Web,
            Kind::Data,
            Kind::Markup,
        ];
        let mut labels = std::collections::HashSet::new();
        let mut accents = std::collections::HashSet::new();
        let mut glyphs = std::collections::HashSet::new();
        for kind in kinds {
            let spec = remote_file_icon_spec(kind);
            assert!(labels.insert(spec.accessible_label));
            assert!(
                spec.monochrome,
                "all GPUI-compatible Filled assets are tintable"
            );
            assert!(accents.insert((spec.accent.red, spec.accent.green, spec.accent.blue)));
            assert!(glyphs.insert(spec.glyph_name));
        }
    }

    #[test]
    fn remote_file_icon_geometry_scales_with_small_and_large_hosts() {
        for size in [16.0_f32, 20.0, 64.0, 256.0, 512.0] {
            let glyph_size = size * 0.94;
            assert!(glyph_size > 0.0 && glyph_size <= size);
            assert!(
                glyph_size >= size - 31.0,
                "small icons must use nearly the full host"
            );
        }
    }
}
