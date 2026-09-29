//! Bounded plain-text and image-property previews, loaded outside the UI thread.

use explorer_common::archive::{ArchivePath, read_member};
use explorer_model::{FileEntry, LocationDescriptor};
use image::ImageDecoder as _;
use std::{
    io::{Cursor, Read},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{self, Receiver},
    },
};

pub const TEXT_CHARACTER_LIMIT: usize = 3_000;
const TEXT_BYTE_LIMIT: usize = 16 * 1024;
const IMAGE_METADATA_LIMIT: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PreviewContent {
    Loading,
    Text {
        text: String,
        truncated: bool,
        encoding: String,
    },
    Image {
        width: u32,
        height: u32,
        bits_per_pixel: u16,
        color: String,
        size: Option<u64>,
        exif: Vec<(String, String)>,
        metadata_note: Option<String>,
    },
    Failed(String),
}

pub fn plain_text(location: &LocationDescriptor) -> bool {
    location
        .path()
        .and_then(Path::extension)
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.eq_ignore_ascii_case("txt") || s.eq_ignore_ascii_case("md"))
}

pub fn image_file(location: &LocationDescriptor) -> bool {
    location
        .path()
        .and_then(Path::extension)
        .and_then(|s| s.to_str())
        .is_some_and(|s| {
            matches!(
                s.to_ascii_lowercase().as_str(),
                "jpg" | "jpeg" | "png" | "bmp" | "gif" | "webp" | "tif" | "tiff"
            )
        })
}

fn decode_text(bytes: &[u8], more_bytes: bool) -> Result<PreviewContent, String> {
    let (text, encoding) = if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        let little = bytes[0] == 0xff;
        let mut units: Vec<_> = bytes[2..]
            .chunks_exact(2)
            .map(|s| {
                if little {
                    u16::from_le_bytes([s[0], s[1]])
                } else {
                    u16::from_be_bytes([s[0], s[1]])
                }
            })
            .collect();
        if more_bytes && units.last().is_some_and(|s| (0xd800..=0xdbff).contains(s)) {
            units.pop();
        }
        if !more_bytes && (bytes.len() - 2) % 2 != 0 {
            return Err("UTF-16 文字不完整".to_owned());
        }
        (
            String::from_utf16(&units).map_err(|_| "UTF-16 編碼有誤")?,
            if little { "UTF-16 LE" } else { "UTF-16 BE" },
        )
    } else {
        let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
        match std::str::from_utf8(bytes) {
            Ok(text) => (text.to_owned(), "UTF-8"),
            Err(error) if more_bytes && error.error_len().is_none() => (
                std::str::from_utf8(&bytes[..error.valid_up_to()])
                    .map_err(|_| "UTF-8 編碼有誤")?
                    .to_owned(),
                "UTF-8",
            ),
            Err(_) => {
                let legacy = explorer_common::archive::decode_legacy_text(bytes)
                    .or_else(|error| {
                        if more_bytes && !bytes.is_empty() {
                            explorer_common::archive::decode_legacy_text(&bytes[..bytes.len() - 1])
                        } else {
                            Err(error)
                        }
                    })
                    .map_err(|_| "文字編碼無法辨識；支援 UTF-8、UTF-16（含 BOM）與 Windows ANSI")?;
                (legacy, "Windows ANSI")
            }
        }
    };
    if text.chars().any(|c| c == '\0') {
        return Err("檔案包含二進位內容，無法以純文字預覽".to_owned());
    }
    let truncated = more_bytes || text.chars().count() > TEXT_CHARACTER_LIMIT;
    Ok(PreviewContent::Text {
        text: text.chars().take(TEXT_CHARACTER_LIMIT).collect(),
        truncated,
        encoding: encoding.to_owned(),
    })
}

fn stored_color(bytes: &[u8], decoded: image::ExtendedColorType) -> (u16, String) {
    // Palette/low-bit-depth decoders expand pixels for rendering. Report the file's
    // stored depth instead of incorrectly labelling every thumbnail as RGBA32.
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") && bytes.len() >= 26 {
        let depth = u16::from(bytes[24]);
        let (channels, name) = match bytes[25] {
            0 => (1, "Gray"),
            2 => (3, "RGB"),
            3 => (1, "Indexed"),
            4 => (2, "Gray+Alpha"),
            6 => (4, "RGBA"),
            _ => return (decoded.bits_per_pixel(), format!("{decoded:?}")),
        };
        return (depth * channels, format!("{name}{depth}"));
    }
    if (bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a")) && bytes.len() >= 11 {
        let depth = if bytes[10] & 0x80 != 0 {
            u16::from((bytes[10] & 7) + 1)
        } else {
            8
        };
        return (depth, format!("Indexed{depth}"));
    }
    if bytes.starts_with(b"BM") && bytes.len() >= 30 {
        let dib = u32::from_le_bytes([bytes[14], bytes[15], bytes[16], bytes[17]]);
        let offset = if dib == 12 { 24 } else { 28 };
        let depth = u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
        if matches!(depth, 1 | 4 | 8 | 16 | 24 | 32) {
            return (
                depth,
                if depth <= 8 {
                    format!("Indexed{depth}")
                } else {
                    format!("{decoded:?}")
                },
            );
        }
    }
    (decoded.bits_per_pixel(), format!("{decoded:?}"))
}

fn image_properties(bytes: &[u8], size: Option<u64>) -> Result<PreviewContent, String> {
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(64 * 1024 * 1024);
    limits.max_image_width = Some(32_768);
    limits.max_image_height = Some(32_768);
    reader.limits(limits);
    let mut decoder = reader
        .into_decoder()
        .map_err(|e| format!("無法讀取圖片資訊：{e}"))?;
    let (width, height) = decoder.dimensions();
    let original_color = decoder.original_color_type();
    let (bits_per_pixel, color) = stored_color(bytes, original_color);
    let (exif, metadata_note) = match decoder.exif_metadata() {
        Ok(Some(raw)) => match exif::Reader::new().read_raw(raw) {
            Ok(exif) => (
                exif.fields()
                    .take(256)
                    .map(|field| {
                        (
                            format!("{} ({})", field.tag, field.ifd_num),
                            field
                                .display_value()
                                .with_unit(&exif)
                                .to_string()
                                .chars()
                                .take(1_024)
                                .collect(),
                        )
                    })
                    .collect(),
                None,
            ),
            Err(_) => (Vec::new(), Some("EXIF 資料損壞或無法解析".to_owned())),
        },
        Ok(None) => (Vec::new(), Some("無 EXIF 資料".to_owned())),
        Err(_) => (
            Vec::new(),
            Some("EXIF 未提供，或超過資訊讀取上限".to_owned()),
        ),
    };
    Ok(PreviewContent::Image {
        width,
        height,
        bits_per_pixel,
        color,
        size,
        exif,
        metadata_note,
    })
}

fn load(entry: &FileEntry, cancelled: &AtomicBool) -> PreviewContent {
    let result = (|| -> Result<PreviewContent, String> {
        let path = entry.location.path().ok_or("無法讀取此位置")?;
        let text = plain_text(&entry.location);
        let cap = if text {
            TEXT_BYTE_LIMIT
        } else {
            IMAGE_METADATA_LIMIT
        };
        let (bytes, size) = if let Some(archive) = ArchivePath::resolve(path) {
            let bytes = read_member(&archive, cap + 1, true, &|| {
                cancelled.load(Ordering::Acquire)
            })
            .map_err(|e| e.to_string())?;
            (bytes, entry.metadata.size_bytes)
        } else {
            let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
            let size = file.metadata().ok().map(|m| m.len());
            let mut bytes = Vec::new();
            file.take((cap + 1) as u64)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            (bytes, size)
        };
        let more = bytes.len() > cap;
        let bytes = &bytes[..bytes.len().min(cap)];
        if text {
            decode_text(bytes, more)
        } else {
            image_properties(bytes, size)
        }
    })();
    result.unwrap_or_else(PreviewContent::Failed)
}

struct Pending {
    entry: FileEntry,
    cancelled: Arc<AtomicBool>,
}

#[derive(Default)]
pub(crate) struct PreviewContentController {
    key: Option<String>,
    cancelled: Option<Arc<AtomicBool>>,
    receiver: Option<Receiver<PreviewContent>>,
    pending: Option<Pending>,
    workers: Arc<AtomicUsize>,
    content: Option<PreviewContent>,
}

impl PreviewContentController {
    pub(crate) fn content(&self) -> Option<PreviewContent> {
        self.content.clone()
    }

    pub(crate) fn synchronize(&mut self, entry: Option<&FileEntry>, epoch: &str) {
        let desired = entry.filter(|entry| {
            !entry.is_container
                && !crate::column_view::offline_placeholder(entry)
                && (plain_text(&entry.location) || image_file(&entry.location))
        });
        let key = desired.map(|entry| {
            format!(
                "{epoch}:{:?}:{}:{:?}:{:?}",
                entry.id,
                entry.location.editable_text(),
                entry.metadata.size_bytes,
                entry.metadata.modified_sort_key
            )
        });
        if self.key == key {
            return;
        }
        if let Some(cancelled) = self.cancelled.take() {
            cancelled.store(true, Ordering::Release);
        }
        self.receiver = None;
        self.pending = None;
        self.content = None;
        self.key = key;
        if let Some(entry) = desired {
            let cancelled = Arc::new(AtomicBool::new(false));
            self.cancelled = Some(cancelled.clone());
            self.content = Some(PreviewContent::Loading);
            self.pending = Some(Pending {
                entry: entry.clone(),
                cancelled,
            });
            self.start_pending();
        }
    }

    fn start_pending(&mut self) {
        if self.pending.is_none()
            || self
                .workers
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                    (n < 2).then_some(n + 1)
                })
                .is_err()
        {
            return;
        }
        let Some(pending) = self.pending.take() else {
            self.workers.fetch_sub(1, Ordering::AcqRel);
            return;
        };
        let workers = self.workers.clone();
        let (sender, receiver) = mpsc::sync_channel(1);
        self.receiver = Some(receiver);
        std::thread::spawn(move || {
            struct Slot(Arc<AtomicUsize>);
            impl Drop for Slot {
                fn drop(&mut self) {
                    self.0.fetch_sub(1, Ordering::AcqRel);
                }
            }
            let _slot = Slot(workers);
            let content = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                load(&pending.entry, &pending.cancelled)
            }))
            .unwrap_or_else(|_| PreviewContent::Failed("無法解析檔案預覽".to_owned()));
            if !pending.cancelled.load(Ordering::Acquire) {
                let _ = sender.send(content);
            }
        });
    }

    pub(crate) fn poll(&mut self) -> bool {
        self.start_pending();
        if let Some(receiver) = self.receiver.as_ref()
            && let Ok(content) = receiver.try_recv()
        {
            self.content = Some(content);
            self.receiver = None;
            return true;
        }
        false
    }
}

impl Drop for PreviewContentController {
    fn drop(&mut self) {
        if let Some(cancelled) = &self.cancelled {
            cancelled.store(true, Ordering::Release);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_text_is_plain_and_bounded_by_unicode_characters() {
        let source = format!("# 標題\n<script>raw</script>\n{}", "繁體😀".repeat(2_000));
        let PreviewContent::Text {
            text, truncated, ..
        } = decode_text(source.as_bytes(), false).unwrap()
        else {
            panic!()
        };
        assert_eq!(text.chars().count(), 3_000);
        assert!(truncated);
        assert!(text.starts_with("# 標題\n<script>raw</script>"));
    }

    #[test]
    fn preview_text_handles_utf16_bom_exact_limit_and_empty_files() {
        for little in [true, false] {
            let source = "文字😀".repeat(1_000);
            let mut bytes = if little {
                vec![0xff, 0xfe]
            } else {
                vec![0xfe, 0xff]
            };
            for unit in source.encode_utf16() {
                bytes.extend_from_slice(&if little {
                    unit.to_le_bytes()
                } else {
                    unit.to_be_bytes()
                });
            }
            let PreviewContent::Text {
                text, truncated, ..
            } = decode_text(&bytes, false).unwrap()
            else {
                panic!()
            };
            assert_eq!(text, source);
            assert!(!truncated);
        }
        assert!(matches!(
            decode_text(b"", false).unwrap(),
            PreviewContent::Text {
                truncated: false,
                ..
            }
        ));
        assert!(decode_text(b"abc\0def", false).is_err());
        assert!(decode_text(&[0xff, 0xfe, 0], false).is_err());
    }

    #[test]
    fn preview_image_reports_original_color_depth_dimensions_and_size() {
        let mut output = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba16(image::ImageBuffer::new(17, 23))
            .write_to(&mut output, image::ImageFormat::Png)
            .unwrap();
        let bytes = output.into_inner();
        let PreviewContent::Image {
            width,
            height,
            bits_per_pixel,
            size,
            metadata_note,
            ..
        } = image_properties(&bytes, Some(bytes.len() as u64)).unwrap()
        else {
            panic!()
        };
        assert_eq!((width, height, bits_per_pixel), (17, 23, 64));
        assert_eq!(size, Some(bytes.len() as u64));
        assert!(metadata_note.is_some());
    }

    #[test]
    fn preview_color_depth_uses_stored_palette_and_grayscale_headers() {
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        png.resize(26, 0);
        png[24] = 4;
        png[25] = 3;
        assert_eq!(
            stored_color(&png, image::ExtendedColorType::Rgba8),
            (4, "Indexed4".to_owned())
        );
        png[24] = 1;
        png[25] = 0;
        assert_eq!(
            stored_color(&png, image::ExtendedColorType::L8),
            (1, "Gray1".to_owned())
        );
        let mut bmp = b"BM".to_vec();
        bmp.resize(30, 0);
        bmp[14] = 40;
        bmp[28] = 8;
        assert_eq!(
            stored_color(&bmp, image::ExtendedColorType::Rgb8),
            (8, "Indexed8".to_owned())
        );
        let mut gif = b"GIF89a".to_vec();
        gif.resize(11, 0);
        gif[10] = 0x87;
        assert_eq!(
            stored_color(&gif, image::ExtendedColorType::Rgba8),
            (8, "Indexed8".to_owned())
        );
    }

    #[test]
    fn preview_image_reads_exif_camera_fields_without_decoding_pixels() {
        let mut output = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(image::ImageBuffer::new(31, 47))
            .write_to(&mut output, image::ImageFormat::Jpeg)
            .unwrap();
        let mut jpeg = output.into_inner();
        let tiff = [
            b'I', b'I', 42, 0, 8, 0, 0, 0, 1, 0, 15, 1, 2, 0, 7, 0, 0, 0, 26, 0, 0, 0, 0, 0, 0, 0,
            b'C', b'a', b'm', b'e', b'r', b'a', 0,
        ];
        let mut segment = vec![0xff, 0xe1];
        segment.extend_from_slice(&((tiff.len() + 8) as u16).to_be_bytes());
        segment.extend_from_slice(b"Exif\0\0");
        segment.extend_from_slice(&tiff);
        jpeg.splice(2..2, segment);
        let PreviewContent::Image {
            width,
            height,
            exif,
            ..
        } = image_properties(&jpeg, Some(jpeg.len() as u64)).unwrap()
        else {
            panic!()
        };
        assert_eq!((width, height), (31, 47));
        assert!(
            exif.iter()
                .any(|(name, value)| name.starts_with("Make") && value.contains("Camera")),
            "{exif:?}"
        );
    }

    fn file_entry(path: &Path, id: u8) -> FileEntry {
        FileEntry {
            id: explorer_model::ShellItemId::from_provider_bytes([id]).unwrap(),
            display_name: path.file_name().unwrap().to_string_lossy().into_owned(),
            location: LocationDescriptor::file_system(path),
            is_container: false,
            metadata: Default::default(),
        }
    }

    fn wait_preview(controller: &mut PreviewContentController) {
        let start = std::time::Instant::now();
        while !controller.poll() {
            assert!(start.elapsed() < std::time::Duration::from_secs(5));
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    #[test]
    fn preview_controller_cancels_stale_selection_and_refreshes_same_item() {
        let temporary = tempfile::tempdir().unwrap();
        let first = file_entry(&temporary.path().join("first.txt"), 1);
        let last = file_entry(&temporary.path().join("last.md"), 2);
        std::fs::write(first.location.path().unwrap(), "OLD".repeat(50_000)).unwrap();
        std::fs::write(last.location.path().unwrap(), "# current\nraw **markdown**").unwrap();
        let mut controller = PreviewContentController::default();
        controller.synchronize(Some(&first), "tab:1");
        controller.synchronize(Some(&last), "tab:1");
        wait_preview(&mut controller);
        assert!(
            matches!(controller.content(), Some(PreviewContent::Text { text, .. }) if text == "# current\nraw **markdown**")
        );
        std::fs::write(last.location.path().unwrap(), "refreshed").unwrap();
        controller.synchronize(Some(&last), "tab:2");
        wait_preview(&mut controller);
        assert!(
            matches!(controller.content(), Some(PreviewContent::Text { text, .. }) if text == "refreshed")
        );
        controller.synchronize(Some(&first), "tab:2");
        controller.synchronize(None, "hidden");
        std::thread::sleep(std::time::Duration::from_millis(30));
        controller.poll();
        assert!(controller.content().is_none());
    }

    #[cfg(windows)]
    #[test]
    fn preview_text_and_image_properties_work_inside_archives() {
        use std::io::Write as _;
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("preview.zip");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        zip.start_file("docs/readme.md", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(format!("# raw\n{}", "文字😀".repeat(5_000)).as_bytes())
            .unwrap();
        let mut output = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(image::ImageBuffer::new(13, 29))
            .write_to(&mut output, image::ImageFormat::Png)
            .unwrap();
        let image = output.into_inner();
        zip.start_file("images/photo.png", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&image).unwrap();
        zip.finish().unwrap();
        let text = file_entry(&path.join("docs/readme.md"), 1);
        let PreviewContent::Text {
            text, truncated, ..
        } = load(&text, &AtomicBool::new(false))
        else {
            panic!("archive text failed")
        };
        assert_eq!(text.chars().count(), 3_000);
        assert!(text.starts_with("# raw\n"));
        assert!(truncated);
        let mut photo = file_entry(&path.join("images/photo.png"), 2);
        photo.metadata.size_bytes = Some(image.len() as u64);
        let PreviewContent::Image {
            width,
            height,
            bits_per_pixel,
            size,
            ..
        } = load(&photo, &AtomicBool::new(false))
        else {
            panic!("archive image failed")
        };
        assert_eq!(
            (width, height, bits_per_pixel, size),
            (13, 29, 24, Some(image.len() as u64))
        );
    }
}
