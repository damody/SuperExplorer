//! Deterministic window geometry used by rendering and regression tests.

use crate::layout::{LayoutTokens, LogicalPx};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LogicalRect {
    pub x: LogicalPx,
    pub y: LogicalPx,
    pub width: LogicalPx,
    pub height: LogicalPx,
}

impl LogicalRect {
    const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x: LogicalPx::new(x),
            y: LogicalPx::new(y),
            width: LogicalPx::new(width),
            height: LogicalPx::new(height),
        }
    }

    pub fn bottom(self) -> f32 {
        self.y.value() + self.height.value()
    }

    pub fn right(self) -> f32 {
        self.x.value() + self.width.value()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowGeometry {
    pub window: LogicalRect,
    pub window_chrome: LogicalRect,
    pub command_bar: LogicalRect,
    pub navigation_bar: LogicalRect,
    pub navigation_pane: LogicalRect,
    pub divider: LogicalRect,
    pub file_view: LogicalRect,
    pub status_bar: LogicalRect,
    pub compact_commands: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GeometryError {
    InvalidWindowDimension,
}

impl WindowGeometry {
    /// Computes ordered, non-overlapping regions and never emits a negative size.
    ///
    /// # Errors
    ///
    /// Returns [`GeometryError::InvalidWindowDimension`] for non-finite or
    /// negative window dimensions.
    pub fn calculate(
        width: f32,
        height: f32,
        requested_pane_width: LogicalPx,
        tokens: LayoutTokens,
    ) -> Result<Self, GeometryError> {
        if !width.is_finite() || !height.is_finite() || width < 0.0 || height < 0.0 {
            return Err(GeometryError::InvalidWindowDimension);
        }

        let title_height = tokens.title_tab_height.value().min(height);
        let navigation_y = title_height;
        let navigation_height = tokens
            .address_bar_height
            .value()
            .min((height - navigation_y).max(0.0));
        let command_y = navigation_y + navigation_height;
        let command_height = tokens
            .command_bar_height
            .value()
            .min((height - command_y).max(0.0));
        let status_height = tokens.status_bar_height.value().min(height);
        let content_y = command_y + command_height;
        let content_bottom = (height - status_height).max(content_y);
        let content_height = (content_bottom - content_y).max(0.0);

        let pane_width = clamp_pane_width(requested_pane_width, tokens).min(width);
        let divider_width = tokens
            .divider_width
            .value()
            .min((width - pane_width).max(0.0));
        let file_width = (width - pane_width - divider_width).max(0.0);

        Ok(Self {
            window: LogicalRect::new(0.0, 0.0, width, height),
            window_chrome: LogicalRect::new(0.0, 0.0, width, title_height),
            command_bar: LogicalRect::new(0.0, command_y, width, command_height),
            navigation_bar: LogicalRect::new(0.0, navigation_y, width, navigation_height),
            navigation_pane: LogicalRect::new(0.0, content_y, pane_width, content_height),
            divider: LogicalRect::new(pane_width, content_y, divider_width, content_height),
            file_view: LogicalRect::new(
                pane_width + divider_width,
                content_y,
                file_width,
                content_height,
            ),
            status_bar: LogicalRect::new(
                0.0,
                (height - status_height).max(0.0),
                width,
                status_height,
            ),
            compact_commands: width < tokens.compact_window_width.value(),
        })
    }
}

pub fn clamp_pane_width(requested: LogicalPx, tokens: LayoutTokens) -> f32 {
    let requested = requested.value();
    if !requested.is_finite() {
        return tokens.navigation_pane_default_width.value();
    }
    requested.clamp(
        tokens.navigation_pane_min_width.value(),
        tokens.navigation_pane_max_width.value(),
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::float_cmp)] // Geometry boundaries intentionally require exact token sums.
    use super::{GeometryError, WindowGeometry, clamp_pane_width};
    use crate::layout::{LayoutTokens, LogicalPx};

    #[test]
    fn baseline_regions_are_ordered_and_non_overlapping() {
        let tokens = LayoutTokens::WINDOWS_11;
        let geometry =
            WindowGeometry::calculate(1_120.0, 720.0, tokens.navigation_pane_default_width, tokens)
                .expect("valid geometry");
        assert_eq!(
            geometry.window_chrome.bottom(),
            geometry.navigation_bar.y.value()
        );
        assert_eq!(
            geometry.navigation_bar.bottom(),
            geometry.command_bar.y.value()
        );
        assert_eq!(geometry.command_bar.bottom(), geometry.file_view.y.value());
        assert!(geometry.file_view.bottom() <= geometry.status_bar.y.value());
        assert_eq!(geometry.navigation_pane.right(), geometry.divider.x.value());
        assert_eq!(geometry.divider.right(), geometry.file_view.x.value());
        assert_eq!(geometry.file_view.right(), geometry.window.width.value());
        assert!(!geometry.compact_commands);
    }

    #[test]
    fn tiny_finite_windows_use_compact_zero_safe_geometry() {
        let tokens = LayoutTokens::WINDOWS_11;
        let geometry = WindowGeometry::calculate(40.0, 30.0, LogicalPx::new(999.0), tokens)
            .expect("tiny windows remain valid");
        assert!(geometry.compact_commands);
        for rect in [
            geometry.window_chrome,
            geometry.command_bar,
            geometry.navigation_bar,
            geometry.navigation_pane,
            geometry.divider,
            geometry.file_view,
            geometry.status_bar,
        ] {
            assert!(rect.width.value() >= 0.0);
            assert!(rect.height.value() >= 0.0);
        }
        assert_eq!(
            WindowGeometry::calculate(f32::NAN, 30.0, LogicalPx::new(200.0), tokens),
            Err(GeometryError::InvalidWindowDimension)
        );
    }

    #[test]
    fn pane_clamp_rejects_extremes_and_non_finite_values() {
        let tokens = LayoutTokens::WINDOWS_11;
        assert_eq!(
            clamp_pane_width(LogicalPx::new(-1.0), tokens),
            tokens.navigation_pane_min_width.value()
        );
        assert_eq!(
            clamp_pane_width(LogicalPx::new(10_000.0), tokens),
            tokens.navigation_pane_max_width.value()
        );
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(
                clamp_pane_width(LogicalPx::new(invalid), tokens),
                tokens.navigation_pane_default_width.value()
            );
        }
    }

    #[test]
    fn maximize_restore_recalculation_preserves_pane_and_fixed_chrome() {
        let tokens = LayoutTokens::WINDOWS_11;
        let pane = LogicalPx::new(300.0);
        let restored = WindowGeometry::calculate(1_120.0, 720.0, pane, tokens).expect("restored");
        let maximized = WindowGeometry::calculate(1_920.0, 1_080.0, pane, tokens).expect("max");
        assert_eq!(
            restored.navigation_pane.width,
            maximized.navigation_pane.width
        );
        assert_eq!(
            restored.window_chrome.height,
            maximized.window_chrome.height
        );
        assert_eq!(restored.command_bar.height, maximized.command_bar.height);
        assert_eq!(
            restored.navigation_bar.height,
            maximized.navigation_bar.height
        );
        assert_eq!(restored.status_bar.height, maximized.status_bar.height);
    }
}
