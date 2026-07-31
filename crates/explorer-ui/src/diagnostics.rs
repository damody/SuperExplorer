//! Actual post-layout region diagnostics for Explorer visual parity.

use std::{collections::BTreeMap, sync::Mutex};

use gpui::{App, Bounds, Global, IntoElement, Pixels, Styled, Window, canvas};
use serde::{Deserialize, Serialize};

pub const REGION_DIAGNOSTICS_SCHEMA_VERSION: u32 = 2;
pub const REGION_ROUNDING_METHOD: &str =
    "gpui-logical-bounds-multiplied-by-window-scale-no-rounding";

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct DiagnosticRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl DiagnosticRect {
    fn is_valid(self) -> bool {
        [self.x, self.y, self.width, self.height]
            .into_iter()
            .all(f32::is_finite)
            && self.width >= 0.0
            && self.height >= 0.0
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RegionObservation {
    pub id: String,
    pub parent: Option<String>,
    pub state: String,
    pub logical_rect: DiagnosticRect,
    pub physical_rect: DiagnosticRect,
    pub icon_bounds: Option<DiagnosticRect>,
    pub typography_reference: Option<TypographyObservation>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TypographyObservation {
    pub profile: String,
    pub family: String,
    pub size: f32,
    pub weight: u16,
    pub line_height: f32,
    pub baseline: f32,
}

impl TypographyObservation {
    fn is_valid(&self) -> bool {
        !self.profile.is_empty()
            && !self.family.is_empty()
            && self.size.is_finite()
            && self.size > 0.0
            && self.line_height.is_finite()
            && self.line_height >= self.size
            && self.baseline.is_finite()
            && self.baseline > 0.0
            && self.baseline <= self.line_height
            && (100..=900).contains(&self.weight)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ExplorerRegionDiagnostics {
    pub schema_version: u32,
    pub actual_scale_factor: f32,
    pub rounding_method: String,
    pub regions: Vec<RegionObservation>,
}

impl ExplorerRegionDiagnostics {
    /// Verifies identity, finite geometry, single scaling, and optional bounds.
    ///
    /// # Errors
    ///
    /// Returns a stable diagnostic string for the first invalid field.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != REGION_DIAGNOSTICS_SCHEMA_VERSION {
            return Err("unsupported schema version");
        }
        if !self.actual_scale_factor.is_finite() || self.actual_scale_factor <= 0.0 {
            return Err("invalid scale factor");
        }
        let mut ids = std::collections::HashSet::with_capacity(self.regions.len());
        for region in &self.regions {
            if region.id.is_empty() || !ids.insert(region.id.as_str()) {
                return Err("region ids must be non-empty and unique");
            }
            if !region.logical_rect.is_valid()
                || !region.physical_rect.is_valid()
                || region.icon_bounds.is_some_and(|bounds| !bounds.is_valid())
                || region
                    .typography_reference
                    .as_ref()
                    .is_some_and(|typography| !typography.is_valid())
            {
                return Err("region rectangles must be finite and non-negative");
            }
            for (logical, physical) in [
                (region.logical_rect.x, region.physical_rect.x),
                (region.logical_rect.y, region.physical_rect.y),
                (region.logical_rect.width, region.physical_rect.width),
                (region.logical_rect.height, region.physical_rect.height),
            ] {
                if (logical * self.actual_scale_factor - physical).abs() > 1.0 {
                    return Err("physical geometry must apply the scale factor once");
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct RegionDiagnosticsRecorder {
    regions: Mutex<BTreeMap<String, RegionObservation>>,
}

impl Global for RegionDiagnosticsRecorder {}

impl RegionDiagnosticsRecorder {
    pub fn record(
        &self,
        id: &str,
        parent: Option<&str>,
        state: &str,
        bounds: Bounds<Pixels>,
        scale: f32,
    ) {
        if !scale.is_finite() || scale <= 0.0 {
            return;
        }
        let logical_rect = DiagnosticRect {
            x: f32::from(bounds.origin.x),
            y: f32::from(bounds.origin.y),
            width: f32::from(bounds.size.width),
            height: f32::from(bounds.size.height),
        };
        let physical_rect = DiagnosticRect {
            x: logical_rect.x * scale,
            y: logical_rect.y * scale,
            width: logical_rect.width * scale,
            height: logical_rect.height * scale,
        };
        let observation = RegionObservation {
            id: id.to_owned(),
            parent: parent.map(str::to_owned),
            state: state.to_owned(),
            logical_rect,
            physical_rect,
            icon_bounds: None,
            typography_reference: None,
        };
        self.regions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(id.to_owned(), observation);
    }

    pub fn snapshot(&self, actual_scale_factor: f32) -> ExplorerRegionDiagnostics {
        ExplorerRegionDiagnostics {
            schema_version: REGION_DIAGNOSTICS_SCHEMA_VERSION,
            actual_scale_factor,
            rounding_method: REGION_ROUNDING_METHOD.to_owned(),
            regions: self
                .regions
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .values()
                .cloned()
                .collect(),
        }
    }

    pub fn record_icon(&self, region_id: &str, bounds: Bounds<Pixels>, scale: f32) {
        if !scale.is_finite() || scale <= 0.0 {
            return;
        }
        let icon_bounds = DiagnosticRect {
            x: f32::from(bounds.origin.x) * scale,
            y: f32::from(bounds.origin.y) * scale,
            width: f32::from(bounds.size.width) * scale,
            height: f32::from(bounds.size.height) * scale,
        };
        if let Some(region) = self
            .regions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get_mut(region_id)
        {
            region.icon_bounds = Some(icon_bounds);
        }
    }

    pub fn record_typography(&self, region_id: &str, typography: TypographyObservation) {
        if !typography.is_valid() {
            return;
        }
        if let Some(region) = self
            .regions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get_mut(region_id)
        {
            region.typography_reference = Some(typography);
        }
    }
}

/// Adds a non-interactive absolute overlay that records its parent's actual bounds.
pub fn region_probe(
    id: impl Into<String>,
    parent: Option<&'static str>,
    state: &'static str,
) -> impl IntoElement {
    let id = id.into();
    canvas(
        move |bounds, window: &mut Window, cx: &mut App| {
            if let Some(recorder) = cx.try_global::<RegionDiagnosticsRecorder>() {
                recorder.record(&id, parent, state, bounds, window.scale_factor());
            }
        },
        |_, (), _, _| {},
    )
    .absolute()
    .inset_0()
}

pub fn icon_probe(region_id: impl Into<String>) -> impl IntoElement {
    let region_id = region_id.into();
    canvas(
        move |bounds, window: &mut Window, cx: &mut App| {
            if let Some(recorder) = cx.try_global::<RegionDiagnosticsRecorder>() {
                recorder.record_icon(&region_id, bounds, window.scale_factor());
            }
        },
        |_, (), _, _| {},
    )
    .absolute()
    .inset_0()
}

pub fn typography_probe(
    region_id: impl Into<String>,
    typography: TypographyObservation,
) -> impl IntoElement {
    let region_id = region_id.into();
    canvas(
        move |_, _, cx: &mut App| {
            if let Some(recorder) = cx.try_global::<RegionDiagnosticsRecorder>() {
                recorder.record_typography(&region_id, typography.clone());
            }
        },
        |_, (), _, _| {},
    )
    .absolute()
    .inset_0()
}

#[cfg(test)]
mod tests {
    use super::{
        DiagnosticRect, ExplorerRegionDiagnostics, REGION_DIAGNOSTICS_SCHEMA_VERSION,
        REGION_ROUNDING_METHOD, RegionObservation, TypographyObservation,
    };

    fn region(id: &str) -> RegionObservation {
        RegionObservation {
            id: id.to_owned(),
            parent: Some("window".to_owned()),
            state: "normal".to_owned(),
            logical_rect: DiagnosticRect {
                x: 1.0,
                y: 2.0,
                width: 10.0,
                height: 20.0,
            },
            physical_rect: DiagnosticRect {
                x: 1.75,
                y: 3.5,
                width: 17.5,
                height: 35.0,
            },
            icon_bounds: None,
            typography_reference: Some(TypographyObservation {
                profile: "reference".to_owned(),
                family: "Microsoft JhengHei UI".to_owned(),
                size: 12.0,
                weight: 400,
                line_height: 16.0,
                baseline: 13.0,
            }),
        }
    }

    fn document(regions: Vec<RegionObservation>) -> ExplorerRegionDiagnostics {
        ExplorerRegionDiagnostics {
            schema_version: REGION_DIAGNOSTICS_SCHEMA_VERSION,
            actual_scale_factor: 1.75,
            rounding_method: REGION_ROUNDING_METHOD.to_owned(),
            regions,
        }
    }

    #[test]
    fn schema_accepts_unique_finite_once_scaled_regions() {
        assert_eq!(document(vec![region("command")]).validate(), Ok(()));
    }

    #[test]
    fn schema_rejects_duplicate_ids_non_finite_geometry_and_double_scaling() {
        assert_eq!(
            document(vec![region("same"), region("same")]).validate(),
            Err("region ids must be non-empty and unique")
        );
        let mut invalid = region("invalid");
        invalid.logical_rect.width = f32::NAN;
        assert_eq!(
            document(vec![invalid]).validate(),
            Err("region rectangles must be finite and non-negative")
        );
        let mut double_scaled = region("scaled-twice");
        double_scaled.physical_rect.width = 30.625;
        assert_eq!(
            document(vec![double_scaled]).validate(),
            Err("physical geometry must apply the scale factor once")
        );
    }

    #[test]
    fn serde_round_trip_preserves_parent_state_icon_and_typography_fields() {
        let expected = document(vec![region("address")]);
        let encoded = serde_json::to_string(&expected).expect("serialize diagnostics");
        let actual: ExplorerRegionDiagnostics =
            serde_json::from_str(&encoded).expect("parse diagnostics");
        assert_eq!(actual, expected);
        assert_eq!(actual.validate(), Ok(()));
    }
}
