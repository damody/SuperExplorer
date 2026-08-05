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
        atomic::{AtomicU64, Ordering},
    },
    time::SystemTime,
};

use explorer_model::{LocationDescriptor, ShellIconKey, ShellIconPayload, ShellIconTheme};
use windows::{
    Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW},
    core::w,
};

const MAGIC: &[u8; 8] = b"RGXICON1";
// Version 3 invalidates entries that may have captured pre-overlay or not-yet-ready Shell pixels.
const SCHEMA_VERSION: u16 = 3;
const PIXEL_FORMAT_RGBA8: u8 = 1;
const HEADER_LEN: usize = 8 + 2 + 8 + 2 + 2 + 4 + 1 + 8 + 4;
const MAX_ENTRY_BYTES: u64 = 16 * 1024 * 1024;
const DEFAULT_MAX_ENTRIES: usize = 4_096;
const DEFAULT_MAX_TOTAL_BYTES: u64 = 512 * 1024 * 1024;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
pub(crate) struct ShellIconDiskCache {
    root: Option<PathBuf>,
    max_entries: usize,
    max_total_bytes: u64,
}

pub(crate) enum DiskCacheLoad {
    Hit(ShellIconPayload),
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
            max_total_bytes: DEFAULT_MAX_TOTAL_BYTES,
        }
    }
}

impl ShellIconDiskCache {
    pub(crate) fn with_root(root: PathBuf) -> Self {
        Self {
            root: Some(root),
            max_entries: DEFAULT_MAX_ENTRIES,
            max_total_bytes: DEFAULT_MAX_TOTAL_BYTES,
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
        }
    }

    pub(crate) fn load_outcome(&self, key: &ShellIconKey) -> DiskCacheLoad {
        let Some(path) = self.entry_path(key) else {
            return DiskCacheLoad::Miss;
        };
        match Self::read_entry(&path, key) {
            Ok(Some(payload)) => DiskCacheLoad::Hit(payload),
            Ok(None) => DiskCacheLoad::Miss,
            Err(error) => {
                tracing::debug!(?error, "Shell icon disk cache entry was rejected");
                let _ = fs::remove_file(path);
                DiskCacheLoad::Rejected
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn load(&self, key: &ShellIconKey) -> Option<ShellIconPayload> {
        match self.load_outcome(key) {
            DiskCacheLoad::Hit(payload) => Some(payload),
            DiskCacheLoad::Miss | DiskCacheLoad::Rejected => None,
        }
    }

    pub(crate) fn store(&self, payload: &ShellIconPayload) -> bool {
        match self.write_entry(payload) {
            Ok(()) => true,
            Err(error) => {
                tracing::debug!(?error, "Shell icon disk cache write was skipped");
                false
            }
        }
    }

    fn entry_path(&self, key: &ShellIconKey) -> Option<PathBuf> {
        let root = self.root.as_ref()?;
        Some(root.join(format!("{:016x}.rgba", key_digest(key))))
    }

    fn read_entry(path: &Path, key: &ShellIconKey) -> io::Result<Option<ShellIconPayload>> {
        let metadata = match fs::metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
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
        let digest = take_u64(&bytes, &mut cursor)?;
        let width = take_u16(&bytes, &mut cursor)?;
        let height = take_u16(&bytes, &mut cursor)?;
        let stride = take_u32(&bytes, &mut cursor)?;
        let pixel_format = take_u8(&bytes, &mut cursor)?;
        let payload_len = take_u64(&bytes, &mut cursor)?;
        let checksum = take_u32(&bytes, &mut cursor)?;
        if schema != SCHEMA_VERSION
            || digest != key_digest(key)
            || pixel_format != PIXEL_FORMAT_RGBA8
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "schema, key digest, or pixel format mismatch",
            ));
        }
        let payload_len = usize::try_from(payload_len)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "payload too large"))?;
        if payload_len != bytes.len().saturating_sub(cursor)
            || payload_len != stride as usize * usize::from(height)
            || crc32(&bytes[cursor..]) != checksum
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "payload length, stride, or checksum mismatch",
            ));
        }
        ShellIconPayload::new(
            key.clone(),
            width,
            height,
            stride,
            bytes[cursor..].to_vec(),
            None,
        )
        .map(Some)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, format!("{error:?}")))
    }

    fn write_entry(&self, payload: &ShellIconPayload) -> io::Result<()> {
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
            file.write_all(&key_digest(&payload.key).to_le_bytes())?;
            file.write_all(&payload.width.to_le_bytes())?;
            file.write_all(&payload.height.to_le_bytes())?;
            file.write_all(&payload.stride.to_le_bytes())?;
            file.write_all(&[PIXEL_FORMAT_RGBA8])?;
            let payload_len = u64::try_from(payload.rgba.len()).map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidInput, "icon payload is too large")
            })?;
            file.write_all(&payload_len.to_le_bytes())?;
            file.write_all(&crc32(&payload.rgba).to_le_bytes())?;
            file.write_all(&payload.rgba)?;
            file.sync_all()?;
            drop(file);
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
                (path.extension().and_then(|value| value.to_str()) == Some("rgba"))
                    .then(|| {
                        let metadata = entry.metadata().ok()?;
                        Some((
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

    use super::{ShellIconDiskCache, key_digest_for_build};

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
    fn round_trip_is_exact_and_generation_changes_the_entry() {
        let root = tempfile::tempdir().expect("temp cache");
        let cache = ShellIconDiskCache::with_root(root.path().to_path_buf());
        let payload =
            ShellIconPayload::new(key(1), 2, 2, 8, (0..16).collect(), None).expect("valid payload");
        cache.store(&payload);
        assert_eq!(cache.load(&key(1)), Some(payload));
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
        let pixel_format_offset = 8 + 2 + 8 + 2 + 2 + 4;
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
                        == Some("rgba")
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
            .find(|entry| entry.path().extension().and_then(|value| value.to_str()) == Some("rgba"))
            .expect("one cache entry")
            .path();
        let mut bytes = std::fs::read(&entry).expect("read entry");
        bytes[8..10].copy_from_slice(&0_u16.to_le_bytes());
        std::fs::write(&entry, bytes).expect("write old schema");
        assert!(cache.load(&key(21)).is_none());
        assert!(!entry.exists());
    }
}
