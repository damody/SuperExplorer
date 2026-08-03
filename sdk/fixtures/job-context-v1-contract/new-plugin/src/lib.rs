//! Current-SDK DLL used to prove the job transport does not change root ABI shape.

#![allow(
    non_camel_case_types,
    reason = "abi_stable convention generates the RootModule reference suffix"
)]
#![allow(
    unsafe_code,
    reason = "the fixture wraps the process allocator to audit its own ABI allocations"
)]

use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicU64, Ordering},
};

use abi_stable::{
    export_root_module,
    prefix_type::PrefixTypeTrait,
    std_types::{ROption, RResult, RString, RVec},
};
use explorer_extension_api::{
    ABI_SCHEMA_V1, EXTENSION_ID_NAMESPACE_V1, ExtensionRegistrarV1, ExtensionRootModuleV1,
    ExtensionRootModuleV1_Ref, IncrementalResultBatchV1, IncrementalResultEntryV1, JobContextV1,
    JobControlStateV1, JobProgressStatusV1, JobProgressUpdateV1, JobProviderCallbackV1,
    JobProviderImplementationV1, JobTerminalV1, PluginMetadataV1, PluginValueKindV1, PluginValueV1,
    ROOT_MODULE_CONTRACT_ID_V1, RegistrarCallbackV1, RegistrarImplementationV1, RegistrarRequestV1,
    RegistrarResultV1, RegistrationOutcomeV1, SDK_MAJOR_VERSION_V1, SinkSubmitStatusV1, StableIdV1,
};

struct CountingAllocator;

static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static DEALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static ALLOCATED_BYTES: AtomicU64 = AtomicU64::new(0);
static DEALLOCATED_BYTES: AtomicU64 = AtomicU64::new(0);

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            ALLOCATED_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) };
        DEALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        DEALLOCATED_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let reallocated = unsafe { System.realloc(pointer, layout, new_size) };
        if !reallocated.is_null() {
            DEALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            DEALLOCATED_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            ALLOCATED_BYTES.fetch_add(new_size as u64, Ordering::Relaxed);
        }
        reallocated
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

#[repr(C)]
pub struct FixtureAllocatorSnapshotV1 {
    allocations: u64,
    deallocations: u64,
    allocated_bytes: u64,
    deallocated_bytes: u64,
}

#[unsafe(no_mangle)]
pub extern "C" fn superexplorer_job_context_v1_allocator_reset() {
    ALLOCATIONS.store(0, Ordering::Relaxed);
    DEALLOCATIONS.store(0, Ordering::Relaxed);
    ALLOCATED_BYTES.store(0, Ordering::Relaxed);
    DEALLOCATED_BYTES.store(0, Ordering::Relaxed);
}

#[unsafe(no_mangle)]
pub extern "C" fn superexplorer_job_context_v1_allocator_snapshot() -> FixtureAllocatorSnapshotV1 {
    FixtureAllocatorSnapshotV1 {
        allocations: ALLOCATIONS.load(Ordering::Relaxed),
        deallocations: DEALLOCATIONS.load(Ordering::Relaxed),
        allocated_bytes: ALLOCATED_BYTES.load(Ordering::Relaxed),
        deallocated_bytes: DEALLOCATED_BYTES.load(Ordering::Relaxed),
    }
}

struct FixtureRegistrar;

impl RegistrarImplementationV1 for FixtureRegistrar {
    fn register(_: RegistrarRequestV1) -> RegistrarResultV1 {
        RResult::ROk(RegistrationOutcomeV1::accepted(1))
    }
}

extern "C" fn describe_contract() -> StableIdV1 {
    ROOT_MODULE_CONTRACT_ID_V1
}

struct TransportProvider;

impl JobProviderImplementationV1 for TransportProvider {
    fn run(context: JobContextV1) -> JobTerminalV1 {
        if context.feature_epoch == u64::MAX {
            panic!("fixture provider panic");
        }
        match context.control_poll.poll(context.job) {
            state if state == JobControlStateV1::ACTIVE => {}
            state
                if state == JobControlStateV1::CANCELLED || state == JobControlStateV1::CLOSED =>
            {
                return JobTerminalV1::CANCELLED;
            }
            state if state == JobControlStateV1::DEADLINE_ELAPSED => {
                return JobTerminalV1::DEADLINE_ELAPSED;
            }
            _ => return JobTerminalV1::INCOMPATIBLE,
        }
        let item = match context.item {
            ROption::RSome(item) => item,
            ROption::RNone => return JobTerminalV1::INCOMPATIBLE,
        };
        let progress_source_generation = if context.feature_epoch == u64::MAX - 1 {
            context.source_generation.saturating_add(1)
        } else {
            context.source_generation
        };
        if context.feature_epoch != u64::MAX - 2 {
            for (sequence, completed_units) in [(1, 1), (2, 2)] {
                match context.progress.try_submit(JobProgressUpdateV1 {
                    job: context.job,
                    sink_capability: context.progress.capability,
                    job_generation: context.job_generation,
                    item_generation: context.item_generation,
                    location_generation: context.location_generation,
                    source_generation: progress_source_generation,
                    sequence,
                    completed_units,
                    total_units: 2,
                    reserved: 0,
                }) {
                    status if status == JobProgressStatusV1::ACCEPTED => {}
                    status
                        if status == JobProgressStatusV1::STALE
                            || status == JobProgressStatusV1::CLOSED =>
                    {
                        return JobTerminalV1::BACKPRESSURED;
                    }
                    _ => return JobTerminalV1::PLUGIN_ERROR,
                }
            }
        }
        let outcome = context.sink.try_submit(IncrementalResultBatchV1 {
            job: context.job,
            sink_capability: context.sink.capability,
            job_generation: context.job_generation,
            location: context.location,
            location_generation: context.location_generation,
            source_generation: context.source_generation,
            sequence: 7,
            entries: RVec::from(vec![IncrementalResultEntryV1 {
                item,
                item_generation: context.item_generation,
                source_generation: context.source_generation,
                value: PluginValueV1 {
                    kind: PluginValueKindV1::TEXT,
                    reserved: 0,
                    integer: 0,
                    float: 0.0,
                    text: RString::from("fixture"),
                    payload: RVec::new(),
                    opaque_schema: StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 0),
                    opaque_schema_version: 0,
                    reserved_tail: 0,
                },
            }]),
        });
        if outcome.status == SinkSubmitStatusV1::ACCEPTED {
            return matches!(outcome.rejected_batch, ROption::RNone)
                .then_some(JobTerminalV1::COMPLETED)
                .unwrap_or(JobTerminalV1::PLUGIN_ERROR);
        }
        let batch = match outcome.rejected_batch {
            ROption::RSome(batch) => batch,
            ROption::RNone => return JobTerminalV1::PLUGIN_ERROR,
        };
        (batch.job == context.job
            && batch.sink_capability == context.sink.capability
            && batch.job_generation == context.job_generation
            && batch.location == context.location
            && batch.location_generation == context.location_generation
            && batch.source_generation == context.source_generation
            && batch.sequence == 7
            && batch.entries.len() == 1
            && batch.entries[0].item == item
            && batch.entries[0].item_generation == context.item_generation
            && batch.entries[0].source_generation == context.source_generation)
            .then_some(JobTerminalV1::BACKPRESSURED)
            .unwrap_or(JobTerminalV1::PLUGIN_ERROR)
    }
}

/// Contract-fixture-only entry point exercised by the host through `libloading`.
///
/// The root registrar remains the ordinary extension ABI; this proves that the
/// synchronous job context remains sound when it crosses the plugin DLL boundary.
#[unsafe(no_mangle)]
pub extern "C" fn superexplorer_job_context_v1_contract_run(
    context: JobContextV1,
) -> JobTerminalV1 {
    JobProviderCallbackV1::new::<TransportProvider>().invoke(context)
}

#[export_root_module]
pub fn get_library() -> ExtensionRootModuleV1_Ref {
    let registrar = ExtensionRegistrarV1 {
        register: RegistrarCallbackV1::new::<FixtureRegistrar>(),
        describe_contract,
        ui_abi_fingerprint_sha256: ROption::RNone,
    }
    .leak_into_prefix();
    ExtensionRootModuleV1 {
        abi_schema: ABI_SCHEMA_V1,
        root_contract_id: ROOT_MODULE_CONTRACT_ID_V1,
        sdk_major: SDK_MAJOR_VERSION_V1,
        reserved: 0,
        metadata: PluginMetadataV1 {
            plugin_id: StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 420),
            primary_interface_id: StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 421),
        },
        registrar,
    }
    .leak_into_prefix()
}
