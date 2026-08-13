//! Windows Shell icon acquisition and owned RGBA conversion.
#![allow(
    unsafe_code,
    reason = "GDI bitmap readback requires audited Win32 calls"
)]

use std::{collections::HashMap, mem::size_of, ptr};

use explorer_common::{ExplorerError, ExplorerErrorKind};
use explorer_model::{ShellIconKey, ShellIconPayload};
use windows::{
    Win32::{
        Foundation::SIZE,
        Graphics::Gdi::{
            BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, DIB_RGB_COLORS,
            GetDIBits, GetObjectW, HGDIOBJ,
        },
        Storage::FileSystem::{
            FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL, FILE_FLAGS_AND_ATTRIBUTES,
        },
        UI::{
            Controls::{IImageList, ILD_TRANSPARENT},
            Shell::{
                IShellItemImageFactory, SHFILEINFOW, SHGFI_ADDOVERLAYS, SHGFI_ICON,
                SHGFI_OVERLAYINDEX, SHGFI_SYSICONINDEX, SHGFI_USEFILEATTRIBUTES, SHGetFileInfoW,
                SHGetImageList, SHIL_EXTRALARGE, SHIL_JUMBO, SHIL_LARGE, SHIL_SMALL,
                SIIGBF_BIGGERSIZEOK, SIIGBF_ICONONLY,
            },
            WindowsAndMessaging::{GetIconInfo, ICONINFO},
        },
    },
    core::{HSTRING, Interface},
};

pub(crate) const SHELL_ICON_CACHE_CAPACITY: usize = 512;

struct CacheEntry {
    payload: ShellIconPayload,
    last_used: u64,
}

pub(crate) struct ShellIconCache {
    entries: HashMap<ShellIconKey, CacheEntry>,
    clock: u64,
    capacity: usize,
    disk: crate::icon_disk_cache::ShellIconDiskCache,
    stats: ShellIconCacheStats,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ShellIconCacheStats {
    pub memory_hits: u64,
    pub disk_hits: u64,
    pub disk_misses: u64,
    pub disk_corrupt: u64,
    pub disk_write_failures: u64,
    pub shell_loads: u64,
}

impl Default for ShellIconCache {
    fn default() -> Self {
        Self::with_capacity(SHELL_ICON_CACHE_CAPACITY)
    }
}

impl ShellIconCache {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: HashMap::new(),
            clock: 0,
            capacity: capacity.max(1),
            disk: crate::icon_disk_cache::ShellIconDiskCache::default(),
            stats: ShellIconCacheStats::default(),
        }
    }

    #[cfg(test)]
    fn with_disk(capacity: usize, disk: crate::icon_disk_cache::ShellIconDiskCache) -> Self {
        Self {
            disk,
            ..Self::with_capacity(capacity)
        }
    }

    #[cfg(test)]
    pub(crate) const fn stats(&self) -> ShellIconCacheStats {
        self.stats
    }

    pub(crate) fn load(&mut self, key: &ShellIconKey) -> Result<ShellIconPayload, ExplorerError> {
        self.clock = self.clock.wrapping_add(1);
        if let Some(entry) = self.entries.get_mut(key) {
            entry.last_used = self.clock;
            self.stats.memory_hits = self.stats.memory_hits.saturating_add(1);
            tracing::debug!(cache_source = "memory", "Shell icon cache hit");
            return Ok(entry.payload.clone());
        }
        match self.disk.load_outcome(key) {
            crate::icon_disk_cache::DiskCacheLoad::Hit(payload) => {
                self.stats.disk_hits = self.stats.disk_hits.saturating_add(1);
                tracing::debug!(cache_source = "bc7-disk", "Shell icon cache hit");
                self.insert(payload.clone());
                return Ok(payload);
            }
            crate::icon_disk_cache::DiskCacheLoad::Miss => {
                self.stats.disk_misses = self.stats.disk_misses.saturating_add(1);
            }
            crate::icon_disk_cache::DiskCacheLoad::Rejected => {
                self.stats.disk_corrupt = self.stats.disk_corrupt.saturating_add(1);
            }
        }
        // Filesystem overlay state (including TortoiseGit) can change while the path and the
        // persisted cache key remain stable across process launches. Ask the live Shell first
        // for existing paths, then refresh the disk entry. The persisted pixels remain a useful
        // fallback when an overlay handler or the Shell image list is temporarily unavailable.
        if key.location.path().is_some_and(std::path::Path::exists) {
            self.stats.shell_loads = self.stats.shell_loads.saturating_add(1);
            match load(key) {
                Ok(payload) => {
                    if !self.disk.store(&payload) {
                        self.stats.disk_write_failures =
                            self.stats.disk_write_failures.saturating_add(1);
                    }
                    let payload = self.disk.load(key).unwrap_or(payload);
                    self.insert(payload.clone());
                    tracing::debug!(cache_source = "shell-refresh", "Shell icon cache refreshed");
                    return Ok(payload);
                }
                Err(error) => {
                    tracing::debug!(?error, "live Shell icon refresh failed; trying disk cache");
                }
            }
        }
        tracing::debug!(cache_source = "shell", "Shell icon cache miss");
        self.stats.shell_loads = self.stats.shell_loads.saturating_add(1);
        let payload = load(key)?;
        if !self.disk.store(&payload) {
            self.stats.disk_write_failures = self.stats.disk_write_failures.saturating_add(1);
        }
        let payload = self.disk.load(key).unwrap_or(payload);
        self.insert(payload.clone());
        tracing::debug!(
            memory_hits = self.stats.memory_hits,
            disk_hits = self.stats.disk_hits,
            disk_misses = self.stats.disk_misses,
            disk_corrupt = self.stats.disk_corrupt,
            disk_write_failures = self.stats.disk_write_failures,
            shell_loads = self.stats.shell_loads,
            "Shell icon cache counters"
        );
        Ok(payload)
    }

    fn insert(&mut self, payload: ShellIconPayload) {
        if self.entries.len() >= self.capacity
            && !self.entries.contains_key(&payload.key)
            && let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(key, _)| key.clone())
        {
            self.entries.remove(&oldest);
        }
        self.clock = self.clock.wrapping_add(1);
        self.entries.insert(
            payload.key.clone(),
            CacheEntry {
                payload,
                last_used: self.clock,
            },
        );
    }
}

pub(crate) fn load(key: &ShellIconKey) -> Result<ShellIconPayload, ExplorerError> {
    if let Some(path) = key.location.path()
        && let Ok(payload) = load_filesystem_icon_with_overlays(key, path)
    {
        return Ok(payload);
    }
    load_shell_item_image_factory(key)
}

fn load_shell_item_image_factory(key: &ShellIconKey) -> Result<ShellIconPayload, ExplorerError> {
    let item = crate::navigation::shell_item(&key.location)?;
    let factory: IShellItemImageFactory = item
        .cast()
        .map_err(|error| windows_error("query Shell image factory", &error))?;
    let requested = i32::from(key.size_bucket.max(1));
    // SAFETY: the COM interface is live on its owning STA and the returned HBITMAP ownership is
    // transferred immediately into OwnedBitmap.
    let mut pending_retries = 0;
    let raw = loop {
        // SAFETY: the live factory is called synchronously on its owning STA.
        match unsafe {
            factory.GetImage(
                SIZE {
                    cx: requested,
                    cy: requested,
                },
                SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK,
            )
        } {
            Ok(bitmap) => break bitmap,
            Err(error) if error.code().0 == -2_147_483_638 && pending_retries < 20 => {
                pending_retries += 1;
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            Err(error) => return Err(windows_error("load Shell icon", &error)),
        }
    };
    // SAFETY: GetImage returned unique caller-owned HBITMAP on success.
    let bitmap = unsafe { crate::native::OwnedBitmap::from_raw(raw) }
        .ok_or_else(|| icon_error("load Shell icon", "Shell returned a null bitmap"))?;
    bitmap_to_rgba(key.clone(), &bitmap)
}

fn load_filesystem_icon_with_overlays(
    key: &ShellIconKey,
    path: &std::path::Path,
) -> Result<ShellIconPayload, ExplorerError> {
    let extensionless_file_marker = path
        .file_name()
        .is_some_and(|name| name.to_string_lossy().ends_with('.'));
    let has_extension = path.extension().is_some();
    let path = HSTRING::from(path.as_os_str().to_string_lossy().as_ref());
    let mut info = SHFILEINFOW::default();
    // SAFETY: path is a live NUL-terminated HSTRING and info is correctly sized writable storage.
    let info_size = u32::try_from(size_of::<SHFILEINFOW>())
        .map_err(|_| icon_error("load Shell icon overlay", "SHFILEINFOW size exceeds u32"))?;
    let shared_base = is_shared_base_icon_request(key);
    let attributes = if shared_base {
        if !has_extension && !extensionless_file_marker {
            FILE_ATTRIBUTE_DIRECTORY
        } else {
            FILE_ATTRIBUTE_NORMAL
        }
    } else {
        FILE_FLAGS_AND_ATTRIBUTES::default()
    };
    let flags = if shared_base {
        SHGFI_SYSICONINDEX | SHGFI_USEFILEATTRIBUTES
    } else {
        // SHGFI_OVERLAYINDEX only asks Shell to encode the overlay slot in iIcon.  Shell
        // extension handlers such as TortoiseGit are not guaranteed to evaluate the live item
        // unless an icon is requested in the same call.  Keep SHGFI_ICON here even though the
        // low-resolution returned HICON is used only to trigger/evaluate that overlay; the base
        // pixels below still come from the requested high-resolution system image list.
        SHGFI_ICON | SHGFI_SYSICONINDEX | SHGFI_ADDOVERLAYS | SHGFI_OVERLAYINDEX
    };
    let result =
        unsafe { SHGetFileInfoW(&path, attributes, Some(&raw mut info), info_size, flags) };
    if result == 0 {
        return Err(icon_error(
            "load Shell icon overlay",
            "SHGetFileInfoW returned no icon",
        ));
    }
    // SAFETY: SHGFI_ICON transfers one caller-owned HICON.  Retain it until the packed overlay
    // index has been consumed and release it on every return path; shared-base requests omit
    // SHGFI_ICON and therefore leave this null.
    let _query_icon = unsafe { crate::native::OwnedIcon::from_raw(info.hIcon) };
    let packed_index = u32::try_from(info.iIcon).map_err(|_| {
        icon_error(
            "load Shell icon overlay",
            "SHGetFileInfoW returned a negative image-list index",
        )
    })?;
    let image_index = i32::try_from(packed_index & 0x00ff_ffff).map_err(|_| {
        icon_error(
            "load Shell icon overlay",
            "Shell image-list index exceeds i32",
        )
    })?;
    let overlay_index = packed_index >> 24;
    if key.size_bucket > 256
        && overlay_index == 0
        && let Ok(payload) = load_shell_item_image_factory(key)
        && payload.width >= key.size_bucket
        && payload.height >= key.size_bucket
    {
        return Ok(payload);
    }
    let image_list_kind = shell_image_list_kind(key.size_bucket);
    // SAFETY: SHGetImageList returns a reference-counted COM interface for the process-wide
    // Shell image list. The interface remains on this STA and is released by IImageList.
    let image_list: IImageList = unsafe {
        SHGetImageList(i32::try_from(image_list_kind).map_err(|_| {
            icon_error(
                "load Shell icon overlay",
                "Shell image-list kind exceeds i32",
            )
        })?)
    }
    .map_err(|error| windows_error("load high-resolution Shell image list", &error))?;
    let overlay_mask = overlay_index << 8;
    // SAFETY: image_index came from SHGetFileInfoW for this system image list. GetIcon returns a
    // unique caller-owned HICON, wrapped immediately below. The overlay mask is the documented
    // INDEXTOOVERLAYMASK representation of SHGFI_OVERLAYINDEX's high byte.
    let raw_icon = unsafe { image_list.GetIcon(image_index, ILD_TRANSPARENT.0 | overlay_mask) }
        .map_err(|error| windows_error("load high-resolution Shell icon", &error))?;
    // SAFETY: IImageList::GetIcon transferred one caller-owned HICON on success.
    let icon = unsafe { crate::native::OwnedIcon::from_raw(raw_icon) }.ok_or_else(|| {
        icon_error(
            "load Shell icon overlay",
            "Shell image list returned a null icon",
        )
    })?;
    let mut icon_info = ICONINFO::default();
    // SAFETY: icon remains live and icon_info is correctly sized writable storage. GetIconInfo
    // transfers caller-owned color/mask bitmaps, both wrapped immediately below.
    unsafe { GetIconInfo(icon.get(), &raw mut icon_info) }
        .map_err(|error| windows_error("read Shell icon overlay", &error))?;
    // SAFETY: GetIconInfo transferred these GDI bitmaps on success.
    let color = unsafe { crate::native::OwnedBitmap::from_raw(icon_info.hbmColor) };
    // SAFETY: same ownership transfer as color; keep the mask alive until conversion completes.
    let _mask = unsafe { crate::native::OwnedBitmap::from_raw(icon_info.hbmMask) };
    let color = color.ok_or_else(|| {
        icon_error(
            "read Shell icon overlay",
            "monochrome Shell icons are unsupported",
        )
    })?;
    bitmap_to_rgba(key.clone(), &color)
}

const fn shell_image_list_kind(requested_physical_size: u16) -> u32 {
    match requested_physical_size {
        0..=16 => SHIL_SMALL,
        17..=32 => SHIL_LARGE,
        33..=48 => SHIL_EXTRALARGE,
        _ => SHIL_JUMBO,
    }
}

fn is_shared_base_icon_request(key: &ShellIconKey) -> bool {
    key.item_id.is_none() && key.association_generation > 0 && key.overlay_generation == 0
}

fn bitmap_to_rgba(
    key: ShellIconKey,
    bitmap: &crate::native::OwnedBitmap,
) -> Result<ShellIconPayload, ExplorerError> {
    let (width, height, stride, rgba) = bitmap_to_owned_rgba(bitmap)?;
    ShellIconPayload::new(key, width, height, stride, rgba, None)
        .map_err(|_| icon_error("convert Shell icon", "owned RGBA invariant failed"))
}

pub(crate) fn bitmap_to_owned_rgba(
    bitmap: &crate::native::OwnedBitmap,
) -> Result<(u16, u16, u32, Vec<u8>), ExplorerError> {
    let bitmap_size = i32::try_from(size_of::<BITMAP>())
        .map_err(|_| icon_error("inspect Shell icon", "BITMAP size exceeds i32"))?;
    let mut object = BITMAP::default();
    // SAFETY: object is writable BITMAP storage and bitmap remains live for the call.
    let copied = unsafe {
        GetObjectW(
            HGDIOBJ::from(bitmap.get()),
            bitmap_size,
            Some(ptr::from_mut(&mut object).cast()),
        )
    };
    if copied != bitmap_size || object.bmWidth <= 0 || object.bmHeight == 0 {
        return Err(icon_error("inspect Shell icon", "invalid BITMAP metadata"));
    }
    let width = u16::try_from(object.bmWidth)
        .map_err(|_| icon_error("inspect Shell icon", "bitmap width exceeds protocol"))?;
    let height_i32 = object.bmHeight.unsigned_abs();
    let height = u16::try_from(height_i32)
        .map_err(|_| icon_error("inspect Shell icon", "bitmap height exceeds protocol"))?;
    let stride = u32::from(width) * 4;
    let mut bgra = vec![0_u8; stride as usize * usize::from(height)];
    let header_size = u32::try_from(size_of::<BITMAPINFOHEADER>())
        .map_err(|_| icon_error("read Shell icon", "BITMAPINFOHEADER size exceeds u32"))?;
    let mut info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: header_size,
            biWidth: i32::from(width),
            biHeight: -i32::from(height),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            biSizeImage: stride * u32::from(height),
            ..Default::default()
        },
        ..Default::default()
    };
    // SAFETY: CreateCompatibleDC returns uniquely owned DC on success.
    let dc = unsafe { crate::native::OwnedDc::from_raw(CreateCompatibleDC(None)) }
        .ok_or_else(|| icon_error("read Shell icon", "CreateCompatibleDC failed"))?;
    // SAFETY: bgra spans biSizeImage bytes, info describes matching top-down 32-bit storage, and
    // both the DC and bitmap remain live for this synchronous call.
    let lines = unsafe {
        GetDIBits(
            dc.get(),
            bitmap.get(),
            0,
            u32::from(height),
            Some(bgra.as_mut_ptr().cast()),
            &raw mut info,
            DIB_RGB_COLORS,
        )
    };
    if lines != i32::from(height) {
        return Err(icon_error(
            "read Shell icon",
            "GetDIBits returned incomplete rows",
        ));
    }
    convert_bgra_to_rgba(&mut bgra);
    Ok((width, height, stride, bgra))
}

fn convert_bgra_to_rgba(pixels: &mut [u8]) {
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
}

fn windows_error(operation: &'static str, error: &windows::core::Error) -> ExplorerError {
    ExplorerError::new(
        ExplorerErrorKind::Availability,
        operation,
        true,
        "Windows 圖示暫時無法載入。",
        format!("HRESULT {:#010x}", error.code().0),
    )
}

fn icon_error(operation: &'static str, detail: &'static str) -> ExplorerError {
    ExplorerError::new(
        ExplorerErrorKind::Availability,
        operation,
        true,
        "Windows 圖示暫時無法載入。",
        detail,
    )
}

#[cfg(test)]
mod tests {
    use super::{
        ShellIconCache, convert_bgra_to_rgba, is_shared_base_icon_request, load,
        shell_image_list_kind,
    };
    use explorer_model::{LocationDescriptor, ShellIconKey, ShellIconTheme, ShellItemId};
    use std::sync::Mutex;
    use windows::Win32::System::Threading::{GR_GDIOBJECTS, GetCurrentProcess, GetGuiResources};

    static REAL_SHELL_ICON_TEST_LOCK: Mutex<()> = Mutex::new(());

    const CROSS_PROCESS_CACHE_ROOT: &str = "EXPLORER_TEST_ICON_CACHE_ROOT";
    const CROSS_PROCESS_CACHE_PHASE: &str = "EXPLORER_TEST_ICON_CACHE_PHASE";

    #[test]
    fn shell_image_list_never_upscales_small_or_large_view_icons() {
        use windows::Win32::UI::Shell::{SHIL_EXTRALARGE, SHIL_JUMBO, SHIL_LARGE, SHIL_SMALL};

        assert_eq!(shell_image_list_kind(16), SHIL_SMALL);
        assert_eq!(shell_image_list_kind(17), SHIL_LARGE);
        assert_eq!(shell_image_list_kind(32), SHIL_LARGE);
        assert_eq!(shell_image_list_kind(33), SHIL_EXTRALARGE);
        assert_eq!(shell_image_list_kind(48), SHIL_EXTRALARGE);
        for requested in [49, 64, 72, 84, 96, 108, 128, 256, 384, 512] {
            assert_eq!(shell_image_list_kind(requested), SHIL_JUMBO);
        }
    }

    #[test]
    fn real_large_folder_request_uses_jumbo_shell_pixels() {
        let _serial = REAL_SHELL_ICON_TEST_LOCK.lock().unwrap();
        let _apartment = crate::sta::ApartmentGuard::initialize().expect("STA initializes");
        let payload = load(&ShellIconKey {
            item_id: ShellItemId::from_provider_bytes([204]),
            location: LocationDescriptor::file_system(r"D:\test"),
            size_bucket: 128,
            dpi: 96,
            theme: ShellIconTheme::Light,
            association_generation: 1,
            overlay_generation: 1,
        })
        .expect("jumbo folder icon loads");

        assert!(
            payload.width >= 128 && payload.height >= 128,
            "a 128px view must not upscale a low-resolution {}x{} Shell icon",
            payload.width,
            payload.height
        );
        assert!(payload.rgba.chunks_exact(4).any(|pixel| pixel[3] != 0));

        let extra_large = load(&ShellIconKey {
            item_id: ShellItemId::from_provider_bytes([205]),
            location: LocationDescriptor::file_system(r"C:\Windows"),
            size_bucket: 512,
            dpi: 96,
            theme: ShellIconTheme::Light,
            association_generation: 1,
            overlay_generation: 1,
        })
        .expect("512px folder icon loads");
        assert!(
            extra_large.width >= 512 && extra_large.height >= 512,
            "an overlay-free 512px request should use Shell image-factory pixels, got {}x{}",
            extra_large.width,
            extra_large.height
        );
    }

    #[test]
    fn generic_breadcrumb_folder_uses_shell_file_attributes_without_overlays() {
        let shared = ShellIconKey {
            item_id: None,
            location: LocationDescriptor::file_system(r"C:\__super_explorer_folder_base__"),
            size_bucket: 20,
            dpi: 144,
            theme: ShellIconTheme::Light,
            association_generation: 7,
            overlay_generation: 0,
        };
        assert!(is_shared_base_icon_request(&shared));

        let mut concrete = shared.clone();
        concrete.location = LocationDescriptor::file_system(r"C:\Windows");
        concrete.association_generation = 0;
        assert!(!is_shared_base_icon_request(&concrete));

        let mut overlay = shared;
        overlay.overlay_generation = 1;
        assert!(!is_shared_base_icon_request(&overlay));
    }

    #[test]
    fn cross_process_disk_cache_helper() {
        let Some(root) = std::env::var_os(CROSS_PROCESS_CACHE_ROOT) else {
            return;
        };
        let phase = std::env::var(CROSS_PROCESS_CACHE_PHASE).expect("helper phase");
        let disk = crate::icon_disk_cache::ShellIconDiskCache::with_root(root.clone().into());
        let mut cache = ShellIconCache::with_disk(2, disk);
        let drive = if std::path::Path::new(r"D:\").is_dir() {
            r"D:\"
        } else {
            r"C:\"
        };
        let key = ShellIconKey {
            item_id: None,
            location: LocationDescriptor::file_system(drive),
            size_bucket: 32,
            dpi: 168,
            theme: ShellIconTheme::Light,
            association_generation: 41,
            overlay_generation: 43,
        };
        let _apartment = crate::sta::ApartmentGuard::initialize().expect("STA initializes");
        let payload = cache.load(&key).expect("cross-process icon load");
        let hash = payload
            .rgba
            .iter()
            .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
                (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
            });
        let stats = cache.stats();
        std::fs::write(
            std::path::PathBuf::from(root).join(format!("{phase}.result")),
            format!(
                "{hash},{},{},{}",
                stats.shell_loads,
                stats.disk_hits,
                payload.rgba.len()
            ),
        )
        .expect("write helper result");
    }

    #[test]
    fn each_process_refreshes_existing_filesystem_overlays_and_persists_the_result() {
        let _serial = REAL_SHELL_ICON_TEST_LOCK.lock().unwrap();
        let root = tempfile::tempdir().expect("cross-process cache");
        let executable = std::env::current_exe().expect("current test executable");
        for phase in ["cold", "warm"] {
            let status = std::process::Command::new(&executable)
                .args([
                    "--exact",
                    "icon::tests::cross_process_disk_cache_helper",
                    "--nocapture",
                ])
                .env(CROSS_PROCESS_CACHE_ROOT, root.path())
                .env(CROSS_PROCESS_CACHE_PHASE, phase)
                .status()
                .expect("spawn cache helper");
            assert!(status.success(), "{phase} helper failed: {status}");
        }
        let read = |phase: &str| {
            let text = std::fs::read_to_string(root.path().join(format!("{phase}.result")))
                .expect("read helper result");
            text.split(',')
                .map(|part| part.parse::<u64>().expect("numeric result"))
                .collect::<Vec<_>>()
        };
        let cold = read("cold");
        let warm = read("warm");
        assert_ne!(cold[0], 0, "cold Shell extraction must produce pixels");
        assert_ne!(warm[0], 0, "warm Shell refresh must produce pixels");
        assert_eq!(
            cold[3], warm[3],
            "refreshed payload length must remain exact"
        );
        assert_eq!(&cold[1..3], &[1, 0], "cold process must extract from Shell");
        assert_eq!(
            &warm[1..3],
            &[1, 0],
            "warm process must refresh live overlay state instead of trusting stale disk pixels"
        );
        assert!(
            std::fs::read_dir(root.path())
                .expect("read persistent cache")
                .any(|entry| entry.is_ok_and(|entry| {
                    entry.path().extension().is_some_and(|ext| ext == "rgba")
                })),
            "live refresh must still persist an exact fallback for later launches"
        );
    }

    #[test]
    fn real_drive_icon_is_owned_rgba_and_has_visible_alpha() {
        let _serial = REAL_SHELL_ICON_TEST_LOCK.lock().unwrap();
        let _apartment = crate::sta::ApartmentGuard::initialize().expect("STA initializes");
        let drive = if std::path::Path::new(r"D:\").is_dir() {
            r"D:\"
        } else {
            r"C:\"
        };
        let payload = load(&ShellIconKey {
            item_id: None,
            location: LocationDescriptor::file_system(drive),
            size_bucket: 32,
            dpi: 168,
            theme: ShellIconTheme::Light,
            association_generation: 0,
            overlay_generation: 0,
        })
        .expect("Shell drive icon loads");
        assert_eq!(
            payload.rgba.len(),
            payload.stride as usize * usize::from(payload.height)
        );
        assert!(payload.rgba.chunks_exact(4).any(|pixel| pixel[3] != 0));
    }

    #[test]
    fn bgra_conversion_preserves_transparency_and_premultiplied_channels() {
        let mut pixels = [0, 0, 0, 0, 10, 20, 30, 40];
        convert_bgra_to_rgba(&mut pixels);
        assert_eq!(pixels, [0, 0, 0, 0, 30, 20, 10, 40]);
        assert!(pixels[4] <= pixels[7] && pixels[5] <= pixels[7] && pixels[6] <= pixels[7]);
    }

    #[test]
    fn lru_cache_evicts_oldest_and_keeps_dpi_theme_association_keys_separate() {
        let mut cache = ShellIconCache::with_capacity(2);
        let make = |suffix: &str, dpi, generation| {
            let key = ShellIconKey {
                item_id: None,
                location: LocationDescriptor::file_system(format!(r"C:\{suffix}")),
                size_bucket: 20,
                dpi,
                theme: ShellIconTheme::Light,
                association_generation: generation,
                overlay_generation: generation,
            };
            explorer_model::ShellIconPayload::new(key, 1, 1, 4, vec![0, 0, 0, 0], None).unwrap()
        };
        let first = make("first", 96, 0);
        let second = make("second", 144, 0);
        let third = make("third", 144, 1);
        cache.insert(first.clone());
        cache.insert(second.clone());
        cache.entries.get_mut(&first.key).unwrap().last_used = 10;
        cache.entries.get_mut(&second.key).unwrap().last_used = 20;
        cache.insert(third.clone());
        assert!(!cache.entries.contains_key(&first.key));
        assert!(cache.entries.contains_key(&second.key));
        assert!(cache.entries.contains_key(&third.key));
    }

    #[test]
    fn warm_disk_payload_becomes_a_memory_hit_with_non_sensitive_counters() {
        let root = tempfile::tempdir().expect("temp cache");
        let disk = crate::icon_disk_cache::ShellIconDiskCache::with_root(root.path().to_path_buf());
        let key = ShellIconKey {
            item_id: None,
            location: LocationDescriptor::file_system(r"D:\counter-fixture"),
            size_bucket: 20,
            dpi: 168,
            theme: ShellIconTheme::Light,
            association_generation: 3,
            overlay_generation: 4,
        };
        let payload =
            explorer_model::ShellIconPayload::new(key.clone(), 1, 1, 4, vec![1, 2, 3, 4], None)
                .expect("valid payload");
        assert!(disk.store(&payload));
        let mut cache = ShellIconCache::with_disk(2, disk);
        let disk_hit = cache.load(&key).expect("disk hit");
        assert_eq!(disk_hit.key, payload.key);
        assert_eq!((disk_hit.width, disk_hit.height), (1, 1));
        assert!(disk_hit.rgba.is_empty());
        assert!(
            disk_hit.bc7.is_some(),
            "persisted icons are promoted to BC7"
        );
        assert_eq!(cache.load(&key).expect("memory hit"), disk_hit);
        assert_eq!(
            cache.stats(),
            super::ShellIconCacheStats {
                memory_hits: 1,
                disk_hits: 1,
                ..Default::default()
            }
        );
    }

    #[test]
    fn real_shell_icon_matrix_covers_folder_drive_archives_and_namespace() {
        let _serial = REAL_SHELL_ICON_TEST_LOCK.lock().unwrap();
        let _apartment = crate::sta::ApartmentGuard::initialize().expect("STA initializes");
        let mut locations = vec![
            LocationDescriptor::file_system(r"D:\"),
            LocationDescriptor::file_system(r"D:\test"),
            LocationDescriptor::ParsingName("shell:MyComputerFolder".into()),
        ];
        if let Some(one_drive) =
            std::env::var_os("OneDrive").filter(|path| std::path::Path::new(path).is_dir())
        {
            locations.push(LocationDescriptor::file_system(one_drive));
        }
        for entry in std::fs::read_dir(r"D:\").expect("D drive reads").flatten() {
            let extension = entry
                .path()
                .extension()
                .and_then(|value| value.to_str())
                .map(str::to_ascii_lowercase);
            if matches!(extension.as_deref(), Some("zip" | "rar")) {
                locations.push(LocationDescriptor::file_system(entry.path()));
            }
        }
        assert!(
            locations.len() >= 5,
            "D drive fixture must include ZIP and RAR"
        );
        let mut hashes = Vec::new();
        for location in locations {
            let key = ShellIconKey {
                item_id: None,
                location: location.clone(),
                size_bucket: 32,
                dpi: 168,
                theme: ShellIconTheme::Light,
                association_generation: 0,
                overlay_generation: 0,
            };
            let payload = load(&key).expect("Shell matrix icon loads");
            assert!(payload.rgba.chunks_exact(4).any(|pixel| pixel[3] != 0));
            let repeated = load(&key).expect("same Shell icon reloads");
            assert_eq!(
                payload.rgba, repeated.rgba,
                "Shell RGBA must be stable for {location:?}"
            );
            hashes.push(
                payload
                    .rgba
                    .iter()
                    .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
                        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
                    }),
            );
        }
        hashes.sort_unstable();
        hashes.dedup();
        assert!(
            hashes.len() >= 4,
            "drive, folder, archive, and namespace icons must not collapse to one tinted asset"
        );

        let folder = load(&ShellIconKey {
            item_id: None,
            location: LocationDescriptor::file_system(r"D:\test"),
            size_bucket: 32,
            dpi: 168,
            theme: ShellIconTheme::Light,
            association_generation: 0,
            overlay_generation: 0,
        })
        .expect("real folder icon");
        assert!(
            folder
                .rgba
                .chunks_exact(4)
                .any(|pixel| { pixel[3] > 0 && pixel[0] > pixel[1] && pixel[1] > pixel[2] }),
            "the Windows folder bitmap must retain its warm khaki pixels instead of an app blue tint"
        );
    }

    #[test]
    fn real_desktop_ini_custom_folder_icon_differs_from_shared_folder_base() {
        use std::os::windows::ffi::OsStrExt;

        let _serial = REAL_SHELL_ICON_TEST_LOCK.lock().unwrap();
        let _apartment = crate::sta::ApartmentGuard::initialize().expect("STA initializes");
        let fixture = tempfile::Builder::new()
            .prefix("custom-folder-icon-")
            .tempdir_in(r"D:\test\target")
            .expect("custom folder fixture");
        let folder = fixture.path().join("custom");
        std::fs::create_dir(&folder).expect("custom folder");
        let windows = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_owned());
        let desktop_ini = folder.join("desktop.ini");
        std::fs::write(
            &desktop_ini,
            format!("[.ShellClassInfo]\r\nIconResource={windows}\\explorer.exe,0\r\n"),
        )
        .expect("desktop.ini");
        assert!(
            std::process::Command::new("attrib.exe")
                .args(["+r", folder.to_string_lossy().as_ref()])
                .status()
                .expect("folder attrib")
                .success()
        );
        assert!(
            std::process::Command::new("attrib.exe")
                .args(["+h", "+s", desktop_ini.to_string_lossy().as_ref()])
                .status()
                .expect("desktop.ini attrib")
                .success()
        );
        let mut folder_wide = folder.as_os_str().encode_wide().collect::<Vec<_>>();
        folder_wide.push(0);
        unsafe {
            windows::Win32::UI::Shell::SHChangeNotify(
                windows::Win32::UI::Shell::SHCNE_UPDATEITEM,
                windows::Win32::UI::Shell::SHCNF_PATHW,
                Some(folder_wide.as_ptr().cast()),
                None,
            );
        }
        let custom = load(&ShellIconKey {
            item_id: ShellItemId::from_provider_bytes([201]),
            location: LocationDescriptor::file_system(&folder),
            size_bucket: 32,
            dpi: 96,
            theme: ShellIconTheme::Light,
            association_generation: 1,
            overlay_generation: 1,
        })
        .expect("custom folder icon");
        let generic = load(&ShellIconKey {
            item_id: None,
            location: LocationDescriptor::file_system(r"C:\__super_explorer_folder_base__"),
            size_bucket: 32,
            dpi: 96,
            theme: ShellIconTheme::Light,
            association_generation: 1,
            overlay_generation: 0,
        })
        .expect("shared folder base");
        let _ = std::process::Command::new("attrib.exe")
            .args(["-h", "-s", desktop_ini.to_string_lossy().as_ref()])
            .status();
        let _ = std::process::Command::new("attrib.exe")
            .args(["-r", folder.to_string_lossy().as_ref()])
            .status();
        assert_ne!(custom.rgba, generic.rgba);
    }

    #[test]
    #[ignore = "requires a configured OneDrive root and installed cloud Shell handlers"]
    fn real_onedrive_visible_result_is_owned_and_comparable_to_shared_base() {
        let _serial = REAL_SHELL_ICON_TEST_LOCK.lock().unwrap();
        let _apartment = crate::sta::ApartmentGuard::initialize().expect("STA initializes");
        let one_drive = std::env::var_os("OneDrive")
            .map(std::path::PathBuf::from)
            .filter(|path| path.is_dir())
            .expect("configured OneDrive directory");
        let visible = load(&ShellIconKey {
            item_id: ShellItemId::from_provider_bytes([202]),
            location: LocationDescriptor::file_system(&one_drive),
            size_bucket: 32,
            dpi: 96,
            theme: ShellIconTheme::Light,
            association_generation: 1,
            overlay_generation: 1,
        })
        .expect("OneDrive visible item icon");
        let generic = load(&ShellIconKey {
            item_id: None,
            location: LocationDescriptor::file_system(r"C:\__super_explorer_folder_base__"),
            size_bucket: 32,
            dpi: 96,
            theme: ShellIconTheme::Light,
            association_generation: 1,
            overlay_generation: 0,
        })
        .expect("shared folder base");
        assert_eq!(visible.width, generic.width);
        assert_eq!(visible.height, generic.height);
        assert!(visible.rgba.chunks_exact(4).any(|pixel| pixel[3] != 0));
        eprintln!(
            "OneDrive Shell visible result classification={}",
            if visible.rgba == generic.rgba {
                "negative"
            } else {
                "override"
            }
        );
    }

    #[test]
    fn real_executable_shortcut_and_association_epoch_matrix_loads_owned_pixels() {
        let _serial = REAL_SHELL_ICON_TEST_LOCK.lock().unwrap();
        let _apartment = crate::sta::ApartmentGuard::initialize().expect("STA initializes");
        let fixture = tempfile::Builder::new()
            .prefix("identity-icon-fixture-")
            .tempdir_in(r"D:\test\target")
            .expect("identity icon fixture");
        let executable = std::env::current_exe().expect("current executable");
        let shortcut = fixture.path().join("executable.lnk");
        let escaped_shortcut = shortcut.display().to_string().replace('\'', "''");
        let escaped_target = executable.display().to_string().replace('\'', "''");
        let script = format!(
            "$w=New-Object -ComObject WScript.Shell; $s=$w.CreateShortcut('{escaped_shortcut}'); $s.TargetPath='{escaped_target}'; $s.Save()"
        );
        let status = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .status()
            .expect("create shortcut");
        assert!(status.success());

        for (ordinal, location) in [executable, shortcut].into_iter().enumerate() {
            let payload = load(&ShellIconKey {
                item_id: ShellItemId::from_provider_bytes([
                    203,
                    u8::try_from(ordinal).expect("small ordinal"),
                ]),
                location: LocationDescriptor::file_system(location),
                size_bucket: 32,
                dpi: 96,
                theme: ShellIconTheme::Light,
                association_generation: 1,
                overlay_generation: 1,
            })
            .expect("identity-specific Shell icon");
            assert!(payload.rgba.chunks_exact(4).any(|pixel| pixel[3] != 0));
        }

        let disk = crate::icon_disk_cache::ShellIconDiskCache::with_root(
            fixture.path().join("association-cache"),
        );
        let mut cache = ShellIconCache::with_disk(4, disk);
        for association_generation in [31, 32] {
            cache
                .load(&ShellIconKey {
                    item_id: None,
                    location: LocationDescriptor::file_system(
                        r"C:\__super_explorer_association__.txt",
                    ),
                    size_bucket: 32,
                    dpi: 96,
                    theme: ShellIconTheme::Light,
                    association_generation,
                    overlay_generation: 0,
                })
                .expect("real association base reload");
        }
        assert_eq!(cache.stats().shell_loads, 2);
    }

    #[test]
    fn repeated_shell_bitmap_readback_releases_every_gdi_object() {
        let _serial = REAL_SHELL_ICON_TEST_LOCK.lock().unwrap();
        let _apartment = crate::sta::ApartmentGuard::initialize().expect("STA initializes");
        let key = ShellIconKey {
            item_id: None,
            location: LocationDescriptor::file_system(r"D:\"),
            size_bucket: 32,
            dpi: 168,
            theme: ShellIconTheme::Light,
            association_generation: 0,
            overlay_generation: 0,
        };
        // Warm process-global Shell/GDI caches before measuring our ownership delta.
        for _ in 0..10 {
            drop(load(&key).expect("warmup icon loads"));
        }
        let before_owned = crate::native::NativeResourceSnapshot::capture();
        // Retain process GDI telemetry as diagnostic evidence, but other parallel
        // Shell tests may legitimately warm a process-global cache during this loop.
        let before_process = unsafe { GetGuiResources(GetCurrentProcess(), GR_GDIOBJECTS) };
        for _ in 0..100 {
            let payload = load(&key).expect("drive icon loads repeatedly");
            assert!(!payload.rgba.is_empty());
        }
        // SAFETY: same non-owning process pseudo handle contract as above.
        let after_process = unsafe { GetGuiResources(GetCurrentProcess(), GR_GDIOBJECTS) };
        let after_owned = crate::native::NativeResourceSnapshot::capture();
        assert_eq!(after_owned.bitmaps, before_owned.bitmaps, "HBITMAP leak");
        assert_eq!(
            after_owned.device_contexts, before_owned.device_contexts,
            "HDC leak"
        );
        assert_eq!(after_owned.icons, before_owned.icons, "HICON leak");
        assert!(
            after_owned.kernel_handles <= before_owned.kernel_handles.saturating_add(8),
            "icon-loop handle ownership must unwind; a bounded delta allows parallel watcher tests"
        );
        assert!(
            after_process <= before_process.saturating_add(8),
            "process-global Shell cache growth must remain bounded"
        );
    }

    #[test]
    #[ignore = "requires installed TortoiseGit overlay handlers and a writable real Git fixture"]
    fn real_tortoise_git_clean_modified_and_added_overlays_are_distinct() {
        let _serial = REAL_SHELL_ICON_TEST_LOCK.lock().unwrap();
        let _apartment = crate::sta::ApartmentGuard::initialize().expect("STA initializes");
        let fixture_parent = std::path::Path::new(r"D:\test\target");
        let fixture = tempfile::Builder::new()
            .prefix("tortoise-overlay-fixture-")
            .tempdir_in(fixture_parent)
            .expect("dedicated overlay fixture");
        let run_git = |arguments: &[&str]| {
            let output = std::process::Command::new("git")
                .args(arguments)
                .current_dir(fixture.path())
                .output()
                .expect("git starts");
            assert!(
                output.status.success(),
                "git {arguments:?} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        };
        run_git(&["init", "--quiet"]);
        run_git(&["config", "user.name", "Explorer Overlay Test"]);
        run_git(&["config", "user.email", "overlay-test@example.invalid"]);
        for name in ["clean.txt", "modified.txt"] {
            std::fs::write(fixture.path().join(name), "initial\n").expect("write tracked file");
        }
        run_git(&["add", "clean.txt", "modified.txt"]);
        run_git(&["commit", "--quiet", "-m", "fixture"]);
        std::fs::write(fixture.path().join("modified.txt"), "changed\n")
            .expect("modify tracked file");
        std::fs::write(fixture.path().join("added.txt"), "added\n").expect("write added file");
        run_git(&["add", "added.txt"]);
        std::fs::write(fixture.path().join("unversioned.txt"), "unversioned\n")
            .expect("write unversioned file");

        std::thread::sleep(std::time::Duration::from_secs(3));
        let icon_hash = |name: &str| {
            let payload = load(&ShellIconKey {
                item_id: None,
                location: LocationDescriptor::file_system(fixture.path().join(name)),
                size_bucket: 32,
                dpi: 168,
                theme: ShellIconTheme::Light,
                association_generation: 0,
                overlay_generation: 1,
            })
            .expect("overlay icon loads");
            payload
                .rgba
                .iter()
                .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
                    (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
                })
        };
        let clean = icon_hash("clean.txt");
        let modified = icon_hash("modified.txt");
        let added = icon_hash("added.txt");
        let unversioned = icon_hash("unversioned.txt");
        eprintln!(
            "TortoiseGit Shell hashes clean={clean:016x} modified={modified:016x} added={added:016x} unversioned={unversioned:016x}"
        );
        let distinct = [clean, modified, added, unversioned]
            .into_iter()
            .collect::<std::collections::HashSet<_>>();
        assert_ne!(clean, modified, "modified badge must differ from clean");
        assert_ne!(clean, added, "added badge must differ from clean");
        assert!(
            distinct.len() >= 3,
            "the installed Shell handlers must expose at least three real status bitmaps"
        );
    }
}
