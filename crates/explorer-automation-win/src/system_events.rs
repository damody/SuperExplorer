//! Clipboard, session, power, display, device, and network event source.

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
    Foundation::{ERROR_SUCCESS, HANDLE, HWND, LPARAM, WPARAM},
    NetworkManagement::IpHelper::{
        CancelMibChangeNotify2, MIB_IPINTERFACE_ROW, MIB_NOTIFICATION_TYPE, NotifyIpInterfaceChange,
    },
    Networking::WinSock::AF_UNSPEC,
    System::{
        DataExchange::{
            AddClipboardFormatListener, IsClipboardFormatAvailable, RemoveClipboardFormatListener,
        },
        Ole::{CF_HDROP, CF_UNICODETEXT},
        RemoteDesktop::{
            NOTIFY_FOR_THIS_SESSION, WTSRegisterSessionNotification,
            WTSUnRegisterSessionNotification,
        },
    },
    UI::WindowsAndMessaging::{
        DBT_DEVICEARRIVAL, DBT_DEVICEREMOVECOMPLETE, PBT_APMRESUMEAUTOMATIC, PBT_APMSUSPEND,
        WM_CLIPBOARDUPDATE, WM_DEVICECHANGE, WM_DISPLAYCHANGE, WM_DPICHANGED, WM_POWERBROADCAST,
        WM_WTSSESSION_CHANGE, WTS_SESSION_LOCK, WTS_SESSION_UNLOCK,
    },
};

static SYSTEM_BRIDGE: OnceLock<Mutex<Option<Arc<EventBridge>>>> = OnceLock::new();

/// Registers native notifications for an application-owned message window.
pub struct SystemEventSource {
    hwnd: HWND,
    network: HANDLE,
    bridge: Arc<EventBridge>,
}

impl SystemEventSource {
    /// Attaches listeners to an existing application window.
    ///
    /// # Errors
    ///
    /// Returns unavailable for duplicate registration or a native listener failure.
    pub fn start(hwnd: HWND, bridge: Arc<EventBridge>) -> AutomationResult<Self> {
        let slot = SYSTEM_BRIDGE.get_or_init(|| Mutex::new(None));
        let mut active = slot.lock().map_err(|_| system_error())?;
        if active.is_some() {
            return Err(system_error());
        }
        // SAFETY: hwnd is owned by the application and remains valid for this service lifetime.
        unsafe { AddClipboardFormatListener(hwnd) }.map_err(|_| system_error())?;
        // SAFETY: hwnd is owned by the application and receives session messages until Drop.
        if unsafe { WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION) }.is_err() {
            // SAFETY: balances the successful listener registration above.
            let _ = unsafe { RemoveClipboardFormatListener(hwnd) };
            return Err(system_error());
        }
        *active = Some(Arc::clone(&bridge));
        let mut network = HANDLE::default();
        // SAFETY: callback is static, output handle is valid, and global bridge owns callback state.
        let status = unsafe {
            NotifyIpInterfaceChange(
                AF_UNSPEC,
                Some(network_callback),
                None,
                false,
                &raw mut network,
            )
        };
        if status != ERROR_SUCCESS {
            *active = None;
            // SAFETY: balances successful registrations above.
            let _ = unsafe { WTSUnRegisterSessionNotification(hwnd) };
            // SAFETY: balances successful registrations above.
            let _ = unsafe { RemoveClipboardFormatListener(hwnd) };
            return Err(system_error());
        }
        Ok(Self {
            hwnd,
            network,
            bridge,
        })
    }

    /// Translates a message received by the registered application window.
    pub fn handle_message(&self, message: u32, wparam: WPARAM, _lparam: LPARAM) -> bool {
        match message {
            WM_CLIPBOARDUPDATE => {
                self.publish("clipboard.changed", EventSource::Clipboard);
                // SAFETY: availability queries do not open or retain clipboard data.
                if unsafe { IsClipboardFormatAvailable(u32::from(CF_UNICODETEXT.0)) }.is_ok() {
                    self.publish("clipboard.text_available", EventSource::Clipboard);
                }
                // SAFETY: availability queries do not open or retain clipboard data.
                if unsafe { IsClipboardFormatAvailable(u32::from(CF_HDROP.0)) }.is_ok() {
                    self.publish("clipboard.files_available", EventSource::Clipboard);
                }
                true
            }
            WM_WTSSESSION_CHANGE => {
                let name = match u32::try_from(wparam.0).unwrap_or_default() {
                    WTS_SESSION_LOCK => Some("system.session_locked"),
                    WTS_SESSION_UNLOCK => Some("system.session_unlocked"),
                    _ => None,
                };
                if let Some(name) = name {
                    self.publish(name, EventSource::System);
                }
                name.is_some()
            }
            WM_POWERBROADCAST => {
                let name = match u32::try_from(wparam.0).unwrap_or_default() {
                    PBT_APMSUSPEND => Some("system.suspend"),
                    PBT_APMRESUMEAUTOMATIC => Some("system.resume"),
                    _ => None,
                };
                if let Some(name) = name {
                    self.publish(name, EventSource::System);
                }
                name.is_some()
            }
            WM_DISPLAYCHANGE => {
                self.publish("system.display_changed", EventSource::System);
                true
            }
            WM_DPICHANGED => {
                self.publish("system.dpi_changed", EventSource::System);
                true
            }
            WM_DEVICECHANGE => {
                let name = match u32::try_from(wparam.0).unwrap_or_default() {
                    DBT_DEVICEARRIVAL => Some("system.device_arrived"),
                    DBT_DEVICEREMOVECOMPLETE => Some("system.device_removed"),
                    _ => None,
                };
                if let Some(name) = name {
                    self.publish(name, EventSource::System);
                }
                name.is_some()
            }
            _ => false,
        }
    }

    fn publish(&self, name: &str, source: EventSource) {
        publish(&self.bridge, name, source);
    }
}

impl Drop for SystemEventSource {
    fn drop(&mut self) {
        // SAFETY: handle was returned by NotifyIpInterfaceChange.
        let _ = unsafe { CancelMibChangeNotify2(self.network) };
        // SAFETY: balances registration for the same live application HWND.
        let _ = unsafe { WTSUnRegisterSessionNotification(self.hwnd) };
        // SAFETY: balances registration for the same live application HWND.
        let _ = unsafe { RemoveClipboardFormatListener(self.hwnd) };
        if let Some(slot) = SYSTEM_BRIDGE.get()
            && let Ok(mut active) = slot.lock()
        {
            *active = None;
        }
    }
}

impl std::fmt::Debug for SystemEventSource {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SystemEventSource")
            .finish_non_exhaustive()
    }
}

unsafe extern "system" fn network_callback(
    _context: *const core::ffi::c_void,
    _row: *const MIB_IPINTERFACE_ROW,
    _notification: MIB_NOTIFICATION_TYPE,
) {
    if let Some(bridge) = SYSTEM_BRIDGE
        .get()
        .and_then(|slot| slot.try_lock().ok())
        .and_then(|active| active.clone())
    {
        publish(&bridge, "system.network_changed", EventSource::System);
    }
}

fn publish(bridge: &EventBridge, name: &str, source: EventSource) {
    let _ = bridge.emit(
        name,
        now_ms(),
        source,
        EventContext {
            script_id: None,
            handler_id: None,
            task_id: None,
            correlation_id: CorrelationId::new(),
            window_id: None,
            tab_id: None,
            cwd: None,
        },
        AutomationEventData::None,
    );
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
        })
}

fn system_error() -> AutomationError {
    AutomationError::new(
        AutomationErrorKind::Unavailable,
        "system_events.start",
        true,
        "Windows system event observation could not be started",
    )
}

#[cfg(test)]
mod tests {
    use std::{mem::ManuallyDrop, sync::Arc};

    use explorer_automation::{EventBridge, fakes::FakeEventSink};
    use windows::Win32::{
        Foundation::{HANDLE, HWND, LPARAM, WPARAM},
        UI::WindowsAndMessaging::{WM_CLIPBOARDUPDATE, WM_DISPLAYCHANGE, WM_POWERBROADCAST},
    };

    use super::SystemEventSource;

    #[test]
    fn system_messages_translate_without_copying_payload_content() {
        let sink = Arc::new(FakeEventSink::new(8).expect("sink"));
        let bridge = Arc::new(EventBridge::new(sink.clone()));
        let source = ManuallyDrop::new(SystemEventSource {
            hwnd: HWND::default(),
            network: HANDLE::default(),
            bridge,
        });
        assert!(source.handle_message(WM_DISPLAYCHANGE, WPARAM(0), LPARAM(0)));
        assert_eq!(
            sink.pop().expect("pop").expect("event").name.as_str(),
            "system.display_changed"
        );
        assert!(source.handle_message(WM_CLIPBOARDUPDATE, WPARAM(0), LPARAM(0)));
        assert_eq!(
            sink.pop().expect("pop").expect("event").name.as_str(),
            "clipboard.changed"
        );
        assert!(!source.handle_message(WM_POWERBROADCAST, WPARAM(0), LPARAM(0)));
    }
}
