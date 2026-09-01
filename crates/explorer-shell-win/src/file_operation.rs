//! Native `IFileOperation` adapter confined to the Shell STA.
#![allow(
    unsafe_code,
    reason = "IFileOperation creation and invocation require audited COM calls on the owning STA"
)]
#![allow(
    clippy::inline_always,
    clippy::ref_as_ptr,
    reason = "windows-rs implement macro emits COM identity glue with these exact patterns"
)]

use std::{
    collections::HashSet,
    os::windows::ffi::OsStrExt as _,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard, mpsc::SyncSender},
};

use explorer_common::{ExplorerError, ExplorerErrorKind};
use explorer_model::{
    ConflictDecision, ExplorerEvent, FileOperationKind, FileOperationRequest, ItemDescriptor,
    LocationDescriptor, OperationItemOutcome, OperationItemResult, OperationProgress,
    OperationTerminal, RequestContext, ShellNewItemRecipe,
};
use windows::{
    Win32::{
        Foundation::{HWND, POINTL},
        Storage::FileSystem::{FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL},
        System::{
            Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, IPersistFile},
            Ole::{DROPEFFECT, DROPEFFECT_COPY, IDropTarget},
            SystemServices::{MK_CONTROL, MK_LBUTTON},
        },
        UI::{
            Shell::{
                Common::ITEMIDLIST, FILEOPERATION_FLAGS, FOF_ALLOWUNDO, FOF_NOCONFIRMATION,
                FOF_NOCONFIRMMKDIR, FOF_NOERRORUI, FOF_RENAMEONCOLLISION, FOFX_EARLYFAILURE,
                FOFX_RECYCLEONDELETE, FOFX_SHOWELEVATIONPROMPT, FileOperation, IFileOperation,
                IFileOperationProgressSink, IFileOperationProgressSink_Impl, IShellItem,
                IShellLinkW, SHBindToParent, SHGetNewLinkInfoW, ShellLink,
            },
            WindowsAndMessaging::{
                GetForegroundWindow, IDCANCEL, IDNO, IDYES, MB_ICONWARNING, MB_TASKMODAL,
                MB_YESNOCANCEL, MessageBoxW,
            },
        },
    },
    core::{BOOL, HRESULT, HSTRING, Interface as _, PCWSTR, Ref, implement},
};

pub(crate) fn execute(
    context: &RequestContext,
    request: &FileOperationRequest,
    events: &SyncSender<ExplorerEvent>,
) -> Result<OperationTerminal, ExplorerError> {
    if context.cancellation.is_cancelled() {
        return Ok(OperationTerminal::Cancelled);
    }
    validate_request(request)?;
    if let FileOperationKind::CreateShortcut { items } = &request.kind {
        return create_shortcuts(context, items, events);
    }
    if request.flags.conflict == ConflictDecision::Prompt
        && let Some(folder_name) = folder_merge_conflict_name(request)
    {
        match prompt_folder_merge(&folder_name) {
            FolderMergeDecision::Merge => {}
            FolderMergeDecision::Skip => {
                let mut skipped_request = request.clone();
                skipped_request.flags.conflict = ConflictDecision::Skip;
                return execute(context, &skipped_request, events);
            }
            FolderMergeDecision::Cancel => return Ok(OperationTerminal::Cancelled),
        }
    }
    let skipped = preflight_conflicts(request);
    if skipped.len() == item_count(&request.kind) {
        return Ok(OperationTerminal::Partial {
            outcomes: skipped_outcomes(&request.kind, &skipped),
        });
    }
    // SAFETY: called only on the initialized Shell STA; no aggregation and in-proc server.
    let class_id = FileOperation;
    let operation: IFileOperation =
        unsafe { CoCreateInstance(&raw const class_id, None, CLSCTX_INPROC_SERVER) }
            .map_err(|error| native_error("create IFileOperation", &error))?;
    let flags = operation_flags(request);
    // SAFETY: operation is apartment-local and flags are documented FILEOPERATION_FLAGS.
    unsafe { operation.SetOperationFlags(flags) }
        .map_err(|error| native_error("configure file operation", &error))?;
    if request.flags.conflict == ConflictDecision::Prompt {
        // A paste is initiated while the Explorer window is foreground. Giving that HWND to the
        // Shell keeps its native folder/file conflict chooser modal to the initiating window,
        // just like File Explorer, without leaking a platform handle into the shared protocol.
        // SAFETY: GetForegroundWindow returns either a live top-level HWND or NULL; the Shell does
        // not retain ownership of the HWND beyond this synchronous IFileOperation.
        let owner = unsafe { GetForegroundWindow() };
        if !owner.is_invalid() {
            // SAFETY: operation and owner are used on the owning STA for the synchronous call.
            unsafe { operation.SetOwnerWindow(owner) }
                .map_err(|error| native_error("set file operation owner", &error))?;
        }
    }

    let total_items = item_count(&request.kind);
    let _ = events.try_send(ExplorerEvent::OperationProgress {
        context: context.clone(),
        progress: OperationProgress {
            completed_items: 0,
            total_items,
            completed_bytes: 0,
            total_bytes: None,
            phase: explorer_model::TransferProgressPhase::Preparing,
            current_item: None,
        },
    });
    let sink_state = Arc::new(Mutex::new(ProgressSinkState::new(request, &skipped)));
    let sink: IFileOperationProgressSink = ProgressSink {
        context: context.clone(),
        events: events.clone(),
        state: Arc::clone(&sink_state),
    }
    .into();
    // SAFETY: the sink is an agile Rust COM implementation, retained by this scope and the
    // operation. The returned cookie is always unadvised on this same STA before release.
    let cookie = unsafe { operation.Advise(&sink) }
        .map_err(|error| native_error("subscribe file operation progress", &error))?;
    let subscription = ProgressSubscription {
        operation: operation.clone(),
        cookie,
    };
    queue_request(&operation, &request.kind, &skipped, request.flags.conflict)?;
    if context.cancellation.is_cancelled() {
        return Ok(OperationTerminal::Cancelled);
    }
    // SAFETY: every queued Shell item remains referenced by IFileOperation through completion.
    let perform_result = unsafe { operation.PerformOperations() };
    // SAFETY: querying aggregate abort state is apartment-local and has no retained output.
    let aborted = unsafe { operation.GetAnyOperationsAborted() }
        .map_err(|error| native_error("query file operation outcome", &error))?
        .as_bool();
    drop(subscription);
    if context.cancellation.is_cancelled() || aborted {
        return Ok(OperationTerminal::Cancelled);
    }
    if let Err(error) = perform_result {
        if let FileOperationKind::Copy { items, destination } = &request.kind
            && requires_shell_drop(items, destination)
        {
            return shell_namespace_copy(items, destination)
                .map(|()| OperationTerminal::Finished)
                .map_err(|mut fallback| {
                    fallback.technical_detail = format!(
                        "IFileOperation failed with {}; IDropTarget fallback failed with {}",
                        error, fallback.technical_detail
                    );
                    fallback
                });
        }
        return Err(native_error("perform file operation", &error));
    }
    write_create_item_data(&request.kind)?;
    let _ = events.try_send(ExplorerEvent::OperationProgress {
        context: context.clone(),
        progress: OperationProgress {
            completed_items: total_items,
            total_items,
            completed_bytes: 0,
            total_bytes: None,
            phase: explorer_model::TransferProgressPhase::Finalizing,
            current_item: None,
        },
    });
    let mut outcomes = skipped_outcomes(&request.kind, &skipped);
    outcomes.extend(lock_progress_state(&sink_state).outcomes.clone());
    if outcomes
        .iter()
        .all(|outcome| outcome.result == OperationItemResult::Succeeded)
    {
        Ok(OperationTerminal::Finished)
    } else {
        Ok(OperationTerminal::Partial { outcomes })
    }
}

fn folder_merge_conflict_name(request: &FileOperationRequest) -> Option<String> {
    conflict_targets(&request.kind)
        .into_iter()
        .find_map(|target| {
            let target = target?;
            let source = target.source.as_deref()?;
            (source.is_dir() && target.destination.is_dir() && !target.is_same_item()).then(|| {
                source
                    .file_name()
                    .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
            })
        })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FolderMergeDecision {
    Merge,
    Skip,
    Cancel,
}

fn prompt_folder_merge(folder_name: &str) -> FolderMergeDecision {
    let title = HSTRING::from("Confirm Folder Replace");
    let content = HSTRING::from(format!(
        "The destination already contains a folder named '{folder_name}'.\n\nIf any files have the same names, you will be asked whether to replace them.\n\nDo you still want to merge this folder?"
    ));
    // SAFETY: both strings and the foreground owner HWND remain live through this synchronous,
    // user-initiated message box on the Shell STA.
    let selected = unsafe {
        MessageBoxW(
            Some(GetForegroundWindow()),
            &content,
            &title,
            MB_YESNOCANCEL | MB_ICONWARNING | MB_TASKMODAL,
        )
    };
    match selected {
        IDYES => FolderMergeDecision::Merge,
        IDNO => FolderMergeDecision::Skip,
        IDCANCEL => FolderMergeDecision::Cancel,
        _ => FolderMergeDecision::Cancel,
    }
}

fn create_shortcuts(
    context: &RequestContext,
    items: &[ItemDescriptor],
    events: &SyncSender<ExplorerEvent>,
) -> Result<OperationTerminal, ExplorerError> {
    let total_items = items.len();
    let _ = events.try_send(ExplorerEvent::OperationProgress {
        context: context.clone(),
        progress: OperationProgress {
            completed_items: 0,
            total_items,
            completed_bytes: 0,
            total_bytes: None,
            phase: explorer_model::TransferProgressPhase::Preparing,
            current_item: None,
        },
    });
    for (index, item) in items.iter().enumerate() {
        if context.cancellation.is_cancelled() {
            return Ok(OperationTerminal::Cancelled);
        }
        create_shortcut(item)?;
        let _ = events.try_send(ExplorerEvent::OperationProgress {
            context: context.clone(),
            progress: OperationProgress {
                completed_items: index.saturating_add(1),
                total_items,
                completed_bytes: 0,
                total_bytes: None,
                phase: explorer_model::TransferProgressPhase::Transferring,
                current_item: None,
            },
        });
    }
    Ok(OperationTerminal::Finished)
}

fn create_shortcut(item: &ItemDescriptor) -> Result<PathBuf, ExplorerError> {
    let source = item.location.path().ok_or_else(|| {
        operation_error(
            "create shortcut",
            "A shortcut cannot be created for this item.",
            "selected Shell item has no filesystem path",
        )
    })?;
    let parent = source.parent().ok_or_else(|| {
        operation_error(
            "create shortcut",
            "A shortcut cannot be created here.",
            "selected filesystem item has no parent directory",
        )
    })?;
    let source_wide = nul_terminated_path(source);
    let parent_wide = nul_terminated_path(parent);
    let mut suggested = [0_u16; 260];
    let mut must_copy = BOOL::default();
    // SAFETY: both input paths are NUL-terminated and the output buffer has MAX_PATH elements as
    // required by SHGetNewLinkInfoW.
    if !unsafe {
        SHGetNewLinkInfoW(
            PCWSTR(source_wide.as_ptr()),
            PCWSTR(parent_wide.as_ptr()),
            &mut suggested,
            &raw mut must_copy,
            0,
        )
    }
    .as_bool()
    {
        return Err(operation_error(
            "create shortcut",
            "Windows could not choose a shortcut name.",
            "SHGetNewLinkInfoW returned false",
        ));
    }
    let length = suggested
        .iter()
        .position(|character| *character == 0)
        .unwrap_or(suggested.len());
    let suggested = PathBuf::from(String::from_utf16_lossy(&suggested[..length]));
    let destination = if suggested.is_absolute() {
        suggested
    } else {
        parent.join(suggested)
    };
    if must_copy.as_bool() {
        std::fs::copy(source, &destination).map_err(|error| {
            operation_error(
                "copy shortcut",
                "The shortcut could not be created.",
                &format!("copy existing shortcut: {error}"),
            )
        })?;
        return Ok(destination);
    }
    // SAFETY: this function runs only on the initialized Shell STA and creates the registered
    // in-process ShellLink COM class without aggregation.
    let link: IShellLinkW = unsafe {
        CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)
            .map_err(|error| native_error("create Shell link", &error))?
    };
    // SAFETY: source_wide remains NUL-terminated and live for the call.
    unsafe { link.SetPath(PCWSTR(source_wide.as_ptr())) }
        .map_err(|error| native_error("set Shell link target", &error))?;
    let persist: IPersistFile = link
        .cast()
        .map_err(|error| native_error("open Shell link persistence", &error))?;
    let destination_wide = nul_terminated_path(&destination);
    // SAFETY: destination_wide is NUL-terminated and the persistence object is apartment-local.
    unsafe { persist.Save(PCWSTR(destination_wide.as_ptr()), true) }
        .map_err(|error| native_error("save Shell link", &error))?;
    Ok(destination)
}

fn nul_terminated_path(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}

fn operation_flags(request: &FileOperationRequest) -> FILEOPERATION_FLAGS {
    let mut flags = base_operation_flags();
    if request.flags.conflict == ConflictDecision::Prompt {
        // These two flags answer file and folder collision questions implicitly. Removing them
        // lets the Windows Shell show its Explorer-standard folder merge/file replacement
        // chooser, including Skip, Cancel, and Apply to all, while retaining our error policy.
        flags &= !(FOF_NOCONFIRMATION | FOF_NOCONFIRMMKDIR | FOF_NOERRORUI | FOFX_EARLYFAILURE);
    }
    // PermanentDelete is a protocol-level no-recycle boundary. Ignore an accidentally permissive
    // caller flag so malformed or older clients cannot turn Shift+Delete into an undoable delete.
    if request.flags.allow_undo
        && !matches!(&request.kind, FileOperationKind::PermanentDelete { .. })
    {
        flags |= FOF_ALLOWUNDO;
    }
    if request.flags.conflict == ConflictDecision::KeepBoth {
        flags |= FOF_RENAMEONCOLLISION;
    }
    if matches!(&request.kind, FileOperationKind::RecycleDelete { .. }) {
        flags |= FOFX_RECYCLEONDELETE | FOF_ALLOWUNDO;
    }
    flags
}

fn requires_shell_drop(items: &[ItemDescriptor], destination: &LocationDescriptor) -> bool {
    destination
        .path()
        .and_then(Path::extension)
        .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
        || items.iter().any(|item| item.location.path().is_none())
}

fn shell_namespace_copy(
    items: &[ItemDescriptor],
    destination: &LocationDescriptor,
) -> Result<(), ExplorerError> {
    let data = crate::clipboard::create_shell_data_object(items)?;
    let destination = crate::navigation::location_absolute_pidl(destination)?;
    let mut child = std::ptr::null_mut::<ITEMIDLIST>();
    // SAFETY: destination is a complete live PIDL; the returned child is borrowed from it.
    let parent: windows::Win32::UI::Shell::IShellFolder =
        unsafe { SHBindToParent(destination.as_ptr(), Some(&raw mut child)) }
            .map_err(|error| native_error("bind Shell namespace drop parent", &error))?;
    // SAFETY: parent and relative child remain live through the synchronous drop session.
    let target: IDropTarget =
        unsafe { parent.GetUIObjectOf(HWND::default(), &[child.cast_const()], None) }
            .map_err(|error| native_error("bind Shell namespace drop target", &error))?;
    let mut effect: DROPEFFECT = DROPEFFECT_COPY;
    let point = POINTL::default();
    let key_state = MK_CONTROL | MK_LBUTTON;
    // SAFETY: the IDataObject, target, effect, and point remain live through the synchronous drop.
    unsafe {
        target
            .DragEnter(&data, key_state, point, &raw mut effect)
            .map_err(|error| native_error("enter Shell namespace drop target", &error))?;
        if effect.0 & DROPEFFECT_COPY.0 == 0 {
            let _ = target.DragLeave();
            return Err(ExplorerError::new(
                ExplorerErrorKind::Availability,
                "copy through Shell namespace drop target",
                true,
                "This location does not accept copied items.",
                "IDropTarget declined DROPEFFECT_COPY",
            ));
        }
        target.Drop(&data, key_state, point, &raw mut effect)
    }
    .map_err(|error| native_error("copy through Shell namespace drop target", &error))?;
    Ok(())
}

fn base_operation_flags() -> FILEOPERATION_FLAGS {
    FOF_NOCONFIRMATION
        | FOF_NOCONFIRMMKDIR
        | FOF_NOERRORUI
        | FOFX_EARLYFAILURE
        | FOFX_SHOWELEVATIONPROMPT
}

struct ProgressSubscription {
    operation: IFileOperation,
    cookie: u32,
}

impl Drop for ProgressSubscription {
    fn drop(&mut self) {
        // SAFETY: subscription and operation are released on their owning Shell STA.
        if let Err(error) = unsafe { self.operation.Unadvise(self.cookie) } {
            tracing::warn!(%error, cookie = self.cookie, "failed to unadvise file operation sink");
        }
    }
}

#[derive(Clone)]
struct OutcomeSeed {
    item: Option<ItemDescriptor>,
    destination: Option<LocationDescriptor>,
}

struct ProgressSinkState {
    seeds: Vec<OutcomeSeed>,
    outcomes: Vec<OperationItemOutcome>,
    last_progress: Option<(u32, u32)>,
    last_emitted_percent: Option<u32>,
    skipped_items: usize,
    total_items: usize,
}

impl ProgressSinkState {
    fn new(request: &FileOperationRequest, skipped: &[usize]) -> Self {
        let all_seeds = outcome_seeds(&request.kind);
        Self {
            total_items: all_seeds.len(),
            seeds: all_seeds
                .into_iter()
                .enumerate()
                .filter_map(|(index, seed)| (!skipped.contains(&index)).then_some(seed))
                .collect(),
            outcomes: Vec::new(),
            last_progress: None,
            last_emitted_percent: None,
            skipped_items: skipped.len(),
        }
    }

    fn finish_item(&mut self, result: HRESULT) {
        let seed = self
            .seeds
            .get(self.outcomes.len())
            .cloned()
            .unwrap_or(OutcomeSeed {
                item: None,
                destination: None,
            });
        let result = if result.is_ok() {
            OperationItemResult::Succeeded
        } else {
            OperationItemResult::Failed(native_error(
                "complete file operation item",
                &windows::core::Error::from_hresult(result),
            ))
        };
        self.outcomes.push(OperationItemOutcome {
            item: seed.item,
            destination: seed.destination,
            result,
        });
    }
}

fn lock_progress_state(state: &Mutex<ProgressSinkState>) -> MutexGuard<'_, ProgressSinkState> {
    state.lock().unwrap_or_else(|poisoned| {
        tracing::error!("recovering poisoned file-operation progress state");
        poisoned.into_inner()
    })
}

#[implement(IFileOperationProgressSink)]
struct ProgressSink {
    context: RequestContext,
    events: SyncSender<ExplorerEvent>,
    state: Arc<Mutex<ProgressSinkState>>,
}

#[allow(non_snake_case)]
impl IFileOperationProgressSink_Impl for ProgressSink_Impl {
    fn StartOperations(&self) -> windows::core::Result<()> {
        self.checkpoint()
    }

    fn FinishOperations(&self, _result: HRESULT) -> windows::core::Result<()> {
        Ok(())
    }

    fn PreRenameItem(
        &self,
        _flags: u32,
        _item: Ref<'_, IShellItem>,
        _name: &PCWSTR,
    ) -> windows::core::Result<()> {
        self.checkpoint()
    }
    fn PostRenameItem(
        &self,
        _flags: u32,
        _item: Ref<'_, IShellItem>,
        _name: &PCWSTR,
        result: HRESULT,
        _created: Ref<'_, IShellItem>,
    ) -> windows::core::Result<()> {
        self.finish_item(result);
        Ok(())
    }
    fn PreMoveItem(
        &self,
        _flags: u32,
        _item: Ref<'_, IShellItem>,
        _destination: Ref<'_, IShellItem>,
        _name: &PCWSTR,
    ) -> windows::core::Result<()> {
        self.checkpoint()
    }
    fn PostMoveItem(
        &self,
        _flags: u32,
        _item: Ref<'_, IShellItem>,
        _destination: Ref<'_, IShellItem>,
        _name: &PCWSTR,
        result: HRESULT,
        _created: Ref<'_, IShellItem>,
    ) -> windows::core::Result<()> {
        self.finish_item(result);
        Ok(())
    }
    fn PreCopyItem(
        &self,
        _flags: u32,
        _item: Ref<'_, IShellItem>,
        _destination: Ref<'_, IShellItem>,
        _name: &PCWSTR,
    ) -> windows::core::Result<()> {
        self.checkpoint()
    }
    fn PostCopyItem(
        &self,
        _flags: u32,
        _item: Ref<'_, IShellItem>,
        _destination: Ref<'_, IShellItem>,
        _name: &PCWSTR,
        result: HRESULT,
        _created: Ref<'_, IShellItem>,
    ) -> windows::core::Result<()> {
        self.finish_item(result);
        Ok(())
    }
    fn PreDeleteItem(&self, _flags: u32, _item: Ref<'_, IShellItem>) -> windows::core::Result<()> {
        self.checkpoint()
    }
    fn PostDeleteItem(
        &self,
        _flags: u32,
        _item: Ref<'_, IShellItem>,
        result: HRESULT,
        _created: Ref<'_, IShellItem>,
    ) -> windows::core::Result<()> {
        self.finish_item(result);
        Ok(())
    }
    fn PreNewItem(
        &self,
        _flags: u32,
        _destination: Ref<'_, IShellItem>,
        _name: &PCWSTR,
    ) -> windows::core::Result<()> {
        self.checkpoint()
    }
    fn PostNewItem(
        &self,
        _flags: u32,
        _destination: Ref<'_, IShellItem>,
        _name: &PCWSTR,
        _template: &PCWSTR,
        _attributes: u32,
        result: HRESULT,
        _created: Ref<'_, IShellItem>,
    ) -> windows::core::Result<()> {
        self.finish_item(result);
        Ok(())
    }

    fn UpdateProgress(&self, total: u32, completed: u32) -> windows::core::Result<()> {
        self.checkpoint()?;
        let mut state = lock_progress_state(&self.state);
        if state.last_progress == Some((total, completed)) {
            return Ok(());
        }
        state.last_progress = Some((total, completed));
        let percent = completed
            .saturating_mul(100)
            .checked_div(total)
            .unwrap_or(100);
        if state.last_emitted_percent == Some(percent) {
            return Ok(());
        }
        state.last_emitted_percent = Some(percent);
        let completed_items = state.skipped_items + state.outcomes.len();
        let total_items = state.total_items;
        drop(state);
        let _ = self.events.try_send(ExplorerEvent::OperationProgress {
            context: self.context.clone(),
            progress: OperationProgress {
                completed_items,
                total_items,
                completed_bytes: u64::from(completed),
                total_bytes: Some(u64::from(total)),
                phase: explorer_model::TransferProgressPhase::Transferring,
                current_item: None,
            },
        });
        Ok(())
    }

    fn ResetTimer(&self) -> windows::core::Result<()> {
        Ok(())
    }
    fn PauseTimer(&self) -> windows::core::Result<()> {
        Ok(())
    }
    fn ResumeTimer(&self) -> windows::core::Result<()> {
        Ok(())
    }
}

impl ProgressSink_Impl {
    fn checkpoint(&self) -> windows::core::Result<()> {
        if self.context.cancellation.is_cancelled() {
            Err(windows::core::Error::from_hresult(HRESULT(-2_147_023_673)))
        } else {
            Ok(())
        }
    }

    fn finish_item(&self, result: HRESULT) {
        lock_progress_state(&self.state).finish_item(result);
    }
}

fn outcome_seeds(kind: &FileOperationKind) -> Vec<OutcomeSeed> {
    match kind {
        FileOperationKind::CreateFolder { parent, .. }
        | FileOperationKind::CreateItem { parent, .. } => vec![OutcomeSeed {
            item: None,
            destination: Some(parent.clone()),
        }],
        FileOperationKind::Rename { item, .. } | FileOperationKind::SetUnixMode { item, .. } => {
            vec![OutcomeSeed {
                item: Some(item.clone()),
                destination: None,
            }]
        }
        FileOperationKind::Copy { items, destination }
        | FileOperationKind::Move { items, destination } => items
            .iter()
            .cloned()
            .map(|item| OutcomeSeed {
                item: Some(item),
                destination: Some(destination.clone()),
            })
            .collect(),
        FileOperationKind::RecycleDelete { items }
        | FileOperationKind::PermanentDelete { items, .. }
        | FileOperationKind::CreateShortcut { items } => items
            .iter()
            .cloned()
            .map(|item| OutcomeSeed {
                item: Some(item),
                destination: None,
            })
            .collect(),
    }
}

fn skipped_outcomes(kind: &FileOperationKind, skipped: &[usize]) -> Vec<OperationItemOutcome> {
    outcome_seeds(kind)
        .into_iter()
        .enumerate()
        .filter_map(|(index, seed)| {
            skipped.contains(&index).then_some(OperationItemOutcome {
                item: seed.item,
                destination: seed.destination,
                result: OperationItemResult::Skipped,
            })
        })
        .collect()
}

fn queue_request(
    operation: &IFileOperation,
    kind: &FileOperationKind,
    skipped: &[usize],
    conflict: ConflictDecision,
) -> Result<(), ExplorerError> {
    match kind {
        FileOperationKind::CreateFolder { parent, name } => {
            if skipped.contains(&0) {
                return Ok(());
            }
            let parent = crate::navigation::shell_item(parent)?;
            let name = HSTRING::from(name);
            // SAFETY: COM references and HSTRING remain live; None selects the global sink.
            unsafe { operation.NewItem(&parent, FILE_ATTRIBUTE_DIRECTORY.0, &name, None, None) }
                .map_err(|error| native_error("queue create folder", &error))
        }
        FileOperationKind::CreateItem {
            parent,
            name,
            recipe,
        } => {
            if skipped.contains(&0) {
                return Ok(());
            }
            let parent = crate::navigation::shell_item(parent)?;
            let name = HSTRING::from(name);
            let attributes = if matches!(recipe, ShellNewItemRecipe::Folder) {
                FILE_ATTRIBUTE_DIRECTORY.0
            } else {
                FILE_ATTRIBUTE_NORMAL.0
            };
            let template = match recipe {
                ShellNewItemRecipe::TemplateFile(path) => Some(HSTRING::from(path.as_os_str())),
                _ => None,
            };
            let template_path = template.as_ref().map(|value| PCWSTR(value.as_ptr()));
            unsafe { operation.NewItem(&parent, attributes, &name, template_path.as_ref(), None) }
                .map_err(|error| native_error("queue create item", &error))
        }
        FileOperationKind::Rename { item, new_name } => {
            if skipped.contains(&0) {
                return Ok(());
            }
            let item = crate::navigation::shell_item(&item.location)?;
            let name = HSTRING::from(new_name);
            // SAFETY: references and HSTRING remain live through the synchronous queue call.
            unsafe { operation.RenameItem(&item, &name, None) }
                .map_err(|error| native_error("queue rename", &error))
        }
        FileOperationKind::Copy { items, destination } => {
            let copy_names = keep_both_copy_names(items, destination, conflict)?;
            let destination = crate::navigation::shell_item(destination)?;
            for (index, item) in items.iter().enumerate() {
                if skipped.contains(&index) {
                    continue;
                }
                let item = crate::navigation::shell_item(&item.location)?;
                if let Some(copy_name) = &copy_names[index] {
                    let copy_name = HSTRING::from(copy_name);
                    // SAFETY: item/destination references and copy_name remain live through the
                    // synchronous queue call.
                    unsafe { operation.CopyItem(&item, &destination, &copy_name, None) }
                        .map_err(|error| native_error("queue collision-safe copy", &error))?;
                } else {
                    // SAFETY: item/destination references remain live through the queue call.
                    unsafe { operation.CopyItem(&item, &destination, None, None) }
                        .map_err(|error| native_error("queue copy", &error))?;
                }
            }
            Ok(())
        }
        FileOperationKind::Move { items, destination } => {
            let destination = crate::navigation::shell_item(destination)?;
            for (index, item) in items.iter().enumerate() {
                if skipped.contains(&index) {
                    continue;
                }
                let item = crate::navigation::shell_item(&item.location)?;
                // SAFETY: item/destination references remain live through the queue call.
                unsafe { operation.MoveItem(&item, &destination, None, None) }
                    .map_err(|error| native_error("queue move", &error))?;
            }
            Ok(())
        }
        FileOperationKind::RecycleDelete { items }
        | FileOperationKind::PermanentDelete { items, .. } => {
            for (index, item) in items.iter().enumerate() {
                if skipped.contains(&index) {
                    continue;
                }
                let item = crate::navigation::shell_item(&item.location)?;
                // SAFETY: item reference remains live through the queue call.
                unsafe { operation.DeleteItem(&item, None) }
                    .map_err(|error| native_error("queue delete", &error))?;
            }
            Ok(())
        }
        FileOperationKind::CreateShortcut { .. } => Ok(()),
        FileOperationKind::SetUnixMode { .. } => Err(ExplorerError::new(
            ExplorerErrorKind::Availability,
            "set Unix mode",
            false,
            "Windows 本機檔案不支援遠端權限編輯。",
            "SetUnixMode reached the local Shell provider",
        )),
    }
}

fn keep_both_copy_names(
    items: &[ItemDescriptor],
    destination: &LocationDescriptor,
    conflict: ConflictDecision,
) -> Result<Vec<Option<String>>, ExplorerError> {
    if conflict != ConflictDecision::KeepBoth {
        return Ok(vec![None; items.len()]);
    }
    let Some(destination) = destination.path() else {
        // Virtual destinations cannot be probed with std::fs. FOF_RENAMEONCOLLISION remains set
        // and lets the owning namespace provider choose a safe name.
        return Ok(vec![None; items.len()]);
    };
    let mut reserved = HashSet::with_capacity(items.len());
    let mut names = Vec::with_capacity(items.len());
    for item in items {
        let Some(source) = item.location.path() else {
            names.push(None);
            continue;
        };
        let Some(original) = source
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
        else {
            names.push(None);
            continue;
        };
        let original_key = original.to_lowercase();
        if !destination.join(&original).exists() && reserved.insert(original_key) {
            names.push(None);
            continue;
        }
        let mut selected = None;
        for ordinal in 1..=1_000_000_u32 {
            let candidate = numbered_copy_name(source, ordinal);
            if !destination.join(&candidate).exists() && reserved.insert(candidate.to_lowercase()) {
                selected = Some(candidate);
                break;
            }
        }
        let Some(selected) = selected else {
            return Err(ExplorerError::new(
                ExplorerErrorKind::Availability,
                "choose collision-safe copy name",
                true,
                "無法為複製的項目產生可用名稱。",
                "one million numbered copy names were already reserved",
            ));
        };
        names.push(Some(selected));
    }
    Ok(names)
}

fn numbered_copy_name(source: &Path, ordinal: u32) -> String {
    let file_name = source
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    if source.is_dir() {
        return format!("{file_name}_{ordinal}");
    }
    let extension = source
        .extension()
        .filter(|extension| !extension.is_empty())
        .map(|extension| extension.to_string_lossy());
    let stem = source
        .file_stem()
        .map_or_else(|| file_name.as_str().into(), |stem| stem.to_string_lossy());
    extension.map_or_else(
        || format!("{file_name}_{ordinal}"),
        |extension| format!("{stem}_{ordinal}.{extension}"),
    )
}

fn validate_request(request: &FileOperationRequest) -> Result<(), ExplorerError> {
    if let FileOperationKind::CreateItem { recipe, .. } = &request.kind
        && let Err(error) = recipe.validate()
    {
        return Err(ExplorerError::new(
            ExplorerErrorKind::Input,
            "validate ShellNew recipe",
            true,
            "This registered file type cannot be created safely.",
            format!("invalid owned ShellNew recipe: {error:?}"),
        ));
    }
    match &request.kind {
        FileOperationKind::CreateFolder { name, .. }
        | FileOperationKind::CreateItem { name, .. }
        | FileOperationKind::Rename { new_name: name, .. } => validate_name(name),
        FileOperationKind::PermanentDelete {
            confirmed: false, ..
        } => Err(ExplorerError::new(
            ExplorerErrorKind::Cancellation,
            "permanent delete confirmation",
            true,
            "已取消永久刪除。",
            "permanent delete request was not explicitly confirmed",
        )),
        FileOperationKind::Copy { items, .. }
        | FileOperationKind::Move { items, .. }
        | FileOperationKind::RecycleDelete { items }
        | FileOperationKind::PermanentDelete { items, .. }
        | FileOperationKind::CreateShortcut { items }
            if items.is_empty() =>
        {
            Err(ExplorerError::new(
                ExplorerErrorKind::Input,
                "validate file operation",
                true,
                "請先選取至少一個項目。",
                "operation item list was empty",
            ))
        }
        _ => Ok(()),
    }
}

fn preflight_conflicts(request: &FileOperationRequest) -> Vec<usize> {
    let targets = conflict_targets(&request.kind);
    let conflicts: Vec<usize> = targets
        .iter()
        .enumerate()
        .filter_map(|(index, target)| {
            target
                .as_ref()
                .filter(|target| target.destination.exists() && !target.is_same_item())
                .map(|_| index)
        })
        .collect();
    match request.flags.conflict {
        ConflictDecision::Skip => conflicts,
        ConflictDecision::Prompt | ConflictDecision::Replace | ConflictDecision::KeepBoth => {
            Vec::new()
        }
    }
}

#[derive(Clone)]
struct ConflictTarget {
    source: Option<PathBuf>,
    destination: PathBuf,
}

impl ConflictTarget {
    fn is_same_item(&self) -> bool {
        self.source
            .as_deref()
            .is_some_and(|source| paths_equal_windows(source, &self.destination))
    }
}

fn conflict_targets(kind: &FileOperationKind) -> Vec<Option<ConflictTarget>> {
    match kind {
        FileOperationKind::CreateFolder { parent, name }
        | FileOperationKind::CreateItem { parent, name, .. } => {
            vec![parent.path().map(|parent| ConflictTarget {
                source: None,
                destination: parent.join(name),
            })]
        }
        FileOperationKind::Rename { item, new_name } => vec![item.location.path().map(|source| {
            ConflictTarget {
                source: Some(source.to_path_buf()),
                destination: source
                    .parent()
                    .unwrap_or_else(|| Path::new(""))
                    .join(new_name),
            }
        })],
        FileOperationKind::SetUnixMode { .. } => vec![None],
        FileOperationKind::Copy { items, destination }
        | FileOperationKind::Move { items, destination } => items
            .iter()
            .map(|item| {
                let source = item.location.path()?;
                let destination = destination.path()?;
                Some(ConflictTarget {
                    source: Some(source.to_path_buf()),
                    destination: destination.join(source.file_name()?),
                })
            })
            .collect(),
        FileOperationKind::RecycleDelete { items }
        | FileOperationKind::PermanentDelete { items, .. }
        | FileOperationKind::CreateShortcut { items } => vec![None; items.len()],
    }
}

fn paths_equal_windows(left: &Path, right: &Path) -> bool {
    left.as_os_str()
        .to_string_lossy()
        .eq_ignore_ascii_case(&right.as_os_str().to_string_lossy())
}

fn validate_name(name: &str) -> Result<(), ExplorerError> {
    if explorer_model::validate_windows_file_name(name).is_err() {
        Err(ExplorerError::new(
            ExplorerErrorKind::Input,
            "validate file name",
            true,
            "檔案名稱無效。",
            "name was empty, ended in space/dot, or contained a reserved Windows character",
        ))
    } else {
        Ok(())
    }
}

fn item_count(kind: &FileOperationKind) -> usize {
    match kind {
        FileOperationKind::CreateFolder { .. }
        | FileOperationKind::CreateItem { .. }
        | FileOperationKind::Rename { .. }
        | FileOperationKind::SetUnixMode { .. } => 1,
        FileOperationKind::Copy { items, .. }
        | FileOperationKind::Move { items, .. }
        | FileOperationKind::RecycleDelete { items }
        | FileOperationKind::PermanentDelete { items, .. }
        | FileOperationKind::CreateShortcut { items } => items.len(),
    }
}

fn write_create_item_data(kind: &FileOperationKind) -> Result<(), ExplorerError> {
    let FileOperationKind::CreateItem {
        parent,
        name,
        recipe: ShellNewItemRecipe::Data(data),
    } = kind
    else {
        return Ok(());
    };
    let Some(parent) = parent.path() else {
        return Err(ExplorerError::new(
            ExplorerErrorKind::Availability,
            "initialize new item data",
            true,
            "This registered file type cannot be initialized in this location.",
            "ShellNew Data requires a filesystem destination",
        ));
    };
    std::fs::write(parent.join(name), data).map_err(|error| {
        ExplorerError::new(
            ExplorerErrorKind::Availability,
            "initialize new item data",
            true,
            "The file was created, but its registered initial content could not be written.",
            error.to_string(),
        )
    })
}

fn native_error(operation: &'static str, error: &windows::core::Error) -> ExplorerError {
    let native_code = error.code().0;
    let kind = match native_code {
        -2_147_024_891 => ExplorerErrorKind::Authorization, // E_ACCESSDENIED
        -2_147_024_816 | -2_147_024_713 => ExplorerErrorKind::Conflict,
        -2_147_023_673 => ExplorerErrorKind::Cancellation,
        _ => ExplorerErrorKind::Availability,
    };
    ExplorerError::new(
        kind,
        operation,
        true,
        "Windows 無法完成檔案操作。",
        error.to_string(),
    )
    .with_native_code(native_code)
}

fn operation_error(
    operation: &'static str,
    user_message: &'static str,
    technical_detail: &str,
) -> ExplorerError {
    ExplorerError::new(
        ExplorerErrorKind::Availability,
        operation,
        true,
        user_message,
        technical_detail,
    )
}

#[cfg(test)]
mod tests {
    use std::{fs::OpenOptions, os::windows::fs::OpenOptionsExt as _, path::Path};

    use super::{
        base_operation_flags, execute, folder_merge_conflict_name, keep_both_copy_names,
        numbered_copy_name, operation_flags, preflight_conflicts, validate_name, validate_request,
    };
    use explorer_model::{
        ConflictDecision, FileOperationFlags, FileOperationKind, FileOperationRequest,
        ItemDescriptor, LocationDescriptor, ShellItemId,
    };
    use windows::Win32::UI::Shell::{
        FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOCONFIRMMKDIR, FOF_NOERRORUI, FOFX_EARLYFAILURE,
        FOFX_RECYCLEONDELETE, FOFX_SHOWELEVATIONPROMPT,
    };

    #[test]
    fn native_file_operations_prompt_for_elevation_only_when_required() {
        let flags = base_operation_flags();

        assert_ne!((flags & FOF_NOERRORUI).0, 0);
        assert_ne!((flags & FOFX_SHOWELEVATIONPROMPT).0, 0);
    }

    #[test]
    fn prompt_conflicts_enable_the_native_shell_chooser() {
        let prompt = FileOperationRequest {
            kind: FileOperationKind::Copy {
                items: vec![],
                destination: LocationDescriptor::file_system(r"C:\fixture"),
            },
            flags: FileOperationFlags {
                conflict: ConflictDecision::Prompt,
                ..FileOperationFlags::default()
            },
        };
        let mut replace = prompt.clone();
        replace.flags.conflict = ConflictDecision::Replace;

        assert_eq!((operation_flags(&prompt) & FOF_NOCONFIRMATION).0, 0);
        assert_eq!((operation_flags(&prompt) & FOF_NOCONFIRMMKDIR).0, 0);
        assert_eq!((operation_flags(&prompt) & FOF_NOERRORUI).0, 0);
        assert_eq!((operation_flags(&prompt) & FOFX_EARLYFAILURE).0, 0);
        assert_ne!((operation_flags(&replace) & FOF_NOCONFIRMATION).0, 0);
        assert_ne!((operation_flags(&replace) & FOF_NOCONFIRMMKDIR).0, 0);
        assert_ne!((operation_flags(&replace) & FOF_NOERRORUI).0, 0);
    }

    #[test]
    fn same_named_directories_are_classified_for_the_folder_prompt() {
        let fixture = tempfile::tempdir().expect("fixture");
        let source_parent = fixture.path().join("source");
        let destination_parent = fixture.path().join("destination");
        let source = source_parent.join("SameFolder");
        std::fs::create_dir_all(&source).expect("source folder");
        std::fs::create_dir_all(destination_parent.join("SameFolder")).expect("destination folder");
        let request = FileOperationRequest {
            kind: FileOperationKind::Copy {
                items: vec![ItemDescriptor {
                    id: ShellItemId::from_provider_bytes([1]).expect("source id"),
                    location: LocationDescriptor::file_system(source),
                }],
                destination: LocationDescriptor::file_system(destination_parent),
            },
            flags: FileOperationFlags::default(),
        };

        assert_eq!(
            folder_merge_conflict_name(&request).as_deref(),
            Some("SameFolder")
        );
    }

    #[test]
    fn permanent_delete_never_sets_recycle_or_undo_flags_even_for_permissive_callers() {
        let item = ItemDescriptor {
            id: ShellItemId::from_provider_bytes([1]).expect("item identity"),
            location: LocationDescriptor::file_system(r"C:\fixture\delete-me.txt"),
        };
        let permanent = FileOperationRequest {
            kind: FileOperationKind::PermanentDelete {
                items: vec![item.clone()],
                confirmed: true,
            },
            flags: FileOperationFlags::default(),
        };
        let flags = operation_flags(&permanent);
        assert_eq!((flags & FOF_ALLOWUNDO).0, 0);
        assert_eq!((flags & FOFX_RECYCLEONDELETE).0, 0);

        let recycle = FileOperationRequest {
            kind: FileOperationKind::RecycleDelete { items: vec![item] },
            flags: FileOperationFlags::default(),
        };
        let flags = operation_flags(&recycle);
        assert_ne!((flags & FOF_ALLOWUNDO).0, 0);
        assert_ne!((flags & FOFX_RECYCLEONDELETE).0, 0);
    }

    #[test]
    fn windows_name_validation_rejects_reserved_and_trailing_characters() {
        for invalid in [
            "",
            ".",
            "..",
            "CON",
            "aux.txt",
            "LPT9.log",
            "bad?.txt",
            "bad/name",
            "bad\u{7}.txt",
            "trailing.",
            "trailing ",
        ] {
            assert!(validate_name(invalid).is_err(), "accepted {invalid:?}");
        }
        assert!(validate_name("合法-😀.txt").is_ok());
    }

    #[test]
    fn destructive_validation_rejects_unconfirmed_and_empty_requests() {
        let unconfirmed = FileOperationRequest {
            kind: FileOperationKind::PermanentDelete {
                items: vec![],
                confirmed: false,
            },
            flags: FileOperationFlags::default(),
        };
        assert!(validate_request(&unconfirmed).is_err());
        let empty_copy = FileOperationRequest {
            kind: FileOperationKind::Copy {
                items: vec![],
                destination: LocationDescriptor::file_system(r"C:\fixture"),
            },
            flags: FileOperationFlags::default(),
        };
        assert!(validate_request(&empty_copy).is_err());
    }

    #[test]
    fn collision_preflight_delegates_prompt_to_shell_and_skip_is_per_item() {
        let fixture = tempfile::tempdir().expect("fixture");
        let source = fixture.path().join("source");
        let destination = fixture.path().join("destination");
        std::fs::create_dir_all(&source).expect("source");
        std::fs::create_dir_all(&destination).expect("destination");
        let first = source.join("first.txt");
        let second = source.join("second.txt");
        std::fs::write(&first, b"first").expect("first");
        std::fs::write(&second, b"second").expect("second");
        std::fs::write(destination.join("first.txt"), b"existing").expect("collision");
        let items = [first, second]
            .into_iter()
            .enumerate()
            .map(|(index, path)| ItemDescriptor {
                id: ShellItemId::from_provider_bytes(vec![u8::try_from(index + 1).unwrap()])
                    .expect("identity"),
                location: LocationDescriptor::file_system(path),
            })
            .collect();
        let mut request = FileOperationRequest {
            kind: FileOperationKind::Copy {
                items,
                destination: LocationDescriptor::file_system(destination),
            },
            flags: FileOperationFlags {
                conflict: ConflictDecision::Prompt,
                ..FileOperationFlags::default()
            },
        };
        assert!(preflight_conflicts(&request).is_empty());
        request.flags.conflict = ConflictDecision::Skip;
        assert_eq!(preflight_conflicts(&request), vec![0]);
        request.flags.conflict = ConflictDecision::Replace;
        assert!(preflight_conflicts(&request).is_empty());
        request.flags.conflict = ConflictDecision::KeepBoth;
        assert!(preflight_conflicts(&request).is_empty());
    }

    #[test]
    fn keep_both_names_insert_monotonic_suffix_before_the_last_extension() {
        let fixture = tempfile::tempdir().expect("fixture");
        let directory = fixture.path().join("Folder.name");
        std::fs::create_dir(&directory).expect("directory");
        assert_eq!(numbered_copy_name(Path::new("File.exe"), 1), "File_1.exe");
        assert_eq!(
            numbered_copy_name(Path::new("archive.tar.gz"), 2),
            "archive.tar_2.gz"
        );
        assert_eq!(numbered_copy_name(Path::new("README"), 3), "README_3");
        assert_eq!(
            numbered_copy_name(&directory, 4),
            "Folder.name_4",
            "directory dots are part of the directory name, not an extension"
        );
    }

    #[test]
    fn repeated_keep_both_copy_creates_file_1_then_file_2_without_overwrite() {
        let _apartment = crate::sta::ApartmentGuard::initialize().expect("STA");
        let fixture = tempfile::tempdir().expect("fixture");
        let source_dir = fixture.path().join("source");
        let destination = fixture.path().join("destination");
        std::fs::create_dir_all(&source_dir).expect("source directory");
        std::fs::create_dir_all(&destination).expect("destination directory");
        let source = source_dir.join("File.exe");
        std::fs::write(&source, b"new executable").expect("source file");
        std::fs::write(destination.join("File.exe"), b"existing executable")
            .expect("existing destination");
        let item = ItemDescriptor {
            id: ShellItemId::from_provider_bytes([1]).expect("source identity"),
            location: LocationDescriptor::file_system(&source),
        };
        let request = FileOperationRequest {
            kind: FileOperationKind::Copy {
                items: vec![item],
                destination: LocationDescriptor::file_system(&destination),
            },
            flags: FileOperationFlags {
                conflict: ConflictDecision::KeepBoth,
                ..FileOperationFlags::default()
            },
        };
        let context = explorer_model::RequestContext::new(
            explorer_model::TabId::new(),
            explorer_model::Generation::new(1),
        );
        let (events, _receiver) = std::sync::mpsc::sync_channel(64);
        for expected in ["File_1.exe", "File_2.exe"] {
            let outcome = execute(&context, &request, &events).expect("keep-both copy");
            assert!(matches!(
                outcome,
                explorer_model::OperationTerminal::Finished
                    | explorer_model::OperationTerminal::Partial { .. }
            ));
            assert_eq!(
                std::fs::read(destination.join(expected)).expect("numbered copy"),
                b"new executable"
            );
        }
        assert_eq!(
            std::fs::read(destination.join("File.exe")).expect("original destination"),
            b"existing executable"
        );
        let planned = keep_both_copy_names(
            match &request.kind {
                FileOperationKind::Copy { items, .. } => items,
                _ => unreachable!(),
            },
            &LocationDescriptor::file_system(&destination),
            ConflictDecision::KeepBoth,
        )
        .expect("next name");
        assert_eq!(planned, [Some("File_3.exe".to_owned())]);
    }

    #[test]
    fn host_context_command_create_shortcut_persists_a_real_lnk() {
        let _apartment = crate::sta::ApartmentGuard::initialize().expect("STA");
        let fixture = tempfile::tempdir().expect("fixture");
        let source = fixture.path().join("shortcut-target.txt");
        std::fs::write(&source, b"target").expect("source file");
        let request = FileOperationRequest {
            kind: FileOperationKind::CreateShortcut {
                items: vec![ItemDescriptor {
                    id: ShellItemId::from_provider_bytes([0x51]).expect("identity"),
                    location: LocationDescriptor::file_system(&source),
                }],
            },
            flags: FileOperationFlags::default(),
        };
        let context = explorer_model::RequestContext::new(
            explorer_model::TabId::new(),
            explorer_model::Generation::new(1),
        );
        let (events, _receiver) = std::sync::mpsc::sync_channel(64);
        assert_eq!(
            execute(&context, &request, &events).expect("create shortcut"),
            explorer_model::OperationTerminal::Finished
        );
        let links = std::fs::read_dir(fixture.path())
            .expect("fixture entries")
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("lnk"))
            })
            .collect::<Vec<_>>();
        assert_eq!(links.len(), 1, "exactly one collision-safe .lnk is created");
    }

    #[test]
    fn locked_delete_preserves_the_native_sharing_violation_for_recovery() {
        let _apartment = crate::sta::ApartmentGuard::initialize().expect("STA");
        let fixture = tempfile::tempdir().expect("fixture");
        let path = fixture.path().join("locked.txt");
        let handle = OpenOptions::new()
            .write(true)
            .create_new(true)
            .share_mode(0)
            .open(&path)
            .expect("exclusive lock");
        let request = FileOperationRequest {
            kind: FileOperationKind::PermanentDelete {
                items: vec![ItemDescriptor {
                    id: ShellItemId::from_provider_bytes([0x44]).expect("identity"),
                    location: LocationDescriptor::file_system(&path),
                }],
                confirmed: true,
            },
            flags: FileOperationFlags {
                allow_undo: false,
                require_confirmation: true,
                ..FileOperationFlags::default()
            },
        };
        let context = explorer_model::RequestContext::new(
            explorer_model::TabId::new(),
            explorer_model::Generation::new(1),
        );
        let (events, _receiver) = std::sync::mpsc::sync_channel(64);
        let terminal = execute(&context, &request, &events);
        let native_codes = match terminal {
            Err(error) => vec![error.native_code],
            Ok(explorer_model::OperationTerminal::Partial { outcomes }) => outcomes
                .into_iter()
                .filter_map(|outcome| match outcome.result {
                    explorer_model::OperationItemResult::Failed(error) => Some(error.native_code),
                    _ => None,
                })
                .collect(),
            other => panic!("exclusive delete unexpectedly completed: {other:?}"),
        };
        eprintln!("locked-delete-native-codes={native_codes:?}");
        assert!(
            native_codes
                .iter()
                .copied()
                .flatten()
                .any(|code| { explorer_model::DeleteLockKind::from_native_code(code).is_some() })
        );
        assert!(path.exists());
        drop(handle);
    }

    #[test]
    fn same_named_directory_replace_merges_contents_without_losing_destination_only_files() {
        let _apartment = crate::sta::ApartmentGuard::initialize().expect("STA");
        let fixture = tempfile::tempdir().expect("fixture");
        let source_parent = fixture.path().join("source");
        let destination_parent = fixture.path().join("destination");
        let source = source_parent.join("SameFolder");
        let destination = destination_parent.join("SameFolder");
        std::fs::create_dir_all(&source).expect("source folder");
        std::fs::create_dir_all(&destination).expect("destination folder");
        std::fs::write(source.join("source-only.txt"), b"source only").expect("source-only");
        std::fs::write(source.join("conflict.txt"), b"new bytes").expect("source conflict");
        std::fs::write(
            destination.join("destination-only.txt"),
            b"destination only",
        )
        .expect("destination-only");
        std::fs::write(destination.join("conflict.txt"), b"old bytes")
            .expect("destination conflict");
        let request = FileOperationRequest {
            kind: FileOperationKind::Copy {
                items: vec![ItemDescriptor {
                    id: ShellItemId::from_provider_bytes([1]).expect("source id"),
                    location: LocationDescriptor::file_system(&source),
                }],
                destination: LocationDescriptor::file_system(&destination_parent),
            },
            flags: FileOperationFlags {
                conflict: ConflictDecision::Replace,
                ..FileOperationFlags::default()
            },
        };
        let context = explorer_model::RequestContext::new(
            explorer_model::TabId::new(),
            explorer_model::Generation::default(),
        );
        let (events, _receiver) = std::sync::mpsc::sync_channel(64);

        assert_eq!(
            execute(&context, &request, &events).expect("merge same-named directory"),
            explorer_model::OperationTerminal::Finished
        );
        assert_eq!(
            std::fs::read(destination.join("source-only.txt")).expect("copied source-only"),
            b"source only"
        );
        assert_eq!(
            std::fs::read(destination.join("destination-only.txt"))
                .expect("preserved destination-only"),
            b"destination only"
        );
        assert_eq!(
            std::fs::read(destination.join("conflict.txt")).expect("replaced conflict"),
            b"new bytes"
        );
    }

    #[test]
    fn windows_zip_namespace_copy_in_and_out_uses_shell_file_operation() {
        let _apartment = crate::sta::ApartmentGuard::initialize().expect("STA");
        let fixture =
            explorer_test_support::OwnedTempFixture::new().expect("ZIP operation fixture");
        let source = fixture
            .create_file("inside.txt", b"Windows owns ZIP namespace parsing")
            .expect("source");
        let _placeholder = fixture
            .create_file("placeholder.txt", b"placeholder")
            .expect("placeholder");
        let archive = fixture.root().join("container.zip");
        let tar = std::process::Command::new("tar.exe")
            .args(["-a", "-c", "-f"])
            .arg(&archive)
            .arg("-C")
            .arg(fixture.root())
            .arg("placeholder.txt")
            .status()
            .expect("start Windows tar");
        assert!(tar.success(), "create valid ZIP fixture");
        let output = fixture.root().join("output");
        std::fs::create_dir(&output).expect("output");
        let context = explorer_model::RequestContext::new(
            explorer_model::TabId::new(),
            explorer_model::Generation::default(),
        );
        let (events, _receiver) = std::sync::mpsc::sync_channel(64);
        let copy_in = FileOperationRequest {
            kind: FileOperationKind::Copy {
                items: vec![ItemDescriptor {
                    id: ShellItemId::from_provider_bytes([1]).expect("source id"),
                    location: LocationDescriptor::file_system(&source),
                }],
                destination: LocationDescriptor::file_system(&archive),
            },
            flags: FileOperationFlags {
                conflict: ConflictDecision::Replace,
                ..FileOperationFlags::default()
            },
        };
        assert!(matches!(
            execute(&context, &copy_in, &events).expect("copy into ZIP"),
            explorer_model::OperationTerminal::Finished
                | explorer_model::OperationTerminal::Partial { .. }
        ));

        let mut inside = None;
        for _ in 0..50 {
            let resolved =
                crate::navigation::resolve_location(&LocationDescriptor::file_system(&archive))
                    .expect("resolve populated ZIP");
            let mut children = Vec::new();
            crate::navigation::enumerate_directory(&context, &resolved, |event| {
                if let explorer_model::ExplorerEvent::DirectoryBatch { entries, .. } = event {
                    children.extend(entries);
                }
                true
            })
            .expect("enumerate populated ZIP");
            inside = children
                .into_iter()
                .find(|entry| entry.display_name.eq_ignore_ascii_case("inside.txt"));
            if inside.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        let inside = inside.expect("copied ZIP child after bounded Shell convergence");
        let copy_out = FileOperationRequest {
            kind: FileOperationKind::Copy {
                items: vec![ItemDescriptor {
                    id: inside.id,
                    location: inside.location,
                }],
                destination: LocationDescriptor::file_system(&output),
            },
            flags: FileOperationFlags {
                conflict: ConflictDecision::Replace,
                ..FileOperationFlags::default()
            },
        };
        let context = explorer_model::RequestContext::new(
            explorer_model::TabId::new(),
            explorer_model::Generation::default(),
        );
        assert!(matches!(
            execute(&context, &copy_out, &events).expect("copy out of ZIP"),
            explorer_model::OperationTerminal::Finished
                | explorer_model::OperationTerminal::Partial { .. }
        ));
        assert_eq!(
            std::fs::read(output.join("inside.txt")).expect("copied-out content"),
            b"Windows owns ZIP namespace parsing"
        );
    }
}
