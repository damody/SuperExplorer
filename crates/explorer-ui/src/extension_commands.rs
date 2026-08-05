//! Pure planning helpers for the two official command-surface examples.

use std::{collections::BTreeSet, fs::File, io::BufReader, path::Path};

use exif::{In, Reader, Tag, Value};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExtensionCommandPanel {
    ExifRename,
    BulkFolder,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExifRenamePreset {
    DateTime,
    DateTimeAndOriginal,
}

pub fn generate_bulk_folder_names(count: u32) -> Result<Vec<String>, String> {
    if !(1..=100_000).contains(&count) {
        return Err("Folder count must be between 1 and 100000".to_owned());
    }
    (1..=count)
        .map(|number| {
            let name = format!("Folder-{number:03}");
            explorer_model::validate_windows_file_name(&name)
                .map_err(|error| format!("Invalid folder name {name}: {error:?}"))?;
            Ok(name)
        })
        .collect()
}

fn exif_ascii(value: &Value) -> Option<String> {
    let bytes = match value {
        Value::Ascii(values) => values.first()?,
        _ => return None,
    };
    Some(
        String::from_utf8_lossy(bytes)
            .trim_matches(char::from(0))
            .trim()
            .to_owned(),
    )
}

fn exif_date_time(path: &Path) -> Result<String, String> {
    let file = File::open(path).map_err(|error| error.to_string())?;
    let exif = Reader::new()
        .read_from_container(&mut BufReader::new(file))
        .map_err(|_| format!("{} has no readable EXIF metadata", path.display()))?;
    let field = exif
        .get_field(Tag::DateTimeOriginal, In::PRIMARY)
        .or_else(|| {
            exif.fields()
                .find(|field| field.tag == Tag::DateTimeOriginal)
        })
        .and_then(|field| exif_ascii(&field.value))
        .ok_or_else(|| format!("{} has no DateTimeOriginal value", path.display()))?;
    let digits = field
        .chars()
        .filter(|character| character.is_ascii_digit())
        .collect::<String>();
    if digits.len() < 14 {
        return Err(format!("{} has an invalid EXIF date", path.display()));
    }
    Ok(format!("{}_{}", &digits[..8], &digits[8..14]))
}

pub fn exif_rename_requests(
    items: &[explorer_model::ItemDescriptor],
    preset: ExifRenamePreset,
) -> Result<Vec<explorer_model::FileOperationRequest>, String> {
    if items.is_empty() {
        return Err("Select at least one image before using EXIF rename".to_owned());
    }
    let mut targets = BTreeSet::new();
    items
        .iter()
        .map(|item| {
            let path = item
                .location
                .path()
                .ok_or_else(|| "EXIF rename only supports file-system items".to_owned())?;
            let extension = path
                .extension()
                .and_then(|value| value.to_str())
                .ok_or_else(|| format!("{} has no file extension", path.display()))?;
            if !matches!(
                extension.to_ascii_lowercase().as_str(),
                "jpg" | "jpeg" | "tif" | "tiff"
            ) {
                return Err(format!("{} is not a supported EXIF image", path.display()));
            }
            let date_time = exif_date_time(path)?;
            let stem = match preset {
                ExifRenamePreset::DateTime => date_time,
                ExifRenamePreset::DateTimeAndOriginal => {
                    let original = path
                        .file_stem()
                        .and_then(|value| value.to_str())
                        .unwrap_or("image");
                    format!("{date_time}_{original}")
                }
            };
            let new_name = format!("{stem}.{extension}");
            explorer_model::validate_windows_file_name(&new_name)
                .map_err(|error| format!("Invalid rename target {new_name}: {error:?}"))?;
            if !targets.insert(new_name.to_lowercase()) {
                return Err(format!("Duplicate rename target: {new_name}"));
            }
            Ok(explorer_model::FileOperationRequest {
                kind: explorer_model::FileOperationKind::Rename {
                    item: item.clone(),
                    new_name,
                },
                flags: explorer_model::FileOperationFlags::default(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bulk_folder_preset_is_bounded_and_deterministic() {
        assert_eq!(
            generate_bulk_folder_names(2).unwrap(),
            ["Folder-001", "Folder-002"]
        );
        assert!(generate_bulk_folder_names(0).is_err());
        assert!(generate_bulk_folder_names(100_001).is_err());
    }
}
