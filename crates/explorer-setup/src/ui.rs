#![expect(
    unsafe_code,
    reason = "native installer wizard is implemented with Win32 controls"
)]
#![allow(unsafe_op_in_unsafe_fn)]

use crate::{PRODUCT_NAME, PRODUCT_PUBLISHER, default_install_dir, install_to, payload_version, uninstall};
use anyhow::Result;
use std::{
    cell::RefCell,
    ffi::c_void,
    mem::{size_of, zeroed},
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
    thread,
};
use windows::{
    Win32::{
        Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::Gdi::{
            BeginPaint, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, CreateFontW, CreateSolidBrush,
            DEFAULT_CHARSET, DEFAULT_PITCH, DeleteObject, EndPaint, FW_NORMAL, FW_SEMIBOLD,
            FillRect, GetStockObject, HBRUSH, HDC, OUT_DEFAULT_PRECIS, SelectObject, SetBkMode,
            SetTextColor, TRANSPARENT, TextOutW, WHITE_BRUSH,
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Controls::{
                ICC_PROGRESS_CLASS, ICC_STANDARD_CLASSES, INITCOMMONCONTROLSEX,
                InitCommonControlsEx, PBM_SETPOS, PBM_SETRANGE32, PROGRESS_CLASSW,
            },
            Input::KeyboardAndMouse::EnableWindow,
            Shell::{
                FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS, FileOpenDialog, IFileOpenDialog, IShellItem,
                SIGDN_FILESYSPATH,
            },
            WindowsAndMessaging::{
                BS_DEFPUSHBUTTON, BS_PUSHBUTTON, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT,
                CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, ES_AUTOHSCROLL,
                ES_READONLY, GetClientRect, GetMessageW, HICON, HMENU, IMAGE_ICON, IDC_ARROW,
                IsDialogMessageW, LR_DEFAULTSIZE, LoadCursorW, LoadImageW, MSG,
                PostQuitMessage, RegisterClassExW, SW_SHOW, SendMessageW, SetWindowLongPtrW,
                SetWindowTextW, ShowWindow, TranslateMessage, WINDOW_EX_STYLE, WM_CLOSE, WM_COMMAND,
                WM_CREATE, WM_CTLCOLORSTATIC, WM_DESTROY, WM_PAINT, WM_SETFONT, WNDCLASSEXW,
                WS_BORDER, WS_CAPTION, WS_CHILD, WS_MINIMIZEBOX, WS_OVERLAPPED, WS_SYSMENU,
                WS_TABSTOP, WS_VISIBLE, GWLP_USERDATA, IMAGE_FLAGS, WINDOW_STYLE,
            },
        },
    },
    core::{PCWSTR, w},
};

const WM_PROGRESS: u32 = 0x8000 + 1;
const WM_FINISHED: u32 = 0x8000 + 2;

const IDC_TITLE: i32 = 101;
const IDC_SUBTITLE: i32 = 102;
const IDC_BODY: i32 = 103;
const IDC_PATH: i32 = 104;
const IDC_BROWSE: i32 = 105;
const IDC_PROGRESS: i32 = 106;
const IDC_STATUS: i32 = 107;
const IDC_BACK: i32 = 108;
const IDC_NEXT: i32 = 109;
const IDC_CANCEL: i32 = 110;
const IDC_LAUNCH: i32 = 111;

const PAGE_WELCOME: i32 = 0;
const PAGE_LOCATION: i32 = 1;
const PAGE_PROGRESS: i32 = 2;
const PAGE_FINISH: i32 = 3;
const PAGE_ERROR: i32 = 4;

const HEADER_COLOR: COLORREF = COLORREF(0x00_5A_3A_1B);
const ACCENT: COLORREF = COLORREF(0x00_D4_7A_2E);

struct Wizard {
    _hwnd: HWND,
    page: i32,
    title: HWND,
    subtitle: HWND,
    body: HWND,
    path: HWND,
    browse: HWND,
    progress: HWND,
    status: HWND,
    back: HWND,
    next: HWND,
    cancel: HWND,
    launch: HWND,
    font: windows::Win32::Graphics::Gdi::HFONT,
    font_title: windows::Win32::Graphics::Gdi::HFONT,
    header_brush: HBRUSH,
    install_dir: PathBuf,
    version: String,
    error: String,
    working: bool,
}

thread_local! {
    static WIZARD: RefCell<Option<Box<Wizard>>> = const { RefCell::new(None) };
}

pub(crate) fn run_install() -> Result<()> {
    crate::ensure_administrator_for_ui()?;
    unsafe { show_wizard(false) }
}

pub(crate) fn run_uninstall() -> Result<()> {
    crate::ensure_administrator_for_ui()?;
    if !confirm_uninstall()? {
        return Ok(());
    }
    uninstall(&[])?;
    unsafe {
        message("SuperExplorer", "SuperExplorer has been uninstalled.");
    }
    Ok(())
}

fn confirm_uninstall() -> Result<bool> {
    use windows::Win32::UI::{
        Controls::{TASKDIALOG_COMMON_BUTTON_FLAGS, TASKDIALOGCONFIG, TaskDialogIndirect, TD_WARNING_ICON},
        WindowsAndMessaging::IDYES,
    };
    unsafe {
        let mut config: TASKDIALOGCONFIG = zeroed();
        config.cbSize = size_of::<TASKDIALOGCONFIG>() as u32;
        config.pszWindowTitle = w!("SuperExplorer");
        config.pszMainInstruction = w!("Uninstall SuperExplorer?");
        config.pszContent = w!("Program files will be removed. Bookmarks and the MFT cache are kept.");
        config.dwCommonButtons = TASKDIALOG_COMMON_BUTTON_FLAGS(6);
        config.Anonymous1.pszMainIcon = TD_WARNING_ICON;
        let mut button = 0;
        TaskDialogIndirect(&config, Some(&mut button), None, None)?;
        Ok(button == IDYES.0)
    }
}

unsafe fn message(title: &str, body: &str) {
    use windows::Win32::UI::WindowsAndMessaging::{MB_OK, MessageBoxW};
    let title = wide(title);
    let body = wide(body);
    let _ = MessageBoxW(None, PCWSTR(body.as_ptr()), PCWSTR(title.as_ptr()), MB_OK);
}

fn wide(text: impl AsRef<str>) -> Vec<u16> {
    Path::new(text.as_ref())
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect()
}

unsafe fn show_wizard(_uninstall: bool) -> Result<()> {
    use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
    use windows::Win32::System::Console::FreeConsole;
    let _ = FreeConsole();
    let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    let _ = InitCommonControlsEx(&INITCOMMONCONTROLSEX {
        dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
        dwICC: ICC_PROGRESS_CLASS | ICC_STANDARD_CLASSES,
    });

    let instance = GetModuleHandleW(None)?;
    let icon = LoadImageW(
        Some(instance.into()),
        PCWSTR(1 as *const u16),
        IMAGE_ICON,
        0,
        0,
        IMAGE_FLAGS(LR_DEFAULTSIZE.0),
    )
    .ok()
    .map(|handle| HICON(handle.0))
    .unwrap_or_default();

    let class = wide("SuperExplorerSetupWizard");
    let wc = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(wndproc),
        hInstance: instance.into(),
        hIcon: icon,
        hCursor: LoadCursorW(None, IDC_ARROW)?,
        hbrBackground: HBRUSH(GetStockObject(WHITE_BRUSH).0),
        lpszClassName: PCWSTR(class.as_ptr()),
        hIconSm: icon,
        ..Default::default()
    };
    RegisterClassExW(&wc);

    let title = wide("SuperExplorer Setup");
    let hwnd = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        PCWSTR(class.as_ptr()),
        PCWSTR(title.as_ptr()),
        WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        660,
        500,
        None,
        None,
        Some(instance.into()),
        None,
    )?;
    let _ = ShowWindow(hwnd, SW_SHOW);
    let mut msg = MSG::default();
    while GetMessageW(&mut msg, None, 0, 0).as_bool() {
        if !IsDialogMessageW(hwnd, &mut msg).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    Ok(())
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => {
            if let Err(error) = create_children(hwnd) {
                let _ = error;
                PostQuitMessage(1);
            }
            LRESULT(0)
        }
        WM_PAINT => {
            paint_header(hwnd);
            LRESULT(0)
        }
        WM_CTLCOLORSTATIC => {
            let hdc = HDC(wparam.0 as *mut c_void);
            SetBkMode(hdc, TRANSPARENT);
            SetTextColor(hdc, COLORREF(0x00_33_33_33));
            LRESULT(GetStockObject(WHITE_BRUSH).0 as isize)
        }
        WM_COMMAND => {
            on_command(hwnd, (wparam.0 & 0xFFFF) as i32);
            LRESULT(0)
        }
        WM_PROGRESS => {
            update_progress(hwnd, wparam.0 as u32, lparam);
            LRESULT(0)
        }
        WM_FINISHED => {
            finish_install(hwnd, wparam.0 != 0, lparam);
            LRESULT(0)
        }
        WM_CLOSE => {
            if !is_working() {
                DestroyWindow(hwnd).ok();
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn is_working() -> bool {
    WIZARD.with(|slot| slot.borrow().as_ref().is_some_and(|wizard| wizard.working))
}

unsafe fn create_children(hwnd: HWND) -> Result<()> {
    let instance = GetModuleHandleW(None)?;
    let font = CreateFontW(
        -15,
        0,
        0,
        0,
        FW_NORMAL.0 as i32,
        0,
        0,
        0,
        DEFAULT_CHARSET,
        OUT_DEFAULT_PRECIS,
        CLIP_DEFAULT_PRECIS,
        CLEARTYPE_QUALITY,
        u32::from(DEFAULT_PITCH.0),
        w!("Segoe UI"),
    );
    let font_title = CreateFontW(
        -22,
        0,
        0,
        0,
        FW_SEMIBOLD.0 as i32,
        0,
        0,
        0,
        DEFAULT_CHARSET,
        OUT_DEFAULT_PRECIS,
        CLIP_DEFAULT_PRECIS,
        CLEARTYPE_QUALITY,
        u32::from(DEFAULT_PITCH.0),
        w!("Segoe UI"),
    );
    let header_brush = CreateSolidBrush(HEADER_COLOR);
    let mut wizard = Box::new(Wizard {
        _hwnd: hwnd,
        page: PAGE_WELCOME,
        title: HWND::default(),
        subtitle: HWND::default(),
        body: HWND::default(),
        path: HWND::default(),
        browse: HWND::default(),
        progress: HWND::default(),
        status: HWND::default(),
        back: HWND::default(),
        next: HWND::default(),
        cancel: HWND::default(),
        launch: HWND::default(),
        font,
        font_title,
        header_brush,
        install_dir: default_install_dir().unwrap_or_else(|_| PathBuf::from(r"C:\Program Files\SuperExplorer")),
        version: payload_version(),
        error: String::new(),
        working: false,
    });

    wizard.title = child(hwnd, "STATIC", "", WS_CHILD | WS_VISIBLE, 24, 108, 600, 32, IDC_TITLE, instance.into())?;
    wizard.subtitle = child(hwnd, "STATIC", "", WS_CHILD | WS_VISIBLE, 24, 144, 600, 24, IDC_SUBTITLE, instance.into())?;
    wizard.body = child(hwnd, "STATIC", "", WS_CHILD | WS_VISIBLE, 24, 180, 600, 96, IDC_BODY, instance.into())?;
    wizard.path = child(
        hwnd,
        "EDIT",
        &wizard.install_dir.display().to_string(),
        WS_CHILD | WS_TABSTOP | WS_BORDER | WINDOW_STYLE(ES_AUTOHSCROLL as u32 | ES_READONLY as u32),
        24,
        280,
        470,
        28,
        IDC_PATH,
        instance.into(),
    )?;
    wizard.browse = child(hwnd, "BUTTON", "Browse...", WS_CHILD | WS_TABSTOP | WINDOW_STYLE(BS_PUSHBUTTON as u32), 504, 278, 110, 32, IDC_BROWSE, instance.into())?;
    wizard.progress = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        PROGRESS_CLASSW,
        w!(""),
        WS_CHILD,
        24,
        280,
        590,
        24,
        Some(hwnd),
        Some(HMENU(IDC_PROGRESS as *mut c_void)),
        Some(instance.into()),
        None,
    )?;
    let _ = SendMessageW(wizard.progress, PBM_SETRANGE32, Some(WPARAM(0)), Some(LPARAM(100)));
    wizard.status = child(hwnd, "STATIC", "", WS_CHILD, 24, 316, 590, 24, IDC_STATUS, instance.into())?;
    wizard.launch = child(hwnd, "BUTTON", "Launch SuperExplorer", WS_CHILD | WS_TABSTOP | WINDOW_STYLE(3), 24, 280, 300, 24, IDC_LAUNCH, instance.into())?;
    wizard.back = child(hwnd, "BUTTON", "< Back", WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_PUSHBUTTON as u32), 250, 420, 110, 32, IDC_BACK, instance.into())?;
    wizard.next = child(hwnd, "BUTTON", "Next >", WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_DEFPUSHBUTTON as u32), 370, 420, 110, 32, IDC_NEXT, instance.into())?;
    wizard.cancel = child(hwnd, "BUTTON", "Cancel", WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_PUSHBUTTON as u32), 490, 420, 110, 32, IDC_CANCEL, instance.into())?;

    for hwnd in [
        wizard.title,
        wizard.subtitle,
        wizard.body,
        wizard.path,
        wizard.browse,
        wizard.status,
        wizard.launch,
        wizard.back,
        wizard.next,
        wizard.cancel,
    ] {
        let _ = SendMessageW(hwnd, WM_SETFONT, Some(WPARAM(font.0 as usize)), Some(LPARAM(1)));
    }
    let _ = SendMessageW(wizard.title, WM_SETFONT, Some(WPARAM(font_title.0 as usize)), Some(LPARAM(1)));

    SetWindowLongPtrW(hwnd, GWLP_USERDATA, (&raw const *wizard) as isize);
    WIZARD.with(|slot| *slot.borrow_mut() = Some(wizard));
    apply_page();
    Ok(())
}

unsafe fn child(
    parent: HWND,
    class: &str,
    text: &str,
    style: WINDOW_STYLE,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    id: i32,
    instance: windows::Win32::Foundation::HINSTANCE,
) -> Result<HWND> {
    let class = wide(class);
    let text = wide(text);
    Ok(CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        PCWSTR(class.as_ptr()),
        PCWSTR(text.as_ptr()),
        style,
        x,
        y,
        w,
        h,
        Some(parent),
        Some(HMENU(id as *mut c_void)),
        Some(instance),
        None,
    )?)
}

unsafe fn paint_header(hwnd: HWND) {
    let mut ps = zeroed();
    let hdc = BeginPaint(hwnd, &mut ps);
    let mut rc = RECT::default();
    GetClientRect(hwnd, &mut rc).ok();
    let header = RECT {
        left: 0,
        top: 0,
        right: rc.right,
        bottom: 88,
    };
    WIZARD.with(|slot| {
        if let Some(wizard) = slot.borrow().as_ref() {
            FillRect(hdc, &header, wizard.header_brush);
            SetBkMode(hdc, TRANSPARENT);
            SetTextColor(hdc, COLORREF(0x00_FF_FF_FF));
            let _ = SelectObject(hdc, wizard.font_title.into());
            let title = wide("SuperExplorer Setup");
            let _ = TextOutW(hdc, 24, 22, &title[..title.len().saturating_sub(1)]);
            let _ = SelectObject(hdc, wizard.font.into());
            let subtitle = wide("Professional file manager for Windows");
            let _ = TextOutW(hdc, 24, 52, &subtitle[..subtitle.len().saturating_sub(1)]);
            let accent = RECT {
                left: 0,
                top: 88,
                right: rc.right,
                bottom: 92,
            };
            let brush = CreateSolidBrush(ACCENT);
            FillRect(hdc, &accent, brush);
            let _ = DeleteObject(brush.into());
            let footer = RECT {
                left: 0,
                top: rc.bottom - 64,
                right: rc.right,
                bottom: rc.bottom,
            };
            FillRect(hdc, &footer, HBRUSH(GetStockObject(WHITE_BRUSH).0));
        }
    });
    let _ = EndPaint(hwnd, &ps);
}

fn apply_page() {
    WIZARD.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(wizard) = slot.as_mut() else { return };
        unsafe {
            let page = wizard.page;
            show(wizard.body, page == PAGE_WELCOME || page == PAGE_LOCATION || page == PAGE_FINISH || page == PAGE_ERROR);
            show(wizard.path, page == PAGE_LOCATION);
            show(wizard.browse, page == PAGE_LOCATION);
            show(wizard.progress, page == PAGE_PROGRESS);
            show(wizard.status, page == PAGE_PROGRESS);
            show(wizard.launch, page == PAGE_FINISH);
            let _ = EnableWindow(wizard.back, page == PAGE_LOCATION);
            let _ = EnableWindow(wizard.next, page != PAGE_PROGRESS);
            let _ = EnableWindow(wizard.cancel, page != PAGE_PROGRESS && page != PAGE_FINISH);
            match page {
                PAGE_WELCOME => {
                    set_text(wizard.title, "Welcome");
                    set_text(
                        wizard.subtitle,
                        &format!("This will install {PRODUCT_NAME} {}", wizard.version),
                    );
                    set_text(
                        wizard.body,
                        &format!(
                            "{PRODUCT_NAME} is published by {PRODUCT_PUBLISHER}.\n\nSetup will copy program files, register the MFT indexing service, and add Start menu and desktop shortcuts.\n\nClick Next to continue."
                        ),
                    );
                    set_text(wizard.next, "Next >");
                }
                PAGE_LOCATION => {
                    set_text(wizard.title, "Installation folder");
                    set_text(wizard.subtitle, "Choose where SuperExplorer should be installed.");
                    set_text(
                        wizard.body,
                        "The default location is recommended. Administrator permission is required.",
                    );
                    set_text(wizard.next, "Install");
                }
                PAGE_PROGRESS => {
                    set_text(wizard.title, "Installing");
                    set_text(wizard.subtitle, "Please wait while SuperExplorer is installed.");
                    set_text(wizard.next, "Install");
                }
                PAGE_FINISH => {
                    set_text(wizard.title, "Completed");
                    set_text(wizard.subtitle, "SuperExplorer was installed successfully.");
                    set_text(
                        wizard.body,
                        &format!("Installed to:\n{}", wizard.install_dir.display()),
                    );
                    set_text(wizard.next, "Finish");
                    set_text(wizard.cancel, "Close");
                }
                PAGE_ERROR => {
                    set_text(wizard.title, "Setup failed");
                    set_text(wizard.subtitle, "SuperExplorer could not be installed.");
                    set_text(wizard.body, &wizard.error);
                    set_text(wizard.next, "Close");
                }
                _ => {}
            }
        }
    });
}

unsafe fn show(hwnd: HWND, visible: bool) {
    let _ = ShowWindow(hwnd, if visible { SW_SHOW } else { windows::Win32::UI::WindowsAndMessaging::SW_HIDE });
}

unsafe fn set_text(hwnd: HWND, text: &str) {
    let text = wide(text);
    let _ = SetWindowTextW(hwnd, PCWSTR(text.as_ptr()));
}

unsafe fn on_command(hwnd: HWND, id: i32) {
    match id {
        IDC_CANCEL => {
            if !is_working() {
                DestroyWindow(hwnd).ok();
            }
        }
        IDC_BACK => {
            WIZARD.with(|slot| {
                if let Some(wizard) = slot.borrow_mut().as_mut()
                    && wizard.page == PAGE_LOCATION
                {
                    wizard.page = PAGE_WELCOME;
                }
            });
            apply_page();
        }
        IDC_BROWSE => {
            if let Some(path) = browse_folder(hwnd) {
                WIZARD.with(|slot| {
                    if let Some(wizard) = slot.borrow_mut().as_mut() {
                        wizard.install_dir = path.clone();
                        unsafe { set_text(wizard.path, &path.display().to_string()) }
                    }
                });
            }
        }
        IDC_NEXT => {
            let page = WIZARD.with(|slot| slot.borrow().as_ref().map(|wizard| wizard.page).unwrap_or(0));
            match page {
                PAGE_WELCOME => {
                    WIZARD.with(|slot| {
                        if let Some(wizard) = slot.borrow_mut().as_mut() {
                            wizard.page = PAGE_LOCATION;
                        }
                    });
                    apply_page();
                }
                PAGE_LOCATION => start_install(hwnd),
                PAGE_FINISH => {
                    launch_if_checked();
                    DestroyWindow(hwnd).ok();
                }
                PAGE_ERROR => {
                    DestroyWindow(hwnd).ok();
                }
                _ => {}
            }
        }
        _ => {}
    }
}

fn launch_if_checked() {
    use windows::Win32::UI::WindowsAndMessaging::BM_GETCHECK;
    WIZARD.with(|slot| {
        if let Some(wizard) = slot.borrow().as_ref() {
            let checked = unsafe { SendMessageW(wizard.launch, BM_GETCHECK, None, None).0 } != 0;
            if checked {
                let exe = wizard.install_dir.join("SuperExplorer.exe");
                let _ = std::process::Command::new(exe).spawn();
            }
        }
    });
}

fn start_install(hwnd: HWND) {
    let install_dir = WIZARD.with(|slot| {
        slot.borrow_mut().as_mut().map(|wizard| {
            wizard.working = true;
            wizard.page = PAGE_PROGRESS;
            wizard.install_dir.clone()
        })
    });
    apply_page();
    let Some(install_dir) = install_dir else { return };
    let hwnd_bits = hwnd.0 as usize;
    thread::spawn(move || {
        let hwnd = HWND(hwnd_bits as *mut c_void);
        let result = install_to(install_dir, false, |status, percent| {
            let text = wide(status);
            unsafe {
                let _ = SendMessageW(
                    hwnd,
                    WM_PROGRESS,
                    Some(WPARAM(percent as usize)),
                    Some(LPARAM(text.as_ptr() as isize)),
                );
            }
            let _ = text;
        });
        let ok = result.is_ok();
        let error = result.err().map(|error| error.to_string()).unwrap_or_default();
        let wide_error = wide(&error);
        unsafe {
            let _ = SendMessageW(
                hwnd,
                WM_FINISHED,
                Some(WPARAM(usize::from(ok))),
                Some(LPARAM(if ok { 0 } else { wide_error.as_ptr() as isize })),
            );
        }
        let _ = wide_error;
    });
}

unsafe fn update_progress(hwnd: HWND, percent: u32, lparam: LPARAM) {
    let _ = hwnd;
    WIZARD.with(|slot| {
        if let Some(wizard) = slot.borrow().as_ref() {
            let _ = SendMessageW(wizard.progress, PBM_SETPOS, Some(WPARAM(percent as usize)), None);
            if lparam.0 != 0 {
                let _ = SetWindowTextW(wizard.status, PCWSTR(lparam.0 as *const u16));
            }
        }
    });
}

unsafe fn finish_install(_hwnd: HWND, ok: bool, lparam: LPARAM) {
    WIZARD.with(|slot| {
        if let Some(wizard) = slot.borrow_mut().as_mut() {
            wizard.working = false;
            if ok {
                wizard.page = PAGE_FINISH;
            } else {
                wizard.page = PAGE_ERROR;
                if lparam.0 != 0 {
                    wizard.error = string_from_pcwstr(PCWSTR(lparam.0 as *const u16));
                }
            }
        }
    });
    apply_page();
}

fn string_from_pcwstr(value: PCWSTR) -> String {
    unsafe {
        if value.0.is_null() {
            return String::new();
        }
        let mut len = 0;
        while *value.0.add(len) != 0 {
            len += 1;
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(value.0, len))
    }
}

fn browse_folder(owner: HWND) -> Option<PathBuf> {
    use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
    unsafe {
        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        dialog.SetOptions(FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM).ok()?;
        dialog.Show(Some(owner)).ok()?;
        let item: IShellItem = dialog.GetResult().ok()?;
        let name = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        Some(PathBuf::from(name.to_string().unwrap_or_default()))
    }
}
