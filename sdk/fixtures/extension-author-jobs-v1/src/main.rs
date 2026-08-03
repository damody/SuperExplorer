//! Compile-checked author-side example for the Rust-first V1 job ABI.
//!
//! This fixture intentionally imports no host crate. Its mock services prove
//! ownership and status handling using only the public SDK transport surface.

use std::sync::{Arc, Mutex, OnceLock};

use abi_stable::std_types::{ROption, RResult, RString, RVec};
use explorer_extension_api::{
    AbiJobHostServicesV1, EXTENSION_ID_NAMESPACE_V1, ExtensionRegistrarImplementationV1,
    ExtensionRootModuleV1, IncrementalResultBatchV1, IncrementalResultEntryV1, ItemHandleV1,
    JobContextV1, JobControlStateV1, JobHostServicesV1, JobProgressStatusV1, JobProgressUpdateV1,
    JobProviderImplementationV1, JobProviderObjectV1, JobTerminalV1, LocationHandleV1,
    PluginItemResultV1, PluginMetadataV1, PluginValueV1, RegisteredContributionKindV1,
    RegisteredContributionV1, RegistrarOutputResultV1, RegistrarOutputV1, RegistrarRequestV1,
    RegistrationOutcomeV1, SinkCapabilityV1, SinkSubmitOutcomeV1, SinkSubmitStatusV1, StableIdV1,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SinkMode {
    Accepted,
    WouldBlock,
    Stale,
    Closed,
    WrongThread,
    Invalid,
}

impl SinkMode {
    const fn status(self) -> SinkSubmitStatusV1 {
        match self {
            Self::Accepted => SinkSubmitStatusV1::ACCEPTED,
            Self::WouldBlock => SinkSubmitStatusV1::WOULD_BLOCK,
            Self::Stale => SinkSubmitStatusV1::STALE,
            Self::Closed => SinkSubmitStatusV1::CLOSED,
            Self::WrongThread => SinkSubmitStatusV1::WRONG_THREAD,
            Self::Invalid => SinkSubmitStatusV1::INVALID,
        }
    }

    const fn credits(self) -> (u32, u32, u64) {
        match self {
            Self::Accepted => (7, 63, 65_536),
            Self::WouldBlock => (6, 62, 65_535),
            Self::Stale => (5, 61, 65_534),
            Self::Closed => (4, 60, 65_533),
            Self::WrongThread => (3, 59, 65_532),
            Self::Invalid => (2, 58, 65_531),
        }
    }
}

#[derive(Debug, Default)]
struct MockState {
    accepted: Vec<IncrementalResultBatchV1>,
    progress: Vec<JobProgressUpdateV1>,
}

#[derive(Clone, Copy, Debug)]
struct ProviderObservation {
    status: SinkSubmitStatusV1,
    remaining_batch_credits: u32,
    remaining_item_credits: u32,
    remaining_byte_credits: u64,
    returned_batch_matches_context: bool,
}

static PROVIDER_OBSERVATION: OnceLock<Mutex<Option<ProviderObservation>>> = OnceLock::new();

fn provider_observation() -> &'static Mutex<Option<ProviderObservation>> {
    PROVIDER_OBSERVATION.get_or_init(|| Mutex::new(None))
}

#[derive(Clone)]
struct MockHost {
    mode: SinkMode,
    state: Arc<Mutex<MockState>>,
}

impl AbiJobHostServicesV1 for MockHost {
    fn poll_control(&self) -> JobControlStateV1 {
        JobControlStateV1::ACTIVE
    }

    fn submit_results(&self, batch: IncrementalResultBatchV1) -> SinkSubmitOutcomeV1 {
        let (remaining_batch_credits, remaining_item_credits, remaining_byte_credits) =
            self.mode.credits();
        if self.mode == SinkMode::Accepted {
            self.state.lock().expect("mock lock").accepted.push(batch);
            SinkSubmitOutcomeV1 {
                status: self.mode.status(),
                remaining_batch_credits,
                remaining_item_credits,
                remaining_byte_credits,
                rejected_batch: ROption::RNone,
            }
        } else {
            SinkSubmitOutcomeV1 {
                status: self.mode.status(),
                remaining_batch_credits,
                remaining_item_credits,
                remaining_byte_credits,
                rejected_batch: ROption::RSome(batch),
            }
        }
    }

    fn submit_progress(&self, update: JobProgressUpdateV1) -> JobProgressStatusV1 {
        self.state.lock().expect("mock lock").progress.push(update);
        JobProgressStatusV1::ACCEPTED
    }
}

struct AuthorRegistrar;

impl ExtensionRegistrarImplementationV1 for AuthorRegistrar {
    fn create() -> Self {
        Self
    }

    fn register(&self, _: RegistrarRequestV1) -> RegistrarOutputResultV1 {
        RResult::ROk(RegistrarOutputV1 {
            outcome: RegistrationOutcomeV1::accepted(1),
            contributions: RVec::from(vec![RegisteredContributionV1 {
                feature_id: RString::from("author-fixture"),
                contribution_id: RString::from("jobs"),
                kind: RegisteredContributionKindV1::COLUMN,
                required_capabilities: RVec::new(),
                interface_id: StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 4_801),
                expected_sort: ROption::RNone,
                opaque_contract: ROption::RNone,
                renderer_contribution_id: ROption::RNone,
                provider: ROption::RSome(JobProviderObjectV1::new(AuthorProvider)),
                visual_column: ROption::RNone,
            }]),
        })
    }
}

struct AuthorProvider;

impl JobProviderImplementationV1 for AuthorProvider {
    fn run(&self, context: JobContextV1) -> JobTerminalV1 {
        let item = match context.item {
            ROption::RSome(item) => item,
            ROption::RNone => return JobTerminalV1::INCOMPATIBLE,
        };
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

        let progress = context.progress.try_submit(JobProgressUpdateV1 {
            job: context.job,
            sink_capability: context.progress.capability,
            job_generation: context.job_generation,
            item_generation: context.item_generation,
            location_generation: context.location_generation,
            source_generation: context.source_generation,
            sequence: 0,
            completed_units: 1,
            total_units: 1,
            reserved: 0,
        });
        if progress != JobProgressStatusV1::ACCEPTED {
            return JobTerminalV1::PLUGIN_ERROR;
        }

        let outcome = context.sink.try_submit(IncrementalResultBatchV1 {
            job: context.job,
            sink_capability: context.sink.capability,
            job_generation: context.job_generation,
            location: context.location,
            location_generation: context.location_generation,
            source_generation: context.source_generation,
            sequence: 0,
            entries: RVec::from(vec![IncrementalResultEntryV1 {
                item,
                item_generation: context.item_generation,
                source_generation: context.source_generation,
                result: PluginItemResultV1::value(
                    PluginValueV1::text("author-fixture").expect("literal is transport-valid"),
                    ROption::RNone,
                ),
            }]),
        });
        let returned_batch_matches_context = match &outcome.rejected_batch {
            ROption::RNone => false,
            ROption::RSome(batch) => batch_matches_context(batch, &context),
        };
        *provider_observation()
            .lock()
            .expect("provider observation lock") = Some(ProviderObservation {
            status: outcome.status,
            remaining_batch_credits: outcome.remaining_batch_credits,
            remaining_item_credits: outcome.remaining_item_credits,
            remaining_byte_credits: outcome.remaining_byte_credits,
            returned_batch_matches_context,
        });
        match outcome.status {
            status if status == SinkSubmitStatusV1::ACCEPTED => {
                if matches!(outcome.rejected_batch, ROption::RNone) {
                    JobTerminalV1::COMPLETED
                } else {
                    JobTerminalV1::PLUGIN_ERROR
                }
            }
            status if status == SinkSubmitStatusV1::WOULD_BLOCK => {
                if matches!(outcome.rejected_batch, ROption::RSome(_)) {
                    JobTerminalV1::BACKPRESSURED
                } else {
                    JobTerminalV1::PLUGIN_ERROR
                }
            }
            status
                if status == SinkSubmitStatusV1::STALE || status == SinkSubmitStatusV1::CLOSED =>
            {
                JobTerminalV1::CANCELLED
            }
            status
                if status == SinkSubmitStatusV1::WRONG_THREAD
                    || status == SinkSubmitStatusV1::INVALID =>
            {
                JobTerminalV1::PLUGIN_ERROR
            }
            _ => JobTerminalV1::PLUGIN_ERROR,
        }
    }
}

fn batch_matches_context(batch: &IncrementalResultBatchV1, context: &JobContextV1) -> bool {
    let Some(item) = context.item.into_option() else {
        return false;
    };
    batch.job == context.job
        && batch.sink_capability == context.sink.capability
        && batch.job_generation == context.job_generation
        && batch.location == context.location
        && batch.location_generation == context.location_generation
        && batch.source_generation == context.source_generation
        && batch.sequence == 0
        && batch.entries.len() == 1
        && batch.entries[0].item == item
        && batch.entries[0].item_generation == context.item_generation
        && batch.entries[0].source_generation == context.source_generation
        && batch.entries[0].result.outcome == explorer_extension_api::PluginItemOutcomeV1::VALUE
        && matches!(
            &batch.entries[0].result.value,
            ROption::RSome(value)
                if value.kind == explorer_extension_api::PluginValueKindV1::TEXT
                    && value.text.as_str() == "author-fixture"
        )
        && matches!(batch.entries[0].result.stable_sort, ROption::RNone)
}

fn context(host: MockHost) -> JobContextV1 {
    let job = explorer_extension_api::JobHandleV1::from_host([1; 16], 11);
    let capability = SinkCapabilityV1::from_host([2; 16]);
    let services = JobHostServicesV1::from_host(host);
    JobContextV1 {
        job,
        item: ROption::RSome(ItemHandleV1::from_host([3; 16], 12)),
        location: LocationHandleV1::from_host([4; 16], 13),
        feature_epoch: 14,
        job_generation: 11,
        item_generation: 12,
        location_generation: 13,
        source_generation: 15,
        input: ROption::RNone,
        sink: services.result_sink(job, capability),
        progress: services.progress_sink(job, capability),
    }
}

fn assert_mode(mode: SinkMode, expected: JobTerminalV1) {
    *provider_observation()
        .lock()
        .expect("provider observation lock") = None;
    let state = Arc::new(Mutex::new(MockState::default()));
    let context = context(MockHost {
        mode,
        state: Arc::clone(&state),
    });
    let terminal = JobProviderObjectV1::new(AuthorProvider).invoke(context.clone());
    assert_eq!(terminal, expected, "unexpected terminal for {mode:?}");
    let observation = provider_observation()
        .lock()
        .expect("provider observation lock")
        .take()
        .expect("provider must observe its sink outcome");
    assert_eq!(observation.status, mode.status());
    assert_eq!(
        (
            observation.remaining_batch_credits,
            observation.remaining_item_credits,
            observation.remaining_byte_credits,
        ),
        mode.credits(),
        "provider-visible remaining credits must match the host response"
    );
    assert_eq!(
        observation.returned_batch_matches_context,
        mode != SinkMode::Accepted,
        "rejected ownership must return the complete original batch to the provider"
    );
    let state = state.lock().expect("mock lock");
    assert_eq!(state.progress.len(), 1);
    let progress = state.progress[0];
    assert_eq!(progress.job, context.job);
    assert_eq!(progress.sink_capability, context.progress.capability);
    assert_eq!(progress.job_generation, context.job_generation);
    assert_eq!(progress.item_generation, context.item_generation);
    assert_eq!(progress.location_generation, context.location_generation);
    assert_eq!(progress.source_generation, context.source_generation);
    assert_eq!(progress.sequence, 0);
    assert_eq!(progress.completed_units, 1);
    assert_eq!(progress.total_units, 1);
    assert_eq!(progress.reserved, 0);
    if mode == SinkMode::Accepted {
        assert_eq!(state.accepted.len(), 1);
        let batch = &state.accepted[0];
        assert_eq!(batch.job, context.job);
        assert_eq!(batch.sink_capability, context.sink.capability);
        assert_eq!(batch.job_generation, context.job_generation);
        assert_eq!(batch.location, context.location);
        assert_eq!(batch.location_generation, context.location_generation);
        assert_eq!(batch.source_generation, context.source_generation);
        assert_eq!(batch.sequence, 0);
        assert_eq!(batch.entries.len(), 1);
        assert_eq!(batch.entries[0].item, context.item.into_option().unwrap());
        assert_eq!(batch.entries[0].item_generation, context.item_generation);
        assert_eq!(
            batch.entries[0].source_generation,
            context.source_generation
        );
    } else {
        assert!(
            state.accepted.is_empty(),
            "rejected batch must remain host-unconsumed"
        );
    }
}

fn main() {
    let _root = ExtensionRootModuleV1::new::<AuthorRegistrar>(
        PluginMetadataV1 {
            plugin_id: StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 4_800),
            primary_interface_id: StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 4_801),
        },
        ROption::RNone,
    );
    assert_mode(SinkMode::Accepted, JobTerminalV1::COMPLETED);
    assert_mode(SinkMode::WouldBlock, JobTerminalV1::BACKPRESSURED);
    assert_mode(SinkMode::Stale, JobTerminalV1::CANCELLED);
    assert_mode(SinkMode::Closed, JobTerminalV1::CANCELLED);
    assert_mode(SinkMode::WrongThread, JobTerminalV1::PLUGIN_ERROR);
    assert_mode(SinkMode::Invalid, JobTerminalV1::PLUGIN_ERROR);
    println!("extension author jobs v1 fixture: PASS");
}
