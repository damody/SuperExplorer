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
//! Windows-specific automation host adapters.

mod clipboard;
mod credential;
mod input_hook;
mod process;
mod system_events;
mod watcher;
mod win_event;

pub use clipboard::WindowsClipboardHost;
pub use credential::WindowsCredentialStore;
pub use input_hook::InputObservationHook;
pub use process::WindowsJobProcessHost;
pub use system_events::SystemEventSource;
pub use watcher::FolderWatchService;
pub use win_event::WindowEventHook;
