//! Explorer typography values, kept separate from geometry and injected once at the root.

use crate::layout::LogicalPx;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FontFallback {
    pub primary: &'static str,
    pub fallbacks: &'static [&'static str],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TypographyStyle {
    pub size: LogicalPx,
    pub line_height: LogicalPx,
    pub baseline: LogicalPx,
    pub weight: u16,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TypographyTokens {
    pub reference_profile: &'static str,
    pub family: FontFallback,
    pub tab: TypographyStyle,
    pub command: TypographyStyle,
    pub address: TypographyStyle,
    pub search: TypographyStyle,
    pub navigation: TypographyStyle,
    pub details_header: TypographyStyle,
    pub file_row: TypographyStyle,
    pub menu: TypographyStyle,
    pub tooltip: TypographyStyle,
    pub status: TypographyStyle,
}

const BODY: TypographyStyle = TypographyStyle {
    size: LogicalPx::new(12.0),
    line_height: LogicalPx::new(16.0),
    baseline: LogicalPx::new(13.0),
    weight: 400,
};

const EXPLORER_INPUT: TypographyStyle = TypographyStyle {
    size: LogicalPx::new(14.0),
    line_height: LogicalPx::new(22.0),
    baseline: LogicalPx::new(17.0),
    weight: 400,
};

impl TypographyTokens {
    pub const WINDOWS_11_ZH_TW: Self = Self {
        reference_profile: "windows-11-26200-explorer-26100.8875-zh-tw-175",
        family: FontFallback {
            primary: "Microsoft JhengHei UI",
            fallbacks: &["Segoe UI Variable Text", "Segoe UI", "sans-serif"],
        },
        tab: BODY,
        command: BODY,
        address: EXPLORER_INPUT,
        search: EXPLORER_INPUT,
        navigation: BODY,
        details_header: TypographyStyle {
            size: LogicalPx::new(11.0),
            line_height: LogicalPx::new(16.0),
            baseline: LogicalPx::new(13.0),
            weight: 400,
        },
        file_row: BODY,
        menu: BODY,
        tooltip: TypographyStyle {
            size: LogicalPx::new(11.0),
            line_height: LogicalPx::new(16.0),
            baseline: LogicalPx::new(13.0),
            weight: 400,
        },
        status: TypographyStyle {
            size: LogicalPx::new(11.0),
            line_height: LogicalPx::new(14.0),
            baseline: LogicalPx::new(11.0),
            weight: 400,
        },
    };

    /// Validates every surface style and the Windows UI fallback profile.
    ///
    /// # Errors
    ///
    /// Returns an error when a profile, family, metric, baseline, or weight is invalid.
    pub fn validate(self) -> Result<(), &'static str> {
        if self.reference_profile.is_empty() || self.family.primary.is_empty() {
            return Err("typography profile and primary family are required");
        }
        for style in self.styles() {
            if !style.size.value().is_finite()
                || style.size.value() <= 0.0
                || !style.line_height.value().is_finite()
                || style.line_height.value() < style.size.value()
                || !style.baseline.value().is_finite()
                || style.baseline.value() <= 0.0
                || style.baseline.value() > style.line_height.value()
                || !(100..=900).contains(&style.weight)
            {
                return Err("invalid typography style");
            }
        }
        Ok(())
    }

    pub const fn styles(self) -> [TypographyStyle; 10] {
        [
            self.tab,
            self.command,
            self.address,
            self.search,
            self.navigation,
            self.details_header,
            self.file_row,
            self.menu,
            self.tooltip,
            self.status,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::TypographyTokens;

    #[test]
    fn explorer_typography_contract_covers_every_surface() {
        let tokens = TypographyTokens::WINDOWS_11_ZH_TW;
        assert_eq!(tokens.styles().len(), 10);
        assert_eq!(tokens.validate(), Ok(()));
        assert!(
            tokens
                .styles()
                .into_iter()
                .all(|style| style.size.value() >= 10.0)
        );
    }

    #[test]
    fn traditional_chinese_uses_windows_ui_fallbacks() {
        let family = TypographyTokens::WINDOWS_11_ZH_TW.family;
        assert_eq!(family.primary, "Microsoft JhengHei UI");
        assert!(family.fallbacks.contains(&"Segoe UI"));
        assert_ne!(family.primary, "Arial");
    }

    #[test]
    fn explorer_inputs_use_the_larger_centerable_metrics() {
        let tokens = TypographyTokens::WINDOWS_11_ZH_TW;

        assert!((tokens.address.size.value() - 14.0).abs() < f32::EPSILON);
        assert!((tokens.address.line_height.value() - 22.0).abs() < f32::EPSILON);
        assert!((tokens.address.baseline.value() - 17.0).abs() < f32::EPSILON);
        assert_eq!(tokens.search, tokens.address);
    }
}
