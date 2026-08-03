//! Host-owned, deadline-bounded UI invalidation coalescing for extension jobs.
//!
//! Extension workers are allowed to complete incrementally, but no worker may
//! cause a synchronous redraw.  The UI thread records accepted host batches
//! here and drains one bounded invalidation transaction after the first result
//! in a window has waited long enough.

use std::{
    collections::BTreeSet,
    marker::PhantomData,
    rc::Rc,
    thread::{self, ThreadId},
    time::{Duration, Instant},
};

use explorer_extension_api::{JobHandleV1, StableIdV1};

use crate::AcceptedIncrementalResultBatchV1;

/// Smallest permitted invalidation coalescing window.
pub const MIN_UI_INVALIDATION_WINDOW_V1: Duration = Duration::from_millis(16);
/// Largest permitted invalidation coalescing window.
pub const MAX_UI_INVALIDATION_WINDOW_V1: Duration = Duration::from_millis(50);
/// Largest number of precise extension-interface scopes in one UI transaction.
pub const MAX_UI_INVALIDATION_SCOPES_V1: usize = 1_024;
/// Largest number of generation-tagged records retained before a broad,
/// still-coalesced fallback replaces per-record tracking.
pub const MAX_UI_INVALIDATION_RECORDS_V1: usize = 1_024;

/// A bounded configuration error for [`UiInvalidationBatcherV1`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiInvalidationBatcherConfigErrorV1 {
    /// A shorter period would permit per-item synchronous redraw pressure.
    WindowTooShort,
    /// A longer period would make incrementally completed values stale to UI.
    WindowTooLong,
    /// The scope budget must be a non-zero bounded value.
    ScopeLimitInvalid,
}

/// Validated limits for host-owned UI invalidation batching.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UiInvalidationBatcherConfigV1 {
    window: Duration,
    max_scopes: usize,
}

impl UiInvalidationBatcherConfigV1 {
    /// Constructs a configuration whose first accepted result is presented in
    /// the inclusive 16--50 ms contract window.
    ///
    /// # Errors
    ///
    /// Returns a bounded configuration error when the window or scope limit
    /// falls outside the host's fixed resource contract.
    pub fn try_new(
        window: Duration,
        max_scopes: usize,
    ) -> Result<Self, UiInvalidationBatcherConfigErrorV1> {
        if window < MIN_UI_INVALIDATION_WINDOW_V1 {
            return Err(UiInvalidationBatcherConfigErrorV1::WindowTooShort);
        }
        if window > MAX_UI_INVALIDATION_WINDOW_V1 {
            return Err(UiInvalidationBatcherConfigErrorV1::WindowTooLong);
        }
        if max_scopes == 0 || max_scopes > MAX_UI_INVALIDATION_SCOPES_V1 {
            return Err(UiInvalidationBatcherConfigErrorV1::ScopeLimitInvalid);
        }
        Ok(Self { window, max_scopes })
    }

    /// Returns the fixed window measured from the first result, never the last.
    #[must_use]
    pub const fn window(self) -> Duration {
        self.window
    }

    /// Returns the precise interface-scope retention bound.
    #[must_use]
    pub const fn max_scopes(self) -> usize {
        self.max_scopes
    }
}

/// A path-free scope which the host UI may invalidate.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum UiInvalidationScopeV1 {
    /// One sealed package's declared extension interface.
    ExtensionInterface {
        package_id: String,
        interface_namespace: u32,
        interface_value: u64,
    },
    /// A safe bounded fallback when precise scopes exceed the configured cap.
    ///
    /// This still produces one coalesced UI transaction; it is never an
    /// indication to redraw for each result. Before this scope is emitted the
    /// UI pump still rechecks current job generations; it asks the UI to read
    /// only host-retained current rows, never stale payloads.
    AllExtensionResults,
}

impl UiInvalidationScopeV1 {
    /// Returns the package identity for a precise scope.
    #[must_use]
    pub fn package_id(&self) -> Option<&str> {
        match self {
            Self::ExtensionInterface { package_id, .. } => Some(package_id),
            Self::AllExtensionResults => None,
        }
    }
}

/// One UI-thread transaction of coalesced extension result invalidations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiInvalidationBatchV1 {
    scopes: Vec<UiInvalidationScopeV1>,
    accepted_batches: usize,
    accepted_items: usize,
    scope_overflowed: bool,
}

#[derive(Clone, Debug)]
struct PendingInvalidationRecordV1 {
    scope: UiInvalidationScopeV1,
    job: JobHandleV1,
    item_generation: u64,
    location_generation: u64,
    source_generation: u64,
    item_count: usize,
    recorded_at: Instant,
}

impl UiInvalidationBatchV1 {
    /// Returns deterministic, bounded scopes needing a redraw.
    #[must_use]
    pub fn scopes(&self) -> &[UiInvalidationScopeV1] {
        &self.scopes
    }

    /// Returns how many host-accepted result batches were coalesced.
    #[must_use]
    pub const fn accepted_batches(&self) -> usize {
        self.accepted_batches
    }

    /// Returns the number of result entries represented without retaining rows.
    #[must_use]
    pub const fn accepted_items(&self) -> usize {
        self.accepted_items
    }

    /// Returns whether a broad, still-coalesced fallback was required.
    #[must_use]
    pub const fn scope_overflowed(&self) -> bool {
        self.scope_overflowed
    }
}

/// Coalesces accepted extension result batches on the host UI thread.
///
/// [`Self::record_accepted_batch`] deliberately has no redraw callback.  The
/// UI must schedule the returned [`Self::next_deadline`] and invoke
/// [`Self::drain_due`] there, so a fast provider cannot create a redraw loop.
#[derive(Debug)]
pub struct UiInvalidationBatcherV1 {
    config: UiInvalidationBatcherConfigV1,
    owner_thread: ThreadId,
    // UI ownership is thread-affine: this makes the batcher neither Send nor
    // Sync, so a worker cannot move it off the GPUI thread.
    _not_send_or_sync: PhantomData<Rc<()>>,
    opened_at: Option<Instant>,
    records: Vec<PendingInvalidationRecordV1>,
    accepted_batches: usize,
    accepted_items: usize,
    record_overflowed: bool,
}

impl UiInvalidationBatcherV1 {
    /// Creates an empty host-owned invalidation accumulator.
    #[must_use]
    pub fn new(config: UiInvalidationBatcherConfigV1) -> Self {
        Self {
            config,
            owner_thread: thread::current().id(),
            _not_send_or_sync: PhantomData,
            opened_at: None,
            records: Vec::new(),
            accepted_batches: 0,
            accepted_items: 0,
            record_overflowed: false,
        }
    }

    /// Reports whether the caller is the GPUI/UI thread that owns this batcher.
    #[must_use]
    pub fn is_on_owner_thread(&self) -> bool {
        self.owner_thread == thread::current().id()
    }

    /// Records a batch only after the host has accepted and applied it.
    ///
    /// This only updates bounded host state. It neither calls extension code
    /// nor synchronously invalidates a GPUI entity.
    pub fn record_accepted_batch(&mut self, batch: &AcceptedIncrementalResultBatchV1) {
        self.record_applied_metadata(
            batch.producer.package_id(),
            batch.producer.interface_id(),
            batch.job,
            batch.item_generation,
            batch.location_generation,
            batch.source_generation,
            batch.entry_count(),
            Instant::now(),
        );
    }

    /// Records a post-commit fact supplied by the bounded UI bridge. This is
    /// crate-visible so the bridge need not retain foreign result bytes after
    /// the runtime has committed them.
    pub(crate) fn record_applied_metadata(
        &mut self,
        package_id: &str,
        interface_id: StableIdV1,
        job: JobHandleV1,
        item_generation: u64,
        location_generation: u64,
        source_generation: u64,
        item_count: usize,
        recorded_at: Instant,
    ) {
        self.record_at(
            package_id,
            interface_id,
            job,
            item_generation,
            location_generation,
            source_generation,
            item_count,
            recorded_at,
        );
    }

    /// Requests one broad, current-state refresh after a bounded upstream
    /// mailbox overflow. No foreign row bytes are retained by this signal.
    pub fn record_full_refresh(&mut self) {
        self.record_full_refresh_at(Instant::now());
    }

    /// Records a broad post-commit refresh at the original acceptance time.
    pub(crate) fn record_full_refresh_at(&mut self, recorded_at: Instant) {
        self.opened_at = Some(
            self.opened_at
                .map_or(recorded_at, |opened_at| opened_at.min(recorded_at)),
        );
        self.record_overflowed = true;
    }

    /// Removes pending records for `job` whose generation is no longer current.
    ///
    /// This is called immediately after the runtime accepts a generation
    /// advance, before a due UI transaction can expose stale work. The bounded
    /// broad-fallback case remains safe because it requests a current full
    /// extension-result refresh rather than publishing a retained old row.
    pub fn discard_stale_for_job(
        &mut self,
        job: JobHandleV1,
        current_item_generation: u64,
        current_location_generation: u64,
        current_source_generation: u64,
    ) {
        let mut discarded_batches = 0_usize;
        let mut discarded_items = 0_usize;
        self.records.retain(|record| {
            let stale = record.job == job
                && (record.item_generation != current_item_generation
                    || record.location_generation != current_location_generation
                    || record.source_generation != current_source_generation);
            if stale {
                discarded_batches = discarded_batches.saturating_add(1);
                discarded_items = discarded_items.saturating_add(record.item_count);
            }
            !stale
        });
        self.accepted_batches = self.accepted_batches.saturating_sub(discarded_batches);
        self.accepted_items = self.accepted_items.saturating_sub(discarded_items);
        self.recompute_opened_at();
    }

    /// Removes every pending record from a cancelled or revoked job.
    pub fn discard_job(&mut self, job: JobHandleV1) {
        self.discard_not_current(|candidate, _, _, _| candidate != job);
    }

    /// Rechecks all pending records against current host state without waiting
    /// for the deadline. This closes the apply-to-record race when navigation
    /// or cancellation advances concurrently with result application.
    pub fn discard_not_current(
        &mut self,
        mut is_current: impl FnMut(JobHandleV1, u64, u64, u64) -> bool,
    ) {
        self.discard_records_not_current(&mut is_current);
    }

    /// Returns the non-sliding deadline for the pending coalescing window.
    #[must_use]
    pub fn next_deadline(&self) -> Option<Instant> {
        self.opened_at
            .and_then(|opened_at| opened_at.checked_add(self.config.window()))
    }

    /// Drains one transaction only when its bounded window has elapsed.
    #[must_use]
    #[cfg(test)]
    pub(crate) fn drain_due(&mut self, now: Instant) -> Option<UiInvalidationBatchV1> {
        self.drain_due_with_current(now, |_, _, _, _| true)
    }

    /// Drains only records whose job/view generation is still current at the
    /// final UI-thread emission point.
    #[must_use]
    pub fn drain_due_with_current(
        &mut self,
        now: Instant,
        mut is_current: impl FnMut(JobHandleV1, u64, u64, u64) -> bool,
    ) -> Option<UiInvalidationBatchV1> {
        let deadline = self.next_deadline()?;
        if now < deadline {
            return None;
        }
        self.discard_records_not_current(&mut is_current);
        if self.accepted_batches == 0 && !self.record_overflowed {
            self.opened_at = None;
            return None;
        }
        Some(self.take_pending())
    }

    /// Drops pending invalidations when their view generation is no longer current.
    pub fn clear(&mut self) {
        self.opened_at = None;
        self.records.clear();
        self.accepted_batches = 0;
        self.accepted_items = 0;
        self.record_overflowed = false;
    }

    /// Returns whether a UI deadline is currently scheduled.
    #[must_use]
    pub const fn is_pending(&self) -> bool {
        self.opened_at.is_some()
    }

    fn record_at(
        &mut self,
        package_id: &str,
        interface_id: StableIdV1,
        job: JobHandleV1,
        item_generation: u64,
        location_generation: u64,
        source_generation: u64,
        item_count: usize,
        now: Instant,
    ) {
        self.opened_at = Some(self.opened_at.map_or(now, |opened_at| opened_at.min(now)));
        let scope = UiInvalidationScopeV1::ExtensionInterface {
            package_id: package_id.to_owned(),
            interface_namespace: interface_id.namespace.into_raw(),
            interface_value: interface_id.value,
        };
        if self.records.len() < MAX_UI_INVALIDATION_RECORDS_V1 {
            self.records.push(PendingInvalidationRecordV1 {
                scope,
                job,
                item_generation,
                location_generation,
                source_generation,
                item_count,
                recorded_at: now,
            });
        } else {
            self.record_overflowed = true;
        }
        self.accepted_batches = self.accepted_batches.saturating_add(1);
        self.accepted_items = self.accepted_items.saturating_add(item_count);
    }

    fn take_pending(&mut self) -> UiInvalidationBatchV1 {
        let scopes = self
            .records
            .iter()
            .map(|record| record.scope.clone())
            .collect::<BTreeSet<_>>();
        let scope_overflowed = self.record_overflowed || scopes.len() > self.config.max_scopes();
        let mut scopes = scopes
            .into_iter()
            .take(self.config.max_scopes())
            .collect::<Vec<_>>();
        if scope_overflowed {
            scopes.push(UiInvalidationScopeV1::AllExtensionResults);
        }
        let batch = UiInvalidationBatchV1 {
            scopes,
            accepted_batches: self.accepted_batches,
            accepted_items: self.accepted_items,
            scope_overflowed,
        };
        self.opened_at = None;
        self.records.clear();
        self.accepted_batches = 0;
        self.accepted_items = 0;
        self.record_overflowed = false;
        batch
    }

    fn discard_records_not_current(
        &mut self,
        is_current: &mut impl FnMut(JobHandleV1, u64, u64, u64) -> bool,
    ) {
        let mut discarded_batches = 0_usize;
        let mut discarded_items = 0_usize;
        self.records.retain(|record| {
            let current = is_current(
                record.job,
                record.item_generation,
                record.location_generation,
                record.source_generation,
            );
            if !current {
                discarded_batches = discarded_batches.saturating_add(1);
                discarded_items = discarded_items.saturating_add(record.item_count);
            }
            current
        });
        self.accepted_batches = self.accepted_batches.saturating_sub(discarded_batches);
        self.accepted_items = self.accepted_items.saturating_sub(discarded_items);
        self.recompute_opened_at();
    }

    fn recompute_opened_at(&mut self) {
        if self.record_overflowed {
            // After the fixed record budget is exhausted, at least one result
            // cannot be selectively tracked. Preserve the original deadline
            // and emit a single broad current-refresh request at due time.
            // The UI reads only runtime-current rows, so this never revives a
            // stale payload while avoiding loss of an untracked current row.
            return;
        }
        self.opened_at = self.records.iter().map(|record| record.recorded_at).min();
        if self.opened_at.is_none() {
            self.accepted_batches = 0;
            self.accepted_items = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use explorer_extension_api::{IdNamespaceV1, StableIdV1};

    fn config(window: Duration, scopes: usize) -> UiInvalidationBatcherConfigV1 {
        UiInvalidationBatcherConfigV1::try_new(window, scopes).expect("valid batcher config")
    }

    fn id(value: u64) -> StableIdV1 {
        StableIdV1::new(IdNamespaceV1::new(1, 1), value)
    }

    fn job(value: u8) -> JobHandleV1 {
        JobHandleV1::from_host([value; 16], 1)
    }

    #[test]
    fn configuration_accepts_only_the_inclusive_16_to_50_ms_window() {
        assert_eq!(
            UiInvalidationBatcherConfigV1::try_new(Duration::from_millis(15), 1),
            Err(UiInvalidationBatcherConfigErrorV1::WindowTooShort)
        );
        assert_eq!(
            UiInvalidationBatcherConfigV1::try_new(Duration::from_millis(51), 1),
            Err(UiInvalidationBatcherConfigErrorV1::WindowTooLong)
        );
        assert!(UiInvalidationBatcherConfigV1::try_new(Duration::from_millis(16), 1).is_ok());
        assert!(UiInvalidationBatcherConfigV1::try_new(Duration::from_millis(50), 1).is_ok());
    }

    #[test]
    fn rapid_results_are_coalesced_without_a_per_item_drain() {
        let start = Instant::now();
        let mut batcher = UiInvalidationBatcherV1::new(config(Duration::from_millis(16), 8));
        for item in 0..1_000 {
            batcher.record_at(
                "package",
                id(7),
                job(1),
                1,
                1,
                1,
                1,
                start + Duration::from_micros(item),
            );
            assert!(
                batcher
                    .drain_due(start + Duration::from_micros(item))
                    .is_none()
            );
        }
        let batch = batcher
            .drain_due(start + Duration::from_millis(16))
            .expect("one coalesced UI transaction");
        assert_eq!(batch.accepted_batches(), 1_000);
        assert_eq!(batch.accepted_items(), 1_000);
        assert_eq!(batch.scopes().len(), 1);
        assert!(!batcher.is_pending());
    }

    #[test]
    fn one_thousand_one_ms_arrivals_use_fifty_non_sliding_twenty_ms_transactions() {
        let start = Instant::now();
        let mut batcher = UiInvalidationBatcherV1::new(config(Duration::from_millis(20), 8));
        let mut transactions = 0_usize;
        let mut represented_items = 0_usize;
        for item in 0..1_000_u64 {
            let now = start + Duration::from_millis(item);
            if let Some(batch) = batcher.drain_due(now) {
                transactions = transactions.saturating_add(1);
                represented_items = represented_items.saturating_add(batch.accepted_items());
            }
            batcher.record_at("package", id(7), job(1), 1, 1, 1, 1, now);
        }
        if let Some(batch) = batcher.drain_due(start + Duration::from_secs(1)) {
            transactions = transactions.saturating_add(1);
            represented_items = represented_items.saturating_add(batch.accepted_items());
        }
        assert_eq!(transactions, 50);
        assert_eq!(represented_items, 1_000);
    }

    #[test]
    fn later_results_cannot_extend_the_first_result_deadline() {
        let start = Instant::now();
        let mut batcher = UiInvalidationBatcherV1::new(config(Duration::from_millis(50), 8));
        batcher.record_at("package", id(7), job(1), 1, 1, 1, 1, start);
        let deadline = batcher.next_deadline().expect("deadline");
        batcher.record_at(
            "package",
            id(8),
            job(1),
            1,
            1,
            1,
            1,
            start + Duration::from_millis(49),
        );
        assert_eq!(batcher.next_deadline(), Some(deadline));
        assert!(
            batcher
                .drain_due(start + Duration::from_millis(49))
                .is_none()
        );
        assert!(batcher.drain_due(deadline).is_some());
    }

    #[test]
    fn bounded_scope_retention_falls_back_to_one_coalesced_global_scope() {
        let start = Instant::now();
        let mut batcher = UiInvalidationBatcherV1::new(config(Duration::from_millis(16), 2));
        for value in 1..=3 {
            batcher.record_at(
                "package",
                id(value),
                job(u8::try_from(value).expect("bounded test value")),
                1,
                1,
                1,
                1,
                start,
            );
        }
        let batch = batcher
            .drain_due(start + Duration::from_millis(16))
            .expect("bounded transaction");
        assert_eq!(batch.scopes().len(), 3);
        assert!(batch.scope_overflowed());
        assert_eq!(
            batch.scopes().last(),
            Some(&UiInvalidationScopeV1::AllExtensionResults)
        );
    }

    #[test]
    fn clear_discards_stale_view_work_before_any_ui_invalidation() {
        let start = Instant::now();
        let mut batcher = UiInvalidationBatcherV1::new(config(Duration::from_millis(16), 8));
        batcher.record_at("package", id(7), job(1), 1, 1, 1, 1, start);
        batcher.clear();
        assert!(!batcher.is_pending());
        assert!(
            batcher
                .drain_due(start + Duration::from_millis(50))
                .is_none()
        );
    }

    #[test]
    fn generation_change_selectively_discards_only_stale_job_records_before_due() {
        let start = Instant::now();
        let mut batcher = UiInvalidationBatcherV1::new(config(Duration::from_millis(20), 8));
        batcher.record_at("one", id(1), job(1), 1, 1, 1, 2, start);
        batcher.record_at("two", id(2), job(2), 1, 1, 1, 3, start);
        batcher.record_at("one", id(1), job(1), 2, 2, 2, 5, start);
        batcher.discard_stale_for_job(job(1), 2, 2, 2);
        let batch = batcher
            .drain_due(start + Duration::from_millis(20))
            .expect("current records emit once due");
        assert_eq!(batch.accepted_batches(), 2);
        assert_eq!(batch.accepted_items(), 8);
        assert_eq!(batch.scopes().len(), 2);
    }

    #[test]
    fn due_emission_rechecks_generation_and_cancel_without_a_worker_redraw() {
        let start = Instant::now();
        let mut batcher = UiInvalidationBatcherV1::new(config(Duration::from_millis(16), 8));
        batcher.record_at("cancelled", id(1), job(1), 1, 1, 1, 1, start);
        batcher.record_at("current", id(2), job(2), 1, 1, 1, 1, start);
        let batch = batcher
            .drain_due_with_current(start + Duration::from_millis(16), |candidate, _, _, _| {
                candidate == job(2)
            })
            .expect("only the current job emits");
        assert_eq!(batch.accepted_batches(), 1);
        assert_eq!(batch.accepted_items(), 1);
        assert_eq!(batch.scopes().len(), 1);
        assert_eq!(batch.scopes()[0].package_id(), Some("current"));
    }

    #[test]
    fn discarding_the_window_opener_restarts_the_deadline_from_current_arrival() {
        let start = Instant::now();
        let mut batcher = UiInvalidationBatcherV1::new(config(Duration::from_millis(16), 8));
        batcher.record_at("stale", id(1), job(1), 1, 1, 1, 1, start);
        let later = start + Duration::from_millis(12);
        batcher.record_at("current", id(2), job(2), 1, 1, 1, 1, later);
        batcher.discard_job(job(1));
        assert_eq!(
            batcher.next_deadline(),
            Some(later + Duration::from_millis(16))
        );
        assert!(
            batcher
                .drain_due(later + Duration::from_millis(15))
                .is_none()
        );
        assert!(
            batcher
                .drain_due(later + Duration::from_millis(16))
                .is_some()
        );
    }

    #[test]
    fn overflow_keeps_a_broad_current_refresh_after_precise_stale_records_are_cancelled() {
        let start = Instant::now();
        let mut batcher = UiInvalidationBatcherV1::new(config(Duration::from_millis(16), 8));
        for value in 0..=MAX_UI_INVALIDATION_RECORDS_V1 {
            batcher.record_at(
                "overflow",
                id(1),
                job(1),
                1,
                1,
                1,
                1,
                start + Duration::from_micros(u64::try_from(value).expect("bounded value")),
            );
        }
        batcher.discard_job(job(1));
        let batch = batcher
            .drain_due_with_current(start + Duration::from_millis(16), |candidate, _, _, _| {
                candidate != job(1)
            })
            .expect("untracked current overflow requests one broad refresh");
        assert!(batch.scope_overflowed());
        assert_eq!(
            batch.scopes().last(),
            Some(&UiInvalidationScopeV1::AllExtensionResults)
        );
    }
}
