//! Observation-only global keyboard, mouse, and chord source.

#![allow(unsafe_code)]

use std::{
    collections::{BTreeMap, HashSet},
    sync::{Arc, Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

use explorer_automation::{
    AutomationError, AutomationErrorKind, AutomationEventData, AutomationResult, CorrelationId,
    EventBridge, EventContext, EventSource,
};
use windows::Win32::{
    Foundation::{LPARAM, LRESULT, WPARAM},
    UI::{
        Input::KeyboardAndMouse::{VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT},
        WindowsAndMessaging::{
            CallNextHookEx, HHOOK, KBDLLHOOKSTRUCT, LLKHF_INJECTED, LLMHF_INJECTED, MSLLHOOKSTRUCT,
            SetWindowsHookExW, UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN,
            WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEHWHEEL,
            WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
            WM_XBUTTONDOWN, WM_XBUTTONUP,
        },
    },
};

const MOD_CTRL: u16 = 1;
const MOD_ALT: u16 = 2;
const MOD_SHIFT: u16 = 4;
const MOD_WIN: u16 = 8;

static INPUT: OnceLock<Mutex<Option<InputState>>> = OnceLock::new();

struct InputState {
    bridge: Arc<EventBridge>,
    pressed: HashSet<u32>,
    modifiers: u16,
    chords: Vec<HotkeyChord>,
}

#[derive(Clone, Debug)]
struct HotkeyChord {
    modifiers: u16,
    virtual_key: u32,
    display: String,
}

/// Installs low-level observation hooks. Every callback always calls the next hook.
pub struct InputObservationHook {
    keyboard: HHOOK,
    mouse: HHOOK,
}

impl InputObservationHook {
    /// Starts one process-wide hook and validates configured chord strings.
    ///
    /// # Errors
    ///
    /// Returns a typed error for an invalid chord, duplicate service, or hook failure.
    pub fn start(bridge: Arc<EventBridge>, chords: &[String]) -> AutomationResult<Self> {
        let chords = chords
            .iter()
            .map(|chord| parse_chord(chord))
            .collect::<AutomationResult<Vec<_>>>()?;
        let slot = INPUT.get_or_init(|| Mutex::new(None));
        let mut state = slot.lock().map_err(|_| input_error("input.start"))?;
        if state.is_some() {
            return Err(input_error("input.start"));
        }
        *state = Some(InputState {
            bridge,
            pressed: HashSet::new(),
            modifiers: 0,
            chords,
        });
        // SAFETY: callbacks use the required system ABI and have static lifetime.
        let keyboard =
            unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_callback), None, 0) }
                .map_err(|_| input_error("input.keyboard_hook"))?;
        // SAFETY: callback uses the required system ABI and has static lifetime.
        // SAFETY: callback uses the required system ABI and has static lifetime.
        let Ok(mouse) = (unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_callback), None, 0) })
        else {
            // SAFETY: keyboard was successfully installed above.
            let _ = unsafe { UnhookWindowsHookEx(keyboard) };
            *state = None;
            return Err(input_error("input.mouse_hook"));
        };
        Ok(Self { keyboard, mouse })
    }
}

impl Drop for InputObservationHook {
    fn drop(&mut self) {
        if let Some(slot) = INPUT.get()
            && let Ok(mut state) = slot.lock()
        {
            *state = None;
        }
        // SAFETY: this value owns both successfully installed hooks.
        let _ = unsafe { UnhookWindowsHookEx(self.keyboard) };
        // SAFETY: this value owns both successfully installed hooks.
        let _ = unsafe { UnhookWindowsHookEx(self.mouse) };
    }
}

impl std::fmt::Debug for InputObservationHook {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("InputObservationHook")
            .finish_non_exhaustive()
    }
}

unsafe extern "system" fn keyboard_callback(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        // SAFETY: Windows supplies a KBDLLHOOKSTRUCT pointer for low-level keyboard callbacks.
        let data = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        let message = u32::try_from(wparam.0).unwrap_or_default();
        let down = message == WM_KEYDOWN || message == WM_SYSKEYDOWN;
        let up = message == WM_KEYUP || message == WM_SYSKEYUP;
        if (down || up)
            && let Some(slot) = INPUT.get()
            && let Ok(mut state) = slot.try_lock()
            && let Some(state) = state.as_mut()
        {
            let repeated = down && !state.pressed.insert(data.vkCode);
            if up {
                state.pressed.remove(&data.vkCode);
            }
            update_modifier(state, data.vkCode, down);
            let _ = state.bridge.emit(
                if down {
                    "input.key_down"
                } else {
                    "input.key_up"
                },
                now_ms(),
                EventSource::Input,
                context(),
                AutomationEventData::Key {
                    virtual_key: data.vkCode,
                    scan_code: data.scanCode,
                    modifiers: state.modifiers,
                    repeated,
                    injected: data.flags.contains(LLKHF_INJECTED),
                },
            );
            if down && !repeated {
                for chord in state.chords.iter().filter(|chord| {
                    chord.virtual_key == data.vkCode && chord.modifiers == state.modifiers
                }) {
                    let mut values = BTreeMap::new();
                    values.insert(
                        "chord".into(),
                        serde_json::Value::String(chord.display.clone()),
                    );
                    let _ = state.bridge.emit(
                        "hotkey.triggered",
                        now_ms(),
                        EventSource::Input,
                        context(),
                        AutomationEventData::Fields { values },
                    );
                }
            }
        }
    }
    // SAFETY: observation hooks must always continue the chain unchanged.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

unsafe extern "system" fn mouse_callback(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        // SAFETY: Windows supplies an MSLLHOOKSTRUCT pointer for low-level mouse callbacks.
        let data = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
        let message = u32::try_from(wparam.0).unwrap_or_default();
        let Some((name, button, wheel)) = mouse_event(message, data.mouseData) else {
            // SAFETY: observation hooks must always continue the chain unchanged.
            return unsafe { CallNextHookEx(None, code, wparam, lparam) };
        };
        if let Some(bridge) = INPUT
            .get()
            .and_then(|slot| slot.try_lock().ok())
            .and_then(|state| state.as_ref().map(|state| Arc::clone(&state.bridge)))
        {
            let _ = bridge.emit(
                name,
                now_ms(),
                EventSource::Input,
                context(),
                AutomationEventData::Mouse {
                    x: data.pt.x,
                    y: data.pt.y,
                    button,
                    wheel_delta: wheel,
                    injected: data.flags & LLMHF_INJECTED != 0,
                },
            );
        }
    }
    // SAFETY: observation hooks must always continue the chain unchanged.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

fn update_modifier(state: &mut InputState, key: u32, down: bool) {
    let bit = if key == u32::from(VK_CONTROL.0) {
        MOD_CTRL
    } else if key == u32::from(VK_MENU.0) {
        MOD_ALT
    } else if key == u32::from(VK_SHIFT.0) {
        MOD_SHIFT
    } else if key == u32::from(VK_LWIN.0) || key == u32::from(VK_RWIN.0) {
        MOD_WIN
    } else {
        return;
    };
    if down {
        state.modifiers |= bit;
    } else {
        state.modifiers &= !bit;
    }
}

fn mouse_event(message: u32, data: u32) -> Option<(&'static str, Option<u8>, Option<i32>)> {
    let wheel = || {
        Some(i32::from(i16::from_ne_bytes(
            (data >> 16).to_ne_bytes()[..2].try_into().ok()?,
        )))
    };
    match message {
        WM_MOUSEMOVE => Some(("input.mouse_move", None, None)),
        WM_LBUTTONDOWN => Some(("input.mouse_down", Some(1), None)),
        WM_LBUTTONUP => Some(("input.mouse_up", Some(1), None)),
        WM_RBUTTONDOWN => Some(("input.mouse_down", Some(2), None)),
        WM_RBUTTONUP => Some(("input.mouse_up", Some(2), None)),
        WM_MBUTTONDOWN => Some(("input.mouse_down", Some(3), None)),
        WM_MBUTTONUP => Some(("input.mouse_up", Some(3), None)),
        WM_XBUTTONDOWN => Some(("input.mouse_down", Some(4), None)),
        WM_XBUTTONUP => Some(("input.mouse_up", Some(4), None)),
        WM_MOUSEWHEEL => Some(("input.mouse_wheel", None, wheel())),
        WM_MOUSEHWHEEL => Some(("input.mouse_hwheel", None, wheel())),
        _ => None,
    }
}

fn parse_chord(value: &str) -> AutomationResult<HotkeyChord> {
    let mut modifiers = 0_u16;
    let mut key = None;
    for part in value.split('+').map(str::trim) {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => modifiers |= MOD_CTRL,
            "alt" => modifiers |= MOD_ALT,
            "shift" => modifiers |= MOD_SHIFT,
            "win" | "windows" => modifiers |= MOD_WIN,
            other if other.len() == 1 => {
                key = other
                    .bytes()
                    .next()
                    .map(|byte| u32::from(byte.to_ascii_uppercase()));
            }
            _ => return Err(input_error("hotkey.parse")),
        }
    }
    let virtual_key = key.ok_or_else(|| input_error("hotkey.parse"))?;
    Ok(HotkeyChord {
        modifiers,
        virtual_key,
        display: value.into(),
    })
}

fn context() -> EventContext {
    EventContext {
        script_id: None,
        handler_id: None,
        task_id: None,
        correlation_id: CorrelationId::new(),
        window_id: None,
        tab_id: None,
        cwd: None,
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
        })
}

fn input_error(operation: &'static str) -> AutomationError {
    AutomationError::new(
        AutomationErrorKind::Unavailable,
        operation,
        true,
        "Global input observation could not be started",
    )
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use explorer_automation::{EventBridge, fakes::FakeEventSink};

    use super::{InputObservationHook, MOD_ALT, MOD_CTRL, mouse_event, parse_chord};

    #[test]
    fn chord_parser_and_mouse_mapping_are_observation_only_metadata() {
        let chord = parse_chord("Ctrl+Alt+S").expect("chord");
        assert_eq!(chord.modifiers, MOD_CTRL | MOD_ALT);
        assert_eq!(chord.virtual_key, u32::from(b'S'));
        assert_eq!(mouse_event(0, 0), None);
    }

    #[test]
    fn hooks_can_be_unloaded_and_installed_again() {
        let bridge = Arc::new(EventBridge::new(Arc::new(
            FakeEventSink::new(16).expect("sink"),
        )));
        let first = InputObservationHook::start(Arc::clone(&bridge), &["Ctrl+Alt+S".into()])
            .expect("first hook");
        drop(first);
        let second = InputObservationHook::start(bridge, &[]).expect("second hook");
        drop(second);
    }
}
