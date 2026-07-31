//! Observation-only `WinEvent` window lifecycle source.

#![allow(unsafe_code)]

use std::{
    sync::{Arc, Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

use explorer_automation::{
    AutomationError, AutomationErrorKind, AutomationEventData, AutomationResult, CorrelationId,
    EventBridge, EventContext, EventSource,
};
use windows::Win32::{
    Foundation::HWND,
    UI::{
        Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent},
        WindowsAndMessaging::{
            EVENT_MAX, EVENT_MIN, EVENT_OBJECT_CREATE, EVENT_OBJECT_DESTROY, EVENT_OBJECT_HIDE,
            EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_NAMECHANGE, EVENT_OBJECT_SHOW,
            EVENT_SYSTEM_FOREGROUND, GetWindowTextLengthW, GetWindowThreadProcessId,
            WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS,
        },
    },
};

static BRIDGE: OnceLock<Mutex<Option<Arc<EventBridge>>>> = OnceLock::new();

/// Global observation hook. It never blocks, modifies, or suppresses a window event.
pub struct WindowEventHook {
    hook: HWINEVENTHOOK,
}

impl WindowEventHook {
    /// Installs one process-wide out-of-context hook.
    ///
    /// # Errors
    ///
    /// Returns unavailable if a hook is already active or Windows rejects installation.
    pub fn start(bridge: Arc<EventBridge>) -> AutomationResult<Self> {
        let state = BRIDGE.get_or_init(|| Mutex::new(None));
        let mut active = state.lock().map_err(|_| hook_error())?;
        if active.is_some() {
            return Err(hook_error());
        }
        *active = Some(bridge);
        // SAFETY: callback has the required system ABI and remains static for the hook lifetime.
        let hook = unsafe {
            SetWinEventHook(
                EVENT_MIN,
                EVENT_MAX,
                None,
                Some(win_event_callback),
                0,
                0,
                WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
            )
        };
        if hook.is_invalid() {
            *active = None;
            return Err(hook_error());
        }
        Ok(Self { hook })
    }
}

impl Drop for WindowEventHook {
    fn drop(&mut self) {
        if let Some(state) = BRIDGE.get()
            && let Ok(mut active) = state.lock()
        {
            *active = None;
        }
        // SAFETY: this value owns the successfully installed hook.
        let _ = unsafe { UnhookWinEvent(self.hook) };
    }
}

impl std::fmt::Debug for WindowEventHook {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WindowEventHook")
            .finish_non_exhaustive()
    }
}

unsafe extern "system" fn win_event_callback(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    object_id: i32,
    _child_id: i32,
    _event_thread: u32,
    _event_time: u32,
) {
    if hwnd.0.is_null() || object_id != 0 {
        return;
    }
    let Some(name) = event_name(event) else {
        return;
    };
    let Some(bridge) = BRIDGE
        .get()
        .and_then(|state| state.try_lock().ok())
        .and_then(|active| active.clone())
    else {
        return;
    };
    let mut process_id = 0_u32;
    // SAFETY: hwnd was supplied by the current WinEvent callback.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&raw mut process_id)) };
    // SAFETY: length query does not write or retain pointers.
    let title_units = unsafe { GetWindowTextLengthW(hwnd) };
    let native_window_id = u64::try_from(hwnd.0.addr()).unwrap_or(u64::MAX);
    let _ = bridge.emit(
        name,
        now_ms(),
        EventSource::Window,
        EventContext {
            script_id: None,
            handler_id: None,
            task_id: None,
            correlation_id: CorrelationId::new(),
            window_id: Some(native_window_id),
            tab_id: None,
            cwd: None,
        },
        AutomationEventData::Window {
            native_window_id,
            process_id: (process_id != 0).then_some(process_id),
            title_utf8_bytes: usize::try_from(title_units).ok(),
        },
    );
}

const fn event_name(event: u32) -> Option<&'static str> {
    match event {
        EVENT_SYSTEM_FOREGROUND => Some("system.foreground_changed"),
        EVENT_OBJECT_CREATE => Some("system.window_created"),
        EVENT_OBJECT_DESTROY => Some("system.window_destroyed"),
        EVENT_OBJECT_SHOW => Some("system.window_shown"),
        EVENT_OBJECT_HIDE => Some("system.window_hidden"),
        EVENT_OBJECT_LOCATIONCHANGE => Some("system.window_location_changed"),
        EVENT_OBJECT_NAMECHANGE => Some("system.window_title_changed"),
        _ => None,
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
        })
}

fn hook_error() -> AutomationError {
    AutomationError::new(
        AutomationErrorKind::Unavailable,
        "window_events.start",
        true,
        "Global window observation could not be started",
    )
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use explorer_automation::{EventBridge, fakes::FakeEventSink};
    use windows::Win32::UI::WindowsAndMessaging::{
        EVENT_OBJECT_CREATE, EVENT_OBJECT_NAMECHANGE, EVENT_SYSTEM_FOREGROUND,
    };

    use super::{WindowEventHook, event_name};

    #[test]
    fn supported_win_events_map_to_public_catalog_names() {
        assert_eq!(
            event_name(EVENT_SYSTEM_FOREGROUND),
            Some("system.foreground_changed")
        );
        assert_eq!(
            event_name(EVENT_OBJECT_CREATE),
            Some("system.window_created")
        );
        assert_eq!(
            event_name(EVENT_OBJECT_NAMECHANGE),
            Some("system.window_title_changed")
        );
    }

    #[test]
    fn window_hook_unloads_and_can_restart() {
        let bridge = Arc::new(EventBridge::new(Arc::new(
            FakeEventSink::new(16).expect("sink"),
        )));
        let first = WindowEventHook::start(Arc::clone(&bridge)).expect("first hook");
        drop(first);
        let second = WindowEventHook::start(bridge).expect("second hook");
        drop(second);
    }
}
