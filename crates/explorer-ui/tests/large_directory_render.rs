use std::time::{Duration, Instant};

use explorer_ui::{
    ExplorerRoot, UiTokens,
    actions::{ActionSource, ExplorerAction},
    chrome::FILE_VIEW_HOST_ID,
};
use gpui::{AppContext as _, TestAppContext, VisualTestContext, px, size};

#[gpui::test]
#[cfg_attr(
    debug_assertions,
    ignore = "release-only 100,000-entry GPUI render and scroll performance gate"
)]
#[allow(
    clippy::cast_precision_loss,
    reason = "bounded deterministic test offsets remain within practical GPUI scroll precision"
)]
fn release_renders_and_scrolls_one_hundred_thousand_rows_in_both_view_families(
    cx: &mut TestAppContext,
) {
    for mode in [
        explorer_model::ViewMode::Details,
        explorer_model::ViewMode::MediumIcons,
    ] {
        let entries = explorer_test_support::synthetic_directory_entries(100_000);
        let window = cx.open_window(size(px(1_120.0), px(720.0)), move |_, _| {
            ExplorerRoot::for_directory_fixture(UiTokens::default(), entries, mode)
        });
        let any_window = window.into();
        cx.update_window(any_window, |_, window, cx| window.draw(cx).clear())
            .expect("initial large-directory draw succeeds");
        let mut visual = VisualTestContext::from_window(any_window, cx);
        assert!(visual.debug_bounds(FILE_VIEW_HOST_ID).is_some());
        assert!(visual.debug_bounds("file-row-250").is_none());
        let initial = cx
            .update_window(any_window, |root_view, _, cx| {
                let root = root_view
                    .downcast::<ExplorerRoot>()
                    .expect("Explorer root type");
                root.read(cx).file_performance_snapshot_for_test()
            })
            .expect("read performance counters");
        assert!(initial.realized_items > 0);
        assert!(initial.realized_items <= 250);

        let mut frames = Vec::with_capacity(40);
        for sample in 0_u32..40 {
            let offset = (sample.saturating_mul(7_919) % 2_300_000) as f32;
            let started = Instant::now();
            cx.update_window(any_window, |root_view, window, cx| {
                let root = root_view
                    .downcast::<ExplorerRoot>()
                    .expect("Explorer root type");
                root.update(cx, |root, cx| {
                    root.set_file_scroll_offset_for_test(offset);
                    cx.notify();
                });
                window.draw(cx).clear();
            })
            .expect("scroll draw succeeds");
            frames.push(started.elapsed());
        }
        frames.sort_unstable();
        let p95 = frames[frames.len() * 95 / 100];
        let maximum = *frames.last().expect("frame samples");
        println!(
            "{mode:?}: realized={}, p95={p95:?}, max={maximum:?}",
            initial.realized_items
        );
        assert!(p95 <= Duration::from_micros(16_700), "{mode:?} p95={p95:?}");
        assert!(
            maximum <= Duration::from_millis(100),
            "{mode:?} maximum={maximum:?}"
        );
    }
}

#[gpui::test]
#[cfg_attr(
    debug_assertions,
    ignore = "release-only first-batch input latency performance gate"
)]
fn release_keeps_pointer_and_keyboard_input_under_budget_while_loading(cx: &mut TestAppContext) {
    let entries = explorer_test_support::synthetic_directory_entries(64);
    let window = cx.open_window(size(px(1_120.0), px(720.0)), move |_, _| {
        ExplorerRoot::for_loading_directory_fixture(
            UiTokens::default(),
            entries,
            explorer_model::ViewMode::Details,
        )
    });
    let any_window = window.into();
    cx.update_window(any_window, |_, window, cx| window.draw(cx).clear())
        .expect("first batch draw succeeds");
    cx.update_window(any_window, |root_view, window, cx| {
        let root = root_view
            .downcast::<ExplorerRoot>()
            .expect("Explorer root type");
        root.update(cx, |root, cx| {
            for sample in 0..100 {
                root.dispatch_action_for_test(
                    ExplorerAction::SelectItem {
                        row_index: sample % 64,
                    },
                    if sample % 2 == 0 {
                        ActionSource::Mouse
                    } else {
                        ActionSource::Keyboard
                    },
                    window,
                    cx,
                );
            }
        });
        window.draw(cx).clear();
    })
    .expect("input actions remain available");
    let snapshot = cx
        .update_window(any_window, |root_view, _, cx| {
            let root = root_view
                .downcast::<ExplorerRoot>()
                .expect("Explorer root type");
            root.read(cx).file_performance_snapshot_for_test()
        })
        .expect("read input distribution");
    let input = snapshot.input.expect("input samples");
    println!(
        "loading input: p95={:?}, max={:?}",
        input.p95, input.maximum
    );
    assert!(input.p95 <= Duration::from_millis(50));
}
