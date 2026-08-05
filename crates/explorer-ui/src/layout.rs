//! Logical layout values that are scaled exactly once by the GPUI/Windows boundary.

/// A device-independent logical pixel value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LogicalPx(f32);

impl LogicalPx {
    pub const fn new(value: f32) -> Self {
        Self(value)
    }

    pub const fn value(self) -> f32 {
        self.0
    }

    pub fn to_physical(self, scale: DpiScale) -> PhysicalPx {
        PhysicalPx(self.0 * scale.factor())
    }
}

/// A validated Windows DPI scale factor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DpiScale(f32);

impl DpiScale {
    /// Converts a Windows scale percentage such as 125 into a factor of 1.25.
    pub fn from_percent(percent: u16) -> Option<Self> {
        (percent > 0).then(|| Self(f32::from(percent) / 100.0))
    }

    pub const fn factor(self) -> f32 {
        self.0
    }
}

/// A physical pixel value. It deliberately has no second scaling operation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhysicalPx(f32);

impl PhysicalPx {
    pub const fn value(self) -> f32 {
        self.0
    }
}

pub const EXPLORER_SEARCH_WINDOW_RATIO: f32 = 0.235;

/// Folder Options uses a dialog-specific geometry profile instead of scattering literals
/// through feature rendering code.
pub mod folder_options {
    use super::LogicalPx;

    pub const DIALOG_WIDTH: LogicalPx = LogicalPx::new(730.0);
    pub const DIALOG_HEIGHT: LogicalPx = LogicalPx::new(610.0);
    pub const PAGE_PADDING: LogicalPx = LogicalPx::new(28.0);
    pub const FOOTER_HEIGHT: LogicalPx = LogicalPx::new(64.0);
    pub const TAB_MIN_WIDTH: LogicalPx = LogicalPx::new(84.0);
    pub const BUTTON_MIN_WIDTH: LogicalPx = LogicalPx::new(112.0);
}

/// Geometry used by the locked-file recovery dialog.
pub mod lock_recovery {
    use super::LogicalPx;

    pub const DIALOG_MAX_HEIGHT: LogicalPx = LogicalPx::new(560.0);
    pub const OWNER_LIST_MAX_HEIGHT: LogicalPx = LogicalPx::new(240.0);
}

/// Feature-specific geometry shared by the corresponding render paths.
pub mod feature {
    use super::LogicalPx;

    pub const PREVIEW_IMAGE_MIN_HEIGHT: LogicalPx = LogicalPx::new(96.0);
    pub const NEW_MENU_MAX_HEIGHT: LogicalPx = LogicalPx::new(520.0);
    pub const THIS_PC_TILE_WIDTH: LogicalPx = LogicalPx::new(260.0);
    pub const THIS_PC_TILE_HEIGHT: LogicalPx = LogicalPx::new(72.0);
    pub const THIS_PC_TILE_ICON_SIZE: LogicalPx = LogicalPx::new(40.0);
    pub const THIS_PC_DRIVE_STATUS_WIDTH: LogicalPx = LogicalPx::new(184.0);
    pub const THIS_PC_CAPACITY_BAR_WIDTH: LogicalPx = LogicalPx::new(184.0);
    pub const THIS_PC_CAPACITY_BAR_HEIGHT: LogicalPx = LogicalPx::new(12.0);
    pub const THIS_PC_CONTENT_HEIGHT: LogicalPx = LogicalPx::new(76.0);
    pub const THIS_PC_CONTENT_BAR_WIDTH: LogicalPx = LogicalPx::new(520.0);
    pub const THIS_PC_CONTENT_TRAILING_WIDTH: LogicalPx = LogicalPx::new(180.0);
    pub const THIS_PC_DETAILS_NAME_WIDTH: LogicalPx = LogicalPx::new(280.0);
    pub const THIS_PC_DETAILS_TYPE_WIDTH: LogicalPx = LogicalPx::new(200.0);
    pub const THIS_PC_DETAILS_TOTAL_WIDTH: LogicalPx = LogicalPx::new(140.0);
    pub const THIS_PC_DETAILS_FREE_WIDTH: LogicalPx = LogicalPx::new(140.0);
    pub const CONTENT_ROW_HEIGHT: LogicalPx = LogicalPx::new(48.0);
    pub const CONTENT_ICON_SIZE: LogicalPx = LogicalPx::new(32.0);
    pub const CONTENT_ROW_DIVIDER_HEIGHT: LogicalPx = LogicalPx::new(1.0);
    /// Explorer icon tiles keep the thumbnail and filename in separate layout regions.
    /// Three file-name lines fit here. Normal items use two lines; Explorer-style selected
    /// items may reveal one additional line without changing the row-major grid geometry.
    pub const STACKED_ICON_LABEL_HEIGHT: LogicalPx = LogicalPx::new(48.0);
    pub const STACKED_ICON_LABEL_GAP: LogicalPx = LogicalPx::new(8.0);
    pub const DETAILS_COLUMN_MENU_WIDTH: LogicalPx = LogicalPx::new(310.0);
    pub const DETAILS_COLUMN_MENU_PADDING: LogicalPx = LogicalPx::new(6.0);
    pub const DETAILS_COLUMN_SEPARATOR_HEIGHT: LogicalPx = LogicalPx::new(1.0);
    pub const DETAILS_COLUMN_SEPARATOR_MARGIN: LogicalPx = LogicalPx::new(4.0);
    pub const DETAILS_COLUMN_ROW_PADDING: LogicalPx = LogicalPx::new(10.0);
    pub const DETAILS_COLUMN_ROW_GAP: LogicalPx = LogicalPx::new(10.0);
}

/// Complete M1 Explorer geometry contract.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutTokens {
    pub reference_profile: &'static str,
    pub title_tab_height: LogicalPx,
    pub command_bar_height: LogicalPx,
    pub address_bar_height: LogicalPx,
    pub status_bar_height: LogicalPx,
    pub navigation_pane_min_width: LogicalPx,
    pub navigation_pane_default_width: LogicalPx,
    pub navigation_pane_max_width: LogicalPx,
    pub side_pane_min_width: LogicalPx,
    pub side_pane_default_width: LogicalPx,
    pub side_pane_max_width: LogicalPx,
    pub content_spacing: LogicalPx,
    pub control_padding_horizontal: LogicalPx,
    pub control_padding_vertical: LogicalPx,
    pub corner_radius: LogicalPx,
    pub focus_stroke: LogicalPx,
    pub divider_width: LogicalPx,
    pub compact_window_width: LogicalPx,
    pub divider_keyboard_step: LogicalPx,
    pub minimum_hit_target: LogicalPx,
    pub maximum_visible_glyph: LogicalPx,
    pub navigation_button_width: LogicalPx,
    pub caption_button_width: LogicalPx,
    pub address_min_width: LogicalPx,
    pub search_box_width: LogicalPx,
    pub compact_address_min_width: LogicalPx,
    pub compact_search_box_width: LogicalPx,
    pub address_search_gap: LogicalPx,
    pub navigation_row_height: LogicalPx,
    pub navigation_pane_vertical_padding: LogicalPx,
    pub navigation_separator_height: LogicalPx,
    pub navigation_icon_size: LogicalPx,
    pub details_header_height: LogicalPx,
    pub file_row_height: LogicalPx,
    pub inline_rename_height: LogicalPx,
    pub details_name_width: LogicalPx,
    pub details_modified_width: LogicalPx,
    pub details_type_width: LogicalPx,
    pub details_size_width: LogicalPx,
    pub menu_row_height: LogicalPx,
    pub menu_max_height: LogicalPx,
    pub animation_duration_ms: u16,
}

impl LayoutTokens {
    pub const WINDOWS_11: Self = Self {
        reference_profile: "windows-11-26200-explorer-26100.8875-zh-tw-175",
        title_tab_height: LogicalPx::new(40.0),
        command_bar_height: LogicalPx::new(48.0),
        address_bar_height: LogicalPx::new(48.0),
        status_bar_height: LogicalPx::new(18.0),
        navigation_pane_min_width: LogicalPx::new(180.0),
        navigation_pane_default_width: LogicalPx::new(293.0),
        navigation_pane_max_width: LogicalPx::new(440.0),
        side_pane_min_width: LogicalPx::new(180.0),
        side_pane_default_width: LogicalPx::new(293.0),
        side_pane_max_width: LogicalPx::new(520.0),
        content_spacing: LogicalPx::new(8.0),
        control_padding_horizontal: LogicalPx::new(12.0),
        control_padding_vertical: LogicalPx::new(8.0),
        corner_radius: LogicalPx::new(8.0),
        focus_stroke: LogicalPx::new(2.0),
        divider_width: LogicalPx::new(8.0),
        compact_window_width: LogicalPx::new(800.0),
        divider_keyboard_step: LogicalPx::new(16.0),
        minimum_hit_target: LogicalPx::new(32.0),
        maximum_visible_glyph: LogicalPx::new(20.0),
        navigation_button_width: LogicalPx::new(32.0),
        caption_button_width: LogicalPx::new(46.0),
        address_min_width: LogicalPx::new(280.0),
        search_box_width: LogicalPx::new(384.0),
        compact_address_min_width: LogicalPx::new(160.0),
        compact_search_box_width: LogicalPx::new(120.0),
        address_search_gap: LogicalPx::new(8.0),
        navigation_row_height: LogicalPx::new(32.0),
        navigation_pane_vertical_padding: LogicalPx::new(7.0),
        navigation_separator_height: LogicalPx::new(17.0),
        navigation_icon_size: LogicalPx::new(20.0),
        details_header_height: LogicalPx::new(28.0),
        file_row_height: LogicalPx::new(32.0),
        inline_rename_height: LogicalPx::new(24.0),
        details_name_width: LogicalPx::new(280.0),
        details_modified_width: LogicalPx::new(150.0),
        details_type_width: LogicalPx::new(115.0),
        details_size_width: LogicalPx::new(90.0),
        menu_row_height: LogicalPx::new(32.0),
        menu_max_height: LogicalPx::new(360.0),
        animation_duration_ms: 120,
    };

    /// Checks ordering, finite dimensions, and hit-target geometry.
    ///
    /// # Errors
    ///
    /// Returns the first violated layout invariant.
    pub fn validate(self) -> Result<(), LayoutValidationError> {
        let dimensions = [
            self.title_tab_height,
            self.command_bar_height,
            self.address_bar_height,
            self.status_bar_height,
            self.navigation_pane_min_width,
            self.navigation_pane_default_width,
            self.navigation_pane_max_width,
            self.side_pane_min_width,
            self.side_pane_default_width,
            self.side_pane_max_width,
            self.content_spacing,
            self.control_padding_horizontal,
            self.control_padding_vertical,
            self.corner_radius,
            self.focus_stroke,
            self.divider_width,
            self.compact_window_width,
            self.divider_keyboard_step,
            self.minimum_hit_target,
            self.maximum_visible_glyph,
            self.navigation_button_width,
            self.caption_button_width,
            self.address_min_width,
            self.search_box_width,
            self.compact_address_min_width,
            self.compact_search_box_width,
            self.address_search_gap,
            self.navigation_row_height,
            self.navigation_pane_vertical_padding,
            self.navigation_separator_height,
            self.navigation_icon_size,
            self.details_header_height,
            self.file_row_height,
            self.inline_rename_height,
            self.details_name_width,
            self.details_modified_width,
            self.details_type_width,
            self.details_size_width,
            self.menu_row_height,
            self.menu_max_height,
        ];
        if dimensions
            .into_iter()
            .any(|dimension| !dimension.value().is_finite() || dimension.value() < 0.0)
        {
            return Err(LayoutValidationError::InvalidDimension);
        }
        if self.navigation_pane_min_width.value() > self.navigation_pane_default_width.value()
            || self.navigation_pane_default_width.value() > self.navigation_pane_max_width.value()
        {
            return Err(LayoutValidationError::NavigationPaneOrder);
        }
        if self.side_pane_min_width.value() > self.side_pane_default_width.value()
            || self.side_pane_default_width.value() > self.side_pane_max_width.value()
        {
            return Err(LayoutValidationError::SidePaneOrder);
        }
        if self.minimum_hit_target.value() < self.maximum_visible_glyph.value() {
            return Err(LayoutValidationError::HitTargetTooSmall);
        }
        if self.reference_profile.is_empty() {
            return Err(LayoutValidationError::MissingReferenceProfile);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LayoutValidationError {
    InvalidDimension,
    NavigationPaneOrder,
    SidePaneOrder,
    HitTargetTooSmall,
    MissingReferenceProfile,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MotionPreference {
    Standard,
    Reduced,
}

impl LayoutTokens {
    pub fn search_box_width_for_window(self, window_width: f32) -> f32 {
        if !window_width.is_finite() || window_width < self.compact_window_width.value() {
            return self.compact_search_box_width.value();
        }
        (window_width * EXPLORER_SEARCH_WINDOW_RATIO).clamp(
            self.compact_search_box_width.value(),
            self.search_box_width.value(),
        )
    }

    pub const fn animation_duration(self, preference: MotionPreference) -> u16 {
        match preference {
            MotionPreference::Standard => self.animation_duration_ms,
            MotionPreference::Reduced => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DpiScale, EXPLORER_SEARCH_WINDOW_RATIO, LayoutTokens, LayoutValidationError, LogicalPx,
        MotionPreference,
    };

    #[test]
    fn windows_11_layout_contract_is_valid() {
        LayoutTokens::WINDOWS_11.validate().expect("valid tokens");
    }

    #[test]
    fn rejects_invalid_pane_order_dimensions_and_hit_targets() {
        let mut tokens = LayoutTokens::WINDOWS_11;
        tokens.navigation_pane_default_width = LogicalPx::new(100.0);
        assert_eq!(
            tokens.validate(),
            Err(LayoutValidationError::NavigationPaneOrder)
        );

        let mut tokens = LayoutTokens::WINDOWS_11;
        tokens.side_pane_default_width = LogicalPx::new(100.0);
        assert_eq!(tokens.validate(), Err(LayoutValidationError::SidePaneOrder));

        let mut tokens = LayoutTokens::WINDOWS_11;
        tokens.content_spacing = LogicalPx::new(f32::NAN);
        assert_eq!(
            tokens.validate(),
            Err(LayoutValidationError::InvalidDimension)
        );

        let mut tokens = LayoutTokens::WINDOWS_11;
        tokens.minimum_hit_target = LogicalPx::new(16.0);
        assert_eq!(
            tokens.validate(),
            Err(LayoutValidationError::HitTargetTooSmall)
        );
    }

    #[test]
    fn logical_values_scale_once_at_supported_windows_percentages() {
        let logical = LogicalPx::new(32.0);
        for (percent, expected) in [
            (100, 32.0),
            (125, 40.0),
            (150, 48.0),
            (175, 56.0),
            (200, 64.0),
        ] {
            let scale = DpiScale::from_percent(percent).expect("positive scale");
            assert!((logical.to_physical(scale).value() - expected).abs() < f32::EPSILON);
        }
        assert!(DpiScale::from_percent(0).is_none());
    }

    #[test]
    fn regional_contract_contains_every_explorer_surface_dimension() {
        let tokens = LayoutTokens::WINDOWS_11;
        let required = [
            tokens.title_tab_height,
            tokens.address_bar_height,
            tokens.command_bar_height,
            tokens.navigation_pane_default_width,
            tokens.side_pane_default_width,
            tokens.divider_width,
            tokens.details_header_height,
            tokens.file_row_height,
            tokens.inline_rename_height,
            tokens.status_bar_height,
            tokens.search_box_width,
            tokens.caption_button_width,
            tokens.menu_row_height,
        ];
        assert!(required.into_iter().all(|value| value.value() > 0.0));
        assert!(tokens.compact_address_min_width.value() < tokens.address_min_width.value());
        assert!(tokens.address_min_width.value() < tokens.search_box_width.value());
        assert!(tokens.compact_search_box_width.value() < tokens.search_box_width.value());
        assert!(!tokens.reference_profile.is_empty());
    }

    #[test]
    fn search_width_matches_the_current_explorer_reference_ratio() {
        let tokens = LayoutTokens::WINDOWS_11;
        for (physical_window, physical_search) in [(1_867.0_f32, 435.0_f32), (2_688.0, 671.0)] {
            let logical_window = physical_window / 1.75;
            let actual_ratio = tokens.search_box_width_for_window(logical_window) / logical_window;
            let reference_ratio = physical_search / physical_window;
            let relative_error = (actual_ratio - reference_ratio).abs() / reference_ratio;
            assert!(relative_error <= 0.10);
        }
        assert!((EXPLORER_SEARCH_WINDOW_RATIO - 0.235).abs() < f32::EPSILON);
        assert!((tokens.search_box_width_for_window(799.0) - 120.0).abs() < f32::EPSILON);
        assert!((tokens.search_box_width_for_window(f32::NAN) - 120.0).abs() < f32::EPSILON);
        assert!((tokens.search_box_width_for_window(2_000.0) - 384.0).abs() < f32::EPSILON);
    }

    #[test]
    fn compact_navigation_fields_fit_without_overlap() {
        let tokens = LayoutTokens::WINDOWS_11;
        let available_after_fixed_controls = 320.0;
        let required = tokens.compact_address_min_width.value()
            + tokens.address_search_gap.value()
            + tokens.compact_search_box_width.value();
        assert!(required <= available_after_fixed_controls);
    }

    #[test]
    fn reduced_motion_contract_disables_nonessential_animation() {
        let tokens = LayoutTokens::WINDOWS_11;
        assert_eq!(
            tokens.animation_duration(MotionPreference::Standard),
            tokens.animation_duration_ms
        );
        assert_eq!(
            tokens.animation_duration(MotionPreference::Reduced),
            0,
            "reduced motion must not depend on a shortened arbitrary duration"
        );
    }
}
