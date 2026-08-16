//! Versioned, handle-free disk cache for Windows Shell RGBA icon payloads.
#![allow(
    unsafe_code,
    reason = "reading the public Windows build registry value requires the Win32 registry API"
)]

use std::{
    fs::{self, OpenOptions},
    io::{self, Write as _},
    mem::size_of_val,
    os::windows::ffi::OsStrExt as _,
    path::{Path, PathBuf},
    sync::{
        OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::SystemTime,
};

use explorer_model::{
    Bc7RasterPayload, LocationDescriptor, ShellIconKey, ShellIconPayload, ShellIconTheme,
};

use crate::bc7_codec::{self, Bc7ContentKind};
use windows::{
    Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW},
    core::w,
};

const MAGIC: &[u8; 8] = b"RGXBC7C1";
const SCHEMA_VERSION: u16 = 6;
const LITTLE_ENDIAN_MARKER: u8 = 1;
const BC7_UNORM_FORMAT: u8 = 7;
const HEADER_LEN: usize = 8 + 2 + 1 + 1 + 1 + 1 + 8 + (5 * 4) + 8 + 4;
const MAX_ENTRY_BYTES: u64 = 16 * 1024 * 1024;
const DEFAULT_MAX_ENTRIES: usize = 4_096;
const DEFAULT_MAX_TOTAL_BYTES: u64 = 512 * 1024 * 1024;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static ICON_MAX_TOTAL_BYTES: AtomicU64 = AtomicU64::new(DEFAULT_MAX_TOTAL_BYTES);
static THUMBNAIL_MAX_TOTAL_BYTES: AtomicU64 = AtomicU64::new(1024 * 1024 * 1024);
static ICON_HITS: AtomicU64 = AtomicU64::new(0);
static ICON_MISSES: AtomicU64 = AtomicU64::new(0);
static ICON_REJECTIONS: AtomicU64 = AtomicU64::new(0);
static THUMBNAIL_HITS: AtomicU64 = AtomicU64::new(0);
static THUMBNAIL_MISSES: AtomicU64 = AtomicU64::new(0);
static THUMBNAIL_REJECTIONS: AtomicU64 = AtomicU64::new(0);
static ICON_BC7_ENABLED: AtomicBool = AtomicBool::new(false);
static THUMBNAIL_BC7_ENABLED: AtomicBool = AtomicBool::new(false);
#[cfg(test)]
pub(crate) static BC7_GATE_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ShellBc7RuntimeGatesV1 {
    pub icon_enabled: bool,
    pub thumbnail_enabled: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ShellDiskCacheStatsV1 {
    pub bytes: u64,
    pub limit_bytes: u64,
    pub entries: u64,
    pub hits: u64,
    pub misses: u64,
    pub rejections: u64,
}

#[derive(Clone, Debug)]
pub(crate) struct ShellIconDiskCache {
    root: Option<PathBuf>,
    max_entries: usize,
    max_total_bytes: u64,
    encoding: DiskCacheEncoding,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DiskCacheEncoding {
    Lossless,
    LossyQuality80,
}

pub(crate) enum DiskCacheLoad {
    Hit(Box<ShellIconPayload>),
    Miss,
    Rejected,
}

impl Default for ShellIconDiskCache {
    fn default() -> Self {
        let root = std::env::var_os("LOCALAPPDATA").map(|base| {
            PathBuf::from(base)
                .join("RustGpuiExplorer")
                .join("icon-cache")
                .join("v1")
        });
        Self {
            root,
            max_entries: DEFAULT_MAX_ENTRIES,
            max_total_bytes: ICON_MAX_TOTAL_BYTES.load(Ordering::Acquire),
            encoding: DiskCacheEncoding::Lossless,
        }
    }
}

impl ShellIconDiskCache {
    #[cfg(test)]
    pub(crate) fn with_root(root: PathBuf) -> Self {
        Self {
            root: Some(root),
            max_entries: DEFAULT_MAX_ENTRIES,
            max_total_bytes: ICON_MAX_TOTAL_BYTES.load(Ordering::Acquire),
            encoding: DiskCacheEncoding::Lossless,
        }
    }

    pub(crate) fn with_root_lossy_thumbnail(root: PathBuf) -> Self {
        Self {
            root: Some(root),
            max_entries: DEFAULT_MAX_ENTRIES,
            max_total_bytes: THUMBNAIL_MAX_TOTAL_BYTES.load(Ordering::Acquire),
            encoding: DiskCacheEncoding::LossyQuality80,
        }
    }

    pub(crate) fn clear(&self) -> io::Result<()> {
        let Some(root) = &self.root else {
            return Ok(());
        };
        match fs::remove_dir_all(root) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_limits(root: PathBuf, max_entries: usize, max_total_bytes: u64) -> Self {
        Self {
            root: Some(root),
            max_entries: max_entries.max(1),
            max_total_bytes: max_total_bytes.max(HEADER_LEN as u64 + 4),
            encoding: DiskCacheEncoding::Lossless,
        }
    }

    #[cfg(test)]
    fn with_thumbnail_limits(root: PathBuf, max_entries: usize, max_total_bytes: u64) -> Self {
        Self {
            root: Some(root),
            max_entries: max_entries.max(1),
            max_total_bytes: max_total_bytes.max(HEADER_LEN as u64 + 4),
            encoding: DiskCacheEncoding::LossyQuality80,
        }
    }

    pub(crate) fn load_outcome(&self, key: &ShellIconKey) -> DiskCacheLoad {
        let Some(path) = self.entry_path(key) else {
            self.counters().1.fetch_add(1, Ordering::Relaxed);
            return DiskCacheLoad::Miss;
        };
        match self.read_entry(&path, key) {
            Ok(Some(payload)) => {
                self.counters().0.fetch_add(1, Ordering::Relaxed);
                DiskCacheLoad::Hit(Box::new(payload))
            }
            Ok(None) => {
                self.counters().1.fetch_add(1, Ordering::Relaxed);
                DiskCacheLoad::Miss
            }
            Err(error) => {
                self.counters().2.fetch_add(1, Ordering::Relaxed);
                tracing::debug!(?error, "Shell icon disk cache entry was rejected");
                let _ = fs::remove_file(path);
                DiskCacheLoad::Rejected
            }
        }
    }

    fn counters(&self) -> (&'static AtomicU64, &'static AtomicU64, &'static AtomicU64) {
        match self.encoding {
            DiskCacheEncoding::Lossless => (&ICON_HITS, &ICON_MISSES, &ICON_REJECTIONS),
            DiskCacheEncoding::LossyQuality80 => {
                (&THUMBNAIL_HITS, &THUMBNAIL_MISSES, &THUMBNAIL_REJECTIONS)
            }
        }
    }

    fn content_kind(&self) -> Bc7ContentKind {
        match self.encoding {
            DiskCacheEncoding::Lossless => Bc7ContentKind::Icon,
            DiskCacheEncoding::LossyQuality80 => Bc7ContentKind::Thumbnail,
        }
    }

    fn stats(&self) -> ShellDiskCacheStatsV1 {
        let (hits, misses, rejections) = self.counters();
        let (bytes, entries) = self.root.as_ref().map_or((0, 0), |root| {
            fs::read_dir(root).map_or((0, 0), |entries| {
                entries
                    .flatten()
                    .filter_map(|entry| {
                        let metadata = fs::symlink_metadata(entry.path()).ok()?;
                        (metadata.is_file() && !metadata.file_type().is_symlink())
                            .then_some(metadata.len())
                    })
                    .fold((0_u64, 0_u64), |(bytes, count), length| {
                        (bytes.saturating_add(length), count.saturating_add(1))
                    })
            })
        });
        ShellDiskCacheStatsV1 {
            bytes,
            limit_bytes: self.max_total_bytes,
            entries,
            hits: hits.load(Ordering::Relaxed),
            misses: misses.load(Ordering::Relaxed),
            rejections: rejections.load(Ordering::Relaxed),
        }
    }

    #[cfg(test)]
    pub(crate) fn load(&self, key: &ShellIconKey) -> Option<ShellIconPayload> {
        match self.load_outcome(key) {
            DiskCacheLoad::Hit(payload) => Some(*payload),
            DiskCacheLoad::Miss | DiskCacheLoad::Rejected => None,
        }
    }

    #[cfg(test)]
    pub(crate) fn store(&self, payload: &ShellIconPayload) -> bool {
        self.store_if(payload, || true)
    }

    pub(crate) fn store_if(
        &self,
        payload: &ShellIconPayload,
        should_publish: impl FnOnce() -> bool,
    ) -> bool {
        match self.write_entry_if(payload, should_publish) {
            Ok(()) => true,
            Err(error) => {
                tracing::debug!(?error, "Shell icon disk cache write was skipped");
                false
            }
        }
    }

    fn entry_path(&self, key: &ShellIconKey) -> Option<PathBuf> {
        let root = self.root.as_ref()?;
        Some(root.join(format!("{:016x}.bc7cache", key_digest(key))))
    }

    fn read_entry(&self, path: &Path, key: &ShellIconKey) -> io::Result<Option<ShellIconPayload>> {
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "cache entry is not a regular file",
            ));
        }
        if metadata.len() < HEADER_LEN as u64 || metadata.len() > MAX_ENTRY_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "icon cache entry length is outside the bounded format",
            ));
        }
        let bytes = fs::read(path)?;
        if bytes.len() < HEADER_LEN {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "icon cache entry was truncated while being read",
            ));
        }
        if &bytes[..8] != MAGIC {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "bad magic"));
        }
        let mut cursor = 8;
        let schema = take_u16(&bytes, &mut cursor)?;
        let endianness = take_u8(&bytes, &mut cursor)?;
        let content_kind = take_u8(&bytes, &mut cursor)?;
        let format = take_u8(&bytes, &mut cursor)?;
        let reserved = take_u8(&bytes, &mut cursor)?;
        let digest = take_u64(&bytes, &mut cursor)?;
        let width = take_u32(&bytes, &mut cursor)?;
        let height = take_u32(&bytes, &mut cursor)?;
        let padded_width = take_u32(&bytes, &mut cursor)?;
        let padded_height = take_u32(&bytes, &mut cursor)?;
        let row_pitch = take_u32(&bytes, &mut cursor)?;
        let payload_len = take_u64(&bytes, &mut cursor)?;
        let checksum = take_u32(&bytes, &mut cursor)?;
        if schema != SCHEMA_VERSION
            || endianness != LITTLE_ENDIAN_MARKER
            || content_kind != self.content_kind() as u8
            || format != BC7_UNORM_FORMAT
            || reserved != 0
            || digest != key_digest(key)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "schema, endianness, kind, format, or invalidation identity mismatch",
            ));
        }
        let payload_len = usize::try_from(payload_len)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "payload too large"))?;
        if payload_len != bytes.len().saturating_sub(cursor) || crc32(&bytes[cursor..]) != checksum
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "payload length, stride, or checksum mismatch",
            ));
        }
        let layout = bc7_codec::checked_layout(width, height)?;
        if padded_width != layout.padded_width
            || padded_height != layout.padded_height
            || row_pitch != layout.row_pitch
            || payload_len != layout.payload_bytes
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "BC7 block layout mismatch",
            ));
        }
        ShellIconPayload::new_bc7(
            key.clone(),
            Bc7RasterPayload {
                kind: match self.content_kind() {
                    Bc7ContentKind::Icon => explorer_model::CompressedRasterKind::Icon,
                    Bc7ContentKind::Thumbnail => explorer_model::CompressedRasterKind::Thumbnail,
                },
                width,
                height,
                padded_width: layout.padded_width,
                padded_height: layout.padded_height,
                row_pitch: layout.row_pitch,
                blocks: bytes[cursor..].to_vec(),
            },
            None,
        )
        .map(Some)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, format!("{error:?}")))
    }

    fn write_entry_if(
        &self,
        payload: &ShellIconPayload,
        should_publish: impl FnOnce() -> bool,
    ) -> io::Result<()> {
        self.write_entry_impl(payload, false, should_publish)
    }

    #[cfg(test)]
    fn write_entry_interrupted_before_publish(&self, payload: &ShellIconPayload) -> io::Result<()> {
        self.write_entry_impl(payload, true, || true)
    }

    fn write_entry_impl(
        &self,
        payload: &ShellIconPayload,
        interrupt_before_publish: bool,
        should_publish: impl FnOnce() -> bool,
    ) -> io::Result<()> {
        let Some(path) = self.entry_path(&payload.key) else {
            return Ok(());
        };
        if path.is_file() {
            return Ok(());
        }
        let Some(root) = path.parent() else {
            return Ok(());
        };
        fs::create_dir_all(root)?;
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temporary = root.join(format!(".icon-{}-{sequence}.tmp", std::process::id()));
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        let result = (|| {
            file.write_all(MAGIC)?;
            file.write_all(&SCHEMA_VERSION.to_le_bytes())?;
            file.write_all(&[
                LITTLE_ENDIAN_MARKER,
                self.content_kind() as u8,
                BC7_UNORM_FORMAT,
                0,
            ])?;
            file.write_all(&key_digest(&payload.key).to_le_bytes())?;
            let encoded = if let Some(raster) = &payload.bc7 {
                bc7_codec::Bc7Raster {
                    kind: self.content_kind(),
                    width: raster.width,
                    height: raster.height,
                    padded_width: raster.padded_width,
                    padded_height: raster.padded_height,
                    row_pitch: raster.row_pitch,
                    blocks: raster.blocks.clone(),
                }
            } else {
                bc7_codec::encode_rgba(
                    self.content_kind(),
                    u32::from(payload.width),
                    u32::from(payload.height),
                    payload.stride,
                    &payload.rgba,
                )?
            };
            encoded.validate()?;
            file.write_all(&encoded.width.to_le_bytes())?;
            file.write_all(&encoded.height.to_le_bytes())?;
            file.write_all(&encoded.padded_width.to_le_bytes())?;
            file.write_all(&encoded.padded_height.to_le_bytes())?;
            file.write_all(&encoded.row_pitch.to_le_bytes())?;
            let payload_len = u64::try_from(encoded.blocks.len()).map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidInput, "icon payload is too large")
            })?;
            file.write_all(&payload_len.to_le_bytes())?;
            file.write_all(&crc32(&encoded.blocks).to_le_bytes())?;
            file.write_all(&encoded.blocks)?;
            file.sync_all()?;
            drop(file);
            if interrupt_before_publish {
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "injected interruption before atomic publish",
                ));
            }
            if !should_publish() {
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "BC7 publication was cancelled or superseded",
                ));
            }
            match fs::rename(&temporary, &path) {
                Ok(()) => Ok(()),
                Err(_error) if path.is_file() => Ok(()),
                Err(error) => Err(error),
            }
        })();
        if result.is_err() || temporary.exists() {
            let _ = fs::remove_file(temporary);
        }
        if result.is_ok() {
            self.cleanup(&path);
        }
        result
    }

    fn cleanup(&self, keep: &Path) {
        let Some(root) = self.root.as_ref() else {
            return;
        };
        let Ok(entries) = fs::read_dir(root) else {
            return;
        };
        let mut candidates = entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let path = entry.path();
                matches!(
                    path.extension().and_then(|value| value.to_str()),
                    Some("bc7cache" | "webp" | "rgba")
                )
                .then(|| {
                    let metadata = fs::symlink_metadata(entry.path()).ok()?;
                    (metadata.is_file() && !metadata.file_type().is_symlink()).then_some((
                        path,
                        metadata.len(),
                        metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                    ))
                })
                .flatten()
            })
            .collect::<Vec<_>>();
        let mut total = candidates.iter().map(|(_, size, _)| *size).sum::<u64>();
        if candidates.len() <= self.max_entries && total <= self.max_total_bytes {
            return;
        }
        candidates.sort_by_key(|(_, _, modified)| *modified);
        let mut count = candidates.len();
        for (path, size, _) in candidates {
            if count <= self.max_entries && total <= self.max_total_bytes {
                break;
            }
            if path == keep {
                continue;
            }
            if fs::remove_file(path).is_ok() {
                count = count.saturating_sub(1);
                total = total.saturating_sub(size);
            }
        }
    }
}

pub fn set_shell_disk_cache_limits(icon_bytes: u64, thumbnail_bytes: u64) {
    ICON_MAX_TOTAL_BYTES.store(icon_bytes.max(64 * 1024 * 1024), Ordering::Release);
    THUMBNAIL_MAX_TOTAL_BYTES.store(thumbnail_bytes.max(128 * 1024 * 1024), Ordering::Release);
    ShellIconDiskCache::default().cleanup(Path::new(""));
    let root = std::env::var_os("LOCALAPPDATA").map(|base| {
        PathBuf::from(base)
            .join("RustGpuiExplorer")
            .join("thumbnail-cache")
            .join("v1")
    });
    if let Some(root) = root {
        ShellIconDiskCache::with_root_lossy_thumbnail(root).cleanup(Path::new(""));
    }
}

/// Applies the independently persisted BC7 rollout gates. Both gates are deny-by-default and
/// changing either one never mutates its sibling. Callers that observe a disabled gate use the
/// provider-backed RGBA path and do not admit new compressed entries.
pub fn set_shell_bc7_runtime_gates(icon_enabled: bool, thumbnail_enabled: bool) {
    ICON_BC7_ENABLED.store(icon_enabled, Ordering::Release);
    THUMBNAIL_BC7_ENABLED.store(thumbnail_enabled, Ordering::Release);
}

pub fn shell_bc7_runtime_gates() -> ShellBc7RuntimeGatesV1 {
    ShellBc7RuntimeGatesV1 {
        icon_enabled: ICON_BC7_ENABLED.load(Ordering::Acquire),
        thumbnail_enabled: THUMBNAIL_BC7_ENABLED.load(Ordering::Acquire),
    }
}

pub(crate) fn icon_bc7_enabled() -> bool {
    ICON_BC7_ENABLED.load(Ordering::Acquire)
}

pub(crate) fn thumbnail_bc7_enabled() -> bool {
    THUMBNAIL_BC7_ENABLED.load(Ordering::Acquire)
}

pub fn icon_disk_cache_stats() -> ShellDiskCacheStatsV1 {
    ShellIconDiskCache::default().stats()
}

pub fn thumbnail_disk_cache_stats() -> ShellDiskCacheStatsV1 {
    let root = std::env::var_os("LOCALAPPDATA")
        .map_or_else(std::env::temp_dir, PathBuf::from)
        .join("RustGpuiExplorer")
        .join("thumbnail-cache")
        .join("v1");
    ShellIconDiskCache::with_root_lossy_thumbnail(root).stats()
}

fn key_digest(key: &ShellIconKey) -> u64 {
    key_digest_for_build(key, windows_build())
}

fn key_digest_for_build(key: &ShellIconKey, build: &str) -> u64 {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&SCHEMA_VERSION.to_le_bytes());
    bytes.extend_from_slice(build.as_bytes());
    if let Some(item_id) = &key.item_id {
        bytes.push(1);
        bytes.extend_from_slice(item_id.provider_bytes());
    } else {
        bytes.push(0);
    }
    match &key.location {
        LocationDescriptor::FileSystem(path) => {
            bytes.push(1);
            for word in path.as_os_str().encode_wide() {
                bytes.extend_from_slice(&word.to_le_bytes());
            }
        }
        LocationDescriptor::ShellNamespace(value) => {
            bytes.push(2);
            bytes.extend_from_slice(value);
        }
        LocationDescriptor::ParsingName(value) => {
            bytes.push(3);
            bytes.extend_from_slice(value.as_bytes());
        }
        LocationDescriptor::KnownFolder(value) => {
            bytes.push(4);
            bytes.extend_from_slice(value);
        }
        LocationDescriptor::Virtual(value) => {
            bytes.push(5);
            bytes.extend_from_slice(value.provider_id.as_bytes());
            bytes.extend_from_slice(&value.container_identity);
            bytes.extend_from_slice(&value.container_generation.to_le_bytes());
            bytes.extend_from_slice(&value.entry_id.unwrap_or_default().to_le_bytes());
            for component in &value.components {
                bytes.extend_from_slice(&(component.len() as u64).to_le_bytes());
                bytes.extend_from_slice(component.as_bytes());
            }
        }
    }
    bytes.extend_from_slice(&key.size_bucket.to_le_bytes());
    bytes.extend_from_slice(&key.dpi.to_le_bytes());
    bytes.push(match key.theme {
        ShellIconTheme::Light => 1,
        ShellIconTheme::Dark => 2,
        ShellIconTheme::HighContrast => 3,
    });
    bytes.extend_from_slice(&key.association_generation.to_le_bytes());
    bytes.extend_from_slice(&key.overlay_generation.to_le_bytes());
    fnv1a64(&bytes)
}

fn windows_build() -> &'static str {
    static BUILD: OnceLock<String> = OnceLock::new();
    BUILD.get_or_init(|| {
        let mut buffer = [0_u16; 64];
        let mut byte_count = u32::try_from(size_of_val(&buffer)).unwrap_or(u32::MAX);
        // SAFETY: the predefined HKLM handle is borrowed, both strings are static NUL-terminated
        // literals, and the writable buffer/byte count remain valid for the synchronous call.
        let status = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                w!(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion"),
                w!("CurrentBuildNumber"),
                RRF_RT_REG_SZ,
                None,
                Some(buffer.as_mut_ptr().cast()),
                Some(&raw mut byte_count),
            )
        };
        if status.is_ok() {
            let length = buffer
                .iter()
                .position(|word| *word == 0)
                .unwrap_or(buffer.len());
            String::from_utf16_lossy(&buffer[..length])
        } else {
            "unknown-build".to_owned()
        }
    })
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0_u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}

fn take_u16(bytes: &[u8], cursor: &mut usize) -> io::Result<u16> {
    Ok(u16::from_le_bytes(take(bytes, cursor)?))
}

fn take_u8(bytes: &[u8], cursor: &mut usize) -> io::Result<u8> {
    Ok(take::<1>(bytes, cursor)?[0])
}

fn take_u32(bytes: &[u8], cursor: &mut usize) -> io::Result<u32> {
    Ok(u32::from_le_bytes(take(bytes, cursor)?))
}

fn take_u64(bytes: &[u8], cursor: &mut usize) -> io::Result<u64> {
    Ok(u64::from_le_bytes(take(bytes, cursor)?))
}

fn take<const N: usize>(bytes: &[u8], cursor: &mut usize) -> io::Result<[u8; N]> {
    let end = cursor
        .checked_add(N)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "header overflow"))?;
    let slice = bytes
        .get(*cursor..end)
        .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "truncated header"))?;
    *cursor = end;
    slice
        .try_into()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid header field"))
}

#[cfg(test)]
mod tests {
    use explorer_model::{LocationDescriptor, ShellIconKey, ShellIconPayload, ShellIconTheme};

    use super::{
        ShellBc7RuntimeGatesV1, ShellIconDiskCache, key_digest_for_build,
        set_shell_bc7_runtime_gates, shell_bc7_runtime_gates,
    };

    type Corruption = (&'static str, Box<dyn Fn(&mut Vec<u8>)>);

    fn key(generation: u64) -> ShellIconKey {
        ShellIconKey {
            item_id: None,
            location: LocationDescriptor::file_system(r"D:\fixture\資料夾"),
            size_bucket: 32,
            dpi: 175,
            theme: ShellIconTheme::Light,
            association_generation: generation,
            overlay_generation: generation,
        }
    }

    #[test]
    fn bc7_round_trip_preserves_identity_geometry_and_generation() {
        let root = tempfile::tempdir().expect("temp cache");
        let cache = ShellIconDiskCache::with_root(root.path().to_path_buf());
        let payload =
            ShellIconPayload::new(key(1), 2, 2, 8, (0..16).collect(), None).expect("valid payload");
        cache.store(&payload);
        let loaded = cache.load(&key(1)).expect("BC7 hit");
        let bc7 = loaded.bc7.expect("compressed blocks");
        assert_eq!(
            (bc7.width, bc7.height, bc7.padded_width, bc7.padded_height),
            (2, 2, 4, 4)
        );
        assert_eq!(bc7.blocks.len(), 16);
        assert!(cache.load(&key(2)).is_none());
        let overlay_only = ShellIconKey {
            overlay_generation: 2,
            ..key(1)
        };
        assert!(cache.load(&overlay_only).is_none());
    }

    #[test]
    fn unknown_pixel_format_is_rejected_instead_of_decoded_as_rgba() {
        let root = tempfile::tempdir().expect("temp cache");
        let cache = ShellIconDiskCache::with_root(root.path().to_path_buf());
        let payload =
            ShellIconPayload::new(key(7), 1, 1, 4, vec![1, 2, 3, 4], None).expect("valid payload");
        cache.store(&payload);
        let entry = std::fs::read_dir(root.path())
            .expect("read cache")
            .next()
            .expect("one entry")
            .expect("valid entry")
            .path();
        let mut bytes = std::fs::read(&entry).expect("read entry");
        let pixel_format_offset = 8 + 2 + 1 + 1;
        bytes[pixel_format_offset] = 0xff;
        std::fs::write(&entry, bytes).expect("replace format field");
        assert!(cache.load(&key(7)).is_none());
        assert!(!entry.exists());
    }

    #[test]
    fn corrupt_entry_is_rejected_and_removed() {
        let root = tempfile::tempdir().expect("temp cache");
        let cache = ShellIconDiskCache::with_root(root.path().to_path_buf());
        let payload =
            ShellIconPayload::new(key(3), 1, 1, 4, vec![1, 2, 3, 4], None).expect("valid payload");
        cache.store(&payload);
        let entry = std::fs::read_dir(root.path())
            .expect("read cache")
            .next()
            .expect("one entry")
            .expect("valid entry")
            .path();
        std::fs::write(&entry, b"corrupt").expect("replace entry");
        assert!(cache.load(&key(3)).is_none());
        assert!(!entry.exists());
    }

    #[test]
    fn oversized_and_symlink_entries_are_rejected_without_following_outside_targets() {
        let root = tempfile::tempdir().expect("temp cache");
        let cache = ShellIconDiskCache::with_root(root.path().to_path_buf());
        let oversized_key = key(61);
        let oversized = cache.entry_path(&oversized_key).expect("entry path");
        std::fs::File::create(&oversized)
            .and_then(|file| file.set_len(super::MAX_ENTRY_BYTES + 1))
            .expect("oversized fixture");
        assert!(matches!(
            cache.load_outcome(&oversized_key),
            super::DiskCacheLoad::Rejected
        ));
        assert!(!oversized.exists());

        let linked_key = key(62);
        let linked = cache.entry_path(&linked_key).expect("entry path");
        let outside = root.path().with_extension("outside-target");
        std::fs::create_dir(&outside).expect("outside fixture");
        let sentinel = outside.join("sentinel.txt");
        std::fs::write(&sentinel, b"outside must remain untouched").expect("outside fixture");
        let junction = std::process::Command::new("cmd.exe")
            .args(["/d", "/c", "mklink", "/J"])
            .arg(&linked)
            .arg(&outside)
            .output()
            .expect("junction command");
        assert!(
            junction.status.success(),
            "junction fixture failed: {}",
            String::from_utf8_lossy(&junction.stderr)
        );
        assert!(matches!(
            cache.load_outcome(&linked_key),
            super::DiskCacheLoad::Rejected
        ));
        assert_eq!(
            std::fs::read(&sentinel).expect("outside remains"),
            b"outside must remain untouched"
        );
        if linked.exists() {
            std::fs::remove_dir(&linked).expect("remove junction fixture");
        }
        std::fs::remove_dir_all(outside).expect("remove outside fixture");
    }

    #[test]
    fn container_rejects_every_bounded_header_and_payload_corruption_class() {
        let root = tempfile::tempdir().expect("temp cache");
        let cache = ShellIconDiskCache::with_root(root.path().to_path_buf());
        let cache_key = key(70);
        let payload =
            ShellIconPayload::new(cache_key.clone(), 5, 7, 20, vec![0x80; 5 * 7 * 4], None)
                .expect("valid payload");
        assert!(cache.store(&payload));
        let entry = cache.entry_path(&cache_key).expect("cache path");
        let valid = std::fs::read(&entry).expect("valid container");

        let corruptions: [Corruption; 13] = [
            ("magic", Box::new(|bytes| bytes[0] ^= 0xff)),
            (
                "schema",
                Box::new(|bytes| bytes[8..10].copy_from_slice(&0_u16.to_le_bytes())),
            ),
            ("endianness", Box::new(|bytes| bytes[10] = 2)),
            ("kind", Box::new(|bytes| bytes[11] = 2)),
            ("format", Box::new(|bytes| bytes[12] = 0xff)),
            ("reserved", Box::new(|bytes| bytes[13] = 1)),
            ("identity", Box::new(|bytes| bytes[14] ^= 1)),
            ("zero-width", Box::new(|bytes| bytes[22..26].fill(0))),
            ("padded-width", Box::new(|bytes| bytes[30..34].fill(0))),
            ("pitch", Box::new(|bytes| bytes[38..42].fill(0))),
            ("length", Box::new(|bytes| bytes[42..50].fill(0))),
            ("checksum", Box::new(|bytes| bytes[50] ^= 1)),
            ("trailing-data", Box::new(|bytes| bytes.push(0))),
        ];
        for (name, corrupt) in corruptions {
            let mut bytes = valid.clone();
            corrupt(&mut bytes);
            std::fs::write(&entry, bytes).expect("write corrupt fixture");
            assert!(cache.load(&cache_key).is_none(), "accepted {name}");
            assert!(!entry.exists(), "rejected {name} must be removed");
        }
    }

    #[test]
    fn thumbnail_bc7_round_trips_bounded_dimensions() {
        let root = tempfile::tempdir().expect("temp cache");
        let cache = ShellIconDiskCache::with_root_lossy_thumbnail(root.path().to_path_buf());
        let payload =
            ShellIconPayload::new(key(31), 2, 1, 8, vec![255, 0, 0, 255, 0, 255, 0, 255], None)
                .expect("valid thumbnail payload");
        assert!(cache.store(&payload));
        let loaded = cache.load(&key(31)).expect("BC7 hit");
        let raster = loaded.bc7.expect("compressed raster");
        assert_eq!(
            (
                raster.width,
                raster.height,
                raster.padded_width,
                raster.padded_height
            ),
            (2, 1, 4, 4)
        );
        assert_eq!(raster.row_pitch, 16);
        assert_eq!(raster.blocks.len(), 16);
        let entry = std::fs::read_dir(root.path())
            .expect("read cache")
            .next()
            .expect("one entry")
            .expect("valid entry")
            .path();
        assert_eq!(
            entry.extension().and_then(|value| value.to_str()),
            Some("bc7cache")
        );
    }

    #[test]
    fn legacy_rgba_entry_is_a_lazy_miss_without_startup_conversion() {
        let root = tempfile::tempdir().expect("temp cache");
        let legacy = root.path().join("deadbeef.rgba");
        std::fs::write(&legacy, b"RGXICON1 legacy raw pixels").expect("legacy fixture");
        let cache = ShellIconDiskCache::with_root(root.path().to_path_buf());
        assert!(cache.load(&key(44)).is_none());
        assert!(
            legacy.exists(),
            "lookup must not bulk-convert or delete unrelated legacy entries"
        );
    }

    #[test]
    fn normal_quota_cleanup_removes_obsolete_rgba_without_crossing_root() {
        let root = tempfile::tempdir().expect("cache root");
        let legacy = root.path().join("old.rgba");
        std::fs::write(&legacy, vec![0_u8; 4_096]).expect("legacy fixture");
        let outside = root.path().with_extension("outside.rgba");
        std::fs::write(&outside, vec![0_u8; 4_096]).expect("outside fixture");
        let cache = ShellIconDiskCache::with_limits(root.path().to_path_buf(), 1, 512);
        let payload =
            ShellIconPayload::new(key(91), 4, 4, 16, vec![0x80; 64], None).expect("valid payload");
        assert!(cache.store(&payload));
        assert!(
            !legacy.exists(),
            "bounded quota cleanup should remove obsolete cache data"
        );
        assert!(
            outside.exists(),
            "cleanup must not cross the registered cache root"
        );
        let _ = std::fs::remove_file(outside);
    }

    #[test]
    fn cleanup_evicts_old_entries_without_removing_the_current_write() {
        let root = tempfile::tempdir().expect("temp cache");
        let cache = ShellIconDiskCache::with_limits(root.path().to_path_buf(), 2, 1_024);
        for generation in 1..=3 {
            let payload = ShellIconPayload::new(
                key(generation),
                1,
                1,
                4,
                vec![u8::try_from(generation).expect("fixture generation fits u8"); 4],
                None,
            )
            .expect("valid payload");
            cache.store(&payload);
        }
        assert_eq!(
            std::fs::read_dir(root.path())
                .expect("read cache")
                .filter_map(Result::ok)
                .count(),
            2
        );
        assert!(cache.load(&key(3)).is_some());
    }

    #[test]
    fn every_variant_and_windows_build_has_an_independent_digest() {
        let baseline = key(11);
        let variants = [
            ShellIconKey {
                dpi: 200,
                ..baseline.clone()
            },
            ShellIconKey {
                theme: ShellIconTheme::Dark,
                ..baseline.clone()
            },
            ShellIconKey {
                association_generation: 12,
                ..baseline.clone()
            },
            ShellIconKey {
                overlay_generation: 12,
                ..baseline.clone()
            },
        ];
        let digest = key_digest_for_build(&baseline, "26100");
        for variant in variants {
            assert_ne!(key_digest_for_build(&variant, "26100"), digest);
        }
        assert_ne!(
            key_digest_for_build(&baseline, "26200"),
            digest,
            "an OS upgrade cannot reuse pixels extracted for an older Shell build"
        );
    }

    #[test]
    fn schema_mismatch_is_rejected_and_concurrent_duplicate_writes_leave_one_clean_entry() {
        let root = tempfile::tempdir().expect("temp cache");
        let cache = std::sync::Arc::new(ShellIconDiskCache::with_root(root.path().to_path_buf()));
        let payload = std::sync::Arc::new(
            ShellIconPayload::new(key(21), 1, 1, 4, vec![9, 8, 7, 6], None).expect("valid payload"),
        );
        let workers = (0..8)
            .map(|_| {
                let cache = cache.clone();
                let payload = payload.clone();
                std::thread::spawn(move || cache.store(&payload))
            })
            .collect::<Vec<_>>();
        assert!(
            workers
                .into_iter()
                .all(|worker| worker.join().expect("writer"))
        );
        let entries = std::fs::read_dir(root.path())
            .expect("read cache")
            .filter_map(Result::ok)
            .collect::<Vec<_>>();
        assert_eq!(
            entries
                .iter()
                .filter(
                    |entry| entry.path().extension().and_then(|value| value.to_str())
                        == Some("bc7cache")
                )
                .count(),
            1
        );
        assert!(
            entries.iter().all(
                |entry| entry.path().extension().and_then(|value| value.to_str()) != Some("tmp")
            )
        );

        let entry = entries
            .into_iter()
            .find(|entry| {
                entry.path().extension().and_then(|value| value.to_str()) == Some("bc7cache")
            })
            .expect("one cache entry")
            .path();
        let mut bytes = std::fs::read(&entry).expect("read entry");
        bytes[8..10].copy_from_slice(&0_u16.to_le_bytes());
        std::fs::write(&entry, bytes).expect("write old schema");
        assert!(cache.load(&key(21)).is_none());
        assert!(!entry.exists());
    }

    #[test]
    fn interrupted_write_removes_temporary_file_without_publishing_an_entry() {
        let root = tempfile::tempdir().expect("temp cache");
        let cache = ShellIconDiskCache::with_root(root.path().to_path_buf());
        let payload =
            ShellIconPayload::new(key(121), 1, 1, 4, vec![1, 2, 3, 4], None).expect("payload");

        let error = cache
            .write_entry_interrupted_before_publish(&payload)
            .expect_err("injected interruption must fail the write");
        assert_eq!(error.kind(), std::io::ErrorKind::Interrupted);
        assert!(cache.load(&key(121)).is_none());
        assert_eq!(
            std::fs::read_dir(root.path())
                .expect("read cache")
                .filter_map(Result::ok)
                .count(),
            0,
            "failed publication must clean its same-directory temporary file"
        );
    }

    #[test]
    fn readers_racing_atomic_publication_observe_only_miss_or_complete_hit() {
        let root = tempfile::tempdir().expect("temp cache");
        let cache = std::sync::Arc::new(ShellIconDiskCache::with_root(root.path().to_path_buf()));
        let cache_key = key(122);
        let payload = std::sync::Arc::new(
            ShellIconPayload::new(cache_key.clone(), 4, 4, 16, vec![0x7f; 64], None)
                .expect("payload"),
        );
        let start = std::sync::Arc::new(std::sync::Barrier::new(5));
        let writer = {
            let cache = std::sync::Arc::clone(&cache);
            let payload = std::sync::Arc::clone(&payload);
            let start = std::sync::Arc::clone(&start);
            std::thread::spawn(move || {
                start.wait();
                assert!(cache.store(&payload));
            })
        };
        let readers = (0..4)
            .map(|_| {
                let cache = std::sync::Arc::clone(&cache);
                let cache_key = cache_key.clone();
                let start = std::sync::Arc::clone(&start);
                std::thread::spawn(move || {
                    start.wait();
                    for _ in 0..64 {
                        assert!(!matches!(
                            cache.load_outcome(&cache_key),
                            super::DiskCacheLoad::Rejected
                        ));
                        std::thread::yield_now();
                    }
                })
            })
            .collect::<Vec<_>>();
        writer.join().expect("writer");
        for reader in readers {
            reader.join().expect("reader");
        }
        assert!(cache.load(&cache_key).is_some());
    }

    #[test]
    fn icon_and_thumbnail_quota_cleanup_is_isolated_by_root_and_kind() {
        let icon_root = tempfile::tempdir().expect("icon cache");
        let thumbnail_root = tempfile::tempdir().expect("thumbnail cache");
        let icons = ShellIconDiskCache::with_limits(icon_root.path().to_path_buf(), 1, 512);
        let thumbnails = ShellIconDiskCache::with_thumbnail_limits(
            thumbnail_root.path().to_path_buf(),
            2,
            1_024,
        );
        for generation in 130..=132 {
            let payload = ShellIconPayload::new(
                key(generation),
                1,
                1,
                4,
                vec![u8::try_from(generation).unwrap(); 4],
                None,
            )
            .expect("payload");
            assert!(icons.store(&payload));
            assert!(thumbnails.store(&payload));
        }
        assert_eq!(
            std::fs::read_dir(icon_root.path())
                .unwrap()
                .filter_map(Result::ok)
                .count(),
            1
        );
        assert_eq!(
            std::fs::read_dir(thumbnail_root.path())
                .unwrap()
                .filter_map(Result::ok)
                .count(),
            2
        );
        assert!(thumbnails.load(&key(132)).is_some());
        assert!(icons.load(&key(132)).is_some());
    }

    #[test]
    fn runtime_gates_are_independent_and_deny_by_default_after_reset() {
        let _guard = super::BC7_GATE_TEST_LOCK.lock().expect("gate test lock");
        set_shell_bc7_runtime_gates(false, false);
        assert_eq!(shell_bc7_runtime_gates(), ShellBc7RuntimeGatesV1::default());

        set_shell_bc7_runtime_gates(true, false);
        let icon_only = shell_bc7_runtime_gates();
        assert!(icon_only.icon_enabled);
        assert!(!icon_only.thumbnail_enabled);

        set_shell_bc7_runtime_gates(false, true);
        let thumbnail_only = shell_bc7_runtime_gates();
        assert!(!thumbnail_only.icon_enabled);
        assert!(thumbnail_only.thumbnail_enabled);
        set_shell_bc7_runtime_gates(false, false);
    }
}
