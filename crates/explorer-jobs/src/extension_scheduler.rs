//! Synchronous, bounded admission for host-owned extension work.
//!
//! This module deliberately owns neither workers nor extension callbacks.  All
//! scheduler mutations return cancellation actions; the host must drop its
//! scheduler lock before calling [`ExtensionCancellationActionsV1::signal_all`].
//! Deadlines and cancellation are cooperative and never unload or interrupt a
//! running in-process extension callback.

use std::{
    collections::{HashMap, HashSet, VecDeque},
    time::Instant,
};

use explorer_common::RequestDeadline;
use explorer_model::CancellationToken;

use crate::JobPriority;

const QUEUE_CLASSES: usize = 2;
const PRIORITY_LANES: usize = 4;
/// Hard ceiling that bounds configuration arithmetic and scope-fence storage.
pub const MAX_EXTENSION_QUEUE_LIMIT_V1: usize = 65_536;

/// The host resource class required by an extension job.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ExtensionJobClassV1 {
    Cpu,
    Io,
}

impl ExtensionJobClassV1 {
    const fn index(self) -> usize {
        match self {
            Self::Cpu => 0,
            Self::Io => 1,
        }
    }
}

/// A validated package identifier used for host-internal scheduler accounting.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ExtensionPackageIdV1(String);

impl ExtensionPackageIdV1 {
    /// Wraps an identifier already validated by package admission.
    #[must_use]
    pub fn from_validated(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the validated package identifier.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Opaque scheduler identity for cancellation and terminal accounting.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ExtensionJobIdV1(u64);

/// Host-owned lifecycle scope for extension work.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ExtensionJobScopeV1 {
    pub package: ExtensionPackageIdV1,
    pub feature_id: String,
    pub lifecycle_epoch: u64,
}

impl ExtensionJobScopeV1 {
    /// Creates a scope from package admission and lifecycle-owned feature state.
    #[must_use]
    pub fn new(
        package: ExtensionPackageIdV1,
        feature_id: impl Into<String>,
        lifecycle_epoch: u64,
    ) -> Self {
        Self {
            package,
            feature_id: feature_id.into(),
            lifecycle_epoch,
        }
    }

    fn owner(&self) -> ExtensionScopeOwnerV1 {
        ExtensionScopeOwnerV1 {
            package: self.package.clone(),
            feature_id: self.feature_id.clone(),
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct ExtensionScopeOwnerV1 {
    package: ExtensionPackageIdV1,
    feature_id: String,
}

/// Independent bounds for one CPU or I/O queue.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExtensionQueueLimitsV1 {
    queued_global: usize,
    queued_per_package: usize,
    running_global: usize,
    running_per_package: usize,
}

impl ExtensionQueueLimitsV1 {
    /// Validates independent global and per-package limits for one class.
    ///
    /// # Errors
    ///
    /// Returns a typed error when a bound is zero, excessive, or inconsistent.
    pub fn try_new(
        maximum_queued_jobs: usize,
        maximum_queued_jobs_per_package: usize,
        maximum_running_jobs: usize,
        maximum_running_jobs_per_package: usize,
    ) -> Result<Self, ExtensionSchedulerConfigErrorV1> {
        for (field, value) in [
            (
                ExtensionSchedulerLimitV1::MaximumQueuedJobs,
                maximum_queued_jobs,
            ),
            (
                ExtensionSchedulerLimitV1::MaximumQueuedJobsPerPackage,
                maximum_queued_jobs_per_package,
            ),
            (
                ExtensionSchedulerLimitV1::MaximumRunningJobs,
                maximum_running_jobs,
            ),
            (
                ExtensionSchedulerLimitV1::MaximumRunningJobsPerPackage,
                maximum_running_jobs_per_package,
            ),
        ] {
            if value == 0 {
                return Err(ExtensionSchedulerConfigErrorV1::Zero { field });
            }
            if value > MAX_EXTENSION_QUEUE_LIMIT_V1 {
                return Err(ExtensionSchedulerConfigErrorV1::Excessive { field, value });
            }
        }
        if maximum_queued_jobs_per_package > maximum_queued_jobs {
            return Err(ExtensionSchedulerConfigErrorV1::PerPackageExceedsGlobal {
                per_package: ExtensionSchedulerLimitV1::MaximumQueuedJobsPerPackage,
                global: ExtensionSchedulerLimitV1::MaximumQueuedJobs,
            });
        }
        if maximum_running_jobs_per_package > maximum_running_jobs {
            return Err(ExtensionSchedulerConfigErrorV1::PerPackageExceedsGlobal {
                per_package: ExtensionSchedulerLimitV1::MaximumRunningJobsPerPackage,
                global: ExtensionSchedulerLimitV1::MaximumRunningJobs,
            });
        }
        Ok(Self {
            queued_global: maximum_queued_jobs,
            queued_per_package: maximum_queued_jobs_per_package,
            running_global: maximum_running_jobs,
            running_per_package: maximum_running_jobs_per_package,
        })
    }
}

/// A specific scheduler configuration field rejected before allocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExtensionSchedulerLimitV1 {
    MaximumQueuedJobs,
    MaximumQueuedJobsPerPackage,
    MaximumRunningJobs,
    MaximumRunningJobsPerPackage,
    MaximumConsecutiveHighPriorityStarts,
}

/// Typed validation failure for bounded scheduler configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExtensionSchedulerConfigErrorV1 {
    Zero {
        field: ExtensionSchedulerLimitV1,
    },
    Excessive {
        field: ExtensionSchedulerLimitV1,
        value: usize,
    },
    PerPackageExceedsGlobal {
        per_package: ExtensionSchedulerLimitV1,
        global: ExtensionSchedulerLimitV1,
    },
}

/// Scheduler limits for the two separate host resource classes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExtensionSchedulerConfigV1 {
    cpu: ExtensionQueueLimitsV1,
    io: ExtensionQueueLimitsV1,
    maximum_consecutive_high_priority_starts: usize,
}

impl ExtensionSchedulerConfigV1 {
    /// Validates limits and the bounded visible-first scheduling burst.
    ///
    /// # Errors
    ///
    /// Returns a typed error when the visible burst is zero or excessive.
    pub fn try_new(
        cpu: ExtensionQueueLimitsV1,
        io: ExtensionQueueLimitsV1,
        maximum_consecutive_high_priority_starts: usize,
    ) -> Result<Self, ExtensionSchedulerConfigErrorV1> {
        if maximum_consecutive_high_priority_starts == 0 {
            return Err(ExtensionSchedulerConfigErrorV1::Zero {
                field: ExtensionSchedulerLimitV1::MaximumConsecutiveHighPriorityStarts,
            });
        }
        if maximum_consecutive_high_priority_starts > MAX_EXTENSION_QUEUE_LIMIT_V1 {
            return Err(ExtensionSchedulerConfigErrorV1::Excessive {
                field: ExtensionSchedulerLimitV1::MaximumConsecutiveHighPriorityStarts,
                value: maximum_consecutive_high_priority_starts,
            });
        }
        Ok(Self {
            cpu,
            io,
            maximum_consecutive_high_priority_starts,
        })
    }

    const fn limits_for(self, class: ExtensionJobClassV1) -> ExtensionQueueLimitsV1 {
        match class {
            ExtensionJobClassV1::Cpu => self.cpu,
            ExtensionJobClassV1::Io => self.io,
        }
    }
}

/// One host-internal extension work request awaiting worker admission.
#[derive(Clone, Debug)]
pub struct ExtensionJobRequestV1<T> {
    pub scope: ExtensionJobScopeV1,
    pub class: ExtensionJobClassV1,
    pub priority: JobPriority,
    pub deadline: RequestDeadline,
    pub cancellation: CancellationToken,
    pub payload: T,
}

/// Result of non-blocking extension-job submission.
#[derive(Clone, Debug)]
pub enum ExtensionScheduleOutcomeV1<T> {
    Queued {
        job_id: ExtensionJobIdV1,
    },
    DeadlineElapsed(ExtensionJobRequestV1<T>),
    Cancelled(ExtensionJobRequestV1<T>),
    Overloaded(ExtensionJobRequestV1<T>),
    /// The owner has closed this feature generation or a newer one.
    ScopeClosed(ExtensionJobRequestV1<T>),
    /// The opaque identifier space is exhausted; no existing job was replaced.
    IdentifierExhausted(ExtensionJobRequestV1<T>),
}

/// Work admitted for a worker to execute synchronously.
#[derive(Clone, Debug)]
pub struct ExtensionStartedJobV1<T> {
    pub job_id: ExtensionJobIdV1,
    pub scope: ExtensionJobScopeV1,
    pub class: ExtensionJobClassV1,
    pub deadline: RequestDeadline,
    pub cancellation: CancellationToken,
    pub payload: T,
}

/// Tokens to signal after leaving every scheduler mutex or other critical section.
#[must_use = "cancellation actions must be signalled after releasing scheduler locks"]
#[derive(Debug, Default)]
pub struct ExtensionCancellationActionsV1 {
    tokens: Vec<CancellationToken>,
}

impl ExtensionCancellationActionsV1 {
    /// Number of distinct job cancellation requests represented by this batch.
    #[must_use]
    pub fn len(&self) -> usize {
        self.tokens.len()
    }

    /// Returns whether no caller-side signals are required.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }

    /// Transfers tokens to a host-owned cancellation executor.
    #[must_use]
    pub fn into_tokens(self) -> Vec<CancellationToken> {
        self.tokens
    }

    /// Signals all actions after releasing scheduler state.
    ///
    /// Scheduler bookkeeping has already completed before this method is
    /// called. [`CancellationToken::cancel`] isolates each callback panic, so
    /// a hostile extension callback cannot prevent later callbacks or tokens.
    pub fn signal_all(self) -> ExtensionCancellationSignalReportV1 {
        let mut report = ExtensionCancellationSignalReportV1::default();
        for token in self.tokens {
            report.signalled_tokens += 1;
            let token_report = token.cancel_with_report();
            report.callbacks_invoked += token_report.callbacks_invoked;
            report.panicked_callbacks += token_report.panicked_callbacks;
            report.already_cancelled_tokens += usize::from(token_report.already_cancelled);
        }
        report
    }

    fn push(&mut self, token: CancellationToken) {
        self.tokens.push(token);
    }

    fn extend(&mut self, other: Self) {
        self.tokens.extend(other.tokens);
    }
}

/// Aggregate-only result from host-side cancellation signalling.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ExtensionCancellationSignalReportV1 {
    pub signalled_tokens: usize,
    pub callbacks_invoked: usize,
    pub panicked_callbacks: usize,
    pub already_cancelled_tokens: usize,
}

/// Result of a cancellation or deadline poll mutation.
#[must_use = "cancellation actions must be handled after releasing scheduler locks"]
#[derive(Debug, Default)]
pub struct ExtensionCancellationResultV1 {
    pub affected_jobs: usize,
    pub actions: ExtensionCancellationActionsV1,
}

/// Terminal scheduler bookkeeping outcome after a worker returns.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExtensionCompletionOutcomeV1 {
    Completed,
    Cancelled,
    DeadlineExceeded,
    UnknownOrAlreadyTerminal,
}

/// Completion bookkeeping plus any host-side signal required after unlocking.
#[must_use = "completion outcome and cancellation actions must be handled"]
#[derive(Debug)]
pub struct ExtensionCompletionResultV1 {
    pub outcome: ExtensionCompletionOutcomeV1,
    pub actions: ExtensionCancellationActionsV1,
}

/// The work selected by a non-blocking worker poll plus expired-job signals.
#[must_use = "started work and expired-job cancellation actions must be handled"]
#[derive(Debug)]
pub struct ExtensionStartPollV1<T> {
    pub started: Option<ExtensionStartedJobV1<T>>,
    pub actions: ExtensionCancellationActionsV1,
}

/// Exact activity for one lifecycle scope.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ExtensionScopeActivityV1 {
    pub queued_jobs: usize,
    pub running_jobs: usize,
}

impl ExtensionScopeActivityV1 {
    /// Returns whether no queued or executing work remains in this exact scope.
    #[must_use]
    pub const fn is_drained(self) -> bool {
        self.queued_jobs == 0 && self.running_jobs == 0
    }
}

/// Permanent monotonic close-through fence for one package feature owner.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ExtensionDrainFenceV1 {
    pub package: ExtensionPackageIdV1,
    pub feature_id: String,
    pub closed_through_epoch: u64,
}

impl ExtensionDrainFenceV1 {
    fn owner(&self) -> ExtensionScopeOwnerV1 {
        ExtensionScopeOwnerV1 {
            package: self.package.clone(),
            feature_id: self.feature_id.clone(),
        }
    }
}

/// Atomic result of installing an epoch fence and cancelling covered work.
#[must_use = "fence capacity failure must abort or roll back the lifecycle transaction"]
#[derive(Debug)]
pub enum ExtensionScopeCloseOutcomeV1 {
    Closed {
        fence: ExtensionDrainFenceV1,
        cancellation: ExtensionCancellationResultV1,
    },
    /// No fence or job state changed because the bounded fence store is full.
    /// The lifecycle adapter must abort or roll back its external admission gate.
    FenceCapacityReached,
}

/// Bounded, path-free scheduler counters for one resource class.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ExtensionQueueStatsV1 {
    pub queued_jobs: usize,
    pub running_jobs: usize,
    pub expired_queued_jobs: u64,
    pub cancelled_queued_jobs: u64,
    pub cancelled_running_jobs: u64,
}

#[derive(Debug)]
struct QueuedJobV1<T> {
    request: ExtensionJobRequestV1<T>,
}

#[derive(Debug)]
struct RunningJobV1 {
    scope: ExtensionJobScopeV1,
    class: ExtensionJobClassV1,
    deadline: RequestDeadline,
    cancellation: CancellationToken,
    cancellation_requested: bool,
}

/// Deterministic extension-job admission scheduler.
#[derive(Debug)]
pub struct ExtensionJobSchedulerV1<T> {
    config: ExtensionSchedulerConfigV1,
    lanes: [[VecDeque<ExtensionJobIdV1>; PRIORITY_LANES]; QUEUE_CLASSES],
    queued: HashMap<ExtensionJobIdV1, QueuedJobV1<T>>,
    running: HashMap<ExtensionJobIdV1, RunningJobV1>,
    running_per_package: HashMap<(ExtensionJobClassV1, ExtensionPackageIdV1), usize>,
    queued_per_package: HashMap<(ExtensionJobClassV1, ExtensionPackageIdV1), usize>,
    closed_epoch_by_owner: HashMap<ExtensionScopeOwnerV1, u64>,
    stats: [ExtensionQueueStatsV1; QUEUE_CLASSES],
    consecutive_visible_starts: [usize; QUEUE_CLASSES],
    next_lower_priority: [usize; QUEUE_CLASSES],
    next_job_id: Option<u64>,
}

impl<T> ExtensionJobSchedulerV1<T> {
    /// Creates an empty scheduler with independent CPU and I/O limits.
    #[must_use]
    pub fn new(config: ExtensionSchedulerConfigV1) -> Self {
        Self {
            config,
            lanes: std::array::from_fn(|_| std::array::from_fn(|_| VecDeque::new())),
            queued: HashMap::new(),
            running: HashMap::new(),
            running_per_package: HashMap::new(),
            queued_per_package: HashMap::new(),
            closed_epoch_by_owner: HashMap::new(),
            stats: [ExtensionQueueStatsV1::default(); QUEUE_CLASSES],
            consecutive_visible_starts: [0; QUEUE_CLASSES],
            next_lower_priority: [1; QUEUE_CLASSES],
            next_job_id: Some(1),
        }
    }

    /// Queues work without waiting; rejected requests are returned intact.
    pub fn submit(
        &mut self,
        request: ExtensionJobRequestV1<T>,
        now: Instant,
    ) -> ExtensionScheduleOutcomeV1<T> {
        if request.cancellation.is_cancelled() {
            return ExtensionScheduleOutcomeV1::Cancelled(request);
        }
        if request.deadline.is_elapsed_at(now) {
            return ExtensionScheduleOutcomeV1::DeadlineElapsed(request);
        }
        if self.scope_is_closed(&request.scope) {
            return ExtensionScheduleOutcomeV1::ScopeClosed(request);
        }
        let class_index = request.class.index();
        let limits = self.config.limits_for(request.class);
        if self.stats[class_index].queued_jobs >= limits.queued_global {
            return ExtensionScheduleOutcomeV1::Overloaded(request);
        }
        let package_key = (request.class, request.scope.package.clone());
        if self
            .queued_per_package
            .get(&package_key)
            .copied()
            .unwrap_or_default()
            >= limits.queued_per_package
        {
            return ExtensionScheduleOutcomeV1::Overloaded(request);
        }
        let Some(raw_id) = self.next_job_id else {
            return ExtensionScheduleOutcomeV1::IdentifierExhausted(request);
        };
        self.next_job_id = raw_id.checked_add(1);
        let job_id = ExtensionJobIdV1(raw_id);
        let priority = priority_index(request.priority);
        self.lanes[class_index][priority].push_back(job_id);
        self.queued.insert(job_id, QueuedJobV1 { request });
        self.stats[class_index].queued_jobs += 1;
        *self.queued_per_package.entry(package_key).or_default() += 1;
        ExtensionScheduleOutcomeV1::Queued { job_id }
    }

    /// Starts the oldest eligible request while enforcing bounded visible bursts.
    pub fn try_start(
        &mut self,
        class: ExtensionJobClassV1,
        now: Instant,
    ) -> ExtensionStartPollV1<T> {
        let mut actions = ExtensionCancellationActionsV1::default();
        let class_index = class.index();
        let limits = self.config.limits_for(class);
        if self.stats[class_index].running_jobs >= limits.running_global {
            return ExtensionStartPollV1 {
                started: None,
                actions,
            };
        }
        let started = if self.consecutive_visible_starts[class_index]
            >= self.config.maximum_consecutive_high_priority_starts
            && let Some(started) = self.try_start_lower_rotating(class, now, limits, &mut actions)
        {
            Some(started)
        } else {
            self.try_start_priority(class, now, limits, 0, &mut actions)
                .or_else(|| self.try_start_lower_rotating(class, now, limits, &mut actions))
        };
        ExtensionStartPollV1 { started, actions }
    }

    /// Marks a job cancelled and returns any token to signal after unlocking.
    pub fn cancel(&mut self, job_id: ExtensionJobIdV1) -> ExtensionCancellationResultV1 {
        if self.queued.contains_key(&job_id) {
            let Some(queued) = self.remove_queued(job_id, true, false) else {
                return ExtensionCancellationResultV1::default();
            };
            return ExtensionCancellationResultV1 {
                affected_jobs: 1,
                actions: ExtensionCancellationActionsV1 {
                    tokens: vec![queued.request.cancellation],
                },
            };
        }
        let Some(action) = self.request_running_cancellation(job_id) else {
            return ExtensionCancellationResultV1::default();
        };
        ExtensionCancellationResultV1 {
            affected_jobs: 1,
            actions: action,
        }
    }

    /// Cancels all known work for exactly one feature lifecycle scope.
    pub fn cancel_scope(&mut self, scope: &ExtensionJobScopeV1) -> ExtensionCancellationResultV1 {
        self.cancel_matching(|candidate| candidate == scope)
    }

    /// Cancels all known work for one package without affecting other packages.
    pub fn cancel_package(
        &mut self,
        package: &ExtensionPackageIdV1,
    ) -> ExtensionCancellationResultV1 {
        self.cancel_matching(|candidate| &candidate.package == package)
    }

    /// Fences this owner at the supplied epoch and cancels this and older work.
    ///
    /// Fences are permanent monotonic floors: after close-through `N`, epochs
    /// `<= N` are always rejected, while epoch `N + 1` may submit immediately.
    pub fn close_scope(&mut self, scope: &ExtensionJobScopeV1) -> ExtensionScopeCloseOutcomeV1 {
        let owner = scope.owner();
        let closed_through_epoch = if let Some(highest) = self.closed_epoch_by_owner.get_mut(&owner)
        {
            *highest = (*highest).max(scope.lifecycle_epoch);
            *highest
        } else {
            if self.closed_epoch_by_owner.len() >= MAX_EXTENSION_QUEUE_LIMIT_V1 {
                return ExtensionScopeCloseOutcomeV1::FenceCapacityReached;
            }
            self.closed_epoch_by_owner
                .insert(owner.clone(), scope.lifecycle_epoch);
            scope.lifecycle_epoch
        };
        let fence = ExtensionDrainFenceV1 {
            package: scope.package.clone(),
            feature_id: scope.feature_id.clone(),
            closed_through_epoch,
        };
        let cancellation = self.cancel_matching(|candidate| {
            candidate.owner() == owner && candidate.lifecycle_epoch <= closed_through_epoch
        });
        ExtensionScopeCloseOutcomeV1::Closed {
            fence,
            cancellation,
        }
    }

    /// Returns activity for exactly one package/feature/epoch scope.
    #[must_use]
    pub fn scope_activity(&self, scope: &ExtensionJobScopeV1) -> ExtensionScopeActivityV1 {
        ExtensionScopeActivityV1 {
            queued_jobs: self
                .queued
                .values()
                .filter(|job| &job.request.scope == scope)
                .count(),
            running_jobs: self
                .running
                .values()
                .filter(|job| &job.scope == scope)
                .count(),
        }
    }

    /// Returns whether exactly one lifecycle scope has no queued or running work.
    #[must_use]
    pub fn is_drained(&self, scope: &ExtensionJobScopeV1) -> bool {
        self.scope_activity(scope).is_drained()
    }

    /// Returns activity for all work covered by a close-through fence.
    #[must_use]
    pub fn fence_activity(&self, fence: &ExtensionDrainFenceV1) -> ExtensionScopeActivityV1 {
        let owner = fence.owner();
        ExtensionScopeActivityV1 {
            queued_jobs: self
                .queued
                .values()
                .filter(|job| {
                    job.request.scope.owner() == owner
                        && job.request.scope.lifecycle_epoch <= fence.closed_through_epoch
                })
                .count(),
            running_jobs: self
                .running
                .values()
                .filter(|job| {
                    job.scope.owner() == owner
                        && job.scope.lifecycle_epoch <= fence.closed_through_epoch
                })
                .count(),
        }
    }

    /// Returns whether every scope covered by this fence has drained.
    #[must_use]
    pub fn is_fence_drained(&self, fence: &ExtensionDrainFenceV1) -> bool {
        self.fence_activity(fence).is_drained()
    }

    /// Expires queued work and requests cooperative cancellation of expired runs.
    /// The returned actions must be signalled by the host after releasing its lock.
    pub fn expire(&mut self, now: Instant) -> ExtensionCancellationResultV1 {
        let queued_ids = self
            .queued
            .iter()
            .filter_map(|(id, queued)| queued.request.deadline.is_elapsed_at(now).then_some(*id))
            .collect::<Vec<_>>();
        let running_ids = self
            .running
            .iter()
            .filter_map(|(id, running)| running.deadline.is_elapsed_at(now).then_some(*id))
            .collect::<Vec<_>>();
        let mut result = ExtensionCancellationResultV1::default();
        for queued in self.remove_queued_batch(&queued_ids, false, true) {
            result.affected_jobs += 1;
            result.actions.push(queued.request.cancellation);
        }
        for job_id in running_ids {
            if let Some(actions) = self.request_running_cancellation(job_id) {
                result.affected_jobs += 1;
                result.actions.extend(actions);
            }
        }
        result
    }

    /// Releases one running slot and returns its terminal outcome plus actions.
    pub fn complete(
        &mut self,
        job_id: ExtensionJobIdV1,
        now: Instant,
    ) -> ExtensionCompletionResultV1 {
        let Some(mut running) = self.running.remove(&job_id) else {
            return ExtensionCompletionResultV1 {
                outcome: ExtensionCompletionOutcomeV1::UnknownOrAlreadyTerminal,
                actions: ExtensionCancellationActionsV1::default(),
            };
        };
        self.release_running_slot(&running);
        let deadline_elapsed = running.deadline.is_elapsed_at(now);
        let actions = if deadline_elapsed && !running.cancellation_requested {
            running.cancellation_requested = true;
            ExtensionCancellationActionsV1 {
                tokens: vec![running.cancellation.clone()],
            }
        } else {
            ExtensionCancellationActionsV1::default()
        };
        ExtensionCompletionResultV1 {
            outcome: if deadline_elapsed {
                ExtensionCompletionOutcomeV1::DeadlineExceeded
            } else if running.cancellation.is_cancelled() || running.cancellation_requested {
                ExtensionCompletionOutcomeV1::Cancelled
            } else {
                ExtensionCompletionOutcomeV1::Completed
            },
            actions,
        }
    }

    /// Returns bounded counters for one resource class without exposing payloads.
    #[must_use]
    pub const fn stats(&self, class: ExtensionJobClassV1) -> ExtensionQueueStatsV1 {
        self.stats[class.index()]
    }

    fn try_start_lower_rotating(
        &mut self,
        class: ExtensionJobClassV1,
        now: Instant,
        limits: ExtensionQueueLimitsV1,
        actions: &mut ExtensionCancellationActionsV1,
    ) -> Option<ExtensionStartedJobV1<T>> {
        let class_index = class.index();
        let start = self.next_lower_priority[class_index];
        for offset in 0..(PRIORITY_LANES - 1) {
            let priority = 1 + ((start - 1 + offset) % (PRIORITY_LANES - 1));
            if let Some(started) = self.try_start_priority(class, now, limits, priority, actions) {
                self.next_lower_priority[class_index] = 1 + (priority % (PRIORITY_LANES - 1));
                return Some(started);
            }
        }
        None
    }

    fn try_start_priority(
        &mut self,
        class: ExtensionJobClassV1,
        now: Instant,
        limits: ExtensionQueueLimitsV1,
        priority: usize,
        actions: &mut ExtensionCancellationActionsV1,
    ) -> Option<ExtensionStartedJobV1<T>> {
        let class_index = class.index();
        let scans = self.lanes[class_index][priority].len();
        for _ in 0..scans {
            let job_id = self.lanes[class_index][priority].pop_front()?;
            let Some(queued) = self.queued.get(&job_id) else {
                continue;
            };
            if queued.request.cancellation.is_cancelled() {
                let _ = self.remove_queued_popped(job_id, true, false);
                continue;
            }
            if queued.request.deadline.is_elapsed_at(now) {
                if let Some(expired) = self.remove_queued_popped(job_id, false, true) {
                    actions.push(expired.request.cancellation);
                }
                continue;
            }
            let package_key = (class, queued.request.scope.package.clone());
            if self
                .running_per_package
                .get(&package_key)
                .copied()
                .unwrap_or_default()
                >= limits.running_per_package
            {
                self.lanes[class_index][priority].push_back(job_id);
                continue;
            }
            let queued = self.remove_queued_popped(job_id, false, false)?;
            self.stats[class_index].running_jobs += 1;
            *self.running_per_package.entry(package_key).or_default() += 1;
            let request = queued.request;
            self.running.insert(
                job_id,
                RunningJobV1 {
                    scope: request.scope.clone(),
                    class,
                    deadline: request.deadline,
                    cancellation: request.cancellation.clone(),
                    cancellation_requested: false,
                },
            );
            if priority == 0 {
                self.consecutive_visible_starts[class_index] += 1;
            } else {
                self.consecutive_visible_starts[class_index] = 0;
            }
            return Some(ExtensionStartedJobV1 {
                job_id,
                scope: request.scope,
                class: request.class,
                deadline: request.deadline,
                cancellation: request.cancellation,
                payload: request.payload,
            });
        }
        None
    }

    fn cancel_matching(
        &mut self,
        matches: impl Fn(&ExtensionJobScopeV1) -> bool,
    ) -> ExtensionCancellationResultV1 {
        let queued_ids = self
            .queued
            .iter()
            .filter_map(|(id, queued)| matches(&queued.request.scope).then_some(*id))
            .collect::<Vec<_>>();
        let running_ids = self
            .running
            .iter()
            .filter_map(|(id, running)| matches(&running.scope).then_some(*id))
            .collect::<Vec<_>>();
        let mut result = ExtensionCancellationResultV1::default();
        for queued in self.remove_queued_batch(&queued_ids, true, false) {
            result.affected_jobs += 1;
            result.actions.push(queued.request.cancellation);
        }
        for job_id in running_ids {
            if let Some(actions) = self.request_running_cancellation(job_id) {
                result.affected_jobs += 1;
                result.actions.extend(actions);
            }
        }
        result
    }

    fn request_running_cancellation(
        &mut self,
        job_id: ExtensionJobIdV1,
    ) -> Option<ExtensionCancellationActionsV1> {
        let running = self.running.get_mut(&job_id)?;
        if running.cancellation_requested || running.cancellation.is_cancelled() {
            return Some(ExtensionCancellationActionsV1::default());
        }
        running.cancellation_requested = true;
        let class_index = running.class.index();
        self.stats[class_index].cancelled_running_jobs += 1;
        Some(ExtensionCancellationActionsV1 {
            tokens: vec![running.cancellation.clone()],
        })
    }

    fn release_running_slot(&mut self, running: &RunningJobV1) {
        let class_index = running.class.index();
        self.stats[class_index].running_jobs =
            self.stats[class_index].running_jobs.saturating_sub(1);
        let package_key = (running.class, running.scope.package.clone());
        if let Some(count) = self.running_per_package.get_mut(&package_key) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                self.running_per_package.remove(&package_key);
            }
        }
    }

    fn remove_queued(
        &mut self,
        job_id: ExtensionJobIdV1,
        cancelled: bool,
        expired: bool,
    ) -> Option<QueuedJobV1<T>> {
        let queued = self.remove_queued_internal(job_id, cancelled, expired)?;
        let class_index = queued.request.class.index();
        let priority = priority_index(queued.request.priority);
        self.lanes[class_index][priority].retain(|id| *id != job_id);
        Some(queued)
    }

    fn remove_queued_popped(
        &mut self,
        job_id: ExtensionJobIdV1,
        cancelled: bool,
        expired: bool,
    ) -> Option<QueuedJobV1<T>> {
        self.remove_queued_internal(job_id, cancelled, expired)
    }

    fn remove_queued_batch(
        &mut self,
        job_ids: &[ExtensionJobIdV1],
        cancelled: bool,
        expired: bool,
    ) -> Vec<QueuedJobV1<T>> {
        let ids = job_ids.iter().copied().collect::<HashSet<_>>();
        if ids.is_empty() {
            return Vec::new();
        }
        for class_lanes in &mut self.lanes {
            for lane in class_lanes {
                lane.retain(|id| !ids.contains(id));
            }
        }
        job_ids
            .iter()
            .filter_map(|job_id| self.remove_queued_internal(*job_id, cancelled, expired))
            .collect()
    }

    fn remove_queued_internal(
        &mut self,
        job_id: ExtensionJobIdV1,
        cancelled: bool,
        expired: bool,
    ) -> Option<QueuedJobV1<T>> {
        let queued = self.queued.remove(&job_id)?;
        let class_index = queued.request.class.index();
        let package_key = (queued.request.class, queued.request.scope.package.clone());
        self.stats[class_index].queued_jobs = self.stats[class_index].queued_jobs.saturating_sub(1);
        if let Some(count) = self.queued_per_package.get_mut(&package_key) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                self.queued_per_package.remove(&package_key);
            }
        }
        if cancelled {
            self.stats[class_index].cancelled_queued_jobs += 1;
        }
        if expired {
            self.stats[class_index].expired_queued_jobs += 1;
        }
        Some(queued)
    }

    fn scope_is_closed(&self, scope: &ExtensionJobScopeV1) -> bool {
        self.closed_epoch_by_owner
            .get(&scope.owner())
            .is_some_and(|closed| scope.lifecycle_epoch <= *closed)
    }
}

const fn priority_index(priority: JobPriority) -> usize {
    match priority {
        JobPriority::VisibleViewport => 0,
        JobPriority::CurrentDirectory => 1,
        JobPriority::Prefetch => 2,
        JobPriority::Maintenance => 3,
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            Arc, Mutex,
            atomic::{AtomicUsize, Ordering},
        },
        time::{Duration, Instant},
    };

    use explorer_common::RequestDeadline;
    use explorer_model::CancellationToken;

    use super::*;

    fn config(
        queued: usize,
        running: usize,
        per_package: usize,
        burst: usize,
    ) -> ExtensionSchedulerConfigV1 {
        let limits = ExtensionQueueLimitsV1::try_new(queued, queued, running, per_package).unwrap();
        ExtensionSchedulerConfigV1::try_new(limits, limits, burst).unwrap()
    }

    fn scope(package: &str, epoch: u64) -> ExtensionJobScopeV1 {
        ExtensionJobScopeV1::new(
            ExtensionPackageIdV1::from_validated(package),
            "feature",
            epoch,
        )
    }

    fn request(
        scope: ExtensionJobScopeV1,
        priority: JobPriority,
        payload: usize,
        token: CancellationToken,
    ) -> ExtensionJobRequestV1<usize> {
        ExtensionJobRequestV1 {
            scope,
            class: ExtensionJobClassV1::Cpu,
            priority,
            deadline: RequestDeadline::none(),
            cancellation: token,
            payload,
        }
    }

    #[allow(
        clippy::needless_pass_by_value,
        reason = "the helper consumes accepted and rejected request ownership"
    )]
    fn queued(outcome: ExtensionScheduleOutcomeV1<usize>) -> ExtensionJobIdV1 {
        match outcome {
            ExtensionScheduleOutcomeV1::Queued { job_id } => job_id,
            _ => panic!("expected queued"),
        }
    }

    #[test]
    fn cancellation_actions_run_after_the_scheduler_lock_is_released() {
        let now = Instant::now();
        let scheduler = Arc::new(Mutex::new(ExtensionJobSchedulerV1::new(config(2, 1, 1, 2))));
        let token = CancellationToken::new();
        let callback_scheduler = scheduler.clone();
        let _registration = token.register(move || {
            let guard = callback_scheduler
                .try_lock()
                .expect("callback must not run under scheduler lock");
            let _ = guard.stats(ExtensionJobClassV1::Cpu);
        });
        let id = {
            let mut guard = scheduler.lock().unwrap();
            queued(guard.submit(
                request(scope("a", 1), JobPriority::Prefetch, 1, token.clone()),
                now,
            ))
        };
        let actions = scheduler.lock().unwrap().cancel(id).actions;
        assert!(!token.is_cancelled());
        actions.signal_all();
        assert!(token.is_cancelled());
    }

    #[test]
    fn a_panicking_callback_does_not_block_later_callbacks_on_the_same_token() {
        let first = CancellationToken::new();
        let second = CancellationToken::new();
        let _first_registration = first.register(|| panic!("hostile extension callback"));
        let later_callback_runs = Arc::new(AtomicUsize::new(0));
        let later_runs = Arc::clone(&later_callback_runs);
        let _second_callback = first.register(move || {
            later_runs.fetch_add(1, Ordering::Relaxed);
        });
        let report = ExtensionCancellationActionsV1 {
            tokens: vec![first.clone(), second.clone()],
        }
        .signal_all();
        assert_eq!(report.signalled_tokens, 2);
        assert_eq!(report.callbacks_invoked, 2);
        assert_eq!(report.panicked_callbacks, 1);
        assert!(first.is_cancelled());
        assert!(second.is_cancelled());
        assert_eq!(later_callback_runs.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn close_scope_fences_same_or_older_epoch_and_reports_exact_drain() {
        let now = Instant::now();
        let mut scheduler = ExtensionJobSchedulerV1::new(config(8, 2, 2, 2));
        let old = scope("a", 1);
        let queued_id = queued(scheduler.submit(
            request(
                old.clone(),
                JobPriority::Prefetch,
                1,
                CancellationToken::new(),
            ),
            now,
        ));
        let started = scheduler
            .try_start(ExtensionJobClassV1::Cpu, now)
            .started
            .unwrap();
        assert_eq!(started.job_id, queued_id);
        let close = scheduler.close_scope(&old);
        let ExtensionScopeCloseOutcomeV1::Closed {
            fence,
            cancellation,
        } = close
        else {
            panic!("fence capacity")
        };
        assert_eq!(cancellation.affected_jobs, 1);
        assert_eq!(scheduler.scope_activity(&old).running_jobs, 1);
        assert_eq!(scheduler.fence_activity(&fence).running_jobs, 1);
        cancellation.actions.signal_all();
        assert_eq!(
            scheduler.complete(started.job_id, now).outcome,
            ExtensionCompletionOutcomeV1::Cancelled
        );
        assert!(scheduler.is_drained(&old));
        assert!(matches!(
            scheduler.submit(
                request(
                    old.clone(),
                    JobPriority::Prefetch,
                    2,
                    CancellationToken::new()
                ),
                now
            ),
            ExtensionScheduleOutcomeV1::ScopeClosed(_)
        ));
        let newer = scope("a", 2);
        let _ = queued(scheduler.submit(
            request(
                newer.clone(),
                JobPriority::Prefetch,
                3,
                CancellationToken::new(),
            ),
            now,
        ));
        assert!(matches!(
            scheduler.submit(
                request(old, JobPriority::Prefetch, 4, CancellationToken::new()),
                now
            ),
            ExtensionScheduleOutcomeV1::ScopeClosed(_)
        ));
        assert!(scheduler.is_fence_drained(&fence));
    }

    #[test]
    fn identifier_exhaustion_never_replaces_an_existing_job() {
        let now = Instant::now();
        let mut scheduler = ExtensionJobSchedulerV1::new(config(2, 1, 1, 1));
        scheduler.next_job_id = Some(u64::MAX);
        let last = queued(scheduler.submit(
            request(
                scope("a", 1),
                JobPriority::Prefetch,
                1,
                CancellationToken::new(),
            ),
            now,
        ));
        assert_eq!(last, ExtensionJobIdV1(u64::MAX));
        assert!(
            matches!(scheduler.submit(request(scope("b", 1), JobPriority::Prefetch, 2, CancellationToken::new()), now), ExtensionScheduleOutcomeV1::IdentifierExhausted(request) if request.payload == 2)
        );
        assert_eq!(
            scheduler
                .try_start(ExtensionJobClassV1::Cpu, now)
                .started
                .unwrap()
                .job_id,
            last
        );
    }

    #[test]
    fn close_through_epoch_drains_n_minus_one_and_n_but_never_reopens_old_epochs() {
        let now = Instant::now();
        let mut scheduler = ExtensionJobSchedulerV1::new(config(4, 1, 1, 1));
        let previous = scope("a", 4);
        let current = scope("a", 5);
        let previous_id = queued(scheduler.submit(
            request(
                previous.clone(),
                JobPriority::VisibleViewport,
                1,
                CancellationToken::new(),
            ),
            now,
        ));
        let _current_id = queued(scheduler.submit(
            request(
                current.clone(),
                JobPriority::Prefetch,
                2,
                CancellationToken::new(),
            ),
            now,
        ));
        let started = scheduler
            .try_start(ExtensionJobClassV1::Cpu, now)
            .started
            .unwrap();
        assert_eq!(started.job_id, previous_id);
        let ExtensionScopeCloseOutcomeV1::Closed {
            fence,
            cancellation,
        } = scheduler.close_scope(&current)
        else {
            panic!("fence capacity")
        };
        assert_eq!(fence.closed_through_epoch, 5);
        assert_eq!(scheduler.fence_activity(&fence).running_jobs, 1);
        assert_eq!(scheduler.fence_activity(&fence).queued_jobs, 0);
        cancellation.actions.signal_all();
        let _ = scheduler.complete(previous_id, now);
        assert!(scheduler.is_fence_drained(&fence));
        for rejected in [previous, current] {
            assert!(matches!(
                scheduler.submit(
                    request(rejected, JobPriority::Prefetch, 3, CancellationToken::new()),
                    now
                ),
                ExtensionScheduleOutcomeV1::ScopeClosed(_)
            ));
        }
        assert!(matches!(
            scheduler.submit(
                request(
                    scope("a", 6),
                    JobPriority::Prefetch,
                    4,
                    CancellationToken::new()
                ),
                now
            ),
            ExtensionScheduleOutcomeV1::Queued { .. }
        ));
    }

    #[test]
    fn fence_capacity_reached_is_an_atomic_no_op() {
        let now = Instant::now();
        let mut scheduler = ExtensionJobSchedulerV1::new(config(2, 1, 1, 1));
        for index in 0..MAX_EXTENSION_QUEUE_LIMIT_V1 {
            scheduler.closed_epoch_by_owner.insert(
                ExtensionScopeOwnerV1 {
                    package: ExtensionPackageIdV1::from_validated(format!("owner-{index}")),
                    feature_id: "feature".to_owned(),
                },
                1,
            );
        }
        let target = scope("target", 1);
        let id = queued(scheduler.submit(
            request(
                target.clone(),
                JobPriority::Prefetch,
                1,
                CancellationToken::new(),
            ),
            now,
        ));
        assert!(matches!(
            scheduler.close_scope(&target),
            ExtensionScopeCloseOutcomeV1::FenceCapacityReached
        ));
        assert_eq!(scheduler.scope_activity(&target).queued_jobs, 1);
        assert_eq!(
            scheduler
                .try_start(ExtensionJobClassV1::Cpu, now)
                .started
                .unwrap()
                .job_id,
            id
        );
    }

    #[test]
    fn cancelling_capacity_one_removes_physical_lane_entries() {
        let now = Instant::now();
        let mut scheduler = ExtensionJobSchedulerV1::new(config(1, 1, 1, 1));
        for payload in 0..128 {
            let id = queued(scheduler.submit(
                request(
                    scope("a", 1),
                    JobPriority::Prefetch,
                    payload,
                    CancellationToken::new(),
                ),
                now,
            ));
            assert_eq!(scheduler.cancel(id).affected_jobs, 1);
            assert_eq!(scheduler.lanes[0][2].len(), 0);
        }
    }

    #[test]
    fn rotating_lower_lanes_get_bounded_turns_under_sustained_visible_arrivals() {
        let now = Instant::now();
        let mut scheduler = ExtensionJobSchedulerV1::new(config(64, 1, 1, 2));
        for (priority, payload) in [
            (JobPriority::CurrentDirectory, 10),
            (JobPriority::Prefetch, 20),
            (JobPriority::Maintenance, 30),
        ] {
            queued(scheduler.submit(
                request(
                    scope("lower", 1),
                    priority,
                    payload,
                    CancellationToken::new(),
                ),
                now,
            ));
        }
        let mut starts = Vec::new();
        for payload in 0..12 {
            queued(scheduler.submit(
                request(
                    scope("visible", payload as u64 + 1),
                    JobPriority::VisibleViewport,
                    payload,
                    CancellationToken::new(),
                ),
                now,
            ));
            let job = scheduler
                .try_start(ExtensionJobClassV1::Cpu, now)
                .started
                .unwrap();
            starts.push(job.payload);
            let _ = scheduler.complete(job.job_id, now);
        }
        for lower in [10, 20, 30] {
            assert!(
                starts
                    .iter()
                    .position(|value| *value == lower)
                    .is_some_and(|index| index <= 8)
            );
        }
    }

    #[test]
    fn expire_returns_actions_for_running_deadlines_without_waiting_for_completion() {
        let now = Instant::now();
        let mut scheduler = ExtensionJobSchedulerV1::new(config(2, 1, 1, 1));
        let token = CancellationToken::new();
        let mut job = request(
            scope("a", 1),
            JobPriority::VisibleViewport,
            1,
            token.clone(),
        );
        job.deadline = RequestDeadline::after(now, Duration::from_millis(1)).unwrap();
        let id = queued(scheduler.submit(job, now));
        assert_eq!(
            scheduler
                .try_start(ExtensionJobClassV1::Cpu, now)
                .started
                .unwrap()
                .job_id,
            id
        );
        let expired = scheduler.expire(now + Duration::from_millis(2));
        assert_eq!(expired.affected_jobs, 1);
        assert!(!token.is_cancelled());
        expired.actions.signal_all();
        assert!(token.is_cancelled());
        assert_eq!(
            scheduler
                .complete(id, now + Duration::from_millis(2))
                .outcome,
            ExtensionCompletionOutcomeV1::DeadlineExceeded
        );
    }

    #[test]
    fn try_start_returns_expired_queued_token_as_an_external_action() {
        let now = Instant::now();
        let mut scheduler = ExtensionJobSchedulerV1::new(config(2, 1, 1, 1));
        let token = CancellationToken::new();
        let mut job = request(
            scope("a", 1),
            JobPriority::VisibleViewport,
            1,
            token.clone(),
        );
        job.deadline = RequestDeadline::after(now, Duration::from_millis(1)).unwrap();
        let _ = queued(scheduler.submit(job, now));
        let poll = scheduler.try_start(ExtensionJobClassV1::Cpu, now + Duration::from_millis(2));
        assert!(poll.started.is_none());
        assert_eq!(poll.actions.len(), 1);
        assert_eq!(scheduler.stats(ExtensionJobClassV1::Cpu).queued_jobs, 0);
        assert!(!token.is_cancelled());
        let report = poll.actions.signal_all();
        assert_eq!(report.signalled_tokens, 1);
        assert!(token.is_cancelled());
    }

    #[test]
    fn batched_cancellation_and_normal_dispatch_keep_lane_storage_bounded() {
        const JOBS: usize = 4_096;
        let now = Instant::now();
        let mut scheduler = ExtensionJobSchedulerV1::new(config(JOBS, 1, 1, 1));
        for payload in 0..JOBS {
            let _ = queued(scheduler.submit(
                request(
                    scope("batch", 1),
                    JobPriority::Prefetch,
                    payload,
                    CancellationToken::new(),
                ),
                now,
            ));
        }
        let package = ExtensionPackageIdV1::from_validated("batch");
        let cancelled = scheduler.cancel_package(&package);
        assert_eq!(cancelled.affected_jobs, JOBS);
        assert_eq!(scheduler.lanes[0][2].len(), 0);
        assert_eq!(scheduler.stats(ExtensionJobClassV1::Cpu).queued_jobs, 0);

        for payload in 0..JOBS {
            let _ = queued(scheduler.submit(
                request(
                    scope("dispatch", 1),
                    JobPriority::Prefetch,
                    payload,
                    CancellationToken::new(),
                ),
                now,
            ));
        }
        for _ in 0..JOBS {
            let poll = scheduler.try_start(ExtensionJobClassV1::Cpu, now);
            let started = poll.started.expect("bounded queue dispatches");
            assert!(poll.actions.is_empty());
            let _ = scheduler.complete(started.job_id, now);
        }
        assert_eq!(scheduler.lanes[0][2].len(), 0);
        assert_eq!(scheduler.stats(ExtensionJobClassV1::Cpu).queued_jobs, 0);
        assert_eq!(scheduler.stats(ExtensionJobClassV1::Cpu).running_jobs, 0);
    }

    #[test]
    fn configuration_remains_strictly_bounded() {
        assert!(matches!(
            ExtensionQueueLimitsV1::try_new(0, 1, 1, 1),
            Err(ExtensionSchedulerConfigErrorV1::Zero { .. })
        ));
        assert!(matches!(
            ExtensionQueueLimitsV1::try_new(MAX_EXTENSION_QUEUE_LIMIT_V1 + 1, 1, 1, 1),
            Err(ExtensionSchedulerConfigErrorV1::Excessive { .. })
        ));
    }
}
