//! Deterministic policy and state models for foreground-gated MFT durability.

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;

pub(crate) const PERSISTENCE_INTERVAL: Duration = Duration::from_secs(10 * 60);

const LIFECYCLE_OPEN: u8 = 0;
const LIFECYCLE_INVOKING: u8 = 1;
const LIFECYCLE_CLOSED: u8 = 2;

/// Linearizes each irreversible durability invocation against SCM stop.
/// Long preparation work does not hold the gate and observes `is_open` for
/// cancellation; only the actual BEGIN/COMMIT/checkpoint/rename/delete call
/// owns `INVOKING`.
#[derive(Debug)]
pub(crate) struct LifecycleBarrierV1 {
    state: AtomicU8,
}

impl LifecycleBarrierV1 {
    pub(crate) const fn new() -> Self {
        Self {
            state: AtomicU8::new(LIFECYCLE_OPEN),
        }
    }

    pub(crate) fn is_open(&self) -> bool {
        self.state.load(Ordering::Acquire) != LIFECYCLE_CLOSED
    }

    pub(crate) fn close(&self) {
        self.state.store(LIFECYCLE_CLOSED, Ordering::Release);
    }

    pub(crate) fn invoke<T>(
        &self,
        operation: impl FnOnce() -> Result<T, String>,
    ) -> Result<T, String> {
        self.state
            .compare_exchange(
                LIFECYCLE_OPEN,
                LIFECYCLE_INVOKING,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .map_err(|state| {
                if state == LIFECYCLE_CLOSED {
                    "MFT lifecycle is closed".to_owned()
                } else {
                    "MFT lifecycle invocation is busy".to_owned()
                }
            })?;
        let result = operation();
        let _ = self.state.compare_exchange(
            LIFECYCLE_INVOKING,
            LIFECYCLE_OPEN,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
        result
    }
}

/// Monotonic time supplied by the service runtime or a deterministic test.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct MonotonicMillis(pub(crate) u64);

impl MonotonicMillis {
    const fn elapsed_since(self, earlier: Self) -> Duration {
        Duration::from_millis(self.0.saturating_sub(earlier.0))
    }

    fn saturating_add(self, duration: Duration) -> Self {
        Self(
            self.0
                .saturating_add(duration.as_millis().min(u128::from(u64::MAX)) as u64),
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct JournalCursorV1 {
    pub(crate) journal_id: u64,
    pub(crate) next_usn: i64,
    pub(crate) generation: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RecoveryReasonV1 {
    PendingOverflow,
    AmbiguousTopology,
    JournalReplaced,
    CursorUnavailable,
    CorruptStore,
    IdentityMismatch,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MigrationStateV1 {
    None,
    LegacyPending,
    QuarantinePending,
    RebuildRequired(RecoveryReasonV1),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PendingBatchV1<T> {
    pub(crate) start: JournalCursorV1,
    pub(crate) observed: JournalCursorV1,
    pub(crate) changes: Vec<T>,
    pub(crate) encoded_bytes: usize,
}

impl<T> PendingBatchV1<T> {
    pub(crate) fn capture(
        start: JournalCursorV1,
        observed: JournalCursorV1,
        changes: Vec<T>,
        encoded_bytes: usize,
    ) -> Result<Self, &'static str> {
        if start.journal_id != observed.journal_id
            || observed.next_usn < start.next_usn
            || observed.generation < start.generation
        {
            return Err("pending MFT batch cursor is not contiguous");
        }
        Ok(Self {
            start,
            observed,
            changes,
            encoded_bytes,
        })
    }
}

/// Atomically separates the currently observed coalesced set from later
/// arrivals. The captured cursor consequently covers only captured values.
pub(crate) fn capture_coalesced_batch<K, V>(
    pending: &mut HashMap<K, V>,
    start: JournalCursorV1,
    observed: JournalCursorV1,
    encoded_bytes: usize,
) -> Result<PendingBatchV1<V>, &'static str>
where
    K: Eq + Hash,
{
    let captured = std::mem::take(pending).into_values().collect();
    PendingBatchV1::capture(start, observed, captured, encoded_bytes)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PersistenceDecisionV1 {
    Idle,
    WaitForInterval,
    WaitForFocus,
    BeginAttempt,
    Stopping,
}

/// Two-clock scheduler: failures do not advance durable success, but every
/// disk-write attempt is throttled for the full ten-minute interval.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PersistenceScheduleV1 {
    startup: MonotonicMillis,
    last_success: Option<MonotonicMillis>,
    last_attempt: Option<MonotonicMillis>,
    initial_recovery_expedited: bool,
    stopping: bool,
}

impl PersistenceScheduleV1 {
    pub(crate) const fn new(startup: MonotonicMillis) -> Self {
        Self {
            startup,
            last_success: None,
            last_attempt: None,
            initial_recovery_expedited: false,
            stopping: false,
        }
    }

    /// Makes the first recovery attempt immediately eligible when an explicit
    /// metadata query is waiting for an exact index. Once an attempt has been
    /// recorded, the normal ten-minute failure throttle remains authoritative.
    pub(crate) fn expedite_initial_recovery(&mut self, _now: MonotonicMillis) {
        if self.last_attempt.is_none() && self.last_success.is_none() {
            self.initial_recovery_expedited = true;
        }
    }

    pub(crate) fn decision(
        &self,
        now: MonotonicMillis,
        has_pending: bool,
        focused: bool,
    ) -> PersistenceDecisionV1 {
        if self.stopping {
            return PersistenceDecisionV1::Stopping;
        }
        if !has_pending {
            return PersistenceDecisionV1::Idle;
        }
        let success_reference = self.last_success.unwrap_or(self.startup);
        let success_due = self.initial_recovery_expedited
            || now.elapsed_since(success_reference) >= PERSISTENCE_INTERVAL;
        let attempt_due = self
            .last_attempt
            .is_none_or(|attempt| now.elapsed_since(attempt) >= PERSISTENCE_INTERVAL);
        if !success_due || !attempt_due {
            return PersistenceDecisionV1::WaitForInterval;
        }
        if !focused {
            return PersistenceDecisionV1::WaitForFocus;
        }
        PersistenceDecisionV1::BeginAttempt
    }

    /// Must be called immediately before the SQLite `BEGIN` boundary.
    pub(crate) fn record_attempt(&mut self, now: MonotonicMillis) -> Result<(), &'static str> {
        if self.stopping {
            return Err("MFT persistence is stopping");
        }
        self.initial_recovery_expedited = false;
        self.last_attempt = Some(now);
        Ok(())
    }

    pub(crate) fn record_success(&mut self, now: MonotonicMillis) {
        self.last_success = Some(now);
    }

    pub(crate) fn inhibit_for_stop(&mut self) {
        self.stopping = true;
    }

    pub(crate) const fn last_success(&self) -> Option<MonotonicMillis> {
        self.last_success
    }

    #[cfg(test)]
    const fn clocks(&self) -> (Option<MonotonicMillis>, Option<MonotonicMillis>) {
        (self.last_success, self.last_attempt)
    }
}

pub(crate) type LeaseIdV1 = u128;
pub(crate) type LeaseOwnerV1 = u64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FocusLeaseV1 {
    owner: LeaseOwnerV1,
    expires_at: MonotonicMillis,
}

#[derive(Debug, Default)]
pub(crate) struct FocusLeaseRegistryV1 {
    leases: HashMap<LeaseIdV1, FocusLeaseV1>,
}

impl FocusLeaseRegistryV1 {
    pub(crate) fn acquire_or_renew(
        &mut self,
        id: LeaseIdV1,
        owner: LeaseOwnerV1,
        now: MonotonicMillis,
        ttl: Duration,
    ) -> Result<(), &'static str> {
        if ttl.is_zero() {
            return Err("focus lease TTL must be positive");
        }
        if self
            .leases
            .get(&id)
            .is_some_and(|lease| lease.owner != owner)
        {
            return Err("focus lease owner mismatch");
        }
        self.leases.insert(
            id,
            FocusLeaseV1 {
                owner,
                expires_at: now.saturating_add(ttl),
            },
        );
        Ok(())
    }

    pub(crate) fn release(&mut self, id: LeaseIdV1, owner: LeaseOwnerV1) -> bool {
        if self
            .leases
            .get(&id)
            .is_some_and(|lease| lease.owner == owner)
        {
            self.leases.remove(&id);
            true
        } else {
            false
        }
    }

    pub(crate) fn disconnect(&mut self, owner: LeaseOwnerV1) {
        self.leases.retain(|_, lease| lease.owner != owner);
    }

    pub(crate) fn clear(&mut self) {
        self.leases.clear();
    }

    pub(crate) fn expire(&mut self, now: MonotonicMillis) {
        self.leases.retain(|_, lease| lease.expires_at > now);
    }

    pub(crate) fn any_focused(&mut self, now: MonotonicMillis) -> bool {
        self.expire(now);
        !self.leases.is_empty()
    }

    pub(crate) fn contains_active(&mut self, id: LeaseIdV1, now: MonotonicMillis) -> bool {
        self.expire(now);
        self.leases.contains_key(&id)
    }

    pub(crate) fn active_count(&mut self, now: MonotonicMillis) -> usize {
        self.expire(now);
        self.leases.len()
    }

    pub(crate) fn expiry_remaining(&mut self, now: MonotonicMillis) -> Duration {
        self.expire(now);
        self.leases
            .values()
            .map(|lease| lease.expires_at.elapsed_since(now))
            .max()
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const START: MonotonicMillis = MonotonicMillis(1_000);
    const FOCUS_TTL: Duration = Duration::from_secs(15);

    fn at(offset: Duration) -> MonotonicMillis {
        START.saturating_add(offset)
    }

    #[test]
    fn scheduler_requires_interval_and_focus_at_exact_boundary() {
        let schedule = PersistenceScheduleV1::new(START);
        assert_eq!(
            schedule.decision(
                at(PERSISTENCE_INTERVAL - Duration::from_millis(1)),
                true,
                true
            ),
            PersistenceDecisionV1::WaitForInterval
        );
        assert_eq!(
            schedule.decision(at(PERSISTENCE_INTERVAL), true, false),
            PersistenceDecisionV1::WaitForFocus
        );
        assert_eq!(
            schedule.decision(at(PERSISTENCE_INTERVAL), true, true),
            PersistenceDecisionV1::BeginAttempt
        );
    }

    #[test]
    fn explicit_query_can_expedite_only_the_initial_recovery_attempt() {
        let now = at(Duration::from_secs(2));
        let mut schedule = PersistenceScheduleV1::new(START);
        assert_eq!(
            schedule.decision(now, true, true),
            PersistenceDecisionV1::WaitForInterval
        );
        schedule.expedite_initial_recovery(now);
        assert_eq!(
            schedule.decision(now, true, true),
            PersistenceDecisionV1::BeginAttempt
        );
        schedule.record_attempt(now).unwrap();
        schedule.expedite_initial_recovery(at(Duration::from_secs(3)));
        assert_eq!(
            schedule.decision(at(Duration::from_secs(3)), true, true),
            PersistenceDecisionV1::WaitForInterval
        );
    }

    #[test]
    fn failed_attempt_is_throttled_without_advancing_success_clock() {
        let mut schedule = PersistenceScheduleV1::new(START);
        let due = at(PERSISTENCE_INTERVAL);
        schedule.record_attempt(due).unwrap();
        assert_eq!(schedule.clocks(), (None, Some(due)));
        assert_eq!(
            schedule.decision(due.saturating_add(Duration::from_secs(1)), true, true),
            PersistenceDecisionV1::WaitForInterval
        );
        assert_eq!(
            schedule.decision(due.saturating_add(PERSISTENCE_INTERVAL), true, true),
            PersistenceDecisionV1::BeginAttempt
        );
    }

    #[test]
    fn success_restarts_both_effective_deadlines_and_stop_inhibits_attempts() {
        let mut schedule = PersistenceScheduleV1::new(START);
        let due = at(PERSISTENCE_INTERVAL);
        schedule.record_attempt(due).unwrap();
        let committed = due.saturating_add(Duration::from_millis(250));
        schedule.record_success(committed);
        assert_eq!(
            schedule.decision(due.saturating_add(Duration::from_secs(30)), true, true),
            PersistenceDecisionV1::WaitForInterval
        );
        assert_eq!(
            schedule.decision(due.saturating_add(PERSISTENCE_INTERVAL), true, true),
            PersistenceDecisionV1::WaitForInterval
        );
        assert_eq!(
            schedule.decision(committed.saturating_add(PERSISTENCE_INTERVAL), true, true),
            PersistenceDecisionV1::BeginAttempt
        );
        schedule.inhibit_for_stop();
        assert_eq!(
            schedule.decision(due.saturating_add(PERSISTENCE_INTERVAL), true, true),
            PersistenceDecisionV1::Stopping
        );
        assert!(schedule.record_attempt(due).is_err());
    }

    #[test]
    fn focus_leases_aggregate_renew_release_disconnect_and_expire() {
        let mut leases = FocusLeaseRegistryV1::default();
        leases.acquire_or_renew(1, 10, START, FOCUS_TTL).unwrap();
        leases.acquire_or_renew(2, 20, START, FOCUS_TTL).unwrap();
        assert_eq!(leases.active_count(START), 2);
        assert!(leases.release(1, 10));
        assert!(leases.any_focused(START));
        leases
            .acquire_or_renew(2, 20, at(Duration::from_secs(10)), FOCUS_TTL)
            .unwrap();
        assert!(leases.any_focused(at(Duration::from_secs(16))));
        leases.disconnect(20);
        assert!(!leases.any_focused(at(Duration::from_secs(16))));

        leases.acquire_or_renew(3, 30, START, FOCUS_TTL).unwrap();
        assert!(!leases.any_focused(at(FOCUS_TTL)));
    }

    #[test]
    fn lease_id_cannot_be_stolen_by_another_owner() {
        let mut leases = FocusLeaseRegistryV1::default();
        leases.acquire_or_renew(7, 70, START, FOCUS_TTL).unwrap();
        assert!(leases.acquire_or_renew(7, 71, START, FOCUS_TTL).is_err());
        assert!(!leases.release(7, 71));
        assert_eq!(leases.active_count(START), 1);
    }

    #[test]
    fn session_change_clears_all_connection_bound_leases() {
        let mut leases = FocusLeaseRegistryV1::default();
        leases.acquire_or_renew(1, 10, START, FOCUS_TTL).unwrap();
        leases.acquire_or_renew(2, 20, START, FOCUS_TTL).unwrap();
        leases.clear();
        assert_eq!(leases.active_count(START), 0);
    }

    #[test]
    fn captured_batch_rejects_cursor_regression_or_identity_change() {
        let start = JournalCursorV1 {
            journal_id: 4,
            next_usn: 10,
            generation: 2,
        };
        let observed = JournalCursorV1 {
            journal_id: 4,
            next_usn: 20,
            generation: 3,
        };
        assert!(PendingBatchV1::capture(start, observed, vec![1], 8).is_ok());
        assert!(
            PendingBatchV1::<u8>::capture(
                start,
                JournalCursorV1 {
                    next_usn: 9,
                    ..observed
                },
                vec![],
                0
            )
            .is_err()
        );
        assert!(
            PendingBatchV1::<u8>::capture(
                start,
                JournalCursorV1 {
                    journal_id: 5,
                    ..observed
                },
                vec![],
                0
            )
            .is_err()
        );
    }

    #[test]
    fn captured_batch_cannot_cover_events_arriving_after_snapshot() {
        let start = JournalCursorV1 {
            journal_id: 7,
            next_usn: 10,
            generation: 0,
        };
        let observed = JournalCursorV1 {
            journal_id: 7,
            next_usn: 20,
            generation: 1,
        };
        let mut pending = HashMap::from([(1_u64, "first")]);
        let captured = capture_coalesced_batch(&mut pending, start, observed, 5).unwrap();
        pending.insert(2, "later");
        assert_eq!(captured.changes, vec!["first"]);
        assert_eq!(captured.observed.next_usn, 20);
        assert_eq!(pending, HashMap::from([(2, "later")]));
    }

    #[test]
    fn lifecycle_close_linearizes_against_real_invocation_without_waiting() {
        use std::sync::{Arc, mpsc};
        let barrier = Arc::new(LifecycleBarrierV1::new());
        let (invoked_tx, invoked_rx) = mpsc::channel();
        let (finish_tx, finish_rx) = mpsc::channel();
        let worker_barrier = Arc::clone(&barrier);
        let worker = std::thread::spawn(move || {
            worker_barrier.invoke(|| {
                invoked_tx.send(()).unwrap();
                finish_rx.recv().unwrap();
                Ok(())
            })
        });
        invoked_rx.recv().unwrap();
        barrier.close();
        assert!(!barrier.is_open());
        assert!(barrier.invoke(|| Ok(())).is_err());
        finish_tx.send(()).unwrap();
        assert!(worker.join().unwrap().is_ok());
        assert!(!barrier.is_open());
    }
}
