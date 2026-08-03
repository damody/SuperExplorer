//! Bounded, host-owned handoff between extension jobs and the GPUI thread.
//!
//! The first queue contains only ready *signals*, never foreign value bytes:
//! accepted data remains under the runtime's credit/backpressure accounting
//! until the UI model drains it. The second queue is populated only after the
//! model has atomically applied a batch. This separation prevents an inbox
//! overflow from turning an un-applied accepted result into silent data loss.

use std::{
    collections::{HashSet, VecDeque},
    sync::{Arc, Mutex, Weak},
    time::Instant,
};

use explorer_extension_api::{JobHandleV1, StableIdV1};

use crate::{
    AcceptedIncrementalResultBatchV1, ExtensionJobRuntimeV1, UiInvalidationBatchV1,
    UiInvalidationBatcherConfigV1, UiInvalidationBatcherV1,
};

/// Maximum number of small, deduplicated ready-job signals retained at once.
pub const MAX_EXTENSION_UI_READY_SIGNALS_V1: usize = 1_024;
/// Maximum post-apply invalidation notices retained before one broad refresh.
pub const MAX_EXTENSION_UI_APPLIED_NOTICES_V1: usize = 1_024;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ExtensionJobUiReadySignalV1 {
    job: JobHandleV1,
    item_generation: u64,
    location_generation: u64,
    source_generation: u64,
}

impl ExtensionJobUiReadySignalV1 {
    pub(crate) const fn from_runtime(
        job: JobHandleV1,
        item_generation: u64,
        location_generation: u64,
        source_generation: u64,
    ) -> Self {
        Self {
            job,
            item_generation,
            location_generation,
            source_generation,
        }
    }
    #[must_use]
    pub const fn job(self) -> JobHandleV1 {
        self.job
    }

    #[must_use]
    pub const fn generations(self) -> (u64, u64, u64) {
        (
            self.item_generation,
            self.location_generation,
            self.source_generation,
        )
    }
}

#[derive(Clone, Debug)]
struct AppliedInvalidationNoticeV1 {
    package_id: String,
    interface_id: StableIdV1,
    job: JobHandleV1,
    item_generation: u64,
    location_generation: u64,
    source_generation: u64,
    item_count: usize,
    applied_at: Instant,
}

#[derive(Debug, Default)]
struct MailboxStateV1 {
    ready: VecDeque<ExtensionJobUiReadySignalV1>,
    ready_dedup: HashSet<ExtensionJobUiReadySignalV1>,
    ready_rescan_required: bool,
    ready_rescan_epoch: u64,
    applied: VecDeque<AppliedInvalidationNoticeV1>,
    applied_full_refresh_required: bool,
    applied_full_refresh_at: Option<Instant>,
    closed: bool,
}

#[derive(Debug)]
pub(crate) struct SharedMailboxV1 {
    state: Mutex<MailboxStateV1>,
}

/// Cloneable producer-only ingress. It cannot consume messages or close the
/// UI inbox. No ingress method holds the mutex while extension-owned data is
/// dropped or code is called.
#[derive(Clone, Debug)]
pub struct ExtensionJobUiIngressV1 {
    runtime: Arc<ExtensionJobRuntimeV1>,
    mailbox: Arc<SharedMailboxV1>,
}

/// The single UI-owned inbox. It is intentionally not cloneable: only the
/// composition root may take and operate the GPUI-facing consumer.
#[derive(Debug)]
pub struct ExtensionJobUiInboxV1 {
    runtime: Arc<ExtensionJobRuntimeV1>,
    mailbox: Arc<SharedMailboxV1>,
}

impl ExtensionJobUiIngressV1 {
    pub(crate) fn new_pair(runtime: Arc<ExtensionJobRuntimeV1>) -> (Self, ExtensionJobUiInboxV1) {
        let mailbox = Arc::new(SharedMailboxV1 {
            state: Mutex::new(MailboxStateV1::default()),
        });
        (
            Self {
                runtime: Arc::clone(&runtime),
                mailbox: Arc::clone(&mailbox),
            },
            ExtensionJobUiInboxV1 { runtime, mailbox },
        )
    }

    /// Creates the otherwise host-private ingress/inbox pair for an
    /// application integration fixture compiled with `integration-test-support`.
    #[cfg(feature = "integration-test-support")]
    #[must_use]
    pub fn new_integration_pair(
        runtime: Arc<ExtensionJobRuntimeV1>,
    ) -> (Self, ExtensionJobUiInboxV1) {
        let (ingress, inbox) = Self::new_pair(runtime);
        ingress
            .runtime
            .install_ready_signal_sink(ingress.runtime_ready_sink());
        (ingress, inbox)
    }

    /// Signals that a runtime-owned job has accepted data. The accepted batch
    /// stays in the runtime queue until a model drains it, so overflow can only
    /// require a bounded rescan and can never lose un-applied values.
    #[must_use]
    pub fn signal_ready(
        &self,
        signal: ExtensionJobUiReadySignalV1,
    ) -> ExtensionJobUiSignalOutcomeV1 {
        let Ok(mut state) = self.mailbox.state.lock() else {
            return ExtensionJobUiSignalOutcomeV1::Closed;
        };
        if state.closed {
            return ExtensionJobUiSignalOutcomeV1::Closed;
        }
        if state.ready_dedup.contains(&signal) {
            return ExtensionJobUiSignalOutcomeV1::AlreadyQueued;
        }
        if state.ready.len() == MAX_EXTENSION_UI_READY_SIGNALS_V1 {
            state.ready_rescan_required = true;
            state.ready_rescan_epoch = state.ready_rescan_epoch.saturating_add(1);
            return ExtensionJobUiSignalOutcomeV1::RescanRequired;
        }
        state.ready_dedup.insert(signal);
        state.ready.push_back(signal);
        ExtensionJobUiSignalOutcomeV1::Queued
    }

    pub(crate) fn runtime_ready_sink(&self) -> RuntimeReadySignalSinkV1 {
        RuntimeReadySignalSinkV1 {
            mailbox: Arc::downgrade(&self.mailbox),
        }
    }

    /// Records a post-commit presentation fact. A full-refresh fallback is
    /// safe here because the rows were already committed to host-owned state.
    pub fn notify_applied(&self, batch: &AcceptedIncrementalResultBatchV1) {
        self.notify_applied_at(batch, Instant::now());
    }

    /// Records a post-commit presentation fact at the host's monotonic event
    /// time. The normal composition path uses [`Self::notify_applied`]; this
    /// crate-visible form lets deterministic integration fixtures preserve the
    /// same mailbox and batcher behavior without wall-clock sleeps.
    pub(crate) fn notify_applied_at(
        &self,
        batch: &AcceptedIncrementalResultBatchV1,
        applied_at: Instant,
    ) {
        let notice = AppliedInvalidationNoticeV1 {
            package_id: batch.producer.package_id().to_owned(),
            interface_id: batch.producer.interface_id(),
            job: batch.job,
            item_generation: batch.item_generation,
            location_generation: batch.location_generation,
            source_generation: batch.source_generation,
            item_count: batch.entry_count(),
            applied_at,
        };
        let Ok(mut state) = self.mailbox.state.lock() else {
            return;
        };
        if state.closed {
            return;
        }
        if state.applied.len() == MAX_EXTENSION_UI_APPLIED_NOTICES_V1 {
            state.applied_full_refresh_required = true;
            state.applied_full_refresh_at = Some(
                state
                    .applied_full_refresh_at
                    .map_or(notice.applied_at, |earliest| {
                        earliest.min(notice.applied_at)
                    }),
            );
            return;
        }
        state.applied.push_back(notice);
    }

    #[must_use]
    pub fn is_for_runtime(&self, runtime: &Arc<ExtensionJobRuntimeV1>) -> bool {
        Arc::ptr_eq(&self.runtime, runtime)
    }

    pub(crate) fn close(&self) {
        if let Ok(mut state) = self.mailbox.state.lock() {
            state.closed = true;
            state.ready.clear();
            state.ready_dedup.clear();
            state.ready_rescan_required = false;
            state.applied.clear();
            state.applied_full_refresh_required = false;
            state.applied_full_refresh_at = None;
        }
    }
}

/// Weak runtime-owned hook used after an accepted batch is safely copied into
/// the bounded runtime queue. It deliberately retains no runtime or batch
/// payload, preventing an ownership cycle and preventing an overflow from
/// dropping accepted data.
#[derive(Clone, Debug)]
pub(crate) struct RuntimeReadySignalSinkV1 {
    mailbox: Weak<SharedMailboxV1>,
}

impl RuntimeReadySignalSinkV1 {
    pub(crate) fn signal(&self, signal: ExtensionJobUiReadySignalV1) {
        let Some(mailbox) = self.mailbox.upgrade() else {
            return;
        };
        let Ok(mut state) = mailbox.state.lock() else {
            return;
        };
        if state.closed || state.ready_dedup.contains(&signal) {
            return;
        }
        if state.ready.len() == MAX_EXTENSION_UI_READY_SIGNALS_V1 {
            state.ready_rescan_required = true;
            state.ready_rescan_epoch = state.ready_rescan_epoch.saturating_add(1);
            return;
        }
        state.ready_dedup.insert(signal);
        state.ready.push_back(signal);
    }
}

/// Result of a non-blocking ready notification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExtensionJobUiSignalOutcomeV1 {
    Queued,
    AlreadyQueued,
    RescanRequired,
    Closed,
}

/// A bounded ready-signal drain. `rescan_required` asks the model to enumerate
/// runtime-current queued jobs before accepting more work; it never means an
/// accepted value was discarded.
#[derive(Debug, Default)]
pub struct ExtensionJobUiReadyDrainV1 {
    pub signals: Vec<ExtensionJobUiReadySignalV1>,
    pub rescan_required: bool,
    pub rescan_epoch: Option<u64>,
}

impl ExtensionJobUiInboxV1 {
    /// Takes ready signals. A zero limit deliberately preserves the rescan
    /// flag, so a scheduling probe cannot accidentally acknowledge overflow.
    #[must_use]
    pub fn take_ready(&mut self, maximum_signals: usize) -> ExtensionJobUiReadyDrainV1 {
        if maximum_signals == 0 {
            return ExtensionJobUiReadyDrainV1::default();
        }
        let Ok(mut state) = self.mailbox.state.lock() else {
            return ExtensionJobUiReadyDrainV1::default();
        };
        let mut signals =
            Vec::with_capacity(maximum_signals.min(MAX_EXTENSION_UI_READY_SIGNALS_V1));
        for _ in 0..maximum_signals.min(MAX_EXTENSION_UI_READY_SIGNALS_V1) {
            let Some(signal) = state.ready.pop_front() else {
                break;
            };
            state.ready_dedup.remove(&signal);
            signals.push(signal);
        }
        let rescan_required = state.ready_rescan_required;
        let rescan_epoch = rescan_required.then_some(state.ready_rescan_epoch);
        ExtensionJobUiReadyDrainV1 {
            signals,
            rescan_required,
            rescan_epoch,
        }
    }

    /// Acknowledges a successful bounded scan of
    /// [`ExtensionJobRuntimeV1::ready_job_signals`]. Callers must invoke this
    /// only after they have merged the entire snapshot into their UI model.
    pub fn acknowledge_ready_rescan(&mut self, observed_epoch: u64) {
        if let Ok(mut state) = self.mailbox.state.lock()
            && state.ready_rescan_epoch == observed_epoch
        {
            state.ready_rescan_required = false;
        }
    }

    fn take_applied(
        &mut self,
        maximum_notices: usize,
    ) -> (Vec<AppliedInvalidationNoticeV1>, Option<Instant>) {
        if maximum_notices == 0 {
            return (Vec::new(), None);
        }
        let Ok(mut state) = self.mailbox.state.lock() else {
            return (Vec::new(), None);
        };
        let mut notices =
            Vec::with_capacity(maximum_notices.min(MAX_EXTENSION_UI_APPLIED_NOTICES_V1));
        for _ in 0..maximum_notices.min(MAX_EXTENSION_UI_APPLIED_NOTICES_V1) {
            let Some(notice) = state.applied.pop_front() else {
                break;
            };
            notices.push(notice);
        }
        let full_refresh_at = state
            .applied_full_refresh_required
            .then_some(state.applied_full_refresh_at)
            .flatten();
        state.applied_full_refresh_required = false;
        state.applied_full_refresh_at = None;
        (notices, full_refresh_at)
    }

    #[must_use]
    pub fn runtime(&self) -> &Arc<ExtensionJobRuntimeV1> {
        &self.runtime
    }
}

/// UI pump errors are path-free and never enter extension code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExtensionJobUiPumpErrorV1 {
    WrongUiThread,
}

/// GPUI-thread-owned post-apply invalidation pump. Ready accepted batches are
/// deliberately exposed separately through [`Self::take_ready`], so task 5's
/// model projection can commit values before this pump emits a redraw.
#[derive(Debug)]
pub struct ExtensionJobUiPumpV1 {
    inbox: ExtensionJobUiInboxV1,
    invalidations: UiInvalidationBatcherV1,
}

impl ExtensionJobUiPumpV1 {
    #[must_use]
    pub fn new(inbox: ExtensionJobUiInboxV1, config: UiInvalidationBatcherConfigV1) -> Self {
        Self {
            inbox,
            invalidations: UiInvalidationBatcherV1::new(config),
        }
    }

    /// Returns runtime-owned ready signals for the UI model to drain and
    /// atomically apply. This method does not fabricate item identities.
    ///
    /// # Errors
    ///
    /// Returns [`ExtensionJobUiPumpErrorV1::WrongUiThread`] when a non-UI
    /// caller attempts to consume the unique inbox.
    pub fn take_ready(
        &mut self,
        maximum_signals: usize,
    ) -> Result<ExtensionJobUiReadyDrainV1, ExtensionJobUiPumpErrorV1> {
        self.ensure_ui_thread()?;
        Ok(self.inbox.take_ready(maximum_signals))
    }

    /// Moves only post-apply metadata into the 16--50 ms batcher. It never
    /// redraws synchronously and it never touches extension-provided rows.
    ///
    /// # Errors
    ///
    /// Returns [`ExtensionJobUiPumpErrorV1::WrongUiThread`] off the owner
    /// thread.
    pub fn poll_applied(
        &mut self,
        maximum_notices: usize,
    ) -> Result<usize, ExtensionJobUiPumpErrorV1> {
        self.ensure_ui_thread()?;
        let (notices, full_refresh_at) = self.inbox.take_applied(maximum_notices);
        let accepted = notices.len();
        for notice in notices {
            self.invalidations.record_applied_metadata(
                &notice.package_id,
                notice.interface_id,
                notice.job,
                notice.item_generation,
                notice.location_generation,
                notice.source_generation,
                notice.item_count,
                notice.applied_at,
            );
        }
        // The mailbox timestamp is captured before its mutex is acquired, so
        // concurrent producers may enqueue a newer fact before an older one.
        // Record all facts with their original times; the batcher retains the
        // minimum and therefore never slides the 16--50 ms deadline later.
        if let Some(applied_at) = full_refresh_at {
            self.invalidations.record_full_refresh_at(applied_at);
        }
        Ok(accepted)
    }

    ///
    /// # Errors
    ///
    /// Returns [`ExtensionJobUiPumpErrorV1::WrongUiThread`] off the owner
    /// thread.
    pub fn next_deadline(&self) -> Result<Option<Instant>, ExtensionJobUiPumpErrorV1> {
        self.ensure_ui_thread()?;
        Ok(self.invalidations.next_deadline())
    }

    ///
    /// # Errors
    ///
    /// Returns [`ExtensionJobUiPumpErrorV1::WrongUiThread`] off the owner
    /// thread.
    pub fn drain_due(
        &mut self,
        now: Instant,
    ) -> Result<Option<UiInvalidationBatchV1>, ExtensionJobUiPumpErrorV1> {
        self.ensure_ui_thread()?;
        Ok(self
            .invalidations
            .drain_due_with_current(now, |job, item, location, source| {
                self.inbox
                    .runtime
                    .is_result_generation_current(job, item, location, source)
            }))
    }

    fn ensure_ui_thread(&self) -> Result<(), ExtensionJobUiPumpErrorV1> {
        self.invalidations
            .is_on_owner_thread()
            .then_some(())
            .ok_or(ExtensionJobUiPumpErrorV1::WrongUiThread)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::Arc,
        time::{Duration, Instant},
    };

    use super::*;
    use crate::ExtensionResultBufferConfigV1;
    use explorer_extension_api::IdNamespaceV1;

    fn runtime() -> Arc<ExtensionJobRuntimeV1> {
        Arc::new(ExtensionJobRuntimeV1::new(
            ExtensionResultBufferConfigV1::try_new(8, 8, 8, 8, 8, 32, 32, 32, 4096, 4096, 4096)
                .expect("bounded config"),
        ))
    }

    fn signal(value: u64) -> ExtensionJobUiReadySignalV1 {
        ExtensionJobUiReadySignalV1::from_runtime(
            JobHandleV1::from_host([1; 16], value + 1),
            1,
            1,
            1,
        )
    }

    #[test]
    fn ready_overflow_remains_sticky_until_the_model_acknowledges_a_complete_scan() {
        let runtime = runtime();
        let (ingress, mut inbox) = ExtensionJobUiIngressV1::new_pair(runtime);
        for value in 0..=u64::try_from(MAX_EXTENSION_UI_READY_SIGNALS_V1).expect("test bound") {
            let _ = ingress.signal_ready(signal(value));
        }
        assert!(inbox.take_ready(0).signals.is_empty());
        let observed = inbox.take_ready(1);
        assert!(observed.rescan_required);
        assert!(inbox.take_ready(1).rescan_required);
        inbox.acknowledge_ready_rescan(observed.rescan_epoch.expect("overflow epoch"));
        assert!(!inbox.take_ready(1).rescan_required);
    }

    #[test]
    fn later_overflow_survives_an_acknowledgement_for_an_older_runtime_snapshot() {
        let runtime = runtime();
        let (ingress, mut inbox) = ExtensionJobUiIngressV1::new_pair(runtime);
        for value in 0..=u64::try_from(MAX_EXTENSION_UI_READY_SIGNALS_V1).expect("test bound") {
            let _ = ingress.signal_ready(signal(value));
        }
        let first = inbox.take_ready(1);
        let first_epoch = first.rescan_epoch.expect("first overflow epoch");
        // Refill the one removed slot, then force a second overflow after the
        // model's snapshot but before it acknowledges that snapshot.
        assert_eq!(
            ingress.signal_ready(signal(8_000)),
            ExtensionJobUiSignalOutcomeV1::Queued
        );
        assert_eq!(
            ingress.signal_ready(signal(8_001)),
            ExtensionJobUiSignalOutcomeV1::RescanRequired
        );
        inbox.acknowledge_ready_rescan(first_epoch);
        let second = inbox.take_ready(1);
        assert!(second.rescan_required);
        assert!(second.rescan_epoch.is_some_and(|epoch| epoch > first_epoch));
    }

    #[test]
    fn ingress_deduplicates_signals_without_granting_consumer_authority() {
        let runtime = runtime();
        let (ingress, mut inbox) = ExtensionJobUiIngressV1::new_pair(runtime);
        assert_eq!(
            ingress.signal_ready(signal(1)),
            ExtensionJobUiSignalOutcomeV1::Queued
        );
        assert_eq!(
            ingress.signal_ready(signal(1)),
            ExtensionJobUiSignalOutcomeV1::AlreadyQueued
        );
        assert_eq!(inbox.take_ready(8).signals, vec![signal(1)]);
    }

    #[test]
    fn host_close_rejects_late_signals_and_drops_pending_non_authoritative_work() {
        let runtime = runtime();
        let (ingress, mut inbox) = ExtensionJobUiIngressV1::new_pair(runtime);
        assert_eq!(
            ingress.signal_ready(signal(1)),
            ExtensionJobUiSignalOutcomeV1::Queued
        );
        ingress.close();
        assert_eq!(
            ingress.signal_ready(signal(2)),
            ExtensionJobUiSignalOutcomeV1::Closed
        );
        assert!(inbox.take_ready(8).signals.is_empty());
    }

    #[test]
    fn delayed_ui_poll_preserves_the_original_post_apply_deadline() {
        let runtime = runtime();
        let (_ingress, inbox) = ExtensionJobUiIngressV1::new_pair(runtime);
        let applied_at = Instant::now();
        inbox
            .mailbox
            .state
            .lock()
            .expect("mailbox lock")
            .applied
            .push_back(AppliedInvalidationNoticeV1 {
                package_id: "package".to_owned(),
                interface_id: StableIdV1::new(IdNamespaceV1::new(1, 1), 1),
                job: JobHandleV1::from_host([1; 16], 1),
                item_generation: 1,
                location_generation: 1,
                source_generation: 1,
                item_count: 1,
                applied_at,
            });
        let mut pump = ExtensionJobUiPumpV1::new(
            inbox,
            UiInvalidationBatcherConfigV1::try_new(Duration::from_millis(50), 8)
                .expect("valid config"),
        );
        assert_eq!(pump.poll_applied(8), Ok(1));
        assert_eq!(
            pump.next_deadline(),
            Ok(Some(applied_at + Duration::from_millis(50)))
        );
    }

    #[test]
    fn applied_overflow_never_moves_deadline_past_the_oldest_committed_fact() {
        let runtime = runtime();
        let (_ingress, inbox) = ExtensionJobUiIngressV1::new_pair(runtime);
        let oldest = Instant::now();
        let newer_overflow = oldest + Duration::from_millis(10);
        {
            let mut state = inbox.mailbox.state.lock().expect("mailbox lock");
            state.applied.push_back(AppliedInvalidationNoticeV1 {
                package_id: "package".to_owned(),
                interface_id: StableIdV1::new(IdNamespaceV1::new(1, 1), 1),
                job: JobHandleV1::from_host([1; 16], 1),
                item_generation: 1,
                location_generation: 1,
                source_generation: 1,
                item_count: 1,
                applied_at: oldest,
            });
            // This represents the first notice dropped after a full mailbox.
            state.applied_full_refresh_required = true;
            state.applied_full_refresh_at = Some(newer_overflow);
        }
        let mut pump = ExtensionJobUiPumpV1::new(
            inbox,
            UiInvalidationBatcherConfigV1::try_new(Duration::from_millis(50), 8)
                .expect("valid config"),
        );
        assert_eq!(pump.poll_applied(8), Ok(1));
        assert_eq!(
            pump.next_deadline(),
            Ok(Some(oldest + Duration::from_millis(50)))
        );
    }

    #[test]
    fn out_of_order_overflow_timestamp_still_uses_the_earliest_applied_fact() {
        let runtime = runtime();
        let (_ingress, inbox) = ExtensionJobUiIngressV1::new_pair(runtime);
        let oldest_overflow = Instant::now();
        let newer_notice = oldest_overflow + Duration::from_millis(10);
        {
            let mut state = inbox.mailbox.state.lock().expect("mailbox lock");
            state.applied.push_back(AppliedInvalidationNoticeV1 {
                package_id: "package".to_owned(),
                interface_id: StableIdV1::new(IdNamespaceV1::new(1, 1), 1),
                job: JobHandleV1::from_host([1; 16], 2),
                item_generation: 1,
                location_generation: 1,
                source_generation: 1,
                item_count: 1,
                applied_at: newer_notice,
            });
            // Timestamps are taken before the mailbox lock, so this older
            // dropped fact can be observed after the newer queued notice.
            state.applied_full_refresh_required = true;
            state.applied_full_refresh_at = Some(oldest_overflow);
        }
        let mut pump = ExtensionJobUiPumpV1::new(
            inbox,
            UiInvalidationBatcherConfigV1::try_new(Duration::from_millis(50), 8)
                .expect("valid config"),
        );
        assert_eq!(pump.poll_applied(8), Ok(1));
        assert_eq!(
            pump.next_deadline(),
            Ok(Some(oldest_overflow + Duration::from_millis(50)))
        );
    }
}
