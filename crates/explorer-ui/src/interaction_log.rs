//! Interaction traces for clicks, clipboard shortcuts, and drag release.
//!
//! These are written even when the gesture is ignored, so a silent no-op still
//! leaves a line in `error.log`.

use gpui::{
    DispatchPhase, KeyDownEvent, MouseButton, MouseDownEvent, MouseUpEvent, Styled, canvas,
};

pub(crate) fn record_ui_interaction(operation: &str, detail: &str) {
    tracing::info!(operation, detail, "Explorer interaction");
    explorer_common::record_process_error_message(
        explorer_common::ErrorSeverity::Warning,
        "ui",
        operation,
        detail,
        Some(file!()),
    );
}

pub(crate) fn interaction_event_layer() -> impl gpui::IntoElement {
    canvas(
        |_, _, _| (),
        |_, (), window, _| {
            window.on_mouse_event(|event: &MouseDownEvent, phase, _, _| {
                if phase == DispatchPhase::Capture {
                    record_ui_interaction("pointer_down", &format_mouse_down(event));
                }
            });
            window.on_mouse_event(|event: &MouseUpEvent, phase, _, _| {
                if phase == DispatchPhase::Capture {
                    record_ui_interaction("pointer_up", &format_mouse_up(event));
                }
            });
            window.on_key_event(|event: &KeyDownEvent, phase, _, _| {
                if phase != DispatchPhase::Capture {
                    return;
                }
                if let Some(chord) = watched_shortcut(event) {
                    record_ui_interaction("key_shortcut", &format!("chord={chord} phase=received"));
                }
            });
        },
    )
    .absolute()
    .size_full()
}

pub(crate) struct ShortcutTrace {
    chord: Option<String>,
    focus: String,
    outcome: &'static str,
}

impl ShortcutTrace {
    pub(crate) fn begin(event: &KeyDownEvent, focus: &str) -> Self {
        Self {
            chord: watched_shortcut(event),
            focus: focus.to_owned(),
            outcome: "ignored",
        }
    }

    pub(crate) fn note(&mut self, outcome: &'static str) {
        if self.chord.is_some() {
            self.outcome = outcome;
        }
    }

    pub(crate) fn note_if_pending(&mut self, outcome: &'static str) {
        if self.chord.is_some() && self.outcome == "ignored" {
            self.outcome = outcome;
        }
    }
}

impl Drop for ShortcutTrace {
    fn drop(&mut self) {
        let Some(chord) = self.chord.as_deref() else {
            return;
        };
        record_ui_interaction(
            "key_shortcut",
            &format!(
                "chord={chord} focus={} outcome={}",
                self.focus, self.outcome
            ),
        );
    }
}

pub(crate) fn watched_shortcut(event: &KeyDownEvent) -> Option<String> {
    if !event.keystroke.modifiers.control {
        return None;
    }
    let key = event.keystroke.key.as_str();
    if !matches!(key, "c" | "v" | "x" | "w" | "z" | "y") {
        return None;
    }
    let mut chord = String::from("ctrl");
    if event.keystroke.modifiers.shift {
        chord.push_str("+shift");
    }
    if event.keystroke.modifiers.alt {
        chord.push_str("+alt");
    }
    if event.keystroke.modifiers.platform {
        chord.push_str("+platform");
    }
    chord.push('+');
    chord.push_str(key);
    Some(chord)
}

fn format_mouse_down(event: &MouseDownEvent) -> String {
    format!(
        "button={} x={} y={} clicks={} ctrl={} shift={} alt={} platform={}",
        mouse_button_name(event.button),
        f32::from(event.position.x),
        f32::from(event.position.y),
        event.click_count,
        event.modifiers.control,
        event.modifiers.shift,
        event.modifiers.alt,
        event.modifiers.platform,
    )
}

fn format_mouse_up(event: &MouseUpEvent) -> String {
    format!(
        "button={} x={} y={} clicks={} ctrl={} shift={} alt={} platform={}",
        mouse_button_name(event.button),
        f32::from(event.position.x),
        f32::from(event.position.y),
        event.click_count,
        event.modifiers.control,
        event.modifiers.shift,
        event.modifiers.alt,
        event.modifiers.platform,
    )
}

fn mouse_button_name(button: MouseButton) -> &'static str {
    match button {
        MouseButton::Left => "left",
        MouseButton::Right => "right",
        MouseButton::Middle => "middle",
        MouseButton::Navigate(_) => "navigate",
    }
}

#[cfg(test)]
mod tests {
    use super::watched_shortcut;

    fn key_event(key: &str, control: bool) -> gpui::KeyDownEvent {
        gpui::KeyDownEvent {
            keystroke: gpui::Keystroke {
                modifiers: gpui::Modifiers {
                    control,
                    ..gpui::Modifiers::default()
                },
                key: key.to_owned(),
                key_char: None,
            },
            is_held: false,
            prefer_character_input: false,
        }
    }

    #[test]
    fn watched_shortcuts_include_clipboard_and_edit_chords() {
        assert_eq!(
            watched_shortcut(&key_event("c", true)).as_deref(),
            Some("ctrl+c")
        );
        assert_eq!(
            watched_shortcut(&key_event("v", true)).as_deref(),
            Some("ctrl+v")
        );
        assert_eq!(
            watched_shortcut(&key_event("x", true)).as_deref(),
            Some("ctrl+x")
        );
        assert_eq!(
            watched_shortcut(&key_event("w", true)).as_deref(),
            Some("ctrl+w")
        );
        assert_eq!(
            watched_shortcut(&key_event("z", true)).as_deref(),
            Some("ctrl+z")
        );
        assert_eq!(
            watched_shortcut(&key_event("y", true)).as_deref(),
            Some("ctrl+y")
        );
        assert!(watched_shortcut(&key_event("c", false)).is_none());
        assert!(watched_shortcut(&key_event("a", true)).is_none());
    }
}
