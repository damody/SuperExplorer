#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )
)]
//! Bounded background-work policy. Shell apartment work does not belong here.
#![allow(
    clippy::must_use_candidate,
    reason = "public constructors and queue observations do not own resources that require consumption"
)]

use std::collections::VecDeque;

use crossbeam_channel::{Receiver, Sender, TryRecvError, TrySendError};
use explorer_model::TabId;

mod preview;
mod qos;
mod thumbnail;
pub use preview::{PreviewCoordinator, PreviewCoordinatorAction};
pub use qos::{
    DegradationLevel, DegradationPolicyConfig, DegradationTransition, FrameDrain, FrameDrainBudget,
    FrameDrainLimit, InteractionFirstPolicy, InteractionFirstQos, InteractionFirstQosConfig,
    PressureSample, QosObservationSnapshot, QosObservations, QosWorkClass,
};
pub use thumbnail::{
    CacheInsertOutcome, ThumbnailCacheStats, ThumbnailMemoryCache, ThumbnailScheduleOutcome,
    ThumbnailScheduler, ThumbnailSchedulerStats,
};

/// Work priority from most interactive to least interactive.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum JobPriority {
    VisibleViewport,
    CurrentDirectory,
    Prefetch,
    Maintenance,
}

impl JobPriority {
    const COUNT: usize = 4;

    const fn index(self) -> usize {
        self as usize
    }

    /// Assigns active viewport work ahead of background-tab enumeration.
    pub fn for_directory(
        active_tab: TabId,
        candidate_tab: TabId,
        first_viewport_ready: bool,
    ) -> Self {
        if active_tab == candidate_tab {
            if first_viewport_ready {
                Self::CurrentDirectory
            } else {
                Self::VisibleViewport
            }
        } else {
            Self::Prefetch
        }
    }
}

/// Initial bounded scheduler configuration used by the composition root.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JobSchedulerConfig {
    pub maximum_queued_jobs: usize,
}

impl Default for JobSchedulerConfig {
    fn default() -> Self {
        Self {
            maximum_queued_jobs: 1_024,
        }
    }
}

/// Creates a bounded, non-blocking endpoint for UI-to-service messages.
///
pub fn bounded_endpoint<T>(capacity: usize) -> (BoundedSender<T>, BoundedReceiver<T>) {
    let (sender, receiver) = crossbeam_channel::bounded(capacity.max(1));
    (BoundedSender(sender), BoundedReceiver(receiver))
}

/// Cloneable sender that exposes only non-blocking dispatch.
#[derive(Clone, Debug)]
pub struct BoundedSender<T>(Sender<T>);

impl<T> BoundedSender<T> {
    /// Attempts dispatch without ever waiting on queue capacity.
    ///
    /// # Errors
    ///
    /// Returns the original message with an explicit overload or shutdown reason.
    pub fn try_send(&self, message: T) -> Result<(), DispatchError<T>> {
        self.0.try_send(message).map_err(|error| match error {
            TrySendError::Full(message) => DispatchError::Overloaded(message),
            TrySendError::Disconnected(message) => DispatchError::Disconnected(message),
        })
    }
}

/// Single service-side receiver for a bounded endpoint.
#[derive(Debug)]
pub struct BoundedReceiver<T>(Receiver<T>);

impl<T> BoundedReceiver<T> {
    /// Receives one queued message without blocking.
    ///
    /// # Errors
    ///
    /// Distinguishes an empty live queue from a disconnected endpoint.
    pub fn try_recv(&self) -> Result<T, ReceiveError> {
        self.0.try_recv().map_err(|error| match error {
            TryRecvError::Empty => ReceiveError::Empty,
            TryRecvError::Disconnected => ReceiveError::Disconnected,
        })
    }
}

/// Failed non-blocking dispatch with ownership returned to the caller.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DispatchError<T> {
    Overloaded(T),
    Disconnected(T),
}

/// Non-blocking receive state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReceiveError {
    Empty,
    Disconnected,
}

/// Bounded deterministic priority queue used by non-Shell background workers.
#[derive(Debug)]
pub struct BoundedPriorityQueue<T> {
    lanes: [VecDeque<T>; JobPriority::COUNT],
    len: usize,
    capacity: usize,
}

impl<T> BoundedPriorityQueue<T> {
    /// Creates a queue with a strict total capacity.
    ///
    pub fn new(capacity: usize) -> Self {
        Self {
            lanes: std::array::from_fn(|_| VecDeque::new()),
            len: 0,
            capacity: capacity.max(1),
        }
    }

    /// Adds work without blocking.
    ///
    /// # Errors
    ///
    /// Returns the original job when the total bounded capacity is full.
    pub fn try_push(&mut self, priority: JobPriority, job: T) -> Result<(), PriorityQueueFull<T>> {
        if self.len == self.capacity {
            return Err(PriorityQueueFull(job));
        }
        self.lanes[priority.index()].push_back(job);
        self.len += 1;
        Ok(())
    }

    /// Pops the oldest job from the highest non-empty priority lane.
    pub fn pop_next(&mut self) -> Option<T> {
        for lane in &mut self.lanes {
            if let Some(job) = lane.pop_front() {
                self.len -= 1;
                return Some(job);
            }
        }
        None
    }

    /// Returns the number of queued jobs across all priorities.
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Returns whether no work remains.
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }
}

/// Bounded priority queue overload with job ownership returned.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PriorityQueueFull<T>(pub T);

#[cfg(test)]
mod tests {
    use explorer_model::TabId;

    use super::{
        BoundedPriorityQueue, DispatchError, JobPriority, PriorityQueueFull, ReceiveError,
        bounded_endpoint,
    };

    #[test]
    fn full_queue_returns_explicit_overload_without_blocking_caller() {
        let (sender, receiver) = bounded_endpoint(1);
        assert_eq!(sender.try_send("first"), Ok(()));
        assert_eq!(
            sender.try_send("necessary-command"),
            Err(DispatchError::Overloaded("necessary-command"))
        );
        assert_eq!(receiver.try_recv(), Ok("first"));
        assert_eq!(receiver.try_recv(), Err(ReceiveError::Empty));
    }

    #[test]
    fn endpoint_reports_shutdown_and_returns_unsent_message() {
        let (sender, receiver) = bounded_endpoint(1);
        drop(receiver);
        assert_eq!(
            sender.try_send("command"),
            Err(DispatchError::Disconnected("command"))
        );
    }

    #[test]
    fn active_viewport_runs_before_background_without_starving_completion() {
        let active = TabId::new();
        let background = TabId::new();
        let mut queue = BoundedPriorityQueue::new(3);
        queue
            .try_push(
                JobPriority::for_directory(active, background, false),
                "background-enumeration",
            )
            .expect("queue background");
        queue
            .try_push(
                JobPriority::for_directory(active, active, false),
                "active-first-viewport",
            )
            .expect("queue viewport");
        queue
            .try_push(JobPriority::Maintenance, "maintenance")
            .expect("queue maintenance");
        assert_eq!(queue.pop_next(), Some("active-first-viewport"));
        assert_eq!(queue.pop_next(), Some("background-enumeration"));
        assert_eq!(queue.pop_next(), Some("maintenance"));
        assert!(queue.is_empty());
    }

    #[test]
    fn priority_queue_returns_overload_without_dropping_job() {
        let mut queue = BoundedPriorityQueue::new(1);
        assert_eq!(queue.try_push(JobPriority::Prefetch, "first"), Ok(()));
        assert_eq!(
            queue.try_push(JobPriority::VisibleViewport, "second"),
            Err(PriorityQueueFull("second"))
        );
    }

    #[test]
    fn zero_capacity_is_safely_promoted_to_one_slot() {
        let (sender, receiver) = bounded_endpoint(0);
        assert_eq!(sender.try_send("one"), Ok(()));
        assert!(matches!(
            sender.try_send("two"),
            Err(DispatchError::Overloaded("two"))
        ));
        assert_eq!(receiver.try_recv(), Ok("one"));

        let mut queue = BoundedPriorityQueue::new(0);
        assert_eq!(queue.try_push(JobPriority::Maintenance, "one"), Ok(()));
        assert_eq!(
            queue.try_push(JobPriority::Maintenance, "two"),
            Err(PriorityQueueFull("two"))
        );
    }
}
