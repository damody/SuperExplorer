//! Shell-compatible OLE clipboard adapter owned entirely by the Shell STA.
#![allow(
    unsafe_code,
    reason = "Shell IDataObject, PIDL, HGLOBAL, and STGMEDIUM ownership require audited FFI"
)]

use std::{collections::HashMap, time::Duration};

use explorer_common::{ExplorerError, ExplorerErrorKind, RequestId};
use explorer_model::{
    ClipboardMode, ClipboardState, ConflictDecision, FileOperationFlags, FileOperationKind,
    FileOperationRequest, ItemDescriptor, OperationItemResult, OperationTerminal, TransferEffects,
};
use windows::Win32::{
    System::{
        Com::{DVASPECT_CONTENT, FORMATETC, IDataObject, STGMEDIUM, TYMED_HGLOBAL},
        DataExchange::{
            CountClipboardFormats, GetClipboardSequenceNumber, IsClipboardFormatAvailable,
            RegisterClipboardFormatW,
        },
        Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock},
        Ole::{
            CF_HDROP, DROPEFFECT_COPY, DROPEFFECT_MOVE, OleFlushClipboard, OleGetClipboard,
            OleInitialize, OleSetClipboard, OleUninitialize, ReleaseStgMedium,
        },
    },
    UI::Shell::{
        BHID_DataObject, CFSTR_PERFORMEDDROPEFFECT, CFSTR_PREFERREDDROPEFFECT, DragQueryFileW,
        HDROP, ILFindLastID, SHCreateDataObject, SHCreateShellItemArrayFromIDLists,
    },
};

/// Reads only native file-drop clipboard data on a short-lived OLE STA. Text, HTML, and image
/// formats are ignored and never cleared, so file Paste cannot consume ordinary clipboard data.
pub fn read_native_file_clipboard()
-> Result<Option<(Vec<ItemDescriptor>, ClipboardMode)>, ExplorerError> {
    // SAFETY: this function is called from a fresh worker thread and balances OLE initialization.
    unsafe { OleInitialize(None) }
        .map_err(|error| native_clipboard_error("initialize OLE clipboard worker", &error))?;
    let result = (|| {
        // SAFETY: format inspection does not acquire or mutate clipboard ownership.
        if unsafe { IsClipboardFormatAvailable(u32::from(CF_HDROP.0)) }.is_err() {
            return Ok(None);
        }
        let data = get_clipboard_with_retry("read native file clipboard", Duration::from_secs(2))?;
        let mode = preferred_mode(&data).unwrap_or(ClipboardMode::Copy);
        read_hdrop_items(&data).map(|items| Some((items, mode)))
    })();
    // SAFETY: balances the successful OleInitialize on this worker thread.
    unsafe { OleUninitialize() };
    result
}

/// Publishes local filesystem items as a standard Shell file clipboard object. Only file-drop
/// formats are authored; text and image clipboard payloads are never interpreted or merged.
pub fn publish_native_file_clipboard(
    items: Vec<ItemDescriptor>,
    mode: ClipboardMode,
) -> Result<(), ExplorerError> {
    // SAFETY: callers use a dedicated worker thread and initialization is balanced below.
    unsafe { OleInitialize(None) }
        .map_err(|error| native_clipboard_error("initialize OLE clipboard worker", &error))?;
    let result = (|| {
        let mut runtime = ClipboardRuntime::new();
        runtime.copy_or_cut(items, mode)?;
        runtime.shutdown();
        Ok(())
    })();
    // SAFETY: balances successful OleInitialize on this worker thread.
    unsafe { OleUninitialize() };
    result
}

#[cfg(test)]
pub(crate) static CLIPBOARD_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct PendingPaste {
    data: IDataObject,
    mode: ClipboardMode,
    clipboard_sequence: u32,
}

const fn background_paste_still_owns_clipboard(expected: u32, current: u32) -> bool {
    expected == current
}

pub(crate) struct ClipboardRuntime {
    owned: Option<IDataObject>,
    pending_pastes: HashMap<RequestId, PendingPaste>,
    state: ClipboardState,
    generation: u64,
    sequence: u32,
}

impl ClipboardRuntime {
    pub(crate) fn new() -> Self {
        // SAFETY: reads process-global clipboard sequence state without acquiring ownership.
        let sequence = unsafe { GetClipboardSequenceNumber() }.wrapping_sub(1);
        Self {
            owned: None,
            pending_pastes: HashMap::new(),
            state: ClipboardState::None { generation: 0 },
            generation: 0,
            sequence,
        }
    }

    pub(crate) fn state(&self) -> ClipboardState {
        self.state.clone()
    }

    pub(crate) fn copy_or_cut(
        &mut self,
        items: Vec<ItemDescriptor>,
        mode: ClipboardMode,
    ) -> Result<ClipboardState, ExplorerError> {
        if items.is_empty() {
            return Err(clipboard_error(
                ExplorerErrorKind::Input,
                "create Shell clipboard object",
                "請先選取至少一個項目。",
                "clipboard selection was empty",
            ));
        }
        let data = create_shell_data_object(&items)?;
        let effect = match mode {
            ClipboardMode::Copy => DROPEFFECT_COPY.0,
            ClipboardMode::Cut => DROPEFFECT_MOVE.0,
        };
        set_drop_effect(&data, CFSTR_PREFERREDDROPEFFECT, effect)?;
        // SAFETY: this thread initialized OLE; OLE retains its own IDataObject reference.
        set_clipboard_with_retry(&data)?;
        self.owned = Some(data);
        self.generation = self.generation.saturating_add(1);
        // SAFETY: sequence is sampled after OleSetClipboard to identify later ownership changes.
        self.sequence = unsafe { GetClipboardSequenceNumber() };
        self.state = ClipboardState::Owned {
            mode,
            items,
            effects: match mode {
                ClipboardMode::Copy => TransferEffects::COPY,
                ClipboardMode::Cut => TransferEffects::MOVE,
            },
            generation: self.generation,
        };
        Ok(self.state())
    }

    pub(crate) fn poll_change(&mut self) -> Option<ClipboardState> {
        // SAFETY: sequence observation is lock-free and does not expose clipboard handles.
        let current = unsafe { GetClipboardSequenceNumber() };
        if current == self.sequence {
            return None;
        }
        self.sequence = current;
        self.owned = None;
        self.generation = self.generation.saturating_add(1);
        // SAFETY: format availability does not open or retain the clipboard.
        if unsafe { IsClipboardFormatAvailable(u32::from(CF_HDROP.0)) }.is_err() {
            // SAFETY: the count distinguishes a genuinely empty clipboard from unsupported data.
            self.state = if unsafe { CountClipboardFormats() } == 0 {
                ClipboardState::None {
                    generation: self.generation,
                }
            } else {
                ClipboardState::Unsupported {
                    error: clipboard_error(
                        ExplorerErrorKind::Availability,
                        "inspect clipboard formats",
                        "剪貼簿不包含可貼上的檔案。",
                        "clipboard contained formats but no CF_HDROP",
                    ),
                    generation: self.generation,
                }
            };
            return Some(self.state());
        }
        self.state = match external_clipboard_state(self.generation) {
            Ok(state) => state,
            Err(error) => ClipboardState::Unsupported {
                error,
                generation: self.generation,
            },
        };
        Some(self.state())
    }

    pub(crate) fn paste_request(
        &self,
        destination: explorer_model::LocationDescriptor,
        conflict: ConflictDecision,
    ) -> Result<(FileOperationRequest, IDataObject, ClipboardMode), ExplorerError> {
        let (items, data, mode) = if let (ClipboardState::Owned { mode, items, .. }, Some(data)) =
            (&self.state, &self.owned)
        {
            (items.clone(), data.clone(), *mode)
        } else {
            // SAFETY: returned interface is used and released on this OLE-initialized STA.
            let data = get_clipboard_with_retry("get OLE clipboard", Duration::from_secs(2))?;
            let items = read_hdrop_items(&data)?;
            let mode = preferred_mode(&data).unwrap_or(ClipboardMode::Copy);
            (items, data, mode)
        };
        let kind = match mode {
            ClipboardMode::Copy => FileOperationKind::Copy { items, destination },
            ClipboardMode::Cut => FileOperationKind::Move { items, destination },
        };
        let conflict = paste_conflict_for_mode(mode, conflict);
        Ok((
            FileOperationRequest {
                kind,
                flags: FileOperationFlags {
                    conflict,
                    ..FileOperationFlags::default()
                },
            },
            data,
            mode,
        ))
    }

    pub(crate) fn begin_background_paste(
        &mut self,
        request_id: RequestId,
        destination: explorer_model::LocationDescriptor,
        conflict: ConflictDecision,
    ) -> Result<FileOperationRequest, ExplorerError> {
        let (request, data, mode) = self.paste_request(destination, conflict)?;
        // SAFETY: sequence observation is lock-free and lets completion avoid clearing clipboard
        // content that the user copied while this background operation was running.
        let clipboard_sequence = unsafe { GetClipboardSequenceNumber() };
        self.pending_pastes.insert(
            request_id,
            PendingPaste {
                data,
                mode,
                clipboard_sequence,
            },
        );
        Ok(request)
    }

    pub(crate) fn complete_background_paste(
        &mut self,
        request_id: RequestId,
        outcome: &OperationTerminal,
    ) -> Option<ClipboardState> {
        let pending = self.pending_pastes.remove(&request_id)?;
        // SAFETY: sequence observation does not acquire or retain clipboard storage.
        let current_sequence = unsafe { GetClipboardSequenceNumber() };
        if !background_paste_still_owns_clipboard(pending.clipboard_sequence, current_sequence) {
            let _ = self.poll_change();
            return Some(self.state());
        }
        self.complete_paste(&pending.data, pending.mode, outcome);
        Some(self.state())
    }

    pub(crate) fn abandon_background_paste(&mut self, request_id: RequestId) {
        self.pending_pastes.remove(&request_id);
    }

    pub(crate) fn complete_paste(
        &mut self,
        data: &IDataObject,
        mode: ClipboardMode,
        outcome: &OperationTerminal,
    ) {
        let finished = matches!(outcome, OperationTerminal::Finished);
        if finished && mode == ClipboardMode::Cut {
            let _ = set_drop_effect(data, CFSTR_PERFORMEDDROPEFFECT, DROPEFFECT_MOVE.0);
            // SAFETY: clearing after a completed move prevents stale cut data from being pasted
            // again; OLE releases the system-held reference.
            let _ = unsafe { OleSetClipboard(None::<&IDataObject>) };
            self.owned = None;
            self.generation = self.generation.saturating_add(1);
            self.state = ClipboardState::None {
                generation: self.generation,
            };
            // SAFETY: sample the sequence produced by clearing ownership.
            self.sequence = unsafe { GetClipboardSequenceNumber() };
        } else if let (
            ClipboardState::Owned {
                mode: ClipboardMode::Cut,
                items,
                ..
            },
            OperationTerminal::Partial { outcomes },
        ) = (&mut self.state, outcome)
        {
            items.retain(|item| {
                !outcomes.iter().any(|outcome| {
                    outcome.item.as_ref().is_some_and(|done| done.id == item.id)
                        && outcome.result == OperationItemResult::Succeeded
                })
            });
            let remaining = items.clone();
            if !remaining.is_empty() {
                let _ = self.copy_or_cut(remaining, ClipboardMode::Cut);
            }
        }
    }

    pub(crate) fn shutdown(&mut self) {
        if self.owned.is_some() {
            // SAFETY: flush asks OLE to render delayed formats before releasing our final ref.
            if let Err(error) = unsafe { OleFlushClipboard() } {
                tracing::warn!(%error, "failed to flush owned OLE clipboard during shutdown");
            }
        }
        self.owned = None;
        self.pending_pastes.clear();
    }
}

fn paste_conflict_for_mode(mode: ClipboardMode, conflict: ConflictDecision) -> ConflictDecision {
    if mode == ClipboardMode::Cut && conflict == ConflictDecision::KeepBoth {
        ConflictDecision::Prompt
    } else {
        conflict
    }
}

impl Drop for ClipboardRuntime {
    fn drop(&mut self) {
        self.shutdown();
    }
}

pub(crate) fn create_shell_data_object(
    items: &[ItemDescriptor],
) -> Result<IDataObject, ExplorerError> {
    let absolute = items
        .iter()
        .map(|item| crate::navigation::location_absolute_pidl(&item.location))
        .collect::<Result<Vec<_>, _>>()?;
    let pointers = absolute
        .iter()
        .map(crate::navigation::OwnedPidl::as_ptr)
        .collect::<Vec<_>>();
    let common_parent = items
        .first()
        .and_then(|item| item.location.path())
        .and_then(std::path::Path::parent)
        .filter(|parent| {
            items.iter().all(|item| {
                item.location
                    .path()
                    .and_then(std::path::Path::parent)
                    .is_some_and(|candidate| candidate == *parent)
            })
        });
    if let Some(parent) = common_parent {
        let parent = crate::navigation::location_absolute_pidl(
            &explorer_model::LocationDescriptor::file_system(parent),
        )?;
        // SAFETY: every complete child PIDL remains live, so its final relative ID remains live
        // through SHCreateDataObject. The common parent was verified above.
        let relative = absolute
            .iter()
            .map(|child| unsafe { ILFindLastID(child.as_ptr()) }.cast_const())
            .collect::<Vec<_>>();
        // SAFETY: parent and relative children remain live through creation; the returned
        // IDataObject owns the Shell formats expected by Explorer drop targets.
        return unsafe {
            SHCreateDataObject(Some(parent.as_ptr()), Some(&relative), None::<&IDataObject>)
        }
        .map_err(|error| native_clipboard_error("create Shell data object", &error));
    }

    // Mixed-parent and non-filesystem selections retain the general Shell item-array path.
    // SAFETY: every absolute PIDL remains live through creation; the array owns its items.
    let array = unsafe { SHCreateShellItemArrayFromIDLists(&pointers) }
        .map_err(|error| native_clipboard_error("create Shell item array", &error))?;
    let data_object_handler = BHID_DataObject;
    // SAFETY: standard Shell binding yields an owned IDataObject for path/non-path combinations.
    unsafe {
        array.BindToHandler(
            None::<&windows::Win32::System::Com::IBindCtx>,
            &raw const data_object_handler,
        )
    }
    .map_err(|error| native_clipboard_error("bind Shell IDataObject", &error))
}

fn external_clipboard_state(generation: u64) -> Result<ClipboardState, ExplorerError> {
    let started = std::time::Instant::now();
    // SAFETY: interface stays on the initialized STA and is dropped before return.
    let data = get_clipboard_with_retry("inspect OLE clipboard", Duration::from_millis(250))?;
    let count = read_hdrop_count(&data)?;
    validate_inspection_duration(started.elapsed())?;
    let mode = preferred_mode(&data).unwrap_or(ClipboardMode::Copy);
    Ok(ClipboardState::External {
        effects: match mode {
            ClipboardMode::Copy => TransferEffects::COPY,
            ClipboardMode::Cut => TransferEffects::MOVE,
        },
        item_count: Some(count),
        generation,
    })
}

fn get_clipboard_with_retry(
    operation: &'static str,
    retry_budget: Duration,
) -> Result<IDataObject, ExplorerError> {
    let deadline = std::time::Instant::now() + retry_budget;
    loop {
        // SAFETY: every returned interface is confined to the caller's initialized STA.
        match unsafe { OleGetClipboard() } {
            Ok(data) => return Ok(data),
            Err(error)
                if error.code().0 == -2_147_221_040 && std::time::Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => return Err(native_clipboard_error(operation, &error)),
        }
    }
}

fn set_clipboard_with_retry(data: &IDataObject) -> Result<(), ExplorerError> {
    let deadline = std::time::Instant::now() + Duration::from_millis(500);
    loop {
        // SAFETY: OLE retains its own reference on success; the caller keeps `data` live.
        match unsafe { OleSetClipboard(data) } {
            Ok(()) => return Ok(()),
            Err(error)
                if error.code().0 == -2_147_221_040 && std::time::Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => return Err(native_clipboard_error("set OLE clipboard", &error)),
        }
    }
}

#[cfg(test)]
fn clear_clipboard_with_retry() -> Result<(), ExplorerError> {
    let deadline = std::time::Instant::now() + Duration::from_millis(500);
    loop {
        // SAFETY: a null data object releases the current OLE clipboard object; this STA remains
        // initialized for the entire retry loop.
        match unsafe { OleSetClipboard(None::<&IDataObject>) } {
            Ok(()) => return Ok(()),
            Err(error)
                if error.code().0 == -2_147_221_040 && std::time::Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => return Err(native_clipboard_error("clear OLE clipboard", &error)),
        }
    }
}

fn validate_inspection_duration(elapsed: Duration) -> Result<(), ExplorerError> {
    if elapsed <= Duration::from_millis(250) {
        return Ok(());
    }
    Err(clipboard_error(
        ExplorerErrorKind::Availability,
        "inspect slow clipboard object",
        "剪貼簿提供者回應過慢，請稍後再試。",
        format!("clipboard IDataObject inspection took {elapsed:?}"),
    ))
}

fn read_hdrop_count(data: &IDataObject) -> Result<usize, ExplorerError> {
    let format = hdrop_format();
    // SAFETY: QueryGetData reads the initialized FORMATETC and retains no pointer.
    let supported = unsafe { data.QueryGetData(&raw const format) };
    if supported.is_err() {
        return Err(clipboard_error(
            ExplorerErrorKind::Availability,
            "inspect clipboard formats",
            "剪貼簿不包含可貼上的檔案。",
            format!("CF_HDROP unavailable: {supported:?}"),
        ));
    }
    let mut medium = Medium::get(data, &format)?;
    // SAFETY: CF_HDROP with TYMED_HGLOBAL guarantees the union contains an HDROP-compatible HGLOBAL.
    let hdrop = HDROP(unsafe { medium.value.u.hGlobal.0 });
    // SAFETY: u32::MAX asks for the file count and no destination buffer is supplied.
    let count = unsafe { DragQueryFileW(hdrop, u32::MAX, None) };
    medium.release();
    usize::try_from(count).map_err(|_| {
        clipboard_error(
            ExplorerErrorKind::Internal,
            "read clipboard item count",
            "剪貼簿項目數量無效。",
            "HDROP count did not fit usize",
        )
    })
}

fn read_hdrop_items(data: &IDataObject) -> Result<Vec<ItemDescriptor>, ExplorerError> {
    let format = hdrop_format();
    let mut medium = Medium::get(data, &format)?;
    // SAFETY: format contract is CF_HDROP/TYMED_HGLOBAL.
    let hdrop = HDROP(unsafe { medium.value.u.hGlobal.0 });
    // SAFETY: count query retains nothing.
    let count = unsafe { DragQueryFileW(hdrop, u32::MAX, None) };
    let mut items = Vec::with_capacity(usize::try_from(count).unwrap_or(0));
    for index in 0..count {
        // SAFETY: null buffer query returns UTF-16 length excluding terminator.
        let length = unsafe { DragQueryFileW(hdrop, index, None) };
        let mut buffer = vec![0_u16; usize::try_from(length).unwrap_or(0).saturating_add(1)];
        // SAFETY: buffer is writable and includes terminator capacity.
        let copied = unsafe { DragQueryFileW(hdrop, index, Some(&mut buffer)) };
        buffer.truncate(usize::try_from(copied).unwrap_or(0));
        let path = std::path::PathBuf::from(String::from_utf16_lossy(&buffer));
        let is_directory = std::fs::metadata(&path)
            .map_err(|error| {
                clipboard_error(
                    ExplorerErrorKind::Availability,
                    "inspect clipboard source",
                    "剪貼簿中的檔案已不存在或無法存取。",
                    error.to_string(),
                )
            })?
            .is_dir();
        let id = explorer_model::ShellItemId::from_provider_bytes(
            crate::navigation::filesystem_identity(&path, is_directory)?,
        )
        .ok_or_else(|| {
            clipboard_error(
                ExplorerErrorKind::Internal,
                "build clipboard item identity",
                "The clipboard item could not be identified.",
                "native filesystem identity was empty",
            )
        })?;
        items.push(ItemDescriptor {
            id,
            location: explorer_model::LocationDescriptor::file_system(path),
        });
    }
    medium.release();
    if items.is_empty() {
        Err(clipboard_error(
            ExplorerErrorKind::Availability,
            "read clipboard files",
            "剪貼簿不包含可貼上的檔案。",
            "CF_HDROP contained zero files",
        ))
    } else {
        Ok(items)
    }
}

fn preferred_mode(data: &IDataObject) -> Option<ClipboardMode> {
    // SAFETY: predefined constant is a valid NUL-terminated format name.
    let format_id = unsafe { RegisterClipboardFormatW(CFSTR_PREFERREDDROPEFFECT) };
    if format_id == 0 {
        return None;
    }
    let format = FORMATETC {
        cfFormat: u16::try_from(format_id).ok()?,
        ptd: std::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: u32::try_from(TYMED_HGLOBAL.0).ok()?,
    };
    let mut medium = Medium::get(data, &format).ok()?;
    // SAFETY: preferred-drop-effect is a DWORD in HGLOBAL.
    let pointer = unsafe { GlobalLock(medium.value.u.hGlobal) }.cast::<u32>();
    if pointer.is_null() {
        medium.release();
        return None;
    }
    // SAFETY: format specifies at least one DWORD and pointer remains locked.
    let effect = unsafe { pointer.read_unaligned() };
    // SAFETY: balances successful GlobalLock; a false return may mean the lock count reached zero.
    let _ = unsafe { GlobalUnlock(medium.value.u.hGlobal) };
    medium.release();
    if effect & DROPEFFECT_MOVE.0 != 0 {
        Some(ClipboardMode::Cut)
    } else {
        Some(ClipboardMode::Copy)
    }
}

pub(crate) fn set_drop_effect(
    data: &IDataObject,
    name: windows::core::PCWSTR,
    effect: u32,
) -> Result<(), ExplorerError> {
    // SAFETY: constant name is NUL-terminated.
    let format_id = unsafe { RegisterClipboardFormatW(name) };
    let cf_format = u16::try_from(format_id).map_err(|_| {
        clipboard_error(
            ExplorerErrorKind::Availability,
            "register clipboard effect format",
            "無法註冊 Windows 剪貼簿格式。",
            "registered clipboard format exceeded u16",
        )
    })?;
    // SAFETY: movable global allocation is transferred to IDataObject on successful SetData.
    let memory = unsafe { GlobalAlloc(GMEM_MOVEABLE, size_of::<u32>()) }
        .map_err(|error| native_clipboard_error("allocate clipboard effect", &error))?;
    // SAFETY: allocation is at least one DWORD and remains locked during the write.
    let pointer = unsafe { GlobalLock(memory) }.cast::<u32>();
    if pointer.is_null() {
        return Err(clipboard_error(
            ExplorerErrorKind::Availability,
            "lock clipboard effect",
            "無法準備 Windows 剪貼簿效果。",
            "GlobalLock returned null",
        ));
    }
    // SAFETY: pointer references one allocated DWORD.
    unsafe { pointer.write_unaligned(effect) };
    // SAFETY: balances GlobalLock before transferring HGLOBAL ownership.
    let _ = unsafe { GlobalUnlock(memory) };
    let format = FORMATETC {
        cfFormat: cf_format,
        ptd: std::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0.unsigned_abs(),
    };
    let medium = STGMEDIUM {
        tymed: TYMED_HGLOBAL.0.unsigned_abs(),
        u: windows::Win32::System::Com::STGMEDIUM_0 { hGlobal: memory },
        pUnkForRelease: std::mem::ManuallyDrop::new(None),
    };
    // SAFETY: frelease=true transfers the HGLOBAL to IDataObject on success.
    unsafe { data.SetData(&raw const format, &raw const medium, true) }
        .map_err(|error| native_clipboard_error("set clipboard drop effect", &error))
}

fn hdrop_format() -> FORMATETC {
    FORMATETC {
        cfFormat: CF_HDROP.0,
        ptd: std::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0.unsigned_abs(),
    }
}

struct Medium {
    value: STGMEDIUM,
    released: bool,
}

impl Medium {
    fn get(data: &IDataObject, format: &FORMATETC) -> Result<Self, ExplorerError> {
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        let value = loop {
            // SAFETY: format pointer is live; returned STGMEDIUM ownership is released by this guard.
            match unsafe { data.GetData(format) } {
                Ok(value) => break value,
                Err(error)
                    if error.code().0 == -2_147_221_040 && std::time::Instant::now() < deadline =>
                {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(error) => {
                    return Err(native_clipboard_error("read clipboard format", &error));
                }
            }
        };
        Ok(Self {
            value,
            released: false,
        })
    }

    fn release(&mut self) {
        if !self.released {
            // SAFETY: this guard owns the STGMEDIUM exactly once.
            unsafe { ReleaseStgMedium(&raw mut self.value) };
            self.released = true;
        }
    }
}

impl Drop for Medium {
    fn drop(&mut self) {
        self.release();
    }
}

fn native_clipboard_error(operation: &'static str, error: &windows::core::Error) -> ExplorerError {
    clipboard_error(
        ExplorerErrorKind::Availability,
        operation,
        "Windows 剪貼簿暫時無法完成要求。",
        error.to_string(),
    )
    .with_native_code(error.code().0)
}

fn clipboard_error(
    kind: ExplorerErrorKind,
    operation: &'static str,
    user: &'static str,
    detail: impl Into<String>,
) -> ExplorerError {
    ExplorerError::new(kind, operation, true, user, detail)
}

#[cfg(test)]
mod tests {
    use std::{process::Command, time::Duration};

    use explorer_model::{
        ClipboardMode, ClipboardState, ConflictDecision, ItemDescriptor, LocationDescriptor,
        ShellItemId, TransferEffects,
    };
    use windows::Win32::System::Ole::{OleInitialize, OleUninitialize};

    use super::{
        CLIPBOARD_TEST_LOCK, ClipboardRuntime, background_paste_still_owns_clipboard,
        clear_clipboard_with_retry, create_shell_data_object, paste_conflict_for_mode,
        set_clipboard_with_retry, set_drop_effect, validate_inspection_duration,
    };

    #[test]
    fn background_paste_never_overwrites_a_newer_clipboard_sequence() {
        assert!(background_paste_still_owns_clipboard(42, 42));
        assert!(!background_paste_still_owns_clipboard(42, 43));
        assert!(!background_paste_still_owns_clipboard(u32::MAX, 0));
    }

    #[test]
    fn keep_both_is_limited_to_copy_paste() {
        assert_eq!(
            paste_conflict_for_mode(ClipboardMode::Copy, ConflictDecision::KeepBoth),
            ConflictDecision::KeepBoth
        );
        assert_eq!(
            paste_conflict_for_mode(ClipboardMode::Cut, ConflictDecision::KeepBoth),
            ConflictDecision::Prompt
        );
    }

    #[test]
    #[allow(
        clippy::assertions_on_constants,
        reason = "the public transfer-effect constants are the contract under test"
    )]
    fn clipboard_domain_effects_are_explicit() {
        assert!(TransferEffects::COPY.copy);
        assert!(TransferEffects::MOVE.move_item);
        let state = ClipboardState::Owned {
            mode: ClipboardMode::Cut,
            items: vec![],
            effects: TransferEffects::MOVE,
            generation: 1,
        };
        assert!(matches!(state, ClipboardState::Owned { generation: 1, .. }));
    }

    #[test]
    fn external_shell_object_and_unsupported_clear_release_owned_state() {
        let _guard = CLIPBOARD_TEST_LOCK.lock().expect("clipboard lock");
        // SAFETY: this test owns one STA-like thread and balances OLE initialization below.
        unsafe { OleInitialize(None) }.expect("initialize OLE");
        let fixture = explorer_test_support::OwnedTempFixture::new().expect("fixture");
        let first = fixture.create_file("first.txt", b"first").expect("first");
        let second = fixture
            .create_file("second.txt", b"second")
            .expect("second");
        let items = [first, second]
            .into_iter()
            .enumerate()
            .map(|(index, path)| ItemDescriptor {
                id: ShellItemId::from_provider_bytes(vec![u8::try_from(index + 1).unwrap()])
                    .unwrap(),
                location: LocationDescriptor::file_system(path),
            })
            .collect::<Vec<_>>();
        let data = create_shell_data_object(&items).expect("Shell IDataObject");
        set_drop_effect(
            &data,
            windows::Win32::UI::Shell::CFSTR_PREFERREDDROPEFFECT,
            windows::Win32::System::Ole::DROPEFFECT_COPY.0,
        )
        .expect("preferred effect");
        let mut runtime = ClipboardRuntime::new();
        // SAFETY: OLE retains a reference and the runtime deliberately does not own this object.
        set_clipboard_with_retry(&data).expect("external clipboard object");
        assert!(matches!(
            runtime.poll_change(),
            Some(ClipboardState::External {
                item_count: Some(2),
                ..
            })
        ));
        let destination = fixture.create_dir("destination").expect("destination");
        let (request, _, mode) = runtime
            .paste_request(
                LocationDescriptor::file_system(destination),
                ConflictDecision::Prompt,
            )
            .expect("external paste request");
        assert_eq!(mode, ClipboardMode::Copy);
        assert!(matches!(
            request.kind,
            explorer_model::FileOperationKind::Copy { ref items, .. } if items.len() == 2
        ));

        let stale = fixture.create_file("stale.txt", b"stale").expect("stale");
        let stale_item = ItemDescriptor {
            id: ShellItemId::from_provider_bytes([99]).unwrap(),
            location: LocationDescriptor::file_system(&stale),
        };
        let stale_data = create_shell_data_object(&[stale_item]).expect("stale data object");
        set_clipboard_with_retry(&stale_data).expect("set stale data object");
        std::fs::remove_file(&stale).expect("remove stale source");
        assert!(
            ClipboardRuntime::new()
                .paste_request(
                    LocationDescriptor::file_system(fixture.root()),
                    ConflictDecision::Prompt,
                )
                .is_err()
        );

        // SAFETY: clears this test thread's OLE clipboard object before switching to raw formats.
        clear_clipboard_with_retry().expect("clear OLE clipboard");
        let external_text = Command::new("powershell.exe")
            .args([
                "-STA",
                "-NoProfile",
                "-Command",
                "Add-Type -AssemblyName System.Windows.Forms; for($i=0;$i -lt 20;$i++){ try { [Windows.Forms.Clipboard]::SetText('unsupported'); exit 0 } catch { Start-Sleep -Milliseconds 50 } }; exit 1",
            ])
            .output()
            .expect("external Unicode text provider");
        assert!(external_text.status.success());
        assert!(matches!(
            runtime.poll_change(),
            Some(ClipboardState::Unsupported { .. })
        ));

        clear_clipboard_with_retry().expect("clear OLE clipboard");
        assert!(matches!(
            runtime.poll_change(),
            Some(ClipboardState::None { .. })
        ));
        drop(runtime);
        // SAFETY: balances this test's OleInitialize after all COM objects are dropped.
        unsafe { OleUninitialize() };
    }

    #[test]
    fn non_path_shell_item_creates_an_owned_data_object() {
        let _guard = CLIPBOARD_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        unsafe { OleInitialize(None) }.expect("OLE initialize");
        let item = ItemDescriptor {
            id: ShellItemId::from_provider_bytes([0x51]).expect("id"),
            location: LocationDescriptor::ParsingName("shell:RecycleBinFolder".to_owned()),
        };
        let data = create_shell_data_object(&[item]).expect("namespace IDataObject");
        drop(data);
        let mut runtime = ClipboardRuntime::new();
        runtime
            .copy_or_cut(
                vec![ItemDescriptor {
                    id: ShellItemId::from_provider_bytes([0x52]).expect("id"),
                    location: LocationDescriptor::ParsingName("shell:RecycleBinFolder".to_owned()),
                }],
                ClipboardMode::Copy,
            )
            .expect("owned namespace clipboard");
        let (request, _, _) = runtime
            .paste_request(
                LocationDescriptor::ParsingName("shell:Desktop".to_owned()),
                ConflictDecision::Prompt,
            )
            .expect("namespace paste request");
        assert!(matches!(
            request.kind,
            explorer_model::FileOperationKind::Copy { .. }
        ));
        unsafe { OleUninitialize() };
    }

    #[test]
    fn slow_clipboard_probe_becomes_recoverable_error() {
        let error = validate_inspection_duration(Duration::from_millis(251))
            .expect_err("slow provider must fail explicitly");
        assert!(error.recoverable);
        assert_eq!(error.kind, explorer_common::ExplorerErrorKind::Availability);
        assert!(validate_inspection_duration(Duration::from_millis(10)).is_ok());
    }
}
