//! Shell STA thumbnail retrieval through `IShellItemImageFactory` with owned pixel transfer.
#![allow(
    unsafe_code,
    reason = "IShellItemImageFactory returns caller-owned HBITMAP handles across an audited FFI boundary"
)]

use explorer_model::{
    LocationDescriptor, ShellIconKey, ShellIconPayload, ThumbnailFallbackReason, ThumbnailMode,
    ThumbnailPixels, ThumbnailRequest, ThumbnailSource, ThumbnailTerminal,
};
use std::os::windows::ffi::OsStrExt as _;
use windows::{
    Win32::{
        Foundation::SIZE,
        Storage::FileSystem::{
            FILE_ATTRIBUTE_OFFLINE, FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS,
            FILE_ATTRIBUTE_RECALL_ON_OPEN, GetFileAttributesW, INVALID_FILE_ATTRIBUTES,
        },
        UI::Shell::{
            IShellItemImageFactory, SIIGBF_BIGGERSIZEOK, SIIGBF_ICONONLY, SIIGBF_INCACHEONLY,
            SIIGBF_RESIZETOFIT, SIIGBF_THUMBNAILONLY,
        },
    },
    core::Interface,
};

/// Retrieves one real Shell thumbnail on the owning STA and returns only owned RGBA bytes.
pub fn load_shell_thumbnail(
    request: &ThumbnailRequest,
    location: &LocationDescriptor,
    cache_only: bool,
    maximum_decoded_bytes: usize,
) -> ThumbnailTerminal {
    let disk = thumbnail_disk_cache();
    load_shell_thumbnail_with_cache(
        request,
        location,
        cache_only,
        maximum_decoded_bytes,
        Some(&disk),
        true,
    )
}

/// Retrieves provider-backed RGBA for ABI consumers that cannot accept GPUI's private BC7 payload.
pub fn load_shell_thumbnail_rgba(
    request: &ThumbnailRequest,
    location: &LocationDescriptor,
    cache_only: bool,
    maximum_decoded_bytes: usize,
) -> ThumbnailTerminal {
    let disk = thumbnail_disk_cache();
    load_shell_thumbnail_with_cache(
        request,
        location,
        cache_only,
        maximum_decoded_bytes,
        Some(&disk),
        false,
    )
}

fn load_shell_thumbnail_with_cache(
    request: &ThumbnailRequest,
    location: &LocationDescriptor,
    cache_only: bool,
    maximum_decoded_bytes: usize,
    disk: Option<&crate::icon_disk_cache::ShellIconDiskCache>,
    allow_compressed: bool,
) -> ThumbnailTerminal {
    let allow_compressed = allow_compressed && crate::icon_disk_cache::thumbnail_bc7_enabled();
    let cache_only = cache_only || requires_cache_only(location);
    if request.context.cancellation.is_cancelled() {
        return ThumbnailTerminal::Fallback(ThumbnailFallbackReason::Cancelled);
    }
    if request
        .context
        .deadline
        .is_elapsed_at(std::time::Instant::now())
    {
        return ThumbnailTerminal::Fallback(ThumbnailFallbackReason::Timeout);
    }
    let disk_key = disk_key(request, location);
    if let Some(crate::icon_disk_cache::DiskCacheLoad::Hit(payload)) =
        disk.map(|cache| cache.load_outcome(&disk_key))
    {
        if allow_compressed
            && crate::icon_disk_cache::thumbnail_bc7_enabled()
            && let Some(raster) = payload.bc7.clone()
        {
            return ThumbnailTerminal::Compressed {
                source: ThumbnailSource::DiskCache,
                raster,
            };
        }
        let pixels = ThumbnailPixels {
            width: u32::from(payload.width),
            height: u32::from(payload.height),
            stride: payload.stride,
            bytes: payload.rgba.clone(),
        };
        if pixels.validate(maximum_decoded_bytes).is_ok() {
            return ThumbnailTerminal::Ready {
                source: ThumbnailSource::DiskCache,
                pixels,
            };
        }
    }
    let item = match crate::navigation::shell_item(location) {
        Ok(item) => item,
        Err(error) => return ThumbnailTerminal::Failed(error.to_string()),
    };
    let factory: IShellItemImageFactory = match item.cast() {
        Ok(factory) => factory,
        Err(_) => return ThumbnailTerminal::Fallback(ThumbnailFallbackReason::Unsupported),
    };
    let requested = i32::from(request.key.physical_size.max(1));
    let mut flags = match request.key.mode {
        ThumbnailMode::Thumbnail => SIIGBF_THUMBNAILONLY | SIIGBF_RESIZETOFIT,
        ThumbnailMode::IconOnly => SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK,
    };
    if cache_only {
        flags |= SIIGBF_INCACHEONLY;
    }
    // SAFETY: the factory is live on its owning STA and transfers one HBITMAP on success.
    let raw = match unsafe {
        factory.GetImage(
            SIZE {
                cx: requested,
                cy: requested,
            },
            flags,
        )
    } {
        Ok(bitmap) => bitmap,
        Err(error) if cache_only => {
            tracing::debug!(hresult = error.code().0, "Shell cache-only thumbnail miss");
            return ThumbnailTerminal::Fallback(ThumbnailFallbackReason::Offline);
        }
        Err(error) => {
            tracing::debug!(hresult = error.code().0, "Shell thumbnail provider failed");
            return ThumbnailTerminal::Fallback(ThumbnailFallbackReason::ProviderFailure);
        }
    };
    // SAFETY: GetImage transferred unique ownership of a non-null GDI bitmap on success.
    let Some(bitmap) = (unsafe { crate::native::OwnedBitmap::from_raw(raw) }) else {
        return ThumbnailTerminal::Fallback(ThumbnailFallbackReason::Corrupt);
    };
    if request.context.cancellation.is_cancelled() {
        return ThumbnailTerminal::Fallback(ThumbnailFallbackReason::Cancelled);
    }
    if request
        .context
        .deadline
        .is_elapsed_at(std::time::Instant::now())
    {
        return ThumbnailTerminal::Fallback(ThumbnailFallbackReason::Timeout);
    }
    let Ok((width, height, stride, bytes)) = crate::icon::bitmap_to_owned_rgba(&bitmap) else {
        return ThumbnailTerminal::Fallback(ThumbnailFallbackReason::Corrupt);
    };
    let pixels = ThumbnailPixels {
        width: u32::from(width),
        height: u32::from(height),
        stride,
        bytes,
    };
    if pixels.validate(maximum_decoded_bytes).is_err() {
        return ThumbnailTerminal::Fallback(ThumbnailFallbackReason::ResourceLimit);
    }
    if let Ok(payload) =
        ShellIconPayload::new(disk_key, width, height, stride, pixels.bytes.clone(), None)
        && let Some(disk) = disk
        && compressed_thumbnail_publish_allowed(allow_compressed)
    {
        let now = std::time::Instant::now();
        let deadline = request
            .context
            .deadline
            .remaining_at(now)
            .and_then(|remaining| now.checked_add(remaining));
        let _ = crate::bc7_pipeline::schedule(
            crate::bc7_codec::Bc7ContentKind::Thumbnail,
            payload,
            disk.clone(),
            Some(request.context.cancellation.clone()),
            deadline,
        );
    }
    ThumbnailTerminal::Ready {
        source: if cache_only {
            ThumbnailSource::WindowsCache
        } else {
            ThumbnailSource::Provider
        },
        pixels,
    }
}

fn compressed_thumbnail_publish_allowed(request_started_compressed: bool) -> bool {
    request_started_compressed && crate::icon_disk_cache::thumbnail_bc7_enabled()
}

/// Detects cloud/offline placeholders using metadata-only file attributes.
/// This function never opens a stream and therefore cannot hydrate content.
pub fn requires_cache_only(location: &LocationDescriptor) -> bool {
    let Some(path) = location.path() else {
        return false;
    };
    let wide = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    // SAFETY: `wide` is NUL terminated and remains alive for the metadata-only call.
    let attributes = unsafe { GetFileAttributesW(windows::core::PCWSTR(wide.as_ptr())) };
    if attributes == INVALID_FILE_ATTRIBUTES {
        return false;
    }
    let placeholder = FILE_ATTRIBUTE_OFFLINE.0
        | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS.0
        | FILE_ATTRIBUTE_RECALL_ON_OPEN.0;
    attributes & placeholder != 0
}

/// Clears only roadmap thumbnail disk entries, never session, logs, or the icon cache.
pub fn clear_thumbnail_disk_cache() -> bool {
    thumbnail_disk_cache().clear().is_ok()
}

fn thumbnail_disk_cache() -> crate::icon_disk_cache::ShellIconDiskCache {
    let root = std::env::var_os("LOCALAPPDATA")
        .map_or_else(std::env::temp_dir, std::path::PathBuf::from)
        .join("RustGpuiExplorer")
        .join("thumbnail-cache")
        .join("v1");
    crate::icon_disk_cache::ShellIconDiskCache::with_root_lossy_thumbnail(root)
}

fn disk_key(request: &ThumbnailRequest, location: &LocationDescriptor) -> ShellIconKey {
    ShellIconKey {
        item_id: Some(request.key.item_id.clone()),
        location: location.clone(),
        size_bucket: request.key.physical_size,
        dpi: request.key.dpi,
        theme: request.key.theme,
        association_generation: request
            .key
            .association_generation
            .wrapping_mul(31)
            .wrapping_add(request.key.source_generation),
        overlay_generation: request.key.overlay_generation,
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use explorer_common::{RequestDeadline, RoadmapLimits};
    use explorer_model::{
        Generation, RequestContext, ShellIconTheme, ShellItemId, TabId, ThumbnailPriority,
        ThumbnailRequestKey,
    };

    use super::*;

    fn benchmark_request(item_id: &[u8]) -> ThumbnailRequest {
        let deadline = RequestDeadline::after(Instant::now(), Duration::from_secs(10))
            .expect("benchmark deadline");
        ThumbnailRequest::new(
            RequestContext::new(TabId::new(), Generation::default()).with_deadline(deadline),
            ThumbnailRequestKey {
                item_id: ShellItemId::from_provider_bytes(item_id.to_vec()).expect("item id"),
                physical_size: 256,
                dpi: 96,
                mode: ThumbnailMode::Thumbnail,
                source_generation: 1,
                theme: ShellIconTheme::Light,
                association_generation: 1,
                overlay_generation: 0,
            },
            ThumbnailPriority::ActiveVisible,
        )
    }

    fn terminal_source(terminal: &ThumbnailTerminal) -> &'static str {
        match terminal {
            ThumbnailTerminal::Ready {
                source: ThumbnailSource::Provider,
                ..
            } => "provider",
            ThumbnailTerminal::Ready {
                source: ThumbnailSource::WindowsCache,
                ..
            } => "windows-cache",
            ThumbnailTerminal::Ready {
                source: ThumbnailSource::DiskCache,
                ..
            } => "project-disk",
            ThumbnailTerminal::Ready { .. } => "other-ready",
            ThumbnailTerminal::Compressed {
                source: ThumbnailSource::DiskCache,
                ..
            } => "project-bc7-disk",
            ThumbnailTerminal::Compressed { .. } => "other-compressed",
            ThumbnailTerminal::Fallback(_) => "fallback",
            ThumbnailTerminal::Failed(_) => "failed",
        }
    }

    #[test]
    fn in_flight_thumbnail_disable_prevents_compressed_publication_without_mutating_icon_gate() {
        let _guard = crate::icon_disk_cache::BC7_GATE_TEST_LOCK
            .lock()
            .expect("gate test lock");
        crate::icon_disk_cache::set_shell_bc7_runtime_gates(true, true);
        let request_started_compressed = crate::icon_disk_cache::thumbnail_bc7_enabled();
        assert!(compressed_thumbnail_publish_allowed(
            request_started_compressed
        ));

        crate::icon_disk_cache::set_shell_bc7_runtime_gates(true, false);
        assert!(crate::icon_disk_cache::icon_bc7_enabled());
        assert!(!compressed_thumbnail_publish_allowed(
            request_started_compressed
        ));
        crate::icon_disk_cache::set_shell_bc7_runtime_gates(false, false);
    }

    #[test]
    fn thumbnail_cache_modes_emit_comparable_benchmark() {
        let _guard = crate::icon_disk_cache::BC7_GATE_TEST_LOCK
            .lock()
            .expect("gate test lock");
        crate::icon_disk_cache::set_shell_bc7_runtime_gates(false, true);
        let _apartment = crate::sta::ApartmentGuard::initialize().expect("STA");
        let fixtures = tempfile::tempdir().expect("benchmark fixtures");
        let escaped = fixtures.path().display().to_string().replace('\'', "''");
        let image_script = format!(
            "$d='{escaped}'; Add-Type -AssemblyName System.Drawing; 1..3 | ForEach-Object {{ $b=[Drawing.Bitmap]::new(640,360); $g=[Drawing.Graphics]::FromImage($b); $g.Clear([Drawing.Color]::FromArgb(255, (40*$_), (60*$_), (80*$_))); $b.Save((Join-Path $d (\"mode-$_.png\")),[Drawing.Imaging.ImageFormat]::Png); $g.Dispose(); $b.Dispose() }}"
        );
        let status = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", &image_script])
            .status()
            .expect("create benchmark images");
        assert!(status.success());
        let limit = RoadmapLimits::default().thumbnail_memory_bytes;

        // A unique image and a disabled project cache exercise the provider path.
        let no_disk_request = benchmark_request(b"benchmark-no-disk");
        let no_disk_location = LocationDescriptor::file_system(fixtures.path().join("mode-1.png"));
        let started = Instant::now();
        let no_disk = load_shell_thumbnail_with_cache(
            &no_disk_request,
            &no_disk_location,
            false,
            limit,
            None,
            false,
        );
        let no_disk_us = started.elapsed().as_micros();
        assert!(matches!(no_disk, ThumbnailTerminal::Ready { .. }));

        // Prime Windows for a second unique image, then require an OS cache hit while
        // keeping the project cache disabled.
        let windows_request = benchmark_request(b"benchmark-windows-cache");
        let windows_location = LocationDescriptor::file_system(fixtures.path().join("mode-2.png"));
        let primed = load_shell_thumbnail_with_cache(
            &windows_request,
            &windows_location,
            false,
            limit,
            None,
            false,
        );
        assert!(matches!(primed, ThumbnailTerminal::Ready { .. }));
        let started = Instant::now();
        let windows = load_shell_thumbnail_with_cache(
            &windows_request,
            &windows_location,
            true,
            limit,
            None,
            false,
        );
        let windows_us = started.elapsed().as_micros();
        assert!(matches!(windows, ThumbnailTerminal::Ready { .. }));

        // Prime an isolated project cache with a third image, then measure its exact
        // serialized BC7 hit without consulting the Shell provider.
        let cache_root = fixtures.path().join("project-cache");
        let disk = crate::icon_disk_cache::ShellIconDiskCache::with_root(cache_root);
        let project_request = benchmark_request(b"benchmark-project-disk");
        let project_location = LocationDescriptor::file_system(fixtures.path().join("mode-3.png"));
        let primed = load_shell_thumbnail_with_cache(
            &project_request,
            &project_location,
            false,
            limit,
            Some(&disk),
            true,
        );
        assert!(matches!(primed, ThumbnailTerminal::Ready { .. }));
        let disk_key = disk_key(&project_request, &project_location);
        let deadline = Instant::now() + Duration::from_secs(5);
        while disk.load(&disk_key).is_none() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        let started = Instant::now();
        let project = load_shell_thumbnail_with_cache(
            &project_request,
            &project_location,
            false,
            limit,
            Some(&disk),
            true,
        );
        let project_us = started.elapsed().as_micros();
        assert!(matches!(
            project,
            ThumbnailTerminal::Compressed {
                source: ThumbnailSource::DiskCache,
                ..
            }
        ));

        println!(
            "thumbnail-cache-benchmark no_disk_us={no_disk_us} no_disk_source={} windows_cache_us={windows_us} windows_cache_source={} project_disk_us={project_us} project_disk_source={}",
            terminal_source(&no_disk),
            terminal_source(&windows),
            terminal_source(&project),
        );
        crate::icon_disk_cache::set_shell_bc7_runtime_gates(false, false);
    }

    #[test]
    fn real_shell_thumbnail_matrix_is_opt_in_and_owned() {
        if std::env::var_os("EXPLORER_RUN_REAL_SHELL_THUMBNAIL").is_none() {
            return;
        }
        let _apartment = crate::sta::ApartmentGuard::initialize().expect("STA");
        let location = LocationDescriptor::file_system(std::env::current_exe().expect("exe"));
        let deadline =
            RequestDeadline::after(Instant::now(), Duration::from_secs(3)).expect("deadline");
        let context =
            RequestContext::new(TabId::new(), Generation::default()).with_deadline(deadline);
        let request = ThumbnailRequest::new(
            context,
            ThumbnailRequestKey {
                item_id: ShellItemId::from_provider_bytes([1]).expect("id"),
                physical_size: 96,
                dpi: 96,
                mode: ThumbnailMode::Thumbnail,
                source_generation: 1,
                theme: ShellIconTheme::Light,
                association_generation: 1,
                overlay_generation: 1,
            },
            ThumbnailPriority::ActiveVisible,
        );
        let terminal = load_shell_thumbnail(
            &request,
            &location,
            false,
            RoadmapLimits::default().thumbnail_memory_bytes,
        );
        assert!(!matches!(terminal, ThumbnailTerminal::Failed(_)));
    }

    #[test]
    fn supplied_jpeg_preview_fixture_decodes_to_bounded_owned_pixels_when_present() {
        let path = std::path::Path::new(r"E:\av_out\326KJN-003.mp4.jpg");
        if !path.is_file() {
            return;
        }
        let _apartment = crate::sta::ApartmentGuard::initialize().expect("STA");
        let deadline = RequestDeadline::after(Instant::now(), Duration::from_secs(5))
            .expect("preview deadline");
        let request = ThumbnailRequest::new(
            RequestContext::new(TabId::new(), Generation::default()).with_deadline(deadline),
            ThumbnailRequestKey {
                item_id: ShellItemId::from_provider_bytes(b"supplied-jpeg-preview".to_vec())
                    .expect("item id"),
                physical_size: 512,
                dpi: 96,
                mode: ThumbnailMode::Thumbnail,
                source_generation: 1,
                theme: ShellIconTheme::Light,
                association_generation: 1,
                overlay_generation: 0,
            },
            ThumbnailPriority::ActiveVisible,
        );
        let limit = RoadmapLimits::default().thumbnail_memory_bytes;
        let terminal = load_shell_thumbnail(
            &request,
            &LocationDescriptor::file_system(path),
            false,
            limit,
        );
        let ThumbnailTerminal::Ready { pixels, .. } = terminal else {
            panic!("supplied JPEG did not produce a real preview thumbnail");
        };
        pixels.validate(limit).expect("bounded preview pixels");
        assert!(pixels.width > 1 && pixels.height > 1);
    }

    #[test]
    fn real_shell_retrieval_matrix_is_bounded_and_truthfully_classified() {
        let _apartment = crate::sta::ApartmentGuard::initialize().expect("STA");
        let fixtures = tempfile::tempdir().expect("thumbnail fixtures");
        let escaped = fixtures.path().display().to_string().replace('\'', "''");
        let image_script = format!(
            "$d='{escaped}'; Add-Type -AssemblyName System.Drawing; $b=[Drawing.Bitmap]::new(8,6); $g=[Drawing.Graphics]::FromImage($b); $g.Clear([Drawing.Color]::CornflowerBlue); $b.Save((Join-Path $d 'sample.png'),[Drawing.Imaging.ImageFormat]::Png); $b.Save((Join-Path $d 'sample.jpg'),[Drawing.Imaging.ImageFormat]::Jpeg); $b.RotateFlip([Drawing.RotateFlipType]::Rotate90FlipNone); $b.Save((Join-Path $d 'rotated.jpg'),[Drawing.Imaging.ImageFormat]::Jpeg); $b.Save((Join-Path $d 'sample.gif'),[Drawing.Imaging.ImageFormat]::Gif); $g.Dispose(); $b.Dispose()"
        );
        let status = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", &image_script])
            .status()
            .expect("create image fixtures");
        assert!(status.success());
        for (name, contents) in [
            ("sample.pdf", b"%PDF-1.1\n%%EOF\n".as_slice()),
            ("sample.rtf", b"{\\rtf1 matrix}".as_slice()),
            ("sample.wav", b"RIFF\x04\0\0\0WAVE".as_slice()),
            ("sample.unknown", b"unknown".as_slice()),
            ("archive-source.txt", b"archive".as_slice()),
        ] {
            std::fs::write(fixtures.path().join(name), contents).expect("matrix fixture");
        }
        let archive_status = std::process::Command::new("tar.exe")
            .current_dir(fixtures.path())
            .args(["-a", "-c", "-f", "sample.zip", "archive-source.txt"])
            .status()
            .expect("create archive fixture");
        assert!(archive_status.success());

        let mut locations = [
            "sample.jpg",
            "sample.png",
            "sample.gif",
            "rotated.jpg",
            "sample.pdf",
            "sample.rtf",
            "sample.wav",
            "sample.zip",
            "sample.unknown",
        ]
        .into_iter()
        .map(|name| (name, fixtures.path().join(name)))
        .collect::<Vec<_>>();
        locations.push(("folder", fixtures.path().to_path_buf()));
        for (index, (name, path)) in locations.into_iter().enumerate() {
            let deadline = RequestDeadline::after(Instant::now(), Duration::from_secs(5))
                .expect("matrix deadline");
            let context =
                RequestContext::new(TabId::new(), Generation::default()).with_deadline(deadline);
            let request = ThumbnailRequest::new(
                context,
                ThumbnailRequestKey {
                    item_id: ShellItemId::from_provider_bytes(vec![
                        b'm',
                        u8::try_from(index).unwrap_or(u8::MAX),
                    ])
                    .expect("matrix id"),
                    physical_size: 96,
                    dpi: 96,
                    mode: ThumbnailMode::Thumbnail,
                    source_generation: 1,
                    theme: ShellIconTheme::Light,
                    association_generation: 1,
                    overlay_generation: 1,
                },
                ThumbnailPriority::ActiveVisible,
            );
            let terminal = load_shell_thumbnail(
                &request,
                &LocationDescriptor::file_system(path),
                false,
                RoadmapLimits::default().thumbnail_memory_bytes,
            );
            if let ThumbnailTerminal::Ready { pixels, .. } = &terminal {
                pixels
                    .validate(RoadmapLimits::default().thumbnail_memory_bytes)
                    .expect("bounded provider pixels");
            }
            let classification = match terminal {
                ThumbnailTerminal::Ready { source, pixels } => {
                    format!("ready:{source:?}:{}x{}", pixels.width, pixels.height)
                }
                ThumbnailTerminal::Compressed { source, raster } => {
                    format!("compressed:{source:?}:{}x{}", raster.width, raster.height)
                }
                ThumbnailTerminal::Fallback(reason) => format!("fallback:{reason:?}"),
                ThumbnailTerminal::Failed(_) => "failed".to_owned(),
            };
            println!("real-thumbnail-matrix item={name} terminal={classification}");
        }
    }
}
