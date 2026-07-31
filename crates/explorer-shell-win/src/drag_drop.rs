//! Native OLE drag source plus drop-effect and operation adapters.
#![allow(
    unsafe_code,
    reason = "OLE drag COM implementations require audited ABI methods and pointer calls"
)]
#![allow(
    clippy::inline_always,
    clippy::ref_as_ptr,
    reason = "windows-rs implement macro generates the COM identity glue"
)]

use explorer_common::{ExplorerError, ExplorerErrorKind};
use explorer_model::{
    DragButton, DragEffect, DragModifiers, FileOperationFlags, FileOperationKind,
    FileOperationRequest, ItemDescriptor, LocationDescriptor, OperationTerminal, TransferEffects,
    negotiate_effect,
};
use windows::{
    Win32::{
        Foundation::{DRAGDROP_S_CANCEL, DRAGDROP_S_DROP, DRAGDROP_S_USEDEFAULTCURSORS},
        System::{
            Ole::{
                DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_LINK, DROPEFFECT_MOVE, DoDragDrop,
                IDropSource, IDropSource_Impl,
            },
            SystemServices::{MK_LBUTTON, MK_RBUTTON, MODIFIERKEYS_FLAGS},
            Threading::{
                AttachThreadInput, GR_GDIOBJECTS, GR_USEROBJECTS, GetCurrentProcess,
                GetCurrentThreadId, GetGuiResources, GetProcessHandleCount,
            },
        },
        UI::{
            Input::KeyboardAndMouse::{GetKeyState, VK_CONTROL, VK_SHIFT},
            WindowsAndMessaging::{
                GetForegroundWindow, GetSystemMetrics, GetWindowThreadProcessId, SM_CXDRAG,
                SM_CYDRAG,
            },
        },
    },
    core::{BOOL, HRESULT, implement},
};

static ACTIVE_NATIVE_DRAGS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

struct ActiveDragGuard;
impl Drop for ActiveDragGuard {
    fn drop(&mut self) {
        ACTIVE_NATIVE_DRAGS.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
    }
}

struct InputQueueAttachment {
    current: u32,
    foreground: u32,
    attached: bool,
}

impl InputQueueAttachment {
    fn to_foreground() -> Self {
        // OLE drag tracking must observe the same input queue that owns the source window.
        // The Shell worker is an STA, but it is not GPUI's window thread, so explicitly join
        // the foreground queue for the duration of the nested DoDragDrop loop.
        let current = unsafe { GetCurrentThreadId() };
        let foreground = unsafe { GetWindowThreadProcessId(GetForegroundWindow(), None) };
        let attached = foreground != 0
            && foreground != current
            && unsafe { AttachThreadInput(current, foreground, true) }.as_bool();
        Self {
            current,
            foreground,
            attached,
        }
    }
}

impl Drop for InputQueueAttachment {
    fn drop(&mut self) {
        if self.attached {
            // SAFETY: balances this guard's successful AttachThreadInput call.
            let _ = unsafe { AttachThreadInput(self.current, self.foreground, false) };
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SystemDragThreshold {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DragResourceSnapshot {
    pub handles: u32,
    pub gdi_objects: u32,
    pub user_objects: u32,
    pub active_native_drags: usize,
}

impl DragResourceSnapshot {
    /// Captures current process handle, GDI object, and USER object counts.
    ///
    /// # Errors
    ///
    /// Returns a recoverable error if Windows cannot read the process handle count.
    pub fn capture() -> Result<Self, ExplorerError> {
        // SAFETY: pseudo-handle is process-local and requires no close.
        let process = unsafe { GetCurrentProcess() };
        let mut handles = 0;
        // SAFETY: output points to initialized writable storage for the duration of the call.
        unsafe { GetProcessHandleCount(process, &raw mut handles) }.map_err(|error| {
            drag_error(
                "capture drag resources",
                "無法量測拖放資源",
                error.to_string(),
            )
        })?;
        // SAFETY: process pseudo-handle is valid and flags select read-only counters.
        let gdi_objects = unsafe { GetGuiResources(process, GR_GDIOBJECTS) };
        // SAFETY: process pseudo-handle is valid and flags select read-only counters.
        let user_objects = unsafe { GetGuiResources(process, GR_USEROBJECTS) };
        Ok(Self {
            handles,
            gdi_objects,
            user_objects,
            active_native_drags: ACTIVE_NATIVE_DRAGS.load(std::sync::atomic::Ordering::Acquire),
        })
    }
}

impl SystemDragThreshold {
    pub fn current() -> Self {
        // SAFETY: GetSystemMetrics has no pointer or ownership preconditions.
        let x = unsafe { GetSystemMetrics(SM_CXDRAG) }.max(1);
        // SAFETY: GetSystemMetrics has no pointer or ownership preconditions.
        let y = unsafe { GetSystemMetrics(SM_CYDRAG) }.max(1);
        Self { x, y }
    }

    #[allow(
        clippy::cast_precision_loss,
        reason = "Windows drag metrics and supported DPI percentages are tiny integral values"
    )]
    pub fn logical(self, dpi_percent: u32) -> (f32, f32) {
        let scale = dpi_percent.max(1) as f32 / 100.0;
        (self.x as f32 / scale, self.y as f32 / scale)
    }

    #[allow(
        clippy::cast_precision_loss,
        reason = "Windows drag metrics are small integral values"
    )]
    pub fn logical_at_scale(self, scale: f32) -> (f32, f32) {
        let scale = if scale.is_finite() {
            scale.max(0.01)
        } else {
            1.0
        };
        (self.x as f32 / scale, self.y as f32 / scale)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RightDragChoice {
    Copy,
    Move,
    Cancel,
}

pub fn dropped_file_operation(
    items: Vec<ItemDescriptor>,
    destination: LocationDescriptor,
    effect: DragEffect,
) -> Option<FileOperationRequest> {
    let kind = match effect {
        DragEffect::Copy => FileOperationKind::Copy { items, destination },
        DragEffect::Move => FileOperationKind::Move { items, destination },
        DragEffect::None | DragEffect::Link => return None,
    };
    Some(FileOperationRequest {
        kind,
        flags: FileOperationFlags::default(),
    })
}

pub fn external_drop_request(
    sources: &[LocationDescriptor],
    destination: LocationDescriptor,
    effect: DragEffect,
    conflict: explorer_model::ConflictDecision,
) -> Result<FileOperationRequest, ExplorerError> {
    let mut items = Vec::with_capacity(sources.len());
    for source in sources {
        let path = source.path().ok_or_else(|| {
            drag_error(
                "resolve external drop",
                "拖曳來源不是可用的檔案系統項目",
                "non-filesystem source",
            )
        })?;
        let metadata = std::fs::symlink_metadata(path).map_err(|error| {
            drag_error(
                "resolve external drop",
                "無法讀取拖曳來源",
                error.to_string(),
            )
        })?;
        let identity = crate::navigation::filesystem_identity(path, metadata.is_dir())?;
        let id = explorer_model::ShellItemId::from_provider_bytes(identity).ok_or_else(|| {
            drag_error(
                "resolve external drop",
                "The dropped item could not be identified.",
                "native filesystem identity was empty",
            )
        })?;
        items.push(ItemDescriptor {
            id,
            location: source.clone(),
        });
    }
    let mut request = dropped_file_operation(items, destination, effect).ok_or_else(|| {
        drag_error(
            "resolve external drop",
            "此拖放效果尚未支援",
            "none/link cannot create copy or move operation",
        )
    })?;
    request.flags.conflict = conflict;
    Ok(request)
}

pub fn choose_right_drag_effect(choice: RightDragChoice, allowed: TransferEffects) -> DragEffect {
    match choice {
        RightDragChoice::Copy if allowed.copy => DragEffect::Copy,
        RightDragChoice::Move if allowed.move_item => DragEffect::Move,
        RightDragChoice::Copy | RightDragChoice::Move | RightDragChoice::Cancel => DragEffect::None,
    }
}

pub fn modifiers_from_key_state(state: MODIFIERKEYS_FLAGS) -> DragModifiers {
    DragModifiers {
        control: state.0 & 0x0008 != 0,
        shift: state.0 & 0x0004 != 0,
        alt: false,
    }
}

pub fn negotiate_native_effect(
    allowed: DROPEFFECT,
    preferred: DragEffect,
    key_state: MODIFIERKEYS_FLAGS,
    can_write: bool,
) -> DROPEFFECT {
    let domain_allowed = TransferEffects {
        copy: allowed.0 & DROPEFFECT_COPY.0 != 0,
        move_item: allowed.0 & DROPEFFECT_MOVE.0 != 0,
        link: allowed.0 & DROPEFFECT_LINK.0 != 0,
    };
    effect_to_native(negotiate_effect(
        domain_allowed,
        preferred,
        modifiers_from_key_state(key_state),
        can_write,
    ))
}

#[implement(IDropSource)]
struct NativeDropSource {
    button: DragButton,
    cancellation: explorer_model::CancellationToken,
}

impl IDropSource_Impl for NativeDropSource_Impl {
    fn QueryContinueDrag(&self, escape_pressed: BOOL, key_state: MODIFIERKEYS_FLAGS) -> HRESULT {
        if escape_pressed.as_bool() || self.cancellation.is_cancelled() {
            return DRAGDROP_S_CANCEL;
        }
        let required = match self.button {
            DragButton::Left => MK_LBUTTON,
            DragButton::Right => MK_RBUTTON,
        };
        if key_state.0 & required.0 == 0 {
            DRAGDROP_S_DROP
        } else {
            HRESULT(0)
        }
    }

    fn GiveFeedback(&self, _effect: DROPEFFECT) -> HRESULT {
        DRAGDROP_S_USEDEFAULTCURSORS
    }
}

pub fn begin_native_drag(
    items: &[ItemDescriptor],
    allowed: TransferEffects,
    button: DragButton,
    cancellation: explorer_model::CancellationToken,
) -> Result<OperationTerminal, ExplorerError> {
    if items.is_empty() {
        return Err(drag_error(
            "begin drag",
            "沒有可拖曳的選取項目",
            "empty selection",
        ));
    }
    let data = crate::clipboard::create_shell_data_object(items)?;
    // CFSTR_PREFERREDDROPEFFECT is inspected by Explorer on its first DragEnter. Capture the
    // real modifier state when the drag crosses the threshold so Ctrl-drag and Shift-drag do
    // not inherit the unmodified same-volume default.
    let control = unsafe { GetKeyState(VK_CONTROL.0 as i32) } < 0;
    let shift = unsafe { GetKeyState(VK_SHIFT.0 as i32) } < 0;
    let preferred = preferred_effect_for_drag(allowed, control, shift);
    crate::clipboard::set_drop_effect(
        &data,
        windows::Win32::UI::Shell::CFSTR_PREFERREDDROPEFFECT,
        preferred,
    )?;
    let source: IDropSource = NativeDropSource {
        button,
        cancellation,
    }
    .into();
    let allowed = effects_to_native(allowed);
    if allowed.0 == 0 {
        return Err(drag_error(
            "begin drag",
            "來源未提供可用的拖放效果",
            "no allowed effects",
        ));
    }
    let mut performed = DROPEFFECT(0);
    ACTIVE_NATIVE_DRAGS.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
    let _active_drag = ActiveDragGuard;
    let _input_queue = InputQueueAttachment::to_foreground();
    // SAFETY: both COM interfaces remain alive for the nested OLE loop and output is writable.
    let result = unsafe { DoDragDrop(&data, &source, allowed, &raw mut performed) };
    tracing::info!(
        hresult = format_args!("{:#010x}", result.0),
        performed_effect = performed.0,
        allowed_effects = allowed.0,
        "OLE drag source reached a terminal result"
    );
    if result == DRAGDROP_S_CANCEL || performed.0 == 0 {
        Ok(OperationTerminal::Cancelled)
    } else if result == DRAGDROP_S_DROP {
        Ok(OperationTerminal::Finished)
    } else {
        Err(drag_error(
            "OLE DoDragDrop",
            "拖放工作階段失敗，請重試",
            format!("HRESULT={:#010x}", result.0),
        ))
    }
}

const fn preferred_effect_for_drag(allowed: TransferEffects, control: bool, shift: bool) -> u32 {
    if control && allowed.copy {
        DROPEFFECT_COPY.0
    } else if shift && allowed.move_item {
        DROPEFFECT_MOVE.0
    } else if allowed.move_item {
        DROPEFFECT_MOVE.0
    } else if allowed.copy {
        DROPEFFECT_COPY.0
    } else {
        DROPEFFECT_LINK.0
    }
}

const fn effects_to_native(effects: TransferEffects) -> DROPEFFECT {
    let mut bits = 0;
    if effects.copy {
        bits |= DROPEFFECT_COPY.0;
    }
    if effects.move_item {
        bits |= DROPEFFECT_MOVE.0;
    }
    if effects.link {
        bits |= DROPEFFECT_LINK.0;
    }
    DROPEFFECT(bits)
}

const fn effect_to_native(effect: DragEffect) -> DROPEFFECT {
    match effect {
        DragEffect::None => DROPEFFECT(0),
        DragEffect::Copy => DROPEFFECT_COPY,
        DragEffect::Move => DROPEFFECT_MOVE,
        DragEffect::Link => DROPEFFECT_LINK,
    }
}

fn drag_error(
    operation: &'static str,
    user_message: &'static str,
    source: impl Into<String>,
) -> ExplorerError {
    ExplorerError::new(
        ExplorerErrorKind::Availability,
        operation,
        true,
        user_message,
        source,
    )
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        process::Command,
        sync::{
            Arc,
            atomic::{AtomicU8, Ordering as AtomicOrdering},
        },
        time::{Duration as TestDuration, Instant as TestInstant},
    };

    use explorer_test_support::OwnedTempFixture;
    use windows::Win32::{
        Foundation::{DRAGDROP_S_CANCEL, DRAGDROP_S_DROP, LPARAM, WPARAM},
        System::{
            Ole::{OleInitialize, OleUninitialize},
            Threading::GetCurrentThreadId,
        },
        UI::{
            Input::KeyboardAndMouse::{MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, mouse_event},
            WindowsAndMessaging::{PostThreadMessageW, SetCursorPos, WM_MOUSEMOVE},
        },
    };

    use super::*;

    #[test]
    fn preferred_effect_tracks_explorer_ctrl_shift_drag_semantics() {
        let all = TransferEffects {
            copy: true,
            move_item: true,
            link: true,
        };
        assert_eq!(
            preferred_effect_for_drag(all, false, false),
            DROPEFFECT_MOVE.0
        );
        assert_eq!(
            preferred_effect_for_drag(all, true, false),
            DROPEFFECT_COPY.0
        );
        assert_eq!(
            preferred_effect_for_drag(all, false, true),
            DROPEFFECT_MOVE.0
        );
        assert_eq!(
            preferred_effect_for_drag(TransferEffects::COPY, false, true),
            DROPEFFECT_COPY.0
        );
    }

    #[implement(IDropSource)]
    struct ControlledDropSource {
        terminal: Arc<AtomicU8>,
    }

    impl IDropSource_Impl for ControlledDropSource_Impl {
        fn QueryContinueDrag(&self, _escape: BOOL, _state: MODIFIERKEYS_FLAGS) -> HRESULT {
            match self.terminal.load(AtomicOrdering::Acquire) {
                1 => DRAGDROP_S_DROP,
                2 => DRAGDROP_S_CANCEL,
                _ => HRESULT(0),
            }
        }

        fn GiveFeedback(&self, _effect: DROPEFFECT) -> HRESULT {
            DRAGDROP_S_USEDEFAULTCURSORS
        }
    }

    fn real_item(path: &std::path::Path) -> ItemDescriptor {
        let identity = crate::navigation::filesystem_identity(path, false).expect("identity");
        ItemDescriptor {
            id: explorer_model::ShellItemId::from_provider_bytes(identity)
                .expect("non-empty identity"),
            location: LocationDescriptor::file_system(path),
        }
    }

    fn open_real_explorer_target(path: &std::path::Path) -> (i32, i32) {
        let script = r#"
$path=$env:EXPLORER_DROP_TARGET
$shell=New-Object -ComObject Shell.Application
$shell.Explore($path)
$deadline=(Get-Date).AddSeconds(10); $window=$null
do { Start-Sleep -Milliseconds 100; $window=@($shell.Windows()) | Where-Object { try { $_.Document.Folder.Self.Path -eq $path } catch {} } | Select-Object -First 1 } while($null -eq $window -and (Get-Date) -lt $deadline)
if($null -eq $window){ exit 2 }
Add-Type -TypeDefinition 'using System; using System.Runtime.InteropServices; public static class Native { [StructLayout(LayoutKind.Sequential)] public struct R { public int L,T,Rt,B; } [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h,IntPtr a,int x,int y,int w,int z,uint f); [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h); [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow(); [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h,out R r); [DllImport("user32.dll")] public static extern void keybd_event(byte k,byte s,uint f,UIntPtr e); public static bool Focus(IntPtr h){ keybd_event(0x12,0,0,UIntPtr.Zero); SetForegroundWindow(h); keybd_event(0x12,0,2,UIntPtr.Zero); return GetForegroundWindow()==h; } }'
[void][Native]::SetWindowPos([IntPtr]$window.HWND,[IntPtr]::Zero,220,120,1000,760,0x0040)
if(-not [Native]::Focus([IntPtr]$window.HWND)){ exit 3 }
Start-Sleep -Milliseconds 300
$rect=New-Object Native+R; [void][Native]::GetWindowRect([IntPtr]$window.HWND,[ref]$rect)
$x=[int]($rect.L+($rect.Rt-$rect.L)*0.72); $y=[int]($rect.T+($rect.B-$rect.T)*0.68); Write-Output "$x,$y"
"#;
        let output = Command::new("powershell.exe")
            .env(
                "EXPLORER_DROP_TARGET",
                crate::navigation::shell_path_text(path),
            )
            .args(["-STA", "-NoProfile", "-Command", script])
            .output()
            .expect("open real Explorer target");
        assert!(
            output.status.success(),
            "Explorer target failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let coordinates = String::from_utf8_lossy(&output.stdout);
        let mut parts = coordinates.trim().split(',');
        (
            parts.next().expect("x").parse().expect("numeric x"),
            parts.next().expect("y").parse().expect("numeric y"),
        )
    }

    fn controlled_real_explorer_drop(
        items: &[ItemDescriptor],
        destination: &std::path::Path,
        effect: DROPEFFECT,
        terminal: u8,
    ) -> DROPEFFECT {
        let (x, y) = open_real_explorer_target(destination);
        // SAFETY: the point is inside the foreground Explorer file view.
        unsafe { SetCursorPos(x, y) }.expect("position cursor over Explorer target");
        let data = crate::clipboard::create_shell_data_object(items).expect("Shell IDataObject");
        crate::clipboard::set_drop_effect(
            &data,
            windows::Win32::UI::Shell::CFSTR_PREFERREDDROPEFFECT,
            effect.0,
        )
        .expect("preferred effect");
        let state = Arc::new(AtomicU8::new(0));
        let source: IDropSource = ControlledDropSource {
            terminal: state.clone(),
        }
        .into();
        // SAFETY: reads the current initialized OLE test STA id.
        let drag_thread = unsafe { GetCurrentThreadId() };
        let release = std::thread::spawn(move || {
            std::thread::sleep(TestDuration::from_millis(250));
            // SAFETY: a one-pixel real cursor transition makes OLE enter the
            // foreground Explorer target before the controlled terminal signal.
            let _ = unsafe { SetCursorPos(x + 2, y) };
            let _ = unsafe { PostThreadMessageW(drag_thread, WM_MOUSEMOVE, WPARAM(0), LPARAM(0)) };
            std::thread::sleep(TestDuration::from_millis(750));
            state.store(terminal, AtomicOrdering::Release);
            // SAFETY: balances the left-button press made immediately before
            // DoDragDrop and lets Explorer observe a real terminal transition.
            unsafe { mouse_event(MOUSEEVENTF_LEFTUP, 0, 0, 0, 0) };
            for _ in 0..20 {
                // SAFETY: a value-only message wakes DoDragDrop so it queries
                // the controlled source; no input is synthesized.
                let _ =
                    unsafe { PostThreadMessageW(drag_thread, WM_MOUSEMOVE, WPARAM(0), LPARAM(0)) };
                std::thread::sleep(TestDuration::from_millis(5));
            }
        });
        let mut performed = DROPEFFECT(0);
        // SAFETY: the matching release is guaranteed by the helper thread.
        unsafe { mouse_event(MOUSEEVENTF_LEFTDOWN, 0, 0, 0, 0) };
        // SAFETY: data/source live through the nested loop and performed is writable.
        let result = unsafe { DoDragDrop(&data, &source, effect, &raw mut performed) };
        release.join().expect("controlled release");
        if terminal == 2 {
            assert_eq!(result, DRAGDROP_S_CANCEL);
        } else {
            assert_eq!(result, DRAGDROP_S_DROP);
        }
        performed
    }

    #[test]
    fn system_threshold_scales_once_for_supported_dpi_values() {
        let threshold = SystemDragThreshold { x: 8, y: 10 };
        assert_eq!(threshold.logical(100), (8.0, 10.0));
        assert_eq!(threshold.logical(200), (4.0, 5.0));
        for dpi in [100, 125, 150, 200] {
            let (x, y) = threshold.logical(dpi);
            assert!(x.is_finite() && y.is_finite() && x > 0.0 && y > 0.0);
        }
    }

    #[test]
    fn native_modifier_negotiation_and_right_drag_cancel_are_explicit() {
        let allowed = TransferEffects {
            copy: true,
            move_item: true,
            link: false,
        };
        assert_eq!(
            negotiate_native_effect(
                effects_to_native(allowed),
                DragEffect::Move,
                MODIFIERKEYS_FLAGS(0x0008),
                true
            ),
            DROPEFFECT_COPY
        );
        assert_eq!(
            choose_right_drag_effect(RightDragChoice::Cancel, allowed),
            DragEffect::None
        );
    }

    #[test]
    fn dropped_copy_and_move_reuse_file_operation_requests() {
        let destination = LocationDescriptor::file_system(r"C:\fixture\destination");
        assert!(
            dropped_file_operation(Vec::new(), destination.clone(), DragEffect::Copy).is_some()
        );
        assert!(dropped_file_operation(Vec::new(), destination, DragEffect::Link).is_none());
    }

    #[test]
    #[ignore = "requires an interactive Windows Explorer desktop"]
    fn real_explorer_drop_target_matrix_records_desktop_capability() {
        let _guard = crate::clipboard::CLIPBOARD_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // SAFETY: this serialized test balances OLE on the same STA.
        unsafe { OleInitialize(None) }.expect("initialize OLE");
        let fixture = OwnedTempFixture::new().expect("Explorer drop fixture");
        fixture.create_dir("source").expect("source directory");

        let copy_source = fixture
            .create_file("source/copy.txt", b"copy")
            .expect("copy source");
        let copy_target = fixture.create_dir("copy-target").expect("copy target");
        let copy_effect = controlled_real_explorer_drop(
            &[real_item(&copy_source)],
            &copy_target,
            DROPEFFECT_COPY,
            1,
        );
        if copy_effect.0 == 0 {
            eprintln!(
                "Explorer returned DROPEFFECT_NONE for the controlled desktop sequence; physical/input-driver matrix is required on this session"
            );
            // SAFETY: balances successful OleInitialize above.
            unsafe { OleUninitialize() };
            return;
        }
        assert_eq!(copy_effect, DROPEFFECT_COPY);
        let copy_deadline = TestInstant::now() + TestDuration::from_secs(10);
        while !copy_target.join("copy.txt").is_file() && TestInstant::now() < copy_deadline {
            std::thread::sleep(TestDuration::from_millis(20));
        }
        assert_eq!(
            fs::read(copy_target.join("copy.txt")).expect("copied bytes"),
            b"copy"
        );
        assert!(copy_source.is_file());

        let move_a = fixture
            .create_file("source/move-a.txt", b"move-a")
            .expect("move a");
        let move_b = fixture
            .create_file("source/move-b.txt", b"move-b")
            .expect("move b");
        let move_target = fixture.create_dir("move-target").expect("move target");
        let move_effect = controlled_real_explorer_drop(
            &[real_item(&move_a), real_item(&move_b)],
            &move_target,
            DROPEFFECT_MOVE,
            1,
        );
        assert_eq!(move_effect, DROPEFFECT_MOVE);
        let move_deadline = TestInstant::now() + TestDuration::from_secs(10);
        while (!move_target.join("move-a.txt").is_file()
            || !move_target.join("move-b.txt").is_file())
            && TestInstant::now() < move_deadline
        {
            std::thread::sleep(TestDuration::from_millis(20));
        }
        assert!(!move_a.exists() && !move_b.exists());
        assert_eq!(
            fs::read(move_target.join("move-a.txt")).expect("move a bytes"),
            b"move-a"
        );
        assert_eq!(
            fs::read(move_target.join("move-b.txt")).expect("move b bytes"),
            b"move-b"
        );

        let cancel_source = fixture
            .create_file("source/cancel.txt", b"cancel")
            .expect("cancel source");
        let cancel_target = fixture.create_dir("cancel-target").expect("cancel target");
        let cancel_effect = controlled_real_explorer_drop(
            &[real_item(&cancel_source)],
            &cancel_target,
            DROPEFFECT_COPY | DROPEFFECT_MOVE,
            2,
        );
        assert_eq!(cancel_effect.0, 0);
        assert!(cancel_source.is_file());
        assert!(!cancel_target.join("cancel.txt").exists());
        // SAFETY: balances successful OleInitialize above.
        unsafe { OleUninitialize() };
    }

    #[test]
    fn real_do_drag_drop_cancel_soak_releases_process_resources() {
        let _guard = crate::clipboard::CLIPBOARD_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // SAFETY: this test initializes and uninitializes OLE on the same serialized thread.
        unsafe { OleInitialize(None) }.expect("initialize OLE");
        let fixture = OwnedTempFixture::new().expect("drag fixture");
        let path = fixture.create_file("drag.txt", b"drag").expect("source");
        let id = explorer_model::ShellItemId::from_provider_bytes(
            crate::navigation::filesystem_identity(&path, false).expect("identity"),
        )
        .expect("non-empty identity");
        let item = ItemDescriptor {
            id,
            location: LocationDescriptor::file_system(path),
        };
        // SAFETY: reads the identifier of the current test thread.
        let drag_thread_id = unsafe { GetCurrentThreadId() };
        let cancel_once = |button| {
            let cancellation = explorer_model::CancellationToken::new();
            let cancel_signal = cancellation.clone();
            let cancel = std::thread::spawn(move || {
                let activation_deadline =
                    std::time::Instant::now() + std::time::Duration::from_secs(1);
                while ACTIVE_NATIVE_DRAGS.load(std::sync::atomic::Ordering::Acquire) == 0 {
                    assert!(
                        std::time::Instant::now() < activation_deadline,
                        "native drag did not enter its modal loop"
                    );
                    std::thread::yield_now();
                }
                cancel_signal.cancel();
                let cancellation_deadline =
                    std::time::Instant::now() + std::time::Duration::from_secs(1);
                while ACTIVE_NATIVE_DRAGS.load(std::sync::atomic::Ordering::Acquire) != 0 {
                    assert!(
                        std::time::Instant::now() < cancellation_deadline,
                        "native drag did not observe cancellation"
                    );
                    // SAFETY: a value-only mouse message wakes OLE's nested modal loop; the
                    // source's cancellation token determines the terminal result.
                    unsafe {
                        PostThreadMessageW(drag_thread_id, WM_MOUSEMOVE, WPARAM(0), LPARAM(0))
                            .expect("wake cancelled drag");
                    }
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
            });
            assert_eq!(
                begin_native_drag(
                    std::slice::from_ref(&item),
                    TransferEffects::COPY,
                    button,
                    cancellation,
                )
                .expect("bounded native drag"),
                OperationTerminal::Cancelled
            );
            cancel.join().expect("cancel sender");
        };
        for _ in 0..5 {
            cancel_once(DragButton::Left);
        }
        cancel_once(DragButton::Right);
        let before = DragResourceSnapshot::capture().expect("before resources");
        for _ in 0..25 {
            cancel_once(DragButton::Left);
        }
        let middle = DragResourceSnapshot::capture().expect("middle resources");
        for _ in 0..25 {
            cancel_once(DragButton::Left);
        }
        let after = DragResourceSnapshot::capture().expect("after resources");
        eprintln!("drag soak resources: before={before:?} middle={middle:?} after={after:?}");
        assert_eq!(after.active_native_drags, 0);
        assert!(after.handles <= middle.handles.saturating_add(64));
        assert!(after.gdi_objects <= middle.gdi_objects.saturating_add(32));
        assert!(after.user_objects <= middle.user_objects.saturating_add(32));
        // SAFETY: balances the successful OleInitialize on this thread.
        unsafe { OleUninitialize() };
    }
}
