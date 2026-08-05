//! In-process Rust EXIF rename command that emits host-owned rename plans.

use abi_stable::{
    export_root_module,
    prefix_type::PrefixTypeTrait,
    std_types::{ROption, RResult},
};
use exif::{In, Reader, Tag, Value};
use explorer_extension_api::*;
use std::{
    collections::BTreeSet,
    fs::File,
    io::{self, BufReader, Read, Seek, SeekFrom},
    path::Path,
};

const PLUGIN_ID: StableIdV1 = StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 7_101);
const INTERFACE_ID: StableIdV1 = StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 7_102);

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExifMetadata {
    pub rawname: String,
    pub extension: String,
    pub x_resolution: Option<f64>,
    pub y_resolution: Option<f64>,
    pub pixel_x_dimension: Option<u32>,
    pub pixel_y_dimension: Option<u32>,
    pub date_time_original: Option<String>,
}

fn unsigned(value: &Value) -> Option<u32> {
    match value {
        Value::Byte(values) => values.first().copied().map(u32::from),
        Value::Short(values) => values.first().copied().map(u32::from),
        Value::Long(values) => values.first().copied(),
        _ => None,
    }
}

fn rational(value: &Value) -> Option<f64> {
    match value {
        Value::Rational(values) => values
            .first()
            .and_then(|v| (v.denom != 0).then(|| v.num as f64 / v.denom as f64)),
        Value::SRational(values) => values
            .first()
            .and_then(|v| (v.denom != 0).then(|| v.num as f64 / v.denom as f64)),
        _ => None,
    }
}

fn ascii(value: &Value) -> Option<String> {
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

/// Decodes EXIF directly from an authorized file stream; no executable or specialist DLL is used.
pub fn decode_file(path: &Path) -> Result<ExifMetadata, String> {
    let file = File::open(path).map_err(|error| error.to_string())?;
    let exif = Reader::new()
        .read_from_container(&mut BufReader::new(file))
        .map_err(|error| error.to_string())?;
    let field = |tag| {
        exif.get_field(tag, In::PRIMARY)
            .or_else(|| exif.fields().find(|field| field.tag == tag))
    };
    Ok(ExifMetadata {
        rawname: path
            .file_stem()
            .and_then(|v| v.to_str())
            .unwrap_or_default()
            .to_owned(),
        extension: path
            .extension()
            .and_then(|v| v.to_str())
            .unwrap_or_default()
            .to_owned(),
        x_resolution: field(Tag::XResolution).and_then(|v| rational(&v.value)),
        y_resolution: field(Tag::YResolution).and_then(|v| rational(&v.value)),
        pixel_x_dimension: field(Tag::PixelXDimension).and_then(|v| unsigned(&v.value)),
        pixel_y_dimension: field(Tag::PixelYDimension).and_then(|v| unsigned(&v.value)),
        date_time_original: field(Tag::DateTimeOriginal).and_then(|v| ascii(&v.value)),
    })
}

struct AuthorizedStream(InputStreamV1);
impl Read for AuthorizedStream {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let maximum_bytes = u32::try_from(buffer.len().min(64 * 1024)).unwrap_or(64 * 1024);
        let result = self.0.read(InputStreamReadRequestV1 {
            maximum_bytes,
            reserved: 0,
        });
        if result.status == InputStreamStatusV1::EOF {
            return Ok(0);
        }
        if result.status != InputStreamStatusV1::OK || result.data.len() > buffer.len() {
            return Err(io::Error::other(format!(
                "authorized stream read failed: {}",
                result.status.into_raw()
            )));
        }
        buffer[..result.data.len()].copy_from_slice(&result.data);
        Ok(result.data.len())
    }
}
impl Seek for AuthorizedStream {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        let (origin, offset) = match position {
            SeekFrom::Start(value) => (
                InputStreamSeekOriginV1::START,
                i64::try_from(value).map_err(|_| io::Error::other("seek overflow"))?,
            ),
            SeekFrom::Current(value) => (InputStreamSeekOriginV1::CURRENT, value),
            SeekFrom::End(value) => (InputStreamSeekOriginV1::END, value),
        };
        let result = self.0.seek(InputStreamSeekRequestV1 {
            origin,
            reserved: 0,
            offset,
        });
        if result.status == InputStreamStatusV1::OK {
            Ok(result.position)
        } else {
            Err(io::Error::other(format!(
                "authorized stream seek failed: {}",
                result.status.into_raw()
            )))
        }
    }
}

/// Production decoder entry: consumes only the host-minted, generation-bound stream capability.
pub fn decode_input_stream(
    stream: InputStreamV1,
    rawname: &str,
    extension: &str,
) -> Result<ExifMetadata, String> {
    let exif = Reader::new()
        .read_from_container(&mut BufReader::new(AuthorizedStream(stream)))
        .map_err(|error| error.to_string())?;
    let field = |tag| {
        exif.get_field(tag, In::PRIMARY)
            .or_else(|| exif.fields().find(|field| field.tag == tag))
    };
    Ok(ExifMetadata {
        rawname: rawname.to_owned(),
        extension: extension.to_owned(),
        x_resolution: field(Tag::XResolution).and_then(|v| rational(&v.value)),
        y_resolution: field(Tag::YResolution).and_then(|v| rational(&v.value)),
        pixel_x_dimension: field(Tag::PixelXDimension).and_then(|v| unsigned(&v.value)),
        pixel_y_dimension: field(Tag::PixelYDimension).and_then(|v| unsigned(&v.value)),
        date_time_original: field(Tag::DateTimeOriginal).and_then(|v| ascii(&v.value)),
    })
}

pub fn sanitize_basename(value: &str) -> String {
    let value = value.trim().trim_end_matches(['.', ' ']);
    let mapped = value
        .chars()
        .map(|c| {
            if c < ' ' || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect::<String>();
    if mapped.is_empty() {
        "untitled".into()
    } else {
        mapped
    }
}

fn decimal(value: f64) -> String {
    let rendered = format!("{value:.6}");
    rendered
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_owned()
}

pub fn render_pattern(pattern: &str, metadata: &ExifMetadata) -> Result<String, String> {
    let tokens = [
        ("{rawname}", Some(metadata.rawname.clone())),
        ("{extension}", Some(metadata.extension.clone())),
        ("{XResolution}", metadata.x_resolution.map(decimal)),
        ("{YResolution}", metadata.y_resolution.map(decimal)),
        (
            "{PixelXDimension}",
            metadata.pixel_x_dimension.map(|v| v.to_string()),
        ),
        (
            "{PixelYDimension}",
            metadata.pixel_y_dimension.map(|v| v.to_string()),
        ),
        ("{DateTimeOriginal}", metadata.date_time_original.clone()),
    ];
    let mut output = pattern.to_owned();
    for (token, value) in tokens {
        if output.contains(token) {
            output = output.replace(
                token,
                value
                    .as_deref()
                    .ok_or_else(|| format!("missing EXIF token {token}"))?,
            );
        }
    }
    if output.contains('{') || output.contains('}') {
        return Err("unknown token".to_owned());
    }
    Ok(sanitize_basename(&output))
}

/// Produces only typed relative rename intents. The host owns preview, identity recheck and commit.
pub fn build_rename_plan(
    root: OperationObjectHandleV1,
    destination_parent: OperationObjectHandleV1,
    files: &[(OperationObjectHandleV1, ExifMetadata)],
    pattern: &str,
) -> Result<OperationPlanV1, String> {
    let mut destinations = BTreeSet::new();
    let mut steps = Vec::with_capacity(files.len());
    for (source, metadata) in files {
        let basename = render_pattern(pattern, metadata)?;
        let destination = if metadata.extension.is_empty() {
            basename
        } else {
            format!("{basename}.{}", metadata.extension)
        };
        if !destinations.insert(destination.to_lowercase()) {
            return Err(format!("case-insensitive target collision: {destination}"));
        }
        steps.push(OperationStepV1 {
            kind: OperationKindV1::RENAME,
            source: ROption::RSome(*source),
            destination_parent: ROption::RSome(destination_parent),
            destination_name: ROption::RSome(destination.into()),
            expected_source: ROption::RNone,
        });
    }
    Ok(OperationPlanV1 {
        title: "Rename from EXIF".into(),
        root,
        steps: steps.into(),
        confirmation_threshold: 1_000,
        undo_requested: true,
    })
}

struct Registrar;
impl ExtensionRegistrarImplementationV1 for Registrar {
    fn create() -> Self {
        Self
    }
    fn register(&self, _: RegistrarRequestV1) -> RegistrarOutputResultV1 {
        let kinds = [
            (
                "rust-exif-rename:command",
                RegisteredContributionKindV1::COMMAND,
            ),
            (
                "rust-exif-rename:plan",
                RegisteredContributionKindV1::OPERATION_PLAN,
            ),
        ];
        RResult::ROk(RegistrarOutputV1 {
            outcome: RegistrationOutcomeV1::accepted(2),
            contributions: kinds
                .into_iter()
                .map(|(id, kind)| RegisteredContributionV1 {
                    feature_id: "rust-exif-rename".into(),
                    contribution_id: id.into(),
                    kind,
                    required_capabilities: vec![
                        "filesystem.read".into(),
                        "filesystem.write".into(),
                    ]
                    .into(),
                    interface_id: INTERFACE_ID,
                    expected_sort: ROption::RNone,
                    opaque_contract: ROption::RNone,
                    renderer_contribution_id: ROption::RNone,
                    provider: ROption::RNone,
                    visual_column: ROption::RNone,
                    size_map_view: ROption::RNone,
                    virtual_folder_provider: ROption::RNone,
                    batch_column_provider: ROption::RNone,
                })
                .collect::<Vec<_>>()
                .into(),
        })
    }
}

#[export_root_module]
pub fn plugin_root() -> ExtensionRootModuleV1_Ref {
    ExtensionRootModuleV1::new::<Registrar>(
        PluginMetadataV1 {
            plugin_id: PLUGIN_ID,
            primary_interface_id: INTERFACE_ID,
        },
        ROption::RNone,
    )
    .leak_into_prefix()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata(name: &str) -> ExifMetadata {
        ExifMetadata {
            rawname: name.into(),
            extension: "jpg".into(),
            x_resolution: Some(72.5),
            y_resolution: Some(300.0),
            pixel_x_dimension: Some(800),
            pixel_y_dimension: Some(600),
            date_time_original: Some("2026:08:04 12:34:56".into()),
        }
    }

    #[test]
    fn renders_exact_dimension_and_rational_tokens() {
        let value = render_pattern(
            "{rawname}_{PixelXDimension}x{PixelYDimension}_{XResolution}_{YResolution}",
            &metadata("照片"),
        )
        .unwrap();
        assert_eq!(value, "照片_800x600_72.5_300");
    }

    #[test]
    fn missing_token_blocks_preview_and_names_are_sanitized() {
        let mut value = metadata("a");
        value.date_time_original = None;
        assert!(render_pattern("{DateTimeOriginal}", &value).is_err());
        assert_eq!(sanitize_basename(" a:b. "), "a_b");
    }

    #[test]
    fn plan_rejects_case_insensitive_collisions() {
        let a = metadata("same");
        let b = metadata("SAME");
        let root = OperationObjectHandleV1::new([1; 16], 1);
        assert!(build_rename_plan(
            root,
            root,
            &[
                (OperationObjectHandleV1::new([2; 16], 1), a),
                (OperationObjectHandleV1::new([3; 16], 1), b)
            ],
            "{rawname}"
        )
        .is_err());
    }
}
