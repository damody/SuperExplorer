#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )
)]
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use std::io::{Read as _, Write as _};
#[cfg(windows)]
use std::mem::size_of_val;

fn main() {
    if std::env::args().any(|argument| argument == "--version-json") {
        println!(
            r#"{{"protocol":{},"build":"{}","arch":"x64","role":"worker"}}"#,
            explorer_extension_protocol::PROTOCOL_VERSION,
            env!("CARGO_PKG_VERSION")
        );
        return;
    }
    let mut input = std::io::stdin().lock();
    let mut length = [0_u8; 4];
    if input.read_exact(&mut length).is_err() {
        std::process::exit(11);
    }
    let length = usize::try_from(u32::from_le_bytes(length)).unwrap_or(usize::MAX);
    if length > explorer_extension_protocol::MAXIMUM_DESCRIPTOR_BYTES.saturating_add(9) {
        std::process::exit(11);
    }
    let mut request = vec![0_u8; length];
    if input.read_exact(&mut request).is_err() {
        std::process::exit(11);
    }
    let Ok(request) = explorer_extension_protocol::StartPayload::decode(&request) else {
        std::process::exit(12);
    };
    #[cfg(windows)]
    if request.operation != explorer_extension_protocol::OperationClass::ContextMenu
        && apply_restricted_thread_token(true).is_err()
    {
        std::process::exit(9);
    }
    #[cfg(windows)]
    let Ok(_apartment) = WorkerApartment::initialize() else {
        std::process::exit(10);
    };
    #[cfg(windows)]
    if request.operation == explorer_extension_protocol::OperationClass::Preview
        && request.flags & 0x4000_0000 != 0
    {
        run_preview_session(&request, &mut input);
        return;
    }
    let mode = if request.flags & 1 == 1 {
        std::str::from_utf8(&request.descriptor).unwrap_or("malformed")
    } else {
        "normal"
    };
    match mode {
        "hang" | "slow" => std::thread::sleep(std::time::Duration::from_secs(60)),
        "child-process" => {
            let mut command = std::process::Command::new("cmd.exe");
            command.args(["/d", "/c", "exit", "0"]);
            explorer_common::configure_background_command(&mut command);
            let child = command.spawn();
            if child.is_ok() {
                std::process::exit(6);
            }
        }
        "crash" | "reentrant" | "oversized" | "privilege" | "late-terminal" | "unload-failure" => {
            std::process::exit(7)
        }
        _ => {}
    }
    let response = if request.flags & 1 == 1 {
        b"success".to_vec()
    } else {
        execute_operation(&request)
    };
    let _ = std::io::stdout().write_all(&response);
}

#[cfg(windows)]
fn run_preview_session(
    request: &explorer_extension_protocol::StartPayload,
    input: &mut impl std::io::Read,
) {
    use explorer_extension_protocol::{PreviewMessage, PreviewStartPayload};
    use explorer_model::{Generation, PreviewHostBounds};
    use windows::Win32::UI::WindowsAndMessaging::{MSG, WM_KEYDOWN};

    let result = (|| {
        let payload = PreviewStartPayload::decode(&request.descriptor).map_err(|_| ())?;
        let location =
            explorer_extension_broker::decode_location_descriptor(&payload.item_descriptor)
                .map_err(|_| ())?;
        let parent = isize::try_from(payload.parent_hwnd).map_err(|_| ())?;
        let bounds = PreviewHostBounds {
            generation: Generation::new(payload.generation),
            left_physical: payload.left,
            top_physical: payload.top,
            width_physical: payload.width,
            height_physical: payload.height,
            dpi: payload.dpi,
        };
        explorer_shell_win::AttachedPreviewSession::attach(
            &location,
            payload.generation,
            parent,
            bounds,
        )
        .map_err(|_| ())
    })();
    let Ok(mut session) = result else {
        let _ = std::io::stdout().write_all(b"preview-unavailable\n");
        let _ = std::io::stdout().flush();
        return;
    };
    let _ = writeln!(
        std::io::stdout(),
        "preview-ready:{:?}",
        session.initialization_mode()
    );
    let _ = std::io::stdout().flush();
    loop {
        let mut length = [0_u8; 4];
        if input.read_exact(&mut length).is_err() {
            let _ = session.unload();
            return;
        }
        let length = usize::try_from(u32::from_le_bytes(length)).unwrap_or(usize::MAX);
        if length > explorer_extension_protocol::MAXIMUM_DESCRIPTOR_BYTES {
            let _ = session.unload();
            return;
        }
        let mut bytes = vec![0_u8; length];
        if input.read_exact(&mut bytes).is_err() {
            let _ = session.unload();
            return;
        }
        let Ok(message) = PreviewMessage::decode(&bytes) else {
            let _ = writeln!(std::io::stdout(), "preview-malformed");
            let _ = std::io::stdout().flush();
            continue;
        };
        let generation = match &message {
            PreviewMessage::Lookup { generation, .. }
            | PreviewMessage::SetBounds { generation, .. }
            | PreviewMessage::SetFocus { generation }
            | PreviewMessage::Accelerator { generation, .. }
            | PreviewMessage::Unload { generation }
            | PreviewMessage::Attach { generation, .. } => *generation,
        };
        if generation != session.generation() {
            let _ = writeln!(std::io::stdout(), "preview-stale");
            let _ = std::io::stdout().flush();
            continue;
        }
        let (response, unload) = match message {
            PreviewMessage::SetBounds {
                generation,
                left,
                top,
                width,
                height,
                dpi,
            } => (
                session.resize(PreviewHostBounds {
                    generation: Generation::new(generation),
                    left_physical: left,
                    top_physical: top,
                    width_physical: width,
                    height_physical: height,
                    dpi,
                }),
                false,
            ),
            PreviewMessage::SetFocus { .. } => (session.set_focus(), false),
            PreviewMessage::Accelerator { virtual_key, .. } => {
                let message = MSG {
                    message: WM_KEYDOWN,
                    wParam: windows::Win32::Foundation::WPARAM(
                        usize::try_from(virtual_key).unwrap_or_default(),
                    ),
                    ..MSG::default()
                };
                (session.translate_accelerator(&message), false)
            }
            PreviewMessage::Unload { .. } => (session.unload(), true),
            PreviewMessage::Lookup { .. } | PreviewMessage::Attach { .. } => {
                let _ = writeln!(std::io::stdout(), "preview-invalid-state");
                let _ = std::io::stdout().flush();
                continue;
            }
        };
        let _ = writeln!(
            std::io::stdout(),
            "{}",
            if response.is_ok() {
                "preview-ok"
            } else {
                "preview-failed"
            }
        );
        let _ = std::io::stdout().flush();
        if unload {
            return;
        }
    }
}

fn execute_operation(request: &explorer_extension_protocol::StartPayload) -> Vec<u8> {
    match request.operation {
        explorer_extension_protocol::OperationClass::Preview => {
            let Ok(path) = std::str::from_utf8(&request.descriptor) else {
                return b"preview-malformed".to_vec();
            };
            let location = explorer_model::LocationDescriptor::file_system(path);
            match explorer_shell_win::render_preview_in_worker(&location, 1, 640, 480) {
                Ok(mode) => format!("preview-ready:{mode:?}").into_bytes(),
                Err(_) => b"preview-unavailable".to_vec(),
            }
        }
        explorer_extension_protocol::OperationClass::ContextMenu => execute_context_menu(request),
        explorer_extension_protocol::OperationClass::Thumbnail => execute_thumbnail(request),
        explorer_extension_protocol::OperationClass::Namespace => execute_namespace(request),
    }
}

fn execute_context_menu(request: &explorer_extension_protocol::StartPayload) -> Vec<u8> {
    if request.flags & 0x8000_0000 != 0 {
        return execute_context_menu_payload(request);
    }
    let Ok(text) = std::str::from_utf8(&request.descriptor) else {
        return b"context-menu-malformed".to_vec();
    };
    let (path, requested_verb) = text
        .split_once('\0')
        .map_or((text, None), |(path, verb)| (path, Some(verb)));
    let item_path = std::path::PathBuf::from(path);
    let Some(parent) = item_path.parent() else {
        return b"context-menu-malformed".to_vec();
    };
    let Some(id) = explorer_model::ShellItemId::from_provider_bytes(b"broker-menu-item".to_vec())
    else {
        return b"context-menu-malformed".to_vec();
    };
    let target = explorer_model::ShellContextMenuTarget::Items {
        parent: explorer_model::LocationDescriptor::file_system(parent),
        items: vec![explorer_model::ItemDescriptor {
            id,
            location: explorer_model::LocationDescriptor::file_system(&item_path),
        }],
    };
    if let Some(verb) = requested_verb {
        let context_request = explorer_model::ContextMenuRequest {
            immersive_native_context_menus: false,
            color_scheme: explorer_model::ContextMenuColorScheme::Light,
            target,
            owner_window: 0,
            point: explorer_model::MenuPoint { x: 0, y: 0 },
            keyboard_invoked: true,
            invocation_profile: explorer_model::ContextMenuInvocationProfile::Explorer,
            paste_available: false,
            requested_verb: (verb != "__show__").then(|| verb.to_owned()),
            deadline_ms: 10_000,
        };
        return match explorer_shell_win::execute_context_menu_in_worker(&context_request) {
            Ok(explorer_model::ContextMenuOutcome::Invoked { command_offset }) => {
                format!("context-menu-invoked:{command_offset}").into_bytes()
            }
            Ok(explorer_model::ContextMenuOutcome::Delegated {
                command_offset,
                command,
                ..
            }) => format!(
                "context-menu-delegated:{command_offset}:{}",
                command.wire_name()
            )
            .into_bytes(),
            Ok(explorer_model::ContextMenuOutcome::Cancelled) => b"context-menu-cancelled".to_vec(),
            Ok(explorer_model::ContextMenuOutcome::ReplayRequested { x, y }) => {
                format!("context-menu-replay:{x}:{y}").into_bytes()
            }
            Ok(explorer_model::ContextMenuOutcome::InstallApk { serial, .. }) => {
                format!("context-menu-install-apk:{serial}").into_bytes()
            }
            Ok(explorer_model::ContextMenuOutcome::DownloadAdb { .. }) => {
                b"context-menu-download-adb".to_vec()
            }
            Ok(explorer_model::ContextMenuOutcome::Failed { .. }) | Err(_) => {
                b"context-menu-unavailable".to_vec()
            }
        };
    }
    match explorer_shell_win::query_context_menu_in_worker(&target) {
        Ok(count) if count > 0 => format!("context-menu-ready:{count}").into_bytes(),
        Ok(_) => b"context-menu-empty".to_vec(),
        Err(_) => b"context-menu-unavailable".to_vec(),
    }
}

fn execute_context_menu_payload(request: &explorer_extension_protocol::StartPayload) -> Vec<u8> {
    let Ok(payload) = explorer_extension_protocol::ContextMenuPayload::decode(&request.descriptor)
    else {
        return b"context-menu-malformed".to_vec();
    };
    let mut locations = Vec::with_capacity(payload.item_descriptors.len());
    for descriptor in &payload.item_descriptors {
        let Ok(location) = explorer_extension_broker::decode_location_descriptor(descriptor) else {
            return b"context-menu-malformed".to_vec();
        };
        locations.push(location);
    }
    let target = if payload.background {
        let Some(parent) = locations.into_iter().next() else {
            return b"context-menu-malformed".to_vec();
        };
        explorer_model::ShellContextMenuTarget::Background { parent }
    } else {
        let Some(first) = locations.first() else {
            return b"context-menu-malformed".to_vec();
        };
        let parent = first.path().and_then(std::path::Path::parent).map_or_else(
            || first.clone(),
            explorer_model::LocationDescriptor::file_system,
        );
        let mut items = Vec::with_capacity(locations.len());
        for (index, location) in locations.into_iter().enumerate() {
            let Some(id) = explorer_model::ShellItemId::from_provider_bytes(
                u64::try_from(index).unwrap_or(u64::MAX).to_le_bytes(),
            ) else {
                return b"context-menu-malformed".to_vec();
            };
            items.push(explorer_model::ItemDescriptor { id, location });
        }
        explorer_model::ShellContextMenuTarget::Items { parent, items }
    };
    let context_request = explorer_model::ContextMenuRequest {
        immersive_native_context_menus: payload.immersive_native_context_menus,
        color_scheme: if payload.dark_theme {
            explorer_model::ContextMenuColorScheme::Dark
        } else {
            explorer_model::ContextMenuColorScheme::Light
        },
        target,
        owner_window: payload.owner_hwnd,
        point: explorer_model::MenuPoint {
            x: payload.point_x,
            y: payload.point_y,
        },
        keyboard_invoked: payload.keyboard_invoked,
        invocation_profile: if payload.invocation_profile == 1 {
            explorer_model::ContextMenuInvocationProfile::ExplorerExtended
        } else {
            explorer_model::ContextMenuInvocationProfile::Explorer
        },
        paste_available: payload.paste_available,
        requested_verb: payload.verb,
        deadline_ms: 10_000,
    };
    if request.flags & 0x4000_0000 != 0 {
        if request.flags & 0x2000_0000 != 0 {
            return match explorer_shell_win::query_context_menu_snapshot_in_worker_with_profile(
                &context_request.target,
                context_request.invocation_profile,
            ) {
                Ok(snapshot) if snapshot.command_count > 0 => {
                    let fingerprints = snapshot
                        .label_fingerprints
                        .iter()
                        .map(|value| format!("{value:016x}"))
                        .collect::<Vec<_>>()
                        .join(",");
                    format!(
                        "context-menu-snapshot:{}:{}:{fingerprints}",
                        snapshot.command_count,
                        u8::from(context_request.invocation_profile.extended_verbs())
                    )
                    .into_bytes()
                }
                Ok(_) => b"context-menu-empty".to_vec(),
                Err(_) => b"context-menu-unavailable".to_vec(),
            };
        }
        return match explorer_shell_win::query_context_menu_in_worker_with_profile(
            &context_request.target,
            context_request.invocation_profile,
        ) {
            Ok(count) if count > 0 => format!(
                "context-menu-ready:{count}:{}",
                u8::from(context_request.invocation_profile.extended_verbs())
            )
            .into_bytes(),
            Ok(_) => b"context-menu-empty".to_vec(),
            Err(_) => b"context-menu-unavailable".to_vec(),
        };
    }
    match explorer_shell_win::execute_context_menu_in_worker(&context_request) {
        Ok(explorer_model::ContextMenuOutcome::Invoked { command_offset }) => {
            format!("context-menu-invoked:{command_offset}").into_bytes()
        }
        Ok(explorer_model::ContextMenuOutcome::Delegated {
            command_offset,
            command,
            ..
        }) => format!(
            "context-menu-delegated:{command_offset}:{}",
            command.wire_name()
        )
        .into_bytes(),
        Ok(explorer_model::ContextMenuOutcome::Cancelled) => b"context-menu-cancelled".to_vec(),
        Ok(explorer_model::ContextMenuOutcome::ReplayRequested { x, y }) => {
            format!("context-menu-replay:{x}:{y}").into_bytes()
        }
        Ok(explorer_model::ContextMenuOutcome::InstallApk { serial, .. }) => {
            format!("context-menu-install-apk:{serial}").into_bytes()
        }
        Ok(explorer_model::ContextMenuOutcome::DownloadAdb { .. }) => {
            b"context-menu-download-adb".to_vec()
        }
        Ok(explorer_model::ContextMenuOutcome::Failed { .. }) | Err(_) => {
            b"context-menu-unavailable".to_vec()
        }
    }
}

fn execute_thumbnail(request: &explorer_extension_protocol::StartPayload) -> Vec<u8> {
    if request.flags & 0x8000_0000 != 0 {
        return execute_thumbnail_payload(request);
    }
    let Ok(path) = std::str::from_utf8(&request.descriptor) else {
        return b"thumbnail-malformed".to_vec();
    };
    let Some(id) = explorer_model::ShellItemId::from_provider_bytes(b"broker-thumbnail".to_vec())
    else {
        return b"thumbnail-malformed".to_vec();
    };
    let context = explorer_model::RequestContext::new(
        explorer_model::TabId::new(),
        explorer_model::Generation::new(1),
    );
    let thumbnail_request = explorer_model::ThumbnailRequest::new(
        context,
        explorer_model::ThumbnailRequestKey {
            item_id: id,
            physical_size: u16::try_from((request.flags >> 16).clamp(1, 4_096)).unwrap_or(256),
            dpi: 96,
            mode: explorer_model::ThumbnailMode::Thumbnail,
            source_generation: 1,
            theme: explorer_model::ShellIconTheme::Light,
            association_generation: 0,
            overlay_generation: 0,
        },
        explorer_model::ThumbnailPriority::ActiveVisible,
    );
    match explorer_shell_win::load_shell_thumbnail_rgba(
        &thumbnail_request,
        &explorer_model::LocationDescriptor::file_system(path),
        request.flags & 2 != 0,
        64 * 1024 * 1024,
    ) {
        explorer_model::ThumbnailTerminal::Ready { source, pixels } => format!(
            "thumbnail-ready:{source:?}:{}:{}:{}:{}",
            pixels.width,
            pixels.height,
            pixels.stride,
            pixels.bytes.len()
        )
        .into_bytes(),
        explorer_model::ThumbnailTerminal::Compressed { .. } => b"thumbnail-failed".to_vec(),
        explorer_model::ThumbnailTerminal::Fallback(reason) => {
            format!("thumbnail-fallback:{reason:?}").into_bytes()
        }
        explorer_model::ThumbnailTerminal::Failed(_) => b"thumbnail-failed".to_vec(),
    }
}

fn execute_thumbnail_payload(request: &explorer_extension_protocol::StartPayload) -> Vec<u8> {
    let Ok(payload) = explorer_extension_protocol::ThumbnailPayload::decode(&request.descriptor)
    else {
        return b"thumbnail-malformed".to_vec();
    };
    let Ok(location) =
        explorer_extension_broker::decode_location_descriptor(&payload.item_descriptor)
    else {
        return b"thumbnail-malformed".to_vec();
    };
    let Some(id) = explorer_model::ShellItemId::from_provider_bytes(b"broker-thumbnail".to_vec())
    else {
        return b"thumbnail-malformed".to_vec();
    };
    let context = explorer_model::RequestContext::new(
        explorer_model::TabId::new(),
        explorer_model::Generation::new(1),
    );
    let thumbnail_request = explorer_model::ThumbnailRequest::new(
        context,
        explorer_model::ThumbnailRequestKey {
            item_id: id,
            physical_size: payload.physical_size,
            dpi: payload.dpi,
            mode: explorer_model::ThumbnailMode::Thumbnail,
            source_generation: 1,
            theme: explorer_model::ShellIconTheme::Light,
            association_generation: 0,
            overlay_generation: 0,
        },
        explorer_model::ThumbnailPriority::ActiveVisible,
    );
    let terminal = explorer_shell_win::load_shell_thumbnail_rgba(
        &thumbnail_request,
        &location,
        payload.cache_only,
        explorer_extension_protocol::MAXIMUM_FRAME_BYTES.saturating_sub(64),
    );
    let result = match terminal {
        explorer_model::ThumbnailTerminal::Ready { source, pixels } => {
            explorer_extension_protocol::ThumbnailResultPayload::Ready {
                source: match source {
                    explorer_model::ThumbnailSource::MemoryCache => 0,
                    explorer_model::ThumbnailSource::DiskCache => 1,
                    explorer_model::ThumbnailSource::WindowsCache => 2,
                    explorer_model::ThumbnailSource::Provider => 3,
                    explorer_model::ThumbnailSource::ShellIcon => 4,
                },
                width: pixels.width,
                height: pixels.height,
                stride: pixels.stride,
                pixels: pixels.bytes,
            }
        }
        explorer_model::ThumbnailTerminal::Compressed { .. } => {
            explorer_extension_protocol::ThumbnailResultPayload::Failed
        }
        explorer_model::ThumbnailTerminal::Fallback(reason) => {
            explorer_extension_protocol::ThumbnailResultPayload::Fallback {
                reason: match reason {
                    explorer_model::ThumbnailFallbackReason::Offline => 1,
                    explorer_model::ThumbnailFallbackReason::Unsupported => 2,
                    explorer_model::ThumbnailFallbackReason::Timeout => 3,
                    explorer_model::ThumbnailFallbackReason::Cancelled => 4,
                    explorer_model::ThumbnailFallbackReason::Corrupt => 5,
                    explorer_model::ThumbnailFallbackReason::ProviderFailure => 6,
                    explorer_model::ThumbnailFallbackReason::ResourceLimit => 7,
                },
            }
        }
        explorer_model::ThumbnailTerminal::Failed(_) => {
            explorer_extension_protocol::ThumbnailResultPayload::Failed
        }
    };
    result
        .encode()
        .unwrap_or_else(|_| b"thumbnail-malformed".to_vec())
}

fn execute_namespace(request: &explorer_extension_protocol::StartPayload) -> Vec<u8> {
    if request.flags & 0x8000_0000 != 0 {
        let Ok(location) =
            explorer_extension_broker::decode_location_descriptor(&request.descriptor)
        else {
            return b"namespace-malformed".to_vec();
        };
        let maximum = usize::try_from(request.flags & 0x0fff)
            .unwrap_or(256)
            .clamp(1, 4_096);
        return match explorer_shell_win::enumerate_namespace_in_worker(&location, maximum) {
            Ok(mut entries) => loop {
                match serde_json::to_vec(&entries) {
                    Ok(bytes)
                        if bytes.len() <= explorer_extension_protocol::MAXIMUM_FRAME_BYTES =>
                    {
                        break bytes;
                    }
                    Ok(_) if !entries.is_empty() => {
                        entries.pop();
                    }
                    Ok(_) | Err(_) => break b"namespace-unavailable".to_vec(),
                }
            },
            Err(_) => b"namespace-unavailable".to_vec(),
        };
    }
    let Ok(path) = std::str::from_utf8(&request.descriptor) else {
        return b"namespace-malformed".to_vec();
    };
    let maximum = usize::try_from((request.flags >> 16).clamp(1, 4_096)).unwrap_or(256);
    match explorer_shell_win::enumerate_namespace_in_worker(
        &explorer_model::LocationDescriptor::ParsingName(path.to_owned()),
        maximum,
    ) {
        Ok(entries) => {
            let capability_rows = entries
                .iter()
                .filter(|entry| entry.metadata.namespace_capabilities.bits() != 0)
                .count();
            format!("namespace-ready:{}:{capability_rows}", entries.len()).into_bytes()
        }
        Err(_) => b"namespace-unavailable".to_vec(),
    }
}

#[cfg(windows)]
struct WorkerApartment;

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "each disposable worker initializes and balances exactly one COM apartment"
)]
impl WorkerApartment {
    fn initialize() -> windows::core::Result<Self> {
        use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()? };
        Ok(Self)
    }
}

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "balances the successful worker CoInitializeEx call"
)]
impl Drop for WorkerApartment {
    fn drop(&mut self) {
        unsafe { windows::Win32::System::Com::CoUninitialize() };
    }
}

/// Applies a max-privilege-disabled impersonation token before any extension class can load.
#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "audited token creation and thread impersonation establish the worker security boundary"
)]
fn apply_restricted_thread_token(low_integrity: bool) -> windows::core::Result<()> {
    use windows::Win32::{
        Foundation::{CloseHandle, HANDLE},
        Security::{
            CreateRestrictedToken, CreateWellKnownSid, DISABLE_MAX_PRIVILEGE, DuplicateTokenEx,
            PSID, SID_AND_ATTRIBUTES, SecurityImpersonation, SetTokenInformation,
            TOKEN_ADJUST_DEFAULT, TOKEN_DUPLICATE, TOKEN_IMPERSONATE, TOKEN_MANDATORY_LABEL,
            TOKEN_QUERY, TokenImpersonation, TokenIntegrityLevel, WinLowLabelSid,
        },
        System::{
            SystemServices::SE_GROUP_INTEGRITY,
            Threading::{GetCurrentProcess, OpenProcessToken, SetThreadToken},
        },
    };

    struct OwnedHandle(HANDLE);
    impl Drop for OwnedHandle {
        fn drop(&mut self) {
            // SAFETY: every OwnedHandle receives one real token handle and owns it uniquely.
            let _ = unsafe { CloseHandle(self.0) };
        }
    }

    let mut process_token = HANDLE::default();
    unsafe {
        OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_DUPLICATE | TOKEN_QUERY | TOKEN_IMPERSONATE | TOKEN_ADJUST_DEFAULT,
            &raw mut process_token,
        )?;
    }
    let process_token = OwnedHandle(process_token);
    let mut restricted = HANDLE::default();
    unsafe {
        CreateRestrictedToken(
            process_token.0,
            DISABLE_MAX_PRIVILEGE,
            None,
            None,
            None,
            &raw mut restricted,
        )?;
    }
    let restricted = OwnedHandle(restricted);
    if low_integrity {
        let mut low_integrity_sid = [0_u8; 68];
        let mut low_integrity_sid_bytes =
            u32::try_from(low_integrity_sid.len()).unwrap_or(u32::MAX);
        let low_integrity_sid = PSID(low_integrity_sid.as_mut_ptr().cast());
        unsafe {
            CreateWellKnownSid(
                WinLowLabelSid,
                None,
                Some(low_integrity_sid),
                &raw mut low_integrity_sid_bytes,
            )?;
        }
        let label = TOKEN_MANDATORY_LABEL {
            Label: SID_AND_ATTRIBUTES {
                Sid: low_integrity_sid,
                Attributes: u32::try_from(SE_GROUP_INTEGRITY).unwrap_or_default(),
            },
        };
        let label_bytes = u32::try_from(size_of_val(&label))
            .unwrap_or(u32::MAX)
            .saturating_add(low_integrity_sid_bytes);
        unsafe {
            SetTokenInformation(
                restricted.0,
                TokenIntegrityLevel,
                (&raw const label).cast(),
                label_bytes,
            )?;
        }
    }
    let mut impersonation = HANDLE::default();
    unsafe {
        DuplicateTokenEx(
            restricted.0,
            TOKEN_QUERY | TOKEN_IMPERSONATE,
            None,
            SecurityImpersonation,
            TokenImpersonation,
            &raw mut impersonation,
        )?;
    }
    let impersonation = OwnedHandle(impersonation);
    unsafe { SetThreadToken(None, Some(impersonation.0)) }
}
