//! Scrollable 256×256 viewer for icon and thumbnail caches.
//! Disk entries stay as paths until a tile is on screen, so a large cache does
//! not decode every file up front.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use gpui::{
    IntoElement, ObjectFit, RenderImage, ScrollHandle, SharedString, div, img, prelude::*, px,
};

use crate::{UiTokens, chrome::ActionCallback};

pub const CACHE_INSPECTOR_TILE: f32 = 256.0;
const TILE_GAP: f32 = 8.0;
const LABEL_HEIGHT: f32 = 36.0;
const CELL_WIDTH: f32 = CACHE_INSPECTOR_TILE + TILE_GAP;
const CELL_HEIGHT: f32 = CACHE_INSPECTOR_TILE + LABEL_HEIGHT + TILE_GAP;
const MAX_LISTED_ENTRIES: usize = 16_384;
const HEADER_LEN: usize = 54;
const SCHEMA_VERSION: u16 = 6;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CacheInspectorSource {
    IconMemory,
    ThumbnailMemory,
    IconDisk,
    ThumbnailDisk,
}

impl CacheInspectorSource {
    pub const fn label_key(self) -> &'static str {
        match self {
            Self::IconMemory => "settings-icon-memory",
            Self::ThumbnailMemory => "settings-thumbnail-memory",
            Self::IconDisk => "settings-icon-disk",
            Self::ThumbnailDisk => "settings-thumbnail-disk",
        }
    }

    pub const fn all() -> [Self; 4] {
        [
            Self::IconMemory,
            Self::ThumbnailMemory,
            Self::IconDisk,
            Self::ThumbnailDisk,
        ]
    }
}

#[derive(Clone, Debug)]
pub enum CacheGalleryImage {
    Texture(Arc<RenderImage>),
    Pixels(Arc<explorer_model::ThumbnailPixels>),
    Disk(PathBuf),
}

#[derive(Clone, Debug)]
pub struct CacheGalleryItem {
    pub label: String,
    pub image: CacheGalleryImage,
}

#[derive(Clone, Debug)]
pub struct CacheInspectorModel {
    pub source: CacheInspectorSource,
    pub items: Arc<Vec<CacheGalleryItem>>,
    pub truncated: bool,
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "column count is clamped to a finite positive viewport before the index cast"
)]
pub fn cache_inspector_columns(viewport_width: f32) -> usize {
    if !viewport_width.is_finite() || viewport_width <= 0.0 {
        return 1;
    }
    (viewport_width / CELL_WIDTH).floor().max(1.0) as usize
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "scroll offsets are finite and non-negative before they become row indexes"
)]
pub fn cache_inspector_visible_range(
    item_count: usize,
    columns: usize,
    scroll_y: f32,
    viewport_height: f32,
) -> std::ops::Range<usize> {
    if item_count == 0 || columns == 0 {
        return 0..0;
    }
    let scroll_y = if scroll_y.is_finite() {
        scroll_y.max(0.0)
    } else {
        0.0
    };
    let viewport_height = if viewport_height.is_finite() {
        viewport_height.max(CELL_HEIGHT)
    } else {
        CELL_HEIGHT
    };
    let first_row = (scroll_y / CELL_HEIGHT).floor() as usize;
    let visible_rows = (viewport_height / CELL_HEIGHT).ceil() as usize;
    let last_row = first_row.saturating_add(visible_rows).saturating_add(1);
    let start = first_row.saturating_mul(columns).min(item_count);
    let end = last_row
        .saturating_add(1)
        .saturating_mul(columns)
        .min(item_count);
    start..end.max(start)
}

pub fn list_disk_cache(source: CacheInspectorSource) -> (Vec<CacheGalleryItem>, bool) {
    let Some(root) = disk_cache_root(source) else {
        return (Vec::new(), false);
    };
    let Ok(entries) = fs::read_dir(&root) else {
        return (Vec::new(), false);
    };
    let mut files = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path).ok()?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return None;
            }
            path.extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("bc7cache"))
                .then_some((
                    path,
                    metadata
                        .modified()
                        .unwrap_or(std::time::SystemTime::UNIX_EPOCH),
                ))
        })
        .collect::<Vec<_>>();
    files.sort_by(|left, right| right.1.cmp(&left.1));
    let truncated = files.len() > MAX_LISTED_ENTRIES;
    let items = files
        .into_iter()
        .take(MAX_LISTED_ENTRIES)
        .map(|(path, _)| CacheGalleryItem {
            label: path.file_stem().map_or_else(
                || path.display().to_string(),
                |stem| stem.to_string_lossy().into_owned(),
            ),
            image: CacheGalleryImage::Disk(path),
        })
        .collect();
    (items, truncated)
}

pub fn disk_cache_root(source: CacheInspectorSource) -> Option<PathBuf> {
    let folder = match source {
        CacheInspectorSource::IconDisk => "icon-cache",
        CacheInspectorSource::ThumbnailDisk => "thumbnail-cache",
        CacheInspectorSource::IconMemory | CacheInspectorSource::ThumbnailMemory => return None,
    };
    let base = std::env::var_os("LOCALAPPDATA").map_or_else(std::env::temp_dir, PathBuf::from);
    Some(base.join("RustGpuiExplorer").join(folder).join("v1"))
}

pub(crate) fn load_disk_texture(path: &Path) -> Option<Arc<RenderImage>> {
    let bytes = fs::read(path).ok()?;
    let raster = parse_bc7_cache(&bytes)?;
    crate::bc7_texture(&raster)
}

fn parse_bc7_cache(bytes: &[u8]) -> Option<explorer_model::Bc7RasterPayload> {
    if bytes.len() < HEADER_LEN || &bytes[..8] != b"RGXBC7C1" {
        return None;
    }
    let schema = u16::from_le_bytes(bytes[8..10].try_into().ok()?);
    if schema != SCHEMA_VERSION || bytes[10] != 1 || bytes[12] != 7 || bytes[13] != 0 {
        return None;
    }
    let kind = match bytes[11] {
        1 => explorer_model::CompressedRasterKind::Icon,
        2 => explorer_model::CompressedRasterKind::Thumbnail,
        _ => return None,
    };
    let width = u32::from_le_bytes(bytes[22..26].try_into().ok()?);
    let height = u32::from_le_bytes(bytes[26..30].try_into().ok()?);
    let padded_width = u32::from_le_bytes(bytes[30..34].try_into().ok()?);
    let padded_height = u32::from_le_bytes(bytes[34..38].try_into().ok()?);
    let row_pitch = u32::from_le_bytes(bytes[38..42].try_into().ok()?);
    let payload_len = u64::from_le_bytes(bytes[42..50].try_into().ok()?);
    let payload_len = usize::try_from(payload_len).ok()?;
    if payload_len != bytes.len().saturating_sub(HEADER_LEN) {
        return None;
    }
    let raster = explorer_model::Bc7RasterPayload {
        kind,
        width,
        height,
        padded_width,
        padded_height,
        row_pitch,
        blocks: bytes[HEADER_LEN..].to_vec(),
    };
    raster.validate(16 * 1024 * 1024).then_some(raster)
}

#[allow(
    clippy::too_many_arguments,
    clippy::implicit_hasher,
    clippy::cast_precision_loss,
    reason = "the overlay keeps scroll, decode cache, and actions explicit; tile positions stay in a practical range"
)]
pub fn cache_inspector_overlay(
    tokens: &UiTokens,
    catalog: explorer_i18n::Catalog,
    inspector: &CacheInspectorModel,
    scroll: &ScrollHandle,
    viewport_width: f32,
    viewport_height: f32,
    decoded: &mut HashMap<usize, Arc<RenderImage>>,
    on_action: &Option<ActionCallback>,
) -> impl IntoElement + use<> {
    let selected = inspector.source;
    let truncated = inspector.truncated;
    let items = Arc::clone(&inspector.items);
    let colors = tokens.theme.colors;
    let columns = cache_inspector_columns((viewport_width - 24.0).max(CELL_WIDTH));
    let scroll_y = (-f32::from(scroll.offset().y)).max(0.0);
    let measured_height = f32::from(scroll.bounds().size.height);
    let viewport = if measured_height > 1.0 {
        measured_height
    } else {
        (viewport_height - 120.0).max(CELL_HEIGHT)
    };
    let visible = cache_inspector_visible_range(items.len(), columns, scroll_y, viewport);
    decoded.retain(|index, _| visible.contains(index));
    for index in visible.clone() {
        if decoded.contains_key(&index) {
            continue;
        }
        let Some(item) = items.get(index) else {
            continue;
        };
        let texture = match &item.image {
            CacheGalleryImage::Texture(texture) => Some(Arc::clone(texture)),
            CacheGalleryImage::Pixels(pixels) => crate::thumbnail_texture(pixels),
            CacheGalleryImage::Disk(path) => load_disk_texture(path),
        };
        if let Some(texture) = texture {
            decoded.insert(index, texture);
        }
    }
    let rows = items.len().div_ceil(columns.max(1));
    let content_height = (rows as f32 * CELL_HEIGHT).max(viewport);
    let mut count_args = explorer_i18n::FluentArgs::new();
    count_args.set("count", i64::try_from(items.len()).unwrap_or(i64::MAX));
    let count_label = if truncated {
        catalog.t_args("settings-cache-inspector-truncated", &count_args)
    } else {
        catalog.t_args("settings-cache-inspector-count", &count_args)
    };
    let tiles = visible
        .filter_map(|index| {
            let item = items.get(index)?;
            let row = index / columns;
            let column = index % columns;
            let texture = decoded.get(&index).cloned();
            let label = item.label.clone();
            Some(
                div()
                    .absolute()
                    .left(px(column as f32 * CELL_WIDTH))
                    .top(px(row as f32 * CELL_HEIGHT))
                    .w(px(CACHE_INSPECTOR_TILE))
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .id(SharedString::from(format!("cache-inspector-tile-{index}")))
                            .w(px(CACHE_INSPECTOR_TILE))
                            .h(px(CACHE_INSPECTOR_TILE))
                            .flex()
                            .items_center()
                            .justify_center()
                            .overflow_hidden()
                            .rounded(px(tokens.layout.corner_radius.value()))
                            .bg(colors.control_fill.to_gpui())
                            .border(px(1.0))
                            .border_color(colors.divider.to_gpui())
                            .when_some(texture, |tile, texture| {
                                tile.child(
                                    img(texture)
                                        .w(px(CACHE_INSPECTOR_TILE))
                                        .h(px(CACHE_INSPECTOR_TILE))
                                        .object_fit(ObjectFit::Contain),
                                )
                            }),
                    )
                    .child(
                        div()
                            .h(px(LABEL_HEIGHT - 4.0))
                            .text_size(px(tokens.typography.tooltip.size.value()))
                            .text_color(colors.text_secondary.to_gpui())
                            .overflow_hidden()
                            .child(label),
                    ),
            )
        })
        .collect::<Vec<_>>();
    div()
        .id("cache-inspector")
        .absolute()
        .inset_0()
        .occlude()
        .flex()
        .flex_col()
        .bg(colors.surface.to_gpui())
        .child(
            div()
                .flex_none()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(8.0))
                .px(px(12.0))
                .h(px(tokens.layout.address_bar_height.value()))
                .border_b(px(1.0))
                .border_color(colors.divider.to_gpui())
                .child(catalog.t("settings-cache-inspector"))
                .child(count_label)
                .child(source_button(
                    "cache-inspector-close",
                    catalog.t("settings-cache-inspector-close"),
                    crate::actions::ExplorerAction::CloseCacheInspector,
                    tokens,
                    on_action.clone(),
                    true,
                )),
        )
        .child(
            div()
                .flex_none()
                .flex()
                .flex_wrap()
                .gap(px(8.0))
                .px(px(12.0))
                .py(px(8.0))
                .children(CacheInspectorSource::all().map(|source| {
                    source_button(
                        match source {
                            CacheInspectorSource::IconMemory => "cache-inspector-icon-memory",
                            CacheInspectorSource::ThumbnailMemory => {
                                "cache-inspector-thumbnail-memory"
                            }
                            CacheInspectorSource::IconDisk => "cache-inspector-icon-disk",
                            CacheInspectorSource::ThumbnailDisk => "cache-inspector-thumbnail-disk",
                        },
                        catalog.t(source.label_key()),
                        crate::actions::ExplorerAction::SetCacheInspectorSource(source),
                        tokens,
                        on_action.clone(),
                        source == selected,
                    )
                })),
        )
        .child(if items.is_empty() {
            div()
                .id("cache-inspector-empty")
                .flex_1()
                .p(px(16.0))
                .child(catalog.t("settings-cache-inspector-empty"))
                .into_any_element()
        } else {
            div()
                .id("cache-inspector-scroll")
                .flex_1()
                .overflow_y_scroll()
                .track_scroll(scroll)
                .on_scroll_wheel(|_, _, cx| cx.refresh_windows())
                .px(px(12.0))
                .child(
                    div()
                        .relative()
                        .w_full()
                        .h(px(content_height))
                        .children(tiles),
                )
                .into_any_element()
        })
}

fn source_button(
    id: &'static str,
    label: impl Into<SharedString>,
    action: crate::actions::ExplorerAction,
    tokens: &UiTokens,
    on_action: Option<ActionCallback>,
    selected: bool,
) -> impl IntoElement {
    let label = label.into();
    let colors = tokens.theme.colors;
    div()
        .id(id)
        .role(gpui::Role::Button)
        .aria_label(label.clone())
        .h(px(tokens.layout.minimum_hit_target.value()))
        .px(px(10.0))
        .flex()
        .items_center()
        .rounded(px(tokens.layout.corner_radius.value()))
        .border(px(1.0))
        .border_color(if selected {
            colors.accent.to_gpui()
        } else {
            colors.divider.to_gpui()
        })
        .bg(if selected {
            colors.control_hover.to_gpui()
        } else {
            colors.surface.to_gpui()
        })
        .when_some(on_action, |button, callback| {
            button.on_click(move |_, window, cx| callback(&action, window, cx))
        })
        .child(label)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_range_stays_inside_a_large_gallery() {
        let range = cache_inspector_visible_range(10_000, 4, 256.0 * 3.0, 256.0 * 2.0);
        assert!(range.start >= 8);
        assert!(range.end > range.start);
        assert!(range.end <= 10_000);
        assert!(range.end - range.start < 40);
        assert_eq!(cache_inspector_columns(256.0), 1);
        assert_eq!(cache_inspector_columns(800.0), 3);
    }

    #[test]
    fn bc7_cache_header_rejects_a_truncated_file_and_accepts_a_4x4_block() {
        assert!(parse_bc7_cache(b"RGXBC7C1").is_none());
        let mut bytes = vec![0_u8; HEADER_LEN + 16];
        bytes[..8].copy_from_slice(b"RGXBC7C1");
        bytes[8..10].copy_from_slice(&SCHEMA_VERSION.to_le_bytes());
        bytes[10] = 1;
        bytes[11] = 2;
        bytes[12] = 7;
        bytes[22..26].copy_from_slice(&4_u32.to_le_bytes());
        bytes[26..30].copy_from_slice(&4_u32.to_le_bytes());
        bytes[30..34].copy_from_slice(&4_u32.to_le_bytes());
        bytes[34..38].copy_from_slice(&4_u32.to_le_bytes());
        bytes[38..42].copy_from_slice(&16_u32.to_le_bytes());
        bytes[42..50].copy_from_slice(&16_u64.to_le_bytes());
        let raster = parse_bc7_cache(&bytes).expect("4x4 block");
        assert_eq!(raster.width, 4);
        assert_eq!(raster.blocks.len(), 16);
        assert_eq!(raster.kind, explorer_model::CompressedRasterKind::Thumbnail);
    }
}
