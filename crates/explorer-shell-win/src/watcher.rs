//! Overlapped directory change watcher and defensive notification parsing.
#![allow(
    unsafe_code,
    reason = "overlapped ReadDirectoryChangesW requires audited buffer and handle pointers"
)]

use std::{
    ffi::c_void,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::SyncSender,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use explorer_model::{DirectoryDelta, ExplorerEvent, Generation, TabId};
use windows::{
    Win32::{
        Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT},
        Storage::FileSystem::{
            CreateFileW, FILE_ACTION_ADDED, FILE_ACTION_MODIFIED, FILE_ACTION_REMOVED,
            FILE_ACTION_RENAMED_NEW_NAME, FILE_ACTION_RENAMED_OLD_NAME, FILE_FLAG_BACKUP_SEMANTICS,
            FILE_FLAG_OVERLAPPED, FILE_LIST_DIRECTORY, FILE_NOTIFY_CHANGE_ATTRIBUTES,
            FILE_NOTIFY_CHANGE_CREATION, FILE_NOTIFY_CHANGE_DIR_NAME, FILE_NOTIFY_CHANGE_FILE_NAME,
            FILE_NOTIFY_CHANGE_LAST_WRITE, FILE_NOTIFY_CHANGE_SIZE, FILE_SHARE_DELETE,
            FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING, ReadDirectoryChangesW,
        },
        System::{
            IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED},
            Threading::{CreateEventW, WaitForSingleObject},
        },
    },
    core::HSTRING,
};

const WATCH_BUFFER_SIZE: usize = 64 * 1024;
const CANCEL_POLL_MS: u32 = 50;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum WatchAction {
    Added(PathBuf),
    Removed(PathBuf),
    Modified(PathBuf),
    Renamed { old: PathBuf, new: PathBuf },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum WatchParseError {
    TruncatedHeader,
    InvalidNameLength,
    TruncatedName,
    InvalidUtf16,
    InvalidNextOffset,
    UnpairedRename,
}

pub(crate) fn parse_notifications(bytes: &[u8]) -> Result<Vec<WatchAction>, WatchParseError> {
    let mut offset = 0_usize;
    let mut raw = Vec::new();
    loop {
        if bytes.len().saturating_sub(offset) < 12 {
            return Err(WatchParseError::TruncatedHeader);
        }
        let next = read_u32(bytes, offset)? as usize;
        let action = read_u32(bytes, offset + 4)?;
        let name_bytes = read_u32(bytes, offset + 8)? as usize;
        if name_bytes % 2 != 0 {
            return Err(WatchParseError::InvalidNameLength);
        }
        let name_start = offset + 12;
        let name_end = name_start
            .checked_add(name_bytes)
            .ok_or(WatchParseError::TruncatedName)?;
        if name_end > bytes.len() {
            return Err(WatchParseError::TruncatedName);
        }
        let words = bytes[name_start..name_end]
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect::<Vec<_>>();
        let name = String::from_utf16(&words).map_err(|_| WatchParseError::InvalidUtf16)?;
        raw.push((action, PathBuf::from(name)));
        if next == 0 {
            break;
        }
        if next < 12 || next % 4 != 0 || offset.saturating_add(next) <= offset {
            return Err(WatchParseError::InvalidNextOffset);
        }
        offset = offset
            .checked_add(next)
            .ok_or(WatchParseError::InvalidNextOffset)?;
        if offset >= bytes.len() {
            return Err(WatchParseError::InvalidNextOffset);
        }
    }

    let mut actions = Vec::new();
    let mut pending_rename = None;
    for (action, name) in raw {
        if action == FILE_ACTION_RENAMED_OLD_NAME.0 {
            if pending_rename.replace(name).is_some() {
                return Err(WatchParseError::UnpairedRename);
            }
        } else if action == FILE_ACTION_RENAMED_NEW_NAME.0 {
            let old = pending_rename
                .take()
                .ok_or(WatchParseError::UnpairedRename)?;
            actions.push(WatchAction::Renamed { old, new: name });
        } else {
            if pending_rename.is_some() {
                return Err(WatchParseError::UnpairedRename);
            }
            if action == FILE_ACTION_ADDED.0 {
                actions.push(WatchAction::Added(name));
            } else if action == FILE_ACTION_REMOVED.0 {
                actions.push(WatchAction::Removed(name));
            } else if action == FILE_ACTION_MODIFIED.0 {
                actions.push(WatchAction::Modified(name));
            }
        }
    }
    if pending_rename.is_some() {
        return Err(WatchParseError::UnpairedRename);
    }
    Ok(actions)
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, WatchParseError> {
    let end = offset
        .checked_add(4)
        .ok_or(WatchParseError::TruncatedHeader)?;
    let chunk = bytes
        .get(offset..end)
        .ok_or(WatchParseError::TruncatedHeader)?;
    let array: [u8; 4] = chunk
        .try_into()
        .map_err(|_| WatchParseError::TruncatedHeader)?;
    Ok(u32::from_le_bytes(array))
}

pub(crate) struct WatcherSession {
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl WatcherSession {
    pub(crate) fn start(
        path: PathBuf,
        tab_id: TabId,
        generation: Generation,
        events: SyncSender<ExplorerEvent>,
    ) -> std::io::Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let join = thread::Builder::new()
            .name("explorer-directory-watcher".to_owned())
            .spawn(move || watch_loop(&path, tab_id, generation, &events, &thread_stop))?;
        Ok(Self {
            stop,
            join: Some(join),
        })
    }

    pub(crate) fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl Drop for WatcherSession {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn watch_loop(
    path: &Path,
    tab_id: TabId,
    generation: Generation,
    events: &SyncSender<ExplorerEvent>,
    stop: &AtomicBool,
) {
    if let Err(error) = watch_loop_inner(path, tab_id, generation, events, stop) {
        tracing::warn!(?tab_id, generation = generation.value(), %error, "directory watcher stopped");
        explorer_common::record_process_error(
            explorer_common::ErrorSeverity::Error,
            "shell",
            "directory_watcher",
            &error,
            Some(file!()),
        );
    }
}

fn watch_loop_inner(
    path: &Path,
    tab_id: TabId,
    generation: Generation,
    events: &SyncSender<ExplorerEvent>,
    stop: &AtomicBool,
) -> windows::core::Result<()> {
    let path = HSTRING::from(path.as_os_str().to_string_lossy().as_ref());
    // SAFETY: path is live and ownership of the directory handle transfers to OwnedHandle.
    let raw_directory = unsafe {
        CreateFileW(
            &path,
            FILE_LIST_DIRECTORY.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OVERLAPPED,
            None,
        )
    }?;
    // SAFETY: CreateFileW returned a unique valid handle on success.
    let directory = unsafe { crate::native::OwnedHandle::from_raw(raw_directory) }
        .ok_or_else(windows::core::Error::from_win32)?;
    // SAFETY: default security and unnamed auto-reset event; ownership transfers to OwnedHandle.
    let raw_event = unsafe { CreateEventW(None, false, false, None) }?;
    // SAFETY: CreateEventW returned a unique valid handle on success.
    let event = unsafe { crate::native::OwnedHandle::from_raw(raw_event) }
        .ok_or_else(windows::core::Error::from_win32)?;
    let mut buffer = vec![0_u8; WATCH_BUFFER_SIZE];
    let filter = FILE_NOTIFY_CHANGE_FILE_NAME
        | FILE_NOTIFY_CHANGE_DIR_NAME
        | FILE_NOTIFY_CHANGE_ATTRIBUTES
        | FILE_NOTIFY_CHANGE_SIZE
        | FILE_NOTIFY_CHANGE_LAST_WRITE
        | FILE_NOTIFY_CHANGE_CREATION;

    while !stop.load(Ordering::Acquire) {
        let mut overlapped = OVERLAPPED {
            hEvent: event.get(),
            ..OVERLAPPED::default()
        };
        // SAFETY: directory was opened OVERLAPPED, buffer and OVERLAPPED remain pinned in this
        // stack frame until completion/cancellation, and no completion callback is requested.
        let buffer_size = u32::try_from(buffer.len()).map_err(|_| {
            windows::core::Error::new(
                windows::Win32::Foundation::E_INVALIDARG,
                "directory watch buffer exceeds Win32 limit",
            )
        })?;
        unsafe {
            ReadDirectoryChangesW(
                directory.get(),
                buffer.as_mut_ptr().cast::<c_void>(),
                buffer_size,
                false,
                filter,
                None,
                Some(&raw mut overlapped),
                None,
            )
        }?;
        loop {
            // SAFETY: event remains live and WaitForSingleObject does not retain the handle.
            let wait = unsafe { WaitForSingleObject(event.get(), CANCEL_POLL_MS) };
            if wait == WAIT_OBJECT_0 {
                break;
            }
            if wait != WAIT_TIMEOUT || stop.load(Ordering::Acquire) {
                // SAFETY: the OVERLAPPED belongs to the pending I/O on this directory handle.
                let _ = unsafe { CancelIoEx(directory.get(), Some(&raw const overlapped)) };
                return Ok(());
            }
        }
        let mut transferred = 0_u32;
        // SAFETY: completion event was signaled and OVERLAPPED remains live; no blocking wait.
        unsafe {
            GetOverlappedResult(
                directory.get(),
                &raw const overlapped,
                &raw mut transferred,
                false,
            )
        }?;
        let valid = usize::try_from(transferred).unwrap_or(0).min(buffer.len());
        let parsed = (valid > 0)
            .then(|| parse_notifications(&buffer[..valid]))
            .transpose();
        if parsed.is_err() || parsed.as_ref().is_ok_and(Option::is_none) {
            tracing::warn!(
                ?tab_id,
                generation = generation.value(),
                "watcher overflow or malformed notification"
            );
        }
        if events
            .try_send(ExplorerEvent::DirectoryChanged {
                tab_id,
                generation,
                changes: vec![DirectoryDelta::Overflow],
            })
            .is_err()
        {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(35));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{WatchAction, WatchParseError, parse_notifications};
    use std::path::PathBuf;

    fn record(action: u32, name: &str, terminal: bool) -> Vec<u8> {
        let words = name.encode_utf16().collect::<Vec<_>>();
        let raw_len = 12 + words.len() * 2;
        let padded = (raw_len + 3) & !3;
        let mut bytes = vec![0_u8; padded];
        bytes[0..4].copy_from_slice(
            &(if terminal {
                0
            } else {
                u32::try_from(padded).expect("test record fits u32")
            })
            .to_le_bytes(),
        );
        bytes[4..8].copy_from_slice(&action.to_le_bytes());
        bytes[8..12].copy_from_slice(&u32::try_from(words.len() * 2).unwrap().to_le_bytes());
        for (index, word) in words.into_iter().enumerate() {
            bytes[12 + index * 2..14 + index * 2].copy_from_slice(&word.to_le_bytes());
        }
        bytes
    }

    #[test]
    fn parses_unicode_actions_and_pairs_rename() {
        let mut bytes = record(1, "新增-😀.txt", false);
        bytes.extend(record(3, "e\u{301}.txt", false));
        bytes.extend(record(4, "old.txt", false));
        bytes.extend(record(5, "new.txt", true));
        assert_eq!(
            parse_notifications(&bytes),
            Ok(vec![
                WatchAction::Added(PathBuf::from("新增-😀.txt")),
                WatchAction::Modified(PathBuf::from("e\u{301}.txt")),
                WatchAction::Renamed {
                    old: PathBuf::from("old.txt"),
                    new: PathBuf::from("new.txt"),
                },
            ])
        );
    }

    #[test]
    fn rejects_truncated_malformed_and_unpaired_records() {
        assert_eq!(
            parse_notifications(&[0; 8]),
            Err(WatchParseError::TruncatedHeader)
        );
        let mut odd = record(1, "x", true);
        odd[8..12].copy_from_slice(&1_u32.to_le_bytes());
        assert_eq!(
            parse_notifications(&odd),
            Err(WatchParseError::InvalidNameLength)
        );
        let mut bad_next = record(1, "x", false);
        bad_next[0..4].copy_from_slice(&13_u32.to_le_bytes());
        assert_eq!(
            parse_notifications(&bad_next),
            Err(WatchParseError::InvalidNextOffset)
        );
        assert_eq!(
            parse_notifications(&record(4, "old", true)),
            Err(WatchParseError::UnpairedRename)
        );
    }
}
