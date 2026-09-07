use explorer_i18n::{AppLocale, Catalog, FluentArgs};
use explorer_model::FileEntry;

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

/// Details Type column and filter label for the active UI catalog.
///
/// Shell `SHGetFileInfo` type names follow the Windows display language, so
/// live app-locale switching must not reuse that stored string.
pub(crate) fn localized_entry_type(entry: &FileEntry, catalog: Catalog) -> String {
    if entry.is_container {
        return catalog.t("type-file-folder");
    }
    if !matches!(
        entry.location,
        explorer_model::LocationDescriptor::FileSystem(_)
    ) {
        return entry
            .metadata
            .type_display
            .as_deref()
            .filter(|value| !value.is_empty())
            .map_or_else(|| catalog.t("chrome-file"), str::to_owned);
    }
    match file_extension_label(&entry.display_name) {
        Some(ext) => {
            let mut args = FluentArgs::new();
            args.set("ext", ext);
            catalog.t_args("type-file-ext", &args)
        }
        None => catalog.t("chrome-file"),
    }
}

fn file_extension_label(name: &str) -> Option<String> {
    let ext = std::path::Path::new(name)
        .extension()?
        .to_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    Some(ext.to_ascii_uppercase())
}

/// Stable Type-filter key that does not change when the UI catalog changes.
pub(crate) fn type_filter_key(entry: &FileEntry) -> String {
    if entry.is_container {
        return "type:file-folder".to_owned();
    }
    match file_extension_label(&entry.display_name) {
        Some(ext) => format!("type:ext:{}", ext.to_ascii_lowercase()),
        None => "type:file".to_owned(),
    }
}

fn fluent_file_size(catalog: &Catalog, value: String, unit_key: &str) -> String {
    let mut args = FluentArgs::new();
    args.set("value", value);
    args.set("unit", catalog.t(unit_key));
    catalog.t_args("file-size", &args)
}

#[cfg(test)]
mod tests {
    use explorer_i18n::{AppLocale, Catalog};

    use super::format_file_size;

    fn strip_isolates(value: &str) -> String {
        value
            .chars()
            .filter(|ch| !matches!(*ch, '\u{2066}' | '\u{2067}' | '\u{2068}' | '\u{2069}'))
            .collect()
    }

    #[test]
    fn localized_entry_type_follows_catalog_not_shell_type_name() {
        let mut folder = explorer_model::FileEntry {
            id: explorer_model::ShellItemId::from_provider_bytes(1u64.to_le_bytes()).unwrap(),
            display_name: "docs".to_owned(),
            location: explorer_model::LocationDescriptor::file_system(r"C:\docs"),
            is_container: true,
            metadata: explorer_model::FileEntryMetadata {
                type_display: Some("檔案資料夾".to_owned()),
                ..explorer_model::FileEntryMetadata::default()
            },
        };
        assert_eq!(
            strip_isolates(&super::localized_entry_type(
                &folder,
                Catalog::new(AppLocale::En)
            )),
            "File folder"
        );
        assert_eq!(
            strip_isolates(&super::localized_entry_type(
                &folder,
                Catalog::new(AppLocale::ZhTw)
            )),
            "檔案資料夾"
        );
        folder.is_container = false;
        folder.display_name = "notes.json".to_owned();
        folder.metadata.type_display = Some("JSON 來源檔案".to_owned());
        assert_eq!(
            strip_isolates(&super::localized_entry_type(
                &folder,
                Catalog::new(AppLocale::En)
            )),
            "JSON File"
        );
        assert_eq!(
            strip_isolates(&super::localized_entry_type(
                &folder,
                Catalog::new(AppLocale::ZhTw)
            )),
            "JSON 檔案"
        );
        assert_eq!(super::type_filter_key(&folder), "type:ext:json");
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
