//! Pure operation-center state machine and conservative undo journal.

use std::collections::HashMap;

use explorer_common::{ExplorerError, RequestId};

use crate::{
    ConflictDecision, FileOperationFlags, FileOperationKind, FileOperationRequest, ItemDescriptor,
    LocationDescriptor, OperationProgress, OperationTerminal,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowsFileNameError {
    Empty,
    DotSegment,
    TrailingSpaceOrDot,
    ReservedDevice,
    InvalidCharacter,
}

/// Validates one leaf name using Win32 reserved-character, suffix, and device-name rules.
///
/// # Errors
///
/// Returns the exact validation category for invalid Windows leaf names.
pub fn validate_windows_file_name(name: &str) -> Result<(), WindowsFileNameError> {
    if name.is_empty() {
        return Err(WindowsFileNameError::Empty);
    }
    if matches!(name, "." | "..") {
        return Err(WindowsFileNameError::DotSegment);
    }
    if name.ends_with([' ', '.']) {
        return Err(WindowsFileNameError::TrailingSpaceOrDot);
    }
    let stem = name.split('.').next().unwrap_or_default();
    if matches!(
        stem.to_ascii_uppercase().as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    ) {
        return Err(WindowsFileNameError::ReservedDevice);
    }
    if name.chars().any(|value| {
        value <= '\u{1f}' || matches!(value, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
    }) {
        return Err(WindowsFileNameError::InvalidCharacter);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenameCommitTrigger {
    Enter,
    Blur,
}

#[derive(Clone, Debug)]
pub struct RenameEditorState {
    pub item: ItemDescriptor,
    pub original_name: String,
    pub buffer: String,
    pub selection: std::ops::Range<usize>,
    pub error: Option<ExplorerError>,
}

impl RenameEditorState {
    pub fn begin(item: ItemDescriptor, original_name: String, is_container: bool) -> Self {
        let selection_end = if is_container {
            original_name.len()
        } else {
            original_name.rfind('.').unwrap_or(original_name.len())
        };
        Self {
            item,
            buffer: original_name.clone(),
            original_name,
            selection: 0..selection_end,
            error: None,
        }
    }

    pub fn update(&mut self, value: String) {
        self.buffer = value;
        self.error = None;
    }

    /// Produces a typed rename request while retaining editor state on validation failure.
    ///
    /// # Errors
    ///
    /// Returns a user-safe input or collision error. The buffer and selection remain available.
    pub fn commit(
        &mut self,
        _trigger: RenameCommitTrigger,
        destination_collision: bool,
    ) -> Result<Option<FileOperationRequest>, ExplorerError> {
        if self.buffer == self.original_name {
            return Ok(None);
        }
        if let Err(reason) = validate_windows_file_name(&self.buffer) {
            let error = ExplorerError::new(
                explorer_common::ExplorerErrorKind::Input,
                "validate inline rename",
                true,
                "檔案名稱無效，請修正後再試一次。",
                format!("Windows file-name validation failed: {reason:?}"),
            );
            self.error = Some(error.clone());
            return Err(error);
        }
        if destination_collision {
            let error = ExplorerError::new(
                explorer_common::ExplorerErrorKind::Conflict,
                "validate inline rename collision",
                true,
                "此位置已有同名項目。",
                "rename destination collision detected before Shell submission",
            );
            self.error = Some(error.clone());
            return Err(error);
        }
        Ok(Some(FileOperationRequest {
            kind: FileOperationKind::Rename {
                item: self.item.clone(),
                new_name: self.buffer.clone(),
            },
            flags: FileOperationFlags {
                conflict: ConflictDecision::Prompt,
                ..FileOperationFlags::default()
            },
        }))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationPhase {
    Queued,
    Running,
    Finished,
    Cancelled,
    Partial,
    Failed,
}

impl OperationPhase {
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Finished | Self::Cancelled | Self::Partial | Self::Failed
        )
    }
}

#[derive(Clone, Debug)]
pub struct OperationRecord {
    pub id: RequestId,
    pub request: FileOperationRequest,
    pub phase: OperationPhase,
    pub progress: OperationProgress,
    pub terminal: Option<OperationTerminal>,
}

impl OperationRecord {
    pub fn queued(id: RequestId, request: FileOperationRequest, total_items: usize) -> Self {
        Self {
            id,
            request,
            phase: OperationPhase::Queued,
            progress: OperationProgress {
                completed_items: 0,
                total_items,
                completed_bytes: 0,
                total_bytes: None,
                phase: crate::TransferProgressPhase::Preparing,
                current_item: None,
            },
            terminal: None,
        }
    }

    /// Starts a queued operation.
    ///
    /// # Errors
    ///
    /// Rejects repeated starts and any transition out of a terminal phase.
    pub fn start(&mut self) -> Result<(), OperationStateError> {
        if self.phase != OperationPhase::Queued {
            return Err(OperationStateError::InvalidTransition {
                from: self.phase,
                to: OperationPhase::Running,
            });
        }
        self.phase = OperationPhase::Running;
        Ok(())
    }

    /// Coalesces monotonic progress while running.
    ///
    /// # Errors
    ///
    /// Rejects late, regressing, or out-of-range progress.
    pub fn update_progress(
        &mut self,
        progress: OperationProgress,
    ) -> Result<(), OperationStateError> {
        if self.phase != OperationPhase::Running {
            return Err(OperationStateError::LateProgress);
        }
        if progress.completed_items < self.progress.completed_items
            || progress.completed_items > progress.total_items
            || progress.total_items != self.progress.total_items
            || progress.completed_bytes < self.progress.completed_bytes
            || matches!(
                (self.progress.phase, progress.phase),
                (
                    crate::TransferProgressPhase::Transferring,
                    crate::TransferProgressPhase::Preparing
                ) | (
                    crate::TransferProgressPhase::Finalizing,
                    crate::TransferProgressPhase::Preparing
                ) | (
                    crate::TransferProgressPhase::Finalizing,
                    crate::TransferProgressPhase::Transferring
                )
            )
        {
            return Err(OperationStateError::RegressingProgress);
        }
        self.progress = progress;
        Ok(())
    }

    /// Records the sole terminal result.
    ///
    /// # Errors
    ///
    /// Rejects completion before running and duplicate terminal results.
    pub fn finish(&mut self, terminal: OperationTerminal) -> Result<(), OperationStateError> {
        if self.terminal.is_some() {
            return Err(OperationStateError::DuplicateTerminal);
        }
        if self.phase != OperationPhase::Running {
            return Err(OperationStateError::InvalidTransition {
                from: self.phase,
                to: phase_for_terminal(&terminal),
            });
        }
        self.phase = phase_for_terminal(&terminal);
        self.terminal = Some(terminal);
        Ok(())
    }
}

fn phase_for_terminal(terminal: &OperationTerminal) -> OperationPhase {
    match terminal {
        OperationTerminal::Finished => OperationPhase::Finished,
        OperationTerminal::Cancelled => OperationPhase::Cancelled,
        OperationTerminal::Partial { .. } => OperationPhase::Partial,
        OperationTerminal::Failed(_) => OperationPhase::Failed,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationStateError {
    InvalidTransition {
        from: OperationPhase,
        to: OperationPhase,
    },
    LateProgress,
    RegressingProgress,
    DuplicateTerminal,
}

#[derive(Clone, Debug, Default)]
pub struct OperationCenterState {
    records: HashMap<RequestId, OperationRecord>,
    latest: Option<RequestId>,
}

impl OperationCenterState {
    pub fn insert(&mut self, record: OperationRecord) -> bool {
        self.latest = Some(record.id);
        self.records.insert(record.id, record).is_none()
    }

    pub fn get(&self, id: RequestId) -> Option<&OperationRecord> {
        self.records.get(&id)
    }

    pub fn get_mut(&mut self, id: RequestId) -> Option<&mut OperationRecord> {
        self.records.get_mut(&id)
    }

    pub fn records(&self) -> impl Iterator<Item = &OperationRecord> {
        self.records.values()
    }

    pub fn latest(&self) -> Option<&OperationRecord> {
        self.latest.and_then(|id| self.records.get(&id))
    }

    /// Applies one correlated operation event without allowing late progress or duplicate terminal
    /// state to mutate the record.
    pub fn apply_event(&mut self, event: &crate::ExplorerEvent) -> bool {
        match event {
            crate::ExplorerEvent::OperationProgress { context, progress } => {
                let Some(record) = self.records.get_mut(&context.request_id) else {
                    return false;
                };
                if record.phase == OperationPhase::Queued && record.start().is_err() {
                    return false;
                }
                record.update_progress(progress.clone()).is_ok()
            }
            crate::ExplorerEvent::OperationFinished { context, outcome } => {
                let Some(record) = self.records.get_mut(&context.request_id) else {
                    return false;
                };
                if record.phase == OperationPhase::Queued && record.start().is_err() {
                    return false;
                }
                record.finish(outcome.clone()).is_ok()
            }
            _ => false,
        }
    }
}

/// Reversible operation description captured only after successful native completion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JournalInverse {
    Rename {
        item: ItemDescriptor,
        prior_name: String,
    },
    Move {
        items: Vec<ItemDescriptor>,
        prior_parent: LocationDescriptor,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JournalEntry {
    pub operation_id: RequestId,
    pub inverse: JournalInverse,
    pub forward: FileOperationRequest,
}

impl JournalEntry {
    pub fn inverse_request(&self) -> FileOperationRequest {
        let kind = match &self.inverse {
            JournalInverse::Rename { item, prior_name } => FileOperationKind::Rename {
                item: item.clone(),
                new_name: prior_name.clone(),
            },
            JournalInverse::Move {
                items,
                prior_parent,
            } => FileOperationKind::Move {
                items: items.clone(),
                destination: prior_parent.clone(),
            },
        };
        FileOperationRequest {
            kind,
            flags: FileOperationFlags {
                conflict: ConflictDecision::Prompt,
                ..FileOperationFlags::default()
            },
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JournalPreimage {
    Rename { prior_name: String },
    Move { prior_parent: LocationDescriptor },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JournalValidation {
    Valid,
    IdentityMismatch,
    SourceMissing,
    DestinationUnavailable,
    NameCollision,
}

impl JournalValidation {
    fn validate(self, operation: &'static str) -> Result<(), ExplorerError> {
        if self == Self::Valid {
            return Ok(());
        }
        Err(ExplorerError::new(
            explorer_common::ExplorerErrorKind::Conflict,
            operation,
            true,
            "檔案已在外部變更，無法安全地完成此動作。",
            format!("journal validation failed: {self:?}"),
        ))
    }
}

#[derive(Clone, Debug, Default)]
pub struct OperationJournal {
    undo: Vec<JournalEntry>,
    redo: Vec<JournalEntry>,
}

impl OperationJournal {
    pub fn record_completed(&mut self, entry: JournalEntry) {
        self.undo.push(entry);
        self.redo.clear();
    }

    pub fn record_completed_request(
        &mut self,
        operation_id: RequestId,
        request: &FileOperationRequest,
        terminal: &OperationTerminal,
        preimage: JournalPreimage,
    ) -> bool {
        if !matches!(terminal, OperationTerminal::Finished) {
            return false;
        }
        let inverse = match (&request.kind, preimage) {
            (
                FileOperationKind::Rename { item, new_name },
                JournalPreimage::Rename { prior_name },
            ) => {
                let mut renamed_item = item.clone();
                if let Some(source) = item.location.path()
                    && let Some(parent) = source.parent()
                {
                    renamed_item.location = LocationDescriptor::file_system(parent.join(new_name));
                }
                JournalInverse::Rename {
                    item: renamed_item,
                    prior_name,
                }
            }
            (
                FileOperationKind::Move { items, destination },
                JournalPreimage::Move { prior_parent },
            ) => {
                let moved_items = items
                    .iter()
                    .cloned()
                    .map(|mut item| {
                        if let (Some(source), Some(destination)) =
                            (item.location.path(), destination.path())
                            && let Some(name) = source.file_name()
                        {
                            item.location = LocationDescriptor::file_system(destination.join(name));
                        }
                        item
                    })
                    .collect();
                JournalInverse::Move {
                    items: moved_items,
                    prior_parent,
                }
            }
            _ => return false,
        };
        self.record_completed(JournalEntry {
            operation_id,
            inverse,
            forward: request.clone(),
        });
        true
    }

    pub fn undo_candidate(&self) -> Option<&JournalEntry> {
        self.undo.last()
    }

    pub fn redo_candidate(&self) -> Option<&JournalEntry> {
        self.redo.last()
    }

    /// Moves one prevalidated journal entry to redo.
    ///
    /// # Errors
    ///
    /// Refuses undo when external identity/name/destination validation failed.
    pub fn commit_undo(
        &mut self,
        preconditions_valid: bool,
    ) -> Result<JournalEntry, ExplorerError> {
        if !preconditions_valid {
            return Err(ExplorerError::new(
                explorer_common::ExplorerErrorKind::Conflict,
                "undo file operation",
                true,
                "檔案已由其他程式變更，無法復原。",
                "journal precondition validation failed",
            ));
        }
        let entry = self.undo.pop().ok_or_else(|| {
            ExplorerError::new(
                explorer_common::ExplorerErrorKind::Availability,
                "undo file operation",
                false,
                "沒有可復原的操作。",
                "undo journal is empty",
            )
        })?;
        self.redo.push(entry.clone());
        Ok(entry)
    }

    /// Moves one externally revalidated undo entry to redo.
    ///
    /// # Errors
    ///
    /// Refuses the transition when identity, source, destination, or name validation fails.
    pub fn commit_undo_validated(
        &mut self,
        validation: JournalValidation,
    ) -> Result<JournalEntry, ExplorerError> {
        validation.validate("undo file operation")?;
        self.commit_undo(true)
    }

    /// Moves one externally revalidated redo entry back to undo.
    ///
    /// # Errors
    ///
    /// Refuses the transition when identity, source, destination, or name validation fails.
    pub fn commit_redo_validated(
        &mut self,
        validation: JournalValidation,
    ) -> Result<JournalEntry, ExplorerError> {
        validation.validate("redo file operation")?;
        let entry = self.redo.pop().ok_or_else(|| {
            ExplorerError::new(
                explorer_common::ExplorerErrorKind::Availability,
                "redo file operation",
                false,
                "沒有可重做的檔案操作。",
                "redo journal is empty",
            )
        })?;
        self.undo.push(entry.clone());
        Ok(entry)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        JournalPreimage, JournalValidation, OperationJournal, OperationPhase, OperationRecord,
        OperationStateError, RenameCommitTrigger, RenameEditorState,
    };
    use crate::{
        ConflictDecision, FileOperationFlags, FileOperationKind, FileOperationRequest,
        LocationDescriptor, OperationProgress, OperationTerminal,
    };

    fn request() -> FileOperationRequest {
        FileOperationRequest {
            kind: FileOperationKind::CreateFolder {
                parent: LocationDescriptor::file_system(r"C:\fixture"),
                name: "new".to_owned(),
            },
            flags: FileOperationFlags {
                conflict: ConflictDecision::Prompt,
                ..FileOperationFlags::default()
            },
        }
    }

    #[test]
    fn operation_state_machine_has_exactly_one_terminal() {
        for terminal in [
            OperationTerminal::Finished,
            OperationTerminal::Cancelled,
            OperationTerminal::Partial { outcomes: vec![] },
            OperationTerminal::Failed(explorer_common::ExplorerError::new(
                explorer_common::ExplorerErrorKind::Availability,
                "test",
                true,
                "failed",
                "injected",
            )),
        ] {
            let mut record =
                OperationRecord::queued(explorer_common::RequestId::new(), request(), 2);
            record.start().expect("start");
            record
                .update_progress(OperationProgress {
                    completed_items: 1,
                    total_items: 2,
                    completed_bytes: 4,
                    total_bytes: Some(8),
                    phase: crate::TransferProgressPhase::Transferring,
                    current_item: None,
                })
                .expect("progress");
            record.finish(terminal).expect("terminal");
            assert!(record.phase.is_terminal());
            assert_eq!(
                record.finish(OperationTerminal::Finished),
                Err(OperationStateError::DuplicateTerminal)
            );
            assert_eq!(
                record.update_progress(OperationProgress {
                    completed_items: 2,
                    total_items: 2,
                    completed_bytes: 8,
                    total_bytes: Some(8),
                    phase: crate::TransferProgressPhase::Transferring,
                    current_item: None,
                }),
                Err(OperationStateError::LateProgress)
            );
        }
    }

    #[test]
    fn progress_cannot_regress_or_exceed_total() {
        let mut record = OperationRecord::queued(explorer_common::RequestId::new(), request(), 1);
        assert_eq!(record.phase, OperationPhase::Queued);
        record.start().unwrap();
        assert_eq!(
            record.update_progress(OperationProgress {
                completed_items: 2,
                total_items: 1,
                completed_bytes: 0,
                total_bytes: None,
                phase: crate::TransferProgressPhase::Transferring,
                current_item: None,
            }),
            Err(OperationStateError::RegressingProgress)
        );
    }

    #[test]
    fn rename_editor_preserves_buffer_error_and_selects_stem() {
        let item = crate::ItemDescriptor {
            id: crate::ShellItemId::from_provider_bytes([9]).unwrap(),
            location: LocationDescriptor::file_system(r"C:\fixture\報告.final.txt"),
        };
        let mut editor = RenameEditorState::begin(item, "報告.final.txt".to_owned(), false);
        assert_eq!(
            &editor.original_name[editor.selection.clone()],
            "報告.final"
        );
        editor.update("bad?.txt".to_owned());
        assert!(editor.commit(RenameCommitTrigger::Enter, false).is_err());
        assert_eq!(editor.buffer, "bad?.txt");
        assert!(editor.error.is_some());
        editor.update("new.txt".to_owned());
        assert!(editor.commit(RenameCommitTrigger::Blur, true).is_err());
        assert_eq!(editor.buffer, "new.txt");
        editor.update("renamed.txt".to_owned());
        let request = editor
            .commit(RenameCommitTrigger::Enter, false)
            .expect("valid rename")
            .expect("changed name");
        assert!(matches!(request.kind, FileOperationKind::Rename { .. }));
    }

    #[test]
    fn rename_editor_unchanged_enter_or_blur_is_a_cancelled_noop() {
        let item = crate::ItemDescriptor {
            id: crate::ShellItemId::from_provider_bytes([10]).unwrap(),
            location: LocationDescriptor::file_system(r"C:\fixture\folder"),
        };
        let mut editor = RenameEditorState::begin(item, "folder".to_owned(), true);
        assert_eq!(editor.selection, 0.."folder".len());
        assert!(
            editor
                .commit(RenameCommitTrigger::Blur, false)
                .expect("unchanged blur")
                .is_none()
        );
    }

    #[test]
    fn journal_records_only_finished_safe_kinds_and_revalidates_undo_redo() {
        let mut journal = OperationJournal::default();
        let request = request();
        assert!(!journal.record_completed_request(
            explorer_common::RequestId::new(),
            &request,
            &OperationTerminal::Finished,
            JournalPreimage::Rename {
                prior_name: "old".to_owned(),
            },
        ));

        let item = crate::ItemDescriptor {
            id: crate::ShellItemId::from_provider_bytes([11]).unwrap(),
            location: LocationDescriptor::file_system(r"C:\fixture\new.txt"),
        };
        let rename = FileOperationRequest {
            kind: FileOperationKind::Rename {
                item,
                new_name: "new.txt".to_owned(),
            },
            flags: FileOperationFlags::default(),
        };
        assert!(!journal.record_completed_request(
            explorer_common::RequestId::new(),
            &rename,
            &OperationTerminal::Cancelled,
            JournalPreimage::Rename {
                prior_name: "old.txt".to_owned(),
            },
        ));
        assert!(journal.record_completed_request(
            explorer_common::RequestId::new(),
            &rename,
            &OperationTerminal::Finished,
            JournalPreimage::Rename {
                prior_name: "old.txt".to_owned(),
            },
        ));
        let invalid = JournalValidation::IdentityMismatch;
        assert!(journal.commit_undo_validated(invalid).is_err());
        assert!(journal.undo_candidate().is_some());
        journal
            .commit_undo_validated(JournalValidation::Valid)
            .expect("validated undo");
        assert!(journal.redo_candidate().is_some());
        journal
            .commit_redo_validated(JournalValidation::Valid)
            .expect("validated redo");
        assert!(journal.undo_candidate().is_some());
    }
}
