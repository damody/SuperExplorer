//! Host-owned capability registry and bounded result transport for extension jobs.
//!
//! Provider registration is intentionally separate: task 4.2 makes the ABI and
//! runtime seam safe before a registrar tail starts exposing provider callbacks.

use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex, OnceLock, Weak},
    thread::ThreadId,
};

use abi_stable::std_types::ROption;
#[cfg(test)]
use explorer_extension_api::JobProviderCallbackV1;
use explorer_extension_api::{
    IncrementalResultBatchV1, IncrementalResultEntryV1, IncrementalResultSinkV1,
    IncrementalResultSubmitV1, ItemHandleV1, JobContextV1, JobControlPollV1, JobControlStateV1,
    JobHandleV1, JobProgressSinkV1, JobProgressStatusV1, JobProgressSubmitV1, JobProgressUpdateV1,
    JobTerminalV1, LocationHandleV1, MAX_INCREMENTAL_RESULT_BYTES_V1,
    MAX_INCREMENTAL_RESULT_ITEMS_V1, SinkCapabilityV1, SinkSubmitOutcomeV1, SinkSubmitStatusV1,
    StableIdV1,
};
use ring::rand::{SecureRandom as _, SystemRandom};
use thiserror::Error;

/// Bounded buffer configuration for accepted extension result batches.
#[allow(clippy::struct_field_names)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExtensionResultBufferConfigV1 {
    max_active_jobs: usize,
    max_active_jobs_per_package: usize,
    max_batches: usize,
    max_batches_per_package: usize,
    max_batches_per_job: usize,
    max_items: usize,
    max_items_per_package: usize,
    max_items_per_job: usize,
    max_bytes: usize,
    max_bytes_per_package: usize,
    max_bytes_per_job: usize,
}

#[allow(clippy::missing_errors_doc)]
impl ExtensionResultBufferConfigV1 {
    /// Validates global, per-package, and per-job queue credits before allocation.
    pub fn try_new(
        max_active_jobs: usize,
        max_active_jobs_per_package: usize,
        max_batches: usize,
        max_batches_per_package: usize,
        max_batches_per_job: usize,
        max_items: usize,
        max_items_per_package: usize,
        max_items_per_job: usize,
        max_bytes: usize,
        max_bytes_per_package: usize,
        max_bytes_per_job: usize,
    ) -> Result<Self, ExtensionJobRuntimeErrorV1> {
        let values = [
            max_active_jobs,
            max_active_jobs_per_package,
            max_batches,
            max_batches_per_package,
            max_batches_per_job,
            max_items,
            max_items_per_package,
            max_items_per_job,
            max_bytes,
            max_bytes_per_package,
            max_bytes_per_job,
        ];
        if values.into_iter().any(|value| value == 0) {
            return Err(ExtensionJobRuntimeErrorV1::InvalidBufferConfig);
        }
        if max_active_jobs_per_package > max_active_jobs
            || max_batches_per_package > max_batches
            || max_batches_per_job > max_batches_per_package
            || max_items_per_package > max_items
            || max_items_per_job > max_items
            || max_items_per_job > max_items_per_package
            || max_bytes_per_package > max_bytes
            || max_bytes_per_job > max_bytes
            || max_bytes_per_job > max_bytes_per_package
            || max_items
                > MAX_INCREMENTAL_RESULT_ITEMS_V1
                    .checked_mul(max_batches)
                    .ok_or(ExtensionJobRuntimeErrorV1::InvalidBufferConfig)?
            || max_bytes
                > MAX_INCREMENTAL_RESULT_BYTES_V1
                    .checked_mul(max_batches)
                    .ok_or(ExtensionJobRuntimeErrorV1::InvalidBufferConfig)?
        {
            return Err(ExtensionJobRuntimeErrorV1::InvalidBufferConfig);
        }
        Ok(Self {
            max_active_jobs,
            max_active_jobs_per_package,
            max_batches,
            max_batches_per_package,
            max_batches_per_job,
            max_items,
            max_items_per_package,
            max_items_per_job,
            max_bytes,
            max_bytes_per_package,
            max_bytes_per_job,
        })
    }
}

/// Host-trusted producer identity attached after a sink accepts copied data.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExtensionJobProducerV1 {
    pub package_id: String,
    pub interface_id: StableIdV1,
    pub feature_id: String,
    pub feature_epoch: u64,
}

/// Input supplied only by validated host registration code.
#[derive(Clone, Debug)]
pub struct ExtensionJobRuntimeRequestV1 {
    pub producer: ExtensionJobProducerV1,
    pub job_generation: u64,
    pub item_generation: u64,
    pub location_generation: u64,
    pub source_generation: u64,
    pub has_item: bool,
}

/// Deep-copied host result. Producer identity never originates from plugin bytes.
#[derive(Clone, Debug)]
pub struct AcceptedIncrementalResultBatchV1 {
    pub producer: ExtensionJobProducerV1,
    pub job: JobHandleV1,
    pub sequence: u64,
    pub job_generation: u64,
    pub location_generation: u64,
    pub source_generation: u64,
    pub entries: Vec<IncrementalResultEntryV1>,
}

/// Exactly-once terminal publication outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExtensionJobFinishOutcomeV1 {
    Published(JobTerminalV1),
    AlreadyTerminal(JobTerminalV1),
    UnknownJob,
}

/// Host-owned extension result runtime.
#[derive(Debug)]
pub struct ExtensionJobRuntimeV1 {
    state: Arc<Mutex<RuntimeStateV1>>,
}

#[derive(Debug)]
struct RuntimeStateV1 {
    config: ExtensionResultBufferConfigV1,
    jobs: HashMap<JobHandleV1, RuntimeJobV1>,
    queued_batches: usize,
    queued_items: usize,
    queued_bytes: usize,
    queued_per_package: HashMap<String, QueueUsageV1>,
    active_jobs_per_package: HashMap<String, usize>,
    accounting_healthy: bool,
    #[cfg(test)]
    fail_next_registry_publish: bool,
    #[cfg(test)]
    panic_next_progress_submit: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct QueueUsageV1 {
    batches: usize,
    items: usize,
    bytes: usize,
}

#[derive(Debug)]
struct RuntimeJobV1 {
    producer: ExtensionJobProducerV1,
    owner_thread: ThreadId,
    item: Option<ItemHandleV1>,
    location: LocationHandleV1,
    job_generation: u64,
    item_generation: u64,
    location_generation: u64,
    source_generation: u64,
    sink_capability: SinkCapabilityV1,
    provider_call_active: bool,
    next_sequence: u64,
    next_progress_sequence: u64,
    pending_progress: Option<JobProgressUpdateV1>,
    terminal: Option<JobTerminalV1>,
    protocol_faulted: bool,
    control: JobControlStateV1,
    queued_batches: VecDeque<StoredBatchV1>,
    queued_items: usize,
    queued_bytes: usize,
}

#[derive(Debug)]
struct StoredBatchV1 {
    sequence: u64,
    job_generation: u64,
    item_generation: u64,
    location_generation: u64,
    source_generation: u64,
    bytes: usize,
    entries: Vec<IncrementalResultEntryV1>,
}

type RuntimeRegistryV1 = HashMap<JobHandleV1, Weak<Mutex<RuntimeStateV1>>>;
static RUNTIMES_V1: OnceLock<Mutex<RuntimeRegistryV1>> = OnceLock::new();
thread_local! {
    static ACTIVE_INVOCATION_V1: RefCell<Option<(JobHandleV1, SinkCapabilityV1)>> = const { RefCell::new(None) };
}

fn runtimes() -> &'static Mutex<RuntimeRegistryV1> {
    RUNTIMES_V1.get_or_init(|| Mutex::new(HashMap::new()))
}

fn invocation_matches(job: JobHandleV1, capability: SinkCapabilityV1) -> bool {
    ACTIVE_INVOCATION_V1.with(|active| *active.borrow() == Some((job, capability)))
}

fn invocation_is_current_job(job: JobHandleV1) -> bool {
    ACTIVE_INVOCATION_V1.with(|active| {
        active
            .borrow()
            .is_some_and(|(active_job, _)| active_job == job)
    })
}

#[allow(clippy::missing_errors_doc)]
impl ExtensionJobRuntimeV1 {
    /// Creates an empty runtime result registry.
    #[must_use]
    pub fn new(config: ExtensionResultBufferConfigV1) -> Self {
        Self {
            state: Arc::new(Mutex::new(RuntimeStateV1 {
                config,
                jobs: HashMap::new(),
                queued_batches: 0,
                queued_items: 0,
                queued_bytes: 0,
                queued_per_package: HashMap::new(),
                active_jobs_per_package: HashMap::new(),
                accounting_healthy: true,
                #[cfg(test)]
                fail_next_registry_publish: false,
                #[cfg(test)]
                panic_next_progress_submit: false,
            })),
        }
    }

    /// Mints a capability-bound ABI context on the worker that will invoke it.
    pub fn open_job(
        &self,
        request: ExtensionJobRuntimeRequestV1,
    ) -> Result<JobContextV1, ExtensionJobRuntimeErrorV1> {
        if request.job_generation == 0
            || request.item_generation == 0
            || request.location_generation == 0
            || request.source_generation == 0
            || request.producer.feature_epoch == 0
            || request.producer.package_id.is_empty()
            || request.producer.feature_id.is_empty()
            || !request.producer.interface_id.is_valid()
        {
            return Err(ExtensionJobRuntimeErrorV1::InvalidRequest);
        }
        let job = self.mint_handle(request.job_generation)?;
        let item = request
            .has_item
            .then(|| Self::mint_item_handle(request.item_generation))
            .transpose()?;
        let location = Self::mint_location_handle(request.location_generation)?;
        let sink_capability = SinkCapabilityV1::from_host(random_nonce()?);
        let feature_epoch = request.producer.feature_epoch;
        let package_id = request.producer.package_id.clone();
        let job_state = RuntimeJobV1 {
            producer: request.producer,
            owner_thread: std::thread::current().id(),
            item,
            location,
            job_generation: request.job_generation,
            item_generation: request.item_generation,
            location_generation: request.location_generation,
            source_generation: request.source_generation,
            sink_capability,
            provider_call_active: false,
            next_sequence: 0,
            next_progress_sequence: 0,
            pending_progress: None,
            terminal: None,
            protocol_faulted: false,
            control: JobControlStateV1::ACTIVE,
            queued_batches: VecDeque::new(),
            queued_items: 0,
            queued_bytes: 0,
        };
        let mut state = self
            .state
            .lock()
            .map_err(|_| ExtensionJobRuntimeErrorV1::StatePoisoned)?;
        let active_for_package = state
            .active_jobs_per_package
            .get(&package_id)
            .copied()
            .unwrap_or(0);
        if state.jobs.len() >= state.config.max_active_jobs
            || active_for_package >= state.config.max_active_jobs_per_package
        {
            return Err(ExtensionJobRuntimeErrorV1::ActiveJobLimitExceeded);
        }
        state.jobs.insert(job, job_state);
        let active = state
            .active_jobs_per_package
            .entry(package_id.clone())
            .or_default()
            .checked_add(1)
            .ok_or(ExtensionJobRuntimeErrorV1::ActiveJobLimitExceeded)?;
        state.active_jobs_per_package.insert(package_id, active);
        drop(state);
        if let Err(error) = publish_runtime(job, &self.state) {
            rollback_open_job(&self.state, job);
            return Err(error);
        }
        Ok(JobContextV1 {
            job,
            item: item.map_or(ROption::RNone, ROption::RSome),
            location,
            feature_epoch,
            job_generation: request.job_generation,
            item_generation: request.item_generation,
            location_generation: request.location_generation,
            source_generation: request.source_generation,
            control_poll: JobControlPollV1::from_host(poll_control),
            sink: IncrementalResultSinkV1 {
                job,
                capability: sink_capability,
                submit: IncrementalResultSubmitV1::from_host(submit_batch),
            },
            progress: JobProgressSinkV1 {
                job,
                capability: sink_capability,
                submit: JobProgressSubmitV1::from_host(submit_progress),
            },
        })
    }

    /// Internal test seam for the future lifecycle-guarded provider dispatcher.
    ///
    /// This is deliberately crate-private: production native invocation must
    /// enter through `PluginCallGuardV1` so its durable Safe Mode marker and
    /// timing record cover the callback.
    #[cfg(test)]
    pub(crate) fn invoke_provider_for_test(
        &self,
        context: JobContextV1,
        provider: JobProviderCallbackV1,
    ) -> Result<ExtensionJobFinishOutcomeV1, ExtensionJobRuntimeErrorV1> {
        let scope = ProviderCallScopeV1::enter(&self.state, context)?;
        let terminal = provider.invoke(context);
        drop(scope);
        Ok(self.finish(
            context.job,
            if terminal.is_known() {
                terminal
            } else {
                JobTerminalV1::INCOMPATIBLE
            },
        ))
    }

    /// Publishes exactly one terminal. Cancellation/deadline host state wins over a plugin claim.
    pub fn finish(&self, job: JobHandleV1, reported: JobTerminalV1) -> ExtensionJobFinishOutcomeV1 {
        let Ok(mut state) = self.state.lock() else {
            return ExtensionJobFinishOutcomeV1::UnknownJob;
        };
        let Some(runtime_job) = state.jobs.get_mut(&job) else {
            return ExtensionJobFinishOutcomeV1::UnknownJob;
        };
        if let Some(terminal) = runtime_job.terminal {
            return ExtensionJobFinishOutcomeV1::AlreadyTerminal(terminal);
        }
        let terminal = terminal_precedence(
            runtime_job.control,
            if reported.is_known() {
                reported
            } else {
                JobTerminalV1::INCOMPATIBLE
            },
            runtime_job.protocol_faulted,
        );
        runtime_job.terminal = Some(terminal);
        runtime_job.control = JobControlStateV1::CLOSED;
        runtime_job.pending_progress = None;
        ExtensionJobFinishOutcomeV1::Published(terminal)
    }

    /// Changes cooperative state without invoking extension code or callbacks.
    pub fn request_control(
        &self,
        job: JobHandleV1,
        control: JobControlStateV1,
    ) -> Result<(), ExtensionJobRuntimeErrorV1> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| ExtensionJobRuntimeErrorV1::StatePoisoned)?;
        let runtime_job = state
            .jobs
            .get_mut(&job)
            .ok_or(ExtensionJobRuntimeErrorV1::UnknownJob)?;
        if runtime_job.terminal.is_none() {
            runtime_job.control = monotonic_control(runtime_job.control, control)?;
            if runtime_job.control.into_raw() != JobControlStateV1::ACTIVE.into_raw() {
                runtime_job.pending_progress = None;
            }
        }
        Ok(())
    }

    /// Advances the host's current location/source view for this job. Queued
    /// batches from an older view are discarded by [`Self::drain`].
    pub fn update_current_generations(
        &self,
        job: JobHandleV1,
        item_generation: u64,
        location_generation: u64,
        source_generation: u64,
    ) -> Result<(), ExtensionJobRuntimeErrorV1> {
        if item_generation == 0 || location_generation == 0 || source_generation == 0 {
            return Err(ExtensionJobRuntimeErrorV1::InvalidRequest);
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| ExtensionJobRuntimeErrorV1::StatePoisoned)?;
        let runtime_job = state
            .jobs
            .get_mut(&job)
            .ok_or(ExtensionJobRuntimeErrorV1::UnknownJob)?;
        if item_generation < runtime_job.item_generation
            || location_generation < runtime_job.location_generation
            || source_generation < runtime_job.source_generation
        {
            return Err(ExtensionJobRuntimeErrorV1::GenerationRegression);
        }
        if item_generation == runtime_job.item_generation
            && location_generation == runtime_job.location_generation
            && source_generation == runtime_job.source_generation
        {
            return Err(ExtensionJobRuntimeErrorV1::GenerationUnchanged);
        }
        runtime_job.item_generation = item_generation;
        runtime_job.location_generation = location_generation;
        runtime_job.source_generation = source_generation;
        runtime_job.pending_progress = None;
        Ok(())
    }

    /// Takes the latest valid progress update without retaining an unbounded history.
    #[must_use]
    pub fn take_progress(
        &self,
        job: JobHandleV1,
        current_job_generation: u64,
        current_item_generation: u64,
        current_location_generation: u64,
        current_source_generation: u64,
    ) -> Option<JobProgressUpdateV1> {
        let Ok(mut state) = self.state.lock() else {
            return None;
        };
        let runtime_job = state.jobs.get_mut(&job)?;
        if runtime_job.terminal.is_some()
            || runtime_job.control.into_raw() != JobControlStateV1::ACTIVE.into_raw()
            || current_job_generation != job.generation()
            || current_job_generation != runtime_job.job_generation
            || current_item_generation != runtime_job.item_generation
            || current_location_generation != runtime_job.location_generation
            || current_source_generation != runtime_job.source_generation
        {
            runtime_job.pending_progress = None;
            return None;
        }
        runtime_job.pending_progress.take()
    }

    /// Applies only batches matching the caller's current generations, and
    /// releases every associated credit exactly once (including stale batches).
    pub fn drain(
        &self,
        job: JobHandleV1,
        current_item_generation: u64,
        current_location_generation: u64,
        current_source_generation: u64,
        maximum_batches: usize,
    ) -> Vec<AcceptedIncrementalResultBatchV1> {
        let Ok(mut state) = self.state.lock() else {
            return Vec::new();
        };
        let (drained, package_id, released, local_accounting_ok) = {
            let Some(runtime_job) = state.jobs.get_mut(&job) else {
                return Vec::new();
            };
            let producer = runtime_job.producer.clone();
            let mut drained = Vec::new();
            let mut released = QueueUsageV1::default();
            let mut local_accounting_ok = true;
            for _ in 0..maximum_batches {
                let Some(batch) = runtime_job.queued_batches.pop_front() else {
                    break;
                };
                let items = batch.entries.len();
                let (Some(queued_items), Some(queued_bytes)) = (
                    runtime_job.queued_items.checked_sub(items),
                    runtime_job.queued_bytes.checked_sub(batch.bytes),
                ) else {
                    local_accounting_ok = false;
                    break;
                };
                runtime_job.queued_items = queued_items;
                runtime_job.queued_bytes = queued_bytes;
                if batch.job_generation == runtime_job.job_generation
                    && batch.item_generation == current_item_generation
                    && batch.item_generation == runtime_job.item_generation
                    && batch.location_generation == current_location_generation
                    && batch.location_generation == runtime_job.location_generation
                    && batch.source_generation == current_source_generation
                    && batch.source_generation == runtime_job.source_generation
                {
                    drained.push(AcceptedIncrementalResultBatchV1 {
                        producer: producer.clone(),
                        job,
                        sequence: batch.sequence,
                        job_generation: batch.job_generation,
                        location_generation: batch.location_generation,
                        source_generation: batch.source_generation,
                        entries: batch.entries,
                    });
                }
                // Queue totals are bounded by the validated config, so these
                // sums cannot overflow while draining a stored queue.
                released.batches += 1;
                released.items += items;
                released.bytes += batch.bytes;
            }
            (drained, producer.package_id, released, local_accounting_ok)
        };
        if !local_accounting_ok {
            state.accounting_healthy = false;
            return Vec::new();
        }
        release_credits(&mut state, &package_id, released);
        drained
    }

    /// Purges queued result data and releases credits when a generation is stale.
    pub fn purge(&self, job: JobHandleV1) -> Result<(), ExtensionJobRuntimeErrorV1> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| ExtensionJobRuntimeErrorV1::StatePoisoned)?;
        let Some(runtime_job) = state.jobs.get_mut(&job) else {
            return Err(ExtensionJobRuntimeErrorV1::UnknownJob);
        };
        let released = QueueUsageV1 {
            batches: runtime_job.queued_batches.len(),
            items: runtime_job.queued_items,
            bytes: runtime_job.queued_bytes,
        };
        let package_id = runtime_job.producer.package_id.clone();
        runtime_job.queued_batches.clear();
        runtime_job.queued_items = 0;
        runtime_job.queued_bytes = 0;
        release_credits(&mut state, &package_id, released);
        Ok(())
    }

    /// Retires a terminal job, releasing queued credits and its registry slot.
    /// Any retained sink immediately becomes closed because registry removal
    /// occurs before the runtime state is destroyed.
    pub fn retire(&self, job: JobHandleV1) -> Result<(), ExtensionJobRuntimeErrorV1> {
        {
            let state = self
                .state
                .lock()
                .map_err(|_| ExtensionJobRuntimeErrorV1::StatePoisoned)?;
            let runtime_job = state
                .jobs
                .get(&job)
                .ok_or(ExtensionJobRuntimeErrorV1::UnknownJob)?;
            if runtime_job.provider_call_active || runtime_job.terminal.is_none() {
                return Err(ExtensionJobRuntimeErrorV1::RetireRequiresTerminal);
            }
        }
        let mut registry = runtimes()
            .lock()
            .map_err(|_| ExtensionJobRuntimeErrorV1::StatePoisoned)?;
        registry.remove(&job);
        drop(registry);

        let mut state = self
            .state
            .lock()
            .map_err(|_| ExtensionJobRuntimeErrorV1::StatePoisoned)?;
        let Some(runtime_job) = state.jobs.remove(&job) else {
            return Err(ExtensionJobRuntimeErrorV1::UnknownJob);
        };
        let released = QueueUsageV1 {
            batches: runtime_job.queued_batches.len(),
            items: runtime_job.queued_items,
            bytes: runtime_job.queued_bytes,
        };
        let package_id = runtime_job.producer.package_id;
        release_credits(&mut state, &package_id, released);
        release_active_job(&mut state, &package_id);
        Ok(())
    }

    fn mint_handle(&self, generation: u64) -> Result<JobHandleV1, ExtensionJobRuntimeErrorV1> {
        for _ in 0..16 {
            let handle = JobHandleV1::from_host(random_nonce()?, generation);
            let state = self
                .state
                .lock()
                .map_err(|_| ExtensionJobRuntimeErrorV1::StatePoisoned)?;
            if !state.jobs.contains_key(&handle) {
                return Ok(handle);
            }
        }
        Err(ExtensionJobRuntimeErrorV1::CapabilityCollision)
    }

    fn mint_item_handle(generation: u64) -> Result<ItemHandleV1, ExtensionJobRuntimeErrorV1> {
        Ok(ItemHandleV1::from_host(random_nonce()?, generation))
    }

    fn mint_location_handle(
        generation: u64,
    ) -> Result<LocationHandleV1, ExtensionJobRuntimeErrorV1> {
        Ok(LocationHandleV1::from_host(random_nonce()?, generation))
    }
}

/// Sets and clears the per-callback sink authorization without retaining a
/// mutable runtime lock during plugin execution.
#[cfg(test)]
struct ProviderCallScopeV1 {
    state: Arc<Mutex<RuntimeStateV1>>,
    job: JobHandleV1,
    capability: SinkCapabilityV1,
}

#[cfg(test)]
impl ProviderCallScopeV1 {
    fn enter(
        state: &Arc<Mutex<RuntimeStateV1>>,
        context: JobContextV1,
    ) -> Result<Self, ExtensionJobRuntimeErrorV1> {
        if ACTIVE_INVOCATION_V1.with(|active| active.borrow().is_some()) {
            return Err(ExtensionJobRuntimeErrorV1::InactiveProviderCall);
        }
        let mut locked = state
            .lock()
            .map_err(|_| ExtensionJobRuntimeErrorV1::StatePoisoned)?;
        let job = locked
            .jobs
            .get_mut(&context.job)
            .ok_or(ExtensionJobRuntimeErrorV1::UnknownJob)?;
        let item_matches = context.item.into_option() == job.item
            && context.item_generation == job.item_generation;
        if context.sink.job != context.job
            || context.sink.capability != job.sink_capability
            || context.progress.job != context.job
            || context.progress.capability != job.sink_capability
            || std::thread::current().id() != job.owner_thread
            || !item_matches
            || context.job_generation != job.job_generation
            || context.location != job.location
            || context.location_generation != job.location_generation
            || context.source_generation != job.source_generation
            || context.feature_epoch != job.producer.feature_epoch
            || job.provider_call_active
            || job.terminal.is_some()
            || job.control.into_raw() != JobControlStateV1::ACTIVE.into_raw()
        {
            return Err(ExtensionJobRuntimeErrorV1::InactiveProviderCall);
        }
        job.provider_call_active = true;
        ACTIVE_INVOCATION_V1.with(|active| {
            *active.borrow_mut() = Some((context.job, context.sink.capability));
        });
        Ok(Self {
            state: Arc::clone(state),
            job: context.job,
            capability: context.sink.capability,
        })
    }
}

#[cfg(test)]
impl Drop for ProviderCallScopeV1 {
    fn drop(&mut self) {
        ACTIVE_INVOCATION_V1.with(|active| {
            if *active.borrow() == Some((self.job, self.capability)) {
                *active.borrow_mut() = None;
            }
        });
        if let Ok(mut state) = self.state.lock()
            && let Some(job) = state.jobs.get_mut(&self.job)
            && job.sink_capability == self.capability
        {
            job.provider_call_active = false;
        }
    }
}

impl Drop for ExtensionJobRuntimeV1 {
    fn drop(&mut self) {
        let Ok(state) = self.state.lock() else {
            return;
        };
        let jobs = state.jobs.keys().copied().collect::<Vec<_>>();
        drop(state);
        let Ok(mut registry) = runtimes().lock() else {
            return;
        };
        for job in jobs {
            registry.remove(&job);
        }
        registry.retain(|_, runtime| runtime.strong_count() != 0);
    }
}

fn publish_runtime(
    job: JobHandleV1,
    state: &Arc<Mutex<RuntimeStateV1>>,
) -> Result<(), ExtensionJobRuntimeErrorV1> {
    #[cfg(test)]
    {
        let mut runtime_state = state
            .lock()
            .map_err(|_| ExtensionJobRuntimeErrorV1::StatePoisoned)?;
        if runtime_state.fail_next_registry_publish {
            runtime_state.fail_next_registry_publish = false;
            return Err(ExtensionJobRuntimeErrorV1::RegistryPublishFailed);
        }
    }
    let mut registry = runtimes()
        .lock()
        .map_err(|_| ExtensionJobRuntimeErrorV1::StatePoisoned)?;
    if registry.contains_key(&job) {
        return Err(ExtensionJobRuntimeErrorV1::CapabilityCollision);
    }
    registry.insert(job, Arc::downgrade(state));
    Ok(())
}

fn rollback_open_job(state: &Arc<Mutex<RuntimeStateV1>>, job: JobHandleV1) {
    let Ok(mut state) = state.lock() else {
        return;
    };
    let Some(runtime_job) = state.jobs.remove(&job) else {
        return;
    };
    release_active_job(&mut state, &runtime_job.producer.package_id);
}

fn release_active_job(state: &mut RuntimeStateV1, package_id: &str) {
    let Some(active) = state.active_jobs_per_package.get_mut(package_id) else {
        state.accounting_healthy = false;
        return;
    };
    let Some(next) = active.checked_sub(1) else {
        state.accounting_healthy = false;
        return;
    };
    if next == 0 {
        state.active_jobs_per_package.remove(package_id);
    } else {
        *active = next;
    }
}

fn random_nonce() -> Result<[u8; 16], ExtensionJobRuntimeErrorV1> {
    let mut nonce = [0_u8; 16];
    SystemRandom::new()
        .fill(&mut nonce)
        .map_err(|_| ExtensionJobRuntimeErrorV1::EntropyUnavailable)?;
    Ok(nonce)
}

extern "C" fn poll_control(job: JobHandleV1) -> JobControlStateV1 {
    std::panic::catch_unwind(|| poll_control_inner(job)).unwrap_or(JobControlStateV1::CLOSED)
}

fn poll_control_inner(job: JobHandleV1) -> JobControlStateV1 {
    if !invocation_is_current_job(job) {
        return JobControlStateV1::CLOSED;
    }
    let Ok(registry) = runtimes().lock() else {
        return JobControlStateV1::CLOSED;
    };
    let Some(runtime) = registry.get(&job).and_then(Weak::upgrade) else {
        return JobControlStateV1::CLOSED;
    };
    drop(registry);
    let Ok(state) = runtime.lock() else {
        return JobControlStateV1::CLOSED;
    };
    state
        .jobs
        .get(&job)
        .map_or(JobControlStateV1::CLOSED, |job| job.control)
}

extern "C" fn submit_batch(batch: IncrementalResultBatchV1) -> SinkSubmitOutcomeV1 {
    let Ok(backup) = std::panic::catch_unwind(|| batch.clone()) else {
        return rejected(SinkSubmitStatusV1::CLOSED, batch, 0, 0, 0);
    };
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| submit_batch_inner(batch)))
        .unwrap_or_else(|_| rejected(SinkSubmitStatusV1::CLOSED, backup, 0, 0, 0))
}

fn submit_batch_inner(batch: IncrementalResultBatchV1) -> SinkSubmitOutcomeV1 {
    let Ok(registry) = runtimes().lock() else {
        return rejected(SinkSubmitStatusV1::CLOSED, batch, 0, 0, 0);
    };
    let Some(runtime) = registry.get(&batch.job).and_then(Weak::upgrade) else {
        return rejected(SinkSubmitStatusV1::CLOSED, batch, 0, 0, 0);
    };
    drop(registry);
    let Ok(mut state) = runtime.lock() else {
        return rejected(SinkSubmitStatusV1::CLOSED, batch, 0, 0, 0);
    };
    submit_locked(&mut state, batch)
}

extern "C" fn submit_progress(update: JobProgressUpdateV1) -> JobProgressStatusV1 {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        submit_progress_inner(update)
    }))
    .unwrap_or(JobProgressStatusV1::CLOSED)
}

fn submit_progress_inner(update: JobProgressUpdateV1) -> JobProgressStatusV1 {
    let Ok(registry) = runtimes().lock() else {
        return JobProgressStatusV1::CLOSED;
    };
    let Some(runtime) = registry.get(&update.job).and_then(Weak::upgrade) else {
        return JobProgressStatusV1::CLOSED;
    };
    drop(registry);
    let Ok(mut state) = runtime.lock() else {
        return JobProgressStatusV1::CLOSED;
    };
    #[cfg(test)]
    let panic_next = if state.panic_next_progress_submit {
        state.panic_next_progress_submit = false;
        true
    } else {
        false
    };
    #[cfg(test)]
    if panic_next {
        drop(state);
        panic!("test-only progress trampoline panic");
    }
    let Some(job) = state.jobs.get_mut(&update.job) else {
        return JobProgressStatusV1::CLOSED;
    };
    if std::thread::current().id() != job.owner_thread {
        return JobProgressStatusV1::WRONG_THREAD;
    }
    if job.terminal.is_some() || job.control.into_raw() != JobControlStateV1::ACTIVE.into_raw() {
        return JobProgressStatusV1::CLOSED;
    }
    if !job.provider_call_active
        || !invocation_matches(update.job, update.sink_capability)
        || update.sink_capability != job.sink_capability
        || update.job_generation != update.job.generation()
        || update.job_generation != job.job_generation
        || update.item_generation != job.item_generation
        || update.location_generation != job.location_generation
        || update.source_generation != job.source_generation
        || update.sequence != job.next_progress_sequence
    {
        return JobProgressStatusV1::STALE;
    }
    if update.reserved != 0
        || update.total_units == 0
        || update.completed_units > update.total_units
    {
        job.protocol_faulted = true;
        return JobProgressStatusV1::INVALID;
    }
    let Some(next_sequence) = job.next_progress_sequence.checked_add(1) else {
        return JobProgressStatusV1::CLOSED;
    };
    job.next_progress_sequence = next_sequence;
    job.pending_progress = Some(update);
    JobProgressStatusV1::ACCEPTED
}

fn submit_locked(
    state: &mut RuntimeStateV1,
    batch: IncrementalResultBatchV1,
) -> SinkSubmitOutcomeV1 {
    let Some(job) = state.jobs.get(&batch.job) else {
        return rejected(SinkSubmitStatusV1::CLOSED, batch, 0, 0, 0);
    };
    let credits = remaining_credits(state, job);
    if std::thread::current().id() != job.owner_thread {
        return rejected(
            SinkSubmitStatusV1::WRONG_THREAD,
            batch,
            credits.0,
            credits.1,
            credits.2,
        );
    }
    if job.terminal.is_some() || job.control.into_raw() != JobControlStateV1::ACTIVE.into_raw() {
        return rejected(
            SinkSubmitStatusV1::CLOSED,
            batch,
            credits.0,
            credits.1,
            credits.2,
        );
    }
    if !job.provider_call_active
        || !invocation_matches(batch.job, batch.sink_capability)
        || batch.sink_capability != job.sink_capability
        || batch.job_generation != batch.job.generation()
        || batch.job_generation != job.job_generation
        || batch.location != job.location
        || batch.location_generation != job.location_generation
        || batch.source_generation != job.source_generation
        || batch.sequence != job.next_sequence
    {
        return rejected(
            SinkSubmitStatusV1::STALE,
            batch,
            credits.0,
            credits.1,
            credits.2,
        );
    }
    let Ok(bytes) = validate_batch(job, &batch) else {
        let Some(job) = state.jobs.get_mut(&batch.job) else {
            return rejected(SinkSubmitStatusV1::CLOSED, batch, 0, 0, 0);
        };
        job.protocol_faulted = true;
        return rejected(
            SinkSubmitStatusV1::INVALID,
            batch,
            credits.0,
            credits.1,
            credits.2,
        );
    };
    let items = batch.entries.len();
    let exceeds_byte_credits = match u64::try_from(bytes) {
        Ok(bytes) => bytes > credits.2,
        Err(_) => true,
    };
    if items > credits.1 as usize || exceeds_byte_credits || credits.0 == 0 {
        return rejected(
            SinkSubmitStatusV1::WOULD_BLOCK,
            batch,
            credits.0,
            credits.1,
            credits.2,
        );
    }
    let entries = batch.entries.iter().cloned().collect::<Vec<_>>();
    let sequence = batch.sequence;
    let Some(next_sequence) = job.next_sequence.checked_add(1) else {
        return rejected(
            SinkSubmitStatusV1::CLOSED,
            batch,
            credits.0,
            credits.1,
            credits.2,
        );
    };
    {
        let Some(job) = state.jobs.get_mut(&batch.job) else {
            return rejected(
                SinkSubmitStatusV1::CLOSED,
                batch,
                credits.0,
                credits.1,
                credits.2,
            );
        };
        job.next_sequence = next_sequence;
        job.queued_batches.push_back(StoredBatchV1 {
            sequence,
            job_generation: batch.job_generation,
            item_generation: job.item_generation,
            location_generation: batch.location_generation,
            source_generation: batch.source_generation,
            bytes,
            entries,
        });
        job.queued_items += items;
        job.queued_bytes += bytes;
    }
    state.queued_batches += 1;
    state.queued_items += items;
    state.queued_bytes += bytes;
    let package_usage = state
        .queued_per_package
        .entry(state.jobs[&batch.job].producer.package_id.clone())
        .or_default();
    package_usage.batches += 1;
    package_usage.items += items;
    package_usage.bytes += bytes;
    let remaining = state
        .jobs
        .get(&batch.job)
        .map_or((0, 0, 0), |job| remaining_credits(state, job));
    SinkSubmitOutcomeV1 {
        status: SinkSubmitStatusV1::ACCEPTED,
        remaining_batch_credits: remaining.0,
        remaining_item_credits: remaining.1,
        remaining_byte_credits: remaining.2,
        rejected_batch: ROption::RNone,
    }
}

fn validate_batch(job: &RuntimeJobV1, batch: &IncrementalResultBatchV1) -> Result<usize, ()> {
    if batch.entries.is_empty() || batch.entries.len() > MAX_INCREMENTAL_RESULT_ITEMS_V1 {
        return Err(());
    }
    let item = job.item.ok_or(())?;
    let mut bytes = 0_usize;
    for entry in &batch.entries {
        if entry.item != item
            || entry.item_generation != job.item_generation
            || entry.source_generation != job.source_generation
        {
            return Err(());
        }
        entry.value.validate_transport().map_err(|_| ())?;
        bytes = bytes
            .checked_add(entry.value.text.len())
            .and_then(|total| total.checked_add(entry.value.payload.len()))
            .ok_or(())?;
    }
    (bytes <= MAX_INCREMENTAL_RESULT_BYTES_V1)
        .then_some(bytes)
        .ok_or(())
}

fn remaining_credits(state: &RuntimeStateV1, job: &RuntimeJobV1) -> (u32, u32, u64) {
    if !state.accounting_healthy {
        return (0, 0, 0);
    }
    let config = state.config;
    let package_usage = state
        .queued_per_package
        .get(&job.producer.package_id)
        .copied()
        .unwrap_or_default();
    let batches = config
        .max_batches
        .saturating_sub(state.queued_batches)
        .min(
            config
                .max_batches_per_package
                .saturating_sub(package_usage.batches),
        )
        .min(
            config
                .max_batches_per_job
                .saturating_sub(job.queued_batches.len()),
        );
    let items = config
        .max_items
        .saturating_sub(state.queued_items)
        .min(
            config
                .max_items_per_package
                .saturating_sub(package_usage.items),
        )
        .min(config.max_items_per_job.saturating_sub(job.queued_items));
    let bytes = config
        .max_bytes
        .saturating_sub(state.queued_bytes)
        .min(
            config
                .max_bytes_per_package
                .saturating_sub(package_usage.bytes),
        )
        .min(config.max_bytes_per_job.saturating_sub(job.queued_bytes));
    (
        u32::try_from(batches).unwrap_or(u32::MAX),
        u32::try_from(items).unwrap_or(u32::MAX),
        u64::try_from(bytes).unwrap_or(u64::MAX),
    )
}

fn release_credits(state: &mut RuntimeStateV1, package_id: &str, released: QueueUsageV1) {
    if released == QueueUsageV1::default() {
        return;
    }
    let Some(queued_batches) = state.queued_batches.checked_sub(released.batches) else {
        state.accounting_healthy = false;
        return;
    };
    let Some(queued_items) = state.queued_items.checked_sub(released.items) else {
        state.accounting_healthy = false;
        return;
    };
    let Some(queued_bytes) = state.queued_bytes.checked_sub(released.bytes) else {
        state.accounting_healthy = false;
        return;
    };
    let Some(usage) = state.queued_per_package.get_mut(package_id) else {
        state.accounting_healthy = false;
        return;
    };
    let (Some(batches), Some(items), Some(bytes)) = (
        usage.batches.checked_sub(released.batches),
        usage.items.checked_sub(released.items),
        usage.bytes.checked_sub(released.bytes),
    ) else {
        state.accounting_healthy = false;
        return;
    };
    state.queued_batches = queued_batches;
    state.queued_items = queued_items;
    state.queued_bytes = queued_bytes;
    usage.batches = batches;
    usage.items = items;
    usage.bytes = bytes;
    if *usage == QueueUsageV1::default() {
        state.queued_per_package.remove(package_id);
    }
}

fn rejected(
    status: SinkSubmitStatusV1,
    batch: IncrementalResultBatchV1,
    batches: u32,
    items: u32,
    bytes: u64,
) -> SinkSubmitOutcomeV1 {
    SinkSubmitOutcomeV1 {
        status,
        remaining_batch_credits: batches,
        remaining_item_credits: items,
        remaining_byte_credits: bytes,
        rejected_batch: ROption::RSome(batch),
    }
}

fn terminal_precedence(
    control: JobControlStateV1,
    reported: JobTerminalV1,
    protocol_faulted: bool,
) -> JobTerminalV1 {
    if reported.into_raw() == JobTerminalV1::PANICKED.into_raw() {
        return JobTerminalV1::PANICKED;
    }
    if reported.into_raw() == JobTerminalV1::INCOMPATIBLE.into_raw() || protocol_faulted {
        return JobTerminalV1::INCOMPATIBLE;
    }
    match control.into_raw() {
        raw if raw == JobControlStateV1::CANCELLED.into_raw() => JobTerminalV1::CANCELLED,
        raw if raw == JobControlStateV1::DEADLINE_ELAPSED.into_raw() => {
            JobTerminalV1::DEADLINE_ELAPSED
        }
        raw if raw == JobControlStateV1::CLOSED.into_raw() => JobTerminalV1::CANCELLED,
        _ => reported,
    }
}

fn monotonic_control(
    current: JobControlStateV1,
    requested: JobControlStateV1,
) -> Result<JobControlStateV1, ExtensionJobRuntimeErrorV1> {
    let requested_raw = requested.into_raw();
    if !matches!(requested_raw, 1..=4) {
        return Err(ExtensionJobRuntimeErrorV1::InvalidControlState);
    }
    match current.into_raw() {
        raw if raw == JobControlStateV1::ACTIVE.into_raw() => Ok(requested),
        raw if raw == JobControlStateV1::CANCELLED.into_raw()
            || raw == JobControlStateV1::DEADLINE_ELAPSED.into_raw()
            || raw == JobControlStateV1::CLOSED.into_raw() =>
        {
            if requested_raw == raw {
                Ok(current)
            } else {
                Err(ExtensionJobRuntimeErrorV1::ControlRegression)
            }
        }
        _ => Err(ExtensionJobRuntimeErrorV1::InvalidControlState),
    }
}

/// Typed host-side runtime error; no path or capability bytes are exposed.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum ExtensionJobRuntimeErrorV1 {
    #[error("extension result buffer configuration is invalid")]
    InvalidBufferConfig,
    #[error("extension job request has invalid host generations or identity")]
    InvalidRequest,
    #[error("extension job capability collision")]
    CapabilityCollision,
    #[error("active extension job limit is reached")]
    ActiveJobLimitExceeded,
    #[error("extension job registry publication failed")]
    RegistryPublishFailed,
    #[error("operating-system entropy is unavailable")]
    EntropyUnavailable,
    #[error("extension job runtime state is poisoned")]
    StatePoisoned,
    #[error("extension job is unknown")]
    UnknownJob,
    #[error("provider call is inactive, mismatched, or already executing")]
    InactiveProviderCall,
    #[error("job control state is not defined by ABI v1")]
    InvalidControlState,
    #[error("job control cannot be reopened or downgraded")]
    ControlRegression,
    #[error("current job generation cannot regress or be reused")]
    GenerationRegression,
    #[error("generation update must advance at least one current generation")]
    GenerationUnchanged,
    #[error("job retirement requires an inactive terminal job")]
    RetireRequiresTerminal,
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{Arc, Barrier, Mutex},
        thread,
    };

    use abi_stable::std_types::{RString, RVec};
    use explorer_extension_api::{
        IdNamespaceV1, JobProviderImplementationV1, PluginValueKindV1, PluginValueV1,
    };

    use super::*;

    fn config() -> ExtensionResultBufferConfigV1 {
        ExtensionResultBufferConfigV1::try_new(8, 2, 8, 1, 1, 32, 8, 4, 4096, 1024, 512).unwrap()
    }

    fn request(package_id: &str) -> ExtensionJobRuntimeRequestV1 {
        ExtensionJobRuntimeRequestV1 {
            producer: ExtensionJobProducerV1 {
                package_id: package_id.to_owned(),
                interface_id: StableIdV1::new(IdNamespaceV1::new(1, 1), 1),
                feature_id: "column".to_owned(),
                feature_epoch: 1,
            },
            job_generation: 1,
            item_generation: 1,
            location_generation: 1,
            source_generation: 1,
            has_item: true,
        }
    }

    fn text_value() -> PluginValueV1 {
        PluginValueV1 {
            kind: PluginValueKindV1::TEXT,
            reserved: 0,
            integer: 0,
            float: 0.0,
            text: RString::from("ok"),
            payload: RVec::new(),
            opaque_schema: StableIdV1::new(IdNamespaceV1::new(0, 0), 0),
            opaque_schema_version: 0,
            reserved_tail: 0,
        }
    }

    fn batch(context: JobContextV1, sequence: u64) -> IncrementalResultBatchV1 {
        IncrementalResultBatchV1 {
            job: context.job,
            sink_capability: context.sink.capability,
            job_generation: context.job_generation,
            location: context.location,
            location_generation: context.location_generation,
            source_generation: context.source_generation,
            sequence,
            entries: RVec::from(vec![IncrementalResultEntryV1 {
                item: context.item.into_option().unwrap(),
                item_generation: context.item_generation,
                source_generation: context.source_generation,
                value: text_value(),
            }]),
        }
    }

    fn progress(
        context: JobContextV1,
        sequence: u64,
        completed_units: u64,
        total_units: u64,
    ) -> JobProgressUpdateV1 {
        JobProgressUpdateV1 {
            job: context.job,
            sink_capability: context.progress.capability,
            job_generation: context.job_generation,
            item_generation: context.item_generation,
            location_generation: context.location_generation,
            source_generation: context.source_generation,
            sequence,
            completed_units,
            total_units,
            reserved: 0,
        }
    }

    #[test]
    fn sink_is_scoped_to_one_provider_call_and_rejects_wrong_thread() {
        let runtime = ExtensionJobRuntimeV1::new(config());
        let context = runtime.open_job(request("one")).unwrap();
        let scope = ProviderCallScopeV1::enter(&runtime.state, context).unwrap();
        let accepted = context.sink.try_submit(batch(context, 0));
        assert_eq!(accepted.status, SinkSubmitStatusV1::ACCEPTED);

        let foreign_context = context;
        let wrong_thread =
            thread::spawn(move || foreign_context.sink.try_submit(batch(foreign_context, 1)))
                .join()
                .unwrap();
        assert_eq!(wrong_thread.status, SinkSubmitStatusV1::WRONG_THREAD);
        assert!(wrong_thread.rejected_batch.into_option().is_some());

        drop(scope);
        let retained = context.sink.try_submit(batch(context, 1));
        assert_eq!(retained.status, SinkSubmitStatusV1::STALE);
        assert!(retained.rejected_batch.into_option().is_some());
    }

    #[test]
    fn stale_generations_are_discarded_and_release_package_credits() {
        let runtime = ExtensionJobRuntimeV1::new(config());
        let first = runtime.open_job(request("one")).unwrap();
        let second = runtime.open_job(request("one")).unwrap();
        let first_scope = ProviderCallScopeV1::enter(&runtime.state, first).unwrap();
        assert_eq!(
            first.sink.try_submit(batch(first, 0)).status,
            SinkSubmitStatusV1::ACCEPTED
        );
        drop(first_scope);

        let second_scope = ProviderCallScopeV1::enter(&runtime.state, second).unwrap();
        let package_limited = second.sink.try_submit(batch(second, 0));
        assert_eq!(package_limited.status, SinkSubmitStatusV1::WOULD_BLOCK);
        drop(second_scope);

        runtime
            .update_current_generations(first.job, 2, 2, 2)
            .unwrap();
        assert!(runtime.drain(first.job, 2, 2, 2, 8).is_empty());
        assert_eq!(
            runtime.update_current_generations(first.job, 1, 2, 2),
            Err(ExtensionJobRuntimeErrorV1::GenerationRegression)
        );

        let second_scope = ProviderCallScopeV1::enter(&runtime.state, second).unwrap();
        assert_eq!(
            second.sink.try_submit(batch(second, 0)).status,
            SinkSubmitStatusV1::ACCEPTED
        );
        drop(second_scope);
    }

    #[test]
    fn drain_releases_job_credits_before_resubmit_and_purge() {
        let runtime = ExtensionJobRuntimeV1::new(config());
        let context = runtime.open_job(request("one")).unwrap();
        let scope = ProviderCallScopeV1::enter(&runtime.state, context).unwrap();
        assert_eq!(
            context.sink.try_submit(batch(context, 0)).status,
            SinkSubmitStatusV1::ACCEPTED
        );
        drop(scope);
        assert_eq!(runtime.drain(context.job, 1, 1, 1, 1).len(), 1);

        let scope = ProviderCallScopeV1::enter(&runtime.state, context).unwrap();
        assert_eq!(
            context.sink.try_submit(batch(context, 1)).status,
            SinkSubmitStatusV1::ACCEPTED
        );
        drop(scope);
        runtime.purge(context.job).unwrap();

        let scope = ProviderCallScopeV1::enter(&runtime.state, context).unwrap();
        assert_eq!(
            context.sink.try_submit(batch(context, 2)).status,
            SinkSubmitStatusV1::ACCEPTED
        );
        drop(scope);
        runtime.purge(context.job).unwrap();
        assert!(runtime.state.lock().unwrap().accounting_healthy);
    }

    #[test]
    fn progress_is_latest_wins_and_invalid_or_out_of_order_updates_do_not_advance_it() {
        let runtime = ExtensionJobRuntimeV1::new(config());
        let context = runtime.open_job(request("one")).unwrap();
        let scope = ProviderCallScopeV1::enter(&runtime.state, context).unwrap();
        assert_eq!(
            context.progress.try_submit(progress(context, 0, 1, 4)),
            JobProgressStatusV1::ACCEPTED
        );
        assert_eq!(
            context.progress.try_submit(progress(context, 1, 3, 4)),
            JobProgressStatusV1::ACCEPTED
        );
        assert_eq!(
            context.progress.try_submit(progress(context, 2, 5, 4)),
            JobProgressStatusV1::INVALID
        );
        assert_eq!(
            context.progress.try_submit(progress(context, 3, 4, 4)),
            JobProgressStatusV1::STALE
        );
        assert_eq!(
            runtime.take_progress(
                context.job,
                context.job_generation,
                context.item_generation,
                context.location_generation,
                context.source_generation,
            ),
            Some(progress(context, 1, 3, 4))
        );
        assert!(
            runtime
                .take_progress(
                    context.job,
                    context.job_generation,
                    context.item_generation,
                    context.location_generation,
                    context.source_generation,
                )
                .is_none()
        );
        drop(scope);
    }

    #[test]
    fn progress_rejects_retained_wrong_thread_cancelled_and_panicking_calls() {
        let runtime = ExtensionJobRuntimeV1::new(config());
        let context = runtime.open_job(request("one")).unwrap();
        let scope = ProviderCallScopeV1::enter(&runtime.state, context).unwrap();
        let foreign_context = context;
        let wrong_thread = thread::spawn(move || {
            foreign_context
                .progress
                .try_submit(progress(foreign_context, 0, 1, 1))
        })
        .join()
        .unwrap();
        assert_eq!(wrong_thread, JobProgressStatusV1::WRONG_THREAD);

        runtime.state.lock().unwrap().panic_next_progress_submit = true;
        assert_eq!(
            context.progress.try_submit(progress(context, 0, 1, 1)),
            JobProgressStatusV1::CLOSED
        );
        assert_eq!(
            context.progress.try_submit(progress(context, 0, 1, 1)),
            JobProgressStatusV1::ACCEPTED
        );
        drop(scope);
        assert_eq!(
            context.progress.try_submit(progress(context, 1, 1, 1)),
            JobProgressStatusV1::STALE
        );
        let scope = ProviderCallScopeV1::enter(&runtime.state, context).unwrap();
        runtime
            .request_control(context.job, JobControlStateV1::CANCELLED)
            .unwrap();
        assert!(
            runtime
                .take_progress(
                    context.job,
                    context.job_generation,
                    context.item_generation,
                    context.location_generation,
                    context.source_generation,
                )
                .is_none()
        );
        assert_eq!(
            context.progress.try_submit(progress(context, 1, 1, 1)),
            JobProgressStatusV1::CLOSED
        );
        drop(scope);
        let _ = runtime.finish(context.job, JobTerminalV1::COMPLETED);
        assert_eq!(
            context.progress.try_submit(progress(context, 1, 1, 1)),
            JobProgressStatusV1::CLOSED
        );
    }

    #[test]
    fn progress_generation_race_rejects_old_callback_and_never_applies_stale_data() {
        let runtime = ExtensionJobRuntimeV1::new(config());
        let context = runtime.open_job(request("one")).unwrap();
        let scope = ProviderCallScopeV1::enter(&runtime.state, context).unwrap();
        let barrier = Arc::new(Barrier::new(2));
        let runtime_ref = &runtime;
        thread::scope(|threads| {
            let advance = Arc::clone(&barrier);
            threads.spawn(move || {
                advance.wait();
                runtime_ref
                    .update_current_generations(context.job, 2, 2, 2)
                    .unwrap();
                advance.wait();
            });
            barrier.wait();
            barrier.wait();
        });
        assert_eq!(
            context.progress.try_submit(progress(context, 0, 1, 1)),
            JobProgressStatusV1::STALE
        );
        assert!(runtime.take_progress(context.job, 1, 2, 2, 2).is_none());
        drop(scope);
    }

    #[test]
    fn dispatch_scope_rejects_nested_or_cancelled_calls_and_terminal_is_exactly_once() {
        let runtime = ExtensionJobRuntimeV1::new(config());
        let first = runtime.open_job(request("one")).unwrap();
        let second = runtime.open_job(request("two")).unwrap();
        let first_scope = ProviderCallScopeV1::enter(&runtime.state, first).unwrap();
        assert!(matches!(
            ProviderCallScopeV1::enter(&runtime.state, second),
            Err(ExtensionJobRuntimeErrorV1::InactiveProviderCall)
        ));
        assert_eq!(poll_control_inner(first.job), JobControlStateV1::ACTIVE);
        assert_eq!(poll_control_inner(second.job), JobControlStateV1::CLOSED);
        drop(first_scope);
        assert_eq!(poll_control_inner(first.job), JobControlStateV1::CLOSED);
        runtime
            .request_control(second.job, JobControlStateV1::CANCELLED)
            .unwrap();
        assert!(matches!(
            ProviderCallScopeV1::enter(&runtime.state, second),
            Err(ExtensionJobRuntimeErrorV1::InactiveProviderCall)
        ));

        let third = runtime.open_job(request("three")).unwrap();
        let barrier = Arc::new(Barrier::new(2));
        let results = Mutex::new(Vec::new());
        let runtime_ref = &runtime;
        thread::scope(|threads| {
            for terminal in [JobTerminalV1::COMPLETED, JobTerminalV1::PANICKED] {
                let barrier = Arc::clone(&barrier);
                let results = &results;
                threads.spawn(move || {
                    barrier.wait();
                    results
                        .lock()
                        .unwrap()
                        .push(runtime_ref.finish(third.job, terminal));
                });
            }
        });
        let results = results.into_inner().unwrap();
        assert_eq!(
            results
                .iter()
                .filter(|result| matches!(result, ExtensionJobFinishOutcomeV1::Published(_)))
                .count(),
            1
        );
    }

    #[test]
    fn protocol_faults_override_control_for_terminal_reporting_and_retirement_requires_safe_state()
    {
        let runtime = ExtensionJobRuntimeV1::new(config());
        let context = runtime.open_job(request("one")).unwrap();
        assert_eq!(
            runtime.retire(context.job),
            Err(ExtensionJobRuntimeErrorV1::RetireRequiresTerminal)
        );
        let scope = ProviderCallScopeV1::enter(&runtime.state, context).unwrap();
        let mut invalid = batch(context, 0);
        invalid.entries[0].value.float = -0.0;
        assert_eq!(
            context.sink.try_submit(invalid).status,
            SinkSubmitStatusV1::INVALID
        );
        let _ = runtime.finish(context.job, JobTerminalV1::COMPLETED);
        assert_eq!(
            runtime.retire(context.job),
            Err(ExtensionJobRuntimeErrorV1::RetireRequiresTerminal)
        );
        drop(scope);
        assert_eq!(
            runtime.finish(context.job, JobTerminalV1::COMPLETED),
            ExtensionJobFinishOutcomeV1::AlreadyTerminal(JobTerminalV1::INCOMPATIBLE)
        );
        runtime.retire(context.job).unwrap();

        let panicked = runtime.open_job(request("two")).unwrap();
        runtime
            .request_control(panicked.job, JobControlStateV1::DEADLINE_ELAPSED)
            .unwrap();
        assert_eq!(
            runtime.finish(panicked.job, JobTerminalV1::PANICKED),
            ExtensionJobFinishOutcomeV1::Published(JobTerminalV1::PANICKED)
        );
        runtime.retire(panicked.job).unwrap();
    }

    #[test]
    fn polling_and_runtime_drop_never_hold_registry_and_runtime_locks_in_reverse_order() {
        let runtime = ExtensionJobRuntimeV1::new(config());
        let context = runtime.open_job(request("one")).unwrap();
        let start = Arc::new(Barrier::new(2));
        let poll_start = Arc::clone(&start);
        let poller = thread::spawn(move || {
            poll_start.wait();
            for _ in 0..1_000 {
                let _ = poll_control_inner(context.job);
            }
        });
        start.wait();
        drop(runtime);
        poller.join().unwrap();
    }

    #[test]
    fn active_jobs_are_bounded_and_retirement_reclaims_state_and_registry_slots() {
        let config =
            ExtensionResultBufferConfigV1::try_new(4, 2, 8, 1, 1, 32, 8, 4, 4096, 1024, 512)
                .unwrap();
        let runtime = ExtensionJobRuntimeV1::new(config);
        let first = runtime.open_job(request("one")).unwrap();
        let second = runtime.open_job(request("one")).unwrap();
        assert!(matches!(
            runtime.open_job(request("one")),
            Err(ExtensionJobRuntimeErrorV1::ActiveJobLimitExceeded)
        ));
        let third = runtime.open_job(request("two")).unwrap();
        let fourth = runtime.open_job(request("three")).unwrap();
        assert!(matches!(
            runtime.open_job(request("four")),
            Err(ExtensionJobRuntimeErrorV1::ActiveJobLimitExceeded)
        ));

        for context in [first, second, third, fourth] {
            assert!(matches!(
                runtime.finish(context.job, JobTerminalV1::COMPLETED),
                ExtensionJobFinishOutcomeV1::Published(JobTerminalV1::COMPLETED)
            ));
            assert!(runtime.drain(context.job, 1, 1, 1, 1).is_empty());
            runtime.retire(context.job).unwrap();
            assert_eq!(poll_control_inner(context.job), JobControlStateV1::CLOSED);
        }
        let state = runtime.state.lock().unwrap();
        assert!(state.jobs.is_empty());
        assert!(state.active_jobs_per_package.is_empty());
        drop(state);

        for index in 0..100 {
            let package = format!("pkg{}", index % 4);
            let context = runtime.open_job(request(&package)).unwrap();
            let _ = runtime.finish(context.job, JobTerminalV1::COMPLETED);
            assert!(runtime.drain(context.job, 1, 1, 1, 1).is_empty());
            runtime.retire(context.job).unwrap();
        }
        assert!(runtime.state.lock().unwrap().jobs.is_empty());
    }

    #[test]
    fn registry_publish_failure_rolls_back_admission() {
        let runtime = ExtensionJobRuntimeV1::new(config());
        runtime.state.lock().unwrap().fail_next_registry_publish = true;
        assert!(matches!(
            runtime.open_job(request("one")),
            Err(ExtensionJobRuntimeErrorV1::RegistryPublishFailed)
        ));
        let state = runtime.state.lock().unwrap();
        assert!(state.jobs.is_empty());
        assert!(state.active_jobs_per_package.is_empty());
        assert!(state.accounting_healthy);
        drop(state);

        let context = runtime.open_job(request("one")).unwrap();
        let _ = runtime.finish(context.job, JobTerminalV1::COMPLETED);
        runtime.retire(context.job).unwrap();
    }

    struct Completes;

    impl JobProviderImplementationV1 for Completes {
        fn run(_: JobContextV1) -> JobTerminalV1 {
            JobTerminalV1::COMPLETED
        }
    }

    #[test]
    fn control_is_monotonic_and_internal_dispatch_normalizes_terminals() {
        let runtime = ExtensionJobRuntimeV1::new(config());
        let context = runtime.open_job(request("one")).unwrap();
        runtime
            .request_control(context.job, JobControlStateV1::CANCELLED)
            .unwrap();
        assert_eq!(
            runtime.request_control(context.job, JobControlStateV1::ACTIVE),
            Err(ExtensionJobRuntimeErrorV1::ControlRegression)
        );
        assert_eq!(
            runtime.finish(context.job, JobTerminalV1::from_raw(99)),
            ExtensionJobFinishOutcomeV1::Published(JobTerminalV1::INCOMPATIBLE)
        );

        let second = runtime.open_job(request("two")).unwrap();
        assert_eq!(
            runtime
                .invoke_provider_for_test(second, JobProviderCallbackV1::new::<Completes>())
                .unwrap(),
            ExtensionJobFinishOutcomeV1::Published(JobTerminalV1::COMPLETED)
        );
    }
}
