//! Executor-independent timing futures and deterministic debounce/throttle state.

use std::{
    collections::{BTreeMap, HashMap},
    future::Future,
    hash::Hash,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll, Waker},
};

use thiserror::Error;

use crate::{AutomationFuture, TimerHost};

/// Deterministic clock and timer source used by runtime tests and adapters.
#[derive(Clone, Debug, Default)]
pub struct ManualTimer {
    state: Arc<Mutex<TimerState>>,
}

#[derive(Debug, Default)]
struct TimerState {
    now_ms: u64,
    next_id: u64,
    entries: BTreeMap<u64, TimerEntry>,
}

#[derive(Debug)]
struct TimerEntry {
    deadline_ms: u64,
    waker: Option<Waker>,
}

impl ManualTimer {
    /// Creates a deterministic timer at an explicit instant.
    #[must_use]
    pub fn at(now_ms: u64) -> Self {
        Self {
            state: Arc::new(Mutex::new(TimerState {
                now_ms,
                ..TimerState::default()
            })),
        }
    }

    /// Returns a future that becomes ready after the supplied duration.
    #[must_use]
    pub fn sleep(&self, duration_ms: u64) -> Sleep {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let id = state.next_id;
        state.next_id = state.next_id.saturating_add(1);
        let deadline_ms = state.now_ms.saturating_add(duration_ms);
        state.entries.insert(
            id,
            TimerEntry {
                deadline_ms,
                waker: None,
            },
        );
        Sleep {
            timer: self.clone(),
            id,
            deadline_ms,
            complete: false,
        }
    }

    /// Alias used for delayed event delivery.
    #[must_use]
    pub fn delay(&self, duration_ms: u64) -> Sleep {
        self.sleep(duration_ms)
    }

    /// Wraps a future with a deterministic timeout.
    #[must_use]
    pub fn timeout<F>(&self, duration_ms: u64, future: F) -> Timeout<F>
    where
        F: Future,
    {
        Timeout {
            future,
            sleep: self.sleep(duration_ms),
        }
    }

    /// Advances virtual time and wakes every newly due future.
    #[must_use]
    pub fn advance(&self, duration_ms: u64) -> u64 {
        let wakers = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.now_ms = state.now_ms.saturating_add(duration_ms);
            let now_ms = state.now_ms;
            state
                .entries
                .values_mut()
                .filter(|entry| entry.deadline_ms <= now_ms)
                .filter_map(|entry| entry.waker.take())
                .collect::<Vec<_>>()
        };
        for waker in wakers {
            waker.wake();
        }
        self.now_ms()
    }

    /// Reads the deterministic instant.
    #[must_use]
    pub fn now_ms(&self) -> u64 {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .now_ms
    }
}

impl TimerHost for ManualTimer {
    fn now_ms(&self) -> u64 {
        Self::now_ms(self)
    }

    fn sleep(&self, duration_ms: u64) -> AutomationFuture<()> {
        let sleep = Self::sleep(self, duration_ms);
        Box::pin(async move {
            sleep.await;
            Ok(())
        })
    }
}

/// Non-blocking sleep driven by `ManualTimer`.
#[derive(Debug)]
pub struct Sleep {
    timer: ManualTimer,
    id: u64,
    deadline_ms: u64,
    complete: bool,
}

impl Future for Sleep {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        if self.complete {
            return Poll::Ready(());
        }
        let ready = {
            let mut state = self
                .timer
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if state.now_ms >= self.deadline_ms {
                state.entries.remove(&self.id);
                true
            } else {
                if let Some(entry) = state.entries.get_mut(&self.id) {
                    entry.waker = Some(context.waker().clone());
                }
                false
            }
        };
        if ready {
            self.complete = true;
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}

impl Drop for Sleep {
    fn drop(&mut self) {
        if !self.complete {
            self.timer
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .entries
                .remove(&self.id);
        }
    }
}

/// Future wrapper that returns an explicit timeout instead of blocking.
#[derive(Debug)]
pub struct Timeout<F> {
    future: F,
    sleep: Sleep,
}

impl<F> Future for Timeout<F>
where
    F: Future + Unpin,
{
    type Output = Result<F::Output, TimeoutElapsed>;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        if let Poll::Ready(output) = Pin::new(&mut this.future).poll(context) {
            return Poll::Ready(Ok(output));
        }
        if Pin::new(&mut this.sleep).poll(context).is_ready() {
            Poll::Ready(Err(TimeoutElapsed))
        } else {
            Poll::Pending
        }
    }
}

/// Deterministic timeout marker.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("automation operation timed out")]
pub struct TimeoutElapsed;

/// Replaces a key's pending value until its quiet-period deadline elapses.
#[derive(Clone, Debug, Default)]
pub struct Debouncer<K, V> {
    next_sequence: u64,
    pending: HashMap<K, Debounced<V>>,
}

#[derive(Clone, Debug)]
struct Debounced<V> {
    deadline_ms: u64,
    sequence: u64,
    value: V,
}

impl<K, V> Debouncer<K, V>
where
    K: Clone + Eq + Hash,
{
    /// Inserts or replaces a pending value for the key.
    pub fn submit(&mut self, key: K, value: V, now_ms: u64, delay_ms: u64) {
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.saturating_add(1);
        self.pending.insert(
            key,
            Debounced {
                deadline_ms: now_ms.saturating_add(delay_ms),
                sequence,
                value,
            },
        );
    }

    /// Drains due values in deterministic submission order.
    #[must_use]
    pub fn drain_due(&mut self, now_ms: u64) -> Vec<(K, V)> {
        let mut due = self
            .pending
            .iter()
            .filter(|(_, value)| value.deadline_ms <= now_ms)
            .map(|(key, value)| (value.sequence, key.clone()))
            .collect::<Vec<_>>();
        due.sort_by_key(|(sequence, _)| *sequence);
        due.into_iter()
            .filter_map(|(_, key)| self.pending.remove(&key).map(|value| (key, value.value)))
            .collect()
    }
}

/// Per-key fixed-window throttle.
#[derive(Clone, Debug, Default)]
pub struct Throttler<K> {
    last_accepted_ms: HashMap<K, u64>,
}

impl<K> Throttler<K>
where
    K: Clone + Eq + Hash,
{
    /// Returns true and records the instant when the key is outside its throttle window.
    pub fn try_accept(&mut self, key: K, now_ms: u64, interval_ms: u64) -> bool {
        let accepted = self
            .last_accepted_ms
            .get(&key)
            .is_none_or(|last| now_ms.saturating_sub(*last) >= interval_ms);
        if accepted {
            self.last_accepted_ms.insert(key, now_ms);
        }
        accepted
    }
}

#[cfg(test)]
mod tests {
    use std::{future::pending, task::Waker};

    use super::{Debouncer, ManualTimer, Throttler};

    #[test]
    fn sleep_and_timeout_follow_virtual_time() {
        let timer = ManualTimer::at(100);
        let mut sleep = Box::pin(timer.sleep(50));
        let waker = Waker::noop();
        let mut context = std::task::Context::from_waker(waker);
        assert!(sleep.as_mut().poll(&mut context).is_pending());
        assert_eq!(timer.advance(49), 149);
        assert!(sleep.as_mut().poll(&mut context).is_pending());
        assert_eq!(timer.advance(1), 150);
        assert!(sleep.as_mut().poll(&mut context).is_ready());

        let mut timeout = Box::pin(timer.timeout(10, pending::<()>()));
        assert!(timeout.as_mut().poll(&mut context).is_pending());
        let _ = timer.advance(10);
        assert!(timeout.as_mut().poll(&mut context).is_ready());
    }

    #[test]
    fn debounce_replaces_value_and_drains_in_submission_order() {
        let mut debounce = Debouncer::default();
        debounce.submit("a", 1, 0, 10);
        debounce.submit("b", 2, 1, 5);
        debounce.submit("a", 3, 2, 10);
        assert_eq!(debounce.drain_due(6), vec![("b", 2)]);
        assert_eq!(debounce.drain_due(12), vec![("a", 3)]);
    }

    #[test]
    fn throttle_accepts_at_most_once_per_window() {
        let mut throttle = Throttler::default();
        assert!(throttle.try_accept("mouse", 100, 10));
        assert!(!throttle.try_accept("mouse", 109, 10));
        assert!(throttle.try_accept("mouse", 110, 10));
    }
}
