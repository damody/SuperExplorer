//! Native Shell `IContextMenu` session hosted by a dedicated STA owner window.
#![allow(
    unsafe_code,
    reason = "Shell PIDLs, HMENU, HWND, and context-menu message forwarding require audited FFI"
)]

use std::{
    cell::Cell,
    ffi::c_void,
    fs::File,
    io::{Read as _, Seek as _, Write as _},
    mem::size_of,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use crate::sta::RequiredTerminalPublisher;
use explorer_common::{
    ErrorSeverity, ExplorerError, ExplorerErrorKind, panic_payload_message, record_process_error,
    record_process_error_message,
};
use explorer_model::{
    ContextMenuHostCommand, ContextMenuInvocationProfile, ContextMenuOutcome, ContextMenuRequest,
    ContextMenuSession, ContextMenuSessionState, ExplorerEvent, RequestContext,
    ShellContextMenuTarget,
};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
        Graphics::Gdi::{
            GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint,
            MonitorFromWindow,
        },
        System::{
            Com::IBindCtx,
            LibraryLoader::GetModuleHandleW,
            Ole::{OleInitialize, OleUninitialize},
            Threading::GetCurrentProcessId,
        },
        UI::{
            Accessibility::{
                HCF_HIGHCONTRASTON, HIGHCONTRASTW, HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent,
            },
            HiDpi::GetDpiForWindow,
            Shell::{
                CMF_CANRENAME, CMF_EXPLORE, CMF_EXTENDEDVERBS, CMF_ITEMMENU, CMF_NORMAL,
                CMF_SYNCCASCADEMENU, CMIC_MASK_PTINVOKE, CMINVOKECOMMANDINFO,
                CMINVOKECOMMANDINFOEX, GCS_VERBA, GCS_VERBW, IContextMenu, IContextMenu3,
                ILFindLastID, ILIsParent, IShellFolder, SHBindToObject,
            },
            WindowsAndMessaging::{
                AppendMenuW, CREATESTRUCTW, CallNextHookEx, CreatePopupMenu, CreateWindowExW,
                DefWindowProcW, DestroyMenu, DestroyWindow, EVENT_OBJECT_SHOW, EnumWindows,
                GA_ROOT, GWLP_USERDATA, GetAncestor, GetClassNameW, GetCursorPos, GetMenuItemCount,
                GetMenuItemID, GetMenuStringW, GetSubMenu, GetWindowLongPtrW, GetWindowRect,
                GetWindowThreadProcessId, HHOOK, HMENU, InsertMenuW, IsWindow, IsWindowVisible,
                MF_BYPOSITION, MF_CHECKED, MF_DISABLED, MF_POPUP, MF_SEPARATOR, MF_STRING,
                OBJID_WINDOW, PostMessageW, RegisterClassW, SPI_GETHIGHCONTRAST, SW_SHOWNORMAL,
                SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SetForegroundWindow, SetWindowLongPtrW,
                SetWindowPos, SetWindowsHookExW, SystemParametersInfoW, TPM_LEFTALIGN,
                TPM_RETURNCMD, TPM_RIGHTBUTTON, TPM_TOPALIGN, TrackPopupMenuEx,
                UnhookWindowsHookEx, WH_MOUSE_LL, WINDOW_EX_STYLE, WINEVENT_INCONTEXT,
                WM_CANCELMODE, WM_DRAWITEM, WM_INITMENUPOPUP, WM_MEASUREITEM, WM_MENUCHAR,
                WM_NCCREATE, WM_RBUTTONDOWN, WM_RBUTTONUP, WNDCLASSW, WS_POPUP, WindowFromPoint,
            },
        },
    },
    core::{BOOL, Interface, PCSTR, PCWSTR, PSTR, w},
};

const COMMAND_FIRST: u32 = 1;
const COMMAND_LAST: u32 = 0x7fff;
// The windows crate does not currently project this CMINVOKECOMMANDINFOEX flag.
const CMIC_MASK_UNICODE: u32 = 0x0000_4000;
const MENU_REPLAY_EXTRA_INFO: usize = 0x5355_5045_524D_454E;
const PROPERTIES_PLACEMENT_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnedPopupMenuItem {
    pub label: String,
    pub checked: bool,
    pub enabled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OwnedPopupMenuEntry {
    Item(OwnedPopupMenuItem),
    Separator,
}

/// Presents application-owned commands through the same top-level popup renderer used by
/// filesystem context menus. The returned index counts command rows and excludes separators.
pub fn show_owned_popup_menu(
    owner_window: u64,
    x: i32,
    y: i32,
    entries: &[OwnedPopupMenuEntry],
    dark: bool,
    immersive: bool,
) -> Result<Option<usize>, ExplorerError> {
    let mut point = POINT { x, y };
    // Match filesystem context menus: pointer events can cross a DPI-virtualized test or host
    // boundary, while the live cursor is already in the desktop coordinate space required by
    // the popup window.
    let _ = unsafe { GetCursorPos(&raw mut point) };
    let owner = validated_owner_window(owner_window).ok_or_else(|| {
        menu_error(
            "validate owned popup window",
            "無法顯示功能表",
            "the owner window is unavailable",
        )
    })?;
    let popup = OwnedMenu::create()?;
    let mut command_count = 0_u32;
    for entry in entries {
        match entry {
            OwnedPopupMenuEntry::Separator => unsafe {
                AppendMenuW(popup.get(), MF_SEPARATOR, 0, PCWSTR::null())
            },
            OwnedPopupMenuEntry::Item(item) => {
                command_count = command_count.saturating_add(1);
                let mut flags = MF_STRING;
                if item.checked {
                    flags |= MF_CHECKED;
                }
                if !item.enabled {
                    flags |= MF_DISABLED;
                }
                let label = item.label.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
                unsafe {
                    AppendMenuW(
                        popup.get(),
                        flags,
                        command_count as usize,
                        PCWSTR(label.as_ptr()),
                    )
                }
            }
        }
        .map_err(|error| {
            menu_error(
                "append owned popup command",
                "無法建立功能表",
                &error.to_string(),
            )
        })?;
    }
    let _ = unsafe { SetForegroundWindow(owner) };
    let dpi = unsafe { GetDpiForWindow(owner) }.max(96);
    let selected = if should_use_owned_popup(immersive, high_contrast_active()) {
        crate::immersive_popup::present(popup.get(), owner, point, dpi, dark)
            .map(|presentation| presentation.command)
            .unwrap_or_else(|reason| {
                tracing::warn!(?reason, "application-owned popup menu fell back");
                unsafe {
                    TrackPopupMenuEx(
                        popup.get(),
                        (TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_LEFTALIGN | TPM_TOPALIGN).0,
                        point.x,
                        point.y,
                        owner,
                        None,
                    )
                }
                .0
            })
    } else {
        unsafe {
            TrackPopupMenuEx(
                popup.get(),
                (TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_LEFTALIGN | TPM_TOPALIGN).0,
                point.x,
                point.y,
                owner,
                None,
            )
        }
        .0
    };
    Ok((selected != 0).then(|| selected.saturating_sub(1) as usize))
}
const WINDOWS_DIALOG_CLASS: &[u16] = &[
    b'#' as u16,
    b'3' as u16,
    b'2' as u16,
    b'7' as u16,
    b'7' as u16,
    b'0' as u16,
];
static ACTIVE_MENUS: AtomicUsize = AtomicUsize::new(0);
static ACTIVE_OWNER_WINDOWS: AtomicUsize = AtomicUsize::new(0);
static ACTIVE_MENU_HOOKS: AtomicUsize = AtomicUsize::new(0);
static FORWARDED_MENU_MESSAGES: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    static MENU_RIGHT_CLICK: Cell<MenuRightClickCapture> = const { Cell::new(MenuRightClickCapture::EMPTY) };
    static MENU_REPLAY_OWNER: Cell<Option<HWND>> = const { Cell::new(None) };
    static MENU_POPUP_OWNER: Cell<Option<HWND>> = const { Cell::new(None) };
}
static PROPERTIES_PLACEMENT: Mutex<Option<PropertiesPlacementState>> = Mutex::new(None);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ScreenPoint {
    x: i32,
    y: i32,
}

impl From<POINT> for ScreenPoint {
    fn from(point: POINT) -> Self {
        Self {
            x: point.x,
            y: point.y,
        }
    }
}

impl From<ScreenPoint> for POINT {
    fn from(point: ScreenPoint) -> Self {
        Self {
            x: point.x,
            y: point.y,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct PropertiesPlacementState {
    anchor: RECT,
    work_area: RECT,
    claimed: bool,
    completed: bool,
    hook_value: usize,
}

fn valid_rect(rect: RECT) -> bool {
    rect.right > rect.left && rect.bottom > rect.top
}

fn centered_axis(
    anchor_start: i32,
    anchor_end: i32,
    work_start: i32,
    work_end: i32,
    size: i32,
) -> Option<i32> {
    if anchor_end <= anchor_start || work_end <= work_start || size <= 0 {
        return None;
    }
    let anchor_start = i64::from(anchor_start);
    let anchor_end = i64::from(anchor_end);
    let work_start = i64::from(work_start);
    let work_end = i64::from(work_end);
    let size = i64::from(size);
    let work_size = work_end - work_start;
    let position = if size >= work_size {
        work_start
    } else {
        let centered = anchor_start + ((anchor_end - anchor_start - size) / 2);
        centered.clamp(work_start, work_end - size)
    };
    i32::try_from(position).ok()
}

fn centered_window_position(anchor: RECT, work_area: RECT, window: RECT) -> Option<POINT> {
    if !valid_rect(anchor) || !valid_rect(work_area) || !valid_rect(window) {
        return None;
    }
    let width = window.right.checked_sub(window.left)?;
    let height = window.bottom.checked_sub(window.top)?;
    Some(POINT {
        x: centered_axis(
            anchor.left,
            anchor.right,
            work_area.left,
            work_area.right,
            width,
        )?,
        y: centered_axis(
            anchor.top,
            anchor.bottom,
            work_area.top,
            work_area.bottom,
            height,
        )?,
    })
}

fn placement_anchor(owner: Option<RECT>, work_area: RECT) -> Option<RECT> {
    if !valid_rect(work_area) {
        return None;
    }
    Some(owner.filter(|rect| valid_rect(*rect)).unwrap_or(work_area))
}

fn should_center_properties(verb: &str) -> bool {
    verb.eq_ignore_ascii_case("properties")
}

fn claim_properties_placement() -> Option<PropertiesPlacementState> {
    let mut state = PROPERTIES_PLACEMENT.lock().ok()?;
    let placement = state.as_mut()?;
    if placement.claimed || placement.completed {
        return None;
    }
    placement.claimed = true;
    Some(*placement)
}

fn finish_properties_placement(completed: bool) {
    let hook_value = if let Ok(mut state) = PROPERTIES_PLACEMENT.lock()
        && let Some(placement) = state.as_mut()
    {
        placement.claimed = false;
        placement.completed = completed;
        if completed {
            std::mem::take(&mut placement.hook_value)
        } else {
            0
        }
    } else {
        0
    };
    release_properties_hook(hook_value);
}

fn monitor_work_area(owner: Option<HWND>, point: POINT) -> Option<(RECT, RECT)> {
    let owner_rect = owner.and_then(|owner| {
        let mut rect = RECT::default();
        // SAFETY: `owner` was validated and `rect` is writable for the duration of the call.
        unsafe { GetWindowRect(owner, &raw mut rect) }
            .ok()
            .filter(|()| valid_rect(rect))
            .map(|()| rect)
    });
    // SAFETY: monitor lookup accepts a validated HWND or physical screen point by value.
    let monitor = match (owner_rect, owner) {
        (Some(_), Some(owner)) => unsafe { MonitorFromWindow(owner, MONITOR_DEFAULTTONEAREST) },
        _ => unsafe { MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST) },
    };
    if monitor.0.is_null() {
        return None;
    }
    let mut info = MONITORINFO {
        cbSize: u32::try_from(size_of::<MONITORINFO>()).ok()?,
        ..MONITORINFO::default()
    };
    // SAFETY: monitor is live and `info` declares the projected structure size.
    if !unsafe { GetMonitorInfoW(monitor, &raw mut info) }.as_bool() || !valid_rect(info.rcWork) {
        return None;
    }
    Some((placement_anchor(owner_rect, info.rcWork)?, info.rcWork))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct MenuRightClickCapture {
    pressed: Option<ScreenPoint>,
    completed: Option<ScreenPoint>,
    cancel_requested: bool,
}

impl MenuRightClickCapture {
    const EMPTY: Self = Self {
        pressed: None,
        completed: None,
        cancel_requested: false,
    };

    fn observe(
        &mut self,
        message: u32,
        point: ScreenPoint,
        belongs_to_owner: bool,
        tagged_replay: bool,
    ) -> MenuHookAction {
        if tagged_replay {
            return MenuHookAction::Pass;
        }
        match message {
            WM_RBUTTONDOWN if belongs_to_owner => {
                self.pressed = Some(point);
                if !self.cancel_requested {
                    self.completed = None;
                }
                MenuHookAction::Suppress
            }
            WM_RBUTTONDOWN => {
                self.pressed = None;
                if !self.cancel_requested {
                    self.completed = None;
                }
                MenuHookAction::Pass
            }
            WM_RBUTTONUP if self.pressed.is_some() && belongs_to_owner => {
                self.pressed = None;
                self.completed = Some(point);
                if self.cancel_requested {
                    MenuHookAction::Suppress
                } else {
                    self.cancel_requested = true;
                    MenuHookAction::SuppressAndPostCancel
                }
            }
            WM_RBUTTONUP if self.pressed.take().is_some() => {
                if !self.cancel_requested {
                    self.completed = None;
                }
                MenuHookAction::Suppress
            }
            _ => MenuHookAction::Pass,
        }
    }

    fn take_completed(&mut self) -> Option<ScreenPoint> {
        let completed = self.completed;
        *self = Self::EMPTY;
        completed
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MenuHookAction {
    Pass,
    Suppress,
    SuppressAndPostCancel,
}

pub(crate) fn start_brokered<P: RequiredTerminalPublisher>(
    context: RequestContext,
    request: ContextMenuRequest,
    events: P,
) {
    let deadline = Duration::from_millis(u64::from(request.deadline_ms.max(1)));
    start_bounded_job(context, deadline, events, move || {
        // SAFETY: the broker worker owns one apartment for its entire native menu session.
        unsafe { OleInitialize(None) }
            .map_err(|error| native_menu_error("initialize context menu broker", &error))?;
        let _apartment = OleApartment;
        show(&request)
    });
}

/// Runs an application-owned built-in command on the caller's persistent Shell STA.
///
/// The Shell STA owns COM/OLE and pumps messages for its full lifetime. Keeping target
/// resolution and invocation on that apartment prevents a Properties handler from retaining
/// interfaces that belonged to a disposable per-click thread.
pub(crate) fn run_host_owned<P: RequiredTerminalPublisher>(
    context: &RequestContext,
    request: &ContextMenuRequest,
    events: P,
) {
    let result = show(request);
    emit_broker_terminal(&AtomicBool::new(false), context, &events, result);
}

fn start_bounded_job<F, P>(context: RequestContext, deadline: Duration, events: P, job: F)
where
    F: FnOnce() -> Result<ContextMenuOutcome, ExplorerError> + Send + 'static,
    P: RequiredTerminalPublisher,
{
    start_bounded_job_inner(context, deadline, events, job);
}

fn start_bounded_job_inner<F, P>(context: RequestContext, deadline: Duration, events: P, job: F)
where
    F: FnOnce() -> Result<ContextMenuOutcome, ExplorerError> + Send + 'static,
    P: RequiredTerminalPublisher,
{
    let terminal_sent = Arc::new(AtomicBool::new(false));
    let worker_gate = Arc::clone(&terminal_sent);
    let worker_context = context.clone();
    let worker_events = events.clone();
    std::thread::spawn(move || {
        let result = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(job)) {
            Ok(completion) => completion,
            Err(payload) => {
                let message = panic_payload_message(payload.as_ref());
                record_process_error_message(
                    ErrorSeverity::Critical,
                    "shell",
                    "context_menu_worker_panic",
                    &message,
                    Some(file!()),
                );
                Err(ExplorerError::new(
                    ExplorerErrorKind::Extension,
                    "context menu handler panic",
                    true,
                    "The context menu failed, but Explorer can continue.",
                    message,
                ))
            }
        };
        let _ = emit_broker_terminal(&worker_gate, &worker_context, &worker_events, result);
    });
    std::thread::spawn(move || {
        std::thread::sleep(deadline);
        let error = ExplorerError::new(
            ExplorerErrorKind::Availability,
            "context menu handler deadline",
            true,
            "內容功能表延伸模組沒有及時回應，檔案總管仍可繼續使用。",
            format!(
                "correlation={:?}; deadline_ms={}; handler worker left isolated",
                context.request_id,
                deadline.as_millis()
            ),
        );
        emit_broker_terminal(&terminal_sent, &context, &events, Err(error));
    });
}

fn emit_broker_terminal<P: RequiredTerminalPublisher>(
    gate: &AtomicBool,
    context: &RequestContext,
    events: &P,
    result: Result<ContextMenuOutcome, ExplorerError>,
) -> bool {
    if gate
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        tracing::warn!(
            request_id = ?context.request_id,
            tab_id = ?context.tab_id,
            generation = context.generation.value(),
            "ignored late context-menu broker terminal"
        );
        return false;
    }
    let outcome = match result {
        Ok(outcome) => outcome,
        Err(mut error) => {
            error.technical_detail = format!(
                "correlation={:?}; {}",
                context.request_id, error.technical_detail
            );
            record_process_error(
                ErrorSeverity::Error,
                "shell",
                &error.operation,
                &error,
                Some(file!()),
            );
            ContextMenuOutcome::Failed { error }
        }
    };
    let event = ExplorerEvent::ContextMenuFinished {
        context: context.clone(),
        outcome,
    };
    events.publish_terminal(event);
    true
}

struct OleApartment;
impl Drop for OleApartment {
    fn drop(&mut self) {
        // SAFETY: balances the successful OleInitialize on this worker thread.
        unsafe { OleUninitialize() };
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContextMenuResourceSnapshot {
    pub active_menus: usize,
    pub active_owner_windows: usize,
    pub active_menu_hooks: usize,
    pub forwarded_messages: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextMenuQuerySnapshot {
    pub command_count: i32,
    pub label_fingerprints: Vec<u64>,
}

impl ContextMenuResourceSnapshot {
    pub fn capture() -> Self {
        Self {
            active_menus: ACTIVE_MENUS.load(Ordering::Acquire),
            active_owner_windows: ACTIVE_OWNER_WINDOWS.load(Ordering::Acquire),
            active_menu_hooks: ACTIVE_MENU_HOOKS.load(Ordering::Acquire),
            forwarded_messages: FORWARDED_MENU_MESSAGES.load(Ordering::Acquire),
        }
    }
}

pub(crate) fn show(request: &ContextMenuRequest) -> Result<ContextMenuOutcome, ExplorerError> {
    let started = Instant::now();
    // Capture immediately, before Shell extensions are queried, so a slow handler cannot move
    // the menu away from the point where the secondary button was released.
    let mut popup_point = POINT {
        x: request.point.x,
        y: request.point.y,
    };
    if !request.keyboard_invoked {
        // SAFETY: `popup_point` is valid writable POINT storage for the call.
        let _ = unsafe { GetCursorPos(&raw mut popup_point) };
    }
    let mut state = ContextMenuSession::default();
    let _ = state.transition(ContextMenuSessionState::Resolving);
    let mut owner_state = MenuOwnerState { menu3: None };
    let app_owner = validated_owner_window(request.owner_window);
    let owner = OwnerWindow::create_owned(&raw mut owner_state, app_owner)?;
    let menu = resolve_menu(&request.target, owner.hwnd())?;
    owner_state.menu3 = menu.cast::<IContextMenu3>().ok();
    let _ = state.transition(ContextMenuSessionState::Querying);
    let popup = OwnedMenu::create()?;
    let profile = if request.requested_verb.is_some() {
        ContextMenuInvocationProfile::ExplorerExtended
    } else {
        request.invocation_profile
    };
    let command_count = query_menu(
        &menu,
        popup.get(),
        matches!(request.target, ShellContextMenuTarget::Items { .. }),
        profile,
    )?;
    let apk_devices = local_apk_devices(&request.target);
    let item_menu = matches!(request.target, ShellContextMenuTarget::Items { .. });
    if item_menu || request.paste_available {
        let first_custom_id = COMMAND_FIRST
            .saturating_add(u32::try_from(command_count).unwrap_or(COMMAND_LAST - COMMAND_FIRST));
        unsafe { AppendMenuW(popup.get(), MF_SEPARATOR, 0, PCWSTR::null()) }.map_err(|error| {
            menu_error(
                "append host command separator",
                "無法建立應用程式命令",
                &error.to_string(),
            )
        })?;
        let mut next_custom_id = first_custom_id;
        if request.paste_available {
            unsafe { AppendMenuW(popup.get(), MF_STRING, next_custom_id as usize, w!("貼上")) }
                .map_err(|error| {
                    menu_error(
                        "append paste command",
                        "無法建立貼上命令",
                        &error.to_string(),
                    )
                })?;
            next_custom_id = next_custom_id.saturating_add(1);
        }
        if item_menu {
            unsafe {
                AppendMenuW(
                    popup.get(),
                    MF_STRING,
                    next_custom_id as usize,
                    w!("加入書籤"),
                )
            }
            .map_err(|error| {
                menu_error(
                    "append bookmark command",
                    "無法建立加入書籤命令",
                    &error.to_string(),
                )
            })?;
        }
    }
    let apk_first_id = COMMAND_FIRST
        .saturating_add(u32::try_from(command_count).unwrap_or(COMMAND_LAST - COMMAND_FIRST))
        .saturating_add(u32::from(request.paste_available))
        .saturating_add(u32::from(item_menu));
    if let Some(menu_data) = &apk_devices {
        let submenu = unsafe { CreatePopupMenu() }.map_err(|error| {
            menu_error("create APK submenu", "無法建立安裝選單", &error.to_string())
        })?;
        if matches!(menu_data, ApkMenuData::MissingTool) {
            unsafe {
                AppendMenuW(
                    submenu,
                    MF_STRING,
                    apk_first_id as usize,
                    w!("下載並安裝 Google 官方 ADB…"),
                )
            }
            .map_err(|error| {
                menu_error(
                    "append ADB download",
                    "無法建立 ADB 下載命令",
                    &error.to_string(),
                )
            })?;
        } else if let ApkMenuData::Devices(devices) = menu_data {
            if devices.is_empty() {
                unsafe {
                    AppendMenuW(
                        submenu,
                        MF_STRING | MF_DISABLED,
                        apk_first_id as usize,
                        w!("未偵測到裝置"),
                    )
                }
                .map_err(|error| {
                    menu_error(
                        "append empty APK device state",
                        "無法建立裝置命令",
                        &error.to_string(),
                    )
                })?;
            }
            for (index, device) in devices.iter().enumerate() {
                let id = apk_first_id.saturating_add(u32::try_from(index).unwrap_or(u32::MAX));
                let label = if device.is_installable() {
                    device.display_name().to_owned()
                } else {
                    format!("{} ({:?})", device.display_name(), device.state)
                };
                let wide = label.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
                let flags = if device.is_installable() {
                    MF_STRING
                } else {
                    MF_STRING | MF_DISABLED
                };
                unsafe { AppendMenuW(submenu, flags, id as usize, PCWSTR(wide.as_ptr())) }
                    .map_err(|error| {
                        menu_error("append APK device", "無法建立裝置命令", &error.to_string())
                    })?;
            }
        }
        unsafe {
            InsertMenuW(
                popup.get(),
                0,
                MF_BYPOSITION | MF_POPUP,
                submenu.0 as usize,
                w!("安裝"),
            )
        }
        .map_err(|error| {
            menu_error(
                "insert APK submenu first",
                "無法建立安裝選單",
                &error.to_string(),
            )
        })?;
        unsafe { InsertMenuW(popup.get(), 1, MF_BYPOSITION | MF_SEPARATOR, 0, None) }.map_err(
            |error| {
                menu_error(
                    "insert APK submenu separator",
                    "無法建立安裝選單",
                    &error.to_string(),
                )
            },
        )?;
    }
    if started.elapsed() > Duration::from_millis(u64::from(request.deadline_ms.max(1))) {
        return Err(menu_error(
            "query context menu",
            "內容功能表處理常式逾時",
            "deadline exceeded before show",
        ));
    }
    let _ = state.transition(ContextMenuSessionState::Showing);
    if let Some(verb) = request.requested_verb.as_deref() {
        let command_offset = match canonical_verb_offset(&menu, command_count, verb) {
            Ok(offset) => offset,
            Err(_) if verb == "Windows.CompressToZip" => {
                compress_selection_to_zip(&request.target)?;
                let _ = state.transition(ContextMenuSessionState::Invoking);
                let _ = state.transition(ContextMenuSessionState::Finished);
                let _ = state.release();
                return Ok(ContextMenuOutcome::Invoked { command_offset: 0 });
            }
            Err(error) => return Err(error),
        };
        let _ = state.transition(ContextMenuSessionState::Invoking);
        invoke_host_owned(
            &menu,
            app_owner.unwrap_or_else(|| owner.hwnd()),
            command_offset,
            popup_point,
            should_center_properties(verb),
            app_owner,
        )?;
        let _ = state.transition(ContextMenuSessionState::Finished);
        let _ = state.release();
        return Ok(ContextMenuOutcome::Invoked { command_offset });
    }
    // SAFETY: the hidden owner window lives through the modal menu loop.
    let _ = unsafe { SetForegroundWindow(owner.hwnd()) };
    // SAFETY: the hidden menu owner is a live HWND until this function returns.
    let dpi = unsafe { GetDpiForWindow(owner.hwnd()) }.max(96);
    // SAFETY: HMENU and HWND remain valid; null TPMPARAMS selects default exclusion behavior.
    // Mouse events can arrive in GPUI logical/client coordinates. Querying the cursor here avoids
    // applying a window origin or DPI scale twice and matches Explorer's TrackPopupMenu contract.
    let replay_hook = MenuRightClickReplayHook::install(app_owner, owner.hwnd());
    tracing::debug!(
        application_owned = request.immersive_native_context_menus,
        dpi,
        "selecting context-menu presentation path"
    );
    let high_contrast = high_contrast_active();
    let (selected, owned_replacement_point) =
        if should_use_owned_popup(request.immersive_native_context_menus, high_contrast) {
            crate::immersive_popup::present(
                popup.get(),
                owner.hwnd(),
                popup_point,
                dpi,
                matches!(
                    request.color_scheme,
                    explorer_model::ContextMenuColorScheme::Dark
                ),
            )
            .map(|presentation| (presentation.command, presentation.replacement_point))
            .unwrap_or_else(|reason| {
                tracing::warn!(?reason, "application-owned context menu fell back");
                let command = unsafe {
                    TrackPopupMenuEx(
                        popup.get(),
                        (TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_LEFTALIGN | TPM_TOPALIGN).0,
                        popup_point.x,
                        popup_point.y,
                        owner.hwnd(),
                        None,
                    )
                }
                .0;
                (command, None)
            })
        } else {
            let command = unsafe {
                TrackPopupMenuEx(
                    popup.get(),
                    (TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_LEFTALIGN | TPM_TOPALIGN).0,
                    popup_point.x,
                    popup_point.y,
                    owner.hwnd(),
                    None,
                )
            }
            .0;
            (command, None)
        };
    let replay_point = replay_hook
        .as_ref()
        .and_then(|_| MenuRightClickReplayHook::take())
        .or(owned_replacement_point);
    drop(replay_hook);
    if selected == 0 {
        // No command needs the queried HMENU after cancellation. Destroy it before replay so the
        // replacement gesture cannot overlap the old native menu's resources or modal lifetime.
        drop(popup);
        if let Some(app_owner) = app_owner {
            // Cancellation must restore the real app owner as well as command activation does.
            // The popup lives in the broker worker and can otherwise leave its hidden owner as
            // the foreground window after Escape or click-outside dismissal.
            let _ = unsafe { SetForegroundWindow(app_owner) };
        }
        let _ = state.transition(ContextMenuSessionState::Cancelled);
        let _ = state.release();
        if let Some(point) = replay_point {
            return Ok(ContextMenuOutcome::ReplayRequested {
                x: point.x,
                y: point.y,
            });
        }
        return Ok(ContextMenuOutcome::Cancelled);
    }
    if let Some(app_owner) = app_owner {
        // The broker's disposable owner forwards IContextMenu3 messages, while foreground focus
        // belongs to the real Explorer window again as soon as the popup terminates.
        let _ = unsafe { SetForegroundWindow(app_owner) };
    }
    let command_id = u32::try_from(selected).map_err(|_| {
        menu_error(
            "select context menu command",
            "內容功能表命令無效",
            "negative command identifier",
        )
    })?;
    let command_offset = command_id.checked_sub(COMMAND_FIRST).ok_or_else(|| {
        menu_error(
            "select context menu command",
            "內容功能表命令無效",
            "identifier below command range",
        )
    })?;
    let _ = state.transition(ContextMenuSessionState::Invoking);
    if let Some(menu_data) = apk_devices
        && command_id >= apk_first_id
    {
        let index = usize::try_from(command_id - apk_first_id).unwrap_or(usize::MAX);
        let _ = state.transition(ContextMenuSessionState::Finished);
        let _ = state.release();
        if matches!(menu_data, ApkMenuData::MissingTool) && index == 0 {
            return Ok(ContextMenuOutcome::DownloadAdb {
                target: request.target.clone(),
            });
        }
        if let ApkMenuData::Devices(devices) = menu_data {
            if let Some(device) = devices.get(index).filter(|device| device.is_installable()) {
                return Ok(ContextMenuOutcome::InstallApk {
                    serial: device.serial.clone(),
                    device_name: device.display_name().to_owned(),
                    target: request.target.clone(),
                });
            }
        }
    }
    if let Some(command) = host_command_at_offset(
        &menu,
        popup.get(),
        command_offset,
        matches!(request.target, ShellContextMenuTarget::Items { .. }),
    ) && host_command_applies_to_target(command, &request.target)
    {
        let _ = state.transition(ContextMenuSessionState::Finished);
        let _ = state.release();
        return Ok(ContextMenuOutcome::Delegated {
            command_offset,
            command,
            target: request.target.clone(),
        });
    }
    invoke(&menu, owner.hwnd(), command_offset)?;
    let _ = state.transition(ContextMenuSessionState::Finished);
    let _ = state.release();
    Ok(ContextMenuOutcome::Invoked { command_offset })
}

enum ApkMenuData {
    MissingTool,
    Devices(Vec<explorer_remote::AdbDevice>),
}

fn local_apk_devices(target: &ShellContextMenuTarget) -> Option<ApkMenuData> {
    let local = local_apk_path(target)?;
    let managed = PathBuf::from(std::env::var_os("LOCALAPPDATA")?)
        .join("RustGpuiExplorer")
        .join("tools")
        .join("adb");
    let resolver = explorer_remote::AdbToolResolver::new(managed);
    let cancellation = explorer_model::CancellationToken::new();
    let _ = local;
    match resolver.resolve(&cancellation) {
        Ok((tool, _)) => resolver
            .discover_devices(tool, &cancellation)
            .ok()
            .map(|snapshot| ApkMenuData::Devices(snapshot.devices)),
        Err(_) => Some(ApkMenuData::MissingTool),
    }
}

fn local_apk_path(target: &ShellContextMenuTarget) -> Option<PathBuf> {
    let ShellContextMenuTarget::Items { parent, items } = target else {
        return None;
    };
    if items.len() != 1 || !matches!(parent, explorer_model::LocationDescriptor::FileSystem(_)) {
        return None;
    }
    let explorer_model::LocationDescriptor::FileSystem(path) = &items[0].location else {
        return None;
    };
    let local = path.to_path_buf();
    if !local.is_file()
        || !local
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("apk"))
    {
        return None;
    }
    Some(local)
}

fn high_contrast_active() -> bool {
    let mut contrast = HIGHCONTRASTW {
        cbSize: size_of::<HIGHCONTRASTW>() as u32,
        ..Default::default()
    };
    unsafe {
        SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            contrast.cbSize,
            Some((&raw mut contrast).cast()),
            Default::default(),
        )
    }
    .is_ok()
        && contrast.dwFlags.contains(HCF_HIGHCONTRASTON)
}

const fn should_use_owned_popup(enabled: bool, high_contrast: bool) -> bool {
    enabled && !high_contrast
}

fn host_command_applies_to_target(
    command: ContextMenuHostCommand,
    target: &ShellContextMenuTarget,
) -> bool {
    // Host-side Open intentionally opens its focused row. A multi-selection Open can launch
    // every selected item, so keep that case in the native Shell invocation path.
    command != ContextMenuHostCommand::Open
        || matches!(target, ShellContextMenuTarget::Items { items, .. } if items.len() == 1)
}

fn host_command_at_offset(
    menu: &IContextMenu,
    popup: HMENU,
    command_offset: u32,
    item_menu: bool,
) -> Option<ContextMenuHostCommand> {
    let verb = canonical_verb_at_offset(menu, command_offset);
    if let Some(verb) = verb {
        tracing::debug!(command_offset, canonical_verb = %verb, "native context command selected");
        if let Some(command) = host_command_from_verb(&verb)
            && (item_menu || command != ContextMenuHostCommand::Properties)
        {
            return Some(command);
        }
    }
    let selected_id = COMMAND_FIRST.checked_add(command_offset)?;
    if command_label(popup, selected_id).is_some_and(|label| label == "貼上") {
        return Some(ContextMenuHostCommand::Paste);
    }
    if item_menu && command_label(popup, selected_id).is_some_and(|label| label == "加入書籤") {
        return Some(ContextMenuHostCommand::AddBookmark);
    }
    if item_menu && command_label(popup, selected_id).is_some_and(|label| is_share_label(&label)) {
        return Some(ContextMenuHostCommand::Share);
    }
    // Some legacy Shell implementations return no canonical verb for the in-box Properties
    // command. In an item menu Windows keeps Properties as the final actionable top-level item;
    // use that stable structural contract instead of invoking it in the disposable worker.
    if item_menu
        && (last_actionable_command_id(popup) == Some(selected_id)
            || command_label(popup, selected_id).is_some_and(|label| is_properties_label(&label)))
    {
        return Some(ContextMenuHostCommand::Properties);
    }
    None
}

fn canonical_verb_at_offset(menu: &IContextMenu, command_offset: u32) -> Option<String> {
    let mut buffer = [0_u16; 260];
    // SAFETY: the command offset was returned from this menu's reserved command range and the
    // writable buffer remains live for the complete call.
    let unicode = unsafe {
        menu.GetCommandString(
            usize::try_from(command_offset).ok()?,
            GCS_VERBW,
            None,
            PSTR(buffer.as_mut_ptr().cast()),
            u32::try_from(buffer.len()).ok()?,
        )
    };
    let unicode_length = buffer
        .iter()
        .position(|character| *character == 0)
        .unwrap_or(buffer.len());
    let mut verb = unicode
        .ok()
        .filter(|()| unicode_length > 0)
        .map(|()| String::from_utf16_lossy(&buffer[..unicode_length]));
    if verb.is_none() {
        // Older Shell handlers sometimes expose only the ANSI canonical verb. Explorer accepts
        // both forms, so fall back before treating the command as extension-owned.
        let mut ansi = [0_u8; 260];
        // SAFETY: the command offset belongs to this menu and the byte buffer is writable.
        if unsafe {
            menu.GetCommandString(
                usize::try_from(command_offset).ok()?,
                GCS_VERBA,
                None,
                PSTR(ansi.as_mut_ptr()),
                u32::try_from(ansi.len()).ok()?,
            )
        }
        .is_ok()
        {
            let length = ansi
                .iter()
                .position(|character| *character == 0)
                .unwrap_or(ansi.len());
            if length > 0 {
                verb = Some(String::from_utf8_lossy(&ansi[..length]).into_owned());
            }
        }
    }
    verb
}

fn is_share_label(label: &str) -> bool {
    let normalized = label.replace('&', "").to_lowercase();
    normalized == "share"
        || normalized.starts_with("share\t")
        || normalized.starts_with("共用(")
        || normalized.starts_with("分享(")
}

fn is_properties_label(label: &str) -> bool {
    let normalized = label.replace('&', "").to_lowercase();
    normalized.starts_with("內容")
        || normalized.starts_with("属性")
        || normalized.starts_with("プロパティ")
        || normalized.starts_with("속성")
        || normalized.starts_with("properties")
}

fn command_label(menu: HMENU, command_id: u32) -> Option<String> {
    let count = unsafe { GetMenuItemCount(Some(menu)) };
    for position in 0..count {
        // SAFETY: the position is within the live menu count.
        if unsafe { GetMenuItemID(menu, position) } != command_id {
            continue;
        }
        let mut buffer = [0_u16; 260];
        // SAFETY: the position is within the live menu count and the buffer is writable.
        let length = unsafe {
            GetMenuStringW(
                menu,
                u32::try_from(position).ok()?,
                Some(&mut buffer),
                MF_BYPOSITION,
            )
        };
        let length = usize::try_from(length).ok()?;
        return (length > 0).then(|| String::from_utf16_lossy(&buffer[..length]));
    }
    None
}

fn last_actionable_command_id(menu: HMENU) -> Option<u32> {
    // SAFETY: `menu` is the live popup owned by this context-menu session.
    let count = unsafe { GetMenuItemCount(Some(menu)) };
    for position in (0..count).rev() {
        // SAFETY: the position is within the menu count. `u32::MAX` denotes a separator/submenu.
        let id = unsafe { GetMenuItemID(menu, position) };
        if id != u32::MAX {
            return Some(id);
        }
    }
    None
}

fn host_command_from_verb(verb: &str) -> Option<ContextMenuHostCommand> {
    match verb.trim().to_ascii_lowercase().as_str() {
        "open" => Some(ContextMenuHostCommand::Open),
        "cut" => Some(ContextMenuHostCommand::Cut),
        "copy" => Some(ContextMenuHostCommand::Copy),
        "paste" => Some(ContextMenuHostCommand::Paste),
        "copyaspath" => Some(ContextMenuHostCommand::CopyPath),
        "link" => Some(ContextMenuHostCommand::CreateShortcut),
        "delete" => Some(ContextMenuHostCommand::Delete),
        "rename" => Some(ContextMenuHostCommand::Rename),
        "windows.share" | "windows.modernshare" | "share" => Some(ContextMenuHostCommand::Share),
        "pintostartscreen" => Some(ContextMenuHostCommand::PinToStart),
        "pintohome" | "pintohomefile" | "unpinfromhome" | "unpinfromhomefile" => {
            Some(ContextMenuHostCommand::ToggleQuickAccess)
        }
        "properties" => Some(ContextMenuHostCommand::Properties),
        _ => None,
    }
}

struct MenuRightClickReplayHook(HHOOK);

impl MenuRightClickReplayHook {
    fn install(app_owner: Option<HWND>, popup_owner: HWND) -> Option<Self> {
        MENU_RIGHT_CLICK.with(|capture| capture.set(MenuRightClickCapture::EMPTY));
        MENU_REPLAY_OWNER.with(|owner| owner.set(app_owner));
        MENU_POPUP_OWNER.with(|owner| owner.set(Some(popup_owner)));
        // SAFETY: the callback uses the system ABI and has static lifetime. The hook is scoped to
        // the native menu loop and always removed before this function returns.
        unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(menu_mouse_hook), None, 0) }
            .ok()
            .map(|hook| {
                ACTIVE_MENU_HOOKS.fetch_add(1, Ordering::AcqRel);
                Self(hook)
            })
    }

    fn take() -> Option<POINT> {
        MENU_RIGHT_CLICK.with(|capture| {
            let mut state = capture.get();
            let completed = state.take_completed();
            capture.set(state);
            completed.map(POINT::from)
        })
    }
}

impl Drop for MenuRightClickReplayHook {
    fn drop(&mut self) {
        // SAFETY: this value uniquely owns the successfully installed hook.
        let _ = unsafe { UnhookWindowsHookEx(self.0) };
        ACTIVE_MENU_HOOKS.fetch_sub(1, Ordering::AcqRel);
        MENU_RIGHT_CLICK.with(|capture| capture.set(MenuRightClickCapture::EMPTY));
        MENU_REPLAY_OWNER.with(|owner| owner.set(None));
        MENU_POPUP_OWNER.with(|owner| owner.set(None));
    }
}

unsafe extern "system" fn menu_mouse_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0
        && let Ok(message) = u32::try_from(wparam.0)
        && matches!(message, WM_RBUTTONDOWN | WM_RBUTTONUP)
    {
        // SAFETY: Windows supplies an MSLLHOOKSTRUCT pointer for WH_MOUSE_LL callbacks.
        let data = unsafe {
            &*(lparam.0 as *const windows::Win32::UI::WindowsAndMessaging::MSLLHOOKSTRUCT)
        };
        let app_owner = MENU_REPLAY_OWNER.with(Cell::get);
        let target = unsafe { WindowFromPoint(data.pt) };
        let belongs_to_owner =
            app_owner.is_some_and(|app_owner| window_belongs_to_app(target, app_owner));
        let action = MENU_RIGHT_CLICK.with(|capture| {
            let mut state = capture.get();
            let action = state.observe(
                message,
                ScreenPoint::from(data.pt),
                belongs_to_owner,
                data.dwExtraInfo == MENU_REPLAY_EXTRA_INFO,
            );
            capture.set(state);
            action
        });
        tracing::debug!(
            message,
            x = data.pt.x,
            y = data.pt.y,
            target = target.0 as usize,
            belongs_to_owner,
            tagged_replay = data.dwExtraInfo == MENU_REPLAY_EXTRA_INFO,
            ?action,
            "observed context-menu right-click gesture"
        );
        match action {
            MenuHookAction::Pass => {}
            MenuHookAction::Suppress => return LRESULT(1),
            MenuHookAction::SuppressAndPostCancel => {
                // Never tear down TrackPopupMenuEx synchronously from a low-level input callback.
                // Posting WM_CANCELMODE lets the popup owner's modal loop cancel itself without
                // re-entering the hook. The captured gesture is replayed only after teardown.
                if let Some(owner) = MENU_POPUP_OWNER.with(Cell::get) {
                    let _ =
                        unsafe { PostMessageW(Some(owner), WM_CANCELMODE, WPARAM(0), LPARAM(0)) };
                }
                return LRESULT(1);
            }
        }
    }
    // SAFETY: observation-only hook callbacks must continue the chain unchanged.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

fn window_belongs_to_app(target: HWND, app_owner: HWND) -> bool {
    if target.0.is_null() || app_owner.0.is_null() {
        return false;
    }
    let target_root = unsafe { GetAncestor(target, GA_ROOT) };
    let owner_root = unsafe { GetAncestor(app_owner, GA_ROOT) };
    if target_root == app_owner || target_root == owner_root {
        return true;
    }
    // GPUI can supply a content/renderer HWND while WindowFromPoint resolves a sibling native
    // surface. Explorer treats those windows as one application target. Process identity is the
    // final boundary: clicks in other applications are never swallowed or replayed.
    let mut target_process = 0;
    let mut owner_process = 0;
    unsafe {
        GetWindowThreadProcessId(target_root, Some(&raw mut target_process));
        GetWindowThreadProcessId(owner_root, Some(&raw mut owner_process));
    }
    target_process != 0 && target_process == owner_process
}

fn compress_selection_to_zip(target: &ShellContextMenuTarget) -> Result<PathBuf, ExplorerError> {
    let ShellContextMenuTarget::Items { parent, items } = target else {
        return Err(menu_error(
            "compress selection to zip",
            "必須先選取要壓縮的檔案或資料夾。",
            "compression requires an item selection",
        ));
    };
    let Some(parent) = parent.path() else {
        return Err(menu_error(
            "compress selection to zip",
            "目前位置不支援壓縮成 ZIP 檔案。",
            "compression parent is not a filesystem path",
        ));
    };
    let names = items
        .iter()
        .map(|item| {
            let path = item.location.path().ok_or_else(|| {
                menu_error(
                    "compress selection to zip",
                    "選取項目不支援壓縮成 ZIP 檔案。",
                    "compression item is not a filesystem path",
                )
            })?;
            if path.parent() != Some(parent) {
                return Err(menu_error(
                    "compress selection to zip",
                    "只能壓縮目前資料夾中的選取項目。",
                    "compression item is outside the selected parent",
                ));
            }
            path.file_name().map(PathBuf::from).ok_or_else(|| {
                menu_error(
                    "compress selection to zip",
                    "選取項目沒有可用的檔名。",
                    "compression item has no file name",
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    if names.is_empty() {
        return Err(menu_error(
            "compress selection to zip",
            "必須先選取要壓縮的檔案或資料夾。",
            "compression selection is empty",
        ));
    }
    let base = if names.len() == 1 {
        names[0]
            .file_stem()
            .and_then(|stem| stem.to_str())
            .filter(|stem| !stem.is_empty())
            .unwrap_or("Archive")
    } else {
        "Archive"
    };
    let destination = available_zip_path(parent, base);
    write_stored_zip(parent, &destination, &names)?;
    Ok(destination)
}

const ZIP_ENTRY_LIMIT: usize = 10_000;
const ZIP_TOTAL_INPUT_LIMIT: u64 = 4_u64 * 1024 * 1024 * 1024 - 1;

struct StoredZipEntry {
    source: Option<PathBuf>,
    name: Vec<u8>,
    crc32: u32,
    size: u32,
    local_offset: u32,
}

fn write_stored_zip(
    parent: &Path,
    destination: &Path,
    selected_names: &[PathBuf],
) -> Result<(), ExplorerError> {
    let result = write_stored_zip_contents(parent, destination, selected_names);
    if result.is_err() {
        let _ = std::fs::remove_file(destination);
    }
    result
}

fn write_stored_zip_contents(
    parent: &Path,
    destination: &Path,
    selected_names: &[PathBuf],
) -> Result<(), ExplorerError> {
    let mut sources = Vec::new();
    for name in selected_names {
        collect_zip_sources(parent, name, &mut sources)?;
    }
    if sources.is_empty() || sources.len() > ZIP_ENTRY_LIMIT {
        return Err(zip_error("ZIP entry count is outside the bounded contract"));
    }
    let mut output = File::create(destination).map_err(|error| zip_error(error.to_string()))?;
    let mut entries = Vec::with_capacity(sources.len());
    let mut total_input = 0_u64;
    for (source, archive_name) in sources {
        let name = archive_name.into_bytes();
        let name_length = u16::try_from(name.len())
            .map_err(|_| zip_error("ZIP entry name exceeds the classic ZIP limit"))?;
        let (crc32, size) = if let Some(source) = source.as_deref() {
            let metadata =
                std::fs::metadata(source).map_err(|error| zip_error(error.to_string()))?;
            total_input = total_input
                .checked_add(metadata.len())
                .ok_or_else(|| zip_error("ZIP input size overflow"))?;
            if total_input > ZIP_TOTAL_INPUT_LIMIT {
                return Err(zip_error("ZIP input exceeds the bounded 4 GiB contract"));
            }
            let size = u32::try_from(metadata.len())
                .map_err(|_| zip_error("ZIP64 input is not supported"))?;
            (crc32_file(source)?, size)
        } else {
            (0, 0)
        };
        let local_offset = u32::try_from(
            output
                .stream_position()
                .map_err(|error| zip_error(error.to_string()))?,
        )
        .map_err(|_| zip_error("ZIP64 offsets are not supported"))?;
        write_local_zip_header(&mut output, name_length, crc32, size)?;
        output
            .write_all(&name)
            .map_err(|error| zip_error(error.to_string()))?;
        if let Some(source) = source.as_deref() {
            let mut input = File::open(source).map_err(|error| zip_error(error.to_string()))?;
            let copied = std::io::copy(&mut input, &mut output)
                .map_err(|error| zip_error(error.to_string()))?;
            if copied != u64::from(size) {
                return Err(zip_error("source changed while creating ZIP"));
            }
        }
        entries.push(StoredZipEntry {
            source,
            name,
            crc32,
            size,
            local_offset,
        });
    }
    let central_offset = u32::try_from(
        output
            .stream_position()
            .map_err(|error| zip_error(error.to_string()))?,
    )
    .map_err(|_| zip_error("ZIP64 offsets are not supported"))?;
    for entry in &entries {
        write_central_zip_header(&mut output, entry)?;
        output
            .write_all(&entry.name)
            .map_err(|error| zip_error(error.to_string()))?;
    }
    let end = u32::try_from(
        output
            .stream_position()
            .map_err(|error| zip_error(error.to_string()))?,
    )
    .map_err(|_| zip_error("ZIP64 offsets are not supported"))?;
    let central_size = end
        .checked_sub(central_offset)
        .ok_or_else(|| zip_error("invalid ZIP central directory offset"))?;
    let count = u16::try_from(entries.len()).map_err(|_| zip_error("too many ZIP entries"))?;
    write_u32(&mut output, 0x0605_4b50)?;
    write_u16(&mut output, 0)?;
    write_u16(&mut output, 0)?;
    write_u16(&mut output, count)?;
    write_u16(&mut output, count)?;
    write_u32(&mut output, central_size)?;
    write_u32(&mut output, central_offset)?;
    write_u16(&mut output, 0)?;
    output
        .flush()
        .map_err(|error| zip_error(error.to_string()))?;
    output
        .sync_all()
        .map_err(|error| zip_error(error.to_string()))
}

fn collect_zip_sources(
    parent: &Path,
    relative: &Path,
    output: &mut Vec<(Option<PathBuf>, String)>,
) -> Result<(), ExplorerError> {
    if output.len() >= ZIP_ENTRY_LIMIT || relative.is_absolute() {
        return Err(zip_error("ZIP selection exceeds its bounded scope"));
    }
    let source = parent.join(relative);
    let metadata =
        std::fs::symlink_metadata(&source).map_err(|error| zip_error(error.to_string()))?;
    if metadata.file_type().is_symlink() {
        return Err(zip_error("reparse-point ZIP traversal is not supported"));
    }
    let mut archive_name = relative.to_string_lossy().replace('\\', "/");
    if archive_name.is_empty() || archive_name.split('/').any(|part| part == "..") {
        return Err(zip_error("invalid ZIP entry name"));
    }
    if metadata.is_dir() {
        archive_name.push('/');
        output.push((None, archive_name));
        let mut children = std::fs::read_dir(&source)
            .map_err(|error| zip_error(error.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| zip_error(error.to_string()))?;
        children.sort_by_key(std::fs::DirEntry::file_name);
        for child in children {
            collect_zip_sources(parent, &relative.join(child.file_name()), output)?;
        }
    } else if metadata.is_file() {
        output.push((Some(source), archive_name));
    } else {
        return Err(zip_error(
            "selected ZIP input is neither a file nor a directory",
        ));
    }
    Ok(())
}

fn crc32_file(path: &Path) -> Result<u32, ExplorerError> {
    let mut input = File::open(path).map_err(|error| zip_error(error.to_string()))?;
    let mut crc = u32::MAX;
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    loop {
        let count = input
            .read(&mut buffer)
            .map_err(|error| zip_error(error.to_string()))?;
        if count == 0 {
            break;
        }
        for byte in &buffer[..count] {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                crc = (crc >> 1) ^ (0xedb8_8320 & 0_u32.wrapping_sub(crc & 1));
            }
        }
    }
    Ok(!crc)
}

fn write_local_zip_header(
    output: &mut File,
    name_length: u16,
    crc32: u32,
    size: u32,
) -> Result<(), ExplorerError> {
    write_u32(output, 0x0403_4b50)?;
    write_u16(output, 20)?;
    write_u16(output, 0x0800)?;
    write_u16(output, 0)?;
    write_u16(output, 0)?;
    write_u16(output, 0)?;
    write_u32(output, crc32)?;
    write_u32(output, size)?;
    write_u32(output, size)?;
    write_u16(output, name_length)?;
    write_u16(output, 0)
}

fn write_central_zip_header(
    output: &mut File,
    entry: &StoredZipEntry,
) -> Result<(), ExplorerError> {
    let name_length = u16::try_from(entry.name.len()).map_err(|_| zip_error("ZIP name limit"))?;
    write_u32(output, 0x0201_4b50)?;
    write_u16(output, 20)?;
    write_u16(output, 20)?;
    write_u16(output, 0x0800)?;
    write_u16(output, 0)?;
    write_u16(output, 0)?;
    write_u16(output, 0)?;
    write_u32(output, entry.crc32)?;
    write_u32(output, entry.size)?;
    write_u32(output, entry.size)?;
    write_u16(output, name_length)?;
    write_u16(output, 0)?;
    write_u16(output, 0)?;
    write_u16(output, 0)?;
    write_u16(output, 0)?;
    write_u32(output, u32::from(entry.source.is_none()) * 0x10)?;
    write_u32(output, entry.local_offset)
}

fn write_u16(output: &mut File, value: u16) -> Result<(), ExplorerError> {
    output
        .write_all(&value.to_le_bytes())
        .map_err(|error| zip_error(error.to_string()))
}

fn write_u32(output: &mut File, value: u32) -> Result<(), ExplorerError> {
    output
        .write_all(&value.to_le_bytes())
        .map_err(|error| zip_error(error.to_string()))
}

fn zip_error(detail: impl Into<String>) -> ExplorerError {
    menu_error("compress selection to zip", "壓縮成 ZIP 檔案失敗。", detail)
}

fn available_zip_path(parent: &Path, base: &str) -> PathBuf {
    let initial = parent.join(format!("{base}.zip"));
    if !initial.exists() {
        return initial;
    }
    (2_u32..=10_000)
        .map(|index| parent.join(format!("{base} ({index}).zip")))
        .find(|path| !path.exists())
        .unwrap_or_else(|| parent.join(format!("{base}-{}.zip", std::process::id())))
}

#[cfg(test)]
pub(crate) fn query_command_count(target: &ShellContextMenuTarget) -> Result<i32, ExplorerError> {
    query_command_count_with_profile(target, ContextMenuInvocationProfile::Explorer)
}

fn query_command_count_with_profile(
    target: &ShellContextMenuTarget,
    profile: ContextMenuInvocationProfile,
) -> Result<i32, ExplorerError> {
    let mut owner_state = MenuOwnerState { menu3: None };
    let owner = OwnerWindow::create(&raw mut owner_state)?;
    let menu = resolve_menu(target, owner.hwnd())?;
    let popup = OwnedMenu::create()?;
    query_menu(
        &menu,
        popup.get(),
        matches!(target, ShellContextMenuTarget::Items { .. }),
        profile,
    )?;
    // SAFETY: popup owns a valid live HMENU.
    let count = unsafe { GetMenuItemCount(Some(popup.get())) };
    (count >= 0).then_some(count).ok_or_else(|| {
        menu_error(
            "count context menu commands",
            "無法讀取內容功能表",
            "GetMenuItemCount failed",
        )
    })
}

/// Executes one complete context-menu session inside a disposable broker worker.
/// The returned value contains no menu, HWND, PIDL, or COM interface.
///
/// # Errors
/// Returns a typed Shell error when OLE initialization, menu query, presentation, or invocation
/// fails.
pub fn execute_in_worker(
    request: &ContextMenuRequest,
) -> Result<ContextMenuOutcome, ExplorerError> {
    // SAFETY: this function is called on a dedicated disposable worker thread and balances OLE.
    unsafe { OleInitialize(None) }
        .map_err(|error| native_menu_error("initialize broker context menu", &error))?;
    let _apartment = OleApartment;
    show(request)
}

/// Queries a menu in a disposable worker without presenting or invoking it.
///
/// # Errors
/// Returns a typed Shell error when OLE initialization or menu construction fails.
pub fn query_in_worker(target: &ShellContextMenuTarget) -> Result<i32, ExplorerError> {
    query_in_worker_with_profile(target, ContextMenuInvocationProfile::Explorer)
}

/// Queries a menu using one explicit Explorer invocation profile without presenting it.
///
/// # Errors
/// Returns a typed Shell error when OLE initialization or menu construction fails.
pub fn query_in_worker_with_profile(
    target: &ShellContextMenuTarget,
    profile: ContextMenuInvocationProfile,
) -> Result<i32, ExplorerError> {
    // SAFETY: this function is called on a dedicated disposable worker thread and balances OLE.
    unsafe { OleInitialize(None) }
        .map_err(|error| native_menu_error("initialize broker context menu", &error))?;
    let _apartment = OleApartment;
    query_command_count_with_profile(target, profile)
}

/// Queries a bounded command/label fingerprint snapshot for same-host broker differentials.
///
/// # Errors
/// Returns a typed Shell error when OLE initialization or menu construction fails.
pub fn query_snapshot_in_worker_with_profile(
    target: &ShellContextMenuTarget,
    profile: ContextMenuInvocationProfile,
) -> Result<ContextMenuQuerySnapshot, ExplorerError> {
    unsafe { OleInitialize(None) }
        .map_err(|error| native_menu_error("initialize broker context menu snapshot", &error))?;
    let _apartment = OleApartment;
    let mut owner_state = MenuOwnerState { menu3: None };
    let owner = OwnerWindow::create(&raw mut owner_state)?;
    let menu = resolve_menu(target, owner.hwnd())?;
    owner_state.menu3 = menu.cast::<IContextMenu3>().ok();
    let popup = OwnedMenu::create()?;
    query_menu(
        &menu,
        popup.get(),
        matches!(target, ShellContextMenuTarget::Items { .. }),
        profile,
    )?;
    let command_count = unsafe { GetMenuItemCount(Some(popup.get())) };
    if command_count < 0 {
        return Err(menu_error(
            "snapshot context menu commands",
            "The context menu could not be inspected.",
            "GetMenuItemCount failed",
        ));
    }
    let mut label_fingerprints = Vec::new();
    collect_menu_label_fingerprints(popup.get(), owner.hwnd(), 0, &mut label_fingerprints);
    owner_state.menu3 = None;
    Ok(ContextMenuQuerySnapshot {
        command_count,
        label_fingerprints,
    })
}

fn collect_menu_label_fingerprints(menu: HMENU, owner: HWND, depth: usize, output: &mut Vec<u64>) {
    const MAXIMUM_DEPTH: usize = 8;
    const MAXIMUM_LABELS: usize = 1_024;
    if depth > MAXIMUM_DEPTH || output.len() >= MAXIMUM_LABELS {
        return;
    }
    let count = unsafe { GetMenuItemCount(Some(menu)) };
    if count <= 0 {
        return;
    }
    for position in 0..count {
        if output.len() >= MAXIMUM_LABELS {
            return;
        }
        let mut buffer = [0_u16; 512];
        let Ok(position) = u32::try_from(position) else {
            return;
        };
        let length = unsafe { GetMenuStringW(menu, position, Some(&mut buffer), MF_BYPOSITION) };
        if let Ok(length) = usize::try_from(length)
            && length > 0
        {
            output.push(fingerprint_menu_label(&buffer[..length]));
        }
        let child = unsafe { GetSubMenu(menu, i32::try_from(position).unwrap_or(i32::MAX)) };
        if !child.0.is_null() {
            // Lazy third-party cascades populate only after the owner forwards WM_INITMENUPOPUP to
            // IContextMenu3. Differential snapshots must exercise the same path as the visible menu.
            unsafe {
                windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                    owner,
                    WM_INITMENUPOPUP,
                    Some(WPARAM(child.0 as usize)),
                    Some(LPARAM(isize::try_from(position).unwrap_or_default())),
                )
            };
            collect_menu_label_fingerprints(child, owner, depth + 1, output);
        }
    }
}

fn fingerprint_menu_label(label: &[u16]) -> u64 {
    label.iter().fold(0xcbf2_9ce4_8422_2325, |hash, unit| {
        unit.to_le_bytes().into_iter().fold(hash, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
        })
    })
}

fn resolve_menu(
    target: &ShellContextMenuTarget,
    owner: HWND,
) -> Result<IContextMenu, ExplorerError> {
    let (parent, items) = match target {
        ShellContextMenuTarget::Background { parent } => (parent, None),
        ShellContextMenuTarget::Items { parent, items } => (parent, Some(items.as_slice())),
    };
    let parent_pidl = crate::navigation::location_absolute_pidl(parent)?;
    // SAFETY: parent PIDL remains live; returned interface is owned by this STA.
    let folder: IShellFolder = unsafe {
        SHBindToObject(
            None::<&IShellFolder>,
            parent_pidl.as_ptr(),
            None::<&IBindCtx>,
        )
    }
    .map_err(|error| native_menu_error("bind context menu parent", &error))?;
    let Some(items) = items else {
        // SAFETY: owner is a live STA window and folder interface stays apartment-confined.
        return unsafe { folder.CreateViewObject(owner) }
            .map_err(|error| native_menu_error("create background context menu", &error));
    };
    if items.is_empty() {
        return Err(menu_error(
            "resolve item context menu",
            "沒有可顯示內容功能表的項目",
            "empty item selection",
        ));
    }
    let children = items
        .iter()
        .map(|item| crate::navigation::location_absolute_pidl(&item.location))
        .collect::<Result<Vec<_>, _>>()?;
    if children.iter().any(|child| {
        // SAFETY: both PIDLs are complete, live, and owned for this synchronous relationship test.
        !unsafe { ILIsParent(parent_pidl.as_ptr(), child.as_ptr(), true) }.as_bool()
    }) {
        return Err(menu_error(
            "resolve item context menu",
            "選取項目不屬於同一個位置",
            "item is not an immediate child of the context menu parent",
        ));
    }
    // SAFETY: child PIDLs stay live; borrowed tails remain valid through GetUIObjectOf.
    let relative = children
        .iter()
        .map(|pidl| unsafe { ILFindLastID(pidl.as_ptr()) }.cast_const())
        .collect::<Vec<_>>();
    // SAFETY: owner, folder and all relative PIDLs remain live through the call.
    unsafe { folder.GetUIObjectOf(owner, &relative, None) }
        .map_err(|error| native_menu_error("get item context menu", &error))
}

fn query_menu(
    menu: &IContextMenu,
    popup: HMENU,
    item_menu: bool,
    profile: ContextMenuInvocationProfile,
) -> Result<usize, ExplorerError> {
    // Explorer supplies context and synchronous-cascade hints so built-in and third-party
    // handlers can expose their complete target-appropriate native command set. Extended verbs
    // remain scoped to the one Shift/programmatic session.
    let mut flags = CMF_NORMAL | CMF_EXPLORE | CMF_SYNCCASCADEMENU;
    if item_menu {
        flags |= CMF_ITEMMENU | CMF_CANRENAME;
    }
    if profile.extended_verbs() {
        flags |= CMF_EXTENDEDVERBS;
    }
    // SAFETY: menu owns HMENU; command range is reserved for this one session.
    let result = unsafe { menu.QueryContextMenu(popup, 0, COMMAND_FIRST, COMMAND_LAST, flags) };
    result
        .ok()
        .map_err(|error| native_menu_error("query context menu", &error))?;
    usize::try_from(result.0 & 0xffff).map_err(|_| {
        menu_error(
            "query context menu",
            "內容功能表命令無效",
            "command count does not fit usize",
        )
    })
}

fn canonical_verb_offset(
    menu: &IContextMenu,
    command_count: usize,
    requested: &str,
) -> Result<u32, ExplorerError> {
    let requested = requested.trim();
    let buffer_length = u32::try_from(260).map_err(|_| {
        menu_error(
            "resolve canonical context verb",
            "The context-menu command could not be read.",
            "verb buffer length does not fit u32",
        )
    })?;
    for offset in 0..command_count {
        let mut buffer = [0_u16; 260];
        // SAFETY: the buffer is writable for its declared character count and the command offset
        // came from this menu's QueryContextMenu result.
        if unsafe {
            menu.GetCommandString(
                offset,
                GCS_VERBW,
                None,
                PSTR(buffer.as_mut_ptr().cast()),
                buffer_length,
            )
        }
        .is_err()
        {
            continue;
        }
        let length = buffer
            .iter()
            .position(|character| *character == 0)
            .unwrap_or(buffer.len());
        let verb = String::from_utf16_lossy(&buffer[..length]);
        if verb.eq_ignore_ascii_case(requested)
            || requested.eq_ignore_ascii_case("Windows.Share")
                && (verb.eq_ignore_ascii_case("share")
                    || verb.eq_ignore_ascii_case("Windows.ModernShare"))
        {
            return u32::try_from(offset).map_err(|_| {
                menu_error(
                    "resolve canonical context verb",
                    "內容功能表命令無效",
                    "canonical verb offset does not fit u32",
                )
            });
        }
    }
    let user_message = if requested.eq_ignore_ascii_case("Windows.Share")
        || requested.eq_ignore_ascii_case("share")
    {
        "這些項目目前無法分享。"
    } else if requested.eq_ignore_ascii_case("properties") {
        "目前無法開啟這些項目的內容。"
    } else {
        "目前無法執行要求的檔案操作。"
    };
    Err(menu_error(
        "resolve canonical context verb",
        user_message,
        "requested canonical verb was not exposed by the Shell context menu",
    ))
}

fn invoke(menu: &IContextMenu, owner: HWND, offset: u32) -> Result<(), ExplorerError> {
    let structure_size = u32::try_from(size_of::<CMINVOKECOMMANDINFO>()).map_err(|_| {
        menu_error(
            "invoke context menu command",
            "The context-menu command could not be started.",
            "CMINVOKECOMMANDINFO size does not fit u32",
        )
    })?;
    let verb_offset = usize::try_from(offset).map_err(|_| {
        menu_error(
            "invoke context menu command",
            "The context-menu command could not be started.",
            "command offset does not fit usize",
        )
    })?;
    let info = CMINVOKECOMMANDINFO {
        cbSize: structure_size,
        hwnd: owner,
        lpVerb: PCSTR(verb_offset as *const u8),
        nShow: SW_SHOWNORMAL.0,
        ..CMINVOKECOMMANDINFO::default()
    };
    // SAFETY: command offset came from this menu's reserved range and info remains live.
    unsafe { menu.InvokeCommand(&raw const info) }
        .map_err(|error| native_menu_error("invoke context menu command", &error))
}

fn invoke_host_owned(
    menu: &IContextMenu,
    owner: HWND,
    offset: u32,
    point: POINT,
    center_properties: bool,
    placement_owner: Option<HWND>,
) -> Result<(), ExplorerError> {
    let structure_size = u32::try_from(size_of::<CMINVOKECOMMANDINFOEX>()).map_err(|_| {
        menu_error(
            "invoke host context command",
            "The context-menu command could not be started.",
            "CMINVOKECOMMANDINFOEX size does not fit u32",
        )
    })?;
    let verb_offset = usize::try_from(offset).map_err(|_| {
        menu_error(
            "invoke host context command",
            "The context-menu command could not be started.",
            "command offset does not fit usize",
        )
    })?;
    let info = CMINVOKECOMMANDINFOEX {
        cbSize: structure_size,
        fMask: CMIC_MASK_UNICODE | CMIC_MASK_PTINVOKE,
        hwnd: owner,
        lpVerb: PCSTR(verb_offset as *const u8),
        lpVerbW: PCWSTR(verb_offset as *const u16),
        nShow: SW_SHOWNORMAL.0,
        ptInvoke: point,
        ..CMINVOKECOMMANDINFOEX::default()
    };
    let placement_hook = center_properties
        .then(|| PropertiesCenteringHook::install(placement_owner, point))
        .flatten();
    // SAFETY: the offset came from this exact IContextMenu instance. The EX structure begins
    // with CMINVOKECOMMANDINFO, remains live for the call, and uses the validated app HWND.
    let result = unsafe { menu.InvokeCommand((&raw const info).cast::<CMINVOKECOMMANDINFO>()) };
    if result.is_ok() {
        if let Some(hook) = placement_hook {
            hook.detach(PROPERTIES_PLACEMENT_TIMEOUT);
        }
    }
    result.map_err(|error| native_menu_error("invoke host context command", &error))
}

struct PropertiesCenteringHook(HWINEVENTHOOK);

impl PropertiesCenteringHook {
    fn install(owner: Option<HWND>, point: POINT) -> Option<Self> {
        let Some((anchor, work_area)) = monitor_work_area(owner, point) else {
            tracing::warn!("Properties centering could not resolve an owner or monitor work area");
            return None;
        };
        if let Ok(mut state) = PROPERTIES_PLACEMENT.lock() {
            *state = Some(PropertiesPlacementState {
                anchor,
                work_area,
                claimed: false,
                completed: false,
                hook_value: 0,
            });
        } else {
            tracing::warn!("Properties centering state lock was unavailable");
            return None;
        }
        // Shell handlers may create the property sheet on a helper thread. A process-scoped
        // in-context WinEvent hook observes that native window without polling or another broker.
        let module = unsafe { GetModuleHandleW(None) }.ok();
        let hook = unsafe {
            SetWinEventHook(
                EVENT_OBJECT_SHOW,
                EVENT_OBJECT_SHOW,
                module,
                Some(properties_centering_event),
                GetCurrentProcessId(),
                0,
                WINEVENT_INCONTEXT,
            )
        };
        if hook.0.is_null() {
            if let Ok(mut state) = PROPERTIES_PLACEMENT.lock() {
                *state = None;
            }
            tracing::warn!("Properties centering WinEvent hook installation failed");
            return None;
        }
        ACTIVE_MENU_HOOKS.fetch_add(1, Ordering::AcqRel);
        let hook_value = hook.0 as usize;
        if let Ok(mut state) = PROPERTIES_PLACEMENT.lock()
            && let Some(placement) = state.as_mut()
        {
            placement.hook_value = hook_value;
        } else {
            release_properties_hook(hook_value);
            return None;
        }
        Some(Self(hook))
    }

    fn detach(self, timeout: Duration) {
        let hook_value = self.0.0 as usize;
        let worker = std::thread::Builder::new()
            .name("properties-centering".to_owned())
            .spawn(move || {
                let deadline = Instant::now() + timeout;
                loop {
                    // SAFETY: this query has no pointer inputs and returns this process identifier.
                    if let Some(hwnd) = visible_properties_dialog(unsafe { GetCurrentProcessId() })
                    {
                        try_position_properties_dialog(hwnd);
                    }
                    let completed = PROPERTIES_PLACEMENT
                        .lock()
                        .ok()
                        .and_then(|state| state.as_ref().map(|placement| placement.completed))
                        .unwrap_or(true);
                    if completed {
                        clear_properties_placement(Some(hook_value));
                        return;
                    }
                    if Instant::now() >= deadline {
                        tracing::warn!(
                            timeout_ms = timeout.as_millis(),
                            "Properties centering event did not arrive within the bound"
                        );
                        clear_properties_placement(Some(hook_value));
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(2));
                }
            });
        if worker.is_ok() {
            std::mem::forget(self);
        }
    }
}

impl Drop for PropertiesCenteringHook {
    fn drop(&mut self) {
        clear_properties_placement(Some(self.0.0 as usize));
    }
}

fn release_properties_hook(hook_value: usize) {
    if hook_value == 0 {
        return;
    }
    // SAFETY: a non-zero value comes from one successful SetWinEventHook call.
    let _ = unsafe { UnhookWinEvent(HWINEVENTHOOK(hook_value as *mut c_void)) };
    ACTIVE_MENU_HOOKS.fetch_sub(1, Ordering::AcqRel);
}

fn clear_properties_placement(expected_hook: Option<usize>) {
    let hook_value = if let Ok(mut state) = PROPERTIES_PLACEMENT.lock() {
        let matches = expected_hook.is_none_or(|expected| {
            state.as_ref().is_some_and(|placement| {
                placement.hook_value == expected || placement.hook_value == 0
            })
        });
        if !matches {
            return;
        }
        state
            .take()
            .map(|placement| placement.hook_value)
            .unwrap_or_default()
    } else {
        0
    };
    release_properties_hook(hook_value);
}

fn is_properties_dialog(hwnd: HWND) -> bool {
    if hwnd.0.is_null()
        || !unsafe { IsWindow(Some(hwnd)).as_bool() }
        || unsafe { GetAncestor(hwnd, GA_ROOT) } != hwnd
    {
        return false;
    }
    let mut class_name = [0_u16; 32];
    // SAFETY: `hwnd` is live and the UTF-16 buffer is writable.
    let length = unsafe { GetClassNameW(hwnd, &mut class_name) };
    usize::try_from(length)
        .ok()
        .is_some_and(|length| &class_name[..length] == WINDOWS_DIALOG_CLASS)
}

fn try_position_properties_dialog(hwnd: HWND) {
    if !is_properties_dialog(hwnd) {
        return;
    }
    let Some(placement) = claim_properties_placement() else {
        return;
    };
    let mut window = RECT::default();
    // SAFETY: `hwnd` is live and `window` is writable for the call.
    let result = unsafe { GetWindowRect(hwnd, &raw mut window) }
        .ok()
        .and_then(|()| centered_window_position(placement.anchor, placement.work_area, window))
        .and_then(|position| {
            // SAFETY: positioning preserves the Shell-owned size, Z-order and activation.
            unsafe {
                SetWindowPos(
                    hwnd,
                    None,
                    position.x,
                    position.y,
                    0,
                    0,
                    SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                )
            }
            .ok()
            .map(|()| position)
        });
    if result.is_none() {
        tracing::warn!("Properties centering could not position the native dialog");
    }
    finish_properties_placement(result.is_some());
}

struct VisiblePropertiesDialogQuery {
    process_id: u32,
    hwnd: HWND,
}

unsafe extern "system" fn find_visible_properties_dialog(hwnd: HWND, parameter: LPARAM) -> BOOL {
    // SAFETY: caller passes one live query for the synchronous EnumWindows call.
    let query = unsafe { &mut *(parameter.0 as *mut VisiblePropertiesDialogQuery) };
    let mut process_id = 0_u32;
    // SAFETY: process ID storage is writable and HWND is supplied by EnumWindows.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&raw mut process_id)) };
    if process_id == query.process_id
        && unsafe { IsWindowVisible(hwnd).as_bool() }
        && is_properties_dialog(hwnd)
    {
        query.hwnd = hwnd;
        return false.into();
    }
    true.into()
}

fn visible_properties_dialog(process_id: u32) -> Option<HWND> {
    let mut query = VisiblePropertiesDialogQuery {
        process_id,
        hwnd: HWND::default(),
    };
    // SAFETY: callback and query pointer remain live for the synchronous enumeration.
    let _ = unsafe {
        EnumWindows(
            Some(find_visible_properties_dialog),
            LPARAM((&raw mut query).cast::<c_void>() as isize),
        )
    };
    (!query.hwnd.0.is_null()).then_some(query.hwnd)
}

unsafe extern "system" fn properties_centering_event(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    object_id: i32,
    child_id: i32,
    _event_thread: u32,
    _event_time: u32,
) {
    if event == EVENT_OBJECT_SHOW && object_id == OBJID_WINDOW.0 && child_id == 0 {
        try_position_properties_dialog(hwnd);
    }
}

struct OwnedMenu(HMENU);
impl OwnedMenu {
    fn create() -> Result<Self, ExplorerError> {
        // SAFETY: returns unique HMENU ownership on success.
        unsafe { CreatePopupMenu() }
            .map(|menu| {
                ACTIVE_MENUS.fetch_add(1, Ordering::AcqRel);
                Self(menu)
            })
            .map_err(|error| native_menu_error("create popup menu", &error))
    }
    const fn get(&self) -> HMENU {
        self.0
    }
}
impl Drop for OwnedMenu {
    fn drop(&mut self) {
        // SAFETY: this wrapper uniquely owns the menu and destroys it once after TrackPopupMenu.
        let _ = unsafe { DestroyMenu(self.0) };
        ACTIVE_MENUS.fetch_sub(1, Ordering::AcqRel);
    }
}

struct MenuOwnerState {
    menu3: Option<IContextMenu3>,
}
struct OwnerWindow(HWND);
impl OwnerWindow {
    fn create(state: *mut MenuOwnerState) -> Result<Self, ExplorerError> {
        Self::create_owned(state, None)
    }

    fn create_owned(
        state: *mut MenuOwnerState,
        app_owner: Option<HWND>,
    ) -> Result<Self, ExplorerError> {
        // SAFETY: null module name returns the current executable module.
        let module = unsafe { GetModuleHandleW(None) }
            .map_err(|error| native_menu_error("get context menu module", &error))?;
        let class = WNDCLASSW {
            hInstance: module.into(),
            lpszClassName: w!("RustGpuiExplorerContextMenuOwner"),
            lpfnWndProc: Some(menu_window_proc),
            ..WNDCLASSW::default()
        };
        // SAFETY: class structure and static class name are valid for process lifetime; already-registered is harmless.
        let _ = unsafe { RegisterClassW(&raw const class) };
        // SAFETY: class is registered and state remains live until after DestroyWindow.
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                class.lpszClassName,
                w!(""),
                WS_POPUP,
                0,
                0,
                1,
                1,
                app_owner,
                None,
                Some(module.into()),
                Some(state.cast()),
            )
        }
        .map_err(|error| native_menu_error("create context menu owner", &error))?;
        ACTIVE_OWNER_WINDOWS.fetch_add(1, Ordering::AcqRel);
        Ok(Self(hwnd))
    }
    const fn hwnd(&self) -> HWND {
        self.0
    }
}

fn validated_owner_window(raw: u64) -> Option<HWND> {
    let value = usize::try_from(raw).ok().filter(|value| *value != 0)?;
    let hwnd = HWND(value as *mut c_void);
    // SAFETY: IsWindow only validates the value; no ownership is transferred across processes.
    if !unsafe { IsWindow(Some(hwnd)) }.as_bool() {
        return None;
    }
    let root = unsafe { GetAncestor(hwnd, GA_ROOT) };
    if root.0.is_null() {
        Some(hwnd)
    } else {
        Some(root)
    }
}
impl Drop for OwnerWindow {
    fn drop(&mut self) {
        // SAFETY: wrapper owns one live hidden window on its creating STA.
        let _ = unsafe { DestroyWindow(self.0) };
        ACTIVE_OWNER_WINDOWS.fetch_sub(1, Ordering::AcqRel);
    }
}

unsafe extern "system" fn menu_window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        // SAFETY: WM_NCCREATE lparam points to CREATESTRUCTW for the duration of this call.
        let create = unsafe { &*(lparam.0 as *const CREATESTRUCTW) };
        // SAFETY: stores the caller-owned state pointer without taking ownership.
        unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize) };
    }
    // SAFETY: value was either zero or the live MenuOwnerState pointer installed above.
    let state = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut MenuOwnerState;
    if !state.is_null()
        && matches!(
            message,
            WM_INITMENUPOPUP | WM_DRAWITEM | WM_MEASUREITEM | WM_MENUCHAR
        )
    {
        FORWARDED_MENU_MESSAGES.fetch_add(1, Ordering::AcqRel);
        let mut result = LRESULT::default();
        // SAFETY: state remains live until after DestroyWindow and interface stays on this STA.
        if let Some(menu3) = unsafe { &*state }.menu3.as_ref()
            && unsafe { menu3.HandleMenuMsg2(message, wparam, lparam, Some(&raw mut result)) }
                .is_ok()
        {
            return result;
        }
    }
    // SAFETY: default procedure receives the original unmodified message values.
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

fn native_menu_error(operation: &'static str, error: &windows::core::Error) -> ExplorerError {
    menu_error(
        operation,
        "Windows Shell 內容功能表失敗",
        format!("HRESULT={:#010x}: {error}", error.code().0),
    )
}
fn menu_error(
    operation: &'static str,
    user: &'static str,
    detail: impl Into<String>,
) -> ExplorerError {
    ExplorerError::new(
        ExplorerErrorKind::Availability,
        operation,
        true,
        user,
        detail,
    )
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::inline_always,
        clippy::ref_as_ptr,
        reason = "windows-rs implement macro generates the controlled COM fixture glue"
    )]

    use std::{
        ffi::c_void,
        sync::{
            Arc, Mutex,
            atomic::{AtomicIsize, Ordering as AtomicOrdering},
        },
        time::Duration,
    };

    use explorer_model::{
        ExplorerEvent, Generation, ItemDescriptor, LocationDescriptor, RequestContext, ShellItemId,
        TabId,
    };
    use windows::{
        Win32::{
            Foundation::{LPARAM, LRESULT, WPARAM},
            System::{
                Ole::{OleInitialize, OleUninitialize},
                Threading::GetCurrentThreadId,
            },
            UI::{
                Shell::{IContextMenu_Impl, IContextMenu2_Impl, IContextMenu3_Impl},
                WindowsAndMessaging::{
                    AppendMenuW, FindWindowW, GetMenuItemID, GetMenuItemInfoW, MENUITEMINFOW,
                    MF_OWNERDRAW, MIIM_BITMAP, MIIM_ID, MIIM_SUBMENU, PostThreadMessageW,
                    SendMessageW, WM_CLOSE, WM_KEYDOWN, WM_KEYUP,
                },
            },
        },
        core::{HRESULT, PCWSTR, PSTR, Result as WinResult, implement},
    };

    use super::*;

    #[test]
    fn local_qq_apk_is_eligible_only_as_a_single_filesystem_item() {
        let fixture = explorer_test_support::OwnedTempFixture::new().expect("APK fixture");
        let apk = fixture
            .create_file("qq9.3.55.apk", b"controlled APK eligibility fixture")
            .expect("APK file");
        let item = ItemDescriptor {
            id: ShellItemId::from_provider_bytes(b"qq-apk".to_vec()).expect("item id"),
            location: LocationDescriptor::file_system(&apk),
        };
        let target = ShellContextMenuTarget::Items {
            parent: LocationDescriptor::file_system(fixture.root()),
            items: vec![item.clone()],
        };
        assert_eq!(local_apk_path(&target), Some(apk));
        let multiple = ShellContextMenuTarget::Items {
            parent: LocationDescriptor::file_system(fixture.root()),
            items: vec![item.clone(), item],
        };
        assert!(local_apk_path(&multiple).is_none());
    }

    #[test]
    fn owned_popup_policy_falls_back_for_disabled_or_high_contrast_sessions() {
        assert!(should_use_owned_popup(true, false));
        assert!(!should_use_owned_popup(false, false));
        assert!(!should_use_owned_popup(true, true));
        assert!(!should_use_owned_popup(false, true));
    }

    fn rect(left: i32, top: i32, right: i32, bottom: i32) -> RECT {
        RECT {
            left,
            top,
            right,
            bottom,
        }
    }

    #[test]
    fn properties_position_centers_on_owner_and_clamps_every_work_area_edge() {
        let work = rect(0, 0, 1_920, 1_080);
        let dialog = rect(0, 0, 400, 300);
        assert_eq!(
            centered_window_position(rect(100, 100, 900, 700), work, dialog),
            Some(POINT { x: 300, y: 250 })
        );
        assert_eq!(
            centered_window_position(rect(-600, 100, -200, 700), work, dialog),
            Some(POINT { x: 0, y: 250 })
        );
        assert_eq!(
            centered_window_position(rect(2_000, 100, 2_400, 700), work, dialog),
            Some(POINT { x: 1_520, y: 250 })
        );
        assert_eq!(
            centered_window_position(rect(100, -500, 900, -100), work, dialog),
            Some(POINT { x: 300, y: 0 })
        );
        assert_eq!(
            centered_window_position(rect(100, 1_200, 900, 1_600), work, dialog),
            Some(POINT { x: 300, y: 780 })
        );
    }

    #[test]
    fn properties_position_handles_oversized_invalid_and_fallback_rectangles() {
        let work = rect(20, 40, 820, 640);
        assert_eq!(
            centered_window_position(rect(120, 140, 720, 540), work, rect(0, 0, 1_000, 800)),
            Some(POINT { x: 20, y: 40 })
        );
        assert_eq!(placement_anchor(Some(rect(0, 0, 0, 100)), work), Some(work));
        assert_eq!(placement_anchor(None, work), Some(work));
        assert_eq!(placement_anchor(None, rect(0, 0, 0, 0)), None);
        assert_eq!(
            centered_window_position(rect(0, 0, 0, 100), work, rect(0, 0, 100, 100)),
            None
        );
    }

    #[test]
    fn properties_hook_state_is_single_attempt_and_command_scoped() {
        assert!(should_center_properties("properties"));
        assert!(should_center_properties("Properties"));
        assert!(!should_center_properties("open"));
        assert!(!should_center_properties("PinToStartScreen"));

        let state = PropertiesPlacementState {
            anchor: rect(100, 100, 900, 700),
            work_area: rect(0, 0, 1_920, 1_080),
            claimed: false,
            completed: false,
            hook_value: 0,
        };
        *PROPERTIES_PLACEMENT.lock().expect("placement state lock") = Some(state);
        assert!(claim_properties_placement().is_some());
        assert!(claim_properties_placement().is_none());
        finish_properties_placement(false);
        assert!(claim_properties_placement().is_some());
        finish_properties_placement(true);
        assert!(claim_properties_placement().is_none());
        *PROPERTIES_PLACEMENT.lock().expect("placement state lock") = None;
        assert!(claim_properties_placement().is_none());
    }

    #[test]
    fn replacement_capture_requires_a_matched_untagged_owner_gesture() {
        let down = ScreenPoint { x: 40, y: 60 };
        let up = ScreenPoint { x: 42, y: 62 };
        let mut capture = MenuRightClickCapture::EMPTY;

        assert_eq!(
            capture.observe(WM_RBUTTONDOWN, down, true, false),
            MenuHookAction::Suppress
        );
        assert_eq!(
            capture.observe(WM_RBUTTONUP, up, true, false),
            MenuHookAction::SuppressAndPostCancel
        );
        assert_eq!(capture.take_completed(), Some(up));
        assert_eq!(capture, MenuRightClickCapture::EMPTY);

        assert_eq!(
            capture.observe(WM_RBUTTONDOWN, down, true, true),
            MenuHookAction::Pass
        );
        assert_eq!(capture, MenuRightClickCapture::EMPTY);
    }

    #[test]
    fn replacement_capture_posts_cancel_once_and_keeps_latest_complete_point() {
        let first_down = ScreenPoint { x: 40, y: 60 };
        let first_up = ScreenPoint { x: 42, y: 62 };
        let latest_down = ScreenPoint { x: 140, y: 160 };
        let latest_up = ScreenPoint { x: 142, y: 162 };
        let mut capture = MenuRightClickCapture::EMPTY;

        assert_eq!(
            capture.observe(WM_RBUTTONDOWN, first_down, true, false),
            MenuHookAction::Suppress
        );
        assert_eq!(
            capture.observe(WM_RBUTTONUP, first_up, true, false),
            MenuHookAction::SuppressAndPostCancel
        );
        assert_eq!(
            capture.observe(WM_RBUTTONDOWN, latest_down, true, false),
            MenuHookAction::Suppress
        );
        assert_eq!(
            capture.observe(WM_RBUTTONUP, latest_up, true, false),
            MenuHookAction::Suppress,
            "the popup owner receives only one asynchronous cancellation request"
        );
        assert_eq!(capture.take_completed(), Some(latest_up));
        assert_eq!(capture, MenuRightClickCapture::EMPTY);
    }

    #[test]
    fn replacement_capture_cleans_incomplete_and_wrong_owner_gestures() {
        let point = ScreenPoint { x: -20, y: 80 };
        let mut capture = MenuRightClickCapture::EMPTY;
        assert_eq!(
            capture.observe(WM_RBUTTONDOWN, point, false, false),
            MenuHookAction::Pass
        );
        assert_eq!(
            capture.observe(WM_RBUTTONUP, point, false, false),
            MenuHookAction::Pass
        );
        assert_eq!(capture.take_completed(), None);

        assert_eq!(
            capture.observe(WM_RBUTTONDOWN, point, true, false),
            MenuHookAction::Suppress
        );
        assert_eq!(
            capture.observe(WM_RBUTTONUP, point, false, false),
            MenuHookAction::Suppress
        );
        assert_eq!(capture, MenuRightClickCapture::EMPTY);
    }

    #[test]
    fn host_context_command_verbs_are_delegated_to_the_long_lived_host() {
        for (verb, expected) in [
            ("open", ContextMenuHostCommand::Open),
            ("cut", ContextMenuHostCommand::Cut),
            ("COPY", ContextMenuHostCommand::Copy),
            ("paste", ContextMenuHostCommand::Paste),
            ("copyaspath", ContextMenuHostCommand::CopyPath),
            ("link", ContextMenuHostCommand::CreateShortcut),
            ("delete", ContextMenuHostCommand::Delete),
            ("rename", ContextMenuHostCommand::Rename),
            ("Windows.Share", ContextMenuHostCommand::Share),
            ("Windows.ModernShare", ContextMenuHostCommand::Share),
            ("PinToStartScreen", ContextMenuHostCommand::PinToStart),
            ("pintohome", ContextMenuHostCommand::ToggleQuickAccess),
            ("pintohomefile", ContextMenuHostCommand::ToggleQuickAccess),
            ("unpinfromhome", ContextMenuHostCommand::ToggleQuickAccess),
            ("properties", ContextMenuHostCommand::Properties),
        ] {
            assert_eq!(host_command_from_verb(verb), Some(expected), "{verb}");
        }
        assert_eq!(host_command_from_verb("7-Zip"), None);
        assert_eq!(host_command_from_verb("provider.command"), None);
        for label in [
            "內容(&R)",
            "属性(&R)",
            "プロパティ(&R)",
            "속성(&R)",
            "&Properties",
        ] {
            assert!(is_properties_label(label), "{label}");
        }
        assert!(!is_properties_label("Open Git Bash here"));
    }

    #[test]
    fn localized_share_labels_are_recognized_without_matching_unrelated_commands() {
        for label in [
            "Share",
            "Share\tS",
            "\u{5171}\u{7528}(&S)",
            "\u{5206}\u{4eab}(&S)",
        ] {
            assert!(is_share_label(label), "{label}");
        }
        for label in ["Share with", "Stop sharing", "Properties", ""] {
            assert!(!is_share_label(label), "{label}");
        }
    }

    #[test]
    fn open_is_host_owned_only_for_a_single_item_target() {
        let parent = LocationDescriptor::file_system(PathBuf::from(r"C:\fixture"));
        let item = |name: &str, id| ItemDescriptor {
            id: ShellItemId::from_provider_bytes([id]).expect("id"),
            location: LocationDescriptor::file_system(PathBuf::from(name)),
        };
        let one = ShellContextMenuTarget::Items {
            parent: parent.clone(),
            items: vec![item(r"C:\fixture\one.txt", 1)],
        };
        let many = ShellContextMenuTarget::Items {
            parent,
            items: vec![
                item(r"C:\fixture\one.txt", 1),
                item(r"C:\fixture\two.txt", 2),
            ],
        };
        assert!(host_command_applies_to_target(
            ContextMenuHostCommand::Open,
            &one
        ));
        assert!(!host_command_applies_to_target(
            ContextMenuHostCommand::Open,
            &many
        ));
        assert!(host_command_applies_to_target(
            ContextMenuHostCommand::Copy,
            &many
        ));
    }

    fn collect_menu_entries(menu: HMENU, depth: usize, output: &mut Vec<(String, u32, usize)>) {
        if depth > 8 {
            return;
        }
        // SAFETY: the caller keeps the queried popup and every submenu alive.
        let count = unsafe { GetMenuItemCount(Some(menu)) };
        if count <= 0 {
            return;
        }
        for position in 0..count {
            let mut buffer = [0_u16; 512];
            // SAFETY: menu is live and buffer is writable for this call.
            let length = unsafe {
                GetMenuStringW(
                    menu,
                    u32::try_from(position).expect("menu position fits u32"),
                    Some(&mut buffer),
                    MF_BYPOSITION,
                )
            };
            let label = if length > 0 {
                String::from_utf16_lossy(&buffer[..usize::try_from(length).expect("menu length")])
            } else {
                String::new()
            };
            // SAFETY: position is within GetMenuItemCount's live range.
            let command_id = unsafe { GetMenuItemID(menu, position) };
            if !label.is_empty() {
                output.push((label, command_id, depth));
            }
            // SAFETY: position is within the current menu; null means this is a leaf.
            let child = unsafe { GetSubMenu(menu, position) };
            if !child.0.is_null() {
                collect_menu_entries(child, depth + 1, output);
            }
        }
    }

    fn top_level_submenu_by_label(menu: HMENU, needle: &str) -> Option<(HMENU, usize)> {
        let count = unsafe { GetMenuItemCount(Some(menu)) };
        for position in 0..count.max(0) {
            let mut buffer = [0_u16; 512];
            let length = unsafe {
                GetMenuStringW(
                    menu,
                    u32::try_from(position).ok()?,
                    Some(&mut buffer),
                    MF_BYPOSITION,
                )
            };
            let label =
                String::from_utf16_lossy(&buffer[..usize::try_from(length).unwrap_or_default()]);
            if label
                .to_ascii_lowercase()
                .contains(&needle.to_ascii_lowercase())
            {
                let submenu = unsafe { GetSubMenu(menu, position) };
                if !submenu.0.is_null() {
                    return Some((submenu, usize::try_from(position).ok()?));
                }
            }
        }
        None
    }

    #[derive(Debug, Eq, PartialEq)]
    struct MenuIdentityRow {
        depth: usize,
        position: i32,
        command_id: u32,
        submenu: isize,
        canonical_verb: Option<String>,
        bitmap_present: bool,
    }

    fn snapshot_menu_identity(
        context_menu: &IContextMenu,
        menu: HMENU,
        depth: usize,
        output: &mut Vec<MenuIdentityRow>,
    ) {
        if depth > 8 {
            return;
        }
        let count = unsafe { GetMenuItemCount(Some(menu)) };
        for position in 0..count.max(0) {
            let mut info = MENUITEMINFOW {
                cbSize: u32::try_from(size_of::<MENUITEMINFOW>()).expect("menu item size"),
                fMask: MIIM_ID | MIIM_SUBMENU | MIIM_BITMAP,
                ..Default::default()
            };
            unsafe {
                GetMenuItemInfoW(menu, u32::try_from(position).unwrap(), true, &raw mut info)
            }
            .expect("snapshot menu identity");
            let canonical_verb = (info.wID >= COMMAND_FIRST && info.wID <= COMMAND_LAST)
                .then(|| canonical_verb_at_offset(context_menu, info.wID - COMMAND_FIRST))
                .flatten();
            output.push(MenuIdentityRow {
                depth,
                position,
                command_id: info.wID,
                submenu: info.hSubMenu.0 as isize,
                canonical_verb,
                bitmap_present: !info.hbmpItem.is_invalid(),
            });
            if !info.hSubMenu.0.is_null() {
                snapshot_menu_identity(context_menu, info.hSubMenu, depth + 1, output);
            }
        }
    }

    fn assert_owned_popup_preserves_menu_identity(
        context_menu: &IContextMenu,
        menu: HMENU,
        owner: HWND,
    ) {
        let mut before = Vec::new();
        snapshot_menu_identity(context_menu, menu, 0, &mut before);
        let thread_id = unsafe { GetCurrentThreadId() };
        let cancel = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(3));
            unsafe {
                PostThreadMessageW(thread_id, WM_KEYDOWN, WPARAM(27), LPARAM(0))
                    .expect("Escape custom popup");
            }
        });
        let outcome =
            crate::immersive_popup::present(menu, owner, POINT { x: 100, y: 100 }, 96, false)
                .expect("installed menu is supported by the application-owned popup");
        cancel.join().expect("cancel sender");
        assert_eq!(outcome.command, 0);
        let mut after = Vec::new();
        snapshot_menu_identity(context_menu, menu, 0, &mut after);
        assert_eq!(before, after, "popup presentation mutated HMENU identity");
    }

    #[allow(
        clippy::inline_always,
        clippy::ref_as_ptr,
        reason = "windows-rs implement macro generates the COM identity glue"
    )]
    #[implement(IContextMenu3)]
    struct OwnerDrawFakeHandler {
        messages: Arc<Mutex<Vec<u32>>>,
        query_flags: Arc<Mutex<Vec<u32>>>,
        release_trace: Arc<Mutex<Vec<&'static str>>>,
        owner: Arc<AtomicIsize>,
        invoke_path: Option<PathBuf>,
        invoked: Arc<AtomicBool>,
    }

    impl Drop for OwnerDrawFakeHandler {
        fn drop(&mut self) {
            self.release_trace
                .lock()
                .expect("release trace")
                .push("handler");
        }
    }

    impl IContextMenu_Impl for OwnerDrawFakeHandler_Impl {
        fn QueryContextMenu(
            &self,
            hmenu: HMENU,
            _indexmenu: u32,
            idcmdfirst: u32,
            _idcmdlast: u32,
            uflags: u32,
        ) -> HRESULT {
            self.query_flags
                .lock()
                .expect("query flag trace")
                .push(uflags);
            // SAFETY: the fixture receives a live popup and inserts two owner-draw commands.
            unsafe { AppendMenuW(hmenu, MF_OWNERDRAW, idcmdfirst as usize, PCWSTR::null()) }
                .expect("append owner-draw command");
            unsafe { AppendMenuW(hmenu, MF_OWNERDRAW, idcmdfirst as usize + 1, PCWSTR::null()) }
                .expect("append properties command");
            HRESULT(2)
        }

        fn InvokeCommand(&self, _pici: *const CMINVOKECOMMANDINFO) -> WinResult<()> {
            if let Some(path) = &self.invoke_path {
                std::fs::write(path, b"created by controlled context-menu extension")
                    .expect("controlled extension mutation");
            }
            self.invoked.store(true, AtomicOrdering::Release);
            Ok(())
        }

        fn GetCommandString(
            &self,
            idcmd: usize,
            utype: u32,
            _preserved: *const u32,
            pszname: PSTR,
            cchmax: u32,
        ) -> WinResult<()> {
            if idcmd <= 1 && utype == GCS_VERBW {
                let verb = if idcmd == 0 {
                    "Windows.ModernShare\0"
                } else {
                    "properties\0"
                }
                .encode_utf16()
                .collect::<Vec<_>>();
                let capacity = usize::try_from(cchmax).expect("verb capacity fits usize");
                assert!(verb.len() <= capacity);
                let bytes = verb
                    .iter()
                    .flat_map(|character| character.to_le_bytes())
                    .collect::<Vec<_>>();
                // SAFETY: the caller supplied writable storage for cchmax UTF-16 characters;
                // byte-wise copying does not strengthen the alignment of PSTR.
                unsafe {
                    std::ptr::copy_nonoverlapping(bytes.as_ptr(), pszname.0, bytes.len());
                }
            }
            Ok(())
        }
    }

    impl IContextMenu2_Impl for OwnerDrawFakeHandler_Impl {
        fn HandleMenuMsg(&self, umsg: u32, wparam: WPARAM, lparam: LPARAM) -> WinResult<()> {
            self.HandleMenuMsg2(umsg, wparam, lparam, std::ptr::null_mut())
        }
    }

    impl IContextMenu3_Impl for OwnerDrawFakeHandler_Impl {
        fn HandleMenuMsg2(
            &self,
            umsg: u32,
            _wparam: WPARAM,
            _lparam: LPARAM,
            plresult: *mut LRESULT,
        ) -> WinResult<()> {
            self.messages.lock().expect("message trace").push(umsg);
            if !plresult.is_null() {
                // SAFETY: the caller supplied writable storage for the duration of the call.
                unsafe { plresult.write(LRESULT(0x51)) };
            }
            if umsg == WM_INITMENUPOPUP {
                let raw = self.owner.load(AtomicOrdering::Acquire);
                if raw != 0 {
                    // SAFETY: the owner HWND remains live throughout the outer SendMessage call.
                    unsafe {
                        SendMessageW(
                            HWND(raw as *mut c_void),
                            WM_MENUCHAR,
                            Some(WPARAM(0)),
                            Some(LPARAM(0)),
                        )
                    };
                }
            }
            Ok(())
        }
    }

    #[test]
    fn real_background_single_and_multi_shell_menus_have_commands_and_release() {
        let _guard = crate::clipboard::CLIPBOARD_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // SAFETY: serialized test owns and balances one OLE STA initialization.
        unsafe { OleInitialize(None) }.expect("OLE initialize");
        let fixture = explorer_test_support::OwnedTempFixture::new().expect("menu fixture");
        let first = fixture.create_file("first.txt", b"one").expect("first");
        let second = fixture.create_file("second.txt", b"two").expect("second");
        let item = |path: PathBuf, id| ItemDescriptor {
            id: ShellItemId::from_provider_bytes([id]).expect("id"),
            location: LocationDescriptor::file_system(path),
        };
        let parent = LocationDescriptor::file_system(fixture.root());
        let first = item(first, 1);
        let second = item(second, 2);
        for target in [
            ShellContextMenuTarget::Background {
                parent: parent.clone(),
            },
            ShellContextMenuTarget::Items {
                parent: parent.clone(),
                items: vec![first.clone()],
            },
            ShellContextMenuTarget::Items {
                parent,
                items: vec![first, second],
            },
        ] {
            assert!(query_command_count(&target).expect("query menu") > 0);
        }
        // SAFETY: balances OleInitialize after all menu interfaces and windows were released.
        unsafe { OleUninitialize() };
    }

    #[test]
    fn real_explorer_profile_baseline_captures_labels_for_every_target_and_shift_profile() {
        let _guard = crate::clipboard::CLIPBOARD_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        unsafe { OleInitialize(None) }.expect("OLE initialize");
        let fixture = explorer_test_support::OwnedTempFixture::new().expect("menu fixture");
        let file = fixture.create_file("item.txt", b"item").expect("file");
        let second_file = fixture
            .create_file("second.txt", b"second")
            .expect("second file");
        let folder = fixture.root().join("folder");
        std::fs::create_dir(&folder).expect("folder");
        let item = |path: PathBuf, id| ItemDescriptor {
            id: ShellItemId::from_provider_bytes([id]).expect("id"),
            location: LocationDescriptor::file_system(path),
        };
        let parent = LocationDescriptor::file_system(fixture.root());
        let file_item = item(file.clone(), 1);
        let targets = [
            (
                "background",
                ShellContextMenuTarget::Background {
                    parent: parent.clone(),
                },
            ),
            (
                "file",
                ShellContextMenuTarget::Items {
                    parent: parent.clone(),
                    items: vec![file_item.clone()],
                },
            ),
            (
                "folder",
                ShellContextMenuTarget::Items {
                    parent: parent.clone(),
                    items: vec![item(folder, 2)],
                },
            ),
            (
                "multi",
                ShellContextMenuTarget::Items {
                    parent,
                    items: vec![file_item, item(second_file, 3)],
                },
            ),
        ];
        for (target_name, target) in targets {
            let mut ordinary_count = 0;
            for profile in [
                ContextMenuInvocationProfile::Explorer,
                ContextMenuInvocationProfile::ExplorerExtended,
            ] {
                let mut owner_state = MenuOwnerState { menu3: None };
                let owner = OwnerWindow::create(&raw mut owner_state).expect("owner");
                let menu = resolve_menu(&target, owner.hwnd()).expect("menu");
                owner_state.menu3 = menu.cast::<IContextMenu3>().ok();
                let popup = OwnedMenu::create().expect("popup");
                let command_count = query_menu(
                    &menu,
                    popup.get(),
                    matches!(target, ShellContextMenuTarget::Items { .. }),
                    profile,
                )
                .expect("query Explorer profile");
                let mut entries = Vec::new();
                collect_menu_entries(popup.get(), 0, &mut entries);
                assert!(!entries.is_empty(), "{target_name} {profile:?} labels");
                let actionable = entries
                    .iter()
                    .filter(|(_, command_id, _)| {
                        *command_id >= COMMAND_FIRST && *command_id <= COMMAND_LAST
                    })
                    .collect::<Vec<_>>();
                let unique_ids = actionable
                    .iter()
                    .map(|(_, command_id, _)| *command_id)
                    .collect::<std::collections::HashSet<_>>();
                assert_eq!(
                    unique_ids.len(),
                    actionable.len(),
                    "{target_name} {profile:?} actionable command IDs must remain unique"
                );
                let ownership = actionable
                    .iter()
                    .map(|(label, command_id, depth)| {
                        let offset = *command_id - COMMAND_FIRST;
                        assert!(
                            offset < u32::try_from(command_count).expect("bounded command count"),
                            "{target_name} {profile:?} command outside queried range: {label}"
                        );
                        (
                            label,
                            *depth,
                            canonical_verb_at_offset(&menu, offset),
                            host_command_at_offset(
                                &menu,
                                popup.get(),
                                offset,
                                matches!(target, ShellContextMenuTarget::Items { .. }),
                            )
                            .map_or("provider", ContextMenuHostCommand::wire_name),
                        )
                    })
                    .collect::<Vec<_>>();
                assert_eq!(ownership.len(), actionable.len());
                if profile == ContextMenuInvocationProfile::Explorer {
                    ordinary_count = entries.len();
                } else {
                    assert!(entries.len() >= ordinary_count);
                }
                eprintln!(
                    "context-menu-baseline target={target_name} profile={profile:?} count={} labels={:?} fingerprints={:?} command_audit={ownership:?}",
                    entries.len(),
                    entries.iter().map(|entry| &entry.0).collect::<Vec<_>>(),
                    entries
                        .iter()
                        .map(|entry| fingerprint_menu_label(
                            &entry.0.encode_utf16().collect::<Vec<_>>()
                        ))
                        .collect::<Vec<_>>()
                );
                owner_state.menu3 = None;
            }
        }
        unsafe { OleUninitialize() };
    }

    #[test]
    fn real_non_path_namespace_background_menu_uses_shell_identity() {
        let _guard = crate::clipboard::CLIPBOARD_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        unsafe { OleInitialize(None) }.expect("OLE initialize");
        for parsing_name in ["shell:MyComputerFolder", "shell:RecycleBinFolder"] {
            let target = ShellContextMenuTarget::Background {
                parent: LocationDescriptor::ParsingName(parsing_name.to_owned()),
            };
            assert!(query_command_count(&target).expect("namespace menu") > 0);
        }
        unsafe { OleUninitialize() };
    }

    #[test]
    fn windows_compress_fallback_creates_a_real_collision_safe_zip() {
        let _guard = crate::clipboard::CLIPBOARD_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        unsafe { OleInitialize(None) }.expect("OLE initialize");
        let fixture = explorer_test_support::OwnedTempFixture::new().expect("compress fixture");
        let source = fixture
            .create_file("compress.txt", b"zip me")
            .expect("source");
        let target = ShellContextMenuTarget::Items {
            parent: LocationDescriptor::file_system(fixture.root()),
            items: vec![ItemDescriptor {
                id: ShellItemId::from_provider_bytes([0x63]).expect("item id"),
                location: LocationDescriptor::file_system(source),
            }],
        };
        let first = compress_selection_to_zip(&target).expect("first ZIP");
        let second = compress_selection_to_zip(&target).expect("collision-safe ZIP");
        assert!(first.is_file());
        assert!(second.is_file());
        assert_ne!(first, second);
        unsafe { OleUninitialize() };
    }

    #[test]
    #[ignore = "requires the locally installed 7-Zip shell extension"]
    fn installed_7zip_extension_queries_submenu_and_invokes_owned_archive_command() {
        let _guard = crate::clipboard::CLIPBOARD_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // SAFETY: serialized test owns and balances one OLE STA initialization.
        unsafe { OleInitialize(None) }.expect("OLE initialize");
        let fixture = explorer_test_support::OwnedTempFixture::new().expect("7-Zip fixture");
        let source = fixture
            .create_file("third-party.txt", b"owned 7-Zip context menu fixture")
            .expect("source fixture");
        let target = ShellContextMenuTarget::Items {
            parent: LocationDescriptor::file_system(fixture.root()),
            items: vec![ItemDescriptor {
                id: ShellItemId::from_provider_bytes([0x7a]).expect("item id"),
                location: LocationDescriptor::file_system(source),
            }],
        };
        let mut owner_state = MenuOwnerState { menu3: None };
        let owner = OwnerWindow::create(&raw mut owner_state).expect("menu owner");
        let menu = resolve_menu(&target, owner.hwnd()).expect("real item context menu");
        owner_state.menu3 = menu.cast::<IContextMenu3>().ok();
        let popup = OwnedMenu::create().expect("popup");
        query_menu(
            &menu,
            popup.get(),
            true,
            ContextMenuInvocationProfile::ExplorerExtended,
        )
        .expect("keyboard/extended query");
        let mut entries = Vec::new();
        collect_menu_entries(popup.get(), 0, &mut entries);
        for (label, command_id, depth) in &entries {
            println!("depth={depth}; id={command_id}; label={label:?}");
        }
        assert!(
            entries
                .iter()
                .any(|(label, _, depth)| *depth == 0 && label.contains("7-Zip")),
            "installed 7-Zip submenu was not exposed by the real Shell menu"
        );
        assert_owned_popup_preserves_menu_identity(&menu, popup.get(), owner.hwnd());
        let (_, command_id, _) = entries
            .iter()
            .find(|(label, id, depth)| {
                *depth > 0
                    && *id >= COMMAND_FIRST
                    && *id <= COMMAND_LAST
                    && label.to_ascii_lowercase().contains("third-party.7z")
            })
            .expect("safe direct 7-Zip add-to-third-party.7z command");
        invoke(&menu, owner.hwnd(), *command_id - COMMAND_FIRST).expect("invoke 7-Zip command");
        let archive = fixture.root().join("third-party.7z");
        let deadline = Instant::now() + Duration::from_secs(10);
        while !archive.exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(
            archive.is_file(),
            "7-Zip command did not create the owned archive"
        );
        assert!(std::fs::metadata(&archive).expect("archive metadata").len() > 0);
        drop(popup);
        drop(owner);
        owner_state.menu3 = None;
        drop(menu);
        // SAFETY: all menu and COM references were released above.
        unsafe { OleUninitialize() };
    }

    #[test]
    #[ignore = "requires the locally installed WinRAR shell extension"]
    fn installed_winrar_extension_initializes_and_invokes_owned_archive_command() {
        let _guard = crate::clipboard::CLIPBOARD_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        unsafe { OleInitialize(None) }.expect("OLE initialize");
        let fixture = explorer_test_support::OwnedTempFixture::new().expect("WinRAR fixture");
        let source = fixture
            .create_file("winrar-safe.txt", b"owned WinRAR context menu fixture")
            .expect("source fixture");
        let target = ShellContextMenuTarget::Items {
            parent: LocationDescriptor::file_system(fixture.root()),
            items: vec![ItemDescriptor {
                id: ShellItemId::from_provider_bytes([0x72]).expect("item id"),
                location: LocationDescriptor::file_system(source),
            }],
        };
        let mut owner_state = MenuOwnerState { menu3: None };
        let owner = OwnerWindow::create(&raw mut owner_state).expect("menu owner");
        let menu = resolve_menu(&target, owner.hwnd()).expect("real item context menu");
        owner_state.menu3 = menu.cast::<IContextMenu3>().ok();
        let popup = OwnedMenu::create().expect("popup");
        query_menu(
            &menu,
            popup.get(),
            true,
            ContextMenuInvocationProfile::ExplorerExtended,
        )
        .expect("extended query");
        let (winrar, position) =
            top_level_submenu_by_label(popup.get(), "WinRAR").expect("installed WinRAR submenu");
        unsafe {
            SendMessageW(
                owner.hwnd(),
                WM_INITMENUPOPUP,
                Some(WPARAM(winrar.0 as usize)),
                Some(LPARAM(position as isize)),
            );
        }
        let mut entries = Vec::new();
        collect_menu_entries(winrar, 1, &mut entries);
        for (label, command_id, depth) in &entries {
            println!("depth={depth}; id={command_id}; label={label:?}");
        }
        assert_owned_popup_preserves_menu_identity(&menu, popup.get(), owner.hwnd());
        let (_, command_id, _) = entries
            .iter()
            .find(|(label, id, _)| {
                *id >= COMMAND_FIRST
                    && *id <= COMMAND_LAST
                    && label.to_ascii_lowercase().contains("winrar-safe.rar")
                    && !label.contains("郵寄")
                    && !label.to_ascii_lowercase().contains("email")
            })
            .expect("safe direct WinRAR add-to-winrar-safe.rar command");
        invoke(&menu, owner.hwnd(), *command_id - COMMAND_FIRST).expect("invoke WinRAR command");
        let archive = fixture.root().join("winrar-safe.rar");
        let deadline = Instant::now() + Duration::from_secs(10);
        while !archive.exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(archive.is_file(), "WinRAR did not create the owned archive");
        assert!(std::fs::metadata(&archive).expect("archive metadata").len() > 0);
        drop(popup);
        drop(owner);
        owner_state.menu3 = None;
        drop(menu);
        unsafe { OleUninitialize() };
    }

    #[test]
    #[ignore = "requires the locally installed TortoiseGit shell extension"]
    fn installed_tortoisegit_extension_initializes_and_closes_about_dialog() {
        let _guard = crate::clipboard::CLIPBOARD_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        unsafe { OleInitialize(None) }.expect("OLE initialize");
        let fixture = explorer_test_support::OwnedTempFixture::new().expect("TortoiseGit fixture");
        let source = fixture
            .create_file(
                "tortoise-safe.txt",
                b"owned TortoiseGit context menu fixture",
            )
            .expect("source fixture");
        let target = ShellContextMenuTarget::Items {
            parent: LocationDescriptor::file_system(fixture.root()),
            items: vec![ItemDescriptor {
                id: ShellItemId::from_provider_bytes([0x74]).expect("item id"),
                location: LocationDescriptor::file_system(source),
            }],
        };
        let mut owner_state = MenuOwnerState { menu3: None };
        let owner = OwnerWindow::create(&raw mut owner_state).expect("menu owner");
        let menu = resolve_menu(&target, owner.hwnd()).expect("real item context menu");
        owner_state.menu3 = menu.cast::<IContextMenu3>().ok();
        let popup = OwnedMenu::create().expect("popup");
        query_menu(
            &menu,
            popup.get(),
            true,
            ContextMenuInvocationProfile::ExplorerExtended,
        )
        .expect("extended query");
        let (tortoise, position) = top_level_submenu_by_label(popup.get(), "TortoiseGit")
            .expect("installed TortoiseGit submenu");
        unsafe {
            SendMessageW(
                owner.hwnd(),
                WM_INITMENUPOPUP,
                Some(WPARAM(tortoise.0 as usize)),
                Some(LPARAM(position as isize)),
            );
        }
        let mut entries = Vec::new();
        collect_menu_entries(tortoise, 1, &mut entries);
        assert_owned_popup_preserves_menu_identity(&menu, popup.get(), owner.hwnd());
        let (_, command_id, _) = entries
            .iter()
            .find(|(label, id, _)| {
                *id >= COMMAND_FIRST
                    && *id <= COMMAND_LAST
                    && label.replace('&', "").eq_ignore_ascii_case("About")
            })
            .expect("non-mutating TortoiseGit About command");
        invoke(&menu, owner.hwnd(), *command_id - COMMAND_FIRST).expect("invoke TortoiseGit About");
        let deadline = Instant::now() + Duration::from_secs(10);
        let dialog = loop {
            if let Ok(dialog) = unsafe { FindWindowW(None, w!("About TortoiseGit")) } {
                break Some(dialog);
            }
            if Instant::now() >= deadline {
                break None;
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        let dialog = dialog.expect("TortoiseGit About dialog did not appear");
        unsafe { PostMessageW(Some(dialog), WM_CLOSE, WPARAM(0), LPARAM(0)) }
            .expect("close TortoiseGit About dialog");
        drop(popup);
        drop(owner);
        owner_state.menu3 = None;
        drop(menu);
        unsafe { OleUninitialize() };
    }

    #[test]
    #[ignore = "requires the locally installed VS Code shell extension"]
    fn installed_vscode_extension_opens_and_closes_owned_folder_window() {
        let _guard = crate::clipboard::CLIPBOARD_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        unsafe { OleInitialize(None) }.expect("OLE initialize");
        let fixture = explorer_test_support::OwnedTempFixture::new().expect("VS Code fixture");
        let folder = fixture.root().join("vscode-shell-safe");
        std::fs::create_dir(&folder).expect("owned VS Code folder");
        let target = ShellContextMenuTarget::Items {
            parent: LocationDescriptor::file_system(fixture.root()),
            items: vec![ItemDescriptor {
                id: ShellItemId::from_provider_bytes([0x76]).expect("item id"),
                location: LocationDescriptor::file_system(folder),
            }],
        };
        let mut owner_state = MenuOwnerState { menu3: None };
        let owner = OwnerWindow::create(&raw mut owner_state).expect("menu owner");
        let menu = resolve_menu(&target, owner.hwnd()).expect("real folder context menu");
        owner_state.menu3 = menu.cast::<IContextMenu3>().ok();
        let popup = OwnedMenu::create().expect("popup");
        query_menu(
            &menu,
            popup.get(),
            true,
            ContextMenuInvocationProfile::ExplorerExtended,
        )
        .expect("extended query");
        let mut entries = Vec::new();
        collect_menu_entries(popup.get(), 0, &mut entries);
        assert_owned_popup_preserves_menu_identity(&menu, popup.get(), owner.hwnd());
        let (_, command_id, _) = entries
            .iter()
            .find(|(label, id, depth)| {
                *depth == 0
                    && *id >= COMMAND_FIRST
                    && *id <= COMMAND_LAST
                    && (label.contains("Code 開啟")
                        || label.to_ascii_lowercase().contains("open with code"))
            })
            .expect("installed VS Code command");
        let verb = canonical_verb_at_offset(&menu, *command_id - COMMAND_FIRST);
        println!("VS Code command id={command_id}; canonical_verb={verb:?}");
        invoke(&menu, owner.hwnd(), *command_id - COMMAND_FIRST).expect("invoke VS Code command");

        let titles = [
            "vscode-shell-safe - Visual Studio Code",
            "vscode-shell-safe - SuperExplorer - Visual Studio Code",
        ]
        .map(|title| title.encode_utf16().chain([0]).collect::<Vec<_>>());
        let deadline = Instant::now() + Duration::from_secs(15);
        let code_window = loop {
            let found = titles
                .iter()
                .find_map(|title| unsafe { FindWindowW(None, PCWSTR(title.as_ptr())) }.ok());
            if found.is_some() || Instant::now() >= deadline {
                break found;
            }
            std::thread::sleep(Duration::from_millis(100));
        };
        let code_window = code_window.expect("owned VS Code folder window did not appear");
        unsafe { PostMessageW(Some(code_window), WM_CLOSE, WPARAM(0), LPARAM(0)) }
            .expect("close owned VS Code window");
        drop(popup);
        drop(owner);
        owner_state.menu3 = None;
        drop(menu);
        unsafe { OleUninitialize() };
    }

    #[test]
    fn controlled_owner_draw_handler_forwards_reentrant_messages_and_releases_in_order() {
        let _guard = crate::clipboard::CLIPBOARD_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // SAFETY: the serialized fixture owns this apartment initialization.
        unsafe { OleInitialize(None) }.expect("OLE initialize");
        let messages = Arc::new(Mutex::new(Vec::new()));
        let query_flags = Arc::new(Mutex::new(Vec::new()));
        let release_trace = Arc::new(Mutex::new(Vec::new()));
        let owner_handle = Arc::new(AtomicIsize::new(0));
        let handler: IContextMenu3 = OwnerDrawFakeHandler {
            messages: Arc::clone(&messages),
            query_flags: Arc::clone(&query_flags),
            release_trace: Arc::clone(&release_trace),
            owner: Arc::clone(&owner_handle),
            invoke_path: None,
            invoked: Arc::new(AtomicBool::new(false)),
        }
        .into();
        let before = ContextMenuResourceSnapshot::capture();
        let mut owner_state = MenuOwnerState {
            menu3: Some(handler.clone()),
        };
        let owner = OwnerWindow::create(&raw mut owner_state).expect("owner window");
        owner_handle.store(owner.hwnd().0 as isize, AtomicOrdering::Release);
        let popup = OwnedMenu::create().expect("owner-draw popup");
        let menu: IContextMenu = handler.cast().expect("base context menu");
        let command_count = query_menu(
            &menu,
            popup.get(),
            true,
            ContextMenuInvocationProfile::Explorer,
        )
        .expect("query controlled menu");
        assert_eq!(command_count, 2);
        assert_eq!(
            *query_flags.lock().expect("query flags"),
            vec![CMF_NORMAL | CMF_EXPLORE | CMF_SYNCCASCADEMENU | CMF_ITEMMENU | CMF_CANRENAME]
        );
        let extended_popup = OwnedMenu::create().expect("extended popup");
        query_menu(
            &menu,
            extended_popup.get(),
            true,
            ContextMenuInvocationProfile::ExplorerExtended,
        )
        .expect("extended controlled query");
        assert_eq!(
            *query_flags.lock().expect("extended query flags"),
            vec![
                CMF_NORMAL | CMF_EXPLORE | CMF_SYNCCASCADEMENU | CMF_ITEMMENU | CMF_CANRENAME,
                CMF_NORMAL
                    | CMF_EXPLORE
                    | CMF_SYNCCASCADEMENU
                    | CMF_ITEMMENU
                    | CMF_CANRENAME
                    | CMF_EXTENDEDVERBS,
            ]
        );
        drop(extended_popup);
        assert_eq!(
            canonical_verb_offset(&menu, command_count, "Windows.Share")
                .expect("controlled canonical Share verb"),
            0
        );
        assert_eq!(
            canonical_verb_offset(&menu, command_count, "properties")
                .expect("controlled canonical Properties verb"),
            1
        );

        for message in [WM_MEASUREITEM, WM_DRAWITEM, WM_MENUCHAR, WM_INITMENUPOPUP] {
            // SAFETY: the hidden owner and its MenuOwnerState are live; the fake does not
            // dereference message payloads and deliberately re-enters on WM_INITMENUPOPUP.
            let result =
                unsafe { SendMessageW(owner.hwnd(), message, Some(WPARAM(0)), Some(LPARAM(0))) };
            assert_eq!(result, LRESULT(0x51));
        }
        assert_eq!(
            *messages.lock().expect("message trace"),
            vec![
                WM_MEASUREITEM,
                WM_DRAWITEM,
                WM_MENUCHAR,
                WM_INITMENUPOPUP,
                WM_MENUCHAR,
            ]
        );

        drop(popup);
        release_trace.lock().expect("release trace").push("menu");
        drop(owner);
        release_trace.lock().expect("release trace").push("owner");
        owner_state.menu3 = None;
        drop(menu);
        drop(handler);
        assert_eq!(
            *release_trace.lock().expect("release trace"),
            vec!["menu", "owner", "handler"]
        );
        let after = ContextMenuResourceSnapshot::capture();
        assert_eq!(after.active_menus, before.active_menus);
        assert_eq!(after.active_owner_windows, before.active_owner_windows);
        assert_eq!(after.active_menu_hooks, before.active_menu_hooks);
        assert!(after.forwarded_messages >= before.forwarded_messages + 5);
        // SAFETY: all COM references and native resources were released above.
        unsafe { OleUninitialize() };
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "one native E2E scenario keeps query, extension invoke, watcher convergence, and broker recovery correlated"
    )]
    fn end_to_end_context_menu_query_invoke_watcher_and_failure_recovery() {
        let _guard = crate::clipboard::CLIPBOARD_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // SAFETY: the serialized E2E fixture owns this OLE apartment.
        unsafe { OleInitialize(None) }.expect("OLE initialize");
        let fixture = explorer_test_support::OwnedTempFixture::new().expect("context E2E fixture");
        let first = fixture.create_file("first.txt", b"one").expect("first");
        let second = fixture.create_file("second.txt", b"two").expect("second");
        let parent = LocationDescriptor::file_system(fixture.root());
        let item = |path: PathBuf, id| ItemDescriptor {
            id: ShellItemId::from_provider_bytes([id]).expect("item id"),
            location: LocationDescriptor::file_system(path),
        };
        let first_item = item(first, 1);
        let second_item = item(second, 2);
        for target in [
            ShellContextMenuTarget::Background {
                parent: parent.clone(),
            },
            ShellContextMenuTarget::Items {
                parent: parent.clone(),
                items: vec![first_item.clone()],
            },
            ShellContextMenuTarget::Items {
                parent: parent.clone(),
                items: vec![first_item, second_item],
            },
        ] {
            assert!(query_command_count(&target).expect("real Shell query") > 0);
        }

        let tab_id = TabId::new();
        let generation = Generation::new(12);
        let (watch_events, watch_receiver) = std::sync::mpsc::sync_channel(16);
        let mut watcher = crate::watcher::WatcherSession::start(
            fixture.root().to_path_buf(),
            tab_id,
            generation,
            watch_events,
        )
        .expect("start context watcher");
        std::thread::sleep(Duration::from_millis(50));
        let created = fixture.root().join("extension-created.txt");
        let invoked = Arc::new(AtomicBool::new(false));
        let handler: IContextMenu3 = OwnerDrawFakeHandler {
            messages: Arc::new(Mutex::new(Vec::new())),
            query_flags: Arc::new(Mutex::new(Vec::new())),
            release_trace: Arc::new(Mutex::new(Vec::new())),
            owner: Arc::new(AtomicIsize::new(0)),
            invoke_path: Some(created.clone()),
            invoked: Arc::clone(&invoked),
        }
        .into();
        let mut owner_state = MenuOwnerState {
            menu3: Some(handler.clone()),
        };
        let owner = OwnerWindow::create(&raw mut owner_state).expect("controlled owner");
        let menu: IContextMenu = handler.cast().expect("base controlled menu");
        let popup = OwnedMenu::create().expect("controlled popup");
        query_menu(
            &menu,
            popup.get(),
            true,
            ContextMenuInvocationProfile::Explorer,
        )
        .expect("controlled query");
        invoke(&menu, owner.hwnd(), 0).expect("controlled invoke");
        assert!(invoked.load(AtomicOrdering::Acquire));
        assert_eq!(
            std::fs::read(&created).expect("extension output"),
            b"created by controlled context-menu extension"
        );
        let watcher_deadline = Instant::now() + Duration::from_secs(5);
        let mut converged = false;
        while Instant::now() < watcher_deadline {
            if let Ok(ExplorerEvent::DirectoryChanged {
                tab_id: event_tab,
                generation: event_generation,
                changes,
            }) = watch_receiver.recv_timeout(Duration::from_millis(100))
                && event_tab == tab_id
                && event_generation == generation
                && !changes.is_empty()
            {
                converged = true;
                break;
            }
        }
        assert!(converged, "watcher must observe extension invoke mutation");
        watcher.shutdown();
        drop(popup);
        drop(owner);
        owner_state.menu3 = None;
        drop(menu);
        drop(handler);

        let (broker_events, broker_receiver) = std::sync::mpsc::sync_channel(4);
        let failed_context = RequestContext::new(TabId::new(), Generation::new(13));
        start_bounded_job(
            failed_context.clone(),
            Duration::from_millis(100),
            broker_events.clone(),
            || {
                Err(menu_error(
                    "controlled extension",
                    "延伸模組失敗",
                    "E2E fixture",
                ))
            },
        );
        assert!(matches!(
            broker_receiver.recv_timeout(Duration::from_millis(200)),
            Ok(ExplorerEvent::ContextMenuFinished {
                context,
                outcome: ContextMenuOutcome::Failed { .. }
            }) if context == failed_context
        ));
        let recovered_context = RequestContext::new(TabId::new(), Generation::new(14));
        start_bounded_job(
            recovered_context.clone(),
            Duration::from_millis(100),
            broker_events,
            || Ok(ContextMenuOutcome::Cancelled),
        );
        assert!(matches!(
            broker_receiver.recv_timeout(Duration::from_millis(200)),
            Ok(ExplorerEvent::ContextMenuFinished {
                context,
                outcome: ContextMenuOutcome::Cancelled
            }) if context == recovered_context
        ));
        // SAFETY: every native/COM fixture resource was released above.
        unsafe { OleUninitialize() };
    }

    #[test]
    fn bounded_broker_isolates_slow_hung_and_error_handlers_with_correlation() {
        let (events, receiver) = std::sync::mpsc::sync_channel(8);
        let hung_context = RequestContext::new(TabId::new(), Generation::new(7));
        let callback_started = Instant::now();
        start_bounded_job(
            hung_context.clone(),
            Duration::from_millis(20),
            events.clone(),
            || {
                std::thread::sleep(Duration::from_millis(150));
                Ok(ContextMenuOutcome::Cancelled)
            },
        );
        assert!(
            callback_started.elapsed() < Duration::from_millis(20),
            "caller must not execute the handler inline"
        );
        let hung_event = receiver
            .recv_timeout(Duration::from_millis(200))
            .expect("bounded hung-handler terminal");
        let ExplorerEvent::ContextMenuFinished {
            context,
            outcome: ContextMenuOutcome::Failed { error },
        } = hung_event
        else {
            panic!("expected correlated timeout outcome");
        };
        assert_eq!(context, hung_context);
        assert!(error.technical_detail.contains("correlation="));
        assert!(error.technical_detail.contains("deadline_ms=20"));

        let error_context = RequestContext::new(TabId::new(), Generation::new(8));
        start_bounded_job(
            error_context.clone(),
            Duration::from_millis(200),
            events.clone(),
            || {
                Err(menu_error(
                    "controlled handler",
                    "可恢復錯誤",
                    "fixture error",
                ))
            },
        );
        let error_event = receiver
            .recv_timeout(Duration::from_millis(100))
            .expect("error terminal");
        assert!(matches!(
            error_event,
            ExplorerEvent::ContextMenuFinished {
                context,
                outcome: ContextMenuOutcome::Failed { error }
            } if context == error_context
                && error.technical_detail.contains("fixture error")
                && error.technical_detail.contains("correlation=")
        ));

        let panic_context = RequestContext::new(TabId::new(), Generation::new(9));
        start_bounded_job(
            panic_context.clone(),
            Duration::from_millis(200),
            events.clone(),
            || panic!("controlled context-menu worker panic"),
        );
        assert!(matches!(
            receiver.recv_timeout(Duration::from_millis(100)),
            Ok(ExplorerEvent::ContextMenuFinished {
                context,
                outcome: ContextMenuOutcome::Failed { error }
            }) if context == panic_context
                && error.technical_detail.contains("controlled context-menu worker panic")
        ));

        let slow_context = RequestContext::new(TabId::new(), Generation::new(10));
        start_bounded_job(
            slow_context.clone(),
            Duration::from_millis(200),
            events,
            || {
                std::thread::sleep(Duration::from_millis(10));
                Ok(ContextMenuOutcome::Cancelled)
            },
        );
        assert!(matches!(
            receiver.recv_timeout(Duration::from_millis(100)),
            Ok(ExplorerEvent::ContextMenuFinished {
                context,
                outcome: ContextMenuOutcome::Cancelled
            }) if context == slow_context
        ));
        std::thread::sleep(Duration::from_millis(220));
        assert!(
            receiver.try_recv().is_err(),
            "late terminals must be suppressed"
        );
    }

    #[test]
    fn real_application_popup_cancel_soak_releases_menu_resources() {
        let _guard = crate::clipboard::CLIPBOARD_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // SAFETY: this test owns and balances one OLE STA initialization.
        unsafe { OleInitialize(None) }.expect("OLE initialize");
        let fixture = explorer_test_support::OwnedTempFixture::new().expect("popup fixture");
        let request = ContextMenuRequest {
            target: ShellContextMenuTarget::Background {
                parent: LocationDescriptor::file_system(fixture.root()),
            },
            owner_window: 0,
            point: explorer_model::MenuPoint { x: 20, y: 20 },
            keyboard_invoked: false,
            invocation_profile: ContextMenuInvocationProfile::Explorer,
            color_scheme: explorer_model::ContextMenuColorScheme::Light,
            immersive_native_context_menus: true,
            paste_available: false,
            requested_verb: None,
            deadline_ms: 2_000,
        };
        // SAFETY: obtains the current OLE menu thread identifier.
        let thread_id = unsafe { GetCurrentThreadId() };
        let (cancel_tx, cancel_rx) = std::sync::mpsc::sync_channel::<()>(0);
        let (posted_tx, posted_rx) = std::sync::mpsc::sync_channel::<()>(0);
        let cancel = std::thread::spawn(move || {
            while cancel_rx.recv().is_ok() {
                std::thread::sleep(Duration::from_millis(5));
                // SAFETY: value-only Escape messages target the live popup loop.
                unsafe {
                    PostThreadMessageW(thread_id, WM_KEYDOWN, WPARAM(27), LPARAM(0))
                        .expect("Escape down");
                    PostThreadMessageW(thread_id, WM_KEYUP, WPARAM(27), LPARAM(0))
                        .expect("Escape up");
                }
                posted_tx.send(()).expect("acknowledge Escape");
            }
        });
        let run_cycle = |request: &ContextMenuRequest| {
            cancel_tx.send(()).expect("request Escape");
            assert_eq!(
                show(request).expect("show and cancel"),
                ContextMenuOutcome::Cancelled
            );
            posted_rx.recv().expect("Escape was posted");
        };
        // Exclude one-time Shell handler, extension cache, allocator, and window-class
        // initialization from the steady-state leak slope.
        const WARMUP_CYCLES: usize = 100;
        for _ in 0..WARMUP_CYCLES {
            run_cycle(&request);
        }
        let before = ContextMenuResourceSnapshot::capture();
        let (handles_before, private_bytes_before) = process_resource_totals();
        const CYCLES: usize = 1_000;
        for _ in 0..CYCLES {
            run_cycle(&request);
        }
        let after = ContextMenuResourceSnapshot::capture();
        let (handles_after, private_bytes_after) = process_resource_totals();
        eprintln!(
            "context-menu-soak warmup_cycles={WARMUP_CYCLES} cycles={CYCLES} handles_before={handles_before} handles_after={handles_after} handle_delta={} private_bytes_before={private_bytes_before} private_bytes_after={private_bytes_after} private_bytes_delta={}",
            i64::from(handles_after) - i64::from(handles_before),
            private_bytes_after as i128 - private_bytes_before as i128,
        );
        assert_eq!(after.active_menus, before.active_menus);
        assert_eq!(after.active_owner_windows, before.active_owner_windows);
        assert_eq!(after.active_menu_hooks, before.active_menu_hooks);
        assert_eq!(after.forwarded_messages, before.forwarded_messages);
        assert!(handles_after <= handles_before.saturating_add(8));
        let owned_private_delta = private_bytes_after.saturating_sub(private_bytes_before);

        let mut native_request = request.clone();
        native_request.immersive_native_context_menus = false;
        for _ in 0..WARMUP_CYCLES {
            run_cycle(&native_request);
        }
        let (native_handles_before, native_private_before) = process_resource_totals();
        for _ in 0..CYCLES {
            run_cycle(&native_request);
        }
        let (native_handles_after, native_private_after) = process_resource_totals();
        let native_private_delta = native_private_after.saturating_sub(native_private_before);
        eprintln!(
            "native-menu-soak warmup_cycles={WARMUP_CYCLES} cycles={CYCLES} handles_before={native_handles_before} handles_after={native_handles_after} handle_delta={} private_bytes_before={native_private_before} private_bytes_after={native_private_after} private_bytes_delta={native_private_delta}",
            i64::from(native_handles_after) - i64::from(native_handles_before),
        );
        let owned_handle_delta = i64::from(handles_after) - i64::from(handles_before);
        let native_handle_delta =
            i64::from(native_handles_after) - i64::from(native_handles_before);
        assert!(
            owned_handle_delta <= native_handle_delta.saturating_add(8),
            "application-owned presentation must not exceed the native Shell-query handle slope"
        );
        assert!(
            owned_private_delta <= native_private_delta.saturating_add(16 * 1024 * 1024),
            "application-owned presentation must stay within 16 MiB of the native Shell-query baseline"
        );
        drop(cancel_tx);
        cancel.join().expect("cancel sender");
        // SAFETY: balances OleInitialize after every menu resource is released.
        unsafe { OleUninitialize() };
    }

    fn process_resource_totals() -> (u32, usize) {
        let process = unsafe { windows::Win32::System::Threading::GetCurrentProcess() };
        let mut handles = 0_u32;
        unsafe {
            windows::Win32::System::Threading::GetProcessHandleCount(process, &raw mut handles)
        }
        .expect("read process handle count");
        let mut counters =
            windows::Win32::System::ProcessStatus::PROCESS_MEMORY_COUNTERS_EX::default();
        unsafe {
            windows::Win32::System::ProcessStatus::GetProcessMemoryInfo(
                process,
                (&raw mut counters)
                    .cast::<windows::Win32::System::ProcessStatus::PROCESS_MEMORY_COUNTERS>(),
                u32::try_from(size_of_val(&counters)).expect("counter size fits u32"),
            )
        }
        .expect("read process private commit");
        (handles, counters.PrivateUsage)
    }
}
