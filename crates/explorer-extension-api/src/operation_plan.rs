//! Data-only command, form, preview, and operation-plan ABI values.

use abi_stable::{
    StableAbi,
    std_types::{ROption, RString, RVec},
};

pub const MAX_OPERATION_STEPS_V1: usize = 100_000;
pub const MAX_FORM_FIELDS_V1: usize = 32;

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct OperationKindV1(u32);
impl OperationKindV1 {
    pub const CREATE_DIRECTORY: Self = Self(1);
    pub const RENAME: Self = Self(2);
    pub const REPLACE_CONTAINER: Self = Self(3);
    #[must_use]
    pub const fn into_raw(self) -> u32 {
        self.0
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
    /// Host-authorized path relative to the plan root. Absolute and parent paths are rejected.
    pub source: ROption<RString>,
    pub destination: RString,
    pub expected_source: ROption<FileIdentityV1>,
}

#[repr(C)]
#[derive(Clone, Debug, Eq, PartialEq, StableAbi)]
pub struct OperationPlanV1 {
    pub title: RString,
    pub steps: RVec<OperationStepV1>,
    pub confirmation_threshold: u32,
    pub undo_requested: bool,
}

#[repr(C)]
#[derive(Clone, Debug, Eq, PartialEq, StableAbi)]
pub struct FormFieldV1 {
    pub id: RString,
    pub label: RString,
    pub value: RString,
    pub required: bool,
}

#[repr(C)]
#[derive(Clone, Debug, Eq, PartialEq, StableAbi)]
pub struct CommandFormV1 {
    pub title: RString,
    pub fields: RVec<FormFieldV1>,
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
}

#[repr(C)]
#[derive(Clone, Debug, Eq, PartialEq, StableAbi)]
pub struct OperationOutcomeV1 {
    pub terminal: OperationTerminalV1,
    pub completed_steps: u32,
    pub failed_step: ROption<u32>,
    pub undo_token: ROption<RString>,
    pub detail: RString,
}
