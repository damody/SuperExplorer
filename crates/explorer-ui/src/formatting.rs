use explorer_i18n::{AppLocale, Catalog, FluentArgs};

const UNIT_KEYS: [&str; 4] = [
    "file-size-unit-kb",
    "file-size-unit-mb",
    "file-size-unit-gb",
    "file-size-unit-tb",
];

const TRANSFER_UNIT_KEYS: [&str; 5] = [
    "file-size-unit-b",
    "file-size-unit-kb",
    "file-size-unit-mb",
    "file-size-unit-gb",
    "file-size-unit-tb",
];

const SPEED_UNIT_KEYS: [&str; 4] = [
    "file-size-unit-b-s",
    "file-size-unit-kb-s",
    "file-size-unit-mb-s",
    "file-size-unit-gb-s",
];

/// Formats a byte count with adaptive binary units for Explorer surfaces.
#[allow(
    clippy::cast_precision_loss,
    reason = "Explorer size labels intentionally use bounded human-readable precision"
)]
pub fn format_file_size(bytes: u64, locale: AppLocale) -> String {
    let catalog = Catalog::new(locale);
    if bytes == 0 {
        return catalog.t("format-zero-kb");
    }
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNIT_KEYS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 && value < 1.0 {
        value = 1.0;
    }
    fluent_file_size(&catalog, format!("{value:.1}"), UNIT_KEYS[unit])
}

/// Formats a transfer byte count, including a whole-byte unit for sub-kilobyte values.
#[allow(
    clippy::cast_precision_loss,
    reason = "Explorer size labels intentionally use bounded human-readable precision"
)]
pub fn format_transfer_bytes(bytes: u64, locale: AppLocale) -> String {
    let catalog = Catalog::new(locale);
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < TRANSFER_UNIT_KEYS.len() {
        value /= 1024.0;
        unit += 1;
    }
    let number = if unit == 0 {
        bytes.to_string()
    } else {
        format!("{value:.1}")
    };
    fluent_file_size(&catalog, number, TRANSFER_UNIT_KEYS[unit])
}

/// Formats a transfer speed with adaptive binary units.
#[allow(
    clippy::cast_precision_loss,
    reason = "Explorer size labels intentionally use bounded human-readable precision"
)]
pub fn format_transfer_speed(bytes_per_second: f64, locale: AppLocale) -> String {
    let catalog = Catalog::new(locale);
    let mut value = bytes_per_second.max(0.0);
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < SPEED_UNIT_KEYS.len() {
        value /= 1024.0;
        unit += 1;
    }
    let number = if unit == 0 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    };
    fluent_file_size(&catalog, number, SPEED_UNIT_KEYS[unit])
}

fn fluent_file_size(catalog: &Catalog, value: String, unit_key: &str) -> String {
    let mut args = FluentArgs::new();
    args.set("value", value);
    args.set("unit", catalog.t(unit_key));
    catalog.t_args("file-size", &args)
}

#[cfg(test)]
mod tests {
    use explorer_i18n::AppLocale;

    use super::format_file_size;

    fn strip_isolates(value: &str) -> String {
        value
            .chars()
            .filter(|ch| !matches!(*ch, '\u{2066}' | '\u{2067}' | '\u{2068}' | '\u{2069}'))
            .collect()
    }

    #[test]
    fn promotes_units_and_rounds_small_files() {
        let cases = [
            (0, "0 KB"),
            (1, "1.0 KB"),
            (1023, "1.0 KB"),
            (1024, "1.0 KB"),
            (1536, "1.5 KB"),
            (10 * 1024, "10.0 KB"),
            (1024 * 1024, "1.0 MB"),
            (1536 * 1024, "1.5 MB"),
            (1024 * 1024 * 1024, "1.0 GB"),
            (5_427_537_920, "5.1 GB"),
            (250 * 1024_u64.pow(3) + 512 * 1024_u64.pow(2), "250.5 GB"),
            (1024_u64.pow(4), "1.0 TB"),
        ];
        for (bytes, expected) in cases {
            assert_eq!(
                strip_isolates(&format_file_size(bytes, AppLocale::En)),
                expected,
                "bytes={bytes}"
            );
        }
    }
}
