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
    env, fs,
    sync::atomic::{AtomicU64, Ordering},
};

use abi_stable::{
    export_root_module,
    prefix_type::PrefixTypeTrait,
    std_types::{ROption, RResult, RString, RVec},
};
use explorer_extension_api::{
    EXTENSION_ID_NAMESPACE_V1, ExtensionRegistrarImplementationV1,
    ExtensionRootModuleV1, ExtensionRootModuleV1_Ref, IncrementalResultBatchV1,
    IncrementalResultEntryV1, JobContextV1, JobControlStateV1, JobProgressStatusV1,
    JobProgressUpdateV1, JobProviderImplementationV1, JobProviderObjectV1, JobTerminalV1,
    PluginItemResultV1, PluginMetadataV1, PluginValueKindV1, PluginValueV1,
    RegisteredContributionKindV1, RegisteredContributionV1,
    RegistrarOutputResultV1, RegistrarOutputV1,
    RegistrarRequestV1, RegistrationOutcomeV1, SinkSubmitStatusV1,
    StableIdV1,
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
fn mode() -> String {
    env::var("JOB_CONTEXT_V1_MODE").unwrap_or_default()
}
fn marker(value: &str) {
    if let Ok(path) = env::var("JOB_CONTEXT_V1_MARKER") {
        let _ = fs::write(path, value);
    }
}
impl Drop for FixtureRegistrar {
    fn drop(&mut self) {
        if mode() == "registrar-drop-panic" {
            marker("registrar-drop");
            panic!("fixture registrar drop panic");
        }
    }
}

impl ExtensionRegistrarImplementationV1 for FixtureRegistrar {
    fn create() -> Self {
        if mode() == "factory-panic" {
            marker("factory");
            panic!("fixture factory panic");
        }
        Self
    }
    fn register(&self, _: RegistrarRequestV1) -> RegistrarOutputResultV1 {
        if mode() == "register-panic" {
            marker("register");
            panic!("fixture register panic");
        }
        RResult::ROk(RegistrarOutputV1 {
            outcome: RegistrationOutcomeV1::accepted(1),
            contributions: RVec::from(vec![RegisteredContributionV1 {
                feature_id: RString::from("fixture"),
                contribution_id: RString::from("job"),
                kind: RegisteredContributionKindV1::COLUMN,
                required_capabilities: RVec::new(),
                interface_id: StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 421),
                expected_sort: ROption::RNone,
                opaque_contract: ROption::RNone,
                renderer_contribution_id: ROption::RNone,
                provider: ROption::RSome(JobProviderObjectV1::new(TransportProvider)),
                visual_column: ROption::RNone,
                size_map_view: ROption::RNone,
                batch_column_provider: ROption::RNone,
            }]),
        })
    }
}

struct TransportProvider;
impl Drop for TransportProvider {
    fn drop(&mut self) {
        if mode() == "provider-drop-panic" {
            marker("provider-drop");
            panic!("fixture provider drop panic");
        }
    }
}

impl JobProviderImplementationV1 for TransportProvider {
    fn run(&self, context: JobContextV1) -> JobTerminalV1 {
        if context.feature_epoch == u64::MAX {
            panic!("fixture provider panic");
        }
        match context.poll_control() {
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
                result: PluginItemResultV1::value(
                    PluginValueV1 {
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
                    ROption::RNone,
                ),
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

#[export_root_module]
pub fn get_library() -> ExtensionRootModuleV1_Ref {
    ExtensionRootModuleV1::new::<FixtureRegistrar>(
        PluginMetadataV1 {
            plugin_id: StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 420),
            primary_interface_id: StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 421),
        },
        ROption::RNone,
    )
    .leak_into_prefix()
}
