//! Cheap persistent RGBA icons, independent of the optional BC7 compression pipeline.
#![allow(unsafe_code, reason = "read-only Shell association registry queries")]

use explorer_model::{ShellIconKey, ShellIconPayload};
use std::{
    collections::HashMap,
    fs,
    io::{Read, Write},
    os::windows::fs::MetadataExt,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use windows::{
    Win32::System::Registry::{
        HKEY, HKEY_CLASSES_ROOT, HKEY_CURRENT_USER, RRF_NOEXPAND, RRF_RT_REG_EXPAND_SZ,
        RRF_RT_REG_SZ, RegGetValueW,
    },
    core::HSTRING,
};

const MAGIC: &[u8; 8] = b"SEICRAW1";
const HEADER: usize = 48;
const MAX_PIXELS: usize = 4 * 1024 * 1024;
const MAX_BYTES: u64 = 64 * 1024 * 1024;
static SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(crate) struct RawIconCache {
    root: Option<PathBuf>,
    associations: HashMap<String, u64>,
    writes: usize,
}

impl Default for RawIconCache {
    fn default() -> Self {
        Self::with_root(std::env::var_os("LOCALAPPDATA").map(|root| {
            PathBuf::from(root)
                .join("RustGpuiExplorer")
                .join("icon-cache")
                .join("rgba-v1")
        }))
    }
}

impl RawIconCache {
    pub(crate) fn with_root(root: Option<PathBuf>) -> Self {
        Self {
            root,
            associations: HashMap::new(),
            writes: 0,
        }
    }

    fn identity(&mut self, key: &ShellIconKey) -> Option<(PathBuf, u64, u64)> {
        let root = self.root.clone()?;
        let mut source = Vec::new();
        if let Some(path) = key.location.path() {
            // Existing files are versioned by metadata; shared attribute-only requests
            // intentionally name no real file. A directory's custom artwork can live in desktop.ini.
            for candidate in [path.to_path_buf(), path.join("desktop.ini")] {
                if let Ok(metadata) = fs::metadata(candidate) {
                    source.extend_from_slice(&metadata.file_size().to_le_bytes());
                    source.extend_from_slice(&metadata.last_write_time().to_le_bytes());
                    source.extend_from_slice(&metadata.file_attributes().to_le_bytes());
                }
            }
            let extension = path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if self.associations.len() >= 1024 && !self.associations.contains_key(&extension) {
                self.associations.clear();
            }
            let association = *self
                .associations
                .entry(extension.clone())
                .or_insert_with(|| association_fingerprint(&extension));
            source.extend_from_slice(&association.to_le_bytes());
        }
        let digest = crate::icon_disk_cache::key_digest(key);
        let source = checksum(&source);
        Some((
            root.join(format!("{digest:016x}-{source:016x}.rgba")),
            digest,
            source,
        ))
    }

    pub(crate) fn load(&mut self, key: &ShellIconKey) -> Option<ShellIconPayload> {
        let (path, digest, source) = self.identity(key)?;
        let metadata = fs::symlink_metadata(&path).ok()?;
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || metadata.len() < HEADER as u64
            || metadata.len() > (HEADER + MAX_PIXELS) as u64
        {
            return None;
        }
        let mut bytes = Vec::new();
        fs::File::open(path)
            .ok()?
            .take((HEADER + MAX_PIXELS + 1) as u64)
            .read_to_end(&mut bytes)
            .ok()?;
        if bytes.len() > HEADER + MAX_PIXELS {
            return None;
        }
        if bytes.len() < HEADER || &bytes[..8] != MAGIC {
            return None;
        }
        let u64_at = |at| u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap());
        let u32_at = |at| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
        if u64_at(8) != digest
            || u64_at(16) != source
            || u32_at(36) as usize != bytes.len() - HEADER
            || u64_at(40) != checksum(&bytes[HEADER..])
        {
            return None;
        }
        ShellIconPayload::new(
            key.clone(),
            u32_at(24).try_into().ok()?,
            u32_at(28).try_into().ok()?,
            u32_at(32),
            bytes[HEADER..].to_vec(),
            None,
        )
        .ok()
    }

    pub(crate) fn store(&mut self, payload: &ShellIconPayload) -> bool {
        if payload.rgba.is_empty() || payload.rgba.len() > MAX_PIXELS {
            return false;
        }
        let Some((path, digest, source)) = self.identity(&payload.key) else {
            return false;
        };
        if path.exists() && self.load(&payload.key).is_none() {
            let _ = fs::remove_file(&path);
        }
        let root = path.parent().unwrap();
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temporary = root.join(format!("{}-{sequence}.tmp", std::process::id()));
        let result = (|| -> std::io::Result<()> {
            fs::create_dir_all(root)?;
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?;
            file.write_all(MAGIC)?;
            file.write_all(&digest.to_le_bytes())?;
            file.write_all(&source.to_le_bytes())?;
            for value in [
                u32::from(payload.width),
                u32::from(payload.height),
                payload.stride,
                payload.rgba.len() as u32,
            ] {
                file.write_all(&value.to_le_bytes())?;
            }
            file.write_all(&checksum(&payload.rgba).to_le_bytes())?;
            file.write_all(&payload.rgba)?;
            drop(file);
            // Windows rename does not replace an entry another worker already published.
            if !path.exists() {
                fs::rename(&temporary, &path)?;
            }
            Ok(())
        })();
        let _ = fs::remove_file(temporary);
        self.writes = self.writes.wrapping_add(1);
        if self.writes % 32 == 1 {
            self.prune();
        }
        result.is_ok()
    }

    fn prune(&self) {
        let Some(root) = &self.root else {
            return;
        };
        let Ok(entries) = fs::read_dir(root) else {
            return;
        };
        let mut entries = entries
            .flatten()
            .filter_map(|entry| {
                let metadata = fs::symlink_metadata(entry.path()).ok()?;
                (metadata.is_file()
                    && !metadata.file_type().is_symlink()
                    && entry.path().extension().is_some_and(|ext| ext == "rgba"))
                .then(|| (entry.path(), metadata.len(), metadata.last_write_time()))
            })
            .collect::<Vec<_>>();
        let mut bytes: u64 = entries.iter().map(|(_, size, _)| size).sum();
        let mut count = entries.len();
        entries.sort_by_key(|(_, _, time)| *time);
        for (path, size, _) in entries {
            if bytes <= MAX_BYTES && count <= 4096 {
                break;
            }
            if fs::remove_file(path).is_ok() {
                bytes = bytes.saturating_sub(size);
                count -= 1;
            }
        }
    }
}

fn checksum(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100_0000_01b3)
    })
}

fn registry_string(root: HKEY, key: &str, value: &str) -> String {
    let mut buffer = [0u16; 2048];
    let mut size = size_of_val(&buffer) as u32;
    // SAFETY: borrowed predefined key, live UTF-16 names and a correctly sized writable buffer.
    let status = unsafe {
        RegGetValueW(
            root,
            &HSTRING::from(key),
            &HSTRING::from(value),
            RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ | RRF_NOEXPAND,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    if status.is_err() {
        return String::new();
    }
    let end = buffer
        .iter()
        .position(|word| *word == 0)
        .unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..end])
}

fn association_fingerprint(extension: &str) -> u64 {
    let extension = if extension.is_empty() {
        "Folder".to_owned()
    } else {
        format!(".{extension}")
    };
    let chosen = registry_string(
        HKEY_CURRENT_USER,
        &format!(
            r"Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\{extension}\UserChoice"
        ),
        "ProgId",
    );
    let registered = registry_string(HKEY_CLASSES_ROOT, &extension, "");
    let class = if chosen.is_empty() {
        &registered
    } else {
        &chosen
    };
    let icon = registry_string(HKEY_CLASSES_ROOT, &format!(r"{class}\DefaultIcon"), "");
    let handler = registry_string(
        HKEY_CLASSES_ROOT,
        &format!(r"{class}\shellex\IconHandler"),
        "",
    );
    checksum(format!("{chosen}\0{registered}\0{icon}\0{handler}").as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use explorer_model::{LocationDescriptor, ShellIconTheme};

    #[test]
    fn raw_icons_survive_restart_reject_corruption_and_follow_source_changes() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("sample.exe");
        fs::write(&file, b"version-one").unwrap();
        let key = ShellIconKey {
            item_id: None,
            location: LocationDescriptor::file_system(&file),
            size_bucket: 32,
            dpi: 96,
            theme: ShellIconTheme::Light,
            association_generation: 1,
            overlay_generation: 0,
        };
        let payload =
            ShellIconPayload::new(key.clone(), 1, 1, 4, vec![1, 2, 3, 255], None).unwrap();
        let cache_root = root.path().join("cache");
        assert!(RawIconCache::with_root(Some(cache_root.clone())).store(&payload));
        let mut reopened = RawIconCache::with_root(Some(cache_root));
        assert_eq!(reopened.load(&key).unwrap().rgba, payload.rgba);
        let mut changed = key.clone();
        changed.dpi = 144;
        assert!(reopened.load(&changed).is_none());
        changed = key.clone();
        changed.overlay_generation = 1;
        assert!(reopened.load(&changed).is_none());
        changed = key.clone();
        changed.association_generation = 2;
        assert!(reopened.load(&changed).is_none());
        let (path, _, _) = reopened.identity(&key).unwrap();
        let mut bytes = fs::read(&path).unwrap();
        *bytes.last_mut().unwrap() ^= 1;
        fs::write(&path, bytes).unwrap();
        assert!(reopened.load(&key).is_none());
        assert!(reopened.store(&payload));
        assert_eq!(reopened.load(&key).unwrap().rgba, payload.rgba);
        fs::write(&file, b"different-executable-version").unwrap();
        assert!(reopened.load(&key).is_none());
    }
}
