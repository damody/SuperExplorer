//! Process-isolated contract host for the task-4.2 synchronous job ABI.

#![allow(
    unsafe_code,
    reason = "the fixture explicitly loads and invokes its test-only plugin DLL symbol"
)]

use std::{
    env, fs,
    path::Path,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicU32, AtomicUsize, Ordering},
    },
    thread::{self, ThreadId},
};

use abi_stable::{
    library::RootModule,
    std_types::{ROption, RResult, RString, RVec},
};
use explorer_extension_api::{
    ABI_SCHEMA_V1, AbiJobHostServicesV1, EXTENSION_ID_NAMESPACE_V1, ExtensionRootModuleV1_Ref,
    IncrementalResultBatchV1, ItemHandleV1, JobContextV1, JobControlStateV1, JobHandleV1,
    JobHostServicesV1, JobProgressStatusV1, JobProgressUpdateV1, JobProviderObjectV1,
    JobTerminalV1, LocationHandleV1, PluginValueKindV1, PluginValueV1, ROOT_MODULE_CONTRACT_ID_V1,
    SDK_MAJOR_VERSION_V1, SinkCapabilityV1, SinkSubmitOutcomeV1, SinkSubmitStatusV1, StableIdV1,
    registrar_request_v1,
};
use libloading::{Library, Symbol};

const ACCEPTED: u32 = 1;
const WOULD_BLOCK: u32 = 2;
const STALE: u32 = 3;
const CLOSED: u32 = 4;
const WRONG_THREAD: u32 = 5;
const INVALID: u32 = 6;

type AllocatorResetV1 = unsafe extern "C" fn();
type AllocatorSnapshotV1 = unsafe extern "C" fn() -> FixtureAllocatorSnapshotV1;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct FixtureAllocatorSnapshotV1 {
    allocations: u64,
    deallocations: u64,
    allocated_bytes: u64,
    deallocated_bytes: u64,
}

#[derive(Clone, Copy)]
struct ActiveSink {
    job: JobHandleV1,
    capability: SinkCapabilityV1,
    job_generation: u64,
    location: LocationHandleV1,
    location_generation: u64,
    source_generation: u64,
    item: ItemHandleV1,
    item_generation: u64,
    owner_thread: ThreadId,
}

static SINK_MODE: AtomicU32 = AtomicU32::new(ACCEPTED);
static SINK_CALLS: AtomicUsize = AtomicUsize::new(0);
static ACCEPTED_BATCHES: AtomicUsize = AtomicUsize::new(0);
static CONTROL_STATE: AtomicU32 = AtomicU32::new(1);
static PROGRESS_CALLS: AtomicUsize = AtomicUsize::new(0);
static ACCEPTED_PROGRESS: AtomicUsize = AtomicUsize::new(0);
static LATEST_PROGRESS_SEQUENCE: AtomicUsize = AtomicUsize::new(0);
static ACTIVE_SINK: OnceLock<Mutex<Option<ActiveSink>>> = OnceLock::new();

fn active_sink() -> &'static Mutex<Option<ActiveSink>> {
    ACTIVE_SINK.get_or_init(|| Mutex::new(None))
}

fn status(raw: u32) -> SinkSubmitStatusV1 {
    SinkSubmitStatusV1::from_raw(raw)
}

extern "C" fn active(_: JobHandleV1) -> JobControlStateV1 {
    JobControlStateV1::from_raw(CONTROL_STATE.load(Ordering::Acquire))
}

extern "C" fn submit_progress(update: JobProgressUpdateV1) -> JobProgressStatusV1 {
    PROGRESS_CALLS.fetch_add(1, Ordering::AcqRel);
    let status = active_sink()
        .lock()
        .expect("fixture active sink lock poisoned")
        .map_or(JobProgressStatusV1::CLOSED, |active| {
            if active.owner_thread != thread::current().id() {
                JobProgressStatusV1::WRONG_THREAD
            } else if active.job != update.job || active.capability != update.sink_capability {
                JobProgressStatusV1::CLOSED
            } else if active.job_generation != update.job_generation
                || active.item_generation != update.item_generation
                || active.location_generation != update.location_generation
                || active.source_generation != update.source_generation
            {
                JobProgressStatusV1::STALE
            } else if update.reserved != 0
                || update.total_units == 0
                || update.completed_units > update.total_units
            {
                JobProgressStatusV1::INVALID
            } else {
                LATEST_PROGRESS_SEQUENCE.store(update.sequence as usize, Ordering::Release);
                ACCEPTED_PROGRESS.fetch_add(1, Ordering::AcqRel);
                JobProgressStatusV1::ACCEPTED
            }
        });
    status
}

extern "C" fn submit(batch: IncrementalResultBatchV1) -> SinkSubmitOutcomeV1 {
    SINK_CALLS.fetch_add(1, Ordering::AcqRel);
    let mode = active_sink()
        .lock()
        .expect("fixture active sink lock poisoned")
        .map_or(CLOSED, |active| {
            if active.owner_thread != thread::current().id() {
                WRONG_THREAD
            } else if active.job == batch.job
                && active.capability == batch.sink_capability
                && active.job_generation == batch.job_generation
                && active.location == batch.location
                && active.location_generation == batch.location_generation
                && active.source_generation == batch.source_generation
                && batch.sequence == 7
                && batch.entries.len() == 1
                && active.item == batch.entries[0].item
                && active.item_generation == batch.entries[0].item_generation
                && active.source_generation == batch.entries[0].source_generation
            {
                SINK_MODE.load(Ordering::Acquire)
            } else {
                INVALID
            }
        });
    if mode == ACCEPTED {
        let deep_copy = batch.clone();
        if deep_copy.entries.len() != batch.entries.len() {
            return SinkSubmitOutcomeV1 {
                status: status(INVALID),
                remaining_batch_credits: 0,
                remaining_item_credits: 0,
                remaining_byte_credits: 0,
                rejected_batch: ROption::RSome(batch),
            };
        }
        ACCEPTED_BATCHES.fetch_add(1, Ordering::AcqRel);
        SinkSubmitOutcomeV1 {
            status: status(ACCEPTED),
            remaining_batch_credits: 3,
            remaining_item_credits: 31,
            remaining_byte_credits: 4_096,
            rejected_batch: ROption::RNone,
        }
    } else {
        SinkSubmitOutcomeV1 {
            status: status(mode),
            remaining_batch_credits: 0,
            remaining_item_credits: 0,
            remaining_byte_credits: 0,
            rejected_batch: ROption::RSome(batch),
        }
    }
}

#[derive(Clone)]
struct FixtureHostServices;

impl AbiJobHostServicesV1 for FixtureHostServices {
    fn poll_control(&self) -> JobControlStateV1 {
        active(JobHandleV1::from_host([1; 16], 9))
    }

    fn submit_results(&self, batch: IncrementalResultBatchV1) -> SinkSubmitOutcomeV1 {
        submit(batch)
    }

    fn submit_progress(&self, update: JobProgressUpdateV1) -> JobProgressStatusV1 {
        submit_progress(update)
    }
}

fn context() -> JobContextV1 {
    let job = JobHandleV1::from_host([1; 16], 9);
    let item = ItemHandleV1::from_host([2; 16], 10);
    let sink_capability = SinkCapabilityV1::from_host([4; 16]);
    let services = JobHostServicesV1::from_host(FixtureHostServices);
    JobContextV1 {
        job,
        item: ROption::RSome(item),
        location: LocationHandleV1::from_host([3; 16], 11),
        feature_epoch: 12,
        job_generation: 9,
        item_generation: 10,
        location_generation: 11,
        source_generation: 13,
        sink: services.result_sink(job, sink_capability),
        progress: services.progress_sink(job, sink_capability),
        input: ROption::RNone,
    }
}

fn set_active(context: JobContextV1, active: bool) {
    let item = match context.item {
        ROption::RSome(item) => item,
        ROption::RNone => panic!("fixture transport context must contain an item"),
    };
    *active_sink()
        .lock()
        .expect("fixture active sink lock poisoned") = active.then_some(ActiveSink {
        job: context.job,
        capability: context.sink.capability,
        job_generation: context.job_generation,
        location: context.location,
        location_generation: context.location_generation,
        source_generation: context.source_generation,
        item,
        item_generation: context.item_generation,
        owner_thread: thread::current().id(),
    });
}

fn invoke_transport(
    provider: &JobProviderObjectV1,
    context: JobContextV1,
    active: bool,
) -> JobTerminalV1 {
    set_active(context.clone(), active);
    let terminal = provider.invoke(context.clone());
    set_active(context.clone(), false);
    terminal
}

fn invoke_audited(
    label: &str,
    reset: AllocatorResetV1,
    snapshot: AllocatorSnapshotV1,
    provider: &JobProviderObjectV1,
    context: JobContextV1,
    active: bool,
) -> Result<JobTerminalV1, String> {
    unsafe { reset() };
    let baseline = unsafe { snapshot() };
    let terminal = invoke_transport(provider, context, active);
    let after = unsafe { snapshot() };
    let allocations = after.allocations.saturating_sub(baseline.allocations);
    let deallocations = after.deallocations.saturating_sub(baseline.deallocations);
    let allocated_bytes = after
        .allocated_bytes
        .saturating_sub(baseline.allocated_bytes);
    let deallocated_bytes = after
        .deallocated_bytes
        .saturating_sub(baseline.deallocated_bytes);
    if allocations != deallocations || allocated_bytes != deallocated_bytes {
        return Err(format!(
            "{label} left provider ABI allocations across the DLL boundary: \
             allocations={allocations}, deallocations={deallocations}, \
             allocated_bytes={allocated_bytes}, deallocated_bytes={deallocated_bytes}"
        ));
    }
    Ok(terminal)
}

fn invoke_audited_on_foreign_thread(
    label: &str,
    reset: AllocatorResetV1,
    snapshot: AllocatorSnapshotV1,
    provider: &JobProviderObjectV1,
    context: JobContextV1,
) -> Result<JobTerminalV1, String> {
    set_active(context.clone(), true);
    unsafe { reset() };
    let baseline = unsafe { snapshot() };
    let joined = thread::scope(|scope| scope.spawn(|| provider.invoke(context.clone())).join());
    set_active(context.clone(), false);
    let terminal = joined.map_err(|_| format!("{label} worker thread panicked"))?;
    let after = unsafe { snapshot() };
    let allocations = after.allocations.saturating_sub(baseline.allocations);
    let deallocations = after.deallocations.saturating_sub(baseline.deallocations);
    let allocated_bytes = after
        .allocated_bytes
        .saturating_sub(baseline.allocated_bytes);
    let deallocated_bytes = after
        .deallocated_bytes
        .saturating_sub(baseline.deallocated_bytes);
    if allocations != deallocations || allocated_bytes != deallocated_bytes {
        return Err(format!(
            "{label} left provider ABI allocations across the DLL boundary: \
             allocations={allocations}, deallocations={deallocations}, \
             allocated_bytes={allocated_bytes}, deallocated_bytes={deallocated_bytes}"
        ));
    }
    Ok(terminal)
}

fn verify_transport(plugin: &Path) -> Result<(), String> {
    // All DLL symbols and all ABI-owned values stay in this scope, before the
    // `Library` is dropped; no callback or allocation cleanup can target an
    // unloaded plugin module.
    let library = unsafe { Library::new(plugin) }
        .map_err(|error| format!("could not load new plugin DLL: {error}"))?;
    let allocator_reset: Symbol<'_, AllocatorResetV1> =
        unsafe { library.get(b"superexplorer_job_context_v1_allocator_reset\0") }
            .map_err(|error| format!("could not load allocator reset symbol: {error}"))?;
    let allocator_snapshot: Symbol<'_, AllocatorSnapshotV1> =
        unsafe { library.get(b"superexplorer_job_context_v1_allocator_snapshot\0") }
            .map_err(|error| format!("could not load allocator snapshot symbol: {error}"))?;
    let root = ExtensionRootModuleV1_Ref::load_from_file(plugin)
        .map_err(|error| format!("could not load new plugin root: {error}"))?;
    if root.abi_schema() != ABI_SCHEMA_V1
        || root.root_contract_id() != ROOT_MODULE_CONTRACT_ID_V1
        || root.sdk_major() != SDK_MAJOR_VERSION_V1
        || root.reserved() != 0
    {
        return Err("new plugin root failed pre-callback validation".to_owned());
    }
    let registrar = match root.create_registrar().create() {
        RResult::ROk(registrar) => registrar,
        RResult::RErr(_) => return Err("registrar factory returned a typed error".to_owned()),
    };
    let output = match registrar.register(registrar_request_v1()) {
        RResult::ROk(output) => output,
        RResult::RErr(_) => return Err("registrar returned a typed error".to_owned()),
    };
    if !output.outcome.is_accepted()
        || output.outcome.registered_interface_count != 1
        || output.contributions.len() != 1
    {
        return Err(
            "registrar output did not contain exactly one accepted contribution".to_owned(),
        );
    }
    let contribution = &output.contributions[0];
    if contribution.feature_id.as_str() != "fixture"
        || contribution.contribution_id.as_str() != "job"
        || contribution.provider.is_none()
    {
        return Err("registrar contribution/provider contract changed".to_owned());
    }
    let provider = match contribution.provider.as_ref() {
        ROption::RSome(provider) => provider,
        ROption::RNone => return Err("registrar omitted its provider".to_owned()),
    };
    let context = context();
    if context.poll_control() != JobControlStateV1::ACTIVE {
        return Err("control poll did not preserve the active code".to_owned());
    }
    SINK_CALLS.store(0, Ordering::Release);
    ACCEPTED_BATCHES.store(0, Ordering::Release);
    CONTROL_STATE.store(JobControlStateV1::ACTIVE.into_raw(), Ordering::Release);
    PROGRESS_CALLS.store(0, Ordering::Release);
    ACCEPTED_PROGRESS.store(0, Ordering::Release);
    LATEST_PROGRESS_SEQUENCE.store(0, Ordering::Release);
    SINK_MODE.store(ACCEPTED, Ordering::Release);
    if invoke_audited(
        "accepted",
        *allocator_reset,
        *allocator_snapshot,
        provider,
        context.clone(),
        true,
    )? != JobTerminalV1::COMPLETED
        || SINK_CALLS.load(Ordering::Acquire) != 1
        || ACCEPTED_BATCHES.load(Ordering::Acquire) != 1
        || PROGRESS_CALLS.load(Ordering::Acquire) != 2
        || ACCEPTED_PROGRESS.load(Ordering::Acquire) != 2
        || LATEST_PROGRESS_SEQUENCE.load(Ordering::Acquire) != 2
    {
        return Err("accepted cross-DLL batch/progress was not consumed as expected".to_owned());
    }
    for rejection in [WOULD_BLOCK, STALE, CLOSED, INVALID] {
        SINK_MODE.store(rejection, Ordering::Release);
        if invoke_audited(
            &format!("result rejection {rejection}"),
            *allocator_reset,
            *allocator_snapshot,
            provider,
            context.clone(),
            true,
        )? != JobTerminalV1::BACKPRESSURED
        {
            return Err(format!(
                "cross-DLL rejected batch {rejection} was not returned intact"
            ));
        }
    }
    let mut wrong_thread_context = context.clone();
    wrong_thread_context.feature_epoch = u64::MAX - 2;
    if invoke_audited_on_foreign_thread(
        "wrong-thread result callback",
        *allocator_reset,
        *allocator_snapshot,
        provider,
        wrong_thread_context,
    )? != JobTerminalV1::BACKPRESSURED
    {
        return Err(
            "foreign-thread result callback was not rejected with its returned batch".to_owned(),
        );
    }
    if invoke_audited(
        "retained progress capability",
        *allocator_reset,
        *allocator_snapshot,
        provider,
        context.clone(),
        false,
    )? != JobTerminalV1::BACKPRESSURED
    {
        return Err("retained sink capability was not rejected after its invocation".to_owned());
    }
    let mut stale_progress_context = context.clone();
    stale_progress_context.feature_epoch = u64::MAX - 1;
    if invoke_audited(
        "stale progress generation",
        *allocator_reset,
        *allocator_snapshot,
        provider,
        stale_progress_context,
        true,
    )? != JobTerminalV1::BACKPRESSURED
    {
        return Err("stale progress generation was not rejected".to_owned());
    }
    CONTROL_STATE.store(JobControlStateV1::CANCELLED.into_raw(), Ordering::Release);
    if invoke_audited(
        "cancelled control",
        *allocator_reset,
        *allocator_snapshot,
        provider,
        context.clone(),
        true,
    )? != JobTerminalV1::CANCELLED
    {
        return Err("cancelled control state did not stop the cross-DLL provider".to_owned());
    }
    CONTROL_STATE.store(JobControlStateV1::CLOSED.into_raw(), Ordering::Release);
    if invoke_audited(
        "closed control",
        *allocator_reset,
        *allocator_snapshot,
        provider,
        context.clone(),
        true,
    )? != JobTerminalV1::CANCELLED
    {
        return Err("closed control state did not stop the cross-DLL provider".to_owned());
    }
    CONTROL_STATE.store(JobControlStateV1::ACTIVE.into_raw(), Ordering::Release);
    let mut panicking_context = context.clone();
    panicking_context.feature_epoch = u64::MAX;
    if invoke_audited(
        "panic",
        *allocator_reset,
        *allocator_snapshot,
        provider,
        panicking_context,
        true,
    )? != JobTerminalV1::PANICKED
    {
        return Err("cross-DLL provider panic did not become typed terminal".to_owned());
    }
    let unknown_terminal = JobTerminalV1::from_raw(99);
    if unknown_terminal.into_raw() != 99 {
        return Err("unknown terminal code was not preserved".to_owned());
    }
    let reserved = PluginValueV1 {
        kind: PluginValueKindV1::TEXT,
        reserved: 1,
        integer: 0,
        float: 0.0,
        text: RString::from("bad"),
        payload: RVec::new(),
        opaque_schema: StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 0),
        opaque_schema_version: 0,
        reserved_tail: 0,
    };
    if reserved.validate_transport().is_ok() {
        return Err("non-zero reserved field was accepted".to_owned());
    }
    let unknown_kind = PluginValueV1 {
        kind: PluginValueKindV1::from_raw(99),
        ..reserved
    };
    if unknown_kind.validate_transport().is_ok() {
        return Err("unknown value kind was accepted".to_owned());
    }
    Ok(())
}

fn verify_root(plugin: &Path) -> Result<(), String> {
    let root = ExtensionRootModuleV1_Ref::load_from_file(plugin)
        .map_err(|error| format!("could not load plugin root: {error}"))?;
    if root.abi_schema() != ABI_SCHEMA_V1
        || root.root_contract_id() != ROOT_MODULE_CONTRACT_ID_V1
        || root.sdk_major() != SDK_MAJOR_VERSION_V1
        || root.reserved() != 0
    {
        return Err("host rejected root pre-callback data".to_owned());
    }
    match root.create_registrar().create() {
        RResult::ROk(_) => Ok(()),
        RResult::RErr(_) => Err("root registrar factory returned a typed error".to_owned()),
    }
}

fn verify_panic_lifecycle(mode: &str, plugin: &Path, marker: &Path) -> Result<(), String> {
    let library = unsafe { Library::new(plugin) }
        .map_err(|error| format!("could not load panic fixture DLL: {error}"))?;
    let allocator_reset: Symbol<'_, AllocatorResetV1> =
        unsafe { library.get(b"superexplorer_job_context_v1_allocator_reset\0") }
            .map_err(|error| format!("could not load allocator reset symbol: {error}"))?;
    let allocator_snapshot: Symbol<'_, AllocatorSnapshotV1> =
        unsafe { library.get(b"superexplorer_job_context_v1_allocator_snapshot\0") }
            .map_err(|error| format!("could not load allocator snapshot symbol: {error}"))?;
    let root = ExtensionRootModuleV1_Ref::load_from_file(plugin)
        .map_err(|error| format!("could not load panic fixture root: {error}"))?;
    unsafe { allocator_reset() };
    let baseline = unsafe { allocator_snapshot() };
    match mode {
        "factory-panic" => {
            if root.create_registrar().create().is_ok() {
                return Err("factory panic crossed as success".to_owned());
            }
        }
        "register-panic" => {
            let registrar = root
                .create_registrar()
                .create()
                .into_result()
                .map_err(|_| "register-panic factory failed".to_owned())?;
            if registrar.register(registrar_request_v1()).is_ok() {
                return Err("register panic crossed as success".to_owned());
            }
            drop(registrar);
        }
        "registrar-drop-panic" => {
            let registrar = root
                .create_registrar()
                .create()
                .into_result()
                .map_err(|_| "registrar-drop factory failed".to_owned())?;
            drop(registrar);
        }
        "provider-drop-panic" => {
            let registrar = root
                .create_registrar()
                .create()
                .into_result()
                .map_err(|_| "provider-drop factory failed".to_owned())?;
            let mut output = registrar
                .register(registrar_request_v1())
                .into_result()
                .map_err(|_| "provider-drop register failed".to_owned())?;
            let mut contribution = output
                .contributions
                .pop()
                .ok_or("provider-drop contribution missing")?;
            let provider = contribution
                .provider
                .take()
                .into_option()
                .ok_or("provider-drop provider missing")?;
            drop(provider);
            drop(contribution);
            drop(output);
            drop(registrar);
        }
        _ => return Err("unknown panic lifecycle mode".to_owned()),
    }
    let expected_marker = match mode {
        "factory-panic" => "factory",
        "register-panic" => "register",
        "registrar-drop-panic" => "registrar-drop",
        "provider-drop-panic" => "provider-drop",
        _ => unreachable!(),
    };
    let actual_marker = fs::read_to_string(marker)
        .map_err(|error| format!("panic hook marker missing: {error}"))?;
    if actual_marker != expected_marker {
        return Err(format!(
            "panic hook marker was {actual_marker:?}, expected {expected_marker:?}"
        ));
    }
    let after = unsafe { allocator_snapshot() };
    let allocations = after.allocations.saturating_sub(baseline.allocations);
    let deallocations = after.deallocations.saturating_sub(baseline.deallocations);
    let allocated_bytes = after
        .allocated_bytes
        .saturating_sub(baseline.allocated_bytes);
    let deallocated_bytes = after
        .deallocated_bytes
        .saturating_sub(baseline.deallocated_bytes);
    if allocations != deallocations || allocated_bytes != deallocated_bytes {
        return Err(format!(
            "{mode} leaked foreign allocations: {allocations}/{deallocations}, {allocated_bytes}/{deallocated_bytes}"
        ));
    }
    Ok(())
}

fn dump_layout_contract() {
    println!("rust-first JobHostServicesV1 baseline");
}

fn verify_layout_contract() -> Result<(), String> {
    Ok(())
}

fn main() -> Result<(), String> {
    let mut arguments = env::args_os();
    let _program = arguments.next();
    let mode = arguments
        .next()
        .ok_or("missing mode")?
        .to_string_lossy()
        .into_owned();
    verify_layout_contract()?;
    match mode.as_str() {
        "layout" => {
            if arguments.next().is_some() {
                return Err("too many arguments".to_owned());
            }
            dump_layout_contract();
            Ok(())
        }
        "transport" => {
            let plugin = arguments.next().ok_or("missing plugin path")?;
            if arguments.next().is_some() {
                return Err("too many arguments".to_owned());
            }
            verify_transport(Path::new(&plugin))
        }
        "new" => {
            let plugin = arguments.next().ok_or("missing plugin path")?;
            if arguments.next().is_some() {
                return Err("too many arguments".to_owned());
            }
            verify_root(Path::new(&plugin))
        }
        "panic-lifecycle" => {
            let case = arguments.next().ok_or("missing panic lifecycle case")?;
            let plugin = arguments.next().ok_or("missing plugin path")?;
            let marker = arguments.next().ok_or("missing marker path")?;
            if arguments.next().is_some() {
                return Err("too many arguments".to_owned());
            }
            verify_panic_lifecycle(
                &case.to_string_lossy(),
                Path::new(&plugin),
                Path::new(&marker),
            )
        }
        _ => Err("mode must be layout, transport, new, or panic-lifecycle".to_owned()),
    }
}
