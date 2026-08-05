use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::atomic::{AtomicU8, Ordering},
};

use abi_stable::{
    StableAbi, sabi_trait,
    std_types::{RArc, RBox, ROption, RString, RVec},
};

use crate::{InputStreamV1, StableIdV1, dispose_caught_panic_payload_v1};

pub const MAX_VIRTUAL_ENTRIES_V1: usize = 16_384;
pub const MAX_VIRTUAL_COMPONENTS_V1: usize = 256;
pub const MAX_VIRTUAL_READ_BYTES_V1: usize = 64 * 1024;
pub const MAX_VIRTUAL_WRITE_BYTES_V1: usize = 64 * 1024;
pub const MAX_VIRTUAL_MUTATION_STEPS_V1: usize = 1_024;
pub const MAX_VIRTUAL_SECRET_UTF16_V1: usize = 1_024;

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct VirtualSecretStatusV1(u32);

impl VirtualSecretStatusV1 {
    pub const READY: Self = Self(1);
    pub const CONSUMED: Self = Self(2);
    pub const CANCELLED: Self = Self(3);
    pub const INVALID: Self = Self(4);
}

#[repr(C)]
#[derive(Clone, StableAbi)]
pub struct VirtualSecretMaterialV1 {
    pub status: VirtualSecretStatusV1,
    pub reserved: u32,
    pub utf16: RVec<u16>,
}

#[sabi_trait]
#[doc(hidden)]
pub trait AbiVirtualSecretServicesV1: Send + Sync + Clone {
    #[sabi(last_prefix_field)]
    fn take(&self) -> VirtualSecretMaterialV1;
}

#[repr(C)]
#[derive(Clone, StableAbi)]
pub struct VirtualSecretV1 {
    services: AbiVirtualSecretServicesV1_TO<'static, RArc<()>>,
}

impl VirtualSecretV1 {
    #[doc(hidden)]
    pub fn from_host<T: AbiVirtualSecretServicesV1 + 'static>(services: T) -> Self {
        Self {
            services: AbiVirtualSecretServicesV1_TO::from_ptr(
                RArc::new(services),
                sabi_trait::TD_Opaque,
            ),
        }
    }

    #[must_use]
    pub fn take(&self) -> VirtualSecretMaterialV1 {
        let material = self.services.take();
        if material.utf16.len() > MAX_VIRTUAL_SECRET_UTF16_V1 {
            VirtualSecretMaterialV1 {
                status: VirtualSecretStatusV1::INVALID,
                reserved: 0,
                utf16: RVec::new(),
            }
        } else {
            material
        }
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct VirtualProviderStatusV1(u32);

impl VirtualProviderStatusV1 {
    pub const READY: Self = Self(1);
    pub const UNSUPPORTED: Self = Self(2);
    pub const INVALID: Self = Self(3);
    pub const STALE: Self = Self(4);
    pub const CANCELLED: Self = Self(5);
    pub const RESOURCE_LIMITED: Self = Self(6);
    pub const INTEGRITY_FAILED: Self = Self(7);
    pub const PASSWORD_REQUIRED: Self = Self(8);
    pub const FAILED: Self = Self(9);
    pub const PANICKED: Self = Self(10);

    #[must_use]
    pub const fn into_raw(self) -> u32 {
        self.0
    }

    #[must_use]
    pub const fn is_known(self) -> bool {
        self.0 >= 1 && self.0 <= 10
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct VirtualEntryKindV1(u32);

impl VirtualEntryKindV1 {
    pub const FILE: Self = Self(1);
    pub const DIRECTORY: Self = Self(2);
}

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct VirtualAllowedOperationsV1(u32);

impl VirtualAllowedOperationsV1 {
    pub const NONE: Self = Self(0);
    pub const READ: Self = Self(1 << 0);
    pub const EXTRACT: Self = Self(1 << 1);
    pub const DELETE: Self = Self(1 << 2);
    pub const RENAME: Self = Self(1 << 3);
    pub const MOVE: Self = Self(1 << 4);

    #[must_use]
    pub const fn from_bits(bits: u32) -> Self {
        Self(bits)
    }

    #[must_use]
    pub const fn bits(self) -> u32 {
        self.0
    }
}

#[repr(C)]
#[derive(Clone, Debug, Eq, PartialEq, StableAbi)]
pub struct VirtualEntryV1 {
    pub id: StableIdV1,
    pub name: RString,
    pub components: RVec<RString>,
    pub kind: VirtualEntryKindV1,
    pub uncompressed_size: u64,
    pub compressed_size: u64,
    pub crc32: ROption<u32>,
    pub modified_unix_seconds: ROption<i64>,
    pub encrypted: bool,
    pub allowed_operations: VirtualAllowedOperationsV1,
}

#[repr(C)]
#[derive(Clone, StableAbi)]
pub struct VirtualEnumerateRequestV1 {
    pub container: InputStreamV1,
    pub container_generation: u64,
    pub source_generation: u64,
    pub parent_components: RVec<RString>,
    pub maximum_entries: u32,
    pub reserved: u32,
    pub secret: ROption<VirtualSecretV1>,
}

#[repr(C)]
#[derive(Clone, Debug, StableAbi)]
pub struct VirtualEnumerationOutcomeV1 {
    pub status: VirtualProviderStatusV1,
    pub reserved: u32,
    pub container_generation: u64,
    pub source_generation: u64,
    pub entries: RVec<VirtualEntryV1>,
}

impl VirtualEnumerationOutcomeV1 {
    #[must_use]
    pub fn terminal(status: VirtualProviderStatusV1, request: &VirtualEnumerateRequestV1) -> Self {
        Self {
            status,
            reserved: 0,
            container_generation: request.container_generation,
            source_generation: request.source_generation,
            entries: RVec::new(),
        }
    }
}

#[repr(C)]
#[derive(Clone, StableAbi)]
pub struct VirtualReadRequestV1 {
    pub container: InputStreamV1,
    pub container_generation: u64,
    pub source_generation: u64,
    pub entry_id: StableIdV1,
    pub offset: u64,
    pub maximum_bytes: u32,
    pub reserved: u32,
    pub secret: ROption<VirtualSecretV1>,
}

#[repr(C)]
#[derive(Clone, Debug, StableAbi)]
pub struct VirtualReadOutcomeV1 {
    pub status: VirtualProviderStatusV1,
    pub reserved: u32,
    pub container_generation: u64,
    pub source_generation: u64,
    pub next_offset: u64,
    pub end_of_entry: bool,
    pub bytes: RVec<u8>,
}

impl VirtualReadOutcomeV1 {
    #[must_use]
    pub fn terminal(status: VirtualProviderStatusV1, request: &VirtualReadRequestV1) -> Self {
        Self {
            status,
            reserved: 0,
            container_generation: request.container_generation,
            source_generation: request.source_generation,
            next_offset: request.offset,
            end_of_entry: true,
            bytes: RVec::new(),
        }
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct VirtualOutputStatusV1(u32);

impl VirtualOutputStatusV1 {
    pub const OK: Self = Self(1);
    pub const CANCELLED: Self = Self(2);
    pub const STALE: Self = Self(3);
    pub const RESOURCE_LIMITED: Self = Self(4);
    pub const CLOSED: Self = Self(5);
    pub const INVALID: Self = Self(6);
}

#[repr(C)]
#[derive(Clone, Debug, StableAbi)]
pub struct VirtualOutputOutcomeV1 {
    pub status: VirtualOutputStatusV1,
    pub reserved: u32,
    pub generation: u64,
    pub position: u64,
}

#[sabi_trait]
#[doc(hidden)]
pub trait AbiVirtualOutputServicesV1: Send + Sync + Clone {
    fn write(&self, bytes: RVec<u8>) -> VirtualOutputOutcomeV1;
    fn seek(&self, request: crate::InputStreamSeekRequestV1) -> VirtualOutputOutcomeV1;
    #[sabi(last_prefix_field)]
    fn flush(&self) -> VirtualOutputOutcomeV1;
}

#[repr(C)]
#[derive(Clone, StableAbi)]
pub struct VirtualOutputStreamV1 {
    generation: u64,
    services: AbiVirtualOutputServicesV1_TO<'static, RArc<()>>,
}

impl VirtualOutputStreamV1 {
    #[doc(hidden)]
    pub fn from_host<T: AbiVirtualOutputServicesV1 + 'static>(
        generation: u64,
        services: T,
    ) -> Self {
        Self {
            generation,
            services: AbiVirtualOutputServicesV1_TO::from_ptr(
                RArc::new(services),
                sabi_trait::TD_Opaque,
            ),
        }
    }

    #[must_use]
    pub fn write(&self, bytes: RVec<u8>) -> VirtualOutputOutcomeV1 {
        if bytes.is_empty() || bytes.len() > MAX_VIRTUAL_WRITE_BYTES_V1 {
            return VirtualOutputOutcomeV1 {
                status: VirtualOutputStatusV1::INVALID,
                reserved: 0,
                generation: self.generation,
                position: 0,
            };
        }
        self.services.write(bytes)
    }

    #[must_use]
    pub fn seek(&self, request: crate::InputStreamSeekRequestV1) -> VirtualOutputOutcomeV1 {
        self.services.seek(request)
    }

    #[must_use]
    pub fn flush(&self) -> VirtualOutputOutcomeV1 {
        self.services.flush()
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct VirtualMutationKindV1(u32);

impl VirtualMutationKindV1 {
    pub const DELETE: Self = Self(1);
    pub const RENAME: Self = Self(2);
    pub const MOVE: Self = Self(3);
    pub const ADD_FILE: Self = Self(4);
    pub const CREATE_DIRECTORY: Self = Self(5);
}

#[repr(C)]
#[derive(Clone, StableAbi)]
pub struct VirtualMutationStepV1 {
    pub kind: VirtualMutationKindV1,
    pub entry_id: StableIdV1,
    pub destination_components: RVec<RString>,
    pub source: ROption<InputStreamV1>,
    pub source_generation: u64,
}

#[repr(C)]
#[derive(Clone, StableAbi)]
pub struct VirtualMutationRequestV1 {
    pub container: InputStreamV1,
    pub staging: VirtualOutputStreamV1,
    pub container_generation: u64,
    pub source_generation: u64,
    pub steps: RVec<VirtualMutationStepV1>,
    pub reserved: u64,
    pub secret: ROption<VirtualSecretV1>,
}

#[repr(C)]
#[derive(Clone, Debug, StableAbi)]
pub struct VirtualMutationOutcomeV1 {
    pub status: VirtualProviderStatusV1,
    pub reserved: u32,
    pub container_generation: u64,
    pub source_generation: u64,
    pub written_entries: u32,
    pub reserved_tail: u32,
}

impl VirtualMutationOutcomeV1 {
    #[must_use]
    pub fn terminal(status: VirtualProviderStatusV1, request: &VirtualMutationRequestV1) -> Self {
        Self {
            status,
            reserved: 0,
            container_generation: request.container_generation,
            source_generation: request.source_generation,
            written_entries: 0,
            reserved_tail: 0,
        }
    }
}

#[sabi_trait]
#[doc(hidden)]
pub trait AbiVirtualFolderProviderV1: Send + Sync {
    fn enumerate(&self, request: VirtualEnumerateRequestV1) -> VirtualEnumerationOutcomeV1;
    fn read(&self, request: VirtualReadRequestV1) -> VirtualReadOutcomeV1;
    #[sabi(last_prefix_field)]
    fn mutate(&self, request: VirtualMutationRequestV1) -> VirtualMutationOutcomeV1;
}

pub trait VirtualFolderProviderImplementationV1: Send + Sync {
    fn enumerate(&self, request: VirtualEnumerateRequestV1) -> VirtualEnumerationOutcomeV1;
    fn read(&self, request: VirtualReadRequestV1) -> VirtualReadOutcomeV1;
    fn mutate(&self, request: VirtualMutationRequestV1) -> VirtualMutationOutcomeV1;
}

#[repr(transparent)]
#[derive(StableAbi)]
pub struct VirtualFolderProviderObjectV1(AbiVirtualFolderProviderV1_TO<'static, RBox<()>>);

const IDLE: u8 = 0;
const RUNNING: u8 = 1;
const FAULTED: u8 = 2;

struct Adapter<T> {
    provider: Option<T>,
    state: AtomicU8,
}

impl<T: VirtualFolderProviderImplementationV1> Adapter<T> {
    fn enter(&self) -> bool {
        self.state
            .compare_exchange(IDLE, RUNNING, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }
}

impl<T: VirtualFolderProviderImplementationV1> AbiVirtualFolderProviderV1 for Adapter<T> {
    fn enumerate(&self, request: VirtualEnumerateRequestV1) -> VirtualEnumerationOutcomeV1 {
        if request.parent_components.len() > MAX_VIRTUAL_COMPONENTS_V1
            || request.maximum_entries == 0
            || request.maximum_entries as usize > MAX_VIRTUAL_ENTRIES_V1
        {
            return VirtualEnumerationOutcomeV1::terminal(
                VirtualProviderStatusV1::INVALID,
                &request,
            );
        }
        if !self.enter() {
            return VirtualEnumerationOutcomeV1::terminal(
                VirtualProviderStatusV1::PANICKED,
                &request,
            );
        }
        let Some(provider) = self.provider.as_ref() else {
            self.state.store(FAULTED, Ordering::Release);
            return VirtualEnumerationOutcomeV1::terminal(
                VirtualProviderStatusV1::FAILED,
                &request,
            );
        };
        match catch_unwind(AssertUnwindSafe(|| provider.enumerate(request.clone()))) {
            Ok(outcome)
                if outcome.status.is_known()
                    && outcome.entries.len() <= request.maximum_entries as usize =>
            {
                self.state.store(IDLE, Ordering::Release);
                outcome
            }
            Ok(_) => {
                self.state.store(IDLE, Ordering::Release);
                VirtualEnumerationOutcomeV1::terminal(VirtualProviderStatusV1::INVALID, &request)
            }
            Err(payload) => {
                self.state.store(FAULTED, Ordering::Release);
                dispose_caught_panic_payload_v1(payload);
                VirtualEnumerationOutcomeV1::terminal(VirtualProviderStatusV1::PANICKED, &request)
            }
        }
    }

    fn read(&self, request: VirtualReadRequestV1) -> VirtualReadOutcomeV1 {
        if request.maximum_bytes == 0 || request.maximum_bytes as usize > MAX_VIRTUAL_READ_BYTES_V1
        {
            return VirtualReadOutcomeV1::terminal(VirtualProviderStatusV1::INVALID, &request);
        }
        if !self.enter() {
            return VirtualReadOutcomeV1::terminal(VirtualProviderStatusV1::PANICKED, &request);
        }
        let Some(provider) = self.provider.as_ref() else {
            self.state.store(FAULTED, Ordering::Release);
            return VirtualReadOutcomeV1::terminal(VirtualProviderStatusV1::FAILED, &request);
        };
        match catch_unwind(AssertUnwindSafe(|| provider.read(request.clone()))) {
            Ok(outcome)
                if outcome.status.is_known()
                    && outcome.bytes.len() <= request.maximum_bytes as usize =>
            {
                self.state.store(IDLE, Ordering::Release);
                outcome
            }
            Ok(_) => {
                self.state.store(IDLE, Ordering::Release);
                VirtualReadOutcomeV1::terminal(VirtualProviderStatusV1::INVALID, &request)
            }
            Err(payload) => {
                self.state.store(FAULTED, Ordering::Release);
                dispose_caught_panic_payload_v1(payload);
                VirtualReadOutcomeV1::terminal(VirtualProviderStatusV1::PANICKED, &request)
            }
        }
    }

    fn mutate(&self, request: VirtualMutationRequestV1) -> VirtualMutationOutcomeV1 {
        if request.reserved != 0
            || request.steps.is_empty()
            || request.steps.len() > MAX_VIRTUAL_MUTATION_STEPS_V1
        {
            return VirtualMutationOutcomeV1::terminal(VirtualProviderStatusV1::INVALID, &request);
        }
        if !self.enter() {
            return VirtualMutationOutcomeV1::terminal(VirtualProviderStatusV1::PANICKED, &request);
        }
        let Some(provider) = self.provider.as_ref() else {
            self.state.store(FAULTED, Ordering::Release);
            return VirtualMutationOutcomeV1::terminal(VirtualProviderStatusV1::FAILED, &request);
        };
        match catch_unwind(AssertUnwindSafe(|| provider.mutate(request.clone()))) {
            Ok(outcome) if outcome.status.is_known() => {
                self.state.store(IDLE, Ordering::Release);
                outcome
            }
            Ok(_) => {
                self.state.store(IDLE, Ordering::Release);
                VirtualMutationOutcomeV1::terminal(VirtualProviderStatusV1::INVALID, &request)
            }
            Err(payload) => {
                self.state.store(FAULTED, Ordering::Release);
                dispose_caught_panic_payload_v1(payload);
                VirtualMutationOutcomeV1::terminal(VirtualProviderStatusV1::PANICKED, &request)
            }
        }
    }
}

impl<T> Drop for Adapter<T> {
    fn drop(&mut self) {
        if let Some(provider) = self.provider.take()
            && let Err(payload) = catch_unwind(AssertUnwindSafe(|| drop(provider)))
        {
            dispose_caught_panic_payload_v1(payload);
        }
    }
}

impl VirtualFolderProviderObjectV1 {
    #[must_use]
    pub fn new<T: VirtualFolderProviderImplementationV1 + 'static>(provider: T) -> Self {
        Self(AbiVirtualFolderProviderV1_TO::from_value(
            Adapter {
                provider: Some(provider),
                state: AtomicU8::new(IDLE),
            },
            sabi_trait::TD_Opaque,
        ))
    }

    #[doc(hidden)]
    pub fn enumerate(&self, request: VirtualEnumerateRequestV1) -> VirtualEnumerationOutcomeV1 {
        self.0.enumerate(request)
    }

    #[doc(hidden)]
    pub fn read(&self, request: VirtualReadRequestV1) -> VirtualReadOutcomeV1 {
        self.0.read(request)
    }

    #[doc(hidden)]
    pub fn mutate(&self, request: VirtualMutationRequestV1) -> VirtualMutationOutcomeV1 {
        self.0.mutate(request)
    }
}
