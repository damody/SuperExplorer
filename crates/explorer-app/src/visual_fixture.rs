//! Deterministic visual-fixture configuration and diagnostics export.

use std::{path::PathBuf, str::FromStr};

use anyhow::{Context as _, Result, bail};
use explorer_ui::{
    MINIMUM_WINDOW_HEIGHT, MINIMUM_WINDOW_WIDTH, UiTokens, VisualFixtureState,
    theme::{SemanticColorSlot, ThemeMode, ThemeTokens},
};
use gpui::Window;
use serde_json::json;

pub const VISUAL_SCHEMA_VERSION: u8 = 1;
pub const VISUAL_FONT: &str = "Microsoft JhengHei UI";
pub const VISUAL_PLACEHOLDER_STATE: &str = "populated";
pub const VISUAL_STATES: [&str; 8] = [
    "empty",
    "populated",
    "error",
    "multi-tab",
    "operation",
    "drag-cue",
    "search",
    "focused",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VisualTheme {
    Light,
    Dark,
}

impl VisualTheme {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    pub const fn tokens(self) -> ThemeTokens {
        match self {
            Self::Light => ThemeTokens::light(),
            Self::Dark => ThemeTokens::dark(),
        }
    }
}

impl FromStr for VisualTheme {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "light" => Ok(Self::Light),
            "dark" => Ok(Self::Dark),
            _ => bail!("visual theme must be 'light' or 'dark'"),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct VisualFixtureConfig {
    pub width: f32,
    pub height: f32,
    pub expected_dpi_percent: u16,
    pub theme: VisualTheme,
    pub font: String,
    pub placeholder_state: String,
    pub diagnostics_path: PathBuf,
    pub real_shell: bool,
}

impl VisualFixtureConfig {
    /// Reads the opt-in fixture contract from process environment variables.
    ///
    /// # Errors
    ///
    /// Returns an error when fixture mode is enabled with an invalid or
    /// uncontrolled dimension, DPI, theme, font, state, or output path.
    pub fn from_environment() -> Result<Option<Self>> {
        Self::from_values(|key| std::env::var(key).ok())
    }

    fn from_values(get: impl Fn(&str) -> Option<String>) -> Result<Option<Self>> {
        if get("EXPLORER_VISUAL_FIXTURE").as_deref() != Some("1") {
            return Ok(None);
        }
        let width: f32 = parse_or_default(&get, "EXPLORER_VISUAL_WIDTH", 1_120.0)?;
        let height: f32 = parse_or_default(&get, "EXPLORER_VISUAL_HEIGHT", 720.0)?;
        if !width.is_finite() || width < MINIMUM_WINDOW_WIDTH {
            bail!("visual fixture width must be finite and at least {MINIMUM_WINDOW_WIDTH}");
        }
        if !height.is_finite() || height < MINIMUM_WINDOW_HEIGHT {
            bail!("visual fixture height must be finite and at least {MINIMUM_WINDOW_HEIGHT}");
        }

        let expected_dpi_percent = parse_or_default(&get, "EXPLORER_VISUAL_DPI", 100_u16)?;
        if ![100, 125, 150, 175, 200].contains(&expected_dpi_percent) {
            bail!("visual fixture DPI must be one of 100, 125, 150, 175, or 200 percent");
        }
        let theme = get("EXPLORER_VISUAL_THEME")
            .unwrap_or_else(|| "light".to_owned())
            .parse()?;
        let font = get("EXPLORER_VISUAL_FONT").unwrap_or_else(|| VISUAL_FONT.to_owned());
        if font != VISUAL_FONT {
            bail!("visual fixture font must be '{VISUAL_FONT}'");
        }
        let placeholder_state =
            get("EXPLORER_VISUAL_STATE").unwrap_or_else(|| VISUAL_PLACEHOLDER_STATE.to_owned());
        if !VISUAL_STATES.contains(&placeholder_state.as_str()) {
            bail!(
                "visual fixture state must be one of {}",
                VISUAL_STATES.join(", ")
            );
        }
        let diagnostics_path = get("EXPLORER_VISUAL_DIAGNOSTICS")
            .map(PathBuf::from)
            .context("EXPLORER_VISUAL_DIAGNOSTICS is required in visual fixture mode")?;
        let real_shell = match get("EXPLORER_VISUAL_REAL_SHELL").as_deref() {
            None | Some("0") => false,
            Some("1") => true,
            Some(_) => bail!("EXPLORER_VISUAL_REAL_SHELL must be 0 or 1"),
        };

        Ok(Some(Self {
            width,
            height,
            expected_dpi_percent,
            theme,
            font,
            placeholder_state,
            diagnostics_path,
            real_shell,
        }))
    }

    pub fn tokens(&self) -> UiTokens {
        UiTokens {
            theme: self.theme.tokens(),
            ..UiTokens::default()
        }
    }

    pub fn state(&self) -> VisualFixtureState {
        match self.placeholder_state.as_str() {
            "empty" => VisualFixtureState::Empty,
            "error" => VisualFixtureState::Error,
            "multi-tab" => VisualFixtureState::MultiTab,
            "operation" => VisualFixtureState::Operation,
            "drag-cue" => VisualFixtureState::DragCue,
            "search" => VisualFixtureState::Search,
            "focused" => VisualFixtureState::Focused,
            _ => VisualFixtureState::Populated,
        }
    }

    /// Writes the rendered fixture contract after GPUI completes its first frame.
    ///
    /// # Errors
    ///
    /// Returns an error when JSON serialization or the fixture-only file write fails.
    pub fn write_diagnostics(
        &self,
        window: &Window,
        tokens: UiTokens,
        regions: &explorer_ui::diagnostics::ExplorerRegionDiagnostics,
    ) -> Result<()> {
        let colors = tokens.theme.colors;
        let layout = tokens.layout;
        let color_values = SemanticColorSlot::ALL.map(|slot| {
            let value = colors.get(slot);
            json!({
                "slot": format!("{slot:?}"),
                "rgba": [value.red, value.green, value.blue, value.alpha]
            })
        });
        let payload = json!({
            "schema_version": VISUAL_SCHEMA_VERSION,
            "fixture": {
                "width_logical": self.width,
                "height_logical": self.height,
                "expected_dpi_percent": self.expected_dpi_percent,
                "actual_scale_factor": window.scale_factor(),
                "theme": self.theme.name(),
                "font": self.font,
                "placeholder_state": self.placeholder_state,
            },
            "theme": {
                "mode": match tokens.theme.mode { ThemeMode::Light => "light", ThemeMode::Dark => "dark" },
                "high_contrast_active": tokens.theme.high_contrast_active,
                "colors": color_values,
            },
            "layout": {
                "title_tab_height": layout.title_tab_height.value(),
                "command_bar_height": layout.command_bar_height.value(),
                "address_bar_height": layout.address_bar_height.value(),
                "status_bar_height": layout.status_bar_height.value(),
                "navigation_pane_width": layout.navigation_pane_default_width.value(),
                "divider_width": layout.divider_width.value(),
                "content_spacing": layout.content_spacing.value(),
                "focus_stroke": layout.focus_stroke.value(),
            },
            "region_diagnostics": regions,
        });
        let bytes = serde_json::to_vec_pretty(&payload)?;
        std::fs::write(&self.diagnostics_path, bytes).with_context(|| {
            format!(
                "failed to write visual diagnostics {}",
                self.diagnostics_path.display()
            )
        })?;
        Ok(())
    }
}

fn parse_or_default<T>(
    get: &impl Fn(&str) -> Option<String>,
    key: &'static str,
    default: T,
) -> Result<T>
where
    T: FromStr + Copy,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    get(key).map_or(Ok(default), |value| {
        value.parse().with_context(|| format!("invalid {key}"))
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{VISUAL_FONT, VISUAL_PLACEHOLDER_STATE, VisualFixtureConfig, VisualTheme};

    fn parse(values: &[(&str, &str)]) -> anyhow::Result<Option<VisualFixtureConfig>> {
        let values: HashMap<_, _> = values
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect();
        VisualFixtureConfig::from_values(|key| values.get(key).cloned())
    }

    #[test]
    fn disabled_fixture_does_not_require_visual_settings() -> anyhow::Result<()> {
        assert!(parse(&[])?.is_none());
        Ok(())
    }

    #[test]
    fn fixture_defaults_are_deterministic() -> anyhow::Result<()> {
        let config = parse(&[
            ("EXPLORER_VISUAL_FIXTURE", "1"),
            ("EXPLORER_VISUAL_DIAGNOSTICS", "diagnostics.json"),
        ])?
        .expect("fixture enabled");
        assert!((config.width - 1_120.0).abs() < f32::EPSILON);
        assert!((config.height - 720.0).abs() < f32::EPSILON);
        assert_eq!(config.expected_dpi_percent, 100);
        assert_eq!(config.theme, VisualTheme::Light);
        assert_eq!(config.font, VISUAL_FONT);
        assert_eq!(config.placeholder_state, VISUAL_PLACEHOLDER_STATE);
        assert!(!config.real_shell);
        Ok(())
    }

    #[test]
    fn fixture_can_capture_production_shell_state_without_replacing_layout_tokens()
    -> anyhow::Result<()> {
        let config = parse(&[
            ("EXPLORER_VISUAL_FIXTURE", "1"),
            ("EXPLORER_VISUAL_DIAGNOSTICS", "diagnostics.json"),
            ("EXPLORER_VISUAL_REAL_SHELL", "1"),
        ])?
        .expect("fixture enabled");
        assert!(config.real_shell);
        assert_eq!(config.theme, VisualTheme::Light);
        assert_eq!(config.font, VISUAL_FONT);
        Ok(())
    }

    #[test]
    fn fixture_rejects_uncontrolled_dimensions_dpi_font_and_state() {
        for (key, value) in [
            ("EXPLORER_VISUAL_WIDTH", "1"),
            ("EXPLORER_VISUAL_HEIGHT", "nan"),
            ("EXPLORER_VISUAL_DPI", "110"),
            ("EXPLORER_VISUAL_THEME", "system"),
            ("EXPLORER_VISUAL_FONT", "Arial"),
            ("EXPLORER_VISUAL_STATE", "fake-files"),
            ("EXPLORER_VISUAL_REAL_SHELL", "yes"),
        ] {
            assert!(
                parse(&[
                    ("EXPLORER_VISUAL_FIXTURE", "1"),
                    ("EXPLORER_VISUAL_DIAGNOSTICS", "diagnostics.json"),
                    (key, value),
                ])
                .is_err(),
                "accepted invalid {key}={value}"
            );
        }
    }
}
