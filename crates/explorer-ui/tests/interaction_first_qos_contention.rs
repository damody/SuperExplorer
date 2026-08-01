//! Deterministic contention coverage assembled from the public UI/model protocol.
//!
//! The production UI pump owns its internal queues, while the real Shell worker gates are
//! deliberately test-private. This test therefore drives the public command/event boundary with
//! explicit ticks and frame gates; it does not use filesystem paths, clocks, or provider timing.

use std::{collections::VecDeque, time::Duration};

use explorer_jobs::FrameDrainBudget;
use explorer_model::{
    ExplorerCommand, ExplorerEvent, ExplorerWindowState, Generation, HistoryEntry,
    LocationDescriptor, LocationMetadata, RequestContext, SearchInput, TabCloseOutcome, TabId,
    WindowEventOutcome,
};
use explorer_test_support::{DeterministicShellService, synthetic_directory_entries};
use explorer_ui::qos::UiDeliveryCounters;

#[derive(Debug, Eq, PartialEq)]
struct ContentionDiagnostics {
    integrated: u64,
    deferred: usize,
    exhausted_frames: u64,
    stale_events: usize,
}

fn initial_history() -> HistoryEntry {
    HistoryEntry::new(
        LocationDescriptor::file_system(r"C:\qos-fixture"),
        "initial",
    )
}

fn navigation_command(context: RequestContext, suffix: &str) -> ExplorerCommand {
    ExplorerCommand::Navigate {
        context,
        location: LocationDescriptor::file_system(format!(r"C:\qos-fixture\{suffix}")),
    }
}

#[test]
fn interaction_first_qos_contention_bounded_frames_interleave_replacement_with_completion_burst() {
    let frame_budget = FrameDrainBudget::new(2, Duration::from_millis(16));
    let mut counters = UiDeliveryCounters::default();
    let mut window = ExplorerWindowState::new(initial_history());
    let mut service = DeterministicShellService::default();

    // Explicit delivery gate: old work completes into the service at tick one, but the next user
    // navigation is admitted before that tick's completion burst is integrated by any UI frame.
    let superseded = window
        .active_tab_mut()
        .begin_navigation_request()
        .expect("first navigation request");
    service
        .submit(navigation_command(superseded.clone(), "superseded"))
        .expect("queue first navigation");
    service.schedule(
        1,
        ExplorerEvent::LocationResolved {
            context: superseded.clone(),
            metadata: LocationMetadata {
                descriptor: LocationDescriptor::file_system(r"C:\qos-fixture\superseded"),
                display_title: "superseded".to_owned(),
                can_go_up: true,
                can_write: true,
            },
        },
    );
    service.schedule(
        1,
        ExplorerEvent::DirectoryBatch {
            context: superseded.clone(),
            entries: synthetic_directory_entries(96),
        },
    );
    service.schedule(
        1,
        ExplorerEvent::DirectoryFinished {
            context: superseded.clone(),
        },
    );

    let mut pending = VecDeque::from(service.advance().expect("release completion gate"));
    assert_eq!(
        pending.len(),
        3,
        "the deterministic burst reached the UI boundary"
    );

    let current = window
        .active_tab_mut()
        .begin_navigation_request()
        .expect("replacement input remains available");
    assert!(
        superseded.cancellation.is_cancelled(),
        "replacement must synchronously cancel the superseded request"
    );
    service
        .submit(navigation_command(current.clone(), "current"))
        .expect("input admission must not wait for queued completions");
    assert!(matches!(
        service.pop_command(),
        Some(ExplorerCommand::Navigate { context, .. }) if context.request_id == superseded.request_id
    ));
    assert!(matches!(
        service.pop_command(),
        Some(ExplorerCommand::Navigate { context, .. }) if context.request_id == current.request_id
    ));

    service.schedule(
        1,
        ExplorerEvent::LocationResolved {
            context: current.clone(),
            metadata: LocationMetadata {
                descriptor: LocationDescriptor::file_system(r"C:\qos-fixture\current"),
                display_title: "current".to_owned(),
                can_go_up: true,
                can_write: true,
            },
        },
    );
    service.schedule(
        1,
        ExplorerEvent::DirectoryBatch {
            context: current.clone(),
            entries: synthetic_directory_entries(96),
        },
    );
    service.schedule(
        1,
        ExplorerEvent::DirectoryFinished {
            context: current.clone(),
        },
    );

    let mut stale_events = 0_usize;
    let first_frame = frame_budget.drain(&mut pending, || Duration::ZERO);
    assert_eq!(first_frame.items.len(), 2, "first frame is item-bounded");
    for event in first_frame.items {
        stale_events += usize::from(window.apply_event(event) == WindowEventOutcome::IgnoredStale);
    }
    counters.record_drain(2, pending.len(), true);

    pending.extend(service.advance().expect("release current navigation gate"));
    while !pending.is_empty() {
        let drained = frame_budget.drain(&mut pending, || Duration::ZERO);
        let integrated = drained.items.len();
        for event in drained.items {
            stale_events +=
                usize::from(window.apply_event(event) == WindowEventOutcome::IgnoredStale);
        }
        counters.record_drain(integrated, pending.len(), !pending.is_empty());
    }

    let diagnostics = ContentionDiagnostics {
        integrated: counters.integrated_results(),
        deferred: counters.deferred_results(),
        exhausted_frames: counters.frame_budget_exhaustions(),
        stale_events,
    };
    assert_eq!(
        diagnostics,
        ContentionDiagnostics {
            integrated: 6,
            deferred: 0,
            exhausted_frames: 2,
            stale_events: 3,
        },
        "privacy-safe QoS diagnostics"
    );
    assert_eq!(window.active_presentation().address_title, "current");
    assert_eq!(window.active_presentation().item_count, 96);
    service
        .verify_drained()
        .expect("all navigation terminals delivered once");
}

#[test]
fn interaction_first_qos_contention_tab_close_and_shutdown_cancel_active_protocol_work() {
    let mut window = ExplorerWindowState::new(initial_history());
    let closing_tab = window.new_tab();
    let navigation = window
        .active_tab_mut()
        .begin_navigation_request()
        .expect("active tab navigation");
    assert_eq!(window.close(closing_tab), TabCloseOutcome::Closed);
    assert!(navigation.cancellation.is_cancelled());
    assert_eq!(
        window.apply_event(ExplorerEvent::DirectoryFinished {
            context: navigation,
        }),
        WindowEventOutcome::IgnoredStale,
        "a closed tab cannot be mutated by a late result"
    );

    // Explicit shutdown gate: retain two active protocol requests, then close the endpoint and
    // require exactly one path-free terminal for each without waiting on a worker clock.
    let mut service = DeterministicShellService::default();
    let search = RequestContext::new(TabId::new(), Generation::new(1));
    let enrichment = RequestContext::new(TabId::new(), Generation::new(1));
    service
        .submit(ExplorerCommand::StartSearch {
            context: search.clone(),
            location: LocationDescriptor::file_system(r"C:\qos-fixture"),
            input: SearchInput::new("held"),
        })
        .expect("queue search");
    service
        .submit(ExplorerCommand::ResolveAncestry {
            context: enrichment.clone(),
            location: LocationDescriptor::file_system(r"C:\qos-fixture"),
        })
        .expect("queue enrichment");

    let terminals = service.close_channel().expect("shutdown protocol endpoint");
    assert_eq!(terminals.len(), 2);
    assert!(terminals.iter().any(|event| matches!(
        event,
        ExplorerEvent::Failed { context, .. } if context.request_id == search.request_id
    )));
    assert!(terminals.iter().any(|event| matches!(
        event,
        ExplorerEvent::AncestryFinished { context, .. } if context.request_id == enrichment.request_id
    )));
    service
        .verify_drained()
        .expect("shutdown leaves no retained request");
}
