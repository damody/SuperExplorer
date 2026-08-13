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
//! Windows-only adapter boundary for Shell, COM, OLE, and native operations.
#![allow(
    clippy::must_use_candidate,
    reason = "Shell value factories and observations do not require universal consumption annotations"
)]

#[cfg(not(windows))]
compile_error!("explorer-shell-win supports Windows targets only");

mod clipboard;
mod context_menu;
mod drag_drop;
mod everything;
pub use everything::{IndexedFolderEntryV1, query_folder_index};
mod bc7_codec;
mod extension;
mod file_operation;
mod icon;
mod icon_disk_cache;
pub use icon_disk_cache::{
    ShellDiskCacheStatsV1, icon_disk_cache_stats, set_shell_disk_cache_limits,
    thumbnail_disk_cache_stats,
};
mod namespace;
mod native;
mod navigation;
mod preview;
mod process_current_directory;
mod restart_manager;
mod search;
mod shell_new;
mod sta;
mod thumbnail;
mod watcher;

pub use context_menu::{
    ContextMenuQuerySnapshot, ContextMenuResourceSnapshot,
    execute_in_worker as execute_context_menu_in_worker,
    query_in_worker as query_context_menu_in_worker,
    query_in_worker_with_profile as query_context_menu_in_worker_with_profile,
    query_snapshot_in_worker_with_profile as query_context_menu_snapshot_in_worker_with_profile,
};
pub use drag_drop::{
    DragResourceSnapshot, RightDragChoice, SystemDragThreshold, choose_right_drag_effect,
    dropped_file_operation, modifiers_from_key_state, negotiate_native_effect,
};
pub use extension::tortoise_git_is_installed;
pub use namespace::inspect_namespace_item;
pub use native::NativeResourceSnapshot;
pub use navigation::{DIRECTORY_BATCH_BYTE_CAP, DIRECTORY_BATCH_ITEM_CAP, open_default};
pub use preview::{
    AttachedPreviewSession, PreviewHandlerHost, PreviewLookup, render_preview_in_worker,
};
pub use process_current_directory::{
    CurrentDirectoryOwnerBatchTerminal, CurrentDirectoryOwnerMatch,
    discover_current_directory_owners_read_only,
};
pub use restart_manager::discover_lock_owners_read_only;
pub use shell_new::{registered_shell_new_items, registered_shell_new_items_in_worker};
pub use sta::{
    ShellDomainDiagnostics, ShellStaEndpointError, ShellStaError, ShellStaHandle, ShellStaState,
    StaResourceSnapshot,
};
pub use thumbnail::{clear_thumbnail_disk_cache, load_shell_thumbnail, load_shell_thumbnail_rgba};

/// Opens a new Command Prompt, using the current Explorer folder when available.
pub fn launch_command_prompt(working_directory: Option<&std::path::Path>) -> std::io::Result<()> {
    use std::os::windows::process::CommandExt as _;

    const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
    let mut command = std::process::Command::new("cmd.exe");
    command.creation_flags(CREATE_NEW_CONSOLE);
    if let Some(directory) = working_directory.filter(|directory| directory.is_dir()) {
        command.current_dir(directory);
    }
    command.spawn().map(|_| ())
}

/// Returns one bounded owned snapshot for a disposable namespace worker.
/// No PIDL or COM interface leaves the caller's apartment.
///
/// # Errors
/// Returns a typed Shell error when the location cannot be resolved or enumerated.
pub fn enumerate_namespace_in_worker(
    location: &explorer_model::LocationDescriptor,
    maximum_items: usize,
) -> Result<Vec<explorer_model::FileEntry>, explorer_common::ExplorerError> {
    let resolved = navigation::resolve_location(location)?;
    let context = explorer_model::RequestContext::new(
        explorer_model::TabId::new(),
        explorer_model::Generation::new(1),
    );
    let mut entries = Vec::new();
    navigation::enumerate_directory(&context, &resolved, |event| {
        if let explorer_model::ExplorerEvent::DirectoryBatch { entries: batch, .. } = event {
            let remaining = maximum_items.saturating_sub(entries.len());
            entries.extend(batch.into_iter().take(remaining));
        }
        entries.len() < maximum_items
    })?;
    Ok(entries)
}

impl explorer_model::ExplorerService for ShellStaHandle {
    fn submit(
        &self,
        command: explorer_model::ExplorerCommand,
    ) -> Result<(), explorer_model::ExplorerServiceError> {
        ShellStaHandle::submit(self, command).map_err(|error| match error {
            ShellStaEndpointError::CommandQueueFull => {
                explorer_model::ExplorerServiceError::Overloaded
            }
            ShellStaEndpointError::CommandEndpointDisconnected
            | ShellStaEndpointError::EventEndpointDisconnected => {
                explorer_model::ExplorerServiceError::Disconnected
            }
            ShellStaEndpointError::Poisoned => explorer_model::ExplorerServiceError::Internal,
        })
    }

    fn try_recv(
        &self,
    ) -> Result<Option<explorer_model::ExplorerEvent>, explorer_model::ExplorerServiceError> {
        self.try_recv_event().map_err(|error| match error {
            ShellStaEndpointError::CommandQueueFull => {
                explorer_model::ExplorerServiceError::Overloaded
            }
            ShellStaEndpointError::CommandEndpointDisconnected
            | ShellStaEndpointError::EventEndpointDisconnected => {
                explorer_model::ExplorerServiceError::Disconnected
            }
            ShellStaEndpointError::Poisoned => explorer_model::ExplorerServiceError::Internal,
        })
    }
}

/// Describes the platform boundary without exposing Win32 types to consumers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShellPlatform {
    pub requires_sta: bool,
}

impl ShellPlatform {
    /// Returns the Windows Shell apartment policy.
    pub const fn windows() -> Self {
        Self { requires_sta: true }
    }
}
