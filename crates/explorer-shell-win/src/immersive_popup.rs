//! Clean-room, application-owned presentation for an authoritative Shell `HMENU`.
//!
//! This module deliberately uses documented Win32/GDI APIs only. It never resolves or
//! declares Explorer's private immersive-menu helper ABI. The HMENU remains owned by the
//! caller and continues to define command identity, state, bitmaps and submenus.

#![allow(unsafe_code, reason = "documented Win32 popup and GDI boundary")]

use std::{ffi::c_void, mem::size_of, ptr::NonNull, sync::OnceLock};

use windows::{
    Win32::{
        Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
        Graphics::Gdi::{
            AC_SRC_ALPHA, AC_SRC_OVER, AlphaBlend, BITMAP, BLACK_BRUSH, BLENDFUNCTION, BeginPaint,
            CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, CreateCompatibleDC, CreateFontIndirectW,
            CreateFontW, CreateSolidBrush, DEFAULT_CHARSET, DEFAULT_GUI_FONT, DEFAULT_PITCH,
            DRAWSTATE_FLAGS, DSS_DISABLED, DSS_NORMAL, DST_BITMAP, DeleteDC, DeleteObject,
            DrawStateW, DrawTextW, EndPaint, FillRect, GetMonitorInfoW, GetObjectW, GetStockObject,
            HBRUSH, HDC, HGDIOBJ, HMONITOR, InvalidateRect, MONITOR_DEFAULTTONEAREST, MONITORINFO,
            MonitorFromPoint, OUT_DEFAULT_PRECIS, PAINTSTRUCT, SelectObject, SetBkMode,
            SetTextColor, TRANSPARENT,
        },
        System::{
            LibraryLoader::GetModuleHandleW,
            Threading::{AttachThreadInput, GetCurrentThreadId},
        },
        UI::{
            HiDpi::SystemParametersInfoForDpi,
            Input::KeyboardAndMouse::{ReleaseCapture, SetCapture, SetFocus},
            WindowsAndMessaging::{
                CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW,
                DestroyWindow, DispatchMessageW, GWLP_USERDATA, GetClientRect, GetCursorPos,
                GetMenuItemCount, GetMenuItemInfoW, GetMessageW, GetWindowRect,
                GetWindowThreadProcessId, LWA_ALPHA, MENUITEMINFOW, MFS_DISABLED, MFS_GRAYED,
                MFT_OWNERDRAW, MFT_SEPARATOR, MIIM_BITMAP, MIIM_FTYPE, MIIM_ID, MIIM_STATE,
                MIIM_STRING, MIIM_SUBMENU, MSG, NONCLIENTMETRICSW, RegisterClassW,
                SPI_GETNONCLIENTMETRICS, SW_SHOW, SendMessageW, SetForegroundWindow,
                SetLayeredWindowAttributes, SetWindowLongPtrW, ShowWindow, TranslateMessage,
                WM_ACTIVATEAPP, WM_CANCELMODE, WM_CHAR, WM_DESTROY, WM_ERASEBKGND, WM_GETDLGCODE,
                WM_INITMENUPOPUP, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
                WM_MOUSEWHEEL, WM_NCCREATE, WM_PAINT, WM_RBUTTONDOWN, WNDCLASSW, WS_BORDER,
                WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
            },
        },
    },
    core::{PCWSTR, PWSTR, w},
};

const VISUAL: explorer_model::ContextMenuVisualMetrics =
    explorer_model::WINDOWS_CONTEXT_MENU_VISUAL_METRICS;
const ROW_HEIGHT: i32 = VISUAL.row_height as i32;
const SEPARATOR_HEIGHT: i32 = VISUAL.separator_height as i32;
const MIN_WIDTH: i32 = VISUAL.minimum_width as i32;
const MAX_WIDTH: i32 = VISUAL.maximum_width as i32;
const ICON_SLOT: i32 = VISUAL.icon_gutter as i32;
const RIGHT_INSET: i32 = VISUAL.right_inset as i32;
const FONT_PX: i32 = VISUAL.font_size as i32;

const fn colorref(rgb: [u8; 3]) -> COLORREF {
    COLORREF((rgb[0] as u32) | ((rgb[1] as u32) << 8) | ((rgb[2] as u32) << 16))
}

const fn surface(dark: bool) -> COLORREF {
    colorref(if dark {
        explorer_model::WINDOWS_CONTEXT_MENU_DARK_PALETTE.surface
    } else {
        explorer_model::WINDOWS_CONTEXT_MENU_LIGHT_PALETTE.surface
    })
}

const fn hover(dark: bool) -> COLORREF {
    colorref(if dark {
        explorer_model::WINDOWS_CONTEXT_MENU_DARK_PALETTE.hover
    } else {
        explorer_model::WINDOWS_CONTEXT_MENU_LIGHT_PALETTE.hover
    })
}

const fn text_color(dark: bool) -> COLORREF {
    colorref(if dark {
        explorer_model::WINDOWS_CONTEXT_MENU_DARK_PALETTE.text
    } else {
        explorer_model::WINDOWS_CONTEXT_MENU_LIGHT_PALETTE.text
    })
}

const fn disabled_text(dark: bool) -> COLORREF {
    colorref(if dark {
        explorer_model::WINDOWS_CONTEXT_MENU_DARK_PALETTE.disabled_text
    } else {
        explorer_model::WINDOWS_CONTEXT_MENU_LIGHT_PALETTE.disabled_text
    })
}

const fn divider_color(dark: bool) -> COLORREF {
    colorref(if dark {
        explorer_model::WINDOWS_CONTEXT_MENU_DARK_PALETTE.divider
    } else {
        explorer_model::WINDOWS_CONTEXT_MENU_LIGHT_PALETTE.divider
    })
}

struct InputQueueAttachment {
    current: u32,
    owner: u32,
    attached: bool,
}

impl InputQueueAttachment {
    fn to_owner(owner: HWND) -> Self {
        let current = unsafe { GetCurrentThreadId() };
        let owner_thread = unsafe { GetWindowThreadProcessId(owner, None) };
        let attached = owner_thread != 0
            && owner_thread != current
            && unsafe { AttachThreadInput(current, owner_thread, true) }.as_bool();
        Self {
            current,
            owner: owner_thread,
            attached,
        }
    }
}

impl Drop for InputQueueAttachment {
    fn drop(&mut self) {
        if self.attached {
            let _ = unsafe { AttachThreadInput(self.current, self.owner, false) };
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PopupUnsupportedReason {
    InvalidMenu,
    EmptyMenu,
    EnumerationFailed,
    WindowClassFailed,
    WindowCreationFailed,
    MessageLoopFailed,
    CleanupFailed,
    UnsupportedOwnerDraw,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PopupTestFault {
    None,
    Apply,
    Message,
    Cleanup,
}

#[cfg(test)]
thread_local! {
    static POPUP_TEST_FAULT: std::cell::Cell<PopupTestFault> =
        const { std::cell::Cell::new(PopupTestFault::None) };
}

#[cfg(test)]
fn popup_test_fault_is(fault: PopupTestFault) -> bool {
    POPUP_TEST_FAULT.with(|current| current.get() == fault)
}

#[cfg(not(test))]
const fn popup_apply_fault() -> bool {
    false
}

#[cfg(test)]
fn popup_apply_fault() -> bool {
    popup_test_fault_is(PopupTestFault::Apply)
}

#[cfg(not(test))]
const fn popup_message_fault() -> bool {
    false
}

#[cfg(test)]
fn popup_message_fault() -> bool {
    popup_test_fault_is(PopupTestFault::Message)
}

#[cfg(not(test))]
const fn popup_cleanup_fault() -> bool {
    false
}

#[cfg(test)]
fn popup_cleanup_fault() -> bool {
    popup_test_fault_is(PopupTestFault::Cleanup)
}

#[derive(Debug)]
struct Row {
    id: u32,
    state: u32,
    kind: u32,
    submenu: windows::Win32::UI::WindowsAndMessaging::HMENU,
    bitmap: windows::Win32::Graphics::Gdi::HBITMAP,
    text: Vec<u16>,
    top: i32,
    height: i32,
}

impl Row {
    fn separator(&self) -> bool {
        self.kind & MFT_SEPARATOR.0 != 0
    }

    fn disabled(&self) -> bool {
        self.state & (MFS_DISABLED.0 | MFS_GRAYED.0) != 0
    }
}

struct PopupState {
    menu: windows::Win32::UI::WindowsAndMessaging::HMENU,
    rows: Vec<Row>,
    owner: HWND,
    dpi: u32,
    dark: bool,
    width: i32,
    height: i32,
    content_height: i32,
    scroll_y: i32,
    selected: Option<usize>,
    pressed: Option<usize>,
    result: i32,
    replacement_point: Option<POINT>,
    hwnd: HWND,
    font: HGDIOBJ,
    font_owned: bool,
    shadows: Vec<HWND>,
}

pub(crate) struct PopupPresentation {
    pub(crate) command: i32,
    pub(crate) replacement_point: Option<POINT>,
}

impl Drop for PopupState {
    fn drop(&mut self) {
        if !self.hwnd.is_invalid() {
            let _ = unsafe { DestroyWindow(self.hwnd) };
            self.hwnd = HWND::default();
        }
        if self.font_owned && !self.font.is_invalid() {
            let _ = unsafe { DeleteObject(self.font) };
        }
        for shadow in self.shadows.drain(..) {
            let _ = unsafe { DestroyWindow(shadow) };
        }
    }
}

pub(crate) fn present(
    menu: windows::Win32::UI::WindowsAndMessaging::HMENU,
    owner: HWND,
    point: POINT,
    dpi: u32,
    dark: bool,
) -> Result<PopupPresentation, PopupUnsupportedReason> {
    tracing::debug!(
        dpi,
        x = point.x,
        y = point.y,
        "opening application-owned popup"
    );
    let _input_queue = InputQueueAttachment::to_owner(owner);
    present_level(menu, owner, point, dpi.max(96), dark, 0)
}

fn present_level(
    menu: windows::Win32::UI::WindowsAndMessaging::HMENU,
    owner: HWND,
    point: POINT,
    dpi: u32,
    dark: bool,
    depth: usize,
) -> Result<PopupPresentation, PopupUnsupportedReason> {
    if menu.is_invalid() || owner.is_invalid() || depth > 16 {
        return Err(PopupUnsupportedReason::InvalidMenu);
    }
    let (rows, width, content_height) = materialize(menu, dpi)?;
    let work = monitor_work_area(point);
    let height = work.map_or(content_height, |work| {
        let shadow_margin = scale(8, dpi);
        content_height.min(
            (work.bottom - work.top)
                .saturating_sub(shadow_margin)
                .max(scale(ROW_HEIGHT, dpi)),
        )
    });
    if popup_apply_fault() {
        return Err(PopupUnsupportedReason::WindowCreationFailed);
    }
    let class = ensure_class()?;
    let font = create_menu_font(dpi);
    let (font, font_owned) = if font.is_invalid() {
        (unsafe { GetStockObject(DEFAULT_GUI_FONT) }, false)
    } else {
        (HGDIOBJ(font.0), true)
    };
    let mut state = Box::new(PopupState {
        menu,
        rows,
        owner,
        dpi,
        dark,
        width,
        height,
        content_height,
        scroll_y: 0,
        selected: None,
        pressed: None,
        result: 0,
        replacement_point: None,
        hwnd: HWND::default(),
        font,
        font_owned,
        shadows: Vec::new(),
    });
    let origin = work.map_or_else(
        || clamp_to_monitor(point, width, height),
        |work| clamp_popup_to_work_area(point, width, height, work, dpi),
    );
    let instance = unsafe { GetModuleHandleW(None) }
        .map_err(|_| PopupUnsupportedReason::WindowCreationFailed)?;
    state.shadows = create_shadows(owner, origin, width, height, HINSTANCE(instance.0));
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            class,
            w!(""),
            WS_POPUP | WS_BORDER,
            origin.x,
            origin.y,
            width,
            height,
            Some(owner),
            None,
            Some(HINSTANCE(instance.0)),
            Some((&raw mut *state).cast::<c_void>()),
        )
    }
    .map_err(|_| PopupUnsupportedReason::WindowCreationFailed)?;
    state.hwnd = hwnd;
    let _ = unsafe { SetForegroundWindow(hwnd) };
    unsafe {
        for shadow in &state.shadows {
            let _ = ShowWindow(*shadow, SW_SHOW);
        }
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetFocus(Some(hwnd));
        SetCapture(hwnd);
    }
    tracing::debug!(
        rows = state.rows.len(),
        width,
        height,
        "application-owned popup is visible"
    );
    let mut message = MSG::default();
    let mut message_failed = false;
    if popup_message_fault() || popup_cleanup_fault() {
        state.result = -1;
        message_failed = popup_message_fault();
    }
    while state.result == 0 {
        let status = unsafe { GetMessageW(&raw mut message, None, 0, 0) };
        if status.0 == -1 {
            state.result = -1;
            message_failed = true;
            break;
        }
        if status.0 == 0 {
            state.result = -1;
            break;
        }
        // Replacement right-clicks are observed by the Shell adapter's low-level hook. The
        // adapter posts WM_CANCELMODE to the disposable Shell owner (the same contract used by
        // TrackPopupMenuEx), not to this application-owned presentation HWND. Consume that
        // thread message here so the old popup is fully torn down before the captured gesture is
        // replayed against the real application window.
        if message.message == WM_CANCELMODE
            || (message.message == WM_ACTIVATEAPP && message.wParam.0 == 0)
        {
            state.result = -1;
            continue;
        }
        if message.hwnd.is_invalid() && message.message == WM_KEYDOWN {
            unsafe {
                SendMessageW(hwnd, WM_KEYDOWN, Some(message.wParam), Some(message.lParam));
            }
        } else {
            unsafe {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        if state.result < -1 {
            let index = usize::try_from((-state.result) - 2).unwrap_or(usize::MAX);
            state.result = 0;
            if let Some(row) = state.rows.get(index)
                && !row.submenu.is_invalid()
            {
                initialize_submenu(state.owner, row.submenu, index);
                let child_point = POINT {
                    x: origin.x.saturating_add(width - 2),
                    y: origin
                        .y
                        .saturating_add(row.top.saturating_sub(state.scroll_y)),
                };
                if let Ok(selected) =
                    present_level(row.submenu, owner, child_point, dpi, dark, depth + 1)
                    && (selected.command > 0 || selected.replacement_point.is_some())
                {
                    state.result = selected.command;
                    state.replacement_point = selected.replacement_point;
                }
                if state.result == 0 {
                    unsafe {
                        SetCapture(hwnd);
                        let _ = SetFocus(Some(hwnd));
                    }
                }
            }
        }
    }
    let destroyed = unsafe {
        let _ = ReleaseCapture();
        DestroyWindow(hwnd).is_ok()
    };
    if destroyed {
        state.hwnd = HWND::default();
    }
    if message_failed {
        return Err(PopupUnsupportedReason::MessageLoopFailed);
    }
    if !destroyed || popup_cleanup_fault() {
        return Err(PopupUnsupportedReason::CleanupFailed);
    }
    Ok(PopupPresentation {
        command: state.result.max(0),
        replacement_point: state.replacement_point,
    })
}

fn initialize_submenu(
    owner: HWND,
    submenu: windows::Win32::UI::WindowsAndMessaging::HMENU,
    index: usize,
) {
    unsafe {
        SendMessageW(
            owner,
            WM_INITMENUPOPUP,
            Some(WPARAM(submenu.0 as usize)),
            Some(LPARAM(index as isize)),
        );
    }
}

fn create_menu_font(dpi: u32) -> windows::Win32::Graphics::Gdi::HFONT {
    let mut metrics = NONCLIENTMETRICSW {
        cbSize: size_of::<NONCLIENTMETRICSW>() as u32,
        ..Default::default()
    };
    let system_font = unsafe {
        SystemParametersInfoForDpi(
            SPI_GETNONCLIENTMETRICS.0,
            metrics.cbSize,
            Some((&mut metrics as *mut NONCLIENTMETRICSW).cast()),
            0,
            dpi,
        )
    }
    .map(|_| unsafe { CreateFontIndirectW(&metrics.lfMenuFont) })
    .unwrap_or_default();
    if !system_font.is_invalid() {
        return system_font;
    }

    unsafe {
        CreateFontW(
            -scale(FONT_PX, dpi),
            0,
            0,
            0,
            400,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            u32::from(DEFAULT_PITCH.0),
            w!("Segoe UI"),
        )
    }
}

fn materialize(
    menu: windows::Win32::UI::WindowsAndMessaging::HMENU,
    dpi: u32,
) -> Result<(Vec<Row>, i32, i32), PopupUnsupportedReason> {
    let count = unsafe { GetMenuItemCount(Some(menu)) };
    if count < 0 {
        return Err(PopupUnsupportedReason::EnumerationFailed);
    }
    if count == 0 {
        return Err(PopupUnsupportedReason::EmptyMenu);
    }
    let mut rows = Vec::with_capacity(count as usize);
    let mut top = scale(3, dpi);
    let mut longest = 0usize;
    for position in 0..u32::try_from(count).unwrap_or(0) {
        let mask = MIIM_FTYPE | MIIM_STATE | MIIM_ID | MIIM_SUBMENU | MIIM_BITMAP | MIIM_STRING;
        let mut info = MENUITEMINFOW {
            cbSize: u32::try_from(size_of::<MENUITEMINFOW>()).unwrap_or(u32::MAX),
            fMask: mask,
            ..Default::default()
        };
        unsafe { GetMenuItemInfoW(menu, position, true, &raw mut info) }
            .map_err(|_| PopupUnsupportedReason::EnumerationFailed)?;
        if info.fType.0 & MFT_OWNERDRAW.0 != 0 {
            return Err(PopupUnsupportedReason::UnsupportedOwnerDraw);
        }
        let mut text = vec![0u16; usize::try_from(info.cch).unwrap_or(0).saturating_add(2)];
        info.dwTypeData = PWSTR(text.as_mut_ptr());
        info.cch = u32::try_from(text.len()).unwrap_or(u32::MAX);
        unsafe { GetMenuItemInfoW(menu, position, true, &raw mut info) }
            .map_err(|_| PopupUnsupportedReason::EnumerationFailed)?;
        text.truncate(usize::try_from(info.cch).unwrap_or(0));
        longest = longest.max(text.len());
        let height = scale(
            if info.fType.0 & MFT_SEPARATOR.0 != 0 {
                SEPARATOR_HEIGHT
            } else {
                ROW_HEIGHT
            },
            dpi,
        );
        rows.push(Row {
            id: info.wID,
            state: info.fState.0,
            kind: info.fType.0,
            submenu: info.hSubMenu,
            bitmap: info.hbmpItem,
            text,
            top,
            height,
        });
        top = top.saturating_add(height);
    }
    let estimated_text = i32::try_from(longest)
        .unwrap_or(i32::MAX)
        .saturating_mul(scale(7, dpi));
    let width = scale(MIN_WIDTH, dpi)
        .max(scale(ICON_SLOT + RIGHT_INSET, dpi).saturating_add(estimated_text))
        .min(scale(MAX_WIDTH, dpi));
    Ok((rows, width, top.saturating_add(scale(3, dpi))))
}

fn ensure_class() -> Result<PCWSTR, PopupUnsupportedReason> {
    static CLASS: OnceLock<Result<(), ()>> = OnceLock::new();
    let result = CLASS.get_or_init(|| {
        let instance = unsafe { GetModuleHandleW(None) }.map_err(|_| ())?;
        let class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(window_proc),
            hInstance: HINSTANCE(instance.0),
            hCursor: unsafe {
                windows::Win32::UI::WindowsAndMessaging::LoadCursorW(
                    None,
                    windows::Win32::UI::WindowsAndMessaging::IDC_ARROW,
                )
            }
            .unwrap_or_default(),
            lpszClassName: w!("SuperExplorer.ImmersivePopup.v1"),
            ..Default::default()
        };
        let atom = unsafe { RegisterClassW(&raw const class) };
        let shadow = WNDCLASSW {
            lpfnWndProc: Some(shadow_proc),
            hInstance: HINSTANCE(instance.0),
            hbrBackground: HBRUSH(unsafe { GetStockObject(BLACK_BRUSH) }.0),
            lpszClassName: w!("SuperExplorer.ImmersivePopupShadow.v1"),
            ..Default::default()
        };
        let shadow_atom = unsafe { RegisterClassW(&raw const shadow) };
        if atom == 0 || shadow_atom == 0 {
            Err(())
        } else {
            Ok(())
        }
    });
    result
        .as_ref()
        .map_err(|_| PopupUnsupportedReason::WindowClassFailed)?;
    Ok(w!("SuperExplorer.ImmersivePopup.v1"))
}

fn create_shadows(
    owner: HWND,
    origin: POINT,
    width: i32,
    height: i32,
    instance: HINSTANCE,
) -> Vec<HWND> {
    let mut windows = Vec::new();
    let style = WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED | WS_EX_TOPMOST;
    for (index, alpha) in [30_u8, 22, 15, 8].into_iter().enumerate() {
        let offset = i32::try_from(index).unwrap_or(0) * 2;
        if let Ok(window) = unsafe {
            CreateWindowExW(
                style,
                w!("SuperExplorer.ImmersivePopupShadow.v1"),
                w!(""),
                WS_POPUP,
                origin.x + 2,
                origin.y + height + offset,
                width + 5,
                2,
                Some(owner),
                None,
                Some(instance),
                None,
            )
        } {
            let _ = unsafe { SetLayeredWindowAttributes(window, COLORREF(0), alpha, LWA_ALPHA) };
            windows.push(window);
        }
    }
    for (index, alpha) in [26_u8, 17, 9].into_iter().enumerate() {
        let offset = i32::try_from(index).unwrap_or(0) * 2;
        if let Ok(window) = unsafe {
            CreateWindowExW(
                style,
                w!("SuperExplorer.ImmersivePopupShadow.v1"),
                w!(""),
                WS_POPUP,
                origin.x + width + offset,
                origin.y + 5,
                2,
                (height - 8).max(1),
                Some(owner),
                None,
                Some(instance),
                None,
            )
        } {
            let _ = unsafe { SetLayeredWindowAttributes(window, COLORREF(0), alpha, LWA_ALPHA) };
            windows.push(window);
        }
    }
    windows
}

unsafe extern "system" fn shadow_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        let create = unsafe { &*(lparam.0 as *const CREATESTRUCTW) };
        unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize) };
    }
    let pointer =
        unsafe { windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(hwnd, GWLP_USERDATA) };
    let Some(mut state) = NonNull::new(pointer as *mut PopupState) else {
        return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    };
    let state = unsafe { state.as_mut() };
    match message {
        0x01E1 => LRESULT(state.menu.0 as isize),
        0x0451 => {
            ensure_row_visible(state, wparam.0);
            let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
            state.rows.get(wparam.0).map_or(LRESULT(-1), |row| {
                let visible_top = row.top.saturating_sub(state.scroll_y);
                LRESULT(((row.height as isize & 0xffff) << 16) | (visible_top as isize & 0xffff))
            })
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            paint(hwnd, state);
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let y = i32::from(i16::from_le_bytes(((lparam.0 >> 16) as u16).to_le_bytes()));
            let next = hit_test(&state.rows, y.saturating_add(state.scroll_y))
                .filter(|index| !state.rows[*index].separator());
            if next != state.selected {
                state.selected = next;
                let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let x = i32::from(i16::from_le_bytes((lparam.0 as u16).to_le_bytes()));
            let y = i32::from(i16::from_le_bytes(((lparam.0 >> 16) as u16).to_le_bytes()));
            if x < 0 || y < 0 || x >= state.width || y >= state.height {
                state.result = -1;
            } else if let Some(index) = hit_test(&state.rows, y.saturating_add(state.scroll_y)) {
                if !state.rows[index].separator() && !state.rows[index].disabled() {
                    state.selected = Some(index);
                    state.pressed = Some(index);
                    let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let y = i32::from(i16::from_le_bytes(((lparam.0 >> 16) as u16).to_le_bytes()));
            let released = hit_test(&state.rows, y.saturating_add(state.scroll_y));
            if let Some(index) = state.pressed.take()
                && released == Some(index)
            {
                activate(state, index);
            }
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            let delta = i32::from(i16::from_le_bytes(((wparam.0 >> 16) as u16).to_le_bytes()));
            let rows = -(delta / 120);
            scroll_by(state, rows.saturating_mul(scale(ROW_HEIGHT * 3, state.dpi)));
            let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
            LRESULT(0)
        }
        WM_RBUTTONDOWN => {
            // LPARAM coordinates from a captured message are unreliable after attaching input
            // queues across the broker/app process boundary. Compare physical cursor and popup
            // rectangles instead—the same coordinate space later consumed by SendInput.
            let mut point = POINT::default();
            let mut popup_rect = RECT::default();
            if unsafe { GetCursorPos(&raw mut point) }.is_ok()
                && unsafe { GetWindowRect(hwnd, &raw mut popup_rect) }.is_ok()
                && (point.x < popup_rect.left
                    || point.y < popup_rect.top
                    || point.x >= popup_rect.right
                    || point.y >= popup_rect.bottom)
            {
                let _ = unsafe { ReleaseCapture() };
                state.replacement_point = Some(point);
            }
            state.result = -1;
            LRESULT(0)
        }
        WM_CANCELMODE => {
            // The Shell adapter posts WM_CANCELMODE only after its right-click replacement hook
            // has validated a complete gesture against the originating app. Retain the physical
            // point here too; this makes the application-owned path independent of which thread
            // Windows chooses for the low-level hook callback's thread-local capture state.
            let mut point = POINT::default();
            if unsafe { GetCursorPos(&raw mut point) }.is_ok() {
                let _ = unsafe { ReleaseCapture() };
                state.replacement_point = Some(point);
            }
            state.result = -1;
            LRESULT(0)
        }
        WM_ACTIVATEAPP if wparam.0 == 0 => {
            // Match native popup lifetime: switching to another application dismisses this menu
            // without manufacturing a replacement gesture or invoking the selected row.
            let _ = unsafe { ReleaseCapture() };
            state.result = -1;
            LRESULT(0)
        }
        WM_CHAR => {
            if let Some(index) = mnemonic_match(&state.rows, char::from_u32(wparam.0 as u32)) {
                activate(state, index);
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            match wparam.0 as u32 {
                0x1B => state.result = -1,
                0x26 => {
                    move_selection(state, -1);
                    ensure_selection_visible(state);
                }
                0x28 => {
                    move_selection(state, 1);
                    ensure_selection_visible(state);
                }
                0x27 | 0x0D => {
                    if let Some(index) = state.selected {
                        activate(state, index);
                    }
                }
                _ => {}
            }
            let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
            LRESULT(0)
        }
        WM_GETDLGCODE => LRESULT(4),
        WM_DESTROY => LRESULT(0),
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

#[cfg(test)]
fn dismissal_message(message: u32) -> bool {
    matches!(message, WM_ACTIVATEAPP | WM_CANCELMODE | WM_RBUTTONDOWN)
}

fn activate(state: &mut PopupState, index: usize) {
    let Some(row) = state.rows.get(index) else {
        return;
    };
    if row.disabled() || row.separator() {
        return;
    }
    state.selected = Some(index);
    state.result = if row.submenu.is_invalid() {
        i32::try_from(row.id).unwrap_or(0)
    } else {
        -(i32::try_from(index).unwrap_or(i32::MAX).saturating_add(2))
    };
}

fn move_selection(state: &mut PopupState, delta: i32) {
    if state.rows.is_empty() {
        return;
    }
    let mut index = state
        .selected
        .map_or(if delta > 0 { -1 } else { 0 }, |value| value as i32);
    for _ in 0..state.rows.len() {
        index = (index + delta).rem_euclid(state.rows.len() as i32);
        let row = &state.rows[index as usize];
        if !row.separator() && !row.disabled() {
            state.selected = Some(index as usize);
            return;
        }
    }
}

fn hit_test(rows: &[Row], y: i32) -> Option<usize> {
    rows.iter()
        .position(|row| y >= row.top && y < row.top.saturating_add(row.height))
}

fn scroll_by(state: &mut PopupState, delta: i32) {
    let maximum = state.content_height.saturating_sub(state.height).max(0);
    state.scroll_y = state.scroll_y.saturating_add(delta).clamp(0, maximum);
}

fn ensure_selection_visible(state: &mut PopupState) {
    let Some(index) = state.selected else {
        return;
    };
    ensure_row_visible(state, index);
}

fn ensure_row_visible(state: &mut PopupState, index: usize) {
    let Some(row) = state.rows.get(index) else {
        return;
    };
    let padding = scale(3, state.dpi);
    if row.top < state.scroll_y.saturating_add(padding) {
        state.scroll_y = row.top.saturating_sub(padding);
    } else if row.top.saturating_add(row.height)
        > state
            .scroll_y
            .saturating_add(state.height)
            .saturating_sub(padding)
    {
        state.scroll_y = row
            .top
            .saturating_add(row.height)
            .saturating_add(padding)
            .saturating_sub(state.height);
    }
    let maximum = state.content_height.saturating_sub(state.height).max(0);
    state.scroll_y = state.scroll_y.clamp(0, maximum);
}

fn mnemonic_match(rows: &[Row], pressed: Option<char>) -> Option<usize> {
    let pressed = pressed?.to_lowercase().next()?;
    let mut matches = rows.iter().enumerate().filter_map(|(index, row)| {
        if row.separator() || row.disabled() {
            return None;
        }
        let label = String::from_utf16_lossy(&row.text);
        let mut characters = label.chars();
        while let Some(character) = characters.next() {
            if character == '&' {
                let mnemonic = characters.next()?;
                if mnemonic != '&' && mnemonic.to_lowercase().next() == Some(pressed) {
                    return Some(index);
                }
            }
        }
        None
    });
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
}

fn paint(hwnd: HWND, state: &PopupState) {
    let mut paint = PAINTSTRUCT::default();
    let dc = unsafe { BeginPaint(hwnd, &raw mut paint) };
    let mut client = RECT::default();
    let _ = unsafe { GetClientRect(hwnd, &raw mut client) };
    fill(dc, &client, surface(state.dark));
    let old_font = unsafe { SelectObject(dc, state.font) };
    let _ = unsafe { SetBkMode(dc, TRANSPARENT) };
    for (index, row) in state.rows.iter().enumerate() {
        let visible_top = row.top.saturating_sub(state.scroll_y);
        let rect = RECT {
            left: 0,
            top: visible_top,
            right: state.width,
            bottom: visible_top.saturating_add(row.height),
        };
        if rect.bottom <= 0 || rect.top >= state.height {
            continue;
        }
        if row.separator() {
            let divider = RECT {
                left: scale(ICON_SLOT, state.dpi),
                top: visible_top + row.height / 2,
                right: state.width - scale(i32::from(VISUAL.divider_right_inset), state.dpi),
                bottom: visible_top + row.height / 2 + 1,
            };
            fill(dc, &divider, divider_color(state.dark));
            continue;
        }
        if state.selected == Some(index) && !row.disabled() {
            fill(dc, &rect, hover(state.dark));
        }
        if !row.bitmap.is_invalid() && (row.bitmap.0 as usize) > 11 {
            let icon = scale(i32::from(VISUAL.icon_size), state.dpi);
            draw_menu_bitmap(
                dc,
                row.bitmap,
                scale(i32::from(VISUAL.icon_left), state.dpi),
                visible_top + (row.height - icon) / 2,
                icon,
                row.disabled(),
            );
        }
        let _ = unsafe {
            SetTextColor(
                dc,
                if row.disabled() {
                    disabled_text(state.dark)
                } else {
                    text_color(state.dark)
                },
            )
        };
        let mut text_rect = RECT {
            left: scale(ICON_SLOT, state.dpi),
            top: visible_top,
            right: state.width - scale(RIGHT_INSET, state.dpi),
            bottom: visible_top + row.height,
        };
        let mut text = row.text.clone();
        let _ = unsafe {
            DrawTextW(
                dc,
                &mut text,
                &raw mut text_rect,
                windows::Win32::Graphics::Gdi::DRAW_TEXT_FORMAT(0x0000 | 0x0004 | 0x0020 | 0x8000),
            )
        };
        if !row.submenu.is_invalid() {
            let x = state.width - scale(17, state.dpi);
            let y = visible_top + row.height / 2;
            let points = [
                POINT { x: x - 3, y: y - 5 },
                POINT { x: x + 2, y },
                POINT { x: x - 3, y: y + 5 },
            ];
            unsafe {
                let _ = windows::Win32::Graphics::Gdi::Polyline(dc, &points);
            }
        }
    }
    unsafe {
        let _ = SelectObject(dc, old_font);
        let _ = EndPaint(hwnd, &paint);
    }
}

fn draw_menu_bitmap(
    destination: HDC,
    bitmap: windows::Win32::Graphics::Gdi::HBITMAP,
    x: i32,
    y: i32,
    size: i32,
    disabled: bool,
) {
    let mut info = BITMAP::default();
    let bitmap_struct_size = i32::try_from(size_of::<BITMAP>()).expect("BITMAP size fits Win32");
    let described = unsafe {
        GetObjectW(
            HGDIOBJ(bitmap.0),
            bitmap_struct_size,
            Some((&raw mut info).cast()),
        )
    } == bitmap_struct_size;

    if described && info.bmBitsPixel == 32 && info.bmWidth > 0 && info.bmHeight != 0 {
        let source = unsafe { CreateCompatibleDC(Some(destination)) };
        if !source.is_invalid() {
            let previous = unsafe { SelectObject(source, HGDIOBJ(bitmap.0)) };
            let blended = unsafe {
                AlphaBlend(
                    destination,
                    x,
                    y,
                    size,
                    size,
                    source,
                    0,
                    0,
                    info.bmWidth,
                    info.bmHeight.abs(),
                    BLENDFUNCTION {
                        BlendOp: u8::try_from(AC_SRC_OVER).expect("AC_SRC_OVER fits BLENDFUNCTION"),
                        BlendFlags: 0,
                        SourceConstantAlpha: if disabled { 110 } else { 255 },
                        AlphaFormat: u8::try_from(AC_SRC_ALPHA)
                            .expect("AC_SRC_ALPHA fits BLENDFUNCTION"),
                    },
                )
            }
            .as_bool();
            let _ = unsafe { SelectObject(source, previous) };
            let _ = unsafe { DeleteDC(source) };
            if blended {
                return;
            }
        }
    }

    let _ = unsafe {
        DrawStateW(
            destination,
            None,
            None,
            LPARAM(bitmap.0 as isize),
            WPARAM(0),
            x,
            y,
            size,
            size,
            DRAWSTATE_FLAGS(
                DST_BITMAP.0
                    | if disabled {
                        DSS_DISABLED.0
                    } else {
                        DSS_NORMAL.0
                    },
            ),
        )
    };
}

fn fill(dc: HDC, rect: &RECT, color: COLORREF) {
    let brush = unsafe { CreateSolidBrush(color) };
    let _ = unsafe { FillRect(dc, rect, HBRUSH(brush.0)) };
    let _ = unsafe { DeleteObject(HGDIOBJ(brush.0)) };
}

fn scale(value: i32, dpi: u32) -> i32 {
    value
        .saturating_mul(i32::try_from(dpi).unwrap_or(96))
        .saturating_add(48)
        / 96
}

fn clamp_to_monitor(point: POINT, width: i32, height: i32) -> POINT {
    monitor_work_area(point).map_or(point, |work| clamp_to_work_area(point, width, height, work))
}

fn monitor_work_area(point: POINT) -> Option<RECT> {
    let monitor: HMONITOR = unsafe { MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST) };
    let mut info = MONITORINFO {
        cbSize: u32::try_from(size_of::<MONITORINFO>()).unwrap_or(u32::MAX),
        ..Default::default()
    };
    if unsafe { GetMonitorInfoW(monitor, &raw mut info) }.as_bool() {
        Some(info.rcWork)
    } else {
        None
    }
}

fn clamp_to_work_area(point: POINT, width: i32, height: i32, work: RECT) -> POINT {
    POINT {
        x: point.x.min(work.right - width).max(work.left),
        y: point.y.min(work.bottom - height).max(work.top),
    }
}

fn clamp_popup_to_work_area(point: POINT, width: i32, height: i32, work: RECT, dpi: u32) -> POINT {
    let right_shadow = scale(i32::from(VISUAL.right_shadow_extent), dpi);
    let bottom_shadow = scale(i32::from(VISUAL.bottom_shadow_extent), dpi);
    POINT {
        x: point
            .x
            .min(
                work.right
                    .saturating_sub(width)
                    .saturating_sub(right_shadow),
            )
            .max(work.left),
        y: point
            .y
            .min(
                work.bottom
                    .saturating_sub(height)
                    .saturating_sub(bottom_shadow),
            )
            .max(work.top),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use windows::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CreatePopupMenu, DestroyMenu, MF_CHECKED, MF_DISABLED, MF_OWNERDRAW, MF_POPUP,
        MF_SEPARATOR, MF_STRING, PostThreadMessageW, SetMenuItemInfoW, WM_KEYUP,
    };

    static DYNAMIC_INIT_COUNT: AtomicUsize = AtomicUsize::new(0);

    unsafe extern "system" fn dynamic_owner_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if message == WM_INITMENUPOPUP {
            DYNAMIC_INIT_COUNT.fetch_add(1, Ordering::SeqCst);
            let submenu = windows::Win32::UI::WindowsAndMessaging::HMENU(wparam.0 as *mut c_void);
            let _ = unsafe { AppendMenuW(submenu, MF_STRING, 313, w!("Dynamic child")) };
            return LRESULT(0);
        }
        unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
    }

    #[test]
    fn materialization_preserves_ids_states_and_geometry() {
        let menu = unsafe { CreatePopupMenu() }.expect("popup");
        unsafe {
            AppendMenuW(menu, MF_STRING, 7, w!("Open"))
                .and_then(|()| AppendMenuW(menu, MF_SEPARATOR, 0, None))
                .and_then(|()| AppendMenuW(menu, MF_STRING | MF_DISABLED, 9, w!("Disabled")))
        }
        .expect("fixture rows");
        let (rows, width, height) = materialize(menu, 96).expect("materialize");
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].id, 7);
        assert!(rows[1].separator());
        assert!(rows[2].disabled());
        assert_eq!(width, MIN_WIDTH);
        assert_eq!(height, 3 + ROW_HEIGHT + SEPARATOR_HEIGHT + ROW_HEIGHT + 3);
        assert!(unsafe { DestroyMenu(menu) }.is_ok());
    }

    #[test]
    fn materialization_preserves_duplicates_checks_bitmaps_and_nested_handles() {
        let menu = unsafe { CreatePopupMenu() }.expect("parent popup");
        let child = unsafe { CreatePopupMenu() }.expect("child popup");
        let bitmap = unsafe { windows::Win32::Graphics::Gdi::CreateBitmap(1, 1, 1, 32, None) };
        assert!(!bitmap.is_invalid());
        unsafe {
            AppendMenuW(child, MF_STRING, 41, w!("Child"))
                .and_then(|()| AppendMenuW(menu, MF_STRING, 7, w!("First")))
                .and_then(|()| AppendMenuW(menu, MF_STRING | MF_CHECKED, 7, w!("Duplicate")))
                .and_then(|()| AppendMenuW(menu, MF_POPUP, child.0 as usize, w!("Nested")))
        }
        .expect("fixture menu");
        let bitmap_info = MENUITEMINFOW {
            cbSize: u32::try_from(size_of::<MENUITEMINFOW>()).unwrap_or(u32::MAX),
            fMask: MIIM_BITMAP,
            hbmpItem: bitmap,
            ..Default::default()
        };
        unsafe { SetMenuItemInfoW(menu, 0, true, &raw const bitmap_info) }.expect("set bitmap");

        let (rows, _, _) = materialize(menu, 144).expect("materialize");
        assert_eq!(rows[0].id, 7);
        assert_eq!(rows[1].id, 7);
        assert_eq!(rows[0].bitmap, bitmap);
        assert!(rows[1].state != 0);
        assert_eq!(rows[2].submenu, child);

        assert!(unsafe { DestroyMenu(menu) }.is_ok());
        let _ = unsafe { DeleteObject(HGDIOBJ(bitmap.0)) };
    }

    #[test]
    fn invalid_empty_and_owner_draw_menus_fail_before_window_creation() {
        assert!(matches!(
            materialize(Default::default(), 96),
            Err(PopupUnsupportedReason::EnumerationFailed)
        ));
        let empty = unsafe { CreatePopupMenu() }.expect("empty popup");
        assert!(matches!(
            materialize(empty, 96),
            Err(PopupUnsupportedReason::EmptyMenu)
        ));
        unsafe { AppendMenuW(empty, MF_OWNERDRAW, 9, w!("opaque")) }.expect("owner draw row");
        assert!(matches!(
            materialize(empty, 96),
            Err(PopupUnsupportedReason::UnsupportedOwnerDraw)
        ));
        assert!(unsafe { DestroyMenu(empty) }.is_ok());
    }

    #[test]
    fn selection_skips_separators_and_disabled_rows() {
        let rows = vec![
            Row {
                id: 1,
                state: 0,
                kind: MFT_SEPARATOR.0,
                submenu: Default::default(),
                bitmap: Default::default(),
                text: vec![],
                top: 0,
                height: 9,
            },
            Row {
                id: 2,
                state: MFS_DISABLED.0,
                kind: 0,
                submenu: Default::default(),
                bitmap: Default::default(),
                text: vec![],
                top: 9,
                height: 28,
            },
            Row {
                id: 3,
                state: 0,
                kind: 0,
                submenu: Default::default(),
                bitmap: Default::default(),
                text: vec![],
                top: 37,
                height: 28,
            },
        ];
        let mut state = PopupState {
            menu: Default::default(),
            rows,
            owner: Default::default(),
            dpi: 96,
            dark: false,
            width: 296,
            height: 68,
            content_height: 68,
            scroll_y: 0,
            selected: None,
            pressed: None,
            result: 0,
            replacement_point: None,
            hwnd: Default::default(),
            font: Default::default(),
            font_owned: false,
            shadows: Vec::new(),
        };
        move_selection(&mut state, 1);
        assert_eq!(state.selected, Some(2));
        activate(&mut state, 2);
        assert_eq!(state.result, 3);
        state.height = 30;
        ensure_selection_visible(&mut state);
        assert_eq!(state.scroll_y, 38);
        assert_eq!(hit_test(&state.rows, state.scroll_y), Some(2));
        scroll_by(&mut state, -10_000);
        assert_eq!(state.scroll_y, 0);
        scroll_by(&mut state, 10_000);
        assert_eq!(state.scroll_y, 38);
    }

    #[test]
    fn mnemonic_matching_is_unique_case_insensitive_and_skips_disabled_rows() {
        let row = |id, state, text: &str| Row {
            id,
            state,
            kind: 0,
            submenu: Default::default(),
            bitmap: Default::default(),
            text: text.encode_utf16().collect(),
            top: 0,
            height: ROW_HEIGHT,
        };
        let rows = vec![
            row(1, 0, "&Open"),
            row(2, MFS_DISABLED.0, "&Disabled"),
            row(3, 0, "Save &As"),
        ];
        assert_eq!(mnemonic_match(&rows, Some('O')), Some(0));
        assert_eq!(mnemonic_match(&rows, Some('a')), Some(2));
        assert_eq!(mnemonic_match(&rows, Some('d')), None);

        let duplicate = vec![row(4, 0, "&Open"), row(5, 0, "&Other")];
        assert_eq!(mnemonic_match(&duplicate, Some('o')), None);
    }

    #[test]
    fn light_and_dark_popup_palettes_keep_geometry_but_change_every_surface_role() {
        assert_ne!(surface(false), surface(true));
        assert_ne!(hover(false), hover(true));
        assert_ne!(text_color(false), text_color(true));
        assert_ne!(disabled_text(false), disabled_text(true));
        assert_ne!(divider_color(false), divider_color(true));
        assert_eq!(ROW_HEIGHT, 23);
        assert_eq!(ICON_SLOT, 42);
    }

    #[test]
    fn popup_geometry_clamps_all_four_edges_and_scales_for_required_dpi_values() {
        let work = RECT {
            left: 100,
            top: 50,
            right: 2_020,
            bottom: 1_130,
        };
        assert_eq!(
            clamp_to_work_area(POINT { x: 0, y: 0 }, 300, 400, work),
            POINT { x: 100, y: 50 }
        );
        assert_eq!(
            clamp_to_work_area(POINT { x: 2_000, y: 0 }, 300, 400, work),
            POINT { x: 1_720, y: 50 }
        );
        assert_eq!(
            clamp_to_work_area(POINT { x: 0, y: 1_100 }, 300, 400, work),
            POINT { x: 100, y: 730 }
        );
        assert_eq!(
            clamp_to_work_area(POINT { x: 2_000, y: 1_100 }, 300, 400, work),
            POINT { x: 1_720, y: 730 }
        );
        assert_eq!(
            [96, 120, 144, 192].map(|dpi| scale(ROW_HEIGHT, dpi)),
            [23, 29, 35, 46]
        );
        assert_eq!(
            [96, 120, 144, 192].map(|dpi| scale(ICON_SLOT, dpi)),
            [42, 53, 63, 84]
        );
    }

    #[test]
    fn controlled_popup_host_1000_cycle_resource_slope_is_bounded() {
        let owner = unsafe {
            CreateWindowExW(
                Default::default(),
                w!("STATIC"),
                w!(""),
                WS_POPUP,
                0,
                0,
                10,
                10,
                None,
                None,
                None,
                None,
            )
        }
        .expect("controlled owner");
        let menu = unsafe { CreatePopupMenu() }.expect("controlled popup");
        unsafe { AppendMenuW(menu, MF_STRING, 1, w!("&Open")) }.expect("controlled row");
        let thread_id = unsafe { GetCurrentThreadId() };
        let (cancel_tx, cancel_rx) = std::sync::mpsc::sync_channel::<()>(0);
        let (posted_tx, posted_rx) = std::sync::mpsc::sync_channel::<()>(0);
        let cancel = std::thread::spawn(move || {
            while cancel_rx.recv().is_ok() {
                std::thread::sleep(std::time::Duration::from_millis(1));
                unsafe {
                    PostThreadMessageW(thread_id, WM_KEYDOWN, WPARAM(27), LPARAM(0))
                        .expect("Escape down");
                    PostThreadMessageW(thread_id, WM_KEYUP, WPARAM(27), LPARAM(0))
                        .expect("Escape up");
                }
                posted_tx.send(()).expect("Escape ack");
            }
        });
        let run_cycle = || {
            cancel_tx.send(()).expect("request Escape");
            let outcome = present(menu, owner, POINT { x: 20, y: 20 }, 96, false)
                .expect("controlled popup outcome");
            assert_eq!(outcome.command, 0);
            assert!(outcome.replacement_point.is_none());
            posted_rx.recv().expect("Escape posted");
        };
        for _ in 0..100 {
            run_cycle();
        }
        let before = popup_process_resources();
        for _ in 0..1_000 {
            run_cycle();
        }
        let after = popup_process_resources();
        eprintln!(
            "controlled-popup-soak cycles=1000 handles_before={} handles_after={} private_bytes_before={} private_bytes_after={}",
            before.0, after.0, before.1, after.1
        );
        assert!(after.0 <= before.0.saturating_add(4));
        assert!(after.1 <= before.1.saturating_add(16 * 1024 * 1024));
        drop(cancel_tx);
        cancel.join().expect("cancel sender");
        assert!(unsafe { DestroyMenu(menu) }.is_ok());
        assert!(unsafe { DestroyWindow(owner) }.is_ok());
    }

    fn popup_process_resources() -> (u32, usize) {
        let process = unsafe { windows::Win32::System::Threading::GetCurrentProcess() };
        let mut handles = 0_u32;
        unsafe {
            windows::Win32::System::Threading::GetProcessHandleCount(process, &raw mut handles)
        }
        .expect("process handles");
        let mut counters =
            windows::Win32::System::ProcessStatus::PROCESS_MEMORY_COUNTERS_EX::default();
        unsafe {
            windows::Win32::System::ProcessStatus::GetProcessMemoryInfo(
                process,
                (&raw mut counters)
                    .cast::<windows::Win32::System::ProcessStatus::PROCESS_MEMORY_COUNTERS>(),
                u32::try_from(size_of_val(&counters)).expect("counter size"),
            )
        }
        .expect("private bytes");
        (handles, counters.PrivateUsage)
    }

    #[test]
    fn cancellation_and_invalid_owner_are_terminal_without_resources() {
        assert!(dismissal_message(WM_ACTIVATEAPP));
        assert!(dismissal_message(WM_CANCELMODE));
        assert!(dismissal_message(WM_RBUTTONDOWN));
        assert!(matches!(
            present(
                Default::default(),
                Default::default(),
                POINT::default(),
                96,
                false
            ),
            Err(PopupUnsupportedReason::InvalidMenu)
        ));
    }

    #[test]
    fn application_deactivation_dismisses_without_selection_or_replay() {
        let owner = unsafe {
            CreateWindowExW(
                Default::default(),
                w!("STATIC"),
                w!(""),
                WS_POPUP,
                0,
                0,
                10,
                10,
                None,
                None,
                None,
                None,
            )
        }
        .expect("controlled owner");
        let menu = unsafe { CreatePopupMenu() }.expect("controlled popup");
        unsafe { AppendMenuW(menu, MF_STRING, 1, w!("&Open")) }.expect("controlled row");
        let thread_id = unsafe { GetCurrentThreadId() };
        let deactivate = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(2));
            unsafe {
                PostThreadMessageW(thread_id, WM_ACTIVATEAPP, WPARAM(0), LPARAM(0))
                    .expect("deactivate popup");
            }
        });
        let outcome =
            present(menu, owner, POINT { x: 20, y: 20 }, 96, false).expect("deactivation outcome");
        deactivate.join().expect("deactivation sender");
        assert_eq!(outcome.command, 0);
        assert!(outcome.replacement_point.is_none());
        assert!(unsafe { DestroyMenu(menu) }.is_ok());
        assert!(unsafe { DestroyWindow(owner) }.is_ok());
    }

    #[test]
    fn keyboard_arrows_enter_and_nested_submenu_dispatch_through_the_real_modal_loop() {
        let owner = unsafe {
            CreateWindowExW(
                Default::default(),
                w!("STATIC"),
                w!(""),
                WS_POPUP,
                0,
                0,
                10,
                10,
                None,
                None,
                None,
                None,
            )
        }
        .expect("controlled owner");
        let child = unsafe { CreatePopupMenu() }.expect("child popup");
        unsafe { AppendMenuW(child, MF_STRING, 313, w!("&Child")) }.expect("child row");
        let menu = unsafe { CreatePopupMenu() }.expect("parent popup");
        unsafe { AppendMenuW(menu, MF_POPUP, child.0 as usize, w!("&Nested")) }
            .expect("nested row");

        let thread_id = unsafe { GetCurrentThreadId() };
        let keyboard = std::thread::spawn(move || {
            // Select and open the parent row, then give the nested HWND time to enter its own
            // modal loop before selecting and invoking the child row.
            std::thread::sleep(std::time::Duration::from_millis(3));
            unsafe {
                PostThreadMessageW(thread_id, WM_KEYDOWN, WPARAM(0x28), LPARAM(0))
                    .expect("parent Down");
                PostThreadMessageW(thread_id, WM_KEYDOWN, WPARAM(0x27), LPARAM(0))
                    .expect("parent Right");
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
            unsafe {
                PostThreadMessageW(thread_id, WM_KEYDOWN, WPARAM(0x28), LPARAM(0))
                    .expect("child Down");
                PostThreadMessageW(thread_id, WM_KEYDOWN, WPARAM(0x0D), LPARAM(0))
                    .expect("child Enter");
            }
        });

        let outcome = present(menu, owner, POINT { x: 20, y: 20 }, 96, false)
            .expect("keyboard nested popup outcome");
        keyboard.join().expect("keyboard sender");
        assert_eq!(outcome.command, 313);
        assert!(outcome.replacement_point.is_none());
        assert!(unsafe { DestroyMenu(menu) }.is_ok());
        assert!(unsafe { DestroyWindow(owner) }.is_ok());
    }

    #[test]
    fn dynamic_submenu_initialization_runs_on_owner_before_rematerialization() {
        DYNAMIC_INIT_COUNT.store(0, Ordering::SeqCst);
        static CLASS: OnceLock<()> = OnceLock::new();
        let instance = unsafe { GetModuleHandleW(None) }.expect("module");
        CLASS.get_or_init(|| {
            let class = WNDCLASSW {
                lpfnWndProc: Some(dynamic_owner_proc),
                hInstance: HINSTANCE(instance.0),
                lpszClassName: w!("SuperExplorer.PopupDynamicOwnerTest.v1"),
                ..Default::default()
            };
            assert_ne!(unsafe { RegisterClassW(&raw const class) }, 0);
        });
        let owner = unsafe {
            CreateWindowExW(
                Default::default(),
                w!("SuperExplorer.PopupDynamicOwnerTest.v1"),
                w!(""),
                WS_POPUP,
                0,
                0,
                1,
                1,
                None,
                None,
                Some(HINSTANCE(instance.0)),
                None,
            )
        }
        .expect("owner window");
        let submenu = unsafe { CreatePopupMenu() }.expect("submenu");
        initialize_submenu(owner, submenu, 2);
        let (rows, _, _) = materialize(submenu, 96).expect("dynamic materialization");
        assert_eq!(DYNAMIC_INIT_COUNT.load(Ordering::SeqCst), 1);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, 313);
        assert!(unsafe { DestroyMenu(submenu) }.is_ok());
        assert!(unsafe { DestroyWindow(owner) }.is_ok());
    }

    fn with_popup_test_fault<T>(fault: PopupTestFault, action: impl FnOnce() -> T) -> T {
        struct Reset;
        impl Drop for Reset {
            fn drop(&mut self) {
                POPUP_TEST_FAULT.with(|current| current.set(PopupTestFault::None));
            }
        }
        POPUP_TEST_FAULT.with(|current| {
            assert_eq!(current.replace(fault), PopupTestFault::None);
        });
        let _reset = Reset;
        action()
    }

    fn cancel_controlled_popup(
        menu: windows::Win32::UI::WindowsAndMessaging::HMENU,
        owner: HWND,
        dark: bool,
    ) -> PopupPresentation {
        let thread_id = unsafe { GetCurrentThreadId() };
        let cancel = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(2));
            unsafe {
                PostThreadMessageW(thread_id, WM_KEYDOWN, WPARAM(27), LPARAM(0))
                    .expect("Escape down");
            }
        });
        let outcome = present(menu, owner, POINT { x: 20, y: 20 }, 96, dark)
            .expect("subsequent popup remains available");
        cancel.join().expect("cancel sender");
        outcome
    }

    #[test]
    fn forced_resolver_apply_message_and_cleanup_failures_are_local_to_one_session() {
        let owner = unsafe {
            CreateWindowExW(
                Default::default(),
                w!("STATIC"),
                w!(""),
                WS_POPUP,
                0,
                0,
                10,
                10,
                None,
                None,
                None,
                None,
            )
        }
        .expect("controlled owner");
        let menu = unsafe { CreatePopupMenu() }.expect("controlled popup");
        unsafe { AppendMenuW(menu, MF_STRING, 1, w!("&Open")) }.expect("controlled row");

        let unsupported = unsafe { CreatePopupMenu() }.expect("unsupported popup");
        unsafe { AppendMenuW(unsupported, MF_OWNERDRAW, 9, w!("opaque")) }.expect("owner-draw row");
        assert!(matches!(
            materialize(unsupported, 96),
            Err(PopupUnsupportedReason::UnsupportedOwnerDraw)
        ));
        assert!(unsafe { DestroyMenu(unsupported) }.is_ok());

        for (fault, expected) in [
            (
                PopupTestFault::Apply,
                PopupUnsupportedReason::WindowCreationFailed,
            ),
            (
                PopupTestFault::Message,
                PopupUnsupportedReason::MessageLoopFailed,
            ),
            (
                PopupTestFault::Cleanup,
                PopupUnsupportedReason::CleanupFailed,
            ),
        ] {
            let result = with_popup_test_fault(fault, || {
                present(menu, owner, POINT { x: 20, y: 20 }, 96, false)
            });
            assert!(matches!(result, Err(reason) if reason == expected));
            let subsequent = cancel_controlled_popup(menu, owner, false);
            assert_eq!(subsequent.command, 0);
            assert!(subsequent.replacement_point.is_none());
        }

        assert!(unsafe { DestroyMenu(menu) }.is_ok());
        assert!(unsafe { DestroyWindow(owner) }.is_ok());
    }

    #[test]
    fn consecutive_light_dark_light_sessions_use_fresh_theme_without_restart() {
        let owner = unsafe {
            CreateWindowExW(
                Default::default(),
                w!("STATIC"),
                w!(""),
                WS_POPUP,
                0,
                0,
                10,
                10,
                None,
                None,
                None,
                None,
            )
        }
        .expect("controlled owner");
        let menu = unsafe { CreatePopupMenu() }.expect("controlled popup");
        unsafe { AppendMenuW(menu, MF_STRING, 1, w!("&Open")) }.expect("controlled row");
        for dark in [false, true, false] {
            let outcome = cancel_controlled_popup(menu, owner, dark);
            assert_eq!(outcome.command, 0);
            assert!(outcome.replacement_point.is_none());
        }
        assert!(unsafe { DestroyMenu(menu) }.is_ok());
        assert!(unsafe { DestroyWindow(owner) }.is_ok());
    }
}
