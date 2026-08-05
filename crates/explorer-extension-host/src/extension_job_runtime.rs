//! Host-owned capability registry and bounded result transport for extension jobs.
//!
//! Provider registration is admitted through the sealed registrar/lifecycle path;
//! this module owns the generation-safe runtime after that admission succeeds.

use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    fmt,
    path::PathBuf,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, Ordering},
    },
    thread::ThreadId,
};

use abi_stable::std_types::{ROption, RString, RVec};
use explorer_extension_api::{
    AbiInputStreamServicesV1, AbiJobHostServicesV1, AbiLockOwnerQueryServiceV1,
    BatchColumnContextV1, BatchColumnItemV1, BatchColumnProviderObjectV1, IncrementalResultBatchV1,
    InputStreamCapabilityV1, InputStreamLengthOutcomeV1, InputStreamReadOutcomeV1,
    InputStreamReadRequestV1, InputStreamSeekOriginV1, InputStreamSeekOutcomeV1,
    InputStreamSeekRequestV1, InputStreamStatusV1, InputStreamV1, ItemHandleV1, JobContextV1,
    JobControlStateV1, JobHandleV1, JobHostServicesV1, JobProgressStatusV1, JobProgressUpdateV1,
    JobTerminalV1, LocationHandleV1, LockOwnerQueryOutcomeV1, LockOwnerQueryRequestV1,
    LockOwnerQueryServiceV1, LockOwnerQueryStatusV1, LockOwnerRecordV1,
    MAX_BATCH_COLUMN_FILE_NAME_BYTES_V1, MAX_BATCH_COLUMN_ITEMS_V1,
    MAX_INCREMENTAL_RESULT_BYTES_V1, MAX_INCREMENTAL_RESULT_ITEMS_V1,
    MAX_INPUT_STREAM_READ_BYTES_V1, SinkCapabilityV1, SinkSubmitOutcomeV1, SinkSubmitStatusV1,
    StableIdV1, StableSortValueKindV1,
};
use ring::rand::{SecureRandom as _, SystemRandom};
use sha2::{Digest as _, Sha256};
use thiserror::Error;

use crate::{ContributionKindV1, NativeDispatchLeaseV1, ValidatedContributionSetV1};
use crate::{
    ExtensionValueRowV1, UiInvalidationBatcherV1,
    extension_job_ui_bridge::{ExtensionJobUiReadySignalV1, RuntimeReadySignalSinkV1},
    extension_result_cache::{
        ExtensionResultCacheAdmissionV1, ExtensionResultCacheConfigV1,
        ExtensionResultCacheFileFactV1, ExtensionResultCacheGenerationV1,
        ExtensionResultCacheInsertOutcomeV1, ExtensionResultCacheKeyV1,
        ExtensionResultCacheLookupV1, ExtensionResultCacheV1,
    },
    extension_value_router::{
        ExtensionValueGenerationStateV1, ExtensionValueGenerationV1, HostIncrementalResultEntryV1,
        ingest_entry_v1,
    },
    runtime_authority::{
        AuthorityAdapterV1, AuthorityClaimsV1, AuthorityEnvelopeV1, RuntimeAuthorityV1,
    },
};

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

/// Hard upper bound for one host runtime's active jobs and ready-signal scan.
pub const MAX_EXTENSION_RUNTIME_REGISTRY_JOBS_V1: usize = 4096;

#[allow(clippy::missing_errors_doc)]
impl ExtensionResultBufferConfigV1 {
    /// Conservative process-host defaults. Individual extension submissions
    /// remain bounded by the ABI contract and all values are validated by the
    /// same constructor used for test configurations.
    pub(crate) fn host_default() -> Self {
        Self {
            max_active_jobs: 256,
            max_active_jobs_per_package: 32,
            max_batches: 1_024,
            max_batches_per_package: 128,
            max_batches_per_job: 32,
            max_items: 32_768,
            max_items_per_package: 4_096,
            max_items_per_job: 1_024,
            max_bytes: 64 * 1024 * 1024,
            max_bytes_per_package: 8 * 1024 * 1024,
            max_bytes_per_job: 1024 * 1024,
        }
    }

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
        if values.into_iter().any(|value| value == 0)
            || max_active_jobs > MAX_EXTENSION_RUNTIME_REGISTRY_JOBS_V1
        {
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

/// Host-attested producer identity attached after a sink accepts copied data.
/// It has no public raw constructor; only [`ExtensionJobAuthorityV1`] can mint it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExtensionJobProducerV1 {
    pub(crate) package_id: String,
    pub(crate) sealed_manifest_digest: String,
    pub(crate) data_version: u64,
    pub(crate) contribution_id: String,
    pub(crate) interface_id: StableIdV1,
    pub(crate) feature_id: String,
    pub(crate) feature_epoch: u64,
    pub(crate) opaque_schema: Option<StableIdV1>,
    pub(crate) opaque_schema_version: Option<u32>,
    pub(crate) renderer_contribution_id: Option<String>,
}

impl ExtensionJobProducerV1 {
    #[must_use]
    pub fn package_id(&self) -> &str {
        &self.package_id
    }
    #[must_use]
    pub fn sealed_manifest_digest(&self) -> &str {
        &self.sealed_manifest_digest
    }
    #[must_use]
    pub const fn data_version(&self) -> u64 {
        self.data_version
    }
    #[must_use]
    pub fn contribution_id(&self) -> &str {
        &self.contribution_id
    }
    #[must_use]
    pub const fn interface_id(&self) -> StableIdV1 {
        self.interface_id
    }
    #[must_use]
    pub fn feature_id(&self) -> &str {
        &self.feature_id
    }
    #[must_use]
    pub const fn feature_epoch(&self) -> u64 {
        self.feature_epoch
    }
}

/// Linear host authority proving a live native feature dispatch and one sealed,
/// validated contribution. Plugins cannot forge package/source/epoch strings.
pub struct ExtensionJobAuthorityV1 {
    producer: ExtensionJobProducerV1,
    pub(crate) contribution_kind: ContributionKindV1,
    expected_sort: ROption<StableSortValueKindV1>,
    pub(crate) opaque_schema: Option<StableIdV1>,
    pub(crate) opaque_schema_version: Option<u32>,
    filesystem_read_authorized: bool,
    lock_owner_query_authorized: bool,
    _lease: Option<NativeDispatchLeaseV1>,
}

impl fmt::Debug for ExtensionJobAuthorityV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExtensionJobAuthorityV1")
            .field("producer", &self.producer)
            .finish_non_exhaustive()
    }
}

impl ExtensionJobAuthorityV1 {
    /// Mints the narrow authority used by the one explicit direct-DLL batch
    /// column example. It has no package discovery or manifest semantics: the
    /// caller must already hold a retained contribution from the direct loader.
    #[must_use]
    pub fn for_direct_batch_column(plugin_id: StableIdV1, contribution_id: &str) -> Option<Self> {
        if !plugin_id.is_valid()
            || contribution_id.is_empty()
            || contribution_id.len() > 64
            || !contribution_id.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b':')
            })
        {
            return None;
        }
        Some(Self {
            producer: ExtensionJobProducerV1 {
                package_id: format!("direct-dll-{:016x}", plugin_id.value),
                sealed_manifest_digest: "explicit-direct-dll-v1".to_owned(),
                data_version: 1,
                contribution_id: contribution_id.to_owned(),
                interface_id: plugin_id,
                feature_id: contribution_id.to_owned(),
                feature_epoch: 1,
                opaque_schema: None,
                opaque_schema_version: None,
                renderer_contribution_id: None,
            },
            expected_sort: ROption::RSome(StableSortValueKindV1::U64),
            contribution_kind: ContributionKindV1::Column,
            opaque_schema: None,
            opaque_schema_version: None,
            filesystem_read_authorized: true,
            lock_owner_query_authorized: false,
            _lease: None,
        })
    }

    #[allow(clippy::missing_errors_doc)]
    pub(crate) fn mint_sealed(
        validated: &ValidatedContributionSetV1,
        contribution_id: &str,
        lease: NativeDispatchLeaseV1,
    ) -> Result<Self, ExtensionJobRuntimeErrorV1> {
        let identity = lease.feature_identity();
        let descriptor = validated
            .job_descriptor(contribution_id)
            .ok_or(ExtensionJobRuntimeErrorV1::UnauthorizedAuthority)?;
        if validated.package_id() != identity.package_id()
            || validated.sealed_manifest_digest() != identity.sealed_manifest_digest()
            || descriptor.feature_id != identity.feature_id()
            || lease.epoch() == 0
        {
            return Err(ExtensionJobRuntimeErrorV1::UnauthorizedAuthority);
        }
        Ok(Self {
            producer: ExtensionJobProducerV1 {
                package_id: validated.package_id().to_owned(),
                sealed_manifest_digest: validated.sealed_manifest_digest().to_owned(),
                data_version: validated.data_version(),
                contribution_id: descriptor.contribution_id.clone(),
                interface_id: descriptor.interface_id,
                feature_id: descriptor.feature_id.clone(),
                feature_epoch: lease.epoch(),
                opaque_schema: descriptor.opaque_schema,
                opaque_schema_version: descriptor.opaque_schema_version,
                renderer_contribution_id: descriptor.renderer_contribution_id.clone(),
            },
            expected_sort: descriptor.expected_sort,
            contribution_kind: descriptor.kind,
            opaque_schema: descriptor.opaque_schema,
            opaque_schema_version: descriptor.opaque_schema_version,
            filesystem_read_authorized: descriptor.filesystem_read_authorized,
            lock_owner_query_authorized: descriptor.lock_owner_query_authorized,
            _lease: Some(lease),
        })
    }
    #[must_use]
    pub fn producer(&self) -> &ExtensionJobProducerV1 {
        &self.producer
    }

    pub(crate) fn with_lock_owner_query_for_direct_loader(mut self) -> Self {
        self.lock_owner_query_authorized = true;
        self
    }

    #[cfg(any(test, feature = "integration-test-support"))]
    fn test_authority(package_id: &str) -> Self {
        Self {
            producer: ExtensionJobProducerV1 {
                package_id: package_id.to_owned(),
                sealed_manifest_digest: "test-digest".to_owned(),
                data_version: 1,
                contribution_id: "test-provider".to_owned(),
                interface_id: StableIdV1::new(explorer_extension_api::IdNamespaceV1::new(1, 1), 1),
                feature_id: "test-feature".to_owned(),
                feature_epoch: 1,
                opaque_schema: None,
                opaque_schema_version: None,
                renderer_contribution_id: None,
            },
            expected_sort: ROption::RNone,
            contribution_kind: ContributionKindV1::Column,
            opaque_schema: None,
            opaque_schema_version: None,
            filesystem_read_authorized: false,
            lock_owner_query_authorized: false,
            _lease: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn for_test(package_id: &str) -> Self {
        Self::test_authority(package_id)
    }

    /// Test-only authority for an application integration fixture compiled
    /// with the non-default `integration-test-support` feature.
    #[cfg(feature = "integration-test-support")]
    #[must_use]
    pub fn for_integration_test(package_id: &str) -> Self {
        Self::test_authority(package_id)
    }

    #[cfg(test)]
    pub(crate) fn for_test_opaque(
        package_id: &str,
        contribution_id: &str,
        kind: ContributionKindV1,
        schema: StableIdV1,
        schema_version: u32,
        renderer_contribution_id: Option<&str>,
    ) -> Self {
        let mut authority = Self::for_test(package_id);
        authority.producer.contribution_id = contribution_id.to_owned();
        authority.producer.opaque_schema = Some(schema);
        authority.producer.opaque_schema_version = Some(schema_version);
        authority.producer.renderer_contribution_id = renderer_contribution_id.map(str::to_owned);
        authority.contribution_kind = kind;
        authority.opaque_schema = Some(schema);
        authority.opaque_schema_version = Some(schema_version);
        authority
    }

    #[cfg(test)]
    pub(crate) fn with_filesystem_read_for_test(mut self) -> Self {
        self.filesystem_read_authorized = true;
        self
    }
}

/// Host-opened, path-free decoder source. This compact V1 seam deliberately
/// owns an immutable byte snapshot rather than a plugin-selected filesystem
/// path or native handle; replacing it advances the authoritative generation.
pub const MAX_HOST_INPUT_STREAM_SOURCE_BYTES_V1: usize = 8 * 1024 * 1024;
/// Aggregate input ceiling for one bounded batch callback.
pub const MAX_BATCH_COLUMN_INPUT_BYTES_V1: usize = 64 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct HostInputStreamSourceV1 {
    state: Arc<Mutex<HostInputStreamStateV1>>,
}

#[derive(Debug)]
struct HostInputStreamStateV1 {
    bytes: Arc<[u8]>,
    generation: u64,
    seekable: bool,
    generation_token: ExtensionValueGenerationV1,
}

impl HostInputStreamSourceV1 {
    fn same_weak_state(&self, other: &Weak<Mutex<HostInputStreamStateV1>>) -> bool {
        Arc::as_ptr(&self.state) == other.as_ptr()
    }

    fn downgrade(&self) -> Weak<Mutex<HostInputStreamStateV1>> {
        Arc::downgrade(&self.state)
    }

    fn matches_generation(&self, generation: u64) -> bool {
        self.state
            .lock()
            .is_ok_and(|state| state.generation == generation)
    }

    fn byte_len(&self) -> Option<usize> {
        self.state.lock().ok().map(|state| state.bytes.len())
    }

    fn generation_token(&self, generation: u64) -> Option<ExtensionValueGenerationV1> {
        self.state.lock().ok().and_then(|state| {
            (state.generation == generation).then(|| state.generation_token.clone())
        })
    }

    /// Creates a host-attested source snapshot. The caller has already opened
    /// and identity-checked the item; paths and OS handles never enter this
    /// type or the extension ABI.
    #[must_use]
    pub fn from_host_snapshot(bytes: Vec<u8>, generation: u64, seekable: bool) -> Option<Self> {
        (generation != 0 && bytes.len() <= MAX_HOST_INPUT_STREAM_SOURCE_BYTES_V1).then(|| Self {
            state: Arc::new(Mutex::new(HostInputStreamStateV1 {
                bytes: bytes.into(),
                generation,
                seekable,
                generation_token: ExtensionValueGenerationV1::current(),
            })),
        })
    }

    /// Replaces an equally sized source after host re-attestation. The old
    /// token is revoked before new bytes are installed, so any row/batch that
    /// raced this update becomes inert even after it has escaped a runtime
    /// generation check. Size changes require a new source/admission.
    pub fn replace_host_snapshot(&self, bytes: Vec<u8>, generation: u64, seekable: bool) -> bool {
        if generation == 0 || bytes.len() > MAX_HOST_INPUT_STREAM_SOURCE_BYTES_V1 {
            return false;
        }
        let Ok(mut state) = self.state.lock() else {
            return false;
        };
        if generation <= state.generation || bytes.len() != state.bytes.len() {
            return false;
        }
        state.generation_token.revoke();
        state.bytes = bytes.into();
        state.generation = generation;
        state.seekable = seekable;
        state.generation_token = ExtensionValueGenerationV1::current();
        true
    }
}

/// Input supplied only by validated host registration code.
#[derive(Debug)]
pub struct ExtensionJobRuntimeRequestV1 {
    pub authority: ExtensionJobAuthorityV1,
    pub job_generation: u64,
    pub item_generation: u64,
    pub location_generation: u64,
    pub source_generation: u64,
    pub has_item: bool,
    pub input_stream: Option<HostInputStreamSourceV1>,
}

/// One ordinary-file input selected by the host for a batch column callback.
///
/// Construction is deliberately host-only in practice: the source itself can
/// only be minted from a size-limited host snapshot and is later wrapped in an
/// opaque `InputStreamV1` service.
#[derive(Clone, Debug)]
pub struct HostBatchColumnItemV1 {
    /// Host-attested basename only; never a path.
    pub file_name: RString,
    pub source: HostInputStreamSourceV1,
    pub cache_identity: RString,
    pub modified_unix_seconds: ROption<u64>,
    pub modified_subsec_nanos: u32,
    pub source_size: ROption<u64>,
    /// Optional host-only filesystem identity for discover-only lock queries.
    /// It is mapped to the opaque item handle and never copied across the ABI.
    pub lock_owner_resource: Option<PathBuf>,
}

/// Host composition seam for discover-only lock-owner queries.
#[derive(Clone)]
pub struct HostLockOwnerQueryServiceV1 {
    query: Arc<HostLockOwnerQueryFnV1>,
}

type HostLockOwnerQueryFnV1 =
    dyn Fn(&PathBuf, u32) -> (LockOwnerQueryStatusV1, Vec<LockOwnerRecordV1>) + Send + Sync;

impl fmt::Debug for HostLockOwnerQueryServiceV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HostLockOwnerQueryServiceV1")
            .finish_non_exhaustive()
    }
}

impl HostLockOwnerQueryServiceV1 {
    #[must_use]
    pub fn new(
        query: impl Fn(&PathBuf, u32) -> (LockOwnerQueryStatusV1, Vec<LockOwnerRecordV1>)
        + Send
        + Sync
        + 'static,
    ) -> Self {
        Self {
            query: Arc::new(query),
        }
    }
}

/// Host-owned request for one Rust batch-column provider invocation.
#[derive(Debug)]
pub struct BatchColumnRuntimeRequestV1 {
    pub authority: ExtensionJobAuthorityV1,
    pub job_generation: u64,
    pub item_generation: u64,
    pub location_generation: u64,
    pub source_generation: u64,
    pub items: Vec<HostBatchColumnItemV1>,
    pub lock_owner_query: Option<HostLockOwnerQueryServiceV1>,
}

/// Deep-copied host result. Producer identity never originates from plugin bytes.
#[derive(Clone, Debug)]
pub struct AcceptedIncrementalResultBatchV1 {
    pub producer: ExtensionJobProducerV1,
    pub job: JobHandleV1,
    pub sequence: u64,
    pub job_generation: u64,
    pub item_generation: u64,
    pub location_generation: u64,
    pub source_generation: u64,
    generation: ExtensionValueGenerationV1,
    cache_bytes: usize,
    entries: Vec<HostIncrementalResultEntryV1>,
}

impl AcceptedIncrementalResultBatchV1 {
    /// Internal projection step used only by the runtime's final apply gate.
    /// Public consumers receive an accepted batch as an opaque apply token;
    /// they cannot construct rows before current-generation, exactly-once, and
    /// retained-row budget checks succeed together.
    pub(crate) fn project_rows(
        &self,
        mut host_identity: impl FnMut(usize) -> (String, u128),
    ) -> Vec<ExtensionValueRowV1> {
        self.entries
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                let (display_name, stable_item_identity) = host_identity(index);
                ExtensionValueRowV1::from_host_with_generation(
                    entry.result.clone(),
                    display_name,
                    stable_item_identity,
                    self.generation.clone(),
                )
            })
            .collect()
    }

    /// The number of ABI entries accepted into this opaque host batch.
    pub(crate) const fn entry_count(&self) -> usize {
        self.entries.len()
    }

    pub(crate) fn cache_payload_parts(
        &self,
    ) -> (
        ExtensionJobProducerV1,
        Vec<HostIncrementalResultEntryV1>,
        usize,
    ) {
        (
            self.producer.clone(),
            self.entries.clone(),
            self.cache_bytes,
        )
    }

    pub(crate) fn cache_generation_parts(&self) -> (u64, u64, u64) {
        (
            self.item_generation,
            self.location_generation,
            self.source_generation,
        )
    }

    pub(crate) const fn cache_job(&self) -> JobHandleV1 {
        self.job
    }

    pub(crate) fn cache_tombstone(&self) -> ExtensionValueGenerationV1 {
        self.generation.clone()
    }
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
    result_cache: Arc<ExtensionResultCacheV1>,
    runtime_authority: Option<Arc<RuntimeAuthorityV1>>,
}

/// Runtime-integrated cache result. A hit has already been rebound to the
/// current job's lifecycle tombstone; a miss admission can only populate the
/// same cache scope if no invalidation races the provider completion.
#[derive(Clone, Debug)]
pub enum ExtensionJobCacheLookupV1 {
    Hit(Vec<ExtensionValueRowV1>),
    Miss(Box<ExtensionResultCacheAdmissionV1>),
    RejectedStale,
}

#[derive(Debug)]
struct RuntimeStateV1 {
    config: ExtensionResultBufferConfigV1,
    jobs: HashMap<JobHandleV1, RuntimeJobV1>,
    applied_rows: HashMap<JobHandleV1, Vec<ExtensionValueRowV1>>,
    applied_batches: HashSet<AppliedBatchKeyV1>,
    applied_row_count: usize,
    queued_batches: usize,
    queued_items: usize,
    queued_bytes: usize,
    queued_per_package: HashMap<String, QueueUsageV1>,
    active_jobs_per_package: HashMap<String, usize>,
    input_stream_bytes: usize,
    input_stream_bytes_per_package: HashMap<String, usize>,
    accounting_healthy: bool,
    quarantines: VecDeque<ExtensionJobQuarantineEventV1>,
    revoked_producers: BTreeMap<ProducerGenerationKeyV1, ProducerRevocationReasonV1>,
    /// Lifecycle-local weak tombstones keep rows invalidatable after their
    /// originating job retires. Dead and already-revoked tokens are pruned on
    /// every insert/revoke, so this remains bounded by live current jobs.
    generation_tombstones:
        BTreeMap<ProducerGenerationKeyV1, Vec<Weak<ExtensionValueGenerationStateV1>>>,
    ready_signal_sink: Option<RuntimeReadySignalSinkV1>,
    active_provider_threads: HashSet<ThreadId>,
    #[cfg(test)]
    panic_next_progress_submit: bool,
}

/// A retained host batch may be applied once only.  This key deliberately
/// includes every host generation, so a recycled handle cannot inherit a
/// previous UI publication decision.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct AppliedBatchKeyV1 {
    job: JobHandleV1,
    sequence: u64,
    job_generation: u64,
    item_generation: u64,
    location_generation: u64,
    source_generation: u64,
}

impl From<&AcceptedIncrementalResultBatchV1> for AppliedBatchKeyV1 {
    fn from(batch: &AcceptedIncrementalResultBatchV1) -> Self {
        Self {
            job: batch.job,
            sequence: batch.sequence,
            job_generation: batch.job_generation,
            item_generation: batch.item_generation,
            location_generation: batch.location_generation,
            source_generation: batch.source_generation,
        }
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct ProducerGenerationKeyV1 {
    package_id: String,
    sealed_manifest_digest: String,
    feature_id: String,
    feature_epoch: u64,
}

/// Host-only reason that a producer generation can no longer expose results.
/// The distinction keeps lifecycle cancellation from looking like malformed ABI
/// traffic, while marker failure remains fail-closed and traceable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProducerRevocationReasonV1 {
    LifecycleCancelled,
    ProtocolViolation,
    MarkerFailure,
}

impl From<&ExtensionJobProducerV1> for ProducerGenerationKeyV1 {
    fn from(producer: &ExtensionJobProducerV1) -> Self {
        Self {
            package_id: producer.package_id.clone(),
            sealed_manifest_digest: producer.sealed_manifest_digest.clone(),
            feature_id: producer.feature_id.clone(),
            feature_epoch: producer.feature_epoch,
        }
    }
}

/// Bounded diagnostic fact for a generation rejected after malformed transport.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExtensionJobQuarantineEventV1 {
    pub producer: ExtensionJobProducerV1,
    /// Full opaque handle retains the nonce and generation without exposing raw bytes.
    pub job: JobHandleV1,
    pub item: Option<ItemHandleV1>,
    pub location: LocationHandleV1,
    pub job_generation: u64,
    pub item_generation: u64,
    pub location_generation: u64,
    pub source_generation: u64,
}
const MAX_QUARANTINE_EVENTS_V1: usize = 128;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct QueueUsageV1 {
    batches: usize,
    items: usize,
    bytes: usize,
}

fn remove_applied_rows(state: &mut RuntimeStateV1, job: JobHandleV1) {
    if let Some(rows) = state.applied_rows.remove(&job) {
        let Some(next) = state.applied_row_count.checked_sub(rows.len()) else {
            state.accounting_healthy = false;
            return;
        };
        state.applied_row_count = next;
    }
    state.applied_batches.retain(|key| key.job != job);
}

#[derive(Debug)]
struct RuntimeJobV1 {
    authority: ExtensionJobAuthorityV1,
    /// Claimed by the first synchronous provider call, not by scheduling.
    /// A scheduler may prepare on one host thread and invoke on another.
    owner_thread: Option<ThreadId>,
    item: Option<ItemHandleV1>,
    location: LocationHandleV1,
    job_generation: u64,
    item_generation: u64,
    location_generation: u64,
    source_generation: u64,
    input_stream: Option<HostInputStreamSourceV1>,
    batch_input_streams: Vec<RuntimeBatchInputV1>,
    /// A batch-column callback produces one complete, host-ordered outcome
    /// record.  This keeps the direct one-plugin application path from ever
    /// associating a valid result with the wrong visible file.
    batch_result_submitted: bool,
    input_stream_bytes: usize,
    expected_sort: ROption<StableSortValueKindV1>,
    /// Shared by every host row published from this job. A revoked generation
    /// stays invalid even after this job is retired and its registry entry is
    /// gone.
    value_generation: ExtensionValueGenerationV1,
    sink_capability: SinkCapabilityV1,
    gate: Arc<InvocationGateV1>,
    provider_call_active: bool,
    finalization: JobFinalizationV1,
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

#[derive(Clone, Debug)]
struct RuntimeBatchInputV1 {
    item: ItemHandleV1,
    file_name: RString,
    source: HostInputStreamSourceV1,
    cache_identity: RString,
    modified_unix_seconds: ROption<u64>,
    modified_subsec_nanos: u32,
    source_size: ROption<u64>,
    lock_owner_resource: Option<PathBuf>,
}

#[derive(Clone, Debug)]
struct HostLockOwnerQueryAdapterV1 {
    state: Weak<Mutex<RuntimeStateV1>>,
    job: JobHandleV1,
    item_generation: u64,
    location_generation: u64,
    resources: Arc<Vec<(ItemHandleV1, PathBuf)>>,
    service: HostLockOwnerQueryServiceV1,
    runtime_authority: Arc<RuntimeAuthorityV1>,
    authority: AuthorityEnvelopeV1,
}

impl AbiLockOwnerQueryServiceV1 for HostLockOwnerQueryAdapterV1 {
    fn query(&self, request: LockOwnerQueryRequestV1) -> LockOwnerQueryOutcomeV1 {
        let empty = |status| LockOwnerQueryOutcomeV1 {
            status,
            reserved: 0,
            item_generation: request.item_generation,
            location_generation: request.location_generation,
            owners: RVec::new(),
        };
        if self
            .runtime_authority
            .revalidate(&self.authority, AuthorityAdapterV1::LockOwner)
            .is_err()
        {
            return empty(LockOwnerQueryStatusV1::CANCELLED);
        }
        if request.items.is_empty()
            || request.items.len() > explorer_extension_api::MAX_LOCK_OWNER_QUERY_ITEMS_V1
            || request.item_generation != self.item_generation
            || request.location_generation != self.location_generation
        {
            return empty(LockOwnerQueryStatusV1::UNAVAILABLE);
        }
        let Some(state) = self.state.upgrade() else {
            return empty(LockOwnerQueryStatusV1::CANCELLED);
        };
        let Ok(state) = state.lock() else {
            return empty(LockOwnerQueryStatusV1::HOST_ERROR);
        };
        let Some(job) = state.jobs.get(&self.job) else {
            return empty(LockOwnerQueryStatusV1::CANCELLED);
        };
        if job.terminal.is_some() || job.control != JobControlStateV1::ACTIVE {
            return empty(LockOwnerQueryStatusV1::CANCELLED);
        }
        drop(state);
        let mut resolved = Vec::with_capacity(request.items.len());
        for handle in &request.items {
            let Some((_, path)) = self.resources.iter().find(|(item, _)| item == handle) else {
                return empty(LockOwnerQueryStatusV1::UNAVAILABLE);
            };
            resolved.push((*handle, path.clone()));
        }
        let started = std::time::Instant::now();
        let mut status = LockOwnerQueryStatusV1::EMPTY;
        let mut owners = Vec::new();
        for (item, path) in resolved {
            let (item_status, mut item_owners) =
                (self.service.query)(&path, request.deadline_millis);
            if item_status == LockOwnerQueryStatusV1::READY {
                status = LockOwnerQueryStatusV1::READY;
            } else if item_status != LockOwnerQueryStatusV1::EMPTY
                && status != LockOwnerQueryStatusV1::READY
            {
                status = item_status;
            }
            for owner in &mut item_owners {
                owner.item = item;
                truncate_utf8_rstring(
                    &mut owner.display_name,
                    explorer_extension_api::MAX_LOCK_OWNER_DISPLAY_NAME_BYTES_V1,
                );
                truncate_utf8_rstring(
                    &mut owner.service_name,
                    explorer_extension_api::MAX_LOCK_OWNER_DISPLAY_NAME_BYTES_V1,
                );
            }
            owners.extend(item_owners);
        }
        if request.deadline_millis != 0
            && started.elapsed()
                > std::time::Duration::from_millis(u64::from(request.deadline_millis))
        {
            return empty(LockOwnerQueryStatusV1::DEADLINE_ELAPSED);
        }
        owners.truncate(explorer_extension_api::MAX_LOCK_OWNER_QUERY_RESULTS_V1);
        LockOwnerQueryOutcomeV1 {
            status,
            reserved: 0,
            item_generation: request.item_generation,
            location_generation: request.location_generation,
            owners: owners.into(),
        }
    }
}

fn truncate_utf8_rstring(value: &mut RString, maximum_bytes: usize) {
    if value.len() <= maximum_bytes {
        return;
    }
    let text = value.as_str();
    let mut boundary = maximum_bytes.min(text.len());
    while boundary > 0 && !text.is_char_boundary(boundary) {
        boundary -= 1;
    }
    *value = RString::from(&text[..boundary]);
}

/// Per-invocation gate retained by the Rust-ABI host-services object. Clones
/// of `JobContextV1` stay inert after callback return without depending on TLS.
#[derive(Debug)]
struct InvocationGateV1 {
    active: AtomicBool,
    owner_thread: Mutex<Option<ThreadId>>,
}

impl InvocationGateV1 {
    fn activate(&self) -> bool {
        let Ok(mut owner) = self.owner_thread.lock() else {
            return false;
        };
        if self.active.load(Ordering::Acquire) || owner.is_some() {
            return false;
        }
        *owner = Some(std::thread::current().id());
        self.active.store(true, Ordering::Release);
        true
    }

    fn deactivate(&self) {
        self.active.store(false, Ordering::Release);
        if let Ok(mut owner) = self.owner_thread.lock() {
            *owner = None;
        }
    }

    fn state(&self) -> InvocationGateStateV1 {
        if !self.active.load(Ordering::Acquire) {
            return InvocationGateStateV1::Closed;
        }
        let Ok(owner) = self.owner_thread.lock() else {
            return InvocationGateStateV1::Closed;
        };
        match *owner {
            Some(owner) if owner == std::thread::current().id() => InvocationGateStateV1::Active,
            Some(_) => InvocationGateStateV1::WrongThread,
            None => InvocationGateStateV1::Closed,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InvocationGateStateV1 {
    Active,
    WrongThread,
    Closed,
}

#[derive(Clone, Debug)]
struct HostJobServicesAdapterV1 {
    state: Weak<Mutex<RuntimeStateV1>>,
    job: JobHandleV1,
    capability: SinkCapabilityV1,
    invocation: InvocationGenerationsV1,
    gate: Arc<InvocationGateV1>,
}

/// Immutable generation snapshot attached to a host-minted callback context.
/// A retained plugin service must never be able to follow a runtime job into a
/// newer item/location/source generation by forging public batch fields.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct InvocationGenerationsV1 {
    job: u64,
    item: u64,
    location: u64,
    source: u64,
}

impl InvocationGenerationsV1 {
    fn matches(self, job: &RuntimeJobV1) -> bool {
        self.job == job.job_generation
            && self.item == job.item_generation
            && self.location == job.location_generation
            && self.source == job.source_generation
    }
}

impl HostJobServicesAdapterV1 {
    fn poll_control_inner(&self) -> JobControlStateV1 {
        if self.gate.state() != InvocationGateStateV1::Active {
            return JobControlStateV1::CLOSED;
        }
        let Some(state) = self.state.upgrade() else {
            return JobControlStateV1::CLOSED;
        };
        let Ok(state) = state.lock() else {
            return JobControlStateV1::CLOSED;
        };
        state
            .jobs
            .get(&self.job)
            .map_or(JobControlStateV1::CLOSED, |job| {
                let source_current = self.invocation.matches(job)
                    && job.value_generation.is_current()
                    && job_sources_current(job);
                if job.sink_capability == self.capability
                    && job.provider_call_active
                    && source_current
                {
                    job.control
                } else {
                    JobControlStateV1::CLOSED
                }
            })
    }

    fn submit_results_decision(&self, batch: &IncrementalResultBatchV1) -> SubmitDecisionV1 {
        match self.gate.state() {
            InvocationGateStateV1::Closed => {
                return SubmitDecisionV1::from_credits(SinkSubmitStatusV1::CLOSED, (0, 0, 0));
            }
            InvocationGateStateV1::WrongThread => {
                return SubmitDecisionV1::from_credits(SinkSubmitStatusV1::WRONG_THREAD, (0, 0, 0));
            }
            InvocationGateStateV1::Active => {}
        }
        if batch.job != self.job || batch.sink_capability != self.capability {
            return SubmitDecisionV1::from_credits(SinkSubmitStatusV1::STALE, (0, 0, 0));
        }
        let Some(state) = self.state.upgrade() else {
            return SubmitDecisionV1::from_credits(SinkSubmitStatusV1::CLOSED, (0, 0, 0));
        };
        let Ok(mut state) = state.lock() else {
            return SubmitDecisionV1::from_credits(SinkSubmitStatusV1::CLOSED, (0, 0, 0));
        };
        let decision = submit_locked(&mut state, batch, self.invocation);
        let ready_sink = (decision.status == SinkSubmitStatusV1::ACCEPTED)
            .then(|| state.ready_signal_sink.clone())
            .flatten();
        drop(state);
        if let (Some(sink), Some(signal)) = (ready_sink, decision.ready_signal) {
            sink.signal(signal);
        }
        decision
    }

    fn submit_progress_inner(&self, update: JobProgressUpdateV1) -> JobProgressStatusV1 {
        match self.gate.state() {
            InvocationGateStateV1::Closed => return JobProgressStatusV1::CLOSED,
            InvocationGateStateV1::WrongThread => return JobProgressStatusV1::WRONG_THREAD,
            InvocationGateStateV1::Active => {}
        }
        if update.job != self.job || update.sink_capability != self.capability {
            return JobProgressStatusV1::STALE;
        }
        let Some(state) = self.state.upgrade() else {
            return JobProgressStatusV1::CLOSED;
        };
        submit_progress_for_state(&state, update, self.invocation)
    }
}

impl AbiJobHostServicesV1 for HostJobServicesAdapterV1 {
    fn poll_control(&self) -> JobControlStateV1 {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.poll_control_inner()))
            .unwrap_or(JobControlStateV1::CLOSED)
    }

    fn submit_results(&self, batch: IncrementalResultBatchV1) -> SinkSubmitOutcomeV1 {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.submit_results_decision(&batch)
        })) {
            Ok(decision) => decision.into_outcome(batch),
            Err(_) => rejected(SinkSubmitStatusV1::CLOSED, batch, 0, 0, 0),
        }
    }

    fn submit_progress(&self, update: JobProgressUpdateV1) -> JobProgressStatusV1 {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.submit_progress_inner(update)
        }))
        .unwrap_or(JobProgressStatusV1::CLOSED)
    }
}

/// Per-invocation, host-minted stream service. The source is an attested host
/// snapshot; all registry and source locks are released before copying bytes
/// into ABI-owned `RVec` memory.
#[derive(Clone, Debug)]
struct HostInputStreamServicesAdapterV1 {
    state: Weak<Mutex<RuntimeStateV1>>,
    job: JobHandleV1,
    gate: Arc<InvocationGateV1>,
    source: Weak<Mutex<HostInputStreamStateV1>>,
    expected_source_generation: u64,
    position: Arc<Mutex<u64>>,
}

impl HostInputStreamServicesAdapterV1 {
    fn current_status(&self) -> Result<(), InputStreamStatusV1> {
        match self.gate.state() {
            InvocationGateStateV1::Active => {}
            InvocationGateStateV1::WrongThread => return Err(InputStreamStatusV1::WRONG_THREAD),
            InvocationGateStateV1::Closed => return Err(InputStreamStatusV1::CLOSED),
        }
        let Some(state) = self.state.upgrade() else {
            return Err(InputStreamStatusV1::CLOSED);
        };
        let Ok(state) = state.lock() else {
            return Err(InputStreamStatusV1::CLOSED);
        };
        let Some(job) = state.jobs.get(&self.job) else {
            return Err(InputStreamStatusV1::CLOSED);
        };
        if !job.provider_call_active
            || job.source_generation != self.expected_source_generation
            || !job_source_matches(job, &self.source)
        {
            return Err(InputStreamStatusV1::STALE);
        }
        match job.control {
            control if control == JobControlStateV1::ACTIVE => {}
            control if control == JobControlStateV1::CANCELLED => {
                return Err(InputStreamStatusV1::CANCELLED);
            }
            control if control == JobControlStateV1::DEADLINE_ELAPSED => {
                return Err(InputStreamStatusV1::DEADLINE_ELAPSED);
            }
            _ => return Err(InputStreamStatusV1::CLOSED),
        }
        let Some(source) = self.source.upgrade() else {
            return Err(InputStreamStatusV1::CLOSED);
        };
        let Ok(source) = source.lock() else {
            return Err(InputStreamStatusV1::CLOSED);
        };
        if source.generation != self.expected_source_generation {
            return Err(InputStreamStatusV1::STALE);
        }
        Ok(())
    }

    fn source_snapshot(&self) -> Result<(Arc<[u8]>, bool), InputStreamStatusV1> {
        self.current_status()?;
        let Some(source) = self.source.upgrade() else {
            return Err(InputStreamStatusV1::CLOSED);
        };
        let Ok(source) = source.lock() else {
            return Err(InputStreamStatusV1::CLOSED);
        };
        if source.generation != self.expected_source_generation {
            return Err(InputStreamStatusV1::STALE);
        }
        Ok((Arc::clone(&source.bytes), source.seekable))
    }

    const fn read_outcome(
        status: InputStreamStatusV1,
        source_generation: u64,
        position: u64,
        data: RVec<u8>,
    ) -> InputStreamReadOutcomeV1 {
        InputStreamReadOutcomeV1 {
            status,
            reserved: 0,
            source_generation,
            position,
            data,
        }
    }

    const fn seek_outcome(
        status: InputStreamStatusV1,
        source_generation: u64,
        position: u64,
    ) -> InputStreamSeekOutcomeV1 {
        InputStreamSeekOutcomeV1 {
            status,
            reserved: 0,
            source_generation,
            position,
        }
    }

    const fn length_outcome(
        status: InputStreamStatusV1,
        source_generation: u64,
        length: u64,
    ) -> InputStreamLengthOutcomeV1 {
        InputStreamLengthOutcomeV1 {
            status,
            reserved: 0,
            source_generation,
            length,
        }
    }

    fn read_inner(&self, request: InputStreamReadRequestV1) -> InputStreamReadOutcomeV1 {
        let position = self.position.lock().map_or(0, |position| *position);
        if request.reserved != 0 || request.maximum_bytes > MAX_INPUT_STREAM_READ_BYTES_V1 {
            return Self::read_outcome(
                InputStreamStatusV1::INVALID,
                self.expected_source_generation,
                position,
                RVec::new(),
            );
        }
        let (bytes, _) = match self.source_snapshot() {
            Ok(snapshot) => snapshot,
            Err(status) => {
                return Self::read_outcome(
                    status,
                    self.expected_source_generation,
                    position,
                    RVec::new(),
                );
            }
        };
        if request.maximum_bytes == 0 {
            return Self::read_outcome(
                InputStreamStatusV1::OK,
                self.expected_source_generation,
                position,
                RVec::new(),
            );
        }
        let length = bytes.len() as u64;
        if position >= length {
            return Self::read_outcome(
                InputStreamStatusV1::EOF,
                self.expected_source_generation,
                position,
                RVec::new(),
            );
        }
        let remaining = length.saturating_sub(position);
        let requested = u64::from(request.maximum_bytes).min(remaining);
        let Ok(start) = usize::try_from(position) else {
            return Self::read_outcome(
                InputStreamStatusV1::INVALID,
                self.expected_source_generation,
                position,
                RVec::new(),
            );
        };
        let Ok(end) = usize::try_from(position.saturating_add(requested)) else {
            return Self::read_outcome(
                InputStreamStatusV1::INVALID,
                self.expected_source_generation,
                position,
                RVec::new(),
            );
        };
        // Copy after source lock release, then recheck cancellation/deadline and
        // source generation before committing the cursor or returning bytes.
        let data = RVec::from(bytes[start..end].to_vec());
        if let Err(status) = self.current_status() {
            return Self::read_outcome(
                status,
                self.expected_source_generation,
                position,
                RVec::new(),
            );
        }
        let Ok(mut cursor) = self.position.lock() else {
            return Self::read_outcome(
                InputStreamStatusV1::CLOSED,
                self.expected_source_generation,
                position,
                RVec::new(),
            );
        };
        if *cursor != position {
            return Self::read_outcome(
                InputStreamStatusV1::STALE,
                self.expected_source_generation,
                position,
                RVec::new(),
            );
        }
        *cursor = position.saturating_add(requested);
        Self::read_outcome(
            InputStreamStatusV1::OK,
            self.expected_source_generation,
            *cursor,
            data,
        )
    }

    fn seek_inner(&self, request: InputStreamSeekRequestV1) -> InputStreamSeekOutcomeV1 {
        let current = self.position.lock().map_or(0, |position| *position);
        if request.reserved != 0 {
            return Self::seek_outcome(
                InputStreamStatusV1::INVALID,
                self.expected_source_generation,
                current,
            );
        }
        let (bytes, seekable) = match self.source_snapshot() {
            Ok(snapshot) => snapshot,
            Err(status) => {
                return Self::seek_outcome(status, self.expected_source_generation, current);
            }
        };
        if !seekable {
            return Self::seek_outcome(
                InputStreamStatusV1::UNSUPPORTED,
                self.expected_source_generation,
                current,
            );
        }
        let base = match request.origin {
            origin if origin == InputStreamSeekOriginV1::START => 0_i128,
            origin if origin == InputStreamSeekOriginV1::CURRENT => i128::from(current),
            origin if origin == InputStreamSeekOriginV1::END => bytes.len() as i128,
            _ => {
                return Self::seek_outcome(
                    InputStreamStatusV1::INVALID,
                    self.expected_source_generation,
                    current,
                );
            }
        };
        let target = base.checked_add(i128::from(request.offset));
        let Some(target) = target.filter(|target| *target >= 0 && *target <= bytes.len() as i128)
        else {
            return Self::seek_outcome(
                InputStreamStatusV1::INVALID,
                self.expected_source_generation,
                current,
            );
        };
        if let Err(status) = self.current_status() {
            return Self::seek_outcome(status, self.expected_source_generation, current);
        }
        let Ok(target) = u64::try_from(target) else {
            return Self::seek_outcome(
                InputStreamStatusV1::INVALID,
                self.expected_source_generation,
                current,
            );
        };
        let Ok(mut position) = self.position.lock() else {
            return Self::seek_outcome(
                InputStreamStatusV1::CLOSED,
                self.expected_source_generation,
                current,
            );
        };
        *position = target;
        Self::seek_outcome(
            InputStreamStatusV1::OK,
            self.expected_source_generation,
            target,
        )
    }

    fn length_inner(&self) -> InputStreamLengthOutcomeV1 {
        match self.source_snapshot() {
            Ok((bytes, _)) => Self::length_outcome(
                InputStreamStatusV1::OK,
                self.expected_source_generation,
                bytes.len() as u64,
            ),
            Err(status) => Self::length_outcome(status, self.expected_source_generation, 0),
        }
    }
}

impl AbiInputStreamServicesV1 for HostInputStreamServicesAdapterV1 {
    fn read(&self, request: InputStreamReadRequestV1) -> InputStreamReadOutcomeV1 {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.read_inner(request)))
            .unwrap_or_else(|_| {
                Self::read_outcome(
                    InputStreamStatusV1::CLOSED,
                    self.expected_source_generation,
                    0,
                    RVec::new(),
                )
            })
    }

    fn seek(&self, request: InputStreamSeekRequestV1) -> InputStreamSeekOutcomeV1 {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.seek_inner(request)))
            .unwrap_or_else(|_| {
                Self::seek_outcome(
                    InputStreamStatusV1::CLOSED,
                    self.expected_source_generation,
                    0,
                )
            })
    }

    fn length(&self) -> InputStreamLengthOutcomeV1 {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.length_inner()))
            .unwrap_or_else(|_| {
                Self::length_outcome(
                    InputStreamStatusV1::CLOSED,
                    self.expected_source_generation,
                    0,
                )
            })
    }
}

/// Lifecycle state of the host-native marker boundary. Terminal state alone
/// cannot release the dispatch lease while marker persistence is unresolved.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum JobFinalizationV1 {
    /// No callback marker exists; an inactive lifecycle revoke can retire.
    Idle,
    /// Callback entry created a marker; wait for clear or fail-close.
    MarkerPending,
    /// Durable marker clear/fail-close (or pre-callback revoke) permits retire.
    RetirementAuthorized,
}

#[derive(Debug)]
struct StoredBatchV1 {
    sequence: u64,
    job_generation: u64,
    item_generation: u64,
    location_generation: u64,
    source_generation: u64,
    bytes: usize,
    entries: Vec<HostIncrementalResultEntryV1>,
}

#[allow(clippy::missing_errors_doc)]
impl ExtensionJobRuntimeV1 {
    /// Creates an empty runtime result registry.
    #[must_use]
    pub fn new(config: ExtensionResultBufferConfigV1) -> Self {
        Self::new_with_result_cache(
            config,
            Arc::new(ExtensionResultCacheV1::new(
                ExtensionResultCacheConfigV1::host_default(),
            )),
        )
    }

    fn new_with_result_cache(
        config: ExtensionResultBufferConfigV1,
        result_cache: Arc<ExtensionResultCacheV1>,
    ) -> Self {
        Self {
            state: Arc::new(Mutex::new(RuntimeStateV1 {
                config,
                jobs: HashMap::new(),
                applied_rows: HashMap::new(),
                applied_batches: HashSet::new(),
                applied_row_count: 0,
                queued_batches: 0,
                queued_items: 0,
                queued_bytes: 0,
                queued_per_package: HashMap::new(),
                active_jobs_per_package: HashMap::new(),
                input_stream_bytes: 0,
                input_stream_bytes_per_package: HashMap::new(),
                accounting_healthy: true,
                quarantines: VecDeque::new(),
                revoked_producers: BTreeMap::new(),
                generation_tombstones: BTreeMap::new(),
                ready_signal_sink: None,
                active_provider_threads: HashSet::new(),
                #[cfg(test)]
                panic_next_progress_submit: false,
            })),
            result_cache,
            runtime_authority: RuntimeAuthorityV1::new().ok().map(Arc::new),
        }
    }

    /// Returns the canonical host-owned result cache attached to this runtime.
    #[must_use]
    pub fn result_cache(&self) -> Arc<ExtensionResultCacheV1> {
        Arc::clone(&self.result_cache)
    }

    #[must_use]
    pub fn cache_watcher_generation(&self, watcher_scope: u128) -> u64 {
        self.result_cache.watcher_generation(watcher_scope)
    }

    /// Looks up a host-minted cache key for an exact current job generation.
    /// Cached entries are copied/rebound under a fresh runtime-current
    /// tombstone, so neither an old row nor a revoked feature can publish.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn lookup_cached_result(
        &self,
        job: JobHandleV1,
        file: ExtensionResultCacheFileFactV1,
        option_hash: [u8; 32],
        watcher_scope: u128,
        watcher_generation: u64,
        recursive: bool,
        item_generation: u64,
        location_generation: u64,
        source_generation: u64,
        host_identity: impl FnMut(usize) -> (String, u128),
    ) -> ExtensionJobCacheLookupV1 {
        self.lookup_cached_result_at(
            job,
            file,
            option_hash,
            watcher_scope,
            watcher_generation,
            recursive,
            item_generation,
            location_generation,
            source_generation,
            std::time::Instant::now(),
            host_identity,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn lookup_cached_result_at(
        &self,
        job: JobHandleV1,
        file: ExtensionResultCacheFileFactV1,
        option_hash: [u8; 32],
        watcher_scope: u128,
        watcher_generation: u64,
        recursive: bool,
        item_generation: u64,
        location_generation: u64,
        source_generation: u64,
        now: std::time::Instant,
        host_identity: impl FnMut(usize) -> (String, u128),
    ) -> ExtensionJobCacheLookupV1 {
        let Some(generation) = ExtensionResultCacheGenerationV1::from_host(
            item_generation,
            location_generation,
            source_generation,
        ) else {
            return ExtensionJobCacheLookupV1::RejectedStale;
        };
        let producer = {
            let Ok(state) = self.state.lock() else {
                return ExtensionJobCacheLookupV1::RejectedStale;
            };
            let Some(runtime_job) = state.jobs.get(&job) else {
                return ExtensionJobCacheLookupV1::RejectedStale;
            };
            if !result_job_is_ui_current(runtime_job)
                || !generation.matches(
                    runtime_job.item_generation,
                    runtime_job.location_generation,
                    runtime_job.source_generation,
                )
                || state
                    .revoked_producers
                    .contains_key(&ProducerGenerationKeyV1::from(
                        &runtime_job.authority.producer,
                    ))
            {
                return ExtensionJobCacheLookupV1::RejectedStale;
            }
            runtime_job.authority.producer.clone()
        };
        let Some(key) = ExtensionResultCacheKeyV1::from_host(
            &producer,
            file,
            option_hash,
            watcher_scope,
            watcher_generation,
            recursive,
        ) else {
            return ExtensionJobCacheLookupV1::RejectedStale;
        };
        match self.result_cache.lookup_at(key, generation, now) {
            ExtensionResultCacheLookupV1::RejectedStale => ExtensionJobCacheLookupV1::RejectedStale,
            ExtensionResultCacheLookupV1::Miss(mut admission) => {
                admission.bind_job(job);
                ExtensionJobCacheLookupV1::Miss(admission)
            }
            ExtensionResultCacheLookupV1::Hit(hit) => {
                // The mapper may consult the host model (or re-enter this
                // runtime), so validation and the tombstone clone are the
                // only operations performed under the runtime mutex. A
                // concurrent revoke after this scope drops still invalidates
                // the returned rows through the cloned generation token.
                let runtime_generation = {
                    let Ok(state) = self.state.lock() else {
                        return ExtensionJobCacheLookupV1::RejectedStale;
                    };
                    let Some(runtime_job) = state.jobs.get(&job) else {
                        return ExtensionJobCacheLookupV1::RejectedStale;
                    };
                    if !result_job_is_ui_current(runtime_job)
                        || !generation.matches(
                            runtime_job.item_generation,
                            runtime_job.location_generation,
                            runtime_job.source_generation,
                        )
                        || hit.producer() != &runtime_job.authority.producer
                        || state
                            .revoked_producers
                            .contains_key(&ProducerGenerationKeyV1::from(
                                &runtime_job.authority.producer,
                            ))
                    {
                        return ExtensionJobCacheLookupV1::RejectedStale;
                    }
                    runtime_job.value_generation.clone()
                };
                ExtensionJobCacheLookupV1::Hit(hit.rebind_rows(runtime_generation, host_identity))
            }
        }
    }

    /// Copies an accepted batch into a prior cache-miss admission. Callers
    /// must still use the final apply gate for UI publication; the cache never
    /// retains ABI buffers or UI rows.
    pub fn cache_accepted_batch(
        &self,
        admission: ExtensionResultCacheAdmissionV1,
        batch: &AcceptedIncrementalResultBatchV1,
    ) -> ExtensionResultCacheInsertOutcomeV1 {
        if !self.is_accepted_batch_current(batch) {
            return ExtensionResultCacheInsertOutcomeV1::RejectedStale;
        }
        self.result_cache
            .insert_batch_at(admission, batch, std::time::Instant::now())
    }

    /// Installs the host-created weak UI signal hook before providers can
    /// submit. The hook retains no accepted bytes and does no UI work.
    pub(crate) fn install_ready_signal_sink(&self, sink: RuntimeReadySignalSinkV1) {
        if let Ok(mut state) = self.state.lock() {
            state.ready_signal_sink = Some(sink);
        }
    }

    /// Enumerates all runtime-current jobs with queued accepted work. This is
    /// bounded by the validated active-job cap and is used only after the UI
    /// signal mailbox reports overflow.
    #[must_use]
    pub fn ready_job_signals(&self) -> Vec<ExtensionJobUiReadySignalV1> {
        let Ok(state) = self.state.lock() else {
            return Vec::new();
        };
        state
            .jobs
            .iter()
            .filter_map(|(job, runtime_job)| {
                (!runtime_job.queued_batches.is_empty() && result_job_is_ui_current(runtime_job))
                    .then_some(ExtensionJobUiReadySignalV1::from_runtime(
                        *job,
                        runtime_job.item_generation,
                        runtime_job.location_generation,
                        runtime_job.source_generation,
                    ))
            })
            .collect()
    }

    /// Revokes one lifecycle feature epoch without affecting sibling features
    /// or work admitted by a later enable epoch.
    pub(crate) fn revoke_feature_generation(
        &self,
        package_id: &str,
        manifest_digest: &str,
        feature_id: &str,
        epoch: u64,
    ) {
        if let Some(authority) = &self.runtime_authority {
            let _ = authority.revoke_feature_incarnation(package_id, feature_id, epoch);
        }
        self.result_cache.invalidate_feature_generation(
            package_id,
            manifest_digest,
            feature_id,
            epoch,
        );
        let inactive = {
            let Ok(mut state) = self.state.lock() else {
                return;
            };
            let matching_keys = state
                .generation_tombstones
                .keys()
                .filter(|key| {
                    key.package_id == package_id
                        && key.sealed_manifest_digest == manifest_digest
                        && key.feature_id == feature_id
                        && key.feature_epoch == epoch
                })
                .cloned()
                .collect::<Vec<_>>();
            for key in matching_keys {
                if state.revoked_producers.contains_key(&key) {
                    continue;
                }
                let triggering_job = state.jobs.iter().find_map(|(handle, job)| {
                    (ProducerGenerationKeyV1::from(&job.authority.producer) == key)
                        .then_some(*handle)
                });
                if let Some(job) = triggering_job {
                    revoke_generation_and_close(
                        &mut state,
                        job,
                        ProducerRevocationReasonV1::LifecycleCancelled,
                    );
                } else {
                    revoke_registered_generation_tombstones(&mut state, &key);
                    state
                        .revoked_producers
                        .insert(key, ProducerRevocationReasonV1::LifecycleCancelled);
                }
            }
            let triggering_jobs = state
                .jobs
                .iter()
                .filter_map(|(handle, job)| {
                    let producer = &job.authority.producer;
                    (producer.package_id == package_id
                        && producer.sealed_manifest_digest == manifest_digest
                        && producer.feature_id == feature_id
                        && producer.feature_epoch == epoch)
                        .then_some(*handle)
                })
                .collect::<Vec<_>>();
            for job in triggering_jobs {
                revoke_generation_and_close(
                    &mut state,
                    job,
                    ProducerRevocationReasonV1::LifecycleCancelled,
                );
            }
            state
                .jobs
                .iter()
                .filter_map(|(handle, job)| {
                    let producer = &job.authority.producer;
                    (!job.provider_call_active
                        && job.terminal.is_some()
                        && producer.package_id == package_id
                        && producer.sealed_manifest_digest == manifest_digest
                        && producer.feature_id == feature_id
                        && producer.feature_epoch == epoch)
                        .then_some(*handle)
                })
                .collect::<Vec<_>>()
        };
        for job in inactive {
            let _ = retire_job(&self.state, job);
        }
    }

    /// Whether callbacks from one lifecycle feature epoch have completed.
    #[must_use]
    pub(crate) fn feature_callbacks_drained(
        &self,
        package_id: &str,
        manifest_digest: &str,
        feature_id: &str,
        epoch: u64,
    ) -> bool {
        self.state.lock().is_ok_and(|state| {
            !state.jobs.values().any(|job| {
                let producer = &job.authority.producer;
                job.provider_call_active
                    && producer.package_id == package_id
                    && producer.sealed_manifest_digest == manifest_digest
                    && producer.feature_id == feature_id
                    && producer.feature_epoch == epoch
            })
        })
    }

    /// Prepares a capability-bound ABI context for one guarded provider call.
    ///
    /// The returned ticket is linear: it can invoke exactly one provider, and
    /// its Drop path fail-closes any callback that did not reach a durable
    /// marker-clear/terminal-publication commit.
    pub(crate) fn prepare_provider_dispatch(
        &self,
        request: ExtensionJobRuntimeRequestV1,
    ) -> Result<PreparedProviderDispatchTicketV1, ExtensionJobRuntimeErrorV1> {
        let context = self.open_job_inner(request)?;
        Ok(PreparedProviderDispatchTicketV1 {
            state: Arc::clone(&self.state),
            context,
            invoked: false,
            terminal_published: false,
        })
    }

    /// Prepares one bounded Rust batch-column callback. The host supplies only
    /// ordinary-file snapshots from the current folder; every snapshot is
    /// capped at 8 MiB and all results share one generation-bound sink.
    pub fn prepare_batch_column_dispatch(
        &self,
        request: BatchColumnRuntimeRequestV1,
    ) -> Result<PreparedBatchColumnDispatchTicketV1, ExtensionJobRuntimeErrorV1> {
        if request.items.is_empty()
            || request.items.len() > MAX_BATCH_COLUMN_ITEMS_V1
            || !request.authority.filesystem_read_authorized
            || request.items.iter().any(|item| {
                !item.source.matches_generation(request.source_generation)
                    || item.source.byte_len().is_none()
                    || item.file_name.is_empty()
                    || item.file_name.len() > MAX_BATCH_COLUMN_FILE_NAME_BYTES_V1
                    || item.file_name.contains(['/', '\\'])
            })
        {
            return Err(ExtensionJobRuntimeErrorV1::InvalidRequest);
        }
        let total_bytes = request.items.iter().try_fold(0_usize, |total, item| {
            total.checked_add(item.source.byte_len()?)
        });
        if total_bytes.is_none_or(|bytes| bytes > MAX_BATCH_COLUMN_INPUT_BYTES_V1) {
            return Err(ExtensionJobRuntimeErrorV1::InputStreamCapacityExceeded);
        }
        let lock_owner_query = request.lock_owner_query.clone();
        let lock_owner_authority = if lock_owner_query.is_some() {
            if !request.authority.lock_owner_query_authorized {
                return Err(ExtensionJobRuntimeErrorV1::UnauthorizedAuthority);
            }
            let runtime_authority = self
                .runtime_authority
                .as_ref()
                .ok_or(ExtensionJobRuntimeErrorV1::UnauthorizedAuthority)?;
            let producer = request.authority.producer();
            let interface = producer.interface_id();
            let authorized_root_sha256 = if producer.sealed_manifest_digest().len() == 64
                && producer
                    .sealed_manifest_digest()
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit())
            {
                producer.sealed_manifest_digest().to_ascii_lowercase()
            } else {
                Sha256::digest(producer.sealed_manifest_digest().as_bytes())
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect()
            };
            Some((
                Arc::clone(runtime_authority),
                runtime_authority
                    .issue(AuthorityClaimsV1 {
                        package_id: producer.package_id().to_owned(),
                        feature_id: producer.feature_id().to_owned(),
                        interface_id: format!(
                            "{}:{}",
                            interface.namespace.into_raw(),
                            interface.value
                        ),
                        incarnation: producer.feature_epoch(),
                        capability: "lock_owner.query".to_owned(),
                        authorized_root_sha256,
                        location_generation: request.location_generation,
                        item_generation: request.item_generation,
                        refresh_generation: request.source_generation,
                        container_generation: request.source_generation,
                        job_generation: request.job_generation,
                    })
                    .map_err(|_| ExtensionJobRuntimeErrorV1::UnauthorizedAuthority)?,
            ))
        } else {
            None
        };
        let sources = request
            .items
            .into_iter()
            .map(|item| {
                Self::mint_item_handle(request.item_generation).map(|handle| RuntimeBatchInputV1 {
                    item: handle,
                    file_name: item.file_name,
                    source: item.source,
                    cache_identity: item.cache_identity,
                    modified_unix_seconds: item.modified_unix_seconds,
                    modified_subsec_nanos: item.modified_subsec_nanos,
                    source_size: item.source_size,
                    lock_owner_resource: item.lock_owner_resource,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let context = self.open_job_inner(ExtensionJobRuntimeRequestV1 {
            authority: request.authority,
            job_generation: request.job_generation,
            item_generation: request.item_generation,
            location_generation: request.location_generation,
            source_generation: request.source_generation,
            has_item: false,
            input_stream: None,
        })?;
        let batch_items = (|| -> Result<_, ExtensionJobRuntimeErrorV1> {
            let mut state = self
                .state
                .lock()
                .map_err(|_| ExtensionJobRuntimeErrorV1::StatePoisoned)?;
            let Some(job) = state.jobs.get(&context.job) else {
                return Err(ExtensionJobRuntimeErrorV1::UnknownJob);
            };
            let package_id = job.authority.producer.package_id.clone();
            let Some(total) = total_bytes else {
                return Err(ExtensionJobRuntimeErrorV1::InputStreamCapacityExceeded);
            };
            let package_current = state
                .input_stream_bytes_per_package
                .get(&package_id)
                .copied()
                .unwrap_or(0);
            let Some(next_total) = state.input_stream_bytes.checked_add(total) else {
                return Err(ExtensionJobRuntimeErrorV1::InputStreamCapacityExceeded);
            };
            let Some(next_package) = package_current.checked_add(total) else {
                return Err(ExtensionJobRuntimeErrorV1::InputStreamCapacityExceeded);
            };
            if next_total > state.config.max_bytes
                || next_package > state.config.max_bytes_per_package
            {
                return Err(ExtensionJobRuntimeErrorV1::InputStreamCapacityExceeded);
            }
            let (gate, batch_sources) = {
                let Some(job) = state.jobs.get_mut(&context.job) else {
                    return Err(ExtensionJobRuntimeErrorV1::UnknownJob);
                };
                job.input_stream_bytes = total;
                job.batch_input_streams = sources;
                (Arc::clone(&job.gate), job.batch_input_streams.clone())
            };
            state.input_stream_bytes = next_total;
            state
                .input_stream_bytes_per_package
                .insert(package_id, next_package);
            let resources = batch_sources
                .iter()
                .filter_map(|item| {
                    item.lock_owner_resource
                        .as_ref()
                        .map(|path| (item.item, path.clone()))
                })
                .collect::<Vec<_>>();
            let items = batch_sources
                .iter()
                .map(|item| {
                    Ok(BatchColumnItemV1 {
                        item: item.item,
                        item_generation: context.item_generation,
                        file_name: item.file_name.clone(),
                        cache_identity: item.cache_identity.clone(),
                        modified_unix_seconds: item.modified_unix_seconds,
                        modified_subsec_nanos: item.modified_subsec_nanos,
                        source_size: item.source_size,
                        input: InputStreamV1::from_host(
                            InputStreamCapabilityV1::from_host(random_nonce()?),
                            HostInputStreamServicesAdapterV1 {
                                state: Arc::downgrade(&self.state),
                                job: context.job,
                                gate: Arc::clone(&gate),
                                source: item.source.downgrade(),
                                expected_source_generation: context.source_generation,
                                position: Arc::new(Mutex::new(0)),
                            },
                        ),
                    })
                })
                .collect::<Result<RVec<_>, ExtensionJobRuntimeErrorV1>>()?;
            Ok((items, resources))
        })();
        let (batch_items, lock_owner_resources) = match batch_items {
            Ok(items) => items,
            Err(error) => {
                fail_close_ticket(
                    &self.state,
                    context.job,
                    ProducerRevocationReasonV1::LifecycleCancelled,
                );
                return Err(error);
            }
        };
        let lock_owner_query = lock_owner_query.zip(lock_owner_authority).map(
            |(service, (runtime_authority, authority))| {
                LockOwnerQueryServiceV1::from_host(HostLockOwnerQueryAdapterV1 {
                    state: Arc::downgrade(&self.state),
                    job: context.job,
                    item_generation: context.item_generation,
                    location_generation: context.location_generation,
                    resources: Arc::new(lock_owner_resources),
                    service,
                    runtime_authority,
                    authority,
                })
            },
        );
        Ok(PreparedBatchColumnDispatchTicketV1 {
            state: Arc::clone(&self.state),
            context: BatchColumnContextV1 {
                job: context.job,
                location: context.location,
                feature_epoch: context.feature_epoch,
                job_generation: context.job_generation,
                item_generation: context.item_generation,
                location_generation: context.location_generation,
                source_generation: context.source_generation,
                items: batch_items,
                lock_owner_query: lock_owner_query.into(),
                sink: context.sink,
                progress: context.progress,
            },
            invoked: false,
            terminal_published: false,
        })
    }

    /// Mints a capability-bound ABI context on the worker that will invoke it.
    ///
    /// This raw test seam deliberately has no production callers.  Production
    /// dispatch must use [`Self::prepare_provider_dispatch`] so marker failure
    /// cannot leave a live job or dispatch lease behind.
    #[cfg(test)]
    pub(crate) fn open_job(
        &self,
        request: ExtensionJobRuntimeRequestV1,
    ) -> Result<JobContextV1, ExtensionJobRuntimeErrorV1> {
        self.open_job_inner(request)
    }

    /// Opens a test-owned context for an application integration fixture.
    /// It is unavailable from normal production builds.
    #[cfg(feature = "integration-test-support")]
    pub fn open_job_for_integration_test(
        &self,
        request: ExtensionJobRuntimeRequestV1,
    ) -> Result<JobContextV1, ExtensionJobRuntimeErrorV1> {
        self.open_job_inner(request)
    }

    /// Submits exactly one batch through a scoped integration-fixture provider
    /// call. The feature-gated helper preserves the normal call-scope rules.
    #[cfg(feature = "integration-test-support")]
    #[must_use]
    pub fn submit_for_integration_test(
        &self,
        context: &JobContextV1,
        batch: IncrementalResultBatchV1,
    ) -> SinkSubmitOutcomeV1 {
        let Ok(scope) = ProviderCallScopeV1::enter(&self.state, context) else {
            return SinkSubmitOutcomeV1 {
                status: SinkSubmitStatusV1::CLOSED,
                remaining_batch_credits: 0,
                remaining_item_credits: 0,
                remaining_byte_credits: 0,
                rejected_batch: ROption::RSome(batch),
            };
        };
        let outcome = context.sink.try_submit(batch);
        drop(scope);
        outcome
    }

    /// Publishes a terminal for a feature-gated application integration fixture.
    #[cfg(feature = "integration-test-support")]
    #[must_use]
    pub fn finish_for_integration_test(
        &self,
        job: JobHandleV1,
        reported: JobTerminalV1,
    ) -> ExtensionJobFinishOutcomeV1 {
        let outcome = finish_job(&self.state, job, reported);
        if !matches!(outcome, ExtensionJobFinishOutcomeV1::UnknownJob) {
            let _ = authorize_retirement(&self.state, job);
        }
        outcome
    }

    fn open_job_inner(
        &self,
        request: ExtensionJobRuntimeRequestV1,
    ) -> Result<JobContextV1, ExtensionJobRuntimeErrorV1> {
        if request.job_generation == 0
            || request.item_generation == 0
            || request.location_generation == 0
            || request.source_generation == 0
            || request.authority.producer.feature_epoch == 0
        {
            return Err(ExtensionJobRuntimeErrorV1::InvalidRequest);
        }
        if request.input_stream.is_some() && !request.authority.filesystem_read_authorized {
            return Err(ExtensionJobRuntimeErrorV1::UnauthorizedInputStream);
        }
        if request
            .input_stream
            .as_ref()
            .is_some_and(|source| !source.matches_generation(request.source_generation))
        {
            return Err(ExtensionJobRuntimeErrorV1::InvalidRequest);
        }
        let Some(input_stream_bytes) = request
            .input_stream
            .as_ref()
            .map_or(Some(0), HostInputStreamSourceV1::byte_len)
        else {
            return Err(ExtensionJobRuntimeErrorV1::StatePoisoned);
        };
        let Some(input_generation_token) =
            request.input_stream.as_ref().map_or(Some(None), |source| {
                source.generation_token(request.source_generation).map(Some)
            })
        else {
            return Err(ExtensionJobRuntimeErrorV1::InvalidRequest);
        };
        let job = self.mint_handle(request.job_generation)?;
        let item = request
            .has_item
            .then(|| Self::mint_item_handle(request.item_generation))
            .transpose()?;
        let location = Self::mint_location_handle(request.location_generation)?;
        let sink_capability = SinkCapabilityV1::from_host(random_nonce()?);
        let input_capability = request
            .input_stream
            .as_ref()
            .map(|_| random_nonce().map(InputStreamCapabilityV1::from_host))
            .transpose()?;
        let gate = Arc::new(InvocationGateV1 {
            active: AtomicBool::new(false),
            owner_thread: Mutex::new(None),
        });
        let feature_epoch = request.authority.producer.feature_epoch;
        let package_id = request.authority.producer.package_id.clone();
        let producer_key = ProducerGenerationKeyV1::from(request.authority.producer());
        let expected_sort = request.authority.expected_sort;
        let invocation = InvocationGenerationsV1 {
            job: request.job_generation,
            item: request.item_generation,
            location: request.location_generation,
            source: request.source_generation,
        };
        let job_state = RuntimeJobV1 {
            authority: request.authority,
            owner_thread: None,
            item,
            location,
            job_generation: request.job_generation,
            item_generation: request.item_generation,
            location_generation: request.location_generation,
            source_generation: request.source_generation,
            input_stream: request.input_stream.clone(),
            batch_input_streams: Vec::new(),
            batch_result_submitted: false,
            input_stream_bytes,
            expected_sort,
            value_generation: input_generation_token.map_or_else(
                ExtensionValueGenerationV1::current,
                |token| {
                    ExtensionValueGenerationV1::combine([
                        ExtensionValueGenerationV1::current(),
                        token,
                    ])
                },
            ),
            sink_capability,
            gate: Arc::clone(&gate),
            provider_call_active: false,
            finalization: JobFinalizationV1::Idle,
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
        if state.revoked_producers.contains_key(&producer_key) {
            return Err(ExtensionJobRuntimeErrorV1::UnauthorizedAuthority);
        }
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
        let package_input_bytes = state
            .input_stream_bytes_per_package
            .get(&package_id)
            .copied()
            .unwrap_or(0);
        let Some(next_input_bytes) = state.input_stream_bytes.checked_add(input_stream_bytes)
        else {
            return Err(ExtensionJobRuntimeErrorV1::InputStreamCapacityExceeded);
        };
        let Some(next_package_input_bytes) = package_input_bytes.checked_add(input_stream_bytes)
        else {
            return Err(ExtensionJobRuntimeErrorV1::InputStreamCapacityExceeded);
        };
        if next_input_bytes > state.config.max_bytes
            || next_package_input_bytes > state.config.max_bytes_per_package
        {
            return Err(ExtensionJobRuntimeErrorV1::InputStreamCapacityExceeded);
        }
        remember_generation_tombstone(&mut state, producer_key, &job_state.value_generation);
        state.jobs.insert(job, job_state);
        let active = state
            .active_jobs_per_package
            .entry(package_id.clone())
            .or_default()
            .checked_add(1)
            .ok_or(ExtensionJobRuntimeErrorV1::ActiveJobLimitExceeded)?;
        state
            .active_jobs_per_package
            .insert(package_id.clone(), active);
        if input_stream_bytes != 0 {
            state.input_stream_bytes = next_input_bytes;
            state
                .input_stream_bytes_per_package
                .insert(package_id, next_package_input_bytes);
        }
        drop(state);
        let services = JobHostServicesV1::from_host(HostJobServicesAdapterV1 {
            state: Arc::downgrade(&self.state),
            job,
            capability: sink_capability,
            invocation,
            gate: Arc::clone(&gate),
        });
        let input = match (request.input_stream, input_capability) {
            (Some(source), Some(capability)) => ROption::RSome(InputStreamV1::from_host(
                capability,
                HostInputStreamServicesAdapterV1 {
                    state: Arc::downgrade(&self.state),
                    job,
                    gate,
                    source: source.downgrade(),
                    expected_source_generation: request.source_generation,
                    position: Arc::new(Mutex::new(0)),
                },
            )),
            _ => ROption::RNone,
        };
        Ok(JobContextV1 {
            job,
            item: item.map_or(ROption::RNone, ROption::RSome),
            location,
            feature_epoch,
            job_generation: request.job_generation,
            item_generation: request.item_generation,
            location_generation: request.location_generation,
            source_generation: request.source_generation,
            input,
            sink: services.result_sink(job, sink_capability),
            progress: services.progress_sink(job, sink_capability),
        })
    }

    /// Test/host-internal terminal publication. Production native dispatch
    /// commits through a prepared ticket only after marker clearing succeeds.
    #[cfg(test)]
    pub(crate) fn finish(
        &self,
        job: JobHandleV1,
        reported: JobTerminalV1,
    ) -> ExtensionJobFinishOutcomeV1 {
        let outcome = finish_job(&self.state, job, reported);
        if !matches!(outcome, ExtensionJobFinishOutcomeV1::UnknownJob) {
            let _ = authorize_retirement(&self.state, job);
        }
        outcome
    }

    /// Takes bounded, path-free producer quarantine diagnostics.
    #[must_use]
    pub fn take_quarantine(&self) -> Vec<ExtensionJobQuarantineEventV1> {
        self.state.lock().map_or_else(
            |_| Vec::new(),
            |mut state| state.quarantines.drain(..).collect(),
        )
    }

    /// Final apply gate for a drained batch. Consumers must call this immediately
    /// before publishing rows so an interleaving protocol fault revokes data that
    /// was drained earlier in the same generation.
    #[must_use]
    pub fn is_accepted_batch_current(&self, batch: &AcceptedIncrementalResultBatchV1) -> bool {
        let Ok(state) = self.state.lock() else {
            return false;
        };
        accepted_batch_is_current(&state, batch)
    }

    /// Atomically rechecks the current generation and commits projected rows to
    /// the host-owned store.  Revocation holds this same mutex, so neither a
    /// marker failure nor lifecycle cancellation can race a stale UI apply.
    pub fn apply_accepted_batch(
        &self,
        batch: &AcceptedIncrementalResultBatchV1,
        host_identity: impl FnMut(usize) -> (String, u128),
    ) -> Option<Vec<ExtensionValueRowV1>> {
        let rows = batch.project_rows(host_identity);
        let Ok(mut state) = self.state.lock() else {
            return None;
        };
        let key = AppliedBatchKeyV1::from(batch);
        if !accepted_batch_is_current(&state, batch)
            || state.applied_batches.contains(&key)
            || rows.len()
                > state
                    .config
                    .max_items
                    .saturating_sub(state.applied_row_count)
        {
            return None;
        }
        state.applied_batches.insert(key);
        state.applied_row_count += rows.len();
        state
            .applied_rows
            .entry(batch.job)
            .or_default()
            .extend(rows.iter().cloned());
        Some(rows)
    }

    /// Applies one accepted batch and only then queues a coalesced UI
    /// invalidation. The batcher never invokes a redraw on this worker path.
    pub fn apply_accepted_batch_and_enqueue_invalidation(
        &self,
        batch: &AcceptedIncrementalResultBatchV1,
        host_identity: impl FnMut(usize) -> (String, u128),
        invalidations: &mut UiInvalidationBatcherV1,
    ) -> Option<Vec<ExtensionValueRowV1>> {
        let rows = self.apply_accepted_batch(batch, host_identity);
        if rows.is_some() {
            invalidations.record_accepted_batch(batch);
            // A concurrent navigation/cancellation can occur after the apply
            // lock is released. Recheck while still on the UI-owned pump path
            // so no stale record survives until its deadline.
            invalidations.discard_not_current(|job, item, location, source| {
                self.is_result_generation_current(job, item, location, source)
            });
        }
        rows
    }

    /// Returns whether a result generation remains eligible for UI emission.
    /// This is intentionally callback-free and is used immediately before a
    /// coalesced invalidation reaches the GPUI composition root.
    #[must_use]
    pub fn is_result_generation_current(
        &self,
        job: JobHandleV1,
        item_generation: u64,
        location_generation: u64,
        source_generation: u64,
    ) -> bool {
        let Ok(state) = self.state.lock() else {
            return false;
        };
        let Some(runtime_job) = state.jobs.get(&job) else {
            return false;
        };
        runtime_job.job_generation == job.generation()
            && runtime_job.item_generation == item_generation
            && runtime_job.location_generation == location_generation
            && runtime_job.source_generation == source_generation
            && result_job_is_ui_current(runtime_job)
            && !state
                .revoked_producers
                .contains_key(&ProducerGenerationKeyV1::from(
                    &runtime_job.authority.producer,
                ))
    }

    /// Returns host-owned rows while the exact batch generation is current.
    /// Every clone retains its shared generation tombstone, so subsequent
    /// revocation invalidates values and opaque routes already retained by UI.
    #[must_use]
    pub fn applied_rows_snapshot(
        &self,
        batch: &AcceptedIncrementalResultBatchV1,
    ) -> Option<Vec<ExtensionValueRowV1>> {
        let Ok(state) = self.state.lock() else {
            return None;
        };
        accepted_batch_is_current(&state, batch).then(|| {
            state
                .applied_rows
                .get(&batch.job)
                .cloned()
                .unwrap_or_default()
        })
    }

    /// Changes cooperative state without invoking extension code or callbacks.
    pub fn request_control(
        &self,
        job: JobHandleV1,
        control: JobControlStateV1,
    ) -> Result<(), ExtensionJobRuntimeErrorV1> {
        request_control_for_job(&self.state, job, control)
    }

    /// Invalidates this job's view after a host item/location/source advance.
    /// The issued handles and service capabilities are generation-bound, so the
    /// job is cancelled and the scheduler must mint a fresh context; queued
    /// batches from the older view are discarded by [`Self::drain`].
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
        let input_generation_token = {
            let runtime_job = state
                .jobs
                .get(&job)
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
            runtime_job
                .input_stream
                .as_ref()
                .map(|source| {
                    source
                        .generation_token(source_generation)
                        .ok_or(ExtensionJobRuntimeErrorV1::InvalidRequest)
                })
                .transpose()?
        };
        {
            let runtime_job = state
                .jobs
                .get_mut(&job)
                .ok_or(ExtensionJobRuntimeErrorV1::UnknownJob)?;
            // Rows applied before this generation change may still be retained
            // by UI code. Revoke their shared tombstone before exposing the new
            // generation, then replace it for any subsequently drained batch.
            runtime_job.value_generation.revoke();
            runtime_job.value_generation =
                input_generation_token.map_or_else(ExtensionValueGenerationV1::current, |token| {
                    ExtensionValueGenerationV1::combine([
                        ExtensionValueGenerationV1::current(),
                        token,
                    ])
                });
            runtime_job.item_generation = item_generation;
            runtime_job.location_generation = location_generation;
            runtime_job.source_generation = source_generation;
            runtime_job.pending_progress = None;
            // Item/location/source handles and service capabilities are minted
            // for one immutable callback context. A newer host view therefore
            // cancels this job instead of letting a retained old context forge
            // public generation tags and publish into the new view. Schedulers
            // mint a new job/context for the replacement generation.
            if runtime_job.terminal.is_none()
                && runtime_job.control.into_raw() == JobControlStateV1::ACTIVE.into_raw()
            {
                runtime_job.control = JobControlStateV1::CANCELLED;
            }
        }
        if let Some(runtime_job) = state.jobs.get(&job) {
            let key = ProducerGenerationKeyV1::from(&runtime_job.authority.producer);
            let generation = runtime_job.value_generation.clone();
            remember_generation_tombstone(&mut state, key, &generation);
        }
        remove_applied_rows(&mut state, job);
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
            || !result_job_is_ui_current(runtime_job)
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
        let producer_revoked = state.jobs.get(&job).is_some_and(|runtime_job| {
            state
                .revoked_producers
                .contains_key(&ProducerGenerationKeyV1::from(
                    &runtime_job.authority.producer,
                ))
        });
        let drain_limit = if producer_revoked {
            usize::MAX
        } else {
            maximum_batches
        };
        let (drained, package_id, released, local_accounting_ok) = {
            let Some(runtime_job) = state.jobs.get_mut(&job) else {
                return Vec::new();
            };
            let producer = runtime_job.authority.producer.clone();
            let source_current = result_job_is_ui_current(runtime_job);
            let mut drained = Vec::new();
            let mut released = QueueUsageV1::default();
            let mut local_accounting_ok = true;
            for _ in 0..drain_limit {
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
                if !producer_revoked
                    && source_current
                    && batch.job_generation == runtime_job.job_generation
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
                        item_generation: batch.item_generation,
                        location_generation: batch.location_generation,
                        source_generation: batch.source_generation,
                        generation: runtime_job.value_generation.clone(),
                        cache_bytes: batch.bytes,
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
        // One ready signal represents a job, not an unbounded drain grant.
        // Re-arm a still-current job after a bounded UI drain so remaining
        // batches cannot be stranded once the inbox removes its dedup entry.
        let rearm = state.jobs.get(&job).and_then(|runtime_job| {
            (!runtime_job.queued_batches.is_empty() && result_job_is_ui_current(runtime_job))
                .then(|| {
                    state.ready_signal_sink.clone().map(|sink| {
                        (
                            sink,
                            ExtensionJobUiReadySignalV1::from_runtime(
                                job,
                                runtime_job.item_generation,
                                runtime_job.location_generation,
                                runtime_job.source_generation,
                            ),
                        )
                    })
                })
                .flatten()
        });
        drop(state);
        if let Some((sink, signal)) = rearm {
            sink.signal(signal);
        }
        drained
    }

    /// Purges queued result data and releases credits when a generation is stale.
    pub fn purge(&self, job: JobHandleV1) -> Result<(), ExtensionJobRuntimeErrorV1> {
        purge_job(&self.state, job)
    }

    /// Retires a terminal job, releasing queued credits and its registry slot.
    /// Any retained sink immediately becomes closed because registry removal
    /// occurs before the runtime state is destroyed.
    pub fn retire(&self, job: JobHandleV1) -> Result<(), ExtensionJobRuntimeErrorV1> {
        retire_job(&self.state, job)
    }

    /// Fail-closes every active producer generation during host shutdown.
    /// Queued credits and retained rows are released/revoked under the same
    /// lock, so an externally held UI inbox can no longer observe current
    /// values after this method returns.
    pub(crate) fn cancel_and_revoke_all(&self) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        let jobs = state.jobs.keys().copied().collect::<Vec<_>>();
        for job in jobs {
            revoke_generation_and_close(
                &mut state,
                job,
                ProducerRevocationReasonV1::LifecycleCancelled,
            );
        }
        drop(state);
        self.result_cache.clear();
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

/// Cloneable host control surface for a prepared dispatch.  It deliberately
/// owns no native dispatch lease; the linear ticket owns cleanup authority.
#[allow(dead_code)]
#[derive(Clone)]
pub(crate) struct ProviderDispatchControlV1 {
    state: Arc<Mutex<RuntimeStateV1>>,
    job: JobHandleV1,
    job_generation: u64,
    item_generation: u64,
    location_generation: u64,
    source_generation: u64,
}

#[allow(dead_code)]
impl ProviderDispatchControlV1 {
    #[must_use]
    pub(crate) const fn job(&self) -> JobHandleV1 {
        self.job
    }

    pub(crate) fn request_control(
        &self,
        control: JobControlStateV1,
    ) -> Result<(), ExtensionJobRuntimeErrorV1> {
        self.ensure_current()?;
        request_control_for_job(&self.state, self.job, control)
    }

    pub(crate) fn purge(&self) -> Result<(), ExtensionJobRuntimeErrorV1> {
        self.ensure_current()?;
        purge_job(&self.state, self.job)
    }

    pub(crate) fn retire(&self) -> Result<(), ExtensionJobRuntimeErrorV1> {
        self.ensure_current()?;
        retire_job(&self.state, self.job)
    }

    fn ensure_current(&self) -> Result<(), ExtensionJobRuntimeErrorV1> {
        let state = self
            .state
            .lock()
            .map_err(|_| ExtensionJobRuntimeErrorV1::StatePoisoned)?;
        let job = state
            .jobs
            .get(&self.job)
            .ok_or(ExtensionJobRuntimeErrorV1::UnknownJob)?;
        (job.job_generation == self.job_generation
            && job.item_generation == self.item_generation
            && job.location_generation == self.location_generation
            && job.source_generation == self.source_generation)
            .then_some(())
            .ok_or(ExtensionJobRuntimeErrorV1::StaleControlHandle)
    }
}

/// Linear host-owned provider dispatch transaction.  It is not ABI-visible and
/// cannot be cloned: exactly one synchronous invocation is permitted, then a
/// durable marker clear must succeed before the terminal can be committed.
pub(crate) struct PreparedProviderDispatchTicketV1 {
    state: Arc<Mutex<RuntimeStateV1>>,
    context: JobContextV1,
    invoked: bool,
    terminal_published: bool,
}

impl PreparedProviderDispatchTicketV1 {
    #[allow(dead_code)]
    #[must_use]
    pub(crate) fn control(&self) -> ProviderDispatchControlV1 {
        ProviderDispatchControlV1 {
            state: Arc::clone(&self.state),
            job: self.context.job,
            job_generation: self.context.job_generation,
            item_generation: self.context.item_generation,
            location_generation: self.context.location_generation,
            source_generation: self.context.source_generation,
        }
    }

    /// Enters the one and only provider callback scope, but intentionally does
    /// not publish a terminal.  The caller must clear its durable marker first.
    pub(crate) fn invoke_once(
        &mut self,
        provider: &explorer_extension_api::JobProviderObjectV1,
    ) -> Result<JobTerminalV1, ExtensionJobRuntimeErrorV1> {
        if self.invoked {
            return Err(ExtensionJobRuntimeErrorV1::ProviderAlreadyInvoked);
        }
        self.invoked = true;
        let scope = ProviderCallScopeV1::enter(&self.state, &self.context)?;
        let terminal = provider.invoke(self.context.clone());
        drop(scope);
        Ok(if terminal.is_known() {
            terminal
        } else {
            JobTerminalV1::INCOMPATIBLE
        })
    }

    /// Commits the provider terminal only after the native marker has been
    /// cleared and durably synced.  A second commit is rejected fail-closed.
    pub(crate) fn publish_terminal_after_marker_clear(
        &mut self,
        terminal: JobTerminalV1,
    ) -> Result<ExtensionJobFinishOutcomeV1, ExtensionJobRuntimeErrorV1> {
        if !self.invoked || self.terminal_published {
            return Err(ExtensionJobRuntimeErrorV1::TerminalPublicationDenied);
        }
        let outcome = finish_job(&self.state, self.context.job, terminal);
        if matches!(outcome, ExtensionJobFinishOutcomeV1::UnknownJob) {
            return Err(ExtensionJobRuntimeErrorV1::UnknownJob);
        }
        authorize_retirement(&self.state, self.context.job)?;
        self.terminal_published = true;
        Ok(outcome)
    }

    /// Explicit marker-failure finalization for the dispatch layer.  It also
    /// invalidates any pre-drained or renderer-routed data from this producer.
    pub(crate) fn fail_marker_clear(&mut self) {
        if !self.terminal_published {
            fail_close_ticket(
                &self.state,
                self.context.job,
                ProducerRevocationReasonV1::MarkerFailure,
            );
            self.terminal_published = true;
        }
    }
}

impl Drop for PreparedProviderDispatchTicketV1 {
    fn drop(&mut self) {
        if self.terminal_published {
            let _ = retire_job(&self.state, self.context.job);
        } else {
            fail_close_ticket(
                &self.state,
                self.context.job,
                ProducerRevocationReasonV1::LifecycleCancelled,
            );
        }
    }
}

/// Linear host-owned transaction for one bounded batch-column callback.
///
/// It has the same marker-clear lifetime rules as ordinary provider dispatch;
/// the only difference is that the callback sees at most 128 host-attested
/// item streams on one current generation.
pub struct PreparedBatchColumnDispatchTicketV1 {
    state: Arc<Mutex<RuntimeStateV1>>,
    context: BatchColumnContextV1,
    invoked: bool,
    terminal_published: bool,
}

impl PreparedBatchColumnDispatchTicketV1 {
    /// Returns the opaque job capability used for bounded result draining.
    #[must_use]
    pub const fn job(&self) -> JobHandleV1 {
        self.context.job
    }

    /// Invokes the one retained batch provider exactly once.
    pub fn invoke_once(
        &mut self,
        provider: &BatchColumnProviderObjectV1,
    ) -> Result<JobTerminalV1, ExtensionJobRuntimeErrorV1> {
        if self.invoked {
            return Err(ExtensionJobRuntimeErrorV1::ProviderAlreadyInvoked);
        }
        self.invoked = true;
        let scope = ProviderCallScopeV1::enter_batch(&self.state, &self.context)?;
        let terminal = provider.invoke(self.context.clone());
        drop(scope);
        Ok(if terminal.is_known() {
            terminal
        } else {
            JobTerminalV1::INCOMPATIBLE
        })
    }

    /// Publishes the callback terminal after the caller's durable marker clear.
    pub fn publish_terminal_after_marker_clear(
        &mut self,
        terminal: JobTerminalV1,
    ) -> Result<ExtensionJobFinishOutcomeV1, ExtensionJobRuntimeErrorV1> {
        if !self.invoked || self.terminal_published {
            return Err(ExtensionJobRuntimeErrorV1::TerminalPublicationDenied);
        }
        let outcome = finish_job(&self.state, self.context.job, terminal);
        if matches!(outcome, ExtensionJobFinishOutcomeV1::UnknownJob) {
            return Err(ExtensionJobRuntimeErrorV1::UnknownJob);
        }
        authorize_retirement(&self.state, self.context.job)?;
        self.terminal_published = true;
        Ok(outcome)
    }

    /// Fail-closes the generation when the outer dispatch marker cannot clear.
    pub fn fail_marker_clear(&mut self) {
        if !self.terminal_published {
            fail_close_ticket(
                &self.state,
                self.context.job,
                ProducerRevocationReasonV1::MarkerFailure,
            );
            self.terminal_published = true;
        }
    }
}

impl Drop for PreparedBatchColumnDispatchTicketV1 {
    fn drop(&mut self) {
        if self.terminal_published {
            let _ = retire_job(&self.state, self.context.job);
        } else {
            fail_close_ticket(
                &self.state,
                self.context.job,
                ProducerRevocationReasonV1::LifecycleCancelled,
            );
        }
    }
}

/// Publishes exactly one terminal. Cancellation/deadline host state wins over a plugin claim.
fn finish_job(
    state: &Arc<Mutex<RuntimeStateV1>>,
    job: JobHandleV1,
    reported: JobTerminalV1,
) -> ExtensionJobFinishOutcomeV1 {
    let Ok(mut state) = state.lock() else {
        return ExtensionJobFinishOutcomeV1::UnknownJob;
    };
    let producer_revoked = state.jobs.get(&job).is_some_and(|runtime_job| {
        state
            .revoked_producers
            .contains_key(&ProducerGenerationKeyV1::from(
                &runtime_job.authority.producer,
            ))
    });
    let Some(runtime_job) = state.jobs.get_mut(&job) else {
        return ExtensionJobFinishOutcomeV1::UnknownJob;
    };
    if let Some(terminal) = runtime_job.terminal {
        return ExtensionJobFinishOutcomeV1::AlreadyTerminal(terminal);
    }
    let source_stale = !job_sources_current(runtime_job);
    let terminal = terminal_precedence(
        runtime_job.control,
        if source_stale {
            JobTerminalV1::CANCELLED
        } else if reported.is_known() {
            reported
        } else {
            JobTerminalV1::INCOMPATIBLE
        },
        runtime_job.protocol_faulted || producer_revoked,
    );
    runtime_job.terminal = Some(terminal);
    runtime_job.control = JobControlStateV1::CLOSED;
    runtime_job.pending_progress = None;
    ExtensionJobFinishOutcomeV1::Published(terminal)
}

/// Commits the host-side finalization boundary after a durable marker clear or
/// an explicit fail-close. This is deliberately separate from terminal state:
/// a lifecycle cancellation observed while a callback is on the stack must
/// retain its lease until the callback reaches this boundary.
fn authorize_retirement(
    state: &Arc<Mutex<RuntimeStateV1>>,
    job: JobHandleV1,
) -> Result<(), ExtensionJobRuntimeErrorV1> {
    let mut state = state
        .lock()
        .map_err(|_| ExtensionJobRuntimeErrorV1::StatePoisoned)?;
    let runtime_job = state
        .jobs
        .get_mut(&job)
        .ok_or(ExtensionJobRuntimeErrorV1::UnknownJob)?;
    if runtime_job.terminal.is_none() {
        return Err(ExtensionJobRuntimeErrorV1::RetireRequiresTerminal);
    }
    runtime_job.finalization = JobFinalizationV1::RetirementAuthorized;
    Ok(())
}

fn accepted_batch_is_current(
    state: &RuntimeStateV1,
    batch: &AcceptedIncrementalResultBatchV1,
) -> bool {
    let Some(job) = state.jobs.get(&batch.job) else {
        return false;
    };
    result_job_is_ui_current(job)
        && !job.protocol_faulted
        && job.job_generation == batch.job_generation
        && job.item_generation == batch.item_generation
        && job.location_generation == batch.location_generation
        && job.source_generation == batch.source_generation
        && job.authority.producer == batch.producer
        && !state
            .revoked_producers
            .contains_key(&ProducerGenerationKeyV1::from(&job.authority.producer))
}

fn result_job_is_ui_current(job: &RuntimeJobV1) -> bool {
    (job.terminal == Some(JobTerminalV1::COMPLETED)
        || (job.terminal.is_none()
            && job.control.into_raw() == JobControlStateV1::ACTIVE.into_raw()))
        && job.value_generation.is_current()
        && job_sources_current(job)
}

fn job_sources_current(job: &RuntimeJobV1) -> bool {
    job.input_stream
        .as_ref()
        .is_none_or(|source| source.matches_generation(job.source_generation))
        && job
            .batch_input_streams
            .iter()
            .all(|item| item.source.matches_generation(job.source_generation))
}

fn job_source_matches(job: &RuntimeJobV1, source: &Weak<Mutex<HostInputStreamStateV1>>) -> bool {
    job.input_stream
        .as_ref()
        .is_some_and(|candidate| candidate.same_weak_state(source))
        || job
            .batch_input_streams
            .iter()
            .any(|item| item.source.same_weak_state(source))
}

/// Sets and clears the per-callback sink authorization without retaining a
/// mutable runtime lock during plugin execution.
struct ProviderCallScopeV1 {
    state: Arc<Mutex<RuntimeStateV1>>,
    job: JobHandleV1,
    capability: SinkCapabilityV1,
    owner_thread: ThreadId,
}

impl ProviderCallScopeV1 {
    fn enter(
        state: &Arc<Mutex<RuntimeStateV1>>,
        context: &JobContextV1,
    ) -> Result<Self, ExtensionJobRuntimeErrorV1> {
        let mut locked = state
            .lock()
            .map_err(|_| ExtensionJobRuntimeErrorV1::StatePoisoned)?;
        let owner_thread = std::thread::current().id();
        if locked.active_provider_threads.contains(&owner_thread) {
            return Err(ExtensionJobRuntimeErrorV1::InactiveProviderCall);
        }
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
            || job.owner_thread.is_some_and(|owner| owner != owner_thread)
            || !item_matches
            || context.job_generation != job.job_generation
            || context.location != job.location
            || context.location_generation != job.location_generation
            || context.source_generation != job.source_generation
            || context.feature_epoch != job.authority.producer.feature_epoch
            || !job_sources_current(job)
            || job.provider_call_active
            || job.terminal.is_some()
            || job.control.into_raw() != JobControlStateV1::ACTIVE.into_raw()
        {
            return Err(ExtensionJobRuntimeErrorV1::InactiveProviderCall);
        }
        if !job.gate.activate() {
            return Err(ExtensionJobRuntimeErrorV1::InactiveProviderCall);
        }
        job.owner_thread = Some(owner_thread);
        job.provider_call_active = true;
        job.finalization = JobFinalizationV1::MarkerPending;
        locked.active_provider_threads.insert(owner_thread);
        Ok(Self {
            state: Arc::clone(state),
            job: context.job,
            capability: context.sink.capability,
            owner_thread,
        })
    }

    fn enter_batch(
        state: &Arc<Mutex<RuntimeStateV1>>,
        context: &BatchColumnContextV1,
    ) -> Result<Self, ExtensionJobRuntimeErrorV1> {
        if !context.is_well_formed() {
            return Err(ExtensionJobRuntimeErrorV1::InvalidRequest);
        }
        let mut locked = state
            .lock()
            .map_err(|_| ExtensionJobRuntimeErrorV1::StatePoisoned)?;
        let owner_thread = std::thread::current().id();
        if locked.active_provider_threads.contains(&owner_thread) {
            return Err(ExtensionJobRuntimeErrorV1::InactiveProviderCall);
        }
        let job = locked
            .jobs
            .get_mut(&context.job)
            .ok_or(ExtensionJobRuntimeErrorV1::UnknownJob)?;
        let items_match = job.batch_input_streams.len() == context.items.len()
            && job
                .batch_input_streams
                .iter()
                .zip(context.items.iter())
                .all(|(expected, actual)| {
                    expected.item == actual.item && actual.item_generation == job.item_generation
                });
        if context.sink.job != context.job
            || context.sink.capability != job.sink_capability
            || context.progress.job != context.job
            || context.progress.capability != job.sink_capability
            || job.owner_thread.is_some_and(|owner| owner != owner_thread)
            || !items_match
            || context.job_generation != job.job_generation
            || context.location != job.location
            || context.location_generation != job.location_generation
            || context.item_generation != job.item_generation
            || context.source_generation != job.source_generation
            || context.feature_epoch != job.authority.producer.feature_epoch
            || !job_sources_current(job)
            || job.provider_call_active
            || job.terminal.is_some()
            || job.control.into_raw() != JobControlStateV1::ACTIVE.into_raw()
        {
            return Err(ExtensionJobRuntimeErrorV1::InactiveProviderCall);
        }
        if !job.gate.activate() {
            return Err(ExtensionJobRuntimeErrorV1::InactiveProviderCall);
        }
        job.owner_thread = Some(owner_thread);
        job.provider_call_active = true;
        job.finalization = JobFinalizationV1::MarkerPending;
        locked.active_provider_threads.insert(owner_thread);
        Ok(Self {
            state: Arc::clone(state),
            job: context.job,
            capability: context.sink.capability,
            owner_thread,
        })
    }
}

impl Drop for ProviderCallScopeV1 {
    fn drop(&mut self) {
        if let Ok(mut state) = self.state.lock()
            && let Some(job) = state.jobs.get_mut(&self.job)
            && job.sink_capability == self.capability
        {
            job.provider_call_active = false;
            job.gate.deactivate();
            state.active_provider_threads.remove(&self.owner_thread);
        }
    }
}

fn request_control_for_job(
    state: &Arc<Mutex<RuntimeStateV1>>,
    job: JobHandleV1,
    control: JobControlStateV1,
) -> Result<(), ExtensionJobRuntimeErrorV1> {
    let mut state = state
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

fn purge_job(
    state: &Arc<Mutex<RuntimeStateV1>>,
    job: JobHandleV1,
) -> Result<(), ExtensionJobRuntimeErrorV1> {
    let mut state = state
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
    let package_id = runtime_job.authority.producer.package_id.clone();
    runtime_job.queued_batches.clear();
    runtime_job.queued_items = 0;
    runtime_job.queued_bytes = 0;
    release_credits(&mut state, &package_id, released);
    Ok(())
}

fn retire_job(
    state: &Arc<Mutex<RuntimeStateV1>>,
    job: JobHandleV1,
) -> Result<(), ExtensionJobRuntimeErrorV1> {
    {
        let state = state
            .lock()
            .map_err(|_| ExtensionJobRuntimeErrorV1::StatePoisoned)?;
        let runtime_job = state
            .jobs
            .get(&job)
            .ok_or(ExtensionJobRuntimeErrorV1::UnknownJob)?;
        if runtime_job.provider_call_active
            || runtime_job.terminal.is_none()
            || runtime_job.finalization != JobFinalizationV1::RetirementAuthorized
        {
            return Err(ExtensionJobRuntimeErrorV1::RetireRequiresTerminal);
        }
    }
    let runtime_job = {
        let mut state = state
            .lock()
            .map_err(|_| ExtensionJobRuntimeErrorV1::StatePoisoned)?;
        let Some(runtime_job) = state.jobs.remove(&job) else {
            return Err(ExtensionJobRuntimeErrorV1::UnknownJob);
        };
        remove_applied_rows(&mut state, job);
        let released = QueueUsageV1 {
            batches: runtime_job.queued_batches.len(),
            items: runtime_job.queued_items,
            bytes: runtime_job.queued_bytes,
        };
        let package_id = runtime_job.authority.producer.package_id.clone();
        release_credits(&mut state, &package_id, released);
        release_active_job(&mut state, &package_id);
        release_input_stream_bytes(&mut state, &package_id, runtime_job.input_stream_bytes);
        runtime_job
    };
    // Drop the lease and any foreign object outside the runtime lock to
    // preserve lifecycle/runtime lock ordering.
    drop(runtime_job);
    Ok(())
}

fn release_input_stream_bytes(state: &mut RuntimeStateV1, package_id: &str, bytes: usize) {
    if bytes == 0 {
        return;
    }
    let Some(total) = state.input_stream_bytes.checked_sub(bytes) else {
        state.accounting_healthy = false;
        return;
    };
    state.input_stream_bytes = total;
    let Some(package) = state.input_stream_bytes_per_package.get_mut(package_id) else {
        state.accounting_healthy = false;
        return;
    };
    let Some(remaining) = package.checked_sub(bytes) else {
        state.accounting_healthy = false;
        return;
    };
    if remaining == 0 {
        state.input_stream_bytes_per_package.remove(package_id);
    } else {
        *package = remaining;
    }
}

fn fail_close_ticket(
    state: &Arc<Mutex<RuntimeStateV1>>,
    job: JobHandleV1,
    reason: ProducerRevocationReasonV1,
) {
    let active = {
        let Ok(mut state) = state.lock() else {
            return;
        };
        let Some(runtime_job) = state.jobs.get(&job) else {
            return;
        };
        let provider_call_active = runtime_job.provider_call_active;
        revoke_generation_and_close(&mut state, job, reason);
        if let Some(runtime_job) = state.jobs.get_mut(&job) {
            runtime_job.finalization = JobFinalizationV1::RetirementAuthorized;
        }
        provider_call_active
    };
    if !active {
        let _ = retire_job(state, job);
    }
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

fn submit_progress_for_state(
    runtime: &Arc<Mutex<RuntimeStateV1>>,
    update: JobProgressUpdateV1,
    invocation: InvocationGenerationsV1,
) -> JobProgressStatusV1 {
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
    let producer_revoked = state.jobs.get(&update.job).is_some_and(|job| {
        state
            .revoked_producers
            .contains_key(&ProducerGenerationKeyV1::from(&job.authority.producer))
    });
    let Some(job) = state.jobs.get_mut(&update.job) else {
        return JobProgressStatusV1::CLOSED;
    };
    if producer_revoked {
        return JobProgressStatusV1::CLOSED;
    }
    if job.owner_thread != Some(std::thread::current().id()) {
        return JobProgressStatusV1::WRONG_THREAD;
    }
    if !invocation.matches(job) {
        return JobProgressStatusV1::STALE;
    }
    if job.terminal.is_some() || job.control.into_raw() != JobControlStateV1::ACTIVE.into_raw() {
        return JobProgressStatusV1::CLOSED;
    }
    if !result_job_is_ui_current(job) {
        job.pending_progress = None;
        return JobProgressStatusV1::STALE;
    }
    if !job.provider_call_active
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
        quarantine_and_close(&mut state, update.job);
        return JobProgressStatusV1::INVALID;
    }
    let Some(next_sequence) = job.next_progress_sequence.checked_add(1) else {
        return JobProgressStatusV1::CLOSED;
    };
    job.next_progress_sequence = next_sequence;
    job.pending_progress = Some(update);
    JobProgressStatusV1::ACCEPTED
}

#[derive(Clone, Copy, Debug)]
struct SubmitDecisionV1 {
    status: SinkSubmitStatusV1,
    batches: u32,
    items: u32,
    bytes: u64,
    ready_signal: Option<ExtensionJobUiReadySignalV1>,
}

impl SubmitDecisionV1 {
    const fn from_credits(status: SinkSubmitStatusV1, credits: (u32, u32, u64)) -> Self {
        Self {
            status,
            batches: credits.0,
            items: credits.1,
            bytes: credits.2,
            ready_signal: None,
        }
    }

    const fn with_ready_signal(mut self, ready_signal: ExtensionJobUiReadySignalV1) -> Self {
        self.ready_signal = Some(ready_signal);
        self
    }

    fn into_outcome(self, batch: IncrementalResultBatchV1) -> SinkSubmitOutcomeV1 {
        if self.status == SinkSubmitStatusV1::ACCEPTED {
            // The batch's ABI-owned buffers are released after the lock above.
            drop(batch);
            rejected_without_batch(self.status, self.batches, self.items, self.bytes)
        } else {
            rejected(self.status, batch, self.batches, self.items, self.bytes)
        }
    }
}

fn submit_locked(
    state: &mut RuntimeStateV1,
    batch: &IncrementalResultBatchV1,
    invocation: InvocationGenerationsV1,
) -> SubmitDecisionV1 {
    let Some(job) = state.jobs.get(&batch.job) else {
        return SubmitDecisionV1::from_credits(SinkSubmitStatusV1::CLOSED, (0, 0, 0));
    };
    let credits = remaining_credits(state, job);
    if state
        .revoked_producers
        .contains_key(&ProducerGenerationKeyV1::from(&job.authority.producer))
    {
        return SubmitDecisionV1::from_credits(SinkSubmitStatusV1::CLOSED, credits);
    }
    if job.owner_thread != Some(std::thread::current().id()) {
        return SubmitDecisionV1::from_credits(SinkSubmitStatusV1::WRONG_THREAD, credits);
    }
    if !invocation.matches(job) {
        return SubmitDecisionV1::from_credits(SinkSubmitStatusV1::STALE, credits);
    }
    if job.terminal.is_some() || job.control.into_raw() != JobControlStateV1::ACTIVE.into_raw() {
        return SubmitDecisionV1::from_credits(SinkSubmitStatusV1::CLOSED, credits);
    }
    if !job_sources_current(job) {
        return SubmitDecisionV1::from_credits(SinkSubmitStatusV1::STALE, credits);
    }
    if !job.provider_call_active
        || batch.sink_capability != job.sink_capability
        || batch.job_generation != batch.job.generation()
        || batch.job_generation != job.job_generation
        || batch.location != job.location
        || batch.location_generation != job.location_generation
        || batch.source_generation != job.source_generation
        || batch.sequence != job.next_sequence
    {
        return SubmitDecisionV1::from_credits(SinkSubmitStatusV1::STALE, credits);
    }
    let Ok(bytes) = validate_batch_preflight(job, batch) else {
        quarantine_and_close(state, batch.job);
        return SubmitDecisionV1::from_credits(SinkSubmitStatusV1::INVALID, credits);
    };
    let items = batch.entries.len();
    let exceeds_byte_credits = match u64::try_from(bytes) {
        Ok(bytes) => bytes > credits.2,
        Err(_) => true,
    };
    if items > credits.1 as usize || exceeds_byte_credits || credits.0 == 0 {
        return SubmitDecisionV1::from_credits(SinkSubmitStatusV1::WOULD_BLOCK, credits);
    }
    // Allocation and deep-copy happen only after every borrowed field and the
    // aggregate byte budget have passed the first-phase validation.
    let Ok(entries) = ingest_batch(job, batch) else {
        quarantine_and_close(state, batch.job);
        return SubmitDecisionV1::from_credits(SinkSubmitStatusV1::INVALID, credits);
    };
    let sequence = batch.sequence;
    let Some(next_sequence) = job.next_sequence.checked_add(1) else {
        return SubmitDecisionV1::from_credits(SinkSubmitStatusV1::CLOSED, credits);
    };
    {
        let Some(job) = state.jobs.get_mut(&batch.job) else {
            return SubmitDecisionV1::from_credits(SinkSubmitStatusV1::CLOSED, credits);
        };
        if !job.batch_input_streams.is_empty() {
            // `validate_batch_preflight` established exact item cardinality
            // and order. A second accepted batch would necessarily duplicate
            // the same host items, so reject it rather than letting a caller
            // accidentally project it onto later requests.
            job.batch_result_submitted = true;
        }
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
        .entry(state.jobs[&batch.job].authority.producer.package_id.clone())
        .or_default();
    package_usage.batches += 1;
    package_usage.items += items;
    package_usage.bytes += bytes;
    let (remaining, ready_signal) = state.jobs.get(&batch.job).map_or(((0, 0, 0), None), |job| {
        (
            remaining_credits(state, job),
            Some(ExtensionJobUiReadySignalV1::from_runtime(
                batch.job,
                job.item_generation,
                job.location_generation,
                job.source_generation,
            )),
        )
    });
    let decision = SubmitDecisionV1::from_credits(SinkSubmitStatusV1::ACCEPTED, remaining);
    ready_signal.map_or(decision, |signal| decision.with_ready_signal(signal))
}

/// Revokes one exact package/digest/feature/epoch producer generation before
/// any queued, pre-drained, or renderer-routed data can be observed again.
/// Lifecycle cancellation is intentionally distinct from malformed transport:
/// only the latter records a protocol quarantine.
fn revoke_generation_and_close(
    state: &mut RuntimeStateV1,
    job: JobHandleV1,
    reason: ProducerRevocationReasonV1,
) {
    let Some(runtime_job) = state.jobs.get(&job) else {
        return;
    };
    let producer = runtime_job.authority.producer.clone();
    let item = runtime_job.item;
    let location = runtime_job.location;
    let job_generation = runtime_job.job_generation;
    let item_generation = runtime_job.item_generation;
    let location_generation = runtime_job.location_generation;
    let source_generation = runtime_job.source_generation;
    let producer_key = ProducerGenerationKeyV1::from(&producer);
    // This is a producer-generation action, never merely a single request
    // action.  Do not overwrite an earlier stronger/equally durable reason.
    if state.revoked_producers.contains_key(&producer_key) {
        return;
    }
    state.revoked_producers.insert(producer_key.clone(), reason);
    revoke_registered_generation_tombstones(state, &producer_key);
    let package_id = producer.package_id.clone();
    let mut released = QueueUsageV1::default();
    let siblings = state
        .jobs
        .iter()
        .filter_map(|(handle, sibling)| {
            (ProducerGenerationKeyV1::from(&sibling.authority.producer) == producer_key)
                .then_some(*handle)
        })
        .collect::<Vec<_>>();
    for handle in siblings {
        let Some(sibling) = state.jobs.get_mut(&handle) else {
            continue;
        };
        let sibling_released = QueueUsageV1 {
            batches: sibling.queued_batches.len(),
            items: sibling.queued_items,
            bytes: sibling.queued_bytes,
        };
        let (Some(batches), Some(items), Some(bytes)) = (
            released.batches.checked_add(sibling_released.batches),
            released.items.checked_add(sibling_released.items),
            released.bytes.checked_add(sibling_released.bytes),
        ) else {
            state.accounting_healthy = false;
            return;
        };
        released = QueueUsageV1 {
            batches,
            items,
            bytes,
        };
        sibling.protocol_faulted = reason == ProducerRevocationReasonV1::ProtocolViolation;
        sibling.terminal = Some(match reason {
            ProducerRevocationReasonV1::LifecycleCancelled => JobTerminalV1::CANCELLED,
            ProducerRevocationReasonV1::ProtocolViolation
            | ProducerRevocationReasonV1::MarkerFailure => JobTerminalV1::INCOMPATIBLE,
        });
        sibling.control = JobControlStateV1::CLOSED;
        sibling.value_generation.revoke();
        // A job revoked before callback entry has no outstanding marker or
        // foreign stack frame and can retire immediately. Once callback entry
        // began, retain the lease through marker clear/fail-close even after
        // `provider_call_active` becomes false on callback return.
        if !sibling.provider_call_active && sibling.finalization == JobFinalizationV1::Idle {
            sibling.finalization = JobFinalizationV1::RetirementAuthorized;
        }
        sibling.pending_progress = None;
        sibling.queued_batches.clear();
        sibling.queued_items = 0;
        sibling.queued_bytes = 0;
        remove_applied_rows(state, handle);
    }
    release_credits(state, &package_id, released);
    if reason == ProducerRevocationReasonV1::ProtocolViolation {
        if state.quarantines.len() == MAX_QUARANTINE_EVENTS_V1 {
            let _ = state.quarantines.pop_front();
        }
        state.quarantines.push_back(ExtensionJobQuarantineEventV1 {
            producer,
            job,
            item,
            location,
            job_generation,
            item_generation,
            location_generation,
            source_generation,
        });
    }
}

/// Invalid ABI transport is terminal immediately: accepted data/progress cannot
/// leak past a protocol fault, and the producer generation is quarantined.
fn quarantine_and_close(state: &mut RuntimeStateV1, job: JobHandleV1) {
    revoke_generation_and_close(state, job, ProducerRevocationReasonV1::ProtocolViolation);
}

fn validate_batch_preflight(
    job: &RuntimeJobV1,
    batch: &IncrementalResultBatchV1,
) -> Result<usize, ()> {
    if batch.entries.is_empty() || batch.entries.len() > MAX_INCREMENTAL_RESULT_ITEMS_V1 {
        return Err(());
    }
    if !job.batch_input_streams.is_empty()
        && (job.batch_result_submitted
            || batch.entries.len() != job.batch_input_streams.len()
            || batch
                .entries
                .iter()
                .zip(&job.batch_input_streams)
                .any(|(entry, expected)| entry.item != expected.item))
    {
        // Batch providers receive host-minted capabilities in visible-file
        // order. Require one result for each input in that exact order so
        // reordered, duplicate, and omitted output all fail closed.
        return Err(());
    }
    let mut bytes = 0_usize;
    for entry in &batch.entries {
        let item_authorized = job.item == Some(entry.item)
            || job
                .batch_input_streams
                .iter()
                .any(|candidate| candidate.item == entry.item);
        if !item_authorized
            || entry.item_generation != job.item_generation
            || entry.source_generation != job.source_generation
        {
            return Err(());
        }
        bytes = bytes
            .checked_add(
                entry
                    .result
                    .validate_transport(job.expected_sort)
                    .map_err(|_| ())?,
            )
            .ok_or(())?;
        if bytes > MAX_INCREMENTAL_RESULT_BYTES_V1 {
            return Err(());
        }
    }
    Ok(bytes)
}

fn ingest_batch(
    job: &RuntimeJobV1,
    batch: &IncrementalResultBatchV1,
) -> Result<Vec<HostIncrementalResultEntryV1>, ()> {
    let mut entries = Vec::with_capacity(batch.entries.len());
    for entry in &batch.entries {
        entries.push(ingest_entry_v1(entry, job.expected_sort, &job.authority.producer).ok_or(())?);
    }
    Ok(entries)
}

fn remember_generation_tombstone(
    state: &mut RuntimeStateV1,
    key: ProducerGenerationKeyV1,
    generation: &ExtensionValueGenerationV1,
) {
    let entries = state.generation_tombstones.entry(key).or_default();
    entries.retain(ExtensionValueGenerationV1::weak_is_current);
    let downgrade = generation.downgrade();
    if !entries.iter().any(|existing| existing.ptr_eq(&downgrade)) {
        entries.push(downgrade);
    }
}

fn revoke_registered_generation_tombstones(
    state: &mut RuntimeStateV1,
    key: &ProducerGenerationKeyV1,
) {
    let Some(entries) = state.generation_tombstones.get_mut(key) else {
        return;
    };
    entries.retain(|weak| !ExtensionValueGenerationV1::revoke_weak(weak));
    if entries.is_empty() {
        state.generation_tombstones.remove(key);
    }
}

fn remaining_credits(state: &RuntimeStateV1, job: &RuntimeJobV1) -> (u32, u32, u64) {
    if !state.accounting_healthy {
        return (0, 0, 0);
    }
    let config = state.config;
    let package_usage = state
        .queued_per_package
        .get(&job.authority.producer.package_id)
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

fn rejected_without_batch(
    status: SinkSubmitStatusV1,
    batches: u32,
    items: u32,
    bytes: u64,
) -> SinkSubmitOutcomeV1 {
    SinkSubmitOutcomeV1 {
        status,
        remaining_batch_credits: batches,
        remaining_item_credits: items,
        remaining_byte_credits: bytes,
        rejected_batch: ROption::RNone,
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
    #[error(
        "extension job authority is not bound to the current sealed contribution and dispatch lease"
    )]
    UnauthorizedAuthority,
    #[error("extension input stream requires the sealed filesystem.read capability")]
    UnauthorizedInputStream,
    #[error("extension input stream byte credits are exhausted")]
    InputStreamCapacityExceeded,
    #[error("extension job capability collision")]
    CapabilityCollision,
    #[error("active extension job limit is reached")]
    ActiveJobLimitExceeded,
    #[error("operating-system entropy is unavailable")]
    EntropyUnavailable,
    #[error("extension job runtime state is poisoned")]
    StatePoisoned,
    #[error("extension job is unknown")]
    UnknownJob,
    #[error("provider call is inactive, mismatched, or already executing")]
    InactiveProviderCall,
    #[error("provider dispatch ticket was already invoked")]
    ProviderAlreadyInvoked,
    #[error("provider terminal can only publish once after marker clearing")]
    TerminalPublicationDenied,
    #[error("native provider marker could not be cleared; result generation was revoked")]
    MarkerClearFailed,
    #[error("provider dispatch control no longer matches current generations")]
    StaleControlHandle,
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
        sync::{
            Arc, Barrier, Mutex,
            atomic::{AtomicUsize, Ordering},
        },
        thread,
        time::{Duration, Instant},
    };

    use abi_stable::std_types::{RString, RVec};
    use explorer_extension_api::{
        IdNamespaceV1, IncrementalResultEntryV1, InputStreamReadRequestV1, InputStreamSeekOriginV1,
        InputStreamSeekRequestV1, InputStreamStatusV1, JobProviderImplementationV1,
        PluginItemOutcomeV1, PluginValueKindV1, PluginValueV1,
    };

    use crate::{
        ExtensionJobUiIngressV1, ExtensionJobUiPumpV1, UiInvalidationBatcherConfigV1,
        UiInvalidationBatcherV1,
    };
    use explorer_common::RequestDeadline;
    use explorer_jobs::{
        ExtensionCompletionOutcomeV1, ExtensionJobClassV1, ExtensionJobRequestV1,
        ExtensionJobSchedulerV1, ExtensionJobScopeV1, ExtensionPackageIdV1, ExtensionQueueLimitsV1,
        ExtensionScheduleOutcomeV1, ExtensionSchedulerConfigV1, JobPriority,
    };
    use explorer_model::CancellationToken;

    use super::*;

    fn config() -> ExtensionResultBufferConfigV1 {
        ExtensionResultBufferConfigV1::try_new(8, 2, 8, 1, 1, 32, 8, 4, 4096, 1024, 512).unwrap()
    }

    fn request(package_id: &str) -> ExtensionJobRuntimeRequestV1 {
        ExtensionJobRuntimeRequestV1 {
            authority: ExtensionJobAuthorityV1::for_test(package_id),
            job_generation: 1,
            item_generation: 1,
            location_generation: 1,
            source_generation: 1,
            has_item: true,
            input_stream: None,
        }
    }

    fn stream_request(
        package_id: &str,
        source: HostInputStreamSourceV1,
    ) -> ExtensionJobRuntimeRequestV1 {
        let mut request = request(package_id);
        request.authority = request.authority.with_filesystem_read_for_test();
        request.input_stream = Some(source);
        request
    }

    fn request_feature(
        package_id: &str,
        feature_id: &str,
        epoch: u64,
    ) -> ExtensionJobRuntimeRequestV1 {
        let mut request = request(package_id);
        request.authority.producer.feature_id = feature_id.to_owned();
        request.authority.producer.feature_epoch = epoch;
        request
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

    fn batch(context: &JobContextV1, sequence: u64) -> IncrementalResultBatchV1 {
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
                result: explorer_extension_api::PluginItemResultV1::value(
                    text_value(),
                    ROption::RNone,
                ),
            }]),
        }
    }

    fn progress(
        context: &JobContextV1,
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

    fn cache_lookup(
        runtime: &ExtensionJobRuntimeV1,
        context: &JobContextV1,
        watcher_scope: u128,
    ) -> ExtensionJobCacheLookupV1 {
        cache_lookup_at(runtime, context, watcher_scope, Instant::now())
    }

    fn cache_lookup_at(
        runtime: &ExtensionJobRuntimeV1,
        context: &JobContextV1,
        watcher_scope: u128,
        now: Instant,
    ) -> ExtensionJobCacheLookupV1 {
        runtime.lookup_cached_result_at(
            context.job,
            ExtensionResultCacheFileFactV1::from_host(7, 11),
            [3; 32],
            watcher_scope,
            runtime.cache_watcher_generation(watcher_scope),
            false,
            context.item_generation,
            context.location_generation,
            context.source_generation,
            now,
            |index| (format!("host-item-{index}"), 100 + index as u128),
        )
    }

    fn accepted_batch(
        runtime: &ExtensionJobRuntimeV1,
        context: &JobContextV1,
    ) -> AcceptedIncrementalResultBatchV1 {
        let scope = ProviderCallScopeV1::enter(&runtime.state, context).unwrap();
        assert_eq!(
            context.sink.try_submit(batch(context, 0)).status,
            SinkSubmitStatusV1::ACCEPTED
        );
        drop(scope);
        runtime
            .drain(
                context.job,
                context.item_generation,
                context.location_generation,
                context.source_generation,
                1,
            )
            .pop()
            .unwrap()
    }

    #[test]
    fn cache_hit_rebinds_to_revocable_runtime_and_cache_generations() {
        let runtime = ExtensionJobRuntimeV1::new(config());
        let context = runtime.open_job(request("cache")).unwrap();
        let admission = match cache_lookup(&runtime, &context, 91) {
            ExtensionJobCacheLookupV1::Miss(admission) => admission,
            other => panic!("expected cache miss, got {other:?}"),
        };
        let accepted = accepted_batch(&runtime, &context);
        assert_eq!(
            runtime.cache_accepted_batch(*admission, &accepted),
            ExtensionResultCacheInsertOutcomeV1::Inserted
        );
        let rows = match cache_lookup(&runtime, &context, 91) {
            ExtensionJobCacheLookupV1::Hit(rows) => rows,
            other => panic!("expected cache hit, got {other:?}"),
        };
        assert_eq!(rows[0].host_display_name(), "host-item-0");
        assert_eq!(rows[0].host_stable_item_identity(), 100);
        let producer = runtime.state.lock().unwrap().jobs[&context.job]
            .authority
            .producer
            .clone();
        runtime.result_cache().invalidate_manual(&producer);
        assert_eq!(rows[0].outcome(), PluginItemOutcomeV1::INCOMPATIBLE);
        assert!(matches!(
            cache_lookup(&runtime, &context, 91),
            ExtensionJobCacheLookupV1::Miss(_)
        ));
    }

    #[test]
    fn cache_hit_identity_mapper_can_reenter_runtime_after_validation() {
        let runtime = ExtensionJobRuntimeV1::new(config());
        let context = runtime
            .open_job(request("cache-reentrant-identity"))
            .unwrap();
        let admission = match cache_lookup(&runtime, &context, 94) {
            ExtensionJobCacheLookupV1::Miss(admission) => admission,
            other => panic!("expected miss, got {other:?}"),
        };
        let accepted = accepted_batch(&runtime, &context);
        assert_eq!(
            runtime.cache_accepted_batch(*admission, &accepted),
            ExtensionResultCacheInsertOutcomeV1::Inserted
        );
        let rows = match runtime.lookup_cached_result(
            context.job,
            ExtensionResultCacheFileFactV1::from_host(7, 11),
            [3; 32],
            94,
            runtime.cache_watcher_generation(94),
            false,
            context.item_generation,
            context.location_generation,
            context.source_generation,
            |_| {
                assert!(runtime.is_result_generation_current(
                    context.job,
                    context.item_generation,
                    context.location_generation,
                    context.source_generation,
                ));
                ("reentrant-host-item".to_owned(), 700)
            },
        ) {
            ExtensionJobCacheLookupV1::Hit(rows) => rows,
            other => panic!("expected hit, got {other:?}"),
        };
        assert_eq!(rows[0].host_display_name(), "reentrant-host-item");
        assert_eq!(rows[0].host_stable_item_identity(), 700);
    }

    #[test]
    fn cache_admissions_reject_late_invalidation_and_cross_job_batches() {
        let runtime = ExtensionJobRuntimeV1::new(
            ExtensionResultBufferConfigV1::try_new(8, 8, 8, 8, 8, 32, 32, 32, 4096, 4096, 4096)
                .unwrap(),
        );
        let first = runtime.open_job(request("cache")).unwrap();
        let second = runtime.open_job(request("cache")).unwrap();
        let cross_job_admission = match cache_lookup(&runtime, &first, 92) {
            ExtensionJobCacheLookupV1::Miss(admission) => admission,
            other => panic!("expected miss, got {other:?}"),
        };
        let second_batch = accepted_batch(&runtime, &second);
        assert_eq!(
            runtime.cache_accepted_batch(*cross_job_admission, &second_batch),
            ExtensionResultCacheInsertOutcomeV1::RejectedStale
        );

        let first_batch = accepted_batch(&runtime, &first);
        let producer = first_batch.producer.clone();
        let manual = match cache_lookup(&runtime, &first, 92) {
            ExtensionJobCacheLookupV1::Miss(admission) => admission,
            other => panic!("expected miss, got {other:?}"),
        };
        runtime.result_cache().invalidate_manual(&producer);
        assert_eq!(
            runtime.cache_accepted_batch(*manual, &first_batch),
            ExtensionResultCacheInsertOutcomeV1::RejectedStale
        );

        let watcher = match cache_lookup(&runtime, &first, 92) {
            ExtensionJobCacheLookupV1::Miss(admission) => admission,
            other => panic!("expected miss, got {other:?}"),
        };
        runtime.result_cache().invalidate_watcher_scope(92);
        assert_eq!(
            runtime.cache_accepted_batch(*watcher, &first_batch),
            ExtensionResultCacheInsertOutcomeV1::RejectedStale
        );

        let data_version = match cache_lookup(&runtime, &first, 93) {
            ExtensionJobCacheLookupV1::Miss(admission) => admission,
            other => panic!("expected miss, got {other:?}"),
        };
        let mut newer = producer;
        newer.data_version = newer.data_version.saturating_add(1);
        runtime.result_cache().invalidate_data_version(&newer);
        assert_eq!(
            runtime.cache_accepted_batch(*data_version, &first_batch),
            ExtensionResultCacheInsertOutcomeV1::RejectedStale
        );
    }

    #[test]
    fn cache_ttl_and_watcher_rollover_reject_replay_at_exact_boundaries() {
        let runtime = ExtensionJobRuntimeV1::new(config());
        let context = runtime.open_job(request("cache-boundary")).unwrap();
        let start = Instant::now();
        let admission = match cache_lookup_at(&runtime, &context, 101, start) {
            ExtensionJobCacheLookupV1::Miss(admission) => admission,
            other => panic!("expected miss, got {other:?}"),
        };
        let accepted = accepted_batch(&runtime, &context);
        assert_eq!(
            runtime
                .result_cache()
                .insert_batch_at(*admission, &accepted, start),
            ExtensionResultCacheInsertOutcomeV1::Inserted
        );
        assert!(matches!(
            cache_lookup_at(
                &runtime,
                &context,
                101,
                start
                    + Duration::from_secs(29)
                    + Duration::from_millis(999)
                    + Duration::from_micros(999)
                    + Duration::from_nanos(999)
            ),
            ExtensionJobCacheLookupV1::Hit(_)
        ));
        assert!(matches!(
            cache_lookup_at(&runtime, &context, 101, start + Duration::from_secs(30)),
            ExtensionJobCacheLookupV1::Miss(_)
        ));

        let cache = Arc::new(ExtensionResultCacheV1::new(
            ExtensionResultCacheConfigV1::try_new(
                1,
                1,
                1,
                4096,
                4096,
                4096,
                Duration::from_secs(1),
            )
            .unwrap(),
        ));
        let bounded = ExtensionJobRuntimeV1::new_with_result_cache(config(), Arc::clone(&cache));
        let bounded_context = bounded.open_job(request("watcher-rollover")).unwrap();
        let old = bounded.cache_watcher_generation(1);
        let _ = bounded.cache_watcher_generation(2); // evicts scope 1 from the bounded registry
        assert!(matches!(
            bounded.lookup_cached_result_at(
                bounded_context.job,
                ExtensionResultCacheFileFactV1::from_host(7, 11),
                [3; 32],
                1,
                old,
                false,
                1,
                1,
                1,
                Instant::now(),
                |_| ("host".to_owned(), 1),
            ),
            ExtensionJobCacheLookupV1::RejectedStale
        ));
        assert!(bounded.cache_watcher_generation(1) > old);
    }

    #[test]
    fn cache_separates_contributions_data_versions_and_current_views() {
        let runtime = ExtensionJobRuntimeV1::new(
            ExtensionResultBufferConfigV1::try_new(8, 8, 8, 8, 8, 32, 32, 32, 4096, 4096, 4096)
                .unwrap(),
        );
        let first = runtime.open_job(request("cache-isolation")).unwrap();
        let admission = match cache_lookup(&runtime, &first, 111) {
            ExtensionJobCacheLookupV1::Miss(admission) => admission,
            other => panic!("expected miss, got {other:?}"),
        };
        let accepted = accepted_batch(&runtime, &first);
        assert_eq!(
            runtime.cache_accepted_batch(*admission, &accepted),
            ExtensionResultCacheInsertOutcomeV1::Inserted
        );

        let mut contribution_request = request("cache-isolation");
        contribution_request.authority.producer.contribution_id = "other-column".to_owned();
        let contribution = runtime.open_job(contribution_request).unwrap();
        assert!(matches!(
            cache_lookup(&runtime, &contribution, 111),
            ExtensionJobCacheLookupV1::Miss(_)
        ));

        let mut version_request = request("cache-isolation");
        version_request.authority.producer.data_version = 2;
        let version = runtime.open_job(version_request).unwrap();
        assert!(matches!(
            cache_lookup(&runtime, &version, 111),
            ExtensionJobCacheLookupV1::Miss(_)
        ));

        let mut second_request = request("cache-isolation");
        second_request.item_generation = 2;
        second_request.location_generation = 2;
        second_request.source_generation = 2;
        let second = runtime.open_job(second_request).unwrap();
        assert!(matches!(
            cache_lookup(&runtime, &second, 111),
            ExtensionJobCacheLookupV1::Hit(_)
        ));
        let first_rows = match cache_lookup(&runtime, &first, 111) {
            ExtensionJobCacheLookupV1::Hit(rows) => rows,
            other => panic!("expected first hit, got {other:?}"),
        };
        runtime
            .update_current_generations(first.job, 2, 2, 2)
            .unwrap();
        assert_eq!(first_rows[0].outcome(), PluginItemOutcomeV1::INCOMPATIBLE);
        assert!(matches!(
            cache_lookup(&runtime, &second, 111),
            ExtensionJobCacheLookupV1::Hit(_)
        ));
    }

    #[test]
    fn cache_capacity_is_exact_replaces_in_place_and_reclaims_expired_entries() {
        let cache = Arc::new(ExtensionResultCacheV1::new(
            ExtensionResultCacheConfigV1::try_new(
                1,
                1,
                1,
                4096,
                4096,
                4096,
                Duration::from_nanos(1),
            )
            .unwrap(),
        ));
        let runtime = ExtensionJobRuntimeV1::new_with_result_cache(config(), Arc::clone(&cache));
        let context = runtime.open_job(request("cache-capacity")).unwrap();
        let now = Instant::now();
        let first = match cache_lookup_at(&runtime, &context, 121, now) {
            ExtensionJobCacheLookupV1::Miss(admission) => admission,
            other => panic!("expected first miss, got {other:?}"),
        };
        let replacement = match cache_lookup_at(&runtime, &context, 121, now) {
            ExtensionJobCacheLookupV1::Miss(admission) => admission,
            other => panic!("expected replacement miss, got {other:?}"),
        };
        let accepted = accepted_batch(&runtime, &context);
        assert_eq!(
            cache.insert_batch_at(*first, &accepted, now),
            ExtensionResultCacheInsertOutcomeV1::Inserted
        );
        assert_eq!(
            cache.insert_batch_at(*replacement, &accepted, now),
            ExtensionResultCacheInsertOutcomeV1::Inserted
        );

        let expired_at = now + Duration::from_nanos(1);
        let expired = match cache_lookup_at(&runtime, &context, 121, expired_at) {
            ExtensionJobCacheLookupV1::Miss(admission) => admission,
            other => panic!("expected expiry miss, got {other:?}"),
        };
        assert_eq!(
            cache.insert_batch_at(*expired, &accepted, expired_at),
            ExtensionResultCacheInsertOutcomeV1::Inserted
        );

        let exact_bytes = accepted.cache_bytes;
        assert!(exact_bytes > 1);
        let tight_cache = ExtensionResultCacheV1::new(
            ExtensionResultCacheConfigV1::try_new(
                1,
                1,
                1,
                exact_bytes,
                exact_bytes,
                exact_bytes,
                Duration::from_secs(1),
            )
            .unwrap(),
        );
        let generation = ExtensionResultCacheGenerationV1::from_host(1, 1, 1).unwrap();
        let producer = accepted.producer.clone();
        let key = ExtensionResultCacheKeyV1::from_host(
            &producer,
            ExtensionResultCacheFileFactV1::from_host(7, 11),
            [8; 32],
            122,
            tight_cache.watcher_generation(122),
            false,
        )
        .unwrap();
        let tight_admission = match tight_cache.lookup_at(key.clone(), generation, now) {
            ExtensionResultCacheLookupV1::Miss(mut admission) => {
                admission.bind_job(context.job);
                admission
            }
            other => panic!("expected exact-limit miss, got {other:?}"),
        };
        assert_eq!(
            tight_cache.insert_batch_at(*tight_admission, &accepted, now),
            ExtensionResultCacheInsertOutcomeV1::Inserted
        );
        let different_key = ExtensionResultCacheKeyV1::from_host(
            &producer,
            ExtensionResultCacheFileFactV1::from_host(7, 11),
            [9; 32],
            122,
            tight_cache.watcher_generation(122),
            false,
        )
        .unwrap();
        let oversized = match tight_cache.lookup_at(different_key, generation, now) {
            ExtensionResultCacheLookupV1::Miss(mut admission) => {
                admission.bind_job(context.job);
                admission
            }
            other => panic!("expected capacity miss, got {other:?}"),
        };
        assert_eq!(
            tight_cache.insert_batch_at(*oversized, &accepted, now),
            ExtensionResultCacheInsertOutcomeV1::RejectedCapacity
        );
    }

    #[test]
    fn input_stream_is_capability_bound_bounded_and_generation_safe() {
        let source =
            HostInputStreamSourceV1::from_host_snapshot(b"abcdef".to_vec(), 1, true).unwrap();
        let runtime = ExtensionJobRuntimeV1::new(config());
        let context = runtime
            .open_job(stream_request("stream", source.clone()))
            .unwrap();
        let stream = context.input.clone().into_option().unwrap();
        let scope = ProviderCallScopeV1::enter(&runtime.state, &context).unwrap();

        let zero = stream.read(InputStreamReadRequestV1 {
            maximum_bytes: 0,
            reserved: 0,
        });
        assert_eq!(zero.status, InputStreamStatusV1::OK);
        assert!(zero.data.is_empty());
        let first = stream.read(InputStreamReadRequestV1 {
            maximum_bytes: 2,
            reserved: 0,
        });
        assert_eq!(first.status, InputStreamStatusV1::OK);
        assert_eq!(first.data.as_slice(), b"ab");
        let seek = stream.seek(InputStreamSeekRequestV1 {
            origin: InputStreamSeekOriginV1::CURRENT,
            reserved: 0,
            offset: -1,
        });
        assert_eq!(seek.status, InputStreamStatusV1::OK);
        assert_eq!(seek.position, 1);
        let partial = stream.read(InputStreamReadRequestV1 {
            maximum_bytes: 64,
            reserved: 0,
        });
        assert_eq!(partial.status, InputStreamStatusV1::OK);
        assert_eq!(partial.data.as_slice(), b"bcdef");
        assert_eq!(
            stream
                .read(InputStreamReadRequestV1 {
                    maximum_bytes: 1,
                    reserved: 0
                })
                .status,
            InputStreamStatusV1::EOF
        );
        assert_eq!(
            stream
                .read(InputStreamReadRequestV1 {
                    maximum_bytes: MAX_INPUT_STREAM_READ_BYTES_V1 + 1,
                    reserved: 0
                })
                .status,
            InputStreamStatusV1::INVALID
        );
        assert_eq!(
            stream
                .read(InputStreamReadRequestV1 {
                    maximum_bytes: 1,
                    reserved: 1
                })
                .status,
            InputStreamStatusV1::INVALID
        );
        assert_eq!(
            stream
                .seek(InputStreamSeekRequestV1 {
                    origin: InputStreamSeekOriginV1::START,
                    reserved: 0,
                    offset: -1
                })
                .status,
            InputStreamStatusV1::INVALID
        );
        assert_eq!(
            stream
                .seek(InputStreamSeekRequestV1 {
                    origin: InputStreamSeekOriginV1::END,
                    reserved: 0,
                    offset: i64::MAX
                })
                .status,
            InputStreamStatusV1::INVALID
        );
        assert_eq!(stream.length().length, 6);

        let foreign = stream.clone();
        assert_eq!(
            thread::spawn(move || foreign.length().status)
                .join()
                .unwrap(),
            InputStreamStatusV1::WRONG_THREAD
        );
        drop(scope);
        assert_eq!(stream.length().status, InputStreamStatusV1::CLOSED);
        drop(context);
        drop(source);
    }

    #[test]
    fn input_stream_denies_unsealed_capability_source_changes_and_control() {
        let source = HostInputStreamSourceV1::from_host_snapshot(vec![1, 2], 1, false).unwrap();
        let runtime = ExtensionJobRuntimeV1::new(config());
        let mut denied = request("stream-denied");
        denied.input_stream = Some(source.clone());
        assert!(matches!(
            runtime.open_job(denied),
            Err(ExtensionJobRuntimeErrorV1::UnauthorizedInputStream)
        ));
        let mut mismatched = stream_request("stream-mismatch", source.clone());
        mismatched.source_generation = 2;
        assert!(matches!(
            runtime.open_job(mismatched),
            Err(ExtensionJobRuntimeErrorV1::InvalidRequest)
        ));

        let context = runtime
            .open_job(stream_request("stream-live", source.clone()))
            .unwrap();
        let stream = context.input.clone().into_option().unwrap();
        let scope = ProviderCallScopeV1::enter(&runtime.state, &context).unwrap();
        assert_eq!(stream.length().status, InputStreamStatusV1::OK);
        assert_eq!(
            stream
                .seek(InputStreamSeekRequestV1 {
                    origin: InputStreamSeekOriginV1::START,
                    reserved: 0,
                    offset: 0
                })
                .status,
            InputStreamStatusV1::UNSUPPORTED
        );
        assert!(source.replace_host_snapshot(vec![9, 9], 2, false));
        assert_eq!(
            stream
                .read(InputStreamReadRequestV1 {
                    maximum_bytes: 1,
                    reserved: 0
                })
                .status,
            InputStreamStatusV1::STALE
        );
        drop(scope);

        let source = HostInputStreamSourceV1::from_host_snapshot(vec![1], 1, true).unwrap();
        let context = runtime
            .open_job(stream_request("stream-control", source))
            .unwrap();
        let stream = context.input.clone().into_option().unwrap();
        let scope = ProviderCallScopeV1::enter(&runtime.state, &context).unwrap();
        runtime
            .request_control(context.job, JobControlStateV1::CANCELLED)
            .unwrap();
        assert_eq!(context.poll_control(), JobControlStateV1::CANCELLED);
        assert_eq!(stream.length().status, InputStreamStatusV1::CANCELLED);
        drop(scope);

        let source = HostInputStreamSourceV1::from_host_snapshot(vec![1], 1, true).unwrap();
        let context = runtime
            .open_job(stream_request("stream-deadline", source))
            .unwrap();
        let stream = context.input.clone().into_option().unwrap();
        let scope = ProviderCallScopeV1::enter(&runtime.state, &context).unwrap();
        runtime
            .request_control(context.job, JobControlStateV1::DEADLINE_ELAPSED)
            .unwrap();
        assert_eq!(context.poll_control(), JobControlStateV1::DEADLINE_ELAPSED);
        assert_eq!(
            stream
                .read(InputStreamReadRequestV1 {
                    maximum_bytes: 1,
                    reserved: 0
                })
                .status,
            InputStreamStatusV1::DEADLINE_ELAPSED
        );
        drop(scope);

        let source = HostInputStreamSourceV1::from_host_snapshot(vec![1], 1, true).unwrap();
        let context = runtime
            .open_job(stream_request(
                "stream-progress-source-change",
                source.clone(),
            ))
            .unwrap();
        let scope = ProviderCallScopeV1::enter(&runtime.state, &context).unwrap();
        assert_eq!(
            context.progress.try_submit(progress(&context, 0, 1, 2)),
            JobProgressStatusV1::ACCEPTED
        );
        assert!(source.replace_host_snapshot(vec![2], 2, true));
        assert_eq!(context.poll_control(), JobControlStateV1::CLOSED);
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
            context.progress.try_submit(progress(&context, 1, 2, 2)),
            JobProgressStatusV1::STALE
        );
        drop(scope);
    }

    #[test]
    fn retained_input_stream_clone_cannot_pin_source_after_job_retirement() {
        let runtime = ExtensionJobRuntimeV1::new(config());
        let source = HostInputStreamSourceV1::from_host_snapshot(vec![1, 2, 3], 1, true).unwrap();
        let weak = source.downgrade();
        let context = runtime
            .open_job(stream_request("stream-retire", source.clone()))
            .unwrap();
        let retained = context.input.clone().into_option().unwrap();
        let scope = ProviderCallScopeV1::enter(&runtime.state, &context).unwrap();
        drop(scope);
        assert!(matches!(
            runtime.finish(context.job, JobTerminalV1::COMPLETED),
            ExtensionJobFinishOutcomeV1::Published(_)
                | ExtensionJobFinishOutcomeV1::AlreadyTerminal(_)
        ));
        retire_job(&runtime.state, context.job).unwrap();
        drop(context);
        drop(source);
        assert!(weak.upgrade().is_none());
        assert_eq!(retained.length().status, InputStreamStatusV1::CLOSED);
    }

    #[test]
    fn source_change_cannot_publish_before_submit_drain_or_apply() {
        let runtime = ExtensionJobRuntimeV1::new(
            ExtensionResultBufferConfigV1::try_new(8, 8, 8, 8, 8, 32, 32, 32, 4096, 4096, 4096)
                .unwrap(),
        );

        let before_submit = HostInputStreamSourceV1::from_host_snapshot(vec![1], 1, true).unwrap();
        let context = runtime
            .open_job(stream_request("stream-race", before_submit.clone()))
            .unwrap();
        let stream = context.input.clone().into_option().unwrap();
        let scope = ProviderCallScopeV1::enter(&runtime.state, &context).unwrap();
        assert_eq!(
            stream
                .read(InputStreamReadRequestV1 {
                    maximum_bytes: 1,
                    reserved: 0
                })
                .status,
            InputStreamStatusV1::OK
        );
        assert!(before_submit.replace_host_snapshot(vec![2], 2, true));
        assert_eq!(
            context.sink.try_submit(batch(&context, 0)).status,
            SinkSubmitStatusV1::STALE
        );
        drop(scope);

        let before_drain = HostInputStreamSourceV1::from_host_snapshot(vec![1], 1, true).unwrap();
        let context = runtime
            .open_job(stream_request("stream-race", before_drain.clone()))
            .unwrap();
        let scope = ProviderCallScopeV1::enter(&runtime.state, &context).unwrap();
        assert_eq!(
            context.sink.try_submit(batch(&context, 0)).status,
            SinkSubmitStatusV1::ACCEPTED
        );
        drop(scope);
        assert!(before_drain.replace_host_snapshot(vec![2], 2, true));
        assert!(runtime.drain(context.job, 1, 1, 1, 1).is_empty());

        let before_apply = HostInputStreamSourceV1::from_host_snapshot(vec![1], 1, true).unwrap();
        let context = runtime
            .open_job(stream_request("stream-race", before_apply.clone()))
            .unwrap();
        let scope = ProviderCallScopeV1::enter(&runtime.state, &context).unwrap();
        assert_eq!(
            context.sink.try_submit(batch(&context, 0)).status,
            SinkSubmitStatusV1::ACCEPTED
        );
        drop(scope);
        let accepted = runtime.drain(context.job, 1, 1, 1, 1);
        assert_eq!(accepted.len(), 1);
        assert!(before_apply.replace_host_snapshot(vec![2], 2, true));
        assert!(
            runtime
                .apply_accepted_batch(&accepted[0], |_| ("stream".to_owned(), 1))
                .is_none()
        );

        // Updating to a new host source generation creates a new composite
        // value token. A later source replacement must revoke rows published
        // for that second generation even if the runtime has not otherwise
        // advanced its item/location generations.
        let successive = HostInputStreamSourceV1::from_host_snapshot(vec![1], 1, true).unwrap();
        let old_context = runtime
            .open_job(stream_request(
                "stream-successive-generation",
                successive.clone(),
            ))
            .unwrap();
        let old_scope = ProviderCallScopeV1::enter(&runtime.state, &old_context).unwrap();
        assert!(successive.replace_host_snapshot(vec![2], 2, true));
        runtime
            .update_current_generations(old_context.job, 1, 1, 2)
            .unwrap();
        let mut forged_second_generation = batch(&old_context, 0);
        forged_second_generation.source_generation = 2;
        forged_second_generation.entries[0].source_generation = 2;
        assert_eq!(
            old_context.sink.try_submit(forged_second_generation).status,
            SinkSubmitStatusV1::STALE
        );
        drop(old_scope);

        let mut new_request = stream_request("stream-successive-generation", successive.clone());
        new_request.source_generation = 2;
        let context = runtime.open_job(new_request).unwrap();
        let scope = ProviderCallScopeV1::enter(&runtime.state, &context).unwrap();
        let second_generation = batch(&context, 0);
        assert_eq!(
            context.sink.try_submit(second_generation).status,
            SinkSubmitStatusV1::ACCEPTED
        );
        drop(scope);
        let accepted = runtime.drain(context.job, 1, 1, 2, 1);
        assert_eq!(accepted.len(), 1);
        let retained_rows = runtime
            .apply_accepted_batch(&accepted[0], |_| ("stream".to_owned(), 2))
            .unwrap();
        assert_eq!(retained_rows.len(), 1);
        assert!(successive.replace_host_snapshot(vec![3], 3, true));
        assert_eq!(
            retained_rows[0].outcome(),
            PluginItemOutcomeV1::INCOMPATIBLE
        );
    }

    #[test]
    fn input_stream_byte_credits_bound_admission_and_reclaim_on_retirement() {
        let runtime = ExtensionJobRuntimeV1::new(
            ExtensionResultBufferConfigV1::try_new(4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4).unwrap(),
        );
        let first_source =
            HostInputStreamSourceV1::from_host_snapshot(vec![1, 2, 3], 1, true).unwrap();
        let first = runtime
            .open_job(stream_request("stream-credit", first_source.clone()))
            .unwrap();
        let second_source =
            HostInputStreamSourceV1::from_host_snapshot(vec![4, 5, 6], 1, true).unwrap();
        assert!(matches!(
            runtime.open_job(stream_request("stream-credit", second_source.clone())),
            Err(ExtensionJobRuntimeErrorV1::InputStreamCapacityExceeded)
        ));
        assert!(!first_source.replace_host_snapshot(vec![1, 2, 3, 4], 2, true));
        assert!(matches!(
            runtime.finish(first.job, JobTerminalV1::COMPLETED),
            ExtensionJobFinishOutcomeV1::Published(_)
        ));
        retire_job(&runtime.state, first.job).unwrap();
        let second = runtime
            .open_job(stream_request("stream-credit", second_source))
            .unwrap();
        assert!(matches!(
            runtime.finish(second.job, JobTerminalV1::COMPLETED),
            ExtensionJobFinishOutcomeV1::Published(_)
        ));
        retire_job(&runtime.state, second.job).unwrap();

        let no_input_one = runtime.open_job(request("no-input")).unwrap();
        let no_input_two = runtime.open_job(request("no-input")).unwrap();
        for context in [no_input_one, no_input_two] {
            assert!(matches!(
                runtime.finish(context.job, JobTerminalV1::COMPLETED),
                ExtensionJobFinishOutcomeV1::Published(_)
            ));
            retire_job(&runtime.state, context.job).unwrap();
        }
        for package_id in ["zero-input-one", "zero-input-two"] {
            let source = HostInputStreamSourceV1::from_host_snapshot(Vec::new(), 1, true).unwrap();
            let context = runtime
                .open_job(stream_request(package_id, source))
                .unwrap();
            assert!(matches!(
                runtime.finish(context.job, JobTerminalV1::COMPLETED),
                ExtensionJobFinishOutcomeV1::Published(_)
            ));
            retire_job(&runtime.state, context.job).unwrap();
        }
        let state = runtime.state.lock().unwrap();
        assert!(state.accounting_healthy);
        assert_eq!(state.input_stream_bytes, 0);
        assert!(state.input_stream_bytes_per_package.is_empty());
    }

    #[derive(Clone)]
    struct PreparedStreamProviderV1(Arc<Mutex<Option<InputStreamStatusV1>>>);

    impl JobProviderImplementationV1 for PreparedStreamProviderV1 {
        fn run(&self, context: JobContextV1) -> JobTerminalV1 {
            *self.0.lock().unwrap() = context
                .input
                .into_option()
                .map_or(Some(InputStreamStatusV1::CLOSED), |stream| {
                    Some(stream.length().status)
                });
            JobTerminalV1::COMPLETED
        }
    }

    #[derive(Clone)]
    struct BatchProbeProviderV1(Arc<Mutex<(usize, InputStreamStatusV1)>>);

    impl explorer_extension_api::BatchColumnProviderImplementationV1 for BatchProbeProviderV1 {
        fn run(&self, context: BatchColumnContextV1) -> JobTerminalV1 {
            let status = context.items[0].input.length().status;
            *self.0.lock().unwrap() = (context.items.len(), status);
            JobTerminalV1::COMPLETED
        }
    }

    #[derive(Clone)]
    struct ReorderedBatchProviderV1(Arc<Mutex<SinkSubmitStatusV1>>);

    impl explorer_extension_api::BatchColumnProviderImplementationV1 for ReorderedBatchProviderV1 {
        fn run(&self, context: BatchColumnContextV1) -> JobTerminalV1 {
            let entries = context
                .items
                .iter()
                .rev()
                .map(|item| IncrementalResultEntryV1 {
                    item: item.item,
                    item_generation: context.item_generation,
                    source_generation: context.source_generation,
                    result: explorer_extension_api::PluginItemResultV1::value(
                        text_value(),
                        ROption::RNone,
                    ),
                })
                .collect::<RVec<_>>();
            let outcome = context.try_submit(IncrementalResultBatchV1 {
                job: context.job,
                sink_capability: context.sink.capability,
                job_generation: context.job_generation,
                location: context.location,
                location_generation: context.location_generation,
                source_generation: context.source_generation,
                sequence: 0,
                entries,
            });
            *self.0.lock().unwrap() = outcome.status;
            JobTerminalV1::COMPLETED
        }
    }

    fn lock_owner_batch_request(
        package_id: &str,
        authority: ExtensionJobAuthorityV1,
        service_calls: Arc<AtomicUsize>,
    ) -> BatchColumnRuntimeRequestV1 {
        BatchColumnRuntimeRequestV1 {
            authority,
            job_generation: 1,
            item_generation: 1,
            location_generation: 1,
            source_generation: 1,
            items: vec![HostBatchColumnItemV1 {
                file_name: RString::from(format!("{package_id}.rs")),
                source: HostInputStreamSourceV1::from_host_snapshot(vec![1], 1, true).unwrap(),
                cache_identity: RString::new(),
                modified_unix_seconds: ROption::RNone,
                modified_subsec_nanos: 0,
                source_size: ROption::RNone,
                lock_owner_resource: Some(PathBuf::from(format!(r"C:\{package_id}.rs"))),
            }],
            lock_owner_query: Some(HostLockOwnerQueryServiceV1::new(move |_, _| {
                service_calls.fetch_add(1, Ordering::SeqCst);
                (LockOwnerQueryStatusV1::EMPTY, Vec::new())
            })),
        }
    }

    #[test]
    fn lock_owner_service_requires_declared_runtime_capability() {
        let runtime = ExtensionJobRuntimeV1::new(config());
        let calls = Arc::new(AtomicUsize::new(0));
        let request = lock_owner_batch_request(
            "lock-owner-denied",
            ExtensionJobAuthorityV1::for_test("lock-owner-denied").with_filesystem_read_for_test(),
            Arc::clone(&calls),
        );

        assert!(matches!(
            runtime.prepare_batch_column_dispatch(request),
            Err(ExtensionJobRuntimeErrorV1::UnauthorizedAuthority)
        ));
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn lock_owner_service_revalidates_after_feature_revoke_before_use() {
        let runtime = ExtensionJobRuntimeV1::new(config());
        let calls = Arc::new(AtomicUsize::new(0));
        let authority = ExtensionJobAuthorityV1::for_test("lock-owner-revoked")
            .with_filesystem_read_for_test()
            .with_lock_owner_query_for_direct_loader();
        let ticket = runtime
            .prepare_batch_column_dispatch(lock_owner_batch_request(
                "lock-owner-revoked",
                authority,
                Arc::clone(&calls),
            ))
            .unwrap();
        let item = ticket.context.items[0].item;
        let service = ticket
            .context
            .lock_owner_query
            .clone()
            .into_option()
            .unwrap();

        runtime.revoke_feature_generation("lock-owner-revoked", "test-digest", "test-feature", 1);
        let outcome = service.query(LockOwnerQueryRequestV1 {
            items: RVec::from(vec![item]),
            item_generation: 1,
            location_generation: 1,
            deadline_millis: 100,
            reserved: 0,
        });

        assert_eq!(outcome.status, LockOwnerQueryStatusV1::CANCELLED);
        assert!(outcome.owners.is_empty());
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn lock_owner_service_bounds_owned_results_and_utf8_display_names() {
        let runtime = ExtensionJobRuntimeV1::new(config());
        let authority = ExtensionJobAuthorityV1::for_test("lock-owner-bounds")
            .with_filesystem_read_for_test()
            .with_lock_owner_query_for_direct_loader();
        let mut request = lock_owner_batch_request(
            "lock-owner-bounds",
            authority,
            Arc::new(AtomicUsize::new(0)),
        );
        request.lock_owner_query = Some(HostLockOwnerQueryServiceV1::new(|_, _| {
            let owners = (0..(explorer_extension_api::MAX_LOCK_OWNER_QUERY_RESULTS_V1 + 32))
                .map(|process_id| LockOwnerRecordV1 {
                    item: ItemHandleV1::from_host([1; 16], 1),
                    process_id: process_id as u32,
                    application_type:
                        explorer_extension_api::LockOwnerApplicationTypeV1::MAIN_WINDOW,
                    display_name: RString::from("界".repeat(300)),
                    service_name: RString::from("服務".repeat(300)),
                })
                .collect();
            (LockOwnerQueryStatusV1::READY, owners)
        }));
        let ticket = runtime.prepare_batch_column_dispatch(request).unwrap();
        let item = ticket.context.items[0].item;
        let outcome = ticket
            .context
            .lock_owner_query
            .clone()
            .into_option()
            .unwrap()
            .query(LockOwnerQueryRequestV1 {
                items: RVec::from(vec![item]),
                item_generation: 1,
                location_generation: 1,
                deadline_millis: 100,
                reserved: 0,
            });

        assert_eq!(outcome.status, LockOwnerQueryStatusV1::READY);
        assert_eq!(
            outcome.owners.len(),
            explorer_extension_api::MAX_LOCK_OWNER_QUERY_RESULTS_V1
        );
        assert!(outcome.owners.iter().all(|owner| {
            owner.item == item
                && owner.display_name.len()
                    <= explorer_extension_api::MAX_LOCK_OWNER_DISPLAY_NAME_BYTES_V1
                && owner.service_name.len()
                    <= explorer_extension_api::MAX_LOCK_OWNER_DISPLAY_NAME_BYTES_V1
        }));
    }

    #[test]
    fn lock_owner_service_enforces_input_bound_and_deadline() {
        let runtime = ExtensionJobRuntimeV1::new(config());
        let calls = Arc::new(AtomicUsize::new(0));
        let authority = ExtensionJobAuthorityV1::for_test("lock-owner-limits")
            .with_filesystem_read_for_test()
            .with_lock_owner_query_for_direct_loader();
        let mut request =
            lock_owner_batch_request("lock-owner-limits", authority, Arc::clone(&calls));
        let delayed_calls = Arc::clone(&calls);
        request.lock_owner_query = Some(HostLockOwnerQueryServiceV1::new(move |_, _| {
            delayed_calls.fetch_add(1, Ordering::SeqCst);
            thread::sleep(Duration::from_millis(5));
            (LockOwnerQueryStatusV1::EMPTY, Vec::new())
        }));
        let ticket = runtime.prepare_batch_column_dispatch(request).unwrap();
        let item = ticket.context.items[0].item;
        let service = ticket
            .context
            .lock_owner_query
            .clone()
            .into_option()
            .unwrap();

        let oversized = service.query(LockOwnerQueryRequestV1 {
            items: RVec::from(vec![
                item;
                explorer_extension_api::MAX_LOCK_OWNER_QUERY_ITEMS_V1
                    + 1
            ]),
            item_generation: 1,
            location_generation: 1,
            deadline_millis: 100,
            reserved: 0,
        });
        assert_eq!(oversized.status, LockOwnerQueryStatusV1::UNAVAILABLE);
        assert_eq!(calls.load(Ordering::SeqCst), 0);

        let expired = service.query(LockOwnerQueryRequestV1 {
            items: RVec::from(vec![item]),
            item_generation: 1,
            location_generation: 1,
            deadline_millis: 1,
            reserved: 0,
        });
        assert_eq!(expired.status, LockOwnerQueryStatusV1::DEADLINE_ELAPSED);
        assert!(expired.owners.is_empty());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn batch_column_dispatch_is_bounded_host_attested_and_generation_safe() {
        let runtime = ExtensionJobRuntimeV1::new(config());
        let observed = Arc::new(Mutex::new((0, InputStreamStatusV1::CLOSED)));
        let provider =
            BatchColumnProviderObjectV1::new(BatchProbeProviderV1(Arc::clone(&observed)));
        let source_one = HostInputStreamSourceV1::from_host_snapshot(vec![1], 1, true).unwrap();
        let source_two = HostInputStreamSourceV1::from_host_snapshot(vec![2], 1, true).unwrap();
        let mut ticket = runtime
            .prepare_batch_column_dispatch(BatchColumnRuntimeRequestV1 {
                authority: ExtensionJobAuthorityV1::for_test("batch")
                    .with_filesystem_read_for_test(),
                job_generation: 1,
                item_generation: 1,
                location_generation: 1,
                source_generation: 1,
                items: vec![
                    HostBatchColumnItemV1 {
                        file_name: RString::from("one.rs"),
                        source: source_one,
                        cache_identity: RString::new(),
                        modified_unix_seconds: ROption::RNone,
                        modified_subsec_nanos: 0,
                        source_size: ROption::RNone,
                        lock_owner_resource: None,
                    },
                    HostBatchColumnItemV1 {
                        file_name: RString::from("two.py"),
                        source: source_two,
                        cache_identity: RString::new(),
                        modified_unix_seconds: ROption::RNone,
                        modified_subsec_nanos: 0,
                        source_size: ROption::RNone,
                        lock_owner_resource: None,
                    },
                ],
                lock_owner_query: None,
            })
            .unwrap();
        assert_eq!(
            ticket.invoke_once(&provider).unwrap(),
            JobTerminalV1::COMPLETED
        );
        assert_eq!(*observed.lock().unwrap(), (2, InputStreamStatusV1::OK));
        assert!(matches!(
            ticket.publish_terminal_after_marker_clear(JobTerminalV1::COMPLETED),
            Ok(ExtensionJobFinishOutcomeV1::Published(
                JobTerminalV1::COMPLETED
            ))
        ));

        let over_limit = (0..=MAX_BATCH_COLUMN_ITEMS_V1)
            .map(|_| HostBatchColumnItemV1 {
                file_name: RString::from("limit.rs"),
                source: HostInputStreamSourceV1::from_host_snapshot(vec![], 1, true).unwrap(),
                cache_identity: RString::new(),
                modified_unix_seconds: ROption::RNone,
                modified_subsec_nanos: 0,
                source_size: ROption::RNone,
                lock_owner_resource: None,
            })
            .collect();
        assert!(matches!(
            runtime.prepare_batch_column_dispatch(BatchColumnRuntimeRequestV1 {
                authority: ExtensionJobAuthorityV1::for_test("batch-limit")
                    .with_filesystem_read_for_test(),
                job_generation: 1,
                item_generation: 1,
                location_generation: 1,
                source_generation: 1,
                items: over_limit,
                lock_owner_query: None,
            }),
            Err(ExtensionJobRuntimeErrorV1::InvalidRequest)
        ));
    }

    #[test]
    fn batch_column_output_must_match_host_input_order_exactly_once() {
        let runtime = ExtensionJobRuntimeV1::new(config());
        let observed = Arc::new(Mutex::new(SinkSubmitStatusV1::ACCEPTED));
        let provider =
            BatchColumnProviderObjectV1::new(ReorderedBatchProviderV1(Arc::clone(&observed)));
        let source = || HostInputStreamSourceV1::from_host_snapshot(vec![1], 1, true).unwrap();
        let mut ticket = runtime
            .prepare_batch_column_dispatch(BatchColumnRuntimeRequestV1 {
                authority: ExtensionJobAuthorityV1::for_test("batch-order")
                    .with_filesystem_read_for_test(),
                job_generation: 1,
                item_generation: 1,
                location_generation: 1,
                source_generation: 1,
                items: vec![
                    HostBatchColumnItemV1 {
                        file_name: RString::from("one.rs"),
                        source: source(),
                        cache_identity: RString::new(),
                        modified_unix_seconds: ROption::RNone,
                        modified_subsec_nanos: 0,
                        source_size: ROption::RNone,
                        lock_owner_resource: None,
                    },
                    HostBatchColumnItemV1 {
                        file_name: RString::from("two.rs"),
                        source: source(),
                        cache_identity: RString::new(),
                        modified_unix_seconds: ROption::RNone,
                        modified_subsec_nanos: 0,
                        source_size: ROption::RNone,
                        lock_owner_resource: None,
                    },
                ],
                lock_owner_query: None,
            })
            .unwrap();
        assert_eq!(ticket.invoke_once(&provider), Ok(JobTerminalV1::COMPLETED));
        assert_eq!(*observed.lock().unwrap(), SinkSubmitStatusV1::INVALID);
        assert!(runtime.drain(ticket.job(), 1, 1, 1, 1).is_empty());
        ticket.fail_marker_clear();
    }

    #[test]
    fn production_dispatch_ticket_delivers_only_authorized_input_stream() {
        let runtime = ExtensionJobRuntimeV1::new(config());
        let source = HostInputStreamSourceV1::from_host_snapshot(vec![1], 1, true).unwrap();
        let observed = Arc::new(Mutex::new(None));
        let provider = explorer_extension_api::JobProviderObjectV1::new(PreparedStreamProviderV1(
            Arc::clone(&observed),
        ));
        let mut ticket = runtime
            .prepare_provider_dispatch(stream_request("stream-ticket", source))
            .unwrap();
        assert_eq!(
            ticket.invoke_once(&provider).unwrap(),
            JobTerminalV1::COMPLETED
        );
        assert_eq!(*observed.lock().unwrap(), Some(InputStreamStatusV1::OK));
        ticket.fail_marker_clear();
    }

    #[test]
    fn stale_dispatch_ticket_cannot_publish_completed_after_source_generation_advances() {
        let runtime = ExtensionJobRuntimeV1::new(config());
        let source = HostInputStreamSourceV1::from_host_snapshot(vec![1], 1, true).unwrap();
        let provider = explorer_extension_api::JobProviderObjectV1::new(PreparedStreamProviderV1(
            Arc::new(Mutex::new(None)),
        ));
        let mut ticket = runtime
            .prepare_provider_dispatch(stream_request("stream-ticket-stale", source.clone()))
            .unwrap();
        let job = ticket.control().job();
        assert_eq!(
            ticket.invoke_once(&provider).unwrap(),
            JobTerminalV1::COMPLETED
        );
        assert!(source.replace_host_snapshot(vec![2], 2, true));
        runtime.update_current_generations(job, 1, 1, 2).unwrap();
        assert_eq!(
            ticket
                .publish_terminal_after_marker_clear(JobTerminalV1::COMPLETED)
                .unwrap(),
            ExtensionJobFinishOutcomeV1::Published(JobTerminalV1::CANCELLED)
        );
    }

    #[test]
    fn sink_is_scoped_to_one_provider_call_and_rejects_wrong_thread() {
        let runtime = ExtensionJobRuntimeV1::new(config());
        let context = runtime.open_job(request("one")).unwrap();
        let scope = ProviderCallScopeV1::enter(&runtime.state, &context).unwrap();
        let accepted = context.sink.try_submit(batch(&context, 0));
        assert_eq!(accepted.status, SinkSubmitStatusV1::ACCEPTED);

        let foreign_context = context.clone();
        let wrong_thread =
            thread::spawn(move || foreign_context.sink.try_submit(batch(&foreign_context, 1)))
                .join()
                .unwrap();
        assert_eq!(wrong_thread.status, SinkSubmitStatusV1::WRONG_THREAD);
        assert!(wrong_thread.rejected_batch.into_option().is_some());

        drop(scope);
        let retained = context.sink.try_submit(batch(&context, 1));
        assert_eq!(retained.status, SinkSubmitStatusV1::CLOSED);
        assert!(retained.rejected_batch.into_option().is_some());
    }

    #[test]
    fn accepted_submission_signals_ui_while_the_batch_remains_runtime_owned() {
        let runtime = Arc::new(ExtensionJobRuntimeV1::new(config()));
        let (ingress, mut inbox) = ExtensionJobUiIngressV1::new_pair(Arc::clone(&runtime));
        runtime.install_ready_signal_sink(ingress.runtime_ready_sink());
        let context = runtime.open_job(request("one")).unwrap();
        let scope = ProviderCallScopeV1::enter(&runtime.state, &context).unwrap();
        assert_eq!(
            context.sink.try_submit(batch(&context, 0)).status,
            SinkSubmitStatusV1::ACCEPTED
        );
        drop(scope);
        let ready = inbox.take_ready(1);
        assert_eq!(ready.signals.len(), 1);
        assert!(runtime.is_result_generation_current(context.job, 1, 1, 1));
        assert_eq!(runtime.drain(context.job, 1, 1, 1, 1).len(), 1);
    }

    #[test]
    fn stale_generations_are_discarded_and_release_package_credits() {
        let runtime = ExtensionJobRuntimeV1::new(config());
        let first = runtime.open_job(request("one")).unwrap();
        let second = runtime.open_job(request("one")).unwrap();
        let first_scope = ProviderCallScopeV1::enter(&runtime.state, &first).unwrap();
        assert_eq!(
            first.sink.try_submit(batch(&first, 0)).status,
            SinkSubmitStatusV1::ACCEPTED
        );
        drop(first_scope);

        let second_scope = ProviderCallScopeV1::enter(&runtime.state, &second).unwrap();
        let package_limited = second.sink.try_submit(batch(&second, 0));
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

        let second_scope = ProviderCallScopeV1::enter(&runtime.state, &second).unwrap();
        assert_eq!(
            second.sink.try_submit(batch(&second, 0)).status,
            SinkSubmitStatusV1::ACCEPTED
        );
        drop(second_scope);
    }

    #[test]
    fn drain_releases_job_credits_before_resubmit_and_purge() {
        let runtime = ExtensionJobRuntimeV1::new(config());
        let context = runtime.open_job(request("one")).unwrap();
        let scope = ProviderCallScopeV1::enter(&runtime.state, &context).unwrap();
        assert_eq!(
            context.sink.try_submit(batch(&context, 0)).status,
            SinkSubmitStatusV1::ACCEPTED
        );
        drop(scope);
        assert_eq!(runtime.drain(context.job, 1, 1, 1, 1).len(), 1);

        let scope = ProviderCallScopeV1::enter(&runtime.state, &context).unwrap();
        assert_eq!(
            context.sink.try_submit(batch(&context, 1)).status,
            SinkSubmitStatusV1::ACCEPTED
        );
        drop(scope);
        runtime.purge(context.job).unwrap();

        let scope = ProviderCallScopeV1::enter(&runtime.state, &context).unwrap();
        assert_eq!(
            context.sink.try_submit(batch(&context, 2)).status,
            SinkSubmitStatusV1::ACCEPTED
        );
        drop(scope);
        runtime.purge(context.job).unwrap();
        assert!(runtime.state.lock().unwrap().accounting_healthy);
    }

    #[test]
    fn drained_batch_is_revoked_when_a_later_malformed_submission_faults_the_job() {
        let runtime = ExtensionJobRuntimeV1::new(config());
        let context = runtime.open_job(request("one")).unwrap();
        let scope = ProviderCallScopeV1::enter(&runtime.state, &context).unwrap();
        assert_eq!(
            context.sink.try_submit(batch(&context, 0)).status,
            SinkSubmitStatusV1::ACCEPTED
        );
        drop(scope);
        let drained = runtime.drain(context.job, 1, 1, 1, 1);
        assert_eq!(drained.len(), 1);
        assert!(runtime.is_accepted_batch_current(&drained[0]));
        let scope = ProviderCallScopeV1::enter(&runtime.state, &context).unwrap();
        let mut malformed = batch(&context, 1);
        if let ROption::RSome(value) = &mut malformed.entries[0].result.value {
            value.float = -0.0;
        }
        assert_eq!(
            context.sink.try_submit(malformed).status,
            SinkSubmitStatusV1::INVALID
        );
        drop(scope);
        assert!(!runtime.is_accepted_batch_current(&drained[0]));
        assert!(
            runtime
                .apply_accepted_batch(&drained[0], |_| ("stale".to_owned(), 1))
                .is_none()
        );
        let quarantine = runtime.take_quarantine();
        assert_eq!(quarantine.len(), 1);
        assert_eq!(quarantine[0].job, context.job);
        assert_eq!(quarantine[0].job_generation, context.job_generation);
        assert_eq!(quarantine[0].item_generation, context.item_generation);
        assert_eq!(
            quarantine[0].location_generation,
            context.location_generation
        );
        assert_eq!(quarantine[0].source_generation, context.source_generation);
    }

    #[test]
    fn producer_fault_revokes_siblings_and_their_predrained_batches() {
        let runtime = ExtensionJobRuntimeV1::new(config());
        let first = runtime.open_job(request("same")).unwrap();
        let second = runtime.open_job(request("same")).unwrap();
        let scope = ProviderCallScopeV1::enter(&runtime.state, &second).unwrap();
        assert_eq!(
            second.sink.try_submit(batch(&second, 0)).status,
            SinkSubmitStatusV1::ACCEPTED
        );
        drop(scope);
        let drained = runtime.drain(second.job, 1, 1, 1, 1);
        assert_eq!(drained.len(), 1);
        assert!(runtime.is_accepted_batch_current(&drained[0]));

        let scope = ProviderCallScopeV1::enter(&runtime.state, &first).unwrap();
        let mut malformed = batch(&first, 0);
        if let ROption::RSome(value) = &mut malformed.entries[0].result.value {
            value.float = -0.0;
        }
        assert_eq!(
            first.sink.try_submit(malformed).status,
            SinkSubmitStatusV1::INVALID
        );
        drop(scope);

        assert!(!runtime.is_accepted_batch_current(&drained[0]));
        assert_eq!(
            runtime.finish(second.job, JobTerminalV1::COMPLETED),
            ExtensionJobFinishOutcomeV1::AlreadyTerminal(JobTerminalV1::INCOMPATIBLE)
        );
        assert!(runtime.drain(second.job, 1, 1, 1, 1).is_empty());
        assert!(matches!(
            runtime.open_job(request("same")),
            Err(ExtensionJobRuntimeErrorV1::UnauthorizedAuthority)
        ));
    }

    #[test]
    fn completed_job_keeps_accepted_batches_current_until_the_host_applies_them() {
        let runtime = ExtensionJobRuntimeV1::new(config());
        let context = runtime.open_job(request("complete")).unwrap();
        let scope = ProviderCallScopeV1::enter(&runtime.state, &context).unwrap();
        assert_eq!(
            context.sink.try_submit(batch(&context, 0)).status,
            SinkSubmitStatusV1::ACCEPTED
        );
        drop(scope);
        assert_eq!(
            runtime.finish(context.job, JobTerminalV1::COMPLETED),
            ExtensionJobFinishOutcomeV1::Published(JobTerminalV1::COMPLETED)
        );
        let drained = runtime.drain(context.job, 1, 1, 1, 1);
        assert_eq!(drained.len(), 1);
        assert!(runtime.is_accepted_batch_current(&drained[0]));
        assert_eq!(
            runtime
                .apply_accepted_batch(&drained[0], |_| ("item".to_owned(), 1))
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn progress_is_latest_wins_and_invalid_or_out_of_order_updates_do_not_advance_it() {
        let runtime = ExtensionJobRuntimeV1::new(config());
        let context = runtime.open_job(request("one")).unwrap();
        let scope = ProviderCallScopeV1::enter(&runtime.state, &context).unwrap();
        assert_eq!(
            context.progress.try_submit(progress(&context, 0, 1, 4)),
            JobProgressStatusV1::ACCEPTED
        );
        assert_eq!(
            context.progress.try_submit(progress(&context, 1, 3, 4)),
            JobProgressStatusV1::ACCEPTED
        );
        assert_eq!(
            context.progress.try_submit(progress(&context, 2, 5, 4)),
            JobProgressStatusV1::INVALID
        );
        assert_eq!(
            context.progress.try_submit(progress(&context, 3, 4, 4)),
            JobProgressStatusV1::CLOSED
        );
        assert_eq!(
            runtime.take_progress(
                context.job,
                context.job_generation,
                context.item_generation,
                context.location_generation,
                context.source_generation,
            ),
            None
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
        let scope = ProviderCallScopeV1::enter(&runtime.state, &context).unwrap();
        let foreign_context = context.clone();
        let wrong_thread = thread::spawn(move || {
            foreign_context
                .progress
                .try_submit(progress(&foreign_context, 0, 1, 1))
        })
        .join()
        .unwrap();
        assert_eq!(wrong_thread, JobProgressStatusV1::WRONG_THREAD);

        runtime.state.lock().unwrap().panic_next_progress_submit = true;
        assert_eq!(
            context.progress.try_submit(progress(&context, 0, 1, 1)),
            JobProgressStatusV1::CLOSED
        );
        assert_eq!(
            context.progress.try_submit(progress(&context, 0, 1, 1)),
            JobProgressStatusV1::ACCEPTED
        );
        drop(scope);
        assert_eq!(
            context.progress.try_submit(progress(&context, 1, 1, 1)),
            JobProgressStatusV1::CLOSED
        );
        let scope = ProviderCallScopeV1::enter(&runtime.state, &context).unwrap();
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
            context.progress.try_submit(progress(&context, 1, 1, 1)),
            JobProgressStatusV1::CLOSED
        );
        drop(scope);
        let _ = runtime.finish(context.job, JobTerminalV1::COMPLETED);
        assert_eq!(
            context.progress.try_submit(progress(&context, 1, 1, 1)),
            JobProgressStatusV1::CLOSED
        );
    }

    #[test]
    fn progress_generation_race_rejects_old_callback_and_never_applies_stale_data() {
        let runtime = ExtensionJobRuntimeV1::new(config());
        let context = runtime.open_job(request("one")).unwrap();
        let scope = ProviderCallScopeV1::enter(&runtime.state, &context).unwrap();
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
            context.progress.try_submit(progress(&context, 0, 1, 1)),
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
        let first_scope = ProviderCallScopeV1::enter(&runtime.state, &first).unwrap();
        assert!(matches!(
            ProviderCallScopeV1::enter(&runtime.state, &second),
            Err(ExtensionJobRuntimeErrorV1::InactiveProviderCall)
        ));
        assert_eq!(first.poll_control(), JobControlStateV1::ACTIVE);
        assert_eq!(second.poll_control(), JobControlStateV1::CLOSED);
        drop(first_scope);
        assert_eq!(first.poll_control(), JobControlStateV1::CLOSED);
        runtime
            .request_control(second.job, JobControlStateV1::CANCELLED)
            .unwrap();
        assert!(matches!(
            ProviderCallScopeV1::enter(&runtime.state, &second),
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
    fn distinct_provider_threads_can_enter_different_jobs_concurrently() {
        let runtime = ExtensionJobRuntimeV1::new(config());
        let first = runtime.open_job(request("one")).unwrap();
        let second = runtime.open_job(request("two")).unwrap();
        let barrier = Arc::new(Barrier::new(3));
        thread::scope(|threads| {
            for context in [&first, &second] {
                let barrier = Arc::clone(&barrier);
                let runtime = &runtime;
                threads.spawn(move || {
                    let scope = ProviderCallScopeV1::enter(&runtime.state, context)
                        .expect("independent provider thread enters");
                    barrier.wait();
                    barrier.wait();
                    drop(scope);
                });
            }
            // Both callback scopes must be live simultaneously. The existing
            // same-thread nesting test above remains the rejection case.
            barrier.wait();
            barrier.wait();
        });
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
        let scope = ProviderCallScopeV1::enter(&runtime.state, &context).unwrap();
        let mut invalid = batch(&context, 0);
        if let ROption::RSome(value) = &mut invalid.entries[0].result.value {
            value.float = -0.0;
        }
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
                let _ = context.poll_control();
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
            assert_eq!(context.poll_control(), JobControlStateV1::CLOSED);
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

    #[cfg(feature = "integration-test-support")]
    #[test]
    fn authority_is_minted_from_a_sealed_contribution_and_live_dispatch_lease() {
        use crate::{
            contribution_gate::integration_test_support::validated_job_fixture,
            native_lifecycle::integration_test_support::live_dispatch_fixture,
        };

        let validated = validated_job_fixture("pkg");
        let lifecycle =
            live_dispatch_fixture(validated.package_id(), validated.sealed_manifest_digest())
                .unwrap();
        let authority =
            ExtensionJobAuthorityV1::mint_sealed(&validated, "column", lifecycle.enter().unwrap())
                .unwrap();
        assert_eq!(authority.producer().package_id(), lifecycle.package_id());
        assert_eq!(
            authority.producer().sealed_manifest_digest(),
            lifecycle.digest()
        );
        assert_eq!(authority.producer().contribution_id(), "column");
        assert_eq!(authority.producer().feature_id(), "feature");
        assert!(authority.producer().interface_id().is_valid());
        assert_ne!(authority.producer().feature_epoch(), 0);

        let wrong = validated_job_fixture("different-package");
        assert!(matches!(
            ExtensionJobAuthorityV1::mint_sealed(&wrong, "column", lifecycle.enter().unwrap(),),
            Err(ExtensionJobRuntimeErrorV1::UnauthorizedAuthority)
        ));
        drop(authority);
        lifecycle.disable().unwrap();
        assert!(lifecycle.enter().is_err());
    }

    struct Completes;

    impl JobProviderImplementationV1 for Completes {
        fn run(&self, _: JobContextV1) -> JobTerminalV1 {
            JobTerminalV1::COMPLETED
        }
    }

    struct EmitsTwo;

    impl JobProviderImplementationV1 for EmitsTwo {
        fn run(&self, context: JobContextV1) -> JobTerminalV1 {
            for sequence in 0..2 {
                assert_eq!(
                    context.sink.try_submit(batch(&context, sequence)).status,
                    SinkSubmitStatusV1::ACCEPTED
                );
            }
            JobTerminalV1::COMPLETED
        }
    }

    #[test]
    fn dispatch_ticket_is_one_shot_and_defers_terminal_until_marker_commit() {
        let runtime = ExtensionJobRuntimeV1::new(
            ExtensionResultBufferConfigV1::try_new(8, 8, 8, 8, 8, 32, 32, 32, 4096, 4096, 4096)
                .unwrap(),
        );
        let mut ticket = runtime
            .prepare_provider_dispatch(request("ticket"))
            .unwrap();
        let control = ticket.control();
        let terminal = ticket
            .invoke_once(&explorer_extension_api::JobProviderObjectV1::new(Completes))
            .unwrap();
        assert_eq!(terminal, JobTerminalV1::COMPLETED);
        assert_eq!(
            ticket.invoke_once(&explorer_extension_api::JobProviderObjectV1::new(Completes)),
            Err(ExtensionJobRuntimeErrorV1::ProviderAlreadyInvoked)
        );
        assert!(
            runtime
                .state
                .lock()
                .unwrap()
                .jobs
                .get(&control.job())
                .is_some_and(|job| job.terminal.is_none())
        );
        assert_eq!(
            ticket
                .publish_terminal_after_marker_clear(terminal)
                .unwrap(),
            ExtensionJobFinishOutcomeV1::Published(JobTerminalV1::COMPLETED)
        );
        control.retire().unwrap();
    }

    #[test]
    fn active_feature_revoke_waits_for_marker_terminal_before_retirement() {
        let runtime = ExtensionJobRuntimeV1::new(config());
        let mut ticket = runtime
            .prepare_provider_dispatch(request_feature("active-cancel", "one", 1))
            .unwrap();
        let control = ticket.control();
        let terminal = ticket
            .invoke_once(&explorer_extension_api::JobProviderObjectV1::new(Completes))
            .unwrap();
        runtime.revoke_feature_generation("active-cancel", "test-digest", "one", 1);
        assert_eq!(terminal, JobTerminalV1::COMPLETED);

        // The callback returned after a feature revoke, but the prepared
        // transaction/control must survive until this marker-clear publication
        // point rather than releasing the feature lease early.
        assert_eq!(
            control.retire(),
            Err(ExtensionJobRuntimeErrorV1::RetireRequiresTerminal)
        );
        assert_eq!(
            ticket
                .publish_terminal_after_marker_clear(terminal)
                .unwrap(),
            ExtensionJobFinishOutcomeV1::AlreadyTerminal(JobTerminalV1::CANCELLED)
        );
        // `PreparedNativeJobV1::retire` delegates to this exact control path;
        // marker publication is what makes that public surface eligible.
        control.retire().unwrap();
        drop(ticket);
        assert!(runtime.state.lock().unwrap().jobs.is_empty());
    }

    #[test]
    fn marker_failure_revokes_predrained_data_without_protocol_quarantine() {
        let runtime = ExtensionJobRuntimeV1::new(
            ExtensionResultBufferConfigV1::try_new(8, 8, 8, 8, 8, 32, 32, 32, 4096, 4096, 4096)
                .unwrap(),
        );
        let mut ticket = runtime
            .prepare_provider_dispatch(request("marker"))
            .unwrap();
        let control = ticket.control();
        let terminal = ticket
            .invoke_once(&explorer_extension_api::JobProviderObjectV1::new(EmitsTwo))
            .unwrap();
        assert_eq!(terminal, JobTerminalV1::COMPLETED);
        let drained = runtime.drain(control.job(), 1, 1, 1, 1);
        assert_eq!(drained.len(), 1);
        assert!(runtime.is_accepted_batch_current(&drained[0]));
        let retained_rows = runtime
            .apply_accepted_batch(&drained[0], |_| ("applied".to_owned(), 7))
            .unwrap();
        assert_eq!(retained_rows.len(), 1);
        assert_eq!(runtime.applied_rows_snapshot(&drained[0]).unwrap().len(), 1);

        ticket.fail_marker_clear();
        assert!(!runtime.is_accepted_batch_current(&drained[0]));
        assert!(runtime.applied_rows_snapshot(&drained[0]).is_none());
        // The UI may have retained a clone before the marker failure.  Its
        // host-only shared tombstone still invalidates it after the runtime
        // job is auto-retired and no registry record remains.
        assert_eq!(
            retained_rows[0].outcome(),
            PluginItemOutcomeV1::INCOMPATIBLE
        );
        assert!(retained_rows[0].value().is_none());
        assert!(matches!(
            control.retire(),
            Err(ExtensionJobRuntimeErrorV1::UnknownJob)
        ));
        assert!(runtime.take_quarantine().is_empty());
        let state = runtime.state.lock().unwrap();
        assert!(state.jobs.is_empty());
        assert_eq!(state.queued_batches, 0);
        assert_eq!(state.queued_items, 0);
        assert_eq!(state.queued_bytes, 0);
    }

    #[test]
    fn accepted_batch_is_applied_exactly_once_and_retention_is_hard_bounded() {
        let runtime = ExtensionJobRuntimeV1::new(
            ExtensionResultBufferConfigV1::try_new(2, 2, 2, 2, 2, 1, 1, 1, 1024, 1024, 1024)
                .unwrap(),
        );
        let context = runtime.open_job(request("apply-once")).unwrap();
        let scope = ProviderCallScopeV1::enter(&runtime.state, &context).unwrap();
        assert_eq!(
            context.sink.try_submit(batch(&context, 0)).status,
            SinkSubmitStatusV1::ACCEPTED
        );
        drop(scope);
        let drained = runtime.drain(context.job, 1, 1, 1, 1);
        assert_eq!(drained.len(), 1);
        assert!(
            runtime
                .apply_accepted_batch(&drained[0], |_| ("one".to_owned(), 1))
                .is_some()
        );
        assert!(
            runtime
                .apply_accepted_batch(&drained[0], |_| ("duplicate".to_owned(), 2))
                .is_none()
        );
        let rows = runtime.applied_rows_snapshot(&drained[0]).unwrap();
        assert_eq!(rows.len(), 1);
    }

    #[test]
    fn generation_advance_revokes_previously_applied_rows_and_reclaims_retention() {
        let runtime = ExtensionJobRuntimeV1::new(
            ExtensionResultBufferConfigV1::try_new(2, 2, 2, 2, 2, 8, 8, 8, 4096, 4096, 4096)
                .unwrap(),
        );
        let context = runtime.open_job(request("generation-advance")).unwrap();
        let scope = ProviderCallScopeV1::enter(&runtime.state, &context).unwrap();
        assert_eq!(
            context.sink.try_submit(batch(&context, 0)).status,
            SinkSubmitStatusV1::ACCEPTED
        );
        drop(scope);
        let drained = runtime.drain(context.job, 1, 1, 1, 1);
        let rows = runtime
            .apply_accepted_batch(&drained[0], |_| ("old".to_owned(), 1))
            .expect("applied old row");
        assert_eq!(rows.len(), 1);
        runtime
            .update_current_generations(context.job, 2, 2, 2)
            .expect("advance generation");
        assert_eq!(rows[0].outcome(), PluginItemOutcomeV1::INCOMPATIBLE);
        assert!(rows[0].value().is_none());
        assert!(runtime.applied_rows_snapshot(&drained[0]).is_none());
        let state = runtime.state.lock().unwrap();
        assert_eq!(state.applied_row_count, 0);
        assert!(state.applied_rows.is_empty());
        assert!(state.applied_batches.is_empty());
    }

    #[test]
    fn rapid_one_thousand_batches_coalesce_without_per_item_redraw() {
        let runtime = ExtensionJobRuntimeV1::new(
            ExtensionResultBufferConfigV1::try_new(
                1,
                1,
                1_000,
                1_000,
                1_000,
                1_000,
                1_000,
                1_000,
                1_024 * 1_024,
                1_024 * 1_024,
                1_024 * 1_024,
            )
            .unwrap(),
        );
        let context = runtime.open_job(request("coalesced-package")).unwrap();
        let scope = ProviderCallScopeV1::enter(&runtime.state, &context).unwrap();
        for sequence in 0..1_000 {
            assert_eq!(
                context.sink.try_submit(batch(&context, sequence)).status,
                SinkSubmitStatusV1::ACCEPTED
            );
        }
        drop(scope);

        let drained = runtime.drain(context.job, 1, 1, 1, 1_000);
        assert_eq!(drained.len(), 1_000);
        let mut invalidations = UiInvalidationBatcherV1::new(
            UiInvalidationBatcherConfigV1::try_new(Duration::from_millis(20), 8)
                .expect("20 ms contract window"),
        );
        for batch in &drained {
            assert!(
                runtime
                    .apply_accepted_batch_and_enqueue_invalidation(
                        batch,
                        |_| ("row".to_owned(), 1),
                        &mut invalidations,
                    )
                    .is_some()
            );
        }

        let deadline = invalidations
            .next_deadline()
            .expect("one scheduled UI batch");
        let mut emitted_ui_transactions = 0_usize;
        let emitted = invalidations
            .drain_due(deadline)
            .inspect(|_| {
                emitted_ui_transactions += 1;
            })
            .expect("a single deadline drains all rapid arrivals");
        assert_eq!(emitted.accepted_batches(), 1_000);
        assert_eq!(emitted.accepted_items(), 1_000);
        assert!(emitted.scopes().len() <= 8);
        // A 20 ms coalescing window makes one transaction for this rapid burst;
        // the external 1,000-item gate permits at most 50 transactions.
        assert!(emitted_ui_transactions <= 50);
        assert!(invalidations.drain_due(deadline).is_none());
    }

    #[test]
    fn feature_scoped_lifecycle_cancel_preserves_sibling_and_newer_epoch() {
        let runtime = ExtensionJobRuntimeV1::new(
            ExtensionResultBufferConfigV1::try_new(8, 8, 8, 8, 8, 32, 32, 32, 4096, 4096, 4096)
                .unwrap(),
        );
        let old = runtime.open_job(request_feature("pkg", "one", 1)).unwrap();
        let sibling = runtime.open_job(request_feature("pkg", "two", 1)).unwrap();
        let newer = runtime.open_job(request_feature("pkg", "one", 2)).unwrap();
        for context in [&old, &sibling, &newer] {
            let scope = ProviderCallScopeV1::enter(&runtime.state, context).unwrap();
            assert_eq!(
                context.sink.try_submit(batch(context, 0)).status,
                SinkSubmitStatusV1::ACCEPTED
            );
            drop(scope);
        }
        let old_drained = runtime.drain(old.job, 1, 1, 1, 1);
        let sibling_drained = runtime.drain(sibling.job, 1, 1, 1, 1);
        let newer_drained = runtime.drain(newer.job, 1, 1, 1, 1);
        assert!(
            runtime
                .apply_accepted_batch(&old_drained[0], |_| ("old".to_owned(), 1))
                .is_some()
        );
        assert!(
            runtime
                .apply_accepted_batch(&sibling_drained[0], |_| ("sibling".to_owned(), 2))
                .is_some()
        );
        assert!(
            runtime
                .apply_accepted_batch(&newer_drained[0], |_| ("new".to_owned(), 3))
                .is_some()
        );

        runtime.revoke_feature_generation("pkg", "test-digest", "one", 1);

        assert!(runtime.applied_rows_snapshot(&old_drained[0]).is_none());
        assert_eq!(
            runtime
                .applied_rows_snapshot(&sibling_drained[0])
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            runtime
                .applied_rows_snapshot(&newer_drained[0])
                .unwrap()
                .len(),
            1
        );
        assert!(runtime.take_quarantine().is_empty());
    }

    #[test]
    fn lifecycle_revoke_invalidates_opaque_rows_after_their_job_has_retired() {
        let schema = StableIdV1::new(IdNamespaceV1::new(1, 2), 9);
        let authority = ExtensionJobAuthorityV1::for_test_opaque(
            "pkg",
            "source",
            ContributionKindV1::Column,
            schema,
            3,
            Some("renderer"),
        );
        let runtime = ExtensionJobRuntimeV1::new(config());
        let context = runtime
            .open_job(ExtensionJobRuntimeRequestV1 {
                authority,
                job_generation: 1,
                item_generation: 1,
                location_generation: 1,
                source_generation: 1,
                has_item: true,
                input_stream: None,
            })
            .unwrap();
        let mut opaque = batch(&context, 0);
        opaque.entries[0].result = explorer_extension_api::PluginItemResultV1::value(
            PluginValueV1::opaque(schema, 3, vec![7]).unwrap(),
            ROption::RNone,
        );
        let scope = ProviderCallScopeV1::enter(&runtime.state, &context).unwrap();
        assert_eq!(
            context.sink.try_submit(opaque).status,
            SinkSubmitStatusV1::ACCEPTED
        );
        drop(scope);
        let drained = runtime.drain(context.job, 1, 1, 1, 1);
        let rows = runtime
            .apply_accepted_batch(&drained[0], |_| ("opaque".to_owned(), 1))
            .unwrap();
        let renderer = ExtensionJobAuthorityV1::for_test_opaque(
            "pkg",
            "renderer",
            ContributionKindV1::GpuiRenderer,
            schema,
            3,
            None,
        );
        let binding = crate::OpaquePayloadBindingV1::bind(&rows[0], renderer).unwrap();
        assert_eq!(binding.route().unwrap().bytes, vec![7]);
        let _ = runtime.finish(context.job, JobTerminalV1::COMPLETED);
        runtime.retire(context.job).unwrap();
        runtime.revoke_feature_generation("pkg", "test-digest", "test-feature", 1);
        assert_eq!(
            binding.route(),
            Err(crate::OpaquePayloadRouteErrorV1::BindingDenied)
        );
    }

    #[test]
    fn lifecycle_local_runtime_revocation_does_not_cross_runtime_boundaries() {
        let first = ExtensionJobRuntimeV1::new(config());
        let second = ExtensionJobRuntimeV1::new(config());
        let first_context = first.open_job(request_feature("pkg", "one", 1)).unwrap();
        let second_context = second.open_job(request_feature("pkg", "one", 1)).unwrap();
        let second_scope = ProviderCallScopeV1::enter(&second.state, &second_context).unwrap();
        first.revoke_feature_generation("pkg", "test-digest", "one", 1);
        assert_eq!(first_context.poll_control(), JobControlStateV1::CLOSED);
        assert_eq!(second_context.poll_control(), JobControlStateV1::ACTIVE);
        drop(second_scope);
    }

    #[test]
    fn dropped_ticket_leaves_active_scope_for_explicit_final_retirement() {
        let runtime = ExtensionJobRuntimeV1::new(config());
        let ticket = runtime.prepare_provider_dispatch(request("drop")).unwrap();
        let control = ticket.control();
        let scope = ProviderCallScopeV1::enter(&runtime.state, &ticket.context).unwrap();
        drop(ticket);
        assert!(
            runtime
                .state
                .lock()
                .unwrap()
                .jobs
                .contains_key(&control.job())
        );
        drop(scope);
        assert!(
            runtime
                .state
                .lock()
                .unwrap()
                .jobs
                .contains_key(&control.job())
        );
        control.retire().unwrap();
        assert_eq!(
            control.request_control(JobControlStateV1::CANCELLED),
            Err(ExtensionJobRuntimeErrorV1::UnknownJob)
        );
    }

    #[test]
    fn prepared_ticket_claims_its_worker_thread_at_first_provider_entry() {
        let runtime = Arc::new(ExtensionJobRuntimeV1::new(config()));
        let ticket = runtime
            .prepare_provider_dispatch(request("worker"))
            .unwrap();
        let worker = thread::spawn(move || {
            let mut ticket = ticket;
            let terminal = ticket
                .invoke_once(&explorer_extension_api::JobProviderObjectV1::new(Completes))
                .unwrap();
            assert_eq!(terminal, JobTerminalV1::COMPLETED);
            ticket
                .publish_terminal_after_marker_clear(terminal)
                .unwrap();
        });
        worker.join().unwrap();
        assert!(runtime.state.lock().unwrap().jobs.is_empty());
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

        let mut second = runtime.prepare_provider_dispatch(request("two")).unwrap();
        let terminal = second
            .invoke_once(&explorer_extension_api::JobProviderObjectV1::new(Completes))
            .unwrap();
        assert_eq!(
            second
                .publish_terminal_after_marker_clear(terminal)
                .unwrap(),
            ExtensionJobFinishOutcomeV1::Published(JobTerminalV1::COMPLETED)
        );
    }

    #[test]
    fn thousand_item_scheduler_runtime_ui_pipeline() {
        const ITEMS: usize = 1_000;
        const VISIBLE_ITEMS: usize = 32;
        const DRAIN_BATCH_LIMIT: usize = 16;
        const MAX_REDRAW_NOTIFICATIONS: usize = 50;
        const MAX_CANCELLATION_DELIVERY_TURNS: usize = 1;

        let now = Instant::now();
        let queue_limits = ExtensionQueueLimitsV1::try_new(ITEMS, ITEMS, 1, 1).unwrap();
        let scheduler_config =
            ExtensionSchedulerConfigV1::try_new(queue_limits, queue_limits, VISIBLE_ITEMS).unwrap();
        let mut scheduler = ExtensionJobSchedulerV1::new(scheduler_config);
        let scheduler_scope = ExtensionJobScopeV1::new(
            ExtensionPackageIdV1::from_validated("thousand-item-fixture"),
            "column",
            1,
        );

        // Deliberately enqueue prefetch first: the scheduler must still start
        // the first visible viewport before any background work.
        for ordinal in VISIBLE_ITEMS..ITEMS {
            assert!(matches!(
                scheduler.submit(
                    ExtensionJobRequestV1 {
                        scope: scheduler_scope.clone(),
                        class: ExtensionJobClassV1::Cpu,
                        priority: JobPriority::Prefetch,
                        deadline: RequestDeadline::none(),
                        cancellation: CancellationToken::new(),
                        payload: ordinal,
                    },
                    now,
                ),
                ExtensionScheduleOutcomeV1::Queued { .. }
            ));
        }
        for ordinal in 0..VISIBLE_ITEMS {
            assert!(matches!(
                scheduler.submit(
                    ExtensionJobRequestV1 {
                        scope: scheduler_scope.clone(),
                        class: ExtensionJobClassV1::Cpu,
                        priority: JobPriority::VisibleViewport,
                        deadline: RequestDeadline::none(),
                        cancellation: CancellationToken::new(),
                        payload: ordinal,
                    },
                    now,
                ),
                ExtensionScheduleOutcomeV1::Queued { .. }
            ));
        }
        assert_eq!(scheduler.stats(ExtensionJobClassV1::Cpu).queued_jobs, ITEMS);

        // This is the concrete host baseline a real list model presents
        // before extension work is admitted. Stable identities deliberately
        // exist independently of any plugin result.
        let basic_list_snapshot = (0..ITEMS)
            .map(|ordinal| u128::try_from(ordinal).expect("fixture ordinal"))
            .collect::<Vec<_>>();
        let mut event_trace = vec!["baseline_ready"];
        assert_eq!(basic_list_snapshot.len(), ITEMS);
        assert_eq!(basic_list_snapshot.first(), Some(&0));
        assert_eq!(
            basic_list_snapshot.last(),
            Some(&u128::try_from(ITEMS - 1).unwrap())
        );
        assert_eq!(scheduler.stats(ExtensionJobClassV1::Cpu).queued_jobs, ITEMS);

        // Keep live-job capacity intentionally small. The fixture must retire
        // its terminal, drained provider after presentation; it cannot hide
        // lifecycle leaks behind a 1,000-job registry.
        let buffer_config = ExtensionResultBufferConfigV1::try_new(
            4, 4, 1_024, 1_024, 1_024, 1_024, 1_024, 1_024, 65_536, 65_536, 65_536,
        )
        .unwrap();
        let runtime = Arc::new(ExtensionJobRuntimeV1::new(buffer_config));
        let (ingress, inbox) = ExtensionJobUiIngressV1::new_pair(Arc::clone(&runtime));
        runtime.install_ready_signal_sink(ingress.runtime_ready_sink());
        let mut pump = ExtensionJobUiPumpV1::new(
            inbox,
            UiInvalidationBatcherConfigV1::try_new(Duration::from_millis(16), 8).unwrap(),
        );

        let mut extension_completions = 0_usize;
        assert_eq!(extension_completions, 0);

        let mut start_order = Vec::with_capacity(ITEMS);
        let mut observed_runtime_jobs = 0_usize;
        let mut observed_queued_batches = 0_usize;
        let mut observed_queued_items = 0_usize;
        let mut observed_queued_bytes = 0_usize;
        let mut accepted_results = 0_usize;
        let mut ready_deliveries = 0_usize;
        let mut drain_turn_sizes = Vec::new();
        // A single provider owns the incremental stream, but every scheduler
        // admission causally contributes exactly its started payload while a
        // real provider-call scope is active.
        let context = runtime.open_job(request("thousand-item-fixture")).unwrap();
        while let Some(started) = {
            let poll = scheduler.try_start(ExtensionJobClassV1::Cpu, now);
            let _ = poll.actions.signal_all();
            poll.started
        } {
            event_trace.push("scheduler_start");
            start_order.push(started.payload);
            let scope = ProviderCallScopeV1::enter(&runtime.state, &context).unwrap();
            assert_eq!(
                context
                    .sink
                    .try_submit(batch(&context, u64::try_from(started.payload).unwrap()))
                    .status,
                SinkSubmitStatusV1::ACCEPTED
            );
            drop(scope);
            assert_eq!(
                scheduler.complete(started.job_id, now).outcome,
                ExtensionCompletionOutcomeV1::Completed
            );
            extension_completions += 1;
        }

        {
            let state = runtime.state.lock().unwrap();
            observed_runtime_jobs = observed_runtime_jobs.max(state.jobs.len());
            observed_queued_batches = observed_queued_batches.max(state.queued_batches);
            observed_queued_items = observed_queued_items.max(state.queued_items);
            observed_queued_bytes = observed_queued_bytes.max(state.queued_bytes);
        }
        assert!(matches!(
            runtime.finish(context.job, JobTerminalV1::COMPLETED),
            ExtensionJobFinishOutcomeV1::Published(JobTerminalV1::COMPLETED)
        ));

        // Each UI turn consumes one ready signal and a bounded number of
        // batches. The runtime re-arms the same job until all 1,000 are
        // committed, proving that deduplication does not strand tail work.
        loop {
            let ready = pump.take_ready(1).unwrap();
            if ready.signals.is_empty() {
                break;
            }
            ready_deliveries += ready.signals.len();
            for signal in ready.signals {
                let (item, location, source) = signal.generations();
                let accepted =
                    runtime.drain(signal.job(), item, location, source, DRAIN_BATCH_LIMIT);
                drain_turn_sizes.push(accepted.len());
                for accepted_batch in accepted {
                    accepted_results += accepted_batch.entry_count();
                    assert!(
                        runtime
                            .apply_accepted_batch(&accepted_batch, |_| {
                                ("fixture-item".to_owned(), 1)
                            })
                            .is_some()
                    );
                    event_trace.push("extension_apply");
                    ingress.notify_applied_at(&accepted_batch, now);
                }
            }
            let applied_notices = pump.poll_applied(DRAIN_BATCH_LIMIT).unwrap();
            assert!(applied_notices <= DRAIN_BATCH_LIMIT);
        }

        assert_eq!(extension_completions, ITEMS);
        assert_eq!(accepted_results, ITEMS);
        assert_eq!(start_order.len(), ITEMS);
        assert_eq!(
            start_order[..VISIBLE_ITEMS],
            (0..VISIBLE_ITEMS).collect::<Vec<_>>()
        );
        assert_eq!(start_order[VISIBLE_ITEMS], VISIBLE_ITEMS);
        assert_eq!(scheduler.stats(ExtensionJobClassV1::Cpu).queued_jobs, 0);
        assert_eq!(scheduler.stats(ExtensionJobClassV1::Cpu).running_jobs, 0);
        assert_eq!(ready_deliveries, ITEMS.div_ceil(DRAIN_BATCH_LIMIT));
        assert_eq!(drain_turn_sizes.iter().sum::<usize>(), ITEMS);
        assert!(
            drain_turn_sizes
                .iter()
                .all(|turn_size| (1..=DRAIN_BATCH_LIMIT).contains(turn_size))
        );
        assert!(observed_runtime_jobs <= buffer_config.max_active_jobs);
        assert!(observed_queued_batches <= buffer_config.max_batches);
        assert!(observed_queued_items <= buffer_config.max_items);
        assert!(observed_queued_bytes <= buffer_config.max_bytes);
        assert_eq!(event_trace.first(), Some(&"baseline_ready"));
        assert_eq!(event_trace.get(1), Some(&"scheduler_start"));
        assert!(
            event_trace
                .iter()
                .position(|event| *event == "extension_apply")
                .is_some()
        );

        let redraw_deadline = pump.next_deadline().unwrap().unwrap();
        assert_eq!(redraw_deadline, now + Duration::from_millis(16));
        let mut redraw_notifications = 0_usize;
        let mut represented_items = 0_usize;
        while let Some(deadline) = pump.next_deadline().unwrap() {
            let redraw = pump.drain_due(deadline).unwrap().unwrap();
            redraw_notifications += 1;
            represented_items += redraw.accepted_items();
            assert_eq!(redraw.scopes().len(), 1);
            event_trace.push("redraw");
        }
        assert_eq!(represented_items, ITEMS);
        assert!((1..=MAX_REDRAW_NOTIFICATIONS).contains(&redraw_notifications));
        runtime.retire(context.job).unwrap();
        let state = runtime.state.lock().unwrap();
        assert!(state.jobs.is_empty());
        assert_eq!(state.queued_batches, 0);
        assert_eq!(state.queued_items, 0);
        assert_eq!(state.queued_bytes, 0);
        drop(state);

        // Re-admission after cleanup proves cancellation uses the scheduler's
        // real token callback rather than a test-only control path. Delivery
        // is bounded by one post-unlock signal turn, without a sleep budget.
        let cancellation = CancellationToken::new();
        let cancelled_job_id = match scheduler.submit(
            ExtensionJobRequestV1 {
                scope: scheduler_scope,
                class: ExtensionJobClassV1::Cpu,
                priority: JobPriority::VisibleViewport,
                deadline: RequestDeadline::none(),
                cancellation: cancellation.clone(),
                payload: ITEMS,
            },
            now,
        ) {
            ExtensionScheduleOutcomeV1::Queued { job_id } => job_id,
            other => panic!("fixture re-admission failed: {other:?}"),
        };
        let started = scheduler
            .try_start(ExtensionJobClassV1::Cpu, now)
            .started
            .unwrap();
        assert_eq!(started.job_id, cancelled_job_id);
        let context = runtime.open_job(request("thousand-item-fixture")).unwrap();
        let delivery_turns = Arc::new(AtomicUsize::new(0));
        let delivery_turns_for_callback = Arc::clone(&delivery_turns);
        let runtime_for_callback = Arc::clone(&runtime);
        let job_for_callback = context.job;
        let _registration = cancellation.register(move || {
            delivery_turns_for_callback.fetch_add(1, Ordering::SeqCst);
            let _ = runtime_for_callback
                .request_control(job_for_callback, JobControlStateV1::CANCELLED);
        });
        let scope = ProviderCallScopeV1::enter(&runtime.state, &context).unwrap();
        assert_eq!(
            context.sink.try_submit(batch(&context, 0)).status,
            SinkSubmitStatusV1::ACCEPTED
        );
        let cancellation_result = scheduler.cancel(cancelled_job_id);
        let delivery = cancellation_result.actions.signal_all();
        assert_eq!(delivery.callbacks_invoked, MAX_CANCELLATION_DELIVERY_TURNS);
        assert_eq!(
            delivery_turns.load(Ordering::SeqCst),
            MAX_CANCELLATION_DELIVERY_TURNS
        );
        assert_eq!(context.poll_control(), JobControlStateV1::CANCELLED);
        drop(scope);
        assert_eq!(
            scheduler.complete(cancelled_job_id, now).outcome,
            ExtensionCompletionOutcomeV1::Cancelled
        );
        assert_eq!(
            runtime.finish(context.job, JobTerminalV1::COMPLETED),
            ExtensionJobFinishOutcomeV1::Published(JobTerminalV1::CANCELLED)
        );
        // The provider managed to enqueue before cancellation, so a UI turn
        // may observe its stale signal. Draining it releases credits but must
        // publish neither values nor invalidations after cancellation.
        let stale_ready = pump.take_ready(1).unwrap();
        assert_eq!(stale_ready.signals.len(), 1);
        for signal in stale_ready.signals {
            let (item, location, source) = signal.generations();
            assert!(
                runtime
                    .drain(signal.job(), item, location, source, DRAIN_BATCH_LIMIT)
                    .is_empty()
            );
        }
        assert_eq!(pump.poll_applied(32).unwrap(), 0);
        assert_eq!(pump.next_deadline().unwrap(), None);
        assert_eq!(pump.drain_due(now).unwrap(), None);
        runtime.retire(context.job).unwrap();
        assert!(pump.take_ready(1).unwrap().signals.is_empty());
    }
}
