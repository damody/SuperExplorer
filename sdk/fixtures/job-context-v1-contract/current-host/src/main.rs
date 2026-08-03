//! Process-isolated contract host for the task-4.2 synchronous job ABI.

#![allow(
    unsafe_code,
    reason = "the fixture explicitly loads and invokes its test-only plugin DLL symbol"
)]

use std::{
    env,
    mem::{align_of, offset_of, size_of},
    path::Path,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicU32, AtomicUsize, Ordering},
    },
    thread::{self, ThreadId},
};

use abi_stable::{
    library::RootModule,
    std_types::{ROption, RString, RVec},
};
use explorer_extension_api::{
    ABI_SCHEMA_V1, EXTENSION_ID_NAMESPACE_V1, ExtensionRootModuleV1_Ref, IncrementalResultBatchV1,
    IncrementalResultEntryV1, IncrementalResultSinkV1, IncrementalResultSubmitV1, ItemHandleV1,
    JobContextV1, JobControlPollV1, JobControlStateV1, JobHandleV1, JobProgressSinkV1,
    JobProgressStatusV1, JobProgressSubmitV1, JobProgressUpdateV1, JobProviderCallbackV1,
    JobTerminalV1, LocationHandleV1, PluginValueKindV1, PluginValueV1, ROOT_MODULE_CONTRACT_ID_V1,
    SDK_MAJOR_VERSION_V1, SinkCapabilityV1, SinkSubmitOutcomeV1, SinkSubmitStatusV1, StableIdV1,
};
use libloading::{Library, Symbol};

const ACCEPTED: u32 = 1;
const WOULD_BLOCK: u32 = 2;
const STALE: u32 = 3;
const CLOSED: u32 = 4;
const WRONG_THREAD: u32 = 5;
const INVALID: u32 = 6;

type RunJobContextV1 = unsafe extern "C" fn(JobContextV1) -> JobTerminalV1;
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

fn context() -> JobContextV1 {
    let job = JobHandleV1::from_host([1; 16], 9);
    let item = ItemHandleV1::from_host([2; 16], 10);
    let sink_capability = SinkCapabilityV1::from_host([4; 16]);
    JobContextV1 {
        job,
        item: ROption::RSome(item),
        location: LocationHandleV1::from_host([3; 16], 11),
        feature_epoch: 12,
        job_generation: 9,
        item_generation: 10,
        location_generation: 11,
        source_generation: 13,
        control_poll: JobControlPollV1::from_host(active),
        sink: IncrementalResultSinkV1 {
            job,
            capability: sink_capability,
            submit: IncrementalResultSubmitV1::from_host(submit),
        },
        progress: JobProgressSinkV1 {
            job,
            capability: sink_capability,
            submit: JobProgressSubmitV1::from_host(submit_progress),
        },
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

fn invoke_transport(run: RunJobContextV1, context: JobContextV1, active: bool) -> JobTerminalV1 {
    set_active(context, active);
    let terminal = unsafe { run(context) };
    set_active(context, false);
    terminal
}

fn invoke_audited(
    label: &str,
    reset: AllocatorResetV1,
    snapshot: AllocatorSnapshotV1,
    run: RunJobContextV1,
    context: JobContextV1,
    active: bool,
) -> Result<JobTerminalV1, String> {
    unsafe { reset() };
    let baseline = unsafe { snapshot() };
    let terminal = invoke_transport(run, context, active);
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
    run: RunJobContextV1,
    context: JobContextV1,
) -> Result<JobTerminalV1, String> {
    set_active(context, true);
    unsafe { reset() };
    let baseline = unsafe { snapshot() };
    let joined = thread::spawn(move || unsafe { run(context) }).join();
    set_active(context, false);
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
    let run: Symbol<'_, RunJobContextV1> =
        unsafe { library.get(b"superexplorer_job_context_v1_contract_run\0") }
            .map_err(|error| format!("could not load job context fixture symbol: {error}"))?;
    let allocator_reset: Symbol<'_, AllocatorResetV1> =
        unsafe { library.get(b"superexplorer_job_context_v1_allocator_reset\0") }
            .map_err(|error| format!("could not load allocator reset symbol: {error}"))?;
    let allocator_snapshot: Symbol<'_, AllocatorSnapshotV1> =
        unsafe { library.get(b"superexplorer_job_context_v1_allocator_snapshot\0") }
            .map_err(|error| format!("could not load allocator snapshot symbol: {error}"))?;
    let context = context();
    if context.control_poll.poll(context.job) != JobControlStateV1::ACTIVE {
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
        *run,
        context,
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
            *run,
            context,
            true,
        )? != JobTerminalV1::BACKPRESSURED
        {
            return Err(format!(
                "cross-DLL rejected batch {rejection} was not returned intact"
            ));
        }
    }
    let mut wrong_thread_context = context;
    wrong_thread_context.feature_epoch = u64::MAX - 2;
    if invoke_audited_on_foreign_thread(
        "wrong-thread result callback",
        *allocator_reset,
        *allocator_snapshot,
        *run,
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
        *run,
        context,
        false,
    )? != JobTerminalV1::BACKPRESSURED
    {
        return Err("retained sink capability was not rejected after its invocation".to_owned());
    }
    let mut stale_progress_context = context;
    stale_progress_context.feature_epoch = u64::MAX - 1;
    if invoke_audited(
        "stale progress generation",
        *allocator_reset,
        *allocator_snapshot,
        *run,
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
        *run,
        context,
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
        *run,
        context,
        true,
    )? != JobTerminalV1::CANCELLED
    {
        return Err("closed control state did not stop the cross-DLL provider".to_owned());
    }
    CONTROL_STATE.store(JobControlStateV1::ACTIVE.into_raw(), Ordering::Release);
    let mut panicking_context = context;
    panicking_context.feature_epoch = u64::MAX;
    if invoke_audited(
        "panic",
        *allocator_reset,
        *allocator_snapshot,
        *run,
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

fn verify_root(kind: &str, plugin: &Path) -> Result<(), String> {
    let root = ExtensionRootModuleV1_Ref::load_from_file(plugin)
        .map_err(|error| format!("could not load plugin root: {error}"))?;
    if root.abi_schema() != ABI_SCHEMA_V1
        || root.root_contract_id() != ROOT_MODULE_CONTRACT_ID_V1
        || root.sdk_major() != SDK_MAJOR_VERSION_V1
        || root.reserved() != 0
    {
        return Err(format!("host rejected {kind} root pre-callback data"));
    }
    match (kind, root.registrar().describe_contract()) {
        ("old", None) => Ok(()),
        ("new", Some(describe))
            if describe() == explorer_extension_api::ROOT_MODULE_CONTRACT_ID_V1 =>
        {
            Ok(())
        }
        _ => Err(format!(
            "{kind} registrar optional tail had the wrong compatibility shape"
        )),
    }
}

fn dump_layout_contract() {
    macro_rules! layout {
        ($type:ty) => {
            println!(
                "{} size={} align={}",
                stringify!($type),
                size_of::<$type>(),
                align_of::<$type>()
            );
        };
    }
    macro_rules! field {
        ($type:ty, $field:tt) => {
            println!(
                "{}::{}={}",
                stringify!($type),
                stringify!($field),
                offset_of!($type, $field)
            );
        };
    }
    layout!(JobHandleV1);
    layout!(ItemHandleV1);
    layout!(LocationHandleV1);
    layout!(SinkCapabilityV1);
    layout!(PluginValueKindV1);
    layout!(PluginValueV1);
    layout!(IncrementalResultEntryV1);
    layout!(IncrementalResultBatchV1);
    layout!(SinkSubmitStatusV1);
    layout!(SinkSubmitOutcomeV1);
    layout!(IncrementalResultSubmitV1);
    layout!(IncrementalResultSinkV1);
    layout!(JobProgressUpdateV1);
    layout!(JobProgressStatusV1);
    layout!(JobProgressSubmitV1);
    layout!(JobProgressSinkV1);
    layout!(JobControlStateV1);
    layout!(JobControlPollV1);
    layout!(JobContextV1);
    layout!(JobTerminalV1);
    layout!(JobProviderCallbackV1);
    field!(PluginValueV1, kind);
    field!(PluginValueV1, reserved);
    field!(PluginValueV1, integer);
    field!(PluginValueV1, float);
    field!(PluginValueV1, text);
    field!(PluginValueV1, payload);
    field!(PluginValueV1, opaque_schema);
    field!(PluginValueV1, opaque_schema_version);
    field!(PluginValueV1, reserved_tail);
    field!(IncrementalResultEntryV1, item);
    field!(IncrementalResultEntryV1, item_generation);
    field!(IncrementalResultEntryV1, source_generation);
    field!(IncrementalResultEntryV1, value);
    field!(IncrementalResultBatchV1, job);
    field!(IncrementalResultBatchV1, sink_capability);
    field!(IncrementalResultBatchV1, job_generation);
    field!(IncrementalResultBatchV1, location);
    field!(IncrementalResultBatchV1, location_generation);
    field!(IncrementalResultBatchV1, source_generation);
    field!(IncrementalResultBatchV1, sequence);
    field!(IncrementalResultBatchV1, entries);
    field!(SinkSubmitOutcomeV1, status);
    field!(SinkSubmitOutcomeV1, remaining_batch_credits);
    field!(SinkSubmitOutcomeV1, remaining_item_credits);
    field!(SinkSubmitOutcomeV1, remaining_byte_credits);
    field!(SinkSubmitOutcomeV1, rejected_batch);
    field!(IncrementalResultSinkV1, job);
    field!(IncrementalResultSinkV1, capability);
    field!(IncrementalResultSinkV1, submit);
    field!(JobProgressUpdateV1, job);
    field!(JobProgressUpdateV1, sink_capability);
    field!(JobProgressUpdateV1, job_generation);
    field!(JobProgressUpdateV1, item_generation);
    field!(JobProgressUpdateV1, location_generation);
    field!(JobProgressUpdateV1, source_generation);
    field!(JobProgressUpdateV1, sequence);
    field!(JobProgressUpdateV1, completed_units);
    field!(JobProgressUpdateV1, total_units);
    field!(JobProgressUpdateV1, reserved);
    field!(JobProgressSinkV1, job);
    field!(JobProgressSinkV1, capability);
    field!(JobProgressSinkV1, submit);
    field!(JobContextV1, job);
    field!(JobContextV1, item);
    field!(JobContextV1, location);
    field!(JobContextV1, feature_epoch);
    field!(JobContextV1, job_generation);
    field!(JobContextV1, item_generation);
    field!(JobContextV1, location_generation);
    field!(JobContextV1, source_generation);
    field!(JobContextV1, control_poll);
    field!(JobContextV1, sink);
    field!(JobContextV1, progress);
    println!(
        "codes control={},{},{},{} result={},{},{},{},{},{}",
        JobControlStateV1::ACTIVE.into_raw(),
        JobControlStateV1::CANCELLED.into_raw(),
        JobControlStateV1::DEADLINE_ELAPSED.into_raw(),
        JobControlStateV1::CLOSED.into_raw(),
        SinkSubmitStatusV1::ACCEPTED.into_raw(),
        SinkSubmitStatusV1::WOULD_BLOCK.into_raw(),
        SinkSubmitStatusV1::STALE.into_raw(),
        SinkSubmitStatusV1::CLOSED.into_raw(),
        SinkSubmitStatusV1::WRONG_THREAD.into_raw(),
        SinkSubmitStatusV1::INVALID.into_raw(),
    );
    println!(
        "codes progress={},{},{},{},{} terminal={},{},{},{},{},{},{},{},{}",
        JobProgressStatusV1::ACCEPTED.into_raw(),
        JobProgressStatusV1::STALE.into_raw(),
        JobProgressStatusV1::CLOSED.into_raw(),
        JobProgressStatusV1::WRONG_THREAD.into_raw(),
        JobProgressStatusV1::INVALID.into_raw(),
        JobTerminalV1::COMPLETED.into_raw(),
        JobTerminalV1::UNSUPPORTED.into_raw(),
        JobTerminalV1::UNAVAILABLE.into_raw(),
        JobTerminalV1::CANCELLED.into_raw(),
        JobTerminalV1::DEADLINE_ELAPSED.into_raw(),
        JobTerminalV1::BACKPRESSURED.into_raw(),
        JobTerminalV1::PLUGIN_ERROR.into_raw(),
        JobTerminalV1::INCOMPATIBLE.into_raw(),
        JobTerminalV1::PANICKED.into_raw(),
    );
    println!(
        "codes value={},{},{},{},{},{},{},{},{},{}",
        PluginValueKindV1::BOOL.into_raw(),
        PluginValueKindV1::I64.into_raw(),
        PluginValueKindV1::F64.into_raw(),
        PluginValueKindV1::BYTES.into_raw(),
        PluginValueKindV1::TIME_UNIX_NANOS.into_raw(),
        PluginValueKindV1::DURATION_NANOS.into_raw(),
        PluginValueKindV1::TEXT.into_raw(),
        PluginValueKindV1::LOCALIZED_TEXT.into_raw(),
        PluginValueKindV1::STRUCTURED.into_raw(),
        PluginValueKindV1::OPAQUE.into_raw(),
    );
}

/// Frozen x86_64 Windows ABI shape for the synchronous job v1 transport.
fn verify_layout_contract() -> Result<(), String> {
    let mut violations = Vec::new();
    macro_rules! layout {
        ($type:ty, $size:expr, $align:expr) => {
            if size_of::<$type>() != $size || align_of::<$type>() != $align {
                violations.push(format!(
                    "{} layout: expected {}/{}; got {}/{}",
                    stringify!($type),
                    $size,
                    $align,
                    size_of::<$type>(),
                    align_of::<$type>()
                ));
            }
        };
    }
    macro_rules! field {
        ($type:ty, $field:tt, $expected:expr) => {
            if offset_of!($type, $field) != $expected {
                violations.push(format!(
                    "{}::{} offset: expected {}; got {}",
                    stringify!($type),
                    stringify!($field),
                    $expected,
                    offset_of!($type, $field)
                ));
            }
        };
    }
    macro_rules! code {
        ($name:expr, $actual:expr, $expected:expr) => {
            if ($actual).into_raw() != $expected {
                violations.push(format!(
                    "{}: expected {}; got {}",
                    $name,
                    $expected,
                    ($actual).into_raw()
                ));
            }
        };
    }
    layout!(JobHandleV1, 24, 8);
    layout!(ItemHandleV1, 24, 8);
    layout!(LocationHandleV1, 24, 8);
    layout!(SinkCapabilityV1, 16, 1);
    layout!(PluginValueKindV1, 4, 4);
    layout!(PluginValueV1, 112, 8);
    layout!(IncrementalResultEntryV1, 152, 8);
    layout!(IncrementalResultBatchV1, 128, 8);
    layout!(SinkSubmitStatusV1, 4, 4);
    layout!(SinkSubmitOutcomeV1, 160, 8);
    layout!(IncrementalResultSubmitV1, 8, 8);
    layout!(IncrementalResultSinkV1, 48, 8);
    layout!(JobProgressUpdateV1, 104, 8);
    layout!(JobProgressStatusV1, 4, 4);
    layout!(JobProgressSubmitV1, 8, 8);
    layout!(JobProgressSinkV1, 48, 8);
    layout!(JobControlStateV1, 4, 4);
    layout!(JobControlPollV1, 8, 8);
    layout!(JobContextV1, 224, 8);
    layout!(JobTerminalV1, 4, 4);
    layout!(JobProviderCallbackV1, 8, 8);
    field!(PluginValueV1, kind, 0);
    field!(PluginValueV1, reserved, 4);
    field!(PluginValueV1, integer, 8);
    field!(PluginValueV1, float, 16);
    field!(PluginValueV1, text, 24);
    field!(PluginValueV1, payload, 56);
    field!(PluginValueV1, opaque_schema, 88);
    field!(PluginValueV1, opaque_schema_version, 104);
    field!(PluginValueV1, reserved_tail, 108);
    field!(IncrementalResultEntryV1, item, 0);
    field!(IncrementalResultEntryV1, item_generation, 24);
    field!(IncrementalResultEntryV1, source_generation, 32);
    field!(IncrementalResultEntryV1, value, 40);
    field!(IncrementalResultBatchV1, job, 0);
    field!(IncrementalResultBatchV1, sink_capability, 24);
    field!(IncrementalResultBatchV1, job_generation, 40);
    field!(IncrementalResultBatchV1, location, 48);
    field!(IncrementalResultBatchV1, location_generation, 72);
    field!(IncrementalResultBatchV1, source_generation, 80);
    field!(IncrementalResultBatchV1, sequence, 88);
    field!(IncrementalResultBatchV1, entries, 96);
    field!(SinkSubmitOutcomeV1, status, 0);
    field!(SinkSubmitOutcomeV1, remaining_batch_credits, 4);
    field!(SinkSubmitOutcomeV1, remaining_item_credits, 8);
    field!(SinkSubmitOutcomeV1, remaining_byte_credits, 16);
    field!(SinkSubmitOutcomeV1, rejected_batch, 24);
    field!(IncrementalResultSinkV1, job, 0);
    field!(IncrementalResultSinkV1, capability, 24);
    field!(IncrementalResultSinkV1, submit, 40);
    field!(JobProgressUpdateV1, job, 0);
    field!(JobProgressUpdateV1, sink_capability, 24);
    field!(JobProgressUpdateV1, job_generation, 40);
    field!(JobProgressUpdateV1, item_generation, 48);
    field!(JobProgressUpdateV1, location_generation, 56);
    field!(JobProgressUpdateV1, source_generation, 64);
    field!(JobProgressUpdateV1, sequence, 72);
    field!(JobProgressUpdateV1, completed_units, 80);
    field!(JobProgressUpdateV1, total_units, 88);
    field!(JobProgressUpdateV1, reserved, 96);
    field!(JobProgressSinkV1, job, 0);
    field!(JobProgressSinkV1, capability, 24);
    field!(JobProgressSinkV1, submit, 40);
    field!(JobContextV1, job, 0);
    field!(JobContextV1, item, 24);
    field!(JobContextV1, location, 56);
    field!(JobContextV1, feature_epoch, 80);
    field!(JobContextV1, job_generation, 88);
    field!(JobContextV1, item_generation, 96);
    field!(JobContextV1, location_generation, 104);
    field!(JobContextV1, source_generation, 112);
    field!(JobContextV1, control_poll, 120);
    field!(JobContextV1, sink, 128);
    field!(JobContextV1, progress, 176);
    code!("JobControlStateV1::ACTIVE", JobControlStateV1::ACTIVE, 1);
    code!(
        "JobControlStateV1::CANCELLED",
        JobControlStateV1::CANCELLED,
        2
    );
    code!(
        "JobControlStateV1::DEADLINE_ELAPSED",
        JobControlStateV1::DEADLINE_ELAPSED,
        3
    );
    code!("JobControlStateV1::CLOSED", JobControlStateV1::CLOSED, 4);
    code!(
        "SinkSubmitStatusV1::ACCEPTED",
        SinkSubmitStatusV1::ACCEPTED,
        1
    );
    code!(
        "SinkSubmitStatusV1::WOULD_BLOCK",
        SinkSubmitStatusV1::WOULD_BLOCK,
        2
    );
    code!("SinkSubmitStatusV1::STALE", SinkSubmitStatusV1::STALE, 3);
    code!("SinkSubmitStatusV1::CLOSED", SinkSubmitStatusV1::CLOSED, 4);
    code!(
        "SinkSubmitStatusV1::WRONG_THREAD",
        SinkSubmitStatusV1::WRONG_THREAD,
        5
    );
    code!(
        "SinkSubmitStatusV1::INVALID",
        SinkSubmitStatusV1::INVALID,
        6
    );
    code!(
        "JobProgressStatusV1::ACCEPTED",
        JobProgressStatusV1::ACCEPTED,
        1
    );
    code!("JobProgressStatusV1::STALE", JobProgressStatusV1::STALE, 2);
    code!(
        "JobProgressStatusV1::CLOSED",
        JobProgressStatusV1::CLOSED,
        3
    );
    code!(
        "JobProgressStatusV1::WRONG_THREAD",
        JobProgressStatusV1::WRONG_THREAD,
        4
    );
    code!(
        "JobProgressStatusV1::INVALID",
        JobProgressStatusV1::INVALID,
        5
    );
    code!("JobTerminalV1::COMPLETED", JobTerminalV1::COMPLETED, 1);
    code!("JobTerminalV1::UNSUPPORTED", JobTerminalV1::UNSUPPORTED, 2);
    code!("JobTerminalV1::UNAVAILABLE", JobTerminalV1::UNAVAILABLE, 3);
    code!("JobTerminalV1::CANCELLED", JobTerminalV1::CANCELLED, 4);
    code!(
        "JobTerminalV1::DEADLINE_ELAPSED",
        JobTerminalV1::DEADLINE_ELAPSED,
        5
    );
    code!(
        "JobTerminalV1::BACKPRESSURED",
        JobTerminalV1::BACKPRESSURED,
        6
    );
    code!(
        "JobTerminalV1::PLUGIN_ERROR",
        JobTerminalV1::PLUGIN_ERROR,
        7
    );
    code!(
        "JobTerminalV1::INCOMPATIBLE",
        JobTerminalV1::INCOMPATIBLE,
        8
    );
    code!("JobTerminalV1::PANICKED", JobTerminalV1::PANICKED, 9);
    code!("PluginValueKindV1::BOOL", PluginValueKindV1::BOOL, 1);
    code!("PluginValueKindV1::I64", PluginValueKindV1::I64, 2);
    code!("PluginValueKindV1::F64", PluginValueKindV1::F64, 3);
    code!("PluginValueKindV1::BYTES", PluginValueKindV1::BYTES, 4);
    code!(
        "PluginValueKindV1::TIME_UNIX_NANOS",
        PluginValueKindV1::TIME_UNIX_NANOS,
        5
    );
    code!(
        "PluginValueKindV1::DURATION_NANOS",
        PluginValueKindV1::DURATION_NANOS,
        6
    );
    code!("PluginValueKindV1::TEXT", PluginValueKindV1::TEXT, 7);
    code!(
        "PluginValueKindV1::LOCALIZED_TEXT",
        PluginValueKindV1::LOCALIZED_TEXT,
        8
    );
    code!(
        "PluginValueKindV1::STRUCTURED",
        PluginValueKindV1::STRUCTURED,
        9
    );
    code!("PluginValueKindV1::OPAQUE", PluginValueKindV1::OPAQUE, 10);
    violations
        .is_empty()
        .then_some(())
        .ok_or_else(|| violations.join("; "))
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
        "old" | "new" => {
            let plugin = arguments.next().ok_or("missing plugin path")?;
            if arguments.next().is_some() {
                return Err("too many arguments".to_owned());
            }
            verify_root(&mode, Path::new(&plugin))
        }
        _ => Err("mode must be transport, old, or new".to_owned()),
    }
}
