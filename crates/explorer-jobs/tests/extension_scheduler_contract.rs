//! Public deterministic contract coverage for extension scheduler admission.

use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use explorer_common::RequestDeadline;
use explorer_jobs::{
    ExtensionCompletionOutcomeV1, ExtensionJobClassV1, ExtensionJobRequestV1,
    ExtensionJobSchedulerV1, ExtensionJobScopeV1, ExtensionPackageIdV1, ExtensionQueueLimitsV1,
    ExtensionScheduleOutcomeV1, ExtensionSchedulerConfigErrorV1, ExtensionSchedulerConfigV1,
    ExtensionSchedulerLimitV1, ExtensionScopeCloseOutcomeV1, JobPriority,
    MAX_EXTENSION_QUEUE_LIMIT_V1,
};
use explorer_model::CancellationToken;

fn limits(
    queued: usize,
    pending_per_package: usize,
    running: usize,
    running_per_package: usize,
) -> ExtensionQueueLimitsV1 {
    ExtensionQueueLimitsV1::try_new(queued, pending_per_package, running, running_per_package)
        .expect("valid queue limits")
}

fn config(
    cpu: ExtensionQueueLimitsV1,
    io: ExtensionQueueLimitsV1,
    burst: usize,
) -> ExtensionSchedulerConfigV1 {
    ExtensionSchedulerConfigV1::try_new(cpu, io, burst).expect("valid scheduler config")
}

fn scope(package: &str, feature: &str, epoch: u64) -> ExtensionJobScopeV1 {
    ExtensionJobScopeV1::new(
        ExtensionPackageIdV1::from_validated(package),
        feature,
        epoch,
    )
}

fn request(
    scope: ExtensionJobScopeV1,
    class: ExtensionJobClassV1,
    priority: JobPriority,
    payload: usize,
    deadline: RequestDeadline,
    cancellation: CancellationToken,
) -> ExtensionJobRequestV1<usize> {
    ExtensionJobRequestV1 {
        scope,
        class,
        priority,
        deadline,
        cancellation,
        payload,
    }
}

#[allow(
    clippy::needless_pass_by_value,
    reason = "the helper mirrors submission ownership and rejects returned payload variants"
)]
fn queued(outcome: ExtensionScheduleOutcomeV1<usize>) -> explorer_jobs::ExtensionJobIdV1 {
    let ExtensionScheduleOutcomeV1::Queued { job_id } = outcome else {
        panic!("expected queued extension job");
    };
    job_id
}

fn complete(
    scheduler: &mut ExtensionJobSchedulerV1<usize>,
    job_id: explorer_jobs::ExtensionJobIdV1,
    now: Instant,
) -> ExtensionCompletionOutcomeV1 {
    let result = scheduler.complete(job_id, now);
    let _ = result.actions.signal_all();
    result.outcome
}

fn poll_started(
    scheduler: &mut ExtensionJobSchedulerV1<usize>,
    class: ExtensionJobClassV1,
    now: Instant,
) -> Option<explorer_jobs::ExtensionStartedJobV1<usize>> {
    let poll = scheduler.try_start(class, now);
    let _ = poll.actions.signal_all();
    poll.started
}

#[test]
fn rejects_zero_excessive_and_inconsistent_scheduler_limits() {
    assert_eq!(
        ExtensionQueueLimitsV1::try_new(0, 1, 1, 1),
        Err(ExtensionSchedulerConfigErrorV1::Zero {
            field: ExtensionSchedulerLimitV1::MaximumQueuedJobs,
        })
    );
    assert_eq!(
        ExtensionQueueLimitsV1::try_new(1, 2, 1, 1),
        Err(ExtensionSchedulerConfigErrorV1::PerPackageExceedsGlobal {
            per_package: ExtensionSchedulerLimitV1::MaximumQueuedJobsPerPackage,
            global: ExtensionSchedulerLimitV1::MaximumQueuedJobs,
        })
    );
    assert_eq!(
        ExtensionQueueLimitsV1::try_new(2, 1, 1, 2),
        Err(ExtensionSchedulerConfigErrorV1::PerPackageExceedsGlobal {
            per_package: ExtensionSchedulerLimitV1::MaximumRunningJobsPerPackage,
            global: ExtensionSchedulerLimitV1::MaximumRunningJobs,
        })
    );
    assert_eq!(
        ExtensionQueueLimitsV1::try_new(MAX_EXTENSION_QUEUE_LIMIT_V1 + 1, 1, 1, 1),
        Err(ExtensionSchedulerConfigErrorV1::Excessive {
            field: ExtensionSchedulerLimitV1::MaximumQueuedJobs,
            value: MAX_EXTENSION_QUEUE_LIMIT_V1 + 1,
        })
    );
    let valid = limits(2, 1, 1, 1);
    assert_eq!(
        ExtensionSchedulerConfigV1::try_new(valid, valid, 0),
        Err(ExtensionSchedulerConfigErrorV1::Zero {
            field: ExtensionSchedulerLimitV1::MaximumConsecutiveHighPriorityStarts,
        })
    );
}

#[test]
fn cpu_io_global_and_per_package_limits_are_isolated_and_recover() {
    let now = Instant::now();
    let mut scheduler =
        ExtensionJobSchedulerV1::new(config(limits(3, 1, 1, 1), limits(2, 2, 1, 1), 2));
    let a = scope("package-a", "feature", 1);
    let b = scope("package-b", "feature", 1);

    let cpu_a = queued(scheduler.submit(
        request(
            a.clone(),
            ExtensionJobClassV1::Cpu,
            JobPriority::VisibleViewport,
            1,
            RequestDeadline::none(),
            CancellationToken::new(),
        ),
        now,
    ));
    assert!(matches!(
        scheduler.submit(request(a.clone(), ExtensionJobClassV1::Cpu, JobPriority::VisibleViewport, 2, RequestDeadline::none(), CancellationToken::new()), now),
        ExtensionScheduleOutcomeV1::Overloaded(request) if request.payload == 2
    ));
    let cpu_b = queued(scheduler.submit(
        request(
            b.clone(),
            ExtensionJobClassV1::Cpu,
            JobPriority::VisibleViewport,
            3,
            RequestDeadline::none(),
            CancellationToken::new(),
        ),
        now,
    ));
    let io_a = queued(scheduler.submit(
        request(
            a,
            ExtensionJobClassV1::Io,
            JobPriority::VisibleViewport,
            4,
            RequestDeadline::none(),
            CancellationToken::new(),
        ),
        now,
    ));

    let started_cpu =
        poll_started(&mut scheduler, ExtensionJobClassV1::Cpu, now).expect("one CPU job starts");
    assert_eq!(started_cpu.payload, 1);
    assert!(
        poll_started(&mut scheduler, ExtensionJobClassV1::Cpu, now).is_none(),
        "CPU global running cap holds"
    );
    assert_eq!(
        poll_started(&mut scheduler, ExtensionJobClassV1::Io, now).map(|job| job.payload),
        Some(4),
        "CPU saturation must not consume I/O capacity"
    );
    assert_eq!(
        complete(&mut scheduler, started_cpu.job_id, now),
        ExtensionCompletionOutcomeV1::Completed
    );
    assert_eq!(
        poll_started(&mut scheduler, ExtensionJobClassV1::Cpu, now).map(|job| job.payload),
        Some(3),
        "blocked package A must not prevent package B progress"
    );

    assert_eq!(scheduler.stats(ExtensionJobClassV1::Cpu).queued_jobs, 0);
    assert_eq!(scheduler.stats(ExtensionJobClassV1::Cpu).running_jobs, 1);
    assert_eq!(
        complete(&mut scheduler, cpu_b, now),
        ExtensionCompletionOutcomeV1::Completed
    );
    assert_eq!(
        complete(&mut scheduler, io_a, now),
        ExtensionCompletionOutcomeV1::Completed
    );
    assert_eq!(
        complete(&mut scheduler, cpu_a, now),
        ExtensionCompletionOutcomeV1::UnknownOrAlreadyTerminal
    );
    assert_eq!(scheduler.stats(ExtensionJobClassV1::Cpu).running_jobs, 0);
    assert_eq!(scheduler.stats(ExtensionJobClassV1::Io).running_jobs, 0);
}

#[test]
fn visible_priority_is_fifo_and_lower_lane_starts_within_burst_bound() {
    let now = Instant::now();
    let mut scheduler =
        ExtensionJobSchedulerV1::new(config(limits(16, 16, 1, 1), limits(1, 1, 1, 1), 2));
    let work = scope("package", "feature", 1);
    let lower = queued(scheduler.submit(
        request(
            work.clone(),
            ExtensionJobClassV1::Cpu,
            JobPriority::Prefetch,
            100,
            RequestDeadline::none(),
            CancellationToken::new(),
        ),
        now,
    ));
    let first = queued(scheduler.submit(
        request(
            work.clone(),
            ExtensionJobClassV1::Cpu,
            JobPriority::VisibleViewport,
            1,
            RequestDeadline::none(),
            CancellationToken::new(),
        ),
        now,
    ));
    let second = queued(scheduler.submit(
        request(
            work.clone(),
            ExtensionJobClassV1::Cpu,
            JobPriority::VisibleViewport,
            2,
            RequestDeadline::none(),
            CancellationToken::new(),
        ),
        now,
    ));

    for expected in [1, 2] {
        let started = poll_started(&mut scheduler, ExtensionJobClassV1::Cpu, now)
            .expect("visible job starts");
        assert_eq!(
            started.payload, expected,
            "same-priority visible work is FIFO"
        );
        assert_eq!(
            complete(&mut scheduler, started.job_id, now),
            ExtensionCompletionOutcomeV1::Completed
        );
        let next_visible = expected + 2;
        let _ = queued(scheduler.submit(
            request(
                work.clone(),
                ExtensionJobClassV1::Cpu,
                JobPriority::VisibleViewport,
                next_visible,
                RequestDeadline::none(),
                CancellationToken::new(),
            ),
            now,
        ));
    }
    let started = poll_started(&mut scheduler, ExtensionJobClassV1::Cpu, now)
        .expect("lower lane must receive its bounded turn");
    assert_eq!(started.job_id, lower);
    assert_eq!(started.payload, 100);
    assert_eq!(
        complete(&mut scheduler, started.job_id, now),
        ExtensionCompletionOutcomeV1::Completed
    );
    assert_eq!(
        complete(&mut scheduler, first, now),
        ExtensionCompletionOutcomeV1::UnknownOrAlreadyTerminal
    );
    assert_eq!(
        complete(&mut scheduler, second, now),
        ExtensionCompletionOutcomeV1::UnknownOrAlreadyTerminal
    );
}

#[test]
fn queued_running_deadline_and_scope_cancellation_are_cooperative_and_exact() {
    let now = Instant::now();
    let mut scheduler =
        ExtensionJobSchedulerV1::new(config(limits(8, 8, 2, 2), limits(1, 1, 1, 1), 2));
    let old_epoch = scope("package", "feature", 1);
    let new_epoch = scope("package", "feature", 2);
    let another_feature = scope("package", "other-feature", 1);
    let queued_token = CancellationToken::new();
    let queued_id = queued(scheduler.submit(
        request(
            old_epoch.clone(),
            ExtensionJobClassV1::Cpu,
            JobPriority::Prefetch,
            1,
            RequestDeadline::none(),
            queued_token.clone(),
        ),
        now,
    ));
    let running_token = CancellationToken::new();
    let running_id = queued(scheduler.submit(
        request(
            old_epoch.clone(),
            ExtensionJobClassV1::Cpu,
            JobPriority::VisibleViewport,
            2,
            RequestDeadline::none(),
            running_token.clone(),
        ),
        now,
    ));
    let new_id = queued(scheduler.submit(
        request(
            new_epoch,
            ExtensionJobClassV1::Cpu,
            JobPriority::VisibleViewport,
            3,
            RequestDeadline::none(),
            CancellationToken::new(),
        ),
        now,
    ));
    let other_id = queued(scheduler.submit(
        request(
            another_feature,
            ExtensionJobClassV1::Cpu,
            JobPriority::VisibleViewport,
            4,
            RequestDeadline::none(),
            CancellationToken::new(),
        ),
        now,
    ));

    let started = poll_started(&mut scheduler, ExtensionJobClassV1::Cpu, now)
        .expect("old epoch running work starts");
    assert_eq!(started.job_id, running_id);
    let cancellation = scheduler.cancel_scope(&old_epoch);
    assert_eq!(
        cancellation.affected_jobs, 2,
        "scope cancellation must not cross feature/epoch boundaries"
    );
    assert!(!queued_token.is_cancelled());
    assert!(!running_token.is_cancelled());
    let signal_report = cancellation.actions.signal_all();
    assert_eq!(signal_report.signalled_tokens, 2);
    assert_eq!(signal_report.panicked_callbacks, 0);
    assert!(queued_token.is_cancelled());
    assert!(running_token.is_cancelled());
    assert_eq!(
        complete(&mut scheduler, running_id, now),
        ExtensionCompletionOutcomeV1::Cancelled
    );
    assert_eq!(
        complete(&mut scheduler, running_id, now),
        ExtensionCompletionOutcomeV1::UnknownOrAlreadyTerminal
    );
    assert_eq!(
        scheduler
            .stats(ExtensionJobClassV1::Cpu)
            .cancelled_queued_jobs,
        1
    );
    assert_eq!(
        scheduler
            .stats(ExtensionJobClassV1::Cpu)
            .cancelled_running_jobs,
        1
    );
    assert_eq!(
        poll_started(&mut scheduler, ExtensionJobClassV1::Cpu, now).map(|job| job.job_id),
        Some(new_id)
    );
    assert_eq!(
        complete(&mut scheduler, new_id, now),
        ExtensionCompletionOutcomeV1::Completed
    );
    assert_eq!(
        poll_started(&mut scheduler, ExtensionJobClassV1::Cpu, now).map(|job| job.job_id),
        Some(other_id)
    );
    assert_eq!(
        complete(&mut scheduler, other_id, now),
        ExtensionCompletionOutcomeV1::Completed
    );
    assert_eq!(
        complete(&mut scheduler, queued_id, now),
        ExtensionCompletionOutcomeV1::UnknownOrAlreadyTerminal
    );

    let elapsed = RequestDeadline::after(now, Duration::from_millis(1)).expect("deadline");
    assert!(matches!(
        scheduler.submit(request(old_epoch, ExtensionJobClassV1::Cpu, JobPriority::Prefetch, 9, elapsed, CancellationToken::new()), now + Duration::from_millis(2)),
        ExtensionScheduleOutcomeV1::DeadlineElapsed(request) if request.payload == 9
    ));
    let running_deadline = RequestDeadline::after(now, Duration::from_millis(1)).expect("deadline");
    let deadline_id = queued(scheduler.submit(
        request(
            scope("deadline", "feature", 1),
            ExtensionJobClassV1::Cpu,
            JobPriority::VisibleViewport,
            10,
            running_deadline,
            CancellationToken::new(),
        ),
        now,
    ));
    let _ = poll_started(&mut scheduler, ExtensionJobClassV1::Cpu, now)
        .expect("deadline job starts before expiry");
    assert_eq!(
        complete(&mut scheduler, deadline_id, now + Duration::from_millis(2)),
        ExtensionCompletionOutcomeV1::DeadlineExceeded
    );
}

#[test]
fn thousand_item_stress_never_exceeds_bounded_queue_or_running_limits() {
    const ITEMS: usize = 1_000;
    const QUEUE_CAP: usize = 1_000;
    const RUNNING_CAP: usize = 8;
    let now = Instant::now();
    let mut scheduler = ExtensionJobSchedulerV1::new(config(
        limits(QUEUE_CAP, 125, RUNNING_CAP, 2),
        limits(1, 1, 1, 1),
        4,
    ));
    for item in 0..ITEMS {
        let package = format!("package-{}", item % 8);
        let priority = if item % 5 == 0 {
            JobPriority::VisibleViewport
        } else {
            JobPriority::Prefetch
        };
        let _ = queued(scheduler.submit(
            request(
                scope(&package, "feature", 1),
                ExtensionJobClassV1::Cpu,
                priority,
                item,
                RequestDeadline::none(),
                CancellationToken::new(),
            ),
            now,
        ));
    }
    assert_eq!(
        scheduler.stats(ExtensionJobClassV1::Cpu).queued_jobs,
        QUEUE_CAP
    );

    let mut active = VecDeque::new();
    let mut completed = 0_usize;
    while completed < ITEMS {
        while active.len() < RUNNING_CAP {
            let Some(started) = poll_started(&mut scheduler, ExtensionJobClassV1::Cpu, now) else {
                break;
            };
            active.push_back(started);
            assert!(scheduler.stats(ExtensionJobClassV1::Cpu).running_jobs <= RUNNING_CAP);
            assert!(scheduler.stats(ExtensionJobClassV1::Cpu).queued_jobs <= QUEUE_CAP);
        }
        let started = active
            .pop_front()
            .expect("bounded queue must make progress");
        assert_eq!(
            complete(&mut scheduler, started.job_id, now),
            ExtensionCompletionOutcomeV1::Completed
        );
        completed += 1;
    }
    assert_eq!(scheduler.stats(ExtensionJobClassV1::Cpu).queued_jobs, 0);
    assert_eq!(scheduler.stats(ExtensionJobClassV1::Cpu).running_jobs, 0);
}

#[test]
fn close_scope_returns_a_permanent_fence_that_drains_all_covered_epochs() {
    let now = Instant::now();
    let mut scheduler =
        ExtensionJobSchedulerV1::new(config(limits(8, 8, 2, 2), limits(1, 1, 1, 1), 2));
    let previous = scope("package", "feature", 6);
    let current = scope("package", "feature", 7);
    let previous_token = CancellationToken::new();
    let current_token = CancellationToken::new();
    let previous_id = queued(scheduler.submit(
        request(
            previous.clone(),
            ExtensionJobClassV1::Cpu,
            JobPriority::VisibleViewport,
            1,
            RequestDeadline::none(),
            previous_token.clone(),
        ),
        now,
    ));
    let current_id = queued(scheduler.submit(
        request(
            current.clone(),
            ExtensionJobClassV1::Cpu,
            JobPriority::VisibleViewport,
            2,
            RequestDeadline::none(),
            current_token.clone(),
        ),
        now,
    ));
    assert_eq!(
        poll_started(&mut scheduler, ExtensionJobClassV1::Cpu, now).map(|job| job.job_id),
        Some(previous_id)
    );
    assert_eq!(
        poll_started(&mut scheduler, ExtensionJobClassV1::Cpu, now).map(|job| job.job_id),
        Some(current_id)
    );

    let ExtensionScopeCloseOutcomeV1::Closed {
        fence,
        cancellation,
    } = scheduler.close_scope(&current)
    else {
        panic!("scope fence must remain bounded but available");
    };
    assert_eq!(fence.closed_through_epoch, 7);
    assert_eq!(cancellation.affected_jobs, 2);
    assert_eq!(scheduler.fence_activity(&fence).queued_jobs, 0);
    assert_eq!(scheduler.fence_activity(&fence).running_jobs, 2);
    assert!(!scheduler.is_fence_drained(&fence));
    assert!(!previous_token.is_cancelled());
    assert!(!current_token.is_cancelled());

    for rejected_scope in [previous.clone(), current.clone()] {
        assert!(matches!(
            scheduler.submit(
                request(
                    rejected_scope,
                    ExtensionJobClassV1::Cpu,
                    JobPriority::Prefetch,
                    3,
                    RequestDeadline::none(),
                    CancellationToken::new(),
                ),
                now,
            ),
            ExtensionScheduleOutcomeV1::ScopeClosed(_)
        ));
    }
    let newer = scope("package", "feature", 8);
    let _newer_id = queued(scheduler.submit(
        request(
            newer.clone(),
            ExtensionJobClassV1::Cpu,
            JobPriority::Prefetch,
            4,
            RequestDeadline::none(),
            CancellationToken::new(),
        ),
        now,
    ));

    let report = cancellation.actions.signal_all();
    assert_eq!(report.signalled_tokens, 2);
    assert_eq!(report.panicked_callbacks, 0);
    assert!(previous_token.is_cancelled());
    assert!(current_token.is_cancelled());
    assert_eq!(
        complete(&mut scheduler, previous_id, now),
        ExtensionCompletionOutcomeV1::Cancelled
    );
    assert_eq!(scheduler.fence_activity(&fence).running_jobs, 1);
    assert!(!scheduler.is_fence_drained(&fence));
    assert_eq!(
        complete(&mut scheduler, current_id, now),
        ExtensionCompletionOutcomeV1::Cancelled
    );
    assert!(scheduler.is_fence_drained(&fence));
    for rejected_scope in [previous, current] {
        assert!(matches!(
            scheduler.submit(
                request(
                    rejected_scope,
                    ExtensionJobClassV1::Cpu,
                    JobPriority::Prefetch,
                    9,
                    RequestDeadline::none(),
                    CancellationToken::new(),
                ),
                now,
            ),
            ExtensionScheduleOutcomeV1::ScopeClosed(_)
        ));
    }
}

#[test]
fn cancellation_actions_signal_only_after_releasing_an_external_mutex_guard() {
    let now = Instant::now();
    let scheduler = Arc::new(Mutex::new(ExtensionJobSchedulerV1::new(config(
        limits(2, 2, 1, 1),
        limits(1, 1, 1, 1),
        1,
    ))));
    let token = CancellationToken::new();
    let callback_scheduler = Arc::clone(&scheduler);
    let callback_ran = Arc::new(AtomicBool::new(false));
    let callback_ran_from_callback = Arc::clone(&callback_ran);
    let _registration = token.register(move || {
        let scheduler = callback_scheduler
            .try_lock()
            .expect("callback must re-lock only after scheduler guard is dropped");
        let _ = scheduler.stats(ExtensionJobClassV1::Cpu);
        callback_ran_from_callback.store(true, Ordering::Release);
    });
    let job_id = {
        let mut guard = scheduler.lock().expect("scheduler lock");
        queued(guard.submit(
            request(
                scope("package", "feature", 1),
                ExtensionJobClassV1::Cpu,
                JobPriority::Prefetch,
                1,
                RequestDeadline::none(),
                token.clone(),
            ),
            now,
        ))
    };
    let actions = {
        let mut guard = scheduler.lock().expect("scheduler lock");
        let cancellation = guard.cancel(job_id);
        assert_eq!(cancellation.affected_jobs, 1);
        assert!(!token.is_cancelled());
        cancellation.actions
    };
    let report = actions.signal_all();
    assert_eq!(report.signalled_tokens, 1);
    assert_eq!(report.panicked_callbacks, 0);
    assert!(token.is_cancelled());
    assert!(callback_ran.load(Ordering::Acquire));
}

#[test]
fn same_token_panic_does_not_block_its_later_callback_or_signal_report() {
    let now = Instant::now();
    let mut scheduler =
        ExtensionJobSchedulerV1::new(config(limits(1, 1, 1, 1), limits(1, 1, 1, 1), 1));
    let token = CancellationToken::new();
    let _panicking_registration = token.register(|| panic!("hostile callback"));
    let later_callback_count = Arc::new(AtomicUsize::new(0));
    let later_callback_count_from_callback = Arc::clone(&later_callback_count);
    let _later_registration = token.register(move || {
        later_callback_count_from_callback.fetch_add(1, Ordering::AcqRel);
    });
    let job_id = queued(scheduler.submit(
        request(
            scope("package", "feature", 1),
            ExtensionJobClassV1::Cpu,
            JobPriority::Prefetch,
            1,
            RequestDeadline::none(),
            token.clone(),
        ),
        now,
    ));
    let cancellation = scheduler.cancel(job_id);
    assert_eq!(cancellation.affected_jobs, 1);
    let report = cancellation.actions.signal_all();
    assert_eq!(report.signalled_tokens, 1);
    assert_eq!(report.callbacks_invoked, 2);
    assert_eq!(report.panicked_callbacks, 1);
    assert_eq!(later_callback_count.load(Ordering::Acquire), 1);
    assert!(token.is_cancelled());
}

#[test]
fn start_poll_returns_expired_queue_actions_for_unlock_then_signal() {
    let now = Instant::now();
    let mut scheduler =
        ExtensionJobSchedulerV1::new(config(limits(1, 1, 1, 1), limits(1, 1, 1, 1), 1));
    let token = CancellationToken::new();
    let callback_ran = Arc::new(AtomicBool::new(false));
    let callback_ran_from_callback = Arc::clone(&callback_ran);
    let _registration = token.register(move || {
        callback_ran_from_callback.store(true, Ordering::Release);
    });
    let deadline = RequestDeadline::after(now, Duration::from_millis(1)).expect("deadline");
    let _ = queued(scheduler.submit(
        request(
            scope("package", "feature", 1),
            ExtensionJobClassV1::Cpu,
            JobPriority::VisibleViewport,
            1,
            deadline,
            token.clone(),
        ),
        now,
    ));
    let poll = scheduler.try_start(ExtensionJobClassV1::Cpu, now + Duration::from_millis(2));
    assert!(poll.started.is_none());
    assert_eq!(scheduler.stats(ExtensionJobClassV1::Cpu).queued_jobs, 0);
    assert_eq!(
        scheduler
            .stats(ExtensionJobClassV1::Cpu)
            .expired_queued_jobs,
        1
    );
    assert!(!token.is_cancelled());
    assert!(!callback_ran.load(Ordering::Acquire));
    let report = poll.actions.signal_all();
    assert_eq!(report.signalled_tokens, 1);
    assert_eq!(report.callbacks_invoked, 1);
    assert_eq!(report.panicked_callbacks, 0);
    assert!(token.is_cancelled());
    assert!(callback_ran.load(Ordering::Acquire));
}

#[test]
fn sustained_all_lane_arrivals_keep_every_lower_lane_within_a_finite_burst_gap() {
    const BURST: usize = 2;
    const MAX_LOWER_DISPATCH_GAP: usize = 3 * (BURST + 1);
    let now = Instant::now();
    let mut scheduler =
        ExtensionJobSchedulerV1::new(config(limits(32, 32, 1, 1), limits(1, 1, 1, 1), BURST));
    let work = scope("package", "feature", 1);
    for (priority, payload) in [
        (JobPriority::VisibleViewport, 0),
        (JobPriority::CurrentDirectory, 1),
        (JobPriority::Prefetch, 2),
        (JobPriority::Maintenance, 3),
    ] {
        let _ = queued(scheduler.submit(
            request(
                work.clone(),
                ExtensionJobClassV1::Cpu,
                priority,
                payload,
                RequestDeadline::none(),
                CancellationToken::new(),
            ),
            now,
        ));
    }

    let mut last_lower_start = [None; 3];
    let mut lower_start_count = [0_usize; 3];
    for dispatch in 0_usize..36 {
        let started = poll_started(&mut scheduler, ExtensionJobClassV1::Cpu, now)
            .expect("continuously replenished lane must start");
        let lane = started.payload % 4;
        if lane != 0 {
            let lower_index = lane - 1;
            if let Some(previous) = last_lower_start[lower_index] {
                assert!(
                    dispatch - previous <= MAX_LOWER_DISPATCH_GAP,
                    "lane {lane} exceeded its finite dispatch gap"
                );
            }
            last_lower_start[lower_index] = Some(dispatch);
            lower_start_count[lower_index] += 1;
        }
        let priority = match lane {
            0 => JobPriority::VisibleViewport,
            1 => JobPriority::CurrentDirectory,
            2 => JobPriority::Prefetch,
            3 => JobPriority::Maintenance,
            _ => unreachable!("payload lane is modulo four"),
        };
        assert_eq!(
            complete(&mut scheduler, started.job_id, now),
            ExtensionCompletionOutcomeV1::Completed
        );
        let _ = queued(scheduler.submit(
            request(
                work.clone(),
                ExtensionJobClassV1::Cpu,
                priority,
                started.payload + 4,
                RequestDeadline::none(),
                CancellationToken::new(),
            ),
            now,
        ));
    }
    for count in lower_start_count {
        assert!(count >= 3, "each lower lane must make repeated progress");
    }
}

#[test]
fn repeated_capacity_one_submit_cancel_preserves_public_queue_and_scope_invariants() {
    let now = Instant::now();
    let mut scheduler =
        ExtensionJobSchedulerV1::new(config(limits(1, 1, 1, 1), limits(1, 1, 1, 1), 1));
    let work = scope("package", "feature", 1);
    for payload in 0..1_000 {
        let token = CancellationToken::new();
        let job_id = queued(scheduler.submit(
            request(
                work.clone(),
                ExtensionJobClassV1::Cpu,
                JobPriority::Prefetch,
                payload,
                RequestDeadline::none(),
                token.clone(),
            ),
            now,
        ));
        let cancellation = scheduler.cancel(job_id);
        assert_eq!(cancellation.affected_jobs, 1);
        assert_eq!(scheduler.stats(ExtensionJobClassV1::Cpu).queued_jobs, 0);
        assert_eq!(scheduler.stats(ExtensionJobClassV1::Cpu).running_jobs, 0);
        assert!(scheduler.is_drained(&work));
        assert!(!token.is_cancelled());
        assert_eq!(cancellation.actions.signal_all().signalled_tokens, 1);
        assert!(token.is_cancelled());
    }
}

#[test]
fn running_deadline_expiry_signals_the_token_before_worker_completion() {
    let now = Instant::now();
    let mut scheduler =
        ExtensionJobSchedulerV1::new(config(limits(2, 2, 1, 1), limits(1, 1, 1, 1), 1));
    let token = CancellationToken::new();
    let deadline = RequestDeadline::after(now, Duration::from_millis(1)).expect("deadline");
    let job_id = queued(scheduler.submit(
        request(
            scope("package", "feature", 1),
            ExtensionJobClassV1::Cpu,
            JobPriority::VisibleViewport,
            1,
            deadline,
            token.clone(),
        ),
        now,
    ));
    assert_eq!(
        poll_started(&mut scheduler, ExtensionJobClassV1::Cpu, now).map(|job| job.job_id),
        Some(job_id)
    );
    let expiration = scheduler.expire(now + Duration::from_millis(2));
    assert_eq!(expiration.affected_jobs, 1);
    assert!(!token.is_cancelled());
    assert_eq!(expiration.actions.signal_all().signalled_tokens, 1);
    assert!(token.is_cancelled());
    assert_eq!(
        complete(&mut scheduler, job_id, now + Duration::from_millis(2)),
        ExtensionCompletionOutcomeV1::DeadlineExceeded
    );
}

#[test]
fn large_dispatch_with_batch_cancel_and_expire_finishes_with_clean_accounting() {
    const NORMAL_JOBS: usize = 300;
    const CANCELLED_JOBS: usize = 120;
    const EXPIRED_JOBS: usize = 120;
    let now = Instant::now();
    let mut scheduler =
        ExtensionJobSchedulerV1::new(config(limits(600, 600, 8, 8), limits(1, 1, 1, 1), 3));
    let normal_scope = scope("normal", "feature", 1);
    let cancelled_scope = scope("cancelled", "feature", 1);
    let expired_scope = scope("expired", "feature", 1);
    for payload in 0..NORMAL_JOBS {
        let priority = match payload % 4 {
            0 => JobPriority::VisibleViewport,
            1 => JobPriority::CurrentDirectory,
            2 => JobPriority::Prefetch,
            _ => JobPriority::Maintenance,
        };
        let _ = queued(scheduler.submit(
            request(
                normal_scope.clone(),
                ExtensionJobClassV1::Cpu,
                priority,
                payload,
                RequestDeadline::none(),
                CancellationToken::new(),
            ),
            now,
        ));
    }
    for payload in 0..CANCELLED_JOBS {
        let _ = queued(scheduler.submit(
            request(
                cancelled_scope.clone(),
                ExtensionJobClassV1::Cpu,
                JobPriority::Prefetch,
                NORMAL_JOBS + payload,
                RequestDeadline::none(),
                CancellationToken::new(),
            ),
            now,
        ));
    }
    let expiry = RequestDeadline::after(now, Duration::from_millis(1)).expect("deadline");
    for payload in 0..EXPIRED_JOBS {
        let _ = queued(scheduler.submit(
            request(
                expired_scope.clone(),
                ExtensionJobClassV1::Cpu,
                JobPriority::Maintenance,
                NORMAL_JOBS + CANCELLED_JOBS + payload,
                expiry,
                CancellationToken::new(),
            ),
            now,
        ));
    }
    let cancellation = scheduler.cancel_scope(&cancelled_scope);
    assert_eq!(cancellation.affected_jobs, CANCELLED_JOBS);
    assert_eq!(
        cancellation.actions.signal_all().signalled_tokens,
        CANCELLED_JOBS
    );
    let expiration = scheduler.expire(now + Duration::from_millis(2));
    assert_eq!(expiration.affected_jobs, EXPIRED_JOBS);
    assert_eq!(
        expiration.actions.signal_all().signalled_tokens,
        EXPIRED_JOBS
    );
    assert!(scheduler.is_drained(&cancelled_scope));
    assert!(scheduler.is_drained(&expired_scope));

    let mut completed = 0_usize;
    while let Some(started) = poll_started(&mut scheduler, ExtensionJobClassV1::Cpu, now) {
        assert_eq!(
            complete(&mut scheduler, started.job_id, now),
            ExtensionCompletionOutcomeV1::Completed
        );
        completed += 1;
    }
    assert_eq!(completed, NORMAL_JOBS);
    let stats = scheduler.stats(ExtensionJobClassV1::Cpu);
    assert_eq!(stats.queued_jobs, 0);
    assert_eq!(stats.running_jobs, 0);
    assert_eq!(stats.cancelled_queued_jobs, CANCELLED_JOBS as u64);
    assert_eq!(stats.expired_queued_jobs, EXPIRED_JOBS as u64);
    assert!(scheduler.is_drained(&normal_scope));
}
