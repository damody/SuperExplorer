//! Bounded, host-owned extension result cache.
//!
//! Cache identity is minted from a sealed job producer plus host filesystem
//! facts. Plugin code cannot supply package, feature, interface, or data
//! version strings for a cache lookup. Entries retain only copied host result
//! entries; cache hits are rebound to a fresh host generation before use.

use explorer_extension_api::JobHandleV1;
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use crate::{
    AcceptedIncrementalResultBatchV1, ExtensionJobProducerV1, ExtensionValueRowV1,
    extension_value_router::{ExtensionValueGenerationV1, HostIncrementalResultEntryV1},
};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct InterfaceKeyV1(u32, u64);

impl From<explorer_extension_api::StableIdV1> for InterfaceKeyV1 {
    fn from(value: explorer_extension_api::StableIdV1) -> Self {
        Self(value.namespace.into_raw(), value.value)
    }
}

/// A host-attested file identity and metadata revision. Neither component is a
/// path, avoiding a cache authority that plugins could forge from strings.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ExtensionResultCacheFileFactV1 {
    file_identity: u128,
    metadata_generation: u64,
}

impl ExtensionResultCacheFileFactV1 {
    #[must_use]
    pub const fn from_host(file_identity: u128, metadata_generation: u64) -> Self {
        Self {
            file_identity,
            metadata_generation,
        }
    }
}

/// Current host view generations required before a cached value may publish.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExtensionResultCacheGenerationV1 {
    item: u64,
    location: u64,
    source: u64,
}

impl ExtensionResultCacheGenerationV1 {
    #[must_use]
    pub const fn from_host(
        item_generation: u64,
        location_generation: u64,
        source_generation: u64,
    ) -> Option<Self> {
        if item_generation == 0 || location_generation == 0 || source_generation == 0 {
            return None;
        }
        Some(Self {
            item: item_generation,
            location: location_generation,
            source: source_generation,
        })
    }

    pub(crate) const fn matches(
        self,
        item_generation: u64,
        location_generation: u64,
        source_generation: u64,
    ) -> bool {
        self.item == item_generation
            && self.location == location_generation
            && self.source == source_generation
    }
}

/// Opaque host-minted cache key. It includes every sealed producer factor
/// required to prevent data-version or feature-generation cache confusion.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ExtensionResultCacheKeyV1 {
    package_id: String,
    sealed_manifest_digest: String,
    contribution_id: String,
    data_version: u64,
    interface_id: InterfaceKeyV1,
    feature_id: String,
    feature_epoch: u64,
    file: ExtensionResultCacheFileFactV1,
    option_hash: [u8; 32],
    watcher_scope: u128,
    watcher_generation: u64,
    recursive: bool,
}

impl ExtensionResultCacheKeyV1 {
    /// Mints an identity from a host-issued producer. The caller supplies only
    /// host filesystem/options/watcher facts; plugin data cannot forge this.
    #[must_use]
    pub fn from_host(
        producer: &ExtensionJobProducerV1,
        file: ExtensionResultCacheFileFactV1,
        option_hash: [u8; 32],
        watcher_scope: u128,
        watcher_generation: u64,
        recursive: bool,
    ) -> Option<Self> {
        (producer.feature_epoch() != 0 && watcher_generation != 0).then(|| Self {
            package_id: producer.package_id().to_owned(),
            sealed_manifest_digest: producer.sealed_manifest_digest().to_owned(),
            contribution_id: producer.contribution_id().to_owned(),
            data_version: producer.data_version(),
            interface_id: producer.interface_id().into(),
            feature_id: producer.feature_id().to_owned(),
            feature_epoch: producer.feature_epoch(),
            file,
            option_hash,
            watcher_scope,
            watcher_generation,
            recursive,
        })
    }
}

/// Bounded cache capacity and TTL policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExtensionResultCacheConfigV1 {
    max_entries: usize,
    max_entries_per_package: usize,
    max_entries_per_interface: usize,
    max_bytes: usize,
    max_bytes_per_package: usize,
    max_bytes_per_interface: usize,
    ttl: Duration,
}

impl ExtensionResultCacheConfigV1 {
    #[must_use]
    pub(crate) const fn host_default() -> Self {
        Self {
            max_entries: 1_024,
            max_entries_per_package: 128,
            max_entries_per_interface: 128,
            max_bytes: 32 * 1024 * 1024,
            max_bytes_per_package: 4 * 1024 * 1024,
            max_bytes_per_interface: 4 * 1024 * 1024,
            ttl: Duration::from_secs(30),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        max_entries: usize,
        max_entries_per_package: usize,
        max_entries_per_interface: usize,
        max_bytes: usize,
        max_bytes_per_package: usize,
        max_bytes_per_interface: usize,
        ttl: Duration,
    ) -> Option<Self> {
        (max_entries != 0
            && max_entries_per_package != 0
            && max_entries_per_interface != 0
            && max_bytes != 0
            && max_bytes_per_package != 0
            && max_bytes_per_interface != 0
            && !ttl.is_zero()
            && max_entries_per_package <= max_entries
            && max_entries_per_interface <= max_entries
            && max_bytes_per_package <= max_bytes
            && max_bytes_per_interface <= max_bytes)
            .then_some(Self {
                max_entries,
                max_entries_per_package,
                max_entries_per_interface,
                max_bytes,
                max_bytes_per_package,
                max_bytes_per_interface,
                ttl,
            })
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct UsageV1 {
    entries: usize,
    bytes: usize,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct ScopeV1 {
    package_id: String,
    sealed_manifest_digest: String,
    contribution_id: String,
    data_version: u64,
    interface_id: InterfaceKeyV1,
    feature_id: String,
    feature_epoch: u64,
    watcher_scope: u128,
}

impl From<&ExtensionResultCacheKeyV1> for ScopeV1 {
    fn from(key: &ExtensionResultCacheKeyV1) -> Self {
        Self {
            package_id: key.package_id.clone(),
            sealed_manifest_digest: key.sealed_manifest_digest.clone(),
            contribution_id: key.contribution_id.clone(),
            data_version: key.data_version,
            interface_id: key.interface_id,
            feature_id: key.feature_id.clone(),
            feature_epoch: key.feature_epoch,
            watcher_scope: key.watcher_scope,
        }
    }
}

#[derive(Clone, Debug)]
struct CacheEntryV1 {
    scope_epoch: u64,
    global_epoch: u64,
    inserted_at: Instant,
    bytes: usize,
    producer: ExtensionJobProducerV1,
    cache_generation: ExtensionValueGenerationV1,
    entries: Vec<HostIncrementalResultEntryV1>,
}

#[derive(Debug)]
struct CacheStateV1 {
    entries: HashMap<ExtensionResultCacheKeyV1, CacheEntryV1>,
    scope_epochs: HashMap<ScopeV1, u64>,
    watcher_generations: HashMap<u128, u64>,
    next_watcher_generation: u64,
    global_epoch: u64,
    exhausted: bool,
    total: UsageV1,
    packages: HashMap<String, UsageV1>,
    interfaces: HashMap<InterfaceKeyV1, UsageV1>,
}

/// Cache write permission captured at the miss that scheduled a provider.
/// A later watcher/manual/data-version/feature invalidation makes it inert.
#[derive(Clone, Debug)]
pub struct ExtensionResultCacheAdmissionV1 {
    key: ExtensionResultCacheKeyV1,
    generation: ExtensionResultCacheGenerationV1,
    scope_epoch: u64,
    global_epoch: u64,
    job: Option<(JobHandleV1, u64)>,
}

impl ExtensionResultCacheAdmissionV1 {
    pub(crate) fn bind_job(&mut self, job: JobHandleV1) {
        self.job = Some((job, job.generation()));
    }

    fn matches_batch(&self, batch: &AcceptedIncrementalResultBatchV1) -> bool {
        self.job.is_some_and(|(job, generation)| {
            job == batch.cache_job() && generation == batch.job_generation
        })
    }
}

/// Cache lookup result. A hit is opaque until host code rebinds its copied
/// values to the current view generation.
#[derive(Clone, Debug)]
pub enum ExtensionResultCacheLookupV1 {
    Hit(Box<ExtensionResultCacheHitV1>),
    Miss(Box<ExtensionResultCacheAdmissionV1>),
    RejectedStale,
}

#[derive(Clone, Debug)]
pub struct ExtensionResultCacheHitV1 {
    producer: ExtensionJobProducerV1,
    cache_generation: ExtensionValueGenerationV1,
    entries: Vec<HostIncrementalResultEntryV1>,
}

impl ExtensionResultCacheHitV1 {
    #[must_use]
    pub(crate) fn producer(&self) -> &ExtensionJobProducerV1 {
        &self.producer
    }

    /// Recreates rows with a new host tombstone. Cached UI rows themselves are
    /// never retained, so a former lifecycle token cannot leak into a hit.
    pub(crate) fn rebind_rows(
        &self,
        generation: ExtensionValueGenerationV1,
        mut host_identity: impl FnMut(usize) -> (String, u128),
    ) -> Vec<ExtensionValueRowV1> {
        let generation =
            ExtensionValueGenerationV1::combine([generation, self.cache_generation.clone()]);
        self.entries
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                let (display_name, identity) = host_identity(index);
                entry.rebind_row(display_name, identity, generation.clone())
            })
            .collect()
    }
}

/// Result of a host cache insert attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExtensionResultCacheInsertOutcomeV1 {
    Inserted,
    RejectedStale,
    RejectedCapacity,
}

/// Host-only result cache. It holds no ABI values, paths, callbacks, or UI
/// rows; every public operation is bounded and lock-contained.
#[derive(Debug)]
pub struct ExtensionResultCacheV1 {
    config: ExtensionResultCacheConfigV1,
    state: Mutex<CacheStateV1>,
}

impl ExtensionResultCacheV1 {
    #[must_use]
    pub fn new(config: ExtensionResultCacheConfigV1) -> Self {
        Self {
            config,
            state: Mutex::new(CacheStateV1 {
                entries: HashMap::new(),
                scope_epochs: HashMap::new(),
                watcher_generations: HashMap::new(),
                next_watcher_generation: 1,
                global_epoch: 0,
                exhausted: false,
                total: UsageV1::default(),
                packages: HashMap::new(),
                interfaces: HashMap::new(),
            }),
        }
    }

    #[must_use]
    pub(crate) fn lookup_at(
        &self,
        key: ExtensionResultCacheKeyV1,
        generation: ExtensionResultCacheGenerationV1,
        now: Instant,
    ) -> ExtensionResultCacheLookupV1 {
        let Ok(mut state) = self.state.lock() else {
            return ExtensionResultCacheLookupV1::Miss(Box::new(ExtensionResultCacheAdmissionV1 {
                key,
                generation,
                scope_epoch: u64::MAX,
                global_epoch: u64::MAX,
                job: None,
            }));
        };
        if state.exhausted {
            return ExtensionResultCacheLookupV1::RejectedStale;
        }
        reclaim_expired(&mut state, now, self.config.ttl);
        let scope = ScopeV1::from(&key);
        let Some(watcher_generation) =
            ensure_watcher_generation(&mut state, key.watcher_scope, self.config.max_entries)
        else {
            return ExtensionResultCacheLookupV1::RejectedStale;
        };
        if watcher_generation != key.watcher_generation {
            return ExtensionResultCacheLookupV1::RejectedStale;
        }
        let Some(scope_epoch) =
            ensure_scope_epoch(&mut state, scope.clone(), self.config.max_entries)
        else {
            return ExtensionResultCacheLookupV1::RejectedStale;
        };
        let invalid_entry = state.entries.get(&key).is_some_and(|entry| {
            entry.scope_epoch != scope_epoch
                || entry.global_epoch != state.global_epoch
                || !entry.cache_generation.is_current()
                || now.saturating_duration_since(entry.inserted_at) >= self.config.ttl
        });
        if invalid_entry {
            remove_entry(&mut state, &key);
            advance_scope_epoch(&mut state, scope.clone());
        }
        if let Some(entry) = state.entries.get(&key)
            // View generations deliberately do not participate in payload
            // validity. A cache entry may serve multiple current tabs; every
            // hit is rebound to that caller's generation below.
            && entry.scope_epoch == state.scope_epochs.get(&scope).copied().unwrap_or(0)
            && entry.global_epoch == state.global_epoch
            && entry.cache_generation.is_current()
        {
            return ExtensionResultCacheLookupV1::Hit(Box::new(ExtensionResultCacheHitV1 {
                producer: entry.producer.clone(),
                cache_generation: entry.cache_generation.clone(),
                entries: entry.entries.clone(),
            }));
        }
        ExtensionResultCacheLookupV1::Miss(Box::new(ExtensionResultCacheAdmissionV1 {
            key,
            generation,
            scope_epoch: state.scope_epochs.get(&scope).copied().unwrap_or(0),
            global_epoch: state.global_epoch,
            job: None,
        }))
    }

    #[must_use]
    pub fn lookup(
        &self,
        key: ExtensionResultCacheKeyV1,
        generation: ExtensionResultCacheGenerationV1,
    ) -> ExtensionResultCacheLookupV1 {
        self.lookup_at(key, generation, Instant::now())
    }

    pub(crate) fn insert_batch_at(
        &self,
        admission: ExtensionResultCacheAdmissionV1,
        batch: &AcceptedIncrementalResultBatchV1,
        now: Instant,
    ) -> ExtensionResultCacheInsertOutcomeV1 {
        let (producer, entries, bytes) = batch.cache_payload_parts();
        if !key_matches_producer(&admission.key, &producer) {
            return ExtensionResultCacheInsertOutcomeV1::RejectedStale;
        }
        if !admission.matches_batch(batch) {
            return ExtensionResultCacheInsertOutcomeV1::RejectedStale;
        }
        let (item_generation, location_generation, source_generation) =
            batch.cache_generation_parts();
        if !admission
            .generation
            .matches(item_generation, location_generation, source_generation)
            || !batch.cache_tombstone().is_current()
        {
            return ExtensionResultCacheInsertOutcomeV1::RejectedStale;
        }
        let Ok(mut state) = self.state.lock() else {
            return ExtensionResultCacheInsertOutcomeV1::RejectedStale;
        };
        // Capacity accounting includes only live entries. This bounded sweep
        // prevents a sequence of never-revisited TTL keys from pinning the
        // cache at capacity forever.
        reclaim_expired(&mut state, now, self.config.ttl);
        if state.exhausted {
            return ExtensionResultCacheInsertOutcomeV1::RejectedStale;
        }
        let scope = ScopeV1::from(&admission.key);
        if admission.global_epoch != state.global_epoch
            || admission.scope_epoch != state.scope_epochs.get(&scope).copied().unwrap_or(0)
        {
            return ExtensionResultCacheInsertOutcomeV1::RejectedStale;
        }
        let package_usage = state
            .packages
            .get(&admission.key.package_id)
            .copied()
            .unwrap_or_default();
        let interface_usage = state
            .interfaces
            .get(&admission.key.interface_id)
            .copied()
            .unwrap_or_default();
        let previous = state.entries.get(&admission.key);
        let previous_bytes = previous.map_or(0, |entry| entry.bytes);
        let previous_entries = usize::from(previous.is_some());
        let total_after = replaced_usage(state.total, previous_entries, previous_bytes, bytes);
        let package_after = replaced_usage(package_usage, previous_entries, previous_bytes, bytes);
        let interface_after =
            replaced_usage(interface_usage, previous_entries, previous_bytes, bytes);
        let (Some(total_after), Some(package_after), Some(interface_after)) =
            (total_after, package_after, interface_after)
        else {
            return ExtensionResultCacheInsertOutcomeV1::RejectedCapacity;
        };
        if total_after.entries > self.config.max_entries
            || total_after.bytes > self.config.max_bytes
            || package_after.entries > self.config.max_entries_per_package
            || package_after.bytes > self.config.max_bytes_per_package
            || interface_after.entries > self.config.max_entries_per_interface
            || interface_after.bytes > self.config.max_bytes_per_interface
        {
            return ExtensionResultCacheInsertOutcomeV1::RejectedCapacity;
        }
        remove_entry(&mut state, &admission.key);
        // The exact post-replacement usages were checked before removal, so
        // commit them directly rather than performing unchecked increments.
        state.total = total_after;
        state
            .packages
            .insert(admission.key.package_id.clone(), package_after);
        state
            .interfaces
            .insert(admission.key.interface_id, interface_after);
        state.entries.insert(
            admission.key,
            CacheEntryV1 {
                scope_epoch: admission.scope_epoch,
                global_epoch: admission.global_epoch,
                inserted_at: now,
                bytes,
                producer,
                // This token is cache-local. The source job's generation is
                // checked at admission time, but must not revoke a reusable
                // payload when that one view later navigates away.
                cache_generation: ExtensionValueGenerationV1::current(),
                entries,
            },
        );
        ExtensionResultCacheInsertOutcomeV1::Inserted
    }

    pub(crate) fn invalidate_feature_generation(
        &self,
        package_id: &str,
        sealed_manifest_digest: &str,
        feature_id: &str,
        feature_epoch: u64,
    ) {
        self.invalidate_matching(
            |key| {
                key.package_id == package_id
                    && key.sealed_manifest_digest == sealed_manifest_digest
                    && key.feature_id == feature_id
                    && key.feature_epoch == feature_epoch
            },
            |scope| {
                scope.package_id == package_id
                    && scope.sealed_manifest_digest == sealed_manifest_digest
                    && scope.feature_id == feature_id
                    && scope.feature_epoch == feature_epoch
            },
        );
    }

    /// Watcher and overflow recovery deliberately share this exact path: both
    /// advance the watcher scope epoch, invalidating recursive cache entries.
    pub fn invalidate_watcher_scope(&self, watcher_scope: u128) {
        if let Ok(mut state) = self.state.lock() {
            if ensure_watcher_generation(&mut state, watcher_scope, self.config.max_entries)
                .is_none()
            {
                return;
            }
            let Some(generation) = allocate_watcher_generation(&mut state) else {
                return;
            };
            state.watcher_generations.insert(watcher_scope, generation);
        }
        self.invalidate_matching(
            |key| key.watcher_scope == watcher_scope,
            |scope| scope.watcher_scope == watcher_scope,
        );
    }

    /// Manual refresh/F5 advances the same monotonic scope generation as all
    /// other cache invalidations for the sealed producer generation.
    pub fn invalidate_manual(&self, producer: &ExtensionJobProducerV1) {
        self.invalidate_feature_generation(
            producer.package_id(),
            producer.sealed_manifest_digest(),
            producer.feature_id(),
            producer.feature_epoch(),
        );
    }

    /// A new sealed data version invalidates old rows even before capacity
    /// pressure would evict them.
    pub fn invalidate_data_version(&self, producer: &ExtensionJobProducerV1) {
        self.invalidate_matching(
            |key| {
                key.package_id == producer.package_id()
                    && key.interface_id == producer.interface_id().into()
                    && key.data_version != producer.data_version()
            },
            |scope| {
                scope.package_id == producer.package_id()
                    && scope.interface_id == producer.interface_id().into()
                    && scope.data_version != producer.data_version()
            },
        );
    }

    /// Returns the host-issued watcher epoch required to mint a cache key for
    /// this scope. A caller must use this rather than replaying an old epoch.
    #[must_use]
    pub fn watcher_generation(&self, watcher_scope: u128) -> u64 {
        let Ok(mut state) = self.state.lock() else {
            return 0;
        };
        ensure_watcher_generation(&mut state, watcher_scope, self.config.max_entries).unwrap_or(0)
    }

    pub(crate) fn clear(&self) {
        self.invalidate_matching(|_| true, |_| true);
    }

    fn invalidate_matching(
        &self,
        matches_key: impl Fn(&ExtensionResultCacheKeyV1) -> bool,
        matches_scope: impl Fn(&ScopeV1) -> bool,
    ) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        let keys = state
            .entries
            .keys()
            .filter(|key| matches_key(key))
            .cloned()
            .collect::<Vec<_>>();
        for key in keys {
            remove_entry(&mut state, &key);
        }
        let scopes = state
            .scope_epochs
            .keys()
            .filter(|scope| matches_scope(scope))
            .cloned()
            .collect::<Vec<_>>();
        for scope in scopes {
            advance_scope_epoch(&mut state, scope);
        }
    }
}

fn key_matches_producer(
    key: &ExtensionResultCacheKeyV1,
    producer: &ExtensionJobProducerV1,
) -> bool {
    key.package_id == producer.package_id()
        && key.sealed_manifest_digest == producer.sealed_manifest_digest()
        && key.contribution_id == producer.contribution_id()
        && key.data_version == producer.data_version()
        && key.interface_id == producer.interface_id().into()
        && key.feature_id == producer.feature_id()
        && key.feature_epoch == producer.feature_epoch()
}

fn advance_scope_epoch(state: &mut CacheStateV1, scope: ScopeV1) {
    let current = state.scope_epochs.get(&scope).copied().unwrap_or(0);
    let Some(next) = current.checked_add(1) else {
        exhaust(state);
        return;
    };
    state.scope_epochs.insert(scope, next);
}

fn reclaim_expired(state: &mut CacheStateV1, now: Instant, ttl: Duration) {
    let expired = state
        .entries
        .iter()
        .filter(|(_, entry)| now.saturating_duration_since(entry.inserted_at) >= ttl)
        .map(|(key, _)| (key.clone(), ScopeV1::from(key)))
        .collect::<Vec<_>>();
    for (key, scope) in expired {
        remove_entry(state, &key);
        advance_scope_epoch(state, scope);
    }
}

fn replaced_usage(
    usage: UsageV1,
    previous_entries: usize,
    previous_bytes: usize,
    bytes: usize,
) -> Option<UsageV1> {
    Some(UsageV1 {
        entries: usage
            .entries
            .checked_sub(previous_entries)?
            .checked_add(1)?,
        bytes: usage
            .bytes
            .checked_sub(previous_bytes)?
            .checked_add(bytes)?,
    })
}

fn ensure_scope_epoch(
    state: &mut CacheStateV1,
    scope: ScopeV1,
    maximum_scopes: usize,
) -> Option<u64> {
    if state.exhausted {
        return None;
    }
    if !state.scope_epochs.contains_key(&scope) && state.scope_epochs.len() == maximum_scopes {
        // A single global epoch makes every outstanding admission stale before
        // pruning the bounded scope map, including admissions for an empty
        // scope that has not inserted a cache entry yet.
        for entry in state.entries.values() {
            entry.cache_generation.revoke();
        }
        state.entries.clear();
        state.packages.clear();
        state.interfaces.clear();
        state.total = UsageV1::default();
        state.scope_epochs.clear();
        state.watcher_generations.clear();
        advance_global_epoch(state);
    }
    (!state.exhausted).then(|| *state.scope_epochs.entry(scope).or_default())
}

fn ensure_watcher_generation(
    state: &mut CacheStateV1,
    watcher_scope: u128,
    maximum_scopes: usize,
) -> Option<u64> {
    if state.exhausted {
        return None;
    }
    if !state.watcher_generations.contains_key(&watcher_scope)
        && state.watcher_generations.len() == maximum_scopes
    {
        for entry in state.entries.values() {
            entry.cache_generation.revoke();
        }
        state.entries.clear();
        state.packages.clear();
        state.interfaces.clear();
        state.total = UsageV1::default();
        state.scope_epochs.clear();
        state.watcher_generations.clear();
        advance_global_epoch(state);
        if state.exhausted {
            return None;
        }
    }
    if let Some(generation) = state.watcher_generations.get(&watcher_scope) {
        return Some(*generation);
    }
    let generation = allocate_watcher_generation(state)?;
    state.watcher_generations.insert(watcher_scope, generation);
    Some(generation)
}

fn allocate_watcher_generation(state: &mut CacheStateV1) -> Option<u64> {
    let generation = state.next_watcher_generation.max(1);
    let Some(next) = generation.checked_add(1) else {
        exhaust(state);
        return None;
    };
    state.next_watcher_generation = next;
    Some(generation)
}

fn advance_global_epoch(state: &mut CacheStateV1) {
    let Some(next) = state.global_epoch.checked_add(1) else {
        exhaust(state);
        return;
    };
    state.global_epoch = next;
}

fn exhaust(state: &mut CacheStateV1) {
    for entry in state.entries.values() {
        entry.cache_generation.revoke();
    }
    state.entries.clear();
    state.packages.clear();
    state.interfaces.clear();
    state.total = UsageV1::default();
    state.scope_epochs.clear();
    state.watcher_generations.clear();
    state.exhausted = true;
}

fn remove_entry(state: &mut CacheStateV1, key: &ExtensionResultCacheKeyV1) {
    let Some(entry) = state.entries.remove(key) else {
        return;
    };
    entry.cache_generation.revoke();
    state.total.entries = state.total.entries.saturating_sub(1);
    state.total.bytes = state.total.bytes.saturating_sub(entry.bytes);
    if let Some(package) = state.packages.get_mut(&key.package_id) {
        package.entries = package.entries.saturating_sub(1);
        package.bytes = package.bytes.saturating_sub(entry.bytes);
        if package.entries == 0 {
            state.packages.remove(&key.package_id);
        }
    }
    if let Some(interface) = state.interfaces.get_mut(&key.interface_id) {
        interface.entries = interface.entries.saturating_sub(1);
        interface.bytes = interface.bytes.saturating_sub(entry.bytes);
        if interface.entries == 0 {
            state.interfaces.remove(&key.interface_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cache() -> ExtensionResultCacheV1 {
        ExtensionResultCacheV1::new(
            ExtensionResultCacheConfigV1::try_new(1, 1, 1, 1, 1, 1, Duration::from_secs(1))
                .unwrap(),
        )
    }

    fn scope() -> ScopeV1 {
        ScopeV1 {
            package_id: "package".to_owned(),
            sealed_manifest_digest: "digest".to_owned(),
            contribution_id: "column".to_owned(),
            data_version: 1,
            interface_id: InterfaceKeyV1(1, 1),
            feature_id: "feature".to_owned(),
            feature_epoch: 1,
            watcher_scope: 1,
        }
    }

    #[test]
    fn exhausted_epoch_or_watcher_counter_permanently_fails_closed() {
        let first_cache = cache();
        let mut state = first_cache.state.lock().unwrap();
        let scope = scope();
        state.scope_epochs.insert(scope.clone(), u64::MAX);
        advance_scope_epoch(&mut state, scope);
        assert!(state.exhausted);
        assert!(state.entries.is_empty());
        drop(state);

        let cache = cache();
        let mut state = cache.state.lock().unwrap();
        state.next_watcher_generation = u64::MAX;
        assert_eq!(allocate_watcher_generation(&mut state), None);
        assert!(state.exhausted);
        assert_eq!(ensure_watcher_generation(&mut state, 9, 1), None);
    }
}
