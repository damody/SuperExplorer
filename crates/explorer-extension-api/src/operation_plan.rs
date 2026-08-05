//! Data-only command, form, preview, and operation-plan ABI values.

use abi_stable::{
    StableAbi,
    std_types::{ROption, RString, RVec},
};

pub const MAX_OPERATION_STEPS_V1: usize = 100_000;
pub const MAX_FORM_FIELDS_V1: usize = 32;

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct CommandPlacementV1(u32);
impl CommandPlacementV1 {
    pub const TOOLBAR: Self = Self(1);
    pub const CONTEXT_MENU: Self = Self(2);
    pub const EXTENSIONS_MENU: Self = Self(3);
}

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct SelectionRequirementV1(u32);
impl SelectionRequirementV1 {
    pub const NONE: Self = Self(0);
    pub const ONE: Self = Self(1);
    pub const ONE_OR_MORE: Self = Self(2);
}

#[repr(C)]
#[derive(Clone, Debug, Eq, PartialEq, StableAbi)]
pub struct CommandDescriptorV1 {
    pub id: RString,
    pub label: RString,
    pub placement: CommandPlacementV1,
    pub selection: SelectionRequirementV1,
    /// Normalized host shortcut such as `Ctrl+Shift+R`; absent means none.
    pub shortcut: ROption<RString>,
}

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct FormFieldKindV1(u32);
impl FormFieldKindV1 {
    pub const TEXT: Self = Self(1);
    pub const INTEGER: Self = Self(2);
    pub const CHOICE: Self = Self(3);
    pub const AUTHORIZED_LOCATION: Self = Self(4);
    pub const TEMPLATE: Self = Self(5);
    pub const BOOLEAN: Self = Self(6);
}

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct OperationKindV1(u32);
impl OperationKindV1 {
    pub const CREATE_DIRECTORY: Self = Self(1);
    pub const RENAME: Self = Self(2);
    pub const COPY: Self = Self(3);
    pub const MOVE: Self = Self(4);
    pub const DELETE: Self = Self(5);
    pub const EXTRACT: Self = Self(6);
    pub const ARCHIVE_MUTATION: Self = Self(7);
    #[must_use]
    pub const fn into_raw(self) -> u32 {
        self.0
    }
}

/// Host-minted, generation-scoped authorization for an operation root, source
/// item, or destination directory. Its random token has no path semantics.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, StableAbi)]
pub struct OperationObjectHandleV1 {
    pub token: [u8; 16],
    pub generation: u64,
}

impl OperationObjectHandleV1 {
    #[must_use]
    pub const fn new(token: [u8; 16], generation: u64) -> Self {
        Self { token, generation }
    }

    #[must_use]
    pub fn is_valid(self) -> bool {
        self.generation != 0 && self.token != [0; 16]
    }
}

#[repr(C)]
#[derive(Clone, Debug, Eq, PartialEq, StableAbi)]
pub struct FileIdentityV1 {
    pub volume_serial: u64,
    pub file_id_low: u64,
    pub file_id_high: u64,
    pub length: u64,
    pub modified_ticks: i64,
}

#[repr(C)]
#[derive(Clone, Debug, Eq, PartialEq, StableAbi)]
pub struct OperationStepV1 {
    pub kind: OperationKindV1,
    pub source: ROption<OperationObjectHandleV1>,
    pub destination_parent: ROption<OperationObjectHandleV1>,
    /// A single Windows basename, never a path.
    pub destination_name: ROption<RString>,
    pub expected_source: ROption<FileIdentityV1>,
}

#[repr(C)]
#[derive(Clone, Debug, Eq, PartialEq, StableAbi)]
pub struct OperationPlanV1 {
    pub title: RString,
    pub root: OperationObjectHandleV1,
    pub steps: RVec<OperationStepV1>,
    pub confirmation_threshold: u32,
    pub undo_requested: bool,
}

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct OperationPermissionV1(u32);
impl OperationPermissionV1 {
    pub const ALLOWED: Self = Self(1);
    pub const DENIED: Self = Self(2);
    pub const UNKNOWN: Self = Self(3);
}

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct OperationConflictV1(u32);
impl OperationConflictV1 {
    pub const NONE: Self = Self(1);
    pub const TARGET_EXISTS: Self = Self(2);
    pub const SOURCE_CHANGED: Self = Self(3);
    pub const CASE_COLLISION: Self = Self(4);
}

#[repr(C)]
#[derive(Clone, Debug, Eq, PartialEq, StableAbi)]
pub struct OperationPreviewStepV1 {
    pub kind: OperationKindV1,
    pub source_display_name: ROption<RString>,
    pub destination_display_name: ROption<RString>,
    pub permission: OperationPermissionV1,
    pub conflict: OperationConflictV1,
    pub estimated_items: u64,
    pub estimated_bytes: u64,
    pub reversible: bool,
    pub warning: ROption<RString>,
    pub irreversible_reason: ROption<RString>,
}

#[repr(C)]
#[derive(Clone, Debug, Eq, PartialEq, StableAbi)]
pub struct FormFieldV1 {
    pub id: RString,
    pub label: RString,
    pub value: RString,
    pub required: bool,
    pub kind: FormFieldKindV1,
    pub choices: RVec<RString>,
    pub minimum: ROption<i64>,
    pub maximum: ROption<i64>,
}

#[repr(C)]
#[derive(Clone, Debug, Eq, PartialEq, StableAbi)]
pub struct CommandFormV1 {
    pub title: RString,
    pub fields: RVec<FormFieldV1>,
}

/// A validated form value. `kind` determines whether `integer` or `text` is
/// populated; the other member must be absent.
#[repr(C)]
#[derive(Clone, Debug, Eq, PartialEq, StableAbi)]
pub struct FormValueV1 {
    pub kind: FormFieldKindV1,
    pub text: ROption<RString>,
    pub integer: ROption<i64>,
}

#[repr(C)]
#[derive(Clone, Debug, Eq, PartialEq, StableAbi)]
pub struct FormSubmissionEntryV1 {
    pub field_id: RString,
    pub value: FormValueV1,
}

#[repr(C)]
#[derive(Clone, Debug, Eq, PartialEq, StableAbi)]
pub struct FormSubmissionV1 {
    pub command_id: RString,
    pub values: RVec<FormSubmissionEntryV1>,
}

#[repr(C)]
#[derive(Clone, Debug, Eq, PartialEq, StableAbi)]
pub struct LocalizedFormErrorV1 {
    pub field_id: ROption<RString>,
    pub message_key: RString,
    pub fallback: RString,
}

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct OperationTerminalV1(u32);
impl OperationTerminalV1 {
    pub const COMPLETED: Self = Self(1);
    pub const CANCELLED: Self = Self(2);
    pub const PARTIAL: Self = Self(3);
    pub const CONFLICT: Self = Self(4);
    pub const REJECTED: Self = Self(5);
    #[must_use]
    pub const fn into_raw(self) -> u32 {
        self.0
    }
}

#[repr(C)]
#[derive(Clone, Debug, Eq, PartialEq, StableAbi)]
pub struct OperationPreviewV1 {
    pub terminal_if_committed: OperationTerminalV1,
    pub step_count: u32,
    pub requires_confirmation: bool,
    pub summary: RString,
    pub estimated_items: u64,
    pub estimated_bytes: u64,
    pub warnings: RVec<RString>,
    pub irreversible_reasons: RVec<RString>,
    pub steps: RVec<OperationPreviewStepV1>,
}

#[repr(C)]
#[derive(Clone, Debug, Eq, PartialEq, StableAbi)]
pub struct OperationOutcomeV1 {
    pub terminal: OperationTerminalV1,
    pub attempted_steps: u32,
    pub completed_steps: u32,
    pub failed_steps: u32,
    pub unattempted_steps: u32,
    pub reverted_steps: u32,
    pub not_reverted_steps: u32,
    pub failed_step: ROption<u32>,
    pub undo_token: ROption<RString>,
    pub journal_id: ROption<RString>,
    pub detail: RString,
}

#[repr(C)]
#[derive(Clone, Debug, Eq, PartialEq, StableAbi)]
pub struct OperationProgressV1 {
    pub completed_steps: u32,
    pub failed_steps: u32,
    pub unattempted_steps: u32,
    pub current_step: ROption<u32>,
}
