//! Native Preview Handler lookup and single-owner host lifecycle.
#![allow(
    unsafe_code,
    reason = "Preview Handler COM activation, initialization, HWND hosting, and message forwarding require audited Windows FFI"
)]

use std::{os::windows::ffi::OsStrExt as _, path::PathBuf};

use explorer_common::{ExplorerError, ExplorerErrorKind};
use explorer_model::{LocationDescriptor, PreviewInitializationMode};
use windows::{
    Win32::{
        Foundation::{HWND, RECT},
        System::{
            Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, STGM_READ},
            Registry::{HKEY_CLASSES_ROOT, RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ, RegGetValueW},
        },
        UI::{
            HiDpi::{
                GetAwarenessFromDpiAwarenessContext, GetThreadDpiAwarenessContext,
                GetWindowDpiAwarenessContext,
            },
            Shell::{
                ASSOCF_NONE,
                ASSOCSTR_SHELLEXTENSION,
                AssocQueryStringW,
                IInitializeWithItem as PreviewItemInitializer, // architecture-check: allow worker-only adapter
                IPreviewHandler, // architecture-check: allow worker-only adapter
                PropertiesSystem::{
                    IInitializeWithFile as PreviewFileInitializer, // architecture-check: allow worker-only adapter
                    IInitializeWithStream as PreviewStreamInitializer, // architecture-check: allow worker-only adapter
                },
                SHCLSIDFromString,
                SHCreateStreamOnFileEx,
            },
            WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DestroyWindow, IsWindow, MSG, RegisterClassW,
                SWP_NOACTIVATE, SWP_NOZORDER, SetWindowPos, WINDOW_EX_STYLE, WNDCLASSW, WS_CHILD,
                WS_CLIPCHILDREN, WS_CLIPSIBLINGS, WS_POPUP, WS_VISIBLE,
            },
        },
    },
    core::{GUID, Interface as _, PCWSTR, PWSTR, w},
};

const PREVIEW_HANDLER_ASSOCIATION: &str = "{8895b1c6-b41f-4c1c-a562-0d564250836f}";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreviewLookup {
    pub clsid: [u8; 16],
    pub extension: String,
    pub server_path: PathBuf,
    pub server_machine: u16,
}

impl PreviewLookup {
    /// Resolves the public Preview Handler association for a filesystem extension.
    ///
    /// # Errors
    /// Returns a recoverable extension error when the item has no extension or
    /// Windows has no valid Preview Handler registration for it.
    pub fn for_location(location: &LocationDescriptor) -> Result<Self, ExplorerError> {
        let path = location.path().ok_or_else(|| {
            preview_error(
                "lookup preview handler",
                "the selected Shell item has no filesystem extension",
            )
        })?;
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                preview_error(
                    "lookup preview handler",
                    "the selected item has no registered file extension",
                )
            })?;
        let extension = format!(".{extension}");
        let association = wide(&extension);
        let extra = wide(PREVIEW_HANDLER_ASSOCIATION);
        let mut output = vec![0_u16; 96];
        let mut length = u32::try_from(output.len()).unwrap_or(u32::MAX);
        // SAFETY: all strings are NUL terminated and output length describes writable storage.
        unsafe {
            AssocQueryStringW(
                ASSOCF_NONE,
                ASSOCSTR_SHELLEXTENSION,
                PCWSTR(association.as_ptr()),
                PCWSTR(extra.as_ptr()),
                Some(PWSTR(output.as_mut_ptr())),
                &raw mut length,
            )
        }
        .ok()
        .map_err(|error| preview_native_error("lookup preview handler", &error))?;
        let used = usize::try_from(length)
            .unwrap_or(output.len())
            .min(output.len());
        let clsid_text = &output[..used];
        // SAFETY: AssocQueryStringW produced a NUL-terminated CLSID string in this buffer.
        let clsid = unsafe { SHCLSIDFromString(PCWSTR(clsid_text.as_ptr())) }.map_err(|_| {
            preview_error(
                "lookup preview handler",
                "the registered Preview Handler CLSID is malformed",
            )
        })?;
        let server_path = registered_server_path(&clsid)?;
        let server_machine = pe_machine(&server_path)?;
        if server_machine != 0x8664 {
            return Err(preview_error(
                "validate preview handler bitness",
                format!(
                    "registered server machine 0x{server_machine:04x} is incompatible with the x64 preview worker"
                ),
            ));
        }
        Ok(Self {
            clsid: clsid.to_u128().to_be_bytes(),
            extension,
            server_path,
            server_machine,
        })
    }

    fn guid(&self) -> GUID {
        GUID::from_u128(u128::from_be_bytes(self.clsid))
    }
}

fn registered_server_path(clsid: &GUID) -> Result<PathBuf, ExplorerError> {
    let value = clsid.to_u128();
    let clsid_text = format!(
        "{{{:08x}-{:04x}-{:04x}-{:04x}-{:012x}}}",
        value >> 96,
        (value >> 80) & 0xffff,
        (value >> 64) & 0xffff,
        (value >> 48) & 0xffff,
        value & 0xffff_ffff_ffff,
    );
    let key = wide(&format!(r"CLSID\{clsid_text}\InprocServer32"));
    let mut output = vec![0_u16; 32_768];
    let mut byte_count = u32::try_from(output.len() * size_of::<u16>()).unwrap_or(u32::MAX);
    // SAFETY: the registry key is NUL terminated and the output buffer/byte count remain valid
    // for this synchronous read. REG_EXPAND_SZ is expanded by RegGetValueW by default.
    let status = unsafe {
        RegGetValueW(
            HKEY_CLASSES_ROOT,
            PCWSTR(key.as_ptr()),
            PCWSTR::null(),
            RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ,
            None,
            Some(output.as_mut_ptr().cast()),
            Some(&raw mut byte_count),
        )
    };
    if status.is_err() {
        return Err(preview_error(
            "read preview handler registration",
            format!("Win32 registry status {}", status.0),
        ));
    }
    let words = usize::try_from(byte_count)
        .unwrap_or(0)
        .saturating_div(size_of::<u16>())
        .min(output.len());
    let end = output[..words]
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(words);
    let value = String::from_utf16_lossy(&output[..end]);
    let path = PathBuf::from(value.trim().trim_matches('"'));
    if !path.is_file() {
        return Err(preview_error(
            "validate preview handler registration",
            "registered InprocServer32 does not resolve to a file",
        ));
    }
    Ok(path)
}

fn pe_machine(path: &std::path::Path) -> Result<u16, ExplorerError> {
    let bytes = std::fs::read(path).map_err(|error| {
        preview_error(
            "read preview handler image",
            format!("registered server could not be read: {error}"),
        )
    })?;
    if bytes.get(..2) != Some(b"MZ") {
        return Err(preview_error(
            "validate preview handler image",
            "registered server has no DOS image header",
        ));
    }
    let offset = bytes
        .get(0x3c..0x40)
        .and_then(|value| value.try_into().ok())
        .map(u32::from_le_bytes)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| preview_error("validate preview handler image", "invalid PE offset"))?;
    if bytes.get(offset..offset.saturating_add(4)) != Some(b"PE\0\0") {
        return Err(preview_error(
            "validate preview handler image",
            "registered server has no PE signature",
        ));
    }
    bytes
        .get(offset.saturating_add(4)..offset.saturating_add(6))
        .and_then(|value| value.try_into().ok())
        .map(u16::from_le_bytes)
        .ok_or_else(|| preview_error("validate preview handler image", "missing PE machine field"))
}

/// Owns one initialized handler on one broker STA. Drop performs idempotent unload.
pub struct PreviewHandlerHost {
    handler: IPreviewHandler, // architecture-check: allow worker-only adapter
    pub lookup: PreviewLookup,
    pub initialization_mode: PreviewInitializationMode,
    generation: u64,
    unloaded: bool,
}

#[allow(
    clippy::missing_errors_doc,
    reason = "each lifecycle method returns the same recoverable stale-generation or native Preview Handler error documented by the host type"
)]
impl PreviewHandlerHost {
    /// Activates and initializes a registered handler using the least capable
    /// supported public interface in file, stream, then Shell-item order.
    pub fn initialize(
        location: &LocationDescriptor,
        generation: u64,
    ) -> Result<Self, ExplorerError> {
        let lookup = PreviewLookup::for_location(location)?;
        // SAFETY: CLSID came from the public association API; the disposable broker STA owns it.
        let handler: IPreviewHandler = // architecture-check: allow worker-only adapter
            unsafe { CoCreateInstance(&lookup.guid(), None, CLSCTX_INPROC_SERVER) }
                .map_err(|error| preview_native_error("activate preview handler", &error))?;
        let initialization_mode = initialize_handler(&handler, location)?;
        Ok(Self {
            handler,
            lookup,
            initialization_mode,
            generation,
            unloaded: false,
        })
    }

    pub const fn generation(&self) -> u64 {
        self.generation
    }

    pub fn set_window(
        &self,
        generation: u64,
        parent: isize,
        bounds: RECT,
    ) -> Result<(), ExplorerError> {
        self.ensure_current(generation)?;
        // SAFETY: the app supplies an owned live host HWND and a copied physical-pixel RECT.
        unsafe {
            self.handler
                .SetWindow(HWND(parent as *mut _), &raw const bounds)
        }
        .map_err(|error| preview_native_error("set preview host window", &error))
    }

    pub fn set_rect(&self, generation: u64, bounds: RECT) -> Result<(), ExplorerError> {
        self.ensure_current(generation)?;
        // SAFETY: bounds is copied and the handler remains owned by this STA.
        unsafe { self.handler.SetRect(&raw const bounds) }
            .map_err(|error| preview_native_error("resize preview handler", &error))
    }

    pub fn do_preview(&self, generation: u64) -> Result<(), ExplorerError> {
        self.ensure_current(generation)?;
        // SAFETY: initialization completed and the handler remains on its owning STA.
        unsafe { self.handler.DoPreview() } // architecture-check: allow worker-only adapter
            .map_err(|error| preview_native_error("render preview handler", &error))
    }

    pub fn set_focus(&self, generation: u64) -> Result<(), ExplorerError> {
        self.ensure_current(generation)?;
        // SAFETY: focus is forwarded only to the current handler generation.
        unsafe { self.handler.SetFocus() }
            .map_err(|error| preview_native_error("focus preview handler", &error))
    }

    pub fn query_focus(&self, generation: u64) -> Result<isize, ExplorerError> {
        self.ensure_current(generation)?;
        // SAFETY: returns a borrowed HWND value; no ownership is transferred.
        unsafe { self.handler.QueryFocus() }
            .map(|value| value.0 as isize)
            .map_err(|error| preview_native_error("query preview focus", &error))
    }

    pub fn translate_accelerator(
        &self,
        generation: u64,
        message: &MSG,
    ) -> Result<(), ExplorerError> {
        self.ensure_current(generation)?;
        // SAFETY: the message is copied by the synchronous handler call.
        unsafe { self.handler.TranslateAccelerator(message) }
            .map_err(|error| preview_native_error("forward preview accelerator", &error))
    }

    pub fn unload(&mut self, generation: u64) -> Result<(), ExplorerError> {
        if self.generation != generation {
            return Err(preview_error(
                "preview generation",
                "stale preview handler unload request",
            ));
        }
        if self.unloaded {
            return Ok(());
        }
        self.unloaded = true;
        // SAFETY: this host is the single owner and invokes Unload at most once.
        unsafe { self.handler.Unload() }
            .map_err(|error| preview_native_error("unload preview handler", &error))
    }

    fn ensure_current(&self, generation: u64) -> Result<(), ExplorerError> {
        if self.generation == generation && !self.unloaded {
            Ok(())
        } else {
            Err(preview_error(
                "preview generation",
                "stale or unloaded preview handler request",
            ))
        }
    }
}

impl Drop for PreviewHandlerHost {
    fn drop(&mut self) {
        if !self.unloaded {
            self.unloaded = true;
            // SAFETY: Drop is the final idempotent owner fallback on the broker STA.
            let _ = unsafe { self.handler.Unload() };
        }
    }
}

/// Owned hidden window used only inside the disposable preview worker.
struct PreviewHostWindow(HWND);

impl PreviewHostWindow {
    fn create(width: u32, height: u32) -> Result<Self, ExplorerError> {
        Self::create_with_parent(None, 0, 0, width, height)
    }

    fn create_attached(
        parent: HWND,
        left: i32,
        top: i32,
        width: u32,
        height: u32,
    ) -> Result<Self, ExplorerError> {
        if !unsafe { IsWindow(Some(parent)) }.as_bool() {
            return Err(preview_error(
                "attach preview host window",
                "the app preview boundary HWND is no longer valid",
            ));
        }
        let parent_awareness =
            unsafe { GetAwarenessFromDpiAwarenessContext(GetWindowDpiAwarenessContext(parent)) };
        let worker_awareness =
            unsafe { GetAwarenessFromDpiAwarenessContext(GetThreadDpiAwarenessContext()) };
        if parent_awareness != worker_awareness {
            return Err(preview_error(
                "attach preview host window",
                "the app and worker DPI awareness modes are incompatible",
            ));
        }
        Self::create_with_parent(Some(parent), left, top, width, height)
    }

    fn create_with_parent(
        parent: Option<HWND>,
        left: i32,
        top: i32,
        width: u32,
        height: u32,
    ) -> Result<Self, ExplorerError> {
        let module = unsafe { windows::Win32::System::LibraryLoader::GetModuleHandleW(None) }
            .map_err(|error| preview_native_error("load preview host module", &error))?;
        let class = WNDCLASSW {
            hInstance: module.into(),
            lpszClassName: w!("RustGpuiExplorerPreviewWorkerHost"),
            lpfnWndProc: Some(preview_window_proc),
            ..WNDCLASSW::default()
        };
        // SAFETY: the static class description remains valid for process lifetime.
        let _ = unsafe { RegisterClassW(&raw const class) };
        let width = i32::try_from(width).unwrap_or(i32::MAX);
        let height = i32::try_from(height).unwrap_or(i32::MAX);
        // SAFETY: the registered class is process-owned and no borrowed create parameter is used.
        let style = if parent.is_some() {
            WS_CHILD | WS_VISIBLE | WS_CLIPCHILDREN | WS_CLIPSIBLINGS
        } else {
            WS_POPUP | WS_CLIPCHILDREN
        };
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                class.lpszClassName,
                w!(""),
                style,
                left,
                top,
                width,
                height,
                parent,
                None,
                Some(module.into()),
                None,
            )
        }
        .map_err(|error| preview_native_error("create preview worker host", &error))?;
        Ok(Self(hwnd))
    }

    fn resize(&self, left: i32, top: i32, width: u32, height: u32) -> Result<(), ExplorerError> {
        let width = i32::try_from(width).map_err(|_| {
            preview_error(
                "resize preview host window",
                "preview width exceeds Win32 bounds",
            )
        })?;
        let height = i32::try_from(height).map_err(|_| {
            preview_error(
                "resize preview host window",
                "preview height exceeds Win32 bounds",
            )
        })?;
        unsafe {
            SetWindowPos(
                self.0,
                None,
                left,
                top,
                width,
                height,
                SWP_NOACTIVATE | SWP_NOZORDER,
            )
        }
        .map_err(|error| preview_native_error("resize preview host window", &error))
    }
}

impl Drop for PreviewHostWindow {
    fn drop(&mut self) {
        // SAFETY: this wrapper uniquely owns the worker HWND on its creating STA.
        let _ = unsafe { DestroyWindow(self.0) };
    }
}

unsafe extern "system" fn preview_window_proc(
    hwnd: HWND,
    message: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    // SAFETY: all messages are delegated unchanged to the Windows default procedure.
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

/// Activates, hosts, renders, and unloads one Preview Handler entirely inside the worker.
///
/// # Errors
/// Returns a recoverable typed error for lookup, activation, initialization, HWND, render, or
/// unload failure. Drop still releases every successfully acquired resource.
pub fn render_preview_in_worker(
    location: &LocationDescriptor,
    generation: u64,
    width: u32,
    height: u32,
) -> Result<PreviewInitializationMode, ExplorerError> {
    if width == 0 || height == 0 || width > 16_384 || height > 16_384 {
        return Err(preview_error(
            "validate preview bounds",
            "preview worker bounds are outside the public contract",
        ));
    }
    let window = PreviewHostWindow::create(width, height)?;
    let mut host = PreviewHandlerHost::initialize(location, generation)?;
    let bounds = RECT {
        left: 0,
        top: 0,
        right: i32::try_from(width).unwrap_or(i32::MAX),
        bottom: i32::try_from(height).unwrap_or(i32::MAX),
    };
    host.set_window(generation, window.0.0 as isize, bounds)?;
    host.do_preview(generation)?;
    let mode = host.initialization_mode;
    host.unload(generation)?;
    Ok(mode)
}

/// Keeps one native Preview Handler and its cross-process child HWND alive until explicit unload.
/// The worker owns all COM interfaces and the child window; the app supplies only a validated
/// parent HWND and generation-bound physical-pixel geometry.
pub struct AttachedPreviewSession {
    host: PreviewHandlerHost,
    window: PreviewHostWindow,
    generation: u64,
    dpi: u32,
}

impl AttachedPreviewSession {
    /// Initializes, attaches, and renders one handler inside the caller's preview boundary.
    ///
    /// # Errors
    ///
    /// Returns [`ExplorerError`] when the generation or boundary is invalid, the host child
    /// window cannot be created, or handler initialization, attachment, or rendering fails.
    pub fn attach(
        location: &LocationDescriptor,
        generation: u64,
        parent: isize,
        bounds: explorer_model::PreviewHostBounds,
    ) -> Result<Self, ExplorerError> {
        if generation != bounds.generation.value() || !bounds.is_valid() || parent == 0 {
            return Err(preview_error(
                "attach preview session",
                "invalid or stale preview host identity",
            ));
        }
        let parent = HWND(parent as *mut _);
        let window = PreviewHostWindow::create_attached(
            parent,
            bounds.left_physical,
            bounds.top_physical,
            bounds.width_physical,
            bounds.height_physical,
        )?;
        let host = PreviewHandlerHost::initialize(location, generation)?;
        let local = RECT {
            left: 0,
            top: 0,
            right: i32::try_from(bounds.width_physical).unwrap_or(i32::MAX),
            bottom: i32::try_from(bounds.height_physical).unwrap_or(i32::MAX),
        };
        host.set_window(generation, window.0.0 as isize, local)?;
        host.do_preview(generation)?;
        Ok(Self {
            host,
            window,
            generation,
            dpi: bounds.dpi,
        })
    }

    /// Moves and resizes the attached handler using generation-bound physical-pixel bounds.
    ///
    /// # Errors
    ///
    /// Returns [`ExplorerError`] for stale or invalid bounds, a native window failure, or a
    /// handler `SetRect` failure.
    pub fn resize(
        &mut self,
        bounds: explorer_model::PreviewHostBounds,
    ) -> Result<(), ExplorerError> {
        if bounds.generation.value() != self.generation || !bounds.is_valid() {
            return Err(preview_error(
                "resize preview session",
                "stale preview bounds",
            ));
        }
        self.window.resize(
            bounds.left_physical,
            bounds.top_physical,
            bounds.width_physical,
            bounds.height_physical,
        )?;
        let local = RECT {
            left: 0,
            top: 0,
            right: i32::try_from(bounds.width_physical).unwrap_or(i32::MAX),
            bottom: i32::try_from(bounds.height_physical).unwrap_or(i32::MAX),
        };
        self.host.set_rect(self.generation, local)?;
        self.dpi = bounds.dpi;
        Ok(())
    }

    /// Transfers keyboard focus into the active Preview Handler.
    ///
    /// # Errors
    ///
    /// Returns [`ExplorerError`] when the handler rejects the focus request.
    pub fn set_focus(&self) -> Result<(), ExplorerError> {
        self.host.set_focus(self.generation)
    }

    /// Returns the native window currently focused inside the handler boundary.
    ///
    /// # Errors
    ///
    /// Returns [`ExplorerError`] when the handler cannot report its focused window.
    pub fn query_focus(&self) -> Result<isize, ExplorerError> {
        self.host.query_focus(self.generation)
    }

    /// Offers one non-reserved keyboard message to the Preview Handler.
    ///
    /// # Errors
    ///
    /// Returns [`ExplorerError`] when the handler does not process the accelerator.
    pub fn translate_accelerator(&self, message: &MSG) -> Result<(), ExplorerError> {
        self.host.translate_accelerator(self.generation, message)
    }

    /// Idempotently unloads the active Preview Handler.
    ///
    /// # Errors
    ///
    /// Returns [`ExplorerError`] when the handler's unload call fails.
    pub fn unload(&mut self) -> Result<(), ExplorerError> {
        self.host.unload(self.generation)
    }

    pub const fn generation(&self) -> u64 {
        self.generation
    }

    pub const fn dpi(&self) -> u32 {
        self.dpi
    }

    pub const fn initialization_mode(&self) -> PreviewInitializationMode {
        self.host.initialization_mode
    }
}

fn initialize_handler(
    handler: &IPreviewHandler, // architecture-check: allow worker-only adapter
    location: &LocationDescriptor,
) -> Result<PreviewInitializationMode, ExplorerError> {
    if let Some(path) = location.path() {
        let path_wide = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        if let Ok(initializer) = handler.cast::<PreviewFileInitializer>() {
            // SAFETY: path is NUL terminated, read-only, and valid for the synchronous call.
            unsafe { initializer.Initialize(PCWSTR(path_wide.as_ptr()), STGM_READ.0) }
                .map_err(|error| preview_native_error("initialize preview with file", &error))?;
            return Ok(PreviewInitializationMode::File);
        }
        if let Ok(initializer) = handler.cast::<PreviewStreamInitializer>() {
            // SAFETY: creates a read-only stream without modifying the selected file.
            let stream = unsafe {
                SHCreateStreamOnFileEx(PCWSTR(path_wide.as_ptr()), STGM_READ.0, 0, false, None)
            }
            .map_err(|error| preview_native_error("open preview stream", &error))?;
            // SAFETY: stream and initializer are live on the same disposable STA.
            unsafe { initializer.Initialize(&stream, STGM_READ.0) }
                .map_err(|error| preview_native_error("initialize preview with stream", &error))?;
            return Ok(PreviewInitializationMode::Stream);
        }
    }
    if let Ok(initializer) = handler.cast::<PreviewItemInitializer>() {
        let item = crate::navigation::shell_item(location)?;
        // SAFETY: the Shell item and handler are owned by the same disposable STA.
        unsafe { initializer.Initialize(&item, STGM_READ.0) }
            .map_err(|error| preview_native_error("initialize preview with Shell item", &error))?;
        return Ok(PreviewInitializationMode::ShellItem);
    }
    Err(preview_error(
        "initialize preview handler",
        "handler exposes no supported public initialization interface",
    ))
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn preview_error(operation: &'static str, detail: impl Into<String>) -> ExplorerError {
    ExplorerError::new(
        ExplorerErrorKind::Extension,
        operation,
        true,
        "Preview is unavailable for this item. You can retry or use Properties.",
        detail,
    )
}

fn preview_native_error(operation: &'static str, error: &windows::core::Error) -> ExplorerError {
    preview_error(
        operation,
        format!(
            "HRESULT 0x{:08x}",
            u32::from_ne_bytes(error.code().0.to_ne_bytes())
        ),
    )
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::inline_always,
        clippy::ref_as_ptr,
        reason = "windows-rs implement generates the COM vtable glue exercised by these tests"
    )]

    use super::*;
    use std::sync::{Arc, Mutex};
    use windows::{
        Win32::{
            Foundation::E_FAIL,
            System::Com::IStream,
            UI::Shell::{
                IInitializeWithItem_Impl, IPreviewHandler_Impl, IShellItem,
                PropertiesSystem::{IInitializeWithFile_Impl, IInitializeWithStream_Impl},
            },
        },
        core::{Error as WinError, Ref, Result as WinResult, implement},
    };

    #[allow(
        clippy::inline_always,
        clippy::ref_as_ptr,
        reason = "windows-rs implement macro generates controlled Preview Handler COM glue"
    )]
    #[implement(IPreviewHandler)] // architecture-check: allow worker-only adapter
    struct ControlledPreviewHandler {
        events: Arc<Mutex<Vec<&'static str>>>,
        focus: HWND,
        fail_render: bool,
    }

    impl IPreviewHandler_Impl for ControlledPreviewHandler_Impl {
        fn SetWindow(&self, _hwnd: HWND, prc: *const RECT) -> WinResult<()> {
            if prc.is_null() {
                return Err(WinError::from(E_FAIL));
            }
            self.events.lock().expect("preview trace").push("window");
            Ok(())
        }

        fn SetRect(&self, prc: *const RECT) -> WinResult<()> {
            if prc.is_null() {
                return Err(WinError::from(E_FAIL));
            }
            self.events.lock().expect("preview trace").push("rect");
            Ok(())
        }

        fn DoPreview(&self) -> WinResult<()> {
            // architecture-check: allow worker-only adapter
            self.events.lock().expect("preview trace").push("render");
            if self.fail_render {
                Err(WinError::from(E_FAIL))
            } else {
                Ok(())
            }
        }

        fn Unload(&self) -> WinResult<()> {
            self.events.lock().expect("preview trace").push("unload");
            Ok(())
        }

        fn SetFocus(&self) -> WinResult<()> {
            self.events.lock().expect("preview trace").push("focus");
            Ok(())
        }

        fn QueryFocus(&self) -> WinResult<HWND> {
            self.events
                .lock()
                .expect("preview trace")
                .push("query-focus");
            Ok(self.focus)
        }

        fn TranslateAccelerator(&self, pmsg: *const MSG) -> WinResult<()> {
            if pmsg.is_null() {
                return Err(WinError::from(E_FAIL));
            }
            self.events
                .lock()
                .expect("preview trace")
                .push("accelerator");
            Ok(())
        }
    }

    macro_rules! controlled_preview_surface {
        ($implementation:ident) => {
            impl IPreviewHandler_Impl for $implementation {
                fn SetWindow(&self, _hwnd: HWND, _prc: *const RECT) -> WinResult<()> {
                    Ok(())
                }
                fn SetRect(&self, _prc: *const RECT) -> WinResult<()> {
                    Ok(())
                }
                fn DoPreview(&self) -> WinResult<()> {
                    Ok(())
                }
                fn Unload(&self) -> WinResult<()> {
                    Ok(())
                }
                fn SetFocus(&self) -> WinResult<()> {
                    Ok(())
                }
                fn QueryFocus(&self) -> WinResult<HWND> {
                    Ok(HWND::default())
                }
                fn TranslateAccelerator(&self, _pmsg: *const MSG) -> WinResult<()> {
                    Ok(())
                }
            }
        };
    }

    #[allow(clippy::inline_always, clippy::ref_as_ptr)]
    #[implement(IPreviewHandler, PreviewFileInitializer)]
    struct ControlledFileInitializer(Arc<Mutex<Vec<&'static str>>>);
    controlled_preview_surface!(ControlledFileInitializer_Impl);
    impl IInitializeWithFile_Impl for ControlledFileInitializer_Impl {
        fn Initialize(&self, _path: &PCWSTR, _mode: u32) -> WinResult<()> {
            self.0.lock().expect("file initialize trace").push("file");
            Ok(())
        }
    }

    #[allow(clippy::inline_always, clippy::ref_as_ptr)]
    #[implement(IPreviewHandler, PreviewStreamInitializer)]
    struct ControlledStreamInitializer(Arc<Mutex<Vec<&'static str>>>);
    controlled_preview_surface!(ControlledStreamInitializer_Impl);
    impl IInitializeWithStream_Impl for ControlledStreamInitializer_Impl {
        fn Initialize(&self, _stream: Ref<'_, IStream>, _mode: u32) -> WinResult<()> {
            self.0
                .lock()
                .expect("stream initialize trace")
                .push("stream");
            Ok(())
        }
    }

    #[allow(clippy::inline_always, clippy::ref_as_ptr)]
    #[implement(IPreviewHandler, PreviewItemInitializer)]
    struct ControlledItemInitializer(Arc<Mutex<Vec<&'static str>>>);
    controlled_preview_surface!(ControlledItemInitializer_Impl);
    impl IInitializeWithItem_Impl for ControlledItemInitializer_Impl {
        fn Initialize(&self, _item: Ref<'_, IShellItem>, _mode: u32) -> WinResult<()> {
            self.0.lock().expect("item initialize trace").push("item");
            Ok(())
        }
    }

    fn controlled_host(
        generation: u64,
        fail_render: bool,
    ) -> (PreviewHandlerHost, Arc<Mutex<Vec<&'static str>>>) {
        let events = Arc::new(Mutex::new(Vec::new()));
        let handler: IPreviewHandler = ControlledPreviewHandler {
            // architecture-check: allow worker-only adapter
            events: Arc::clone(&events),
            focus: HWND(0x1234_usize as *mut _),
            fail_render,
        }
        .into();
        (
            PreviewHandlerHost {
                handler,
                lookup: PreviewLookup {
                    clsid: [1; 16],
                    extension: ".controlled".to_owned(),
                    server_path: PathBuf::from("controlled-preview-handler.dll"),
                    server_machine: 0x8664,
                },
                initialization_mode: PreviewInitializationMode::File,
                generation,
                unloaded: false,
            },
            events,
        )
    }

    #[test]
    fn lookup_reports_registered_or_truthfully_unavailable() {
        let _apartment = crate::sta::ApartmentGuard::initialize().expect("STA");
        let location = LocationDescriptor::file_system(r"C:\preview-fixture.txt");
        match PreviewLookup::for_location(&location) {
            Ok(lookup) => {
                assert_eq!(lookup.extension, ".txt");
                assert_ne!(lookup.clsid, [0; 16]);
                assert!(lookup.server_path.is_file());
                assert_eq!(lookup.server_machine, 0x8664);
            }
            Err(error) => assert!(error.recoverable),
        }
    }

    #[test]
    fn extensionless_and_nonpath_lookup_fail_closed() {
        assert!(
            PreviewLookup::for_location(&LocationDescriptor::file_system(r"C:\folder")).is_err()
        );
        assert!(
            PreviewLookup::for_location(&LocationDescriptor::ParsingName(
                "shell:RecycleBinFolder".to_owned()
            ))
            .is_err()
        );
    }

    #[test]
    fn pe_bitness_validation_accepts_the_x64_worker_and_rejects_non_images() {
        assert_eq!(
            pe_machine(&std::env::current_exe().expect("test executable")).expect("PE machine"),
            0x8664
        );
        let fixture = tempfile::NamedTempFile::new().expect("malformed image fixture");
        std::fs::write(fixture.path(), b"not a PE image").expect("fixture bytes");
        assert!(pe_machine(fixture.path()).is_err());
    }

    #[test]
    fn controlled_handler_covers_resize_focus_accelerator_stale_and_idempotent_unload() {
        let (mut host, events) = controlled_host(9, false);
        let bounds = RECT {
            left: 0,
            top: 0,
            right: 640,
            bottom: 360,
        };
        host.set_window(9, 1, bounds).expect("set window");
        host.do_preview(9).expect("render");
        host.set_rect(9, bounds).expect("resize");
        host.set_focus(9).expect("focus");
        assert_eq!(host.query_focus(9).expect("query focus"), 0x1234);
        host.translate_accelerator(9, &MSG::default())
            .expect("accelerator");
        assert!(host.set_rect(8, bounds).is_err(), "stale generation");
        host.unload(9).expect("first unload");
        host.unload(9).expect("idempotent unload");
        drop(host);
        assert_eq!(
            *events.lock().expect("preview trace"),
            [
                "window",
                "render",
                "rect",
                "focus",
                "query-focus",
                "accelerator",
                "unload"
            ]
        );
    }

    #[test]
    fn controlled_handler_render_failure_remains_recoverable_and_unloads_on_drop() {
        let (host, events) = controlled_host(3, true);
        assert!(host.do_preview(3).is_err());
        drop(host);
        assert_eq!(*events.lock().expect("preview trace"), ["render", "unload"]);
    }

    #[test]
    fn controlled_initializers_negotiate_file_then_stream_then_shell_item() {
        let _apartment = crate::sta::ApartmentGuard::initialize().expect("STA");
        let fixture = tempfile::NamedTempFile::with_suffix(".txt").expect("preview input");
        std::fs::write(fixture.path(), b"controlled preview input").expect("preview bytes");
        let location = LocationDescriptor::file_system(fixture.path());

        let file_events = Arc::new(Mutex::new(Vec::new()));
        let file_handler: IPreviewHandler =
            ControlledFileInitializer(Arc::clone(&file_events)).into();
        assert_eq!(
            initialize_handler(&file_handler, &location).expect("file initialization"),
            PreviewInitializationMode::File
        );
        assert_eq!(*file_events.lock().expect("file trace"), ["file"]);

        let stream_events = Arc::new(Mutex::new(Vec::new()));
        let stream_handler: IPreviewHandler =
            ControlledStreamInitializer(Arc::clone(&stream_events)).into();
        assert_eq!(
            initialize_handler(&stream_handler, &location).expect("stream initialization"),
            PreviewInitializationMode::Stream
        );
        assert_eq!(*stream_events.lock().expect("stream trace"), ["stream"]);

        let item_events = Arc::new(Mutex::new(Vec::new()));
        let item_handler: IPreviewHandler =
            ControlledItemInitializer(Arc::clone(&item_events)).into();
        assert_eq!(
            initialize_handler(&item_handler, &location).expect("item initialization"),
            PreviewInitializationMode::ShellItem
        );
        assert_eq!(*item_events.lock().expect("item trace"), ["item"]);
    }

    #[test]
    fn attached_session_rejects_zero_stale_and_destroyed_parent_windows_before_activation() {
        let location = LocationDescriptor::file_system(r"C:\controlled-preview.txt");
        let valid = explorer_model::PreviewHostBounds {
            generation: explorer_model::Generation::new(4),
            left_physical: 0,
            top_physical: 0,
            width_physical: 640,
            height_physical: 360,
            dpi: 96,
        };
        assert!(AttachedPreviewSession::attach(&location, 4, 0, valid).is_err());
        assert!(AttachedPreviewSession::attach(&location, 3, 1, valid).is_err());
        assert!(AttachedPreviewSession::attach(&location, 4, 1, valid).is_err());
    }
}
