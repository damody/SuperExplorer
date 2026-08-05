//! Package-attested bundled-tool capabilities. A plugin never supplies an executable path.
use abi_stable::{
    StableAbi, sabi_trait,
    std_types::{RArc, RString, RVec},
};

pub const MAX_TOOL_ARGUMENTS_V1: usize = 128;
pub const MAX_TOOL_ARGUMENT_BYTES_V1: usize = 4096;
pub const MAX_TOOL_OUTPUT_BYTES_V1: usize = 8 * 1024 * 1024;

#[repr(C)]
#[derive(Clone, Debug, Eq, PartialEq, StableAbi)]
pub struct ToolExecuteRequestV1 {
    pub arguments: RVec<RString>,
    pub timeout_millis: u32,
    pub max_output_bytes: u32,
}

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct ToolExecuteStatusV1(u32);
impl ToolExecuteStatusV1 {
    pub const COMPLETED: Self = Self(1);
    pub const CANCELLED: Self = Self(2);
    pub const TIMED_OUT: Self = Self(3);
    pub const REJECTED: Self = Self(4);
    pub const FAILED: Self = Self(5);
    pub const OUTPUT_TRUNCATED: Self = Self(6);
    #[must_use]
    pub const fn into_raw(self) -> u32 {
        self.0
    }
}

#[repr(C)]
#[derive(Clone, Debug, Eq, PartialEq, StableAbi)]
pub struct ToolExecuteOutcomeV1 {
    pub status: ToolExecuteStatusV1,
    pub exit_code: i32,
    pub stdout: RVec<u8>,
    pub stderr: RVec<u8>,
}

#[sabi_trait]
pub trait AbiToolExecutorV1: Send + Sync + Clone {
    #[sabi(last_prefix_field)]
    fn execute(&self, request: ToolExecuteRequestV1) -> ToolExecuteOutcomeV1;
}

#[repr(transparent)]
#[derive(Clone, StableAbi)]
pub struct ToolHandleV1(AbiToolExecutorV1_TO<'static, RArc<()>>);
impl ToolHandleV1 {
    #[doc(hidden)]
    pub fn from_host<T: AbiToolExecutorV1 + 'static>(executor: T) -> Self {
        Self(AbiToolExecutorV1_TO::from_ptr(
            RArc::new(executor),
            abi_stable::sabi_trait::TD_Opaque,
        ))
    }
    #[must_use]
    pub fn execute(&self, request: ToolExecuteRequestV1) -> ToolExecuteOutcomeV1 {
        self.0.execute(request)
    }
}
