use std::{sync::Arc, time::Duration};

use explorer_common::RoadmapLimits;
use explorer_extension_broker::{QuarantineRegistry, TerminalGate};
use explorer_jobs::{PreviewCoordinator, PreviewCoordinatorAction, ThumbnailMemoryCache};
use explorer_model::{
    ExplorerWindowState, Generation, HistoryEntry, LocationDescriptor, PersistedRect,
    PersistedSessionEnvelope, PersistedWindowPlacement, PreviewEligibility, PreviewSelection,
    SessionProvenance, SessionStore as _, ShellIconTheme, ShellItemId, ThumbnailMode,
    ThumbnailPixels, ThumbnailRequestKey,
};

fn placement() -> PersistedWindowPlacement {
    let bounds = PersistedRect {
        left: 20,
        top: 30,
        width: 1120,
        height: 720,
    };
    PersistedWindowPlacement {
        normal_bounds: bounds,
        source_work_area: PersistedRect {
            left: 0,
            top: 0,
            width: 1920,
            height: 1080,
        },
        source_dpi: 96,
        maximized: false,
    }
}

fn provenance() -> SessionProvenance {
    SessionProvenance {
        app_version: "test".to_owned(),
        app_revision: "combined".to_owned(),
        windows_build: "fixture".to_owned(),
    }
}

fn thumbnail_key() -> ThumbnailRequestKey {
    ThumbnailRequestKey {
        item_id: ShellItemId::from_provider_bytes([9]).expect("item"),
        physical_size: 96,
        dpi: 96,
        mode: ThumbnailMode::Thumbnail,
        source_generation: 1,
        theme: ShellIconTheme::Light,
        association_generation: 1,
        overlay_generation: 1,
    }
}

#[test]
fn combined_restore_namespace_thumbnail_broker_preview_and_save_flow() {
    let limits = RoadmapLimits::default();
    let root = tempfile::tempdir().expect("state root");
    let store = explorer_app::session_store::WindowsSessionStore::at_root(
        root.path().to_path_buf(),
        limits,
    );
    let mut window = ExplorerWindowState::new(HistoryEntry::new(
        LocationDescriptor::ParsingName("shell:MyComputerFolder".to_owned()),
        "This PC",
    ));
    window.active_tab_mut().view.settings.preview_pane = true;
    let first =
        PersistedSessionEnvelope::project(&window, placement(), &[], true, 1, provenance(), limits)
            .expect("project initial session");
    store.save(&first).expect("save initial session");
    let restored = store
        .load()
        .expect("load session")
        .envelope
        .expect("saved envelope")
        .restore_plan(limits)
        .expect("restore plan");
    assert_eq!(restored.tabs.len(), 1);
    assert!(restored.tabs[0].view_settings.preview_pane);
    let restored_window = restored
        .resolve_window(
            HistoryEntry::new(LocationDescriptor::file_system(r"C:\"), "C:"),
            |location| Some(HistoryEntry::new(location.clone(), "restored")),
        )
        .expect("resolve restored window");

    let mut cache = ThumbnailMemoryCache::new(64, 4);
    let pixels = Arc::new(ThumbnailPixels {
        width: 2,
        height: 2,
        stride: 8,
        bytes: vec![0; 16],
    });
    cache.insert(thumbnail_key(), pixels);
    assert!(cache.get(&thumbnail_key()).is_some());

    let terminal = TerminalGate::default();
    assert!(terminal.claim());
    assert!(!terminal.claim());

    let mut preview = PreviewCoordinator::new(Duration::from_millis(10));
    preview.open().expect("open preview pane");
    preview
        .select(
            &PreviewEligibility::SingleEligible(PreviewSelection {
                item_id: ShellItemId::from_provider_bytes([9]).expect("preview item"),
                location: LocationDescriptor::file_system(r"C:\fixture.txt"),
                display_name: "fixture.txt".to_owned(),
            }),
            Duration::ZERO,
        )
        .expect("select preview item");
    let PreviewCoordinatorAction::Start { generation, .. } = preview
        .poll(Duration::from_millis(10))
        .expect("poll preview")
        .expect("preview start")
    else {
        panic!("expected preview start")
    };
    assert!(preview.finish(generation, true, false));

    let second = PersistedSessionEnvelope::project(
        &restored_window,
        restored.window,
        &restored.quick_access,
        true,
        2,
        provenance(),
        limits,
    )
    .expect("project restored session");
    store.save(&second).expect("save restored session");
    assert_eq!(
        store
            .load()
            .expect("reload")
            .envelope
            .expect("envelope")
            .write_generation,
        2
    );
}

#[test]
fn combined_corruption_crash_quarantine_and_preview_failure_remain_recoverable() {
    let limits = RoadmapLimits::default();
    let root = tempfile::tempdir().expect("state root");
    let store = explorer_app::session_store::WindowsSessionStore::at_root(
        root.path().to_path_buf(),
        limits,
    );
    std::fs::create_dir_all(root.path()).expect("root");
    std::fs::write(root.path().join("session.json"), b"{corrupt").expect("corrupt session fixture");
    assert!(
        store
            .load()
            .expect("recover corrupt state")
            .envelope
            .is_none()
    );

    let malformed = ThumbnailPixels {
        width: 2,
        height: 2,
        stride: 8,
        bytes: vec![0; 15],
    };
    assert!(malformed.validate(limits.thumbnail_memory_bytes).is_err());

    let now = std::time::Instant::now();
    let mut quarantine = QuarantineRegistry::new(2, Duration::from_secs(1), 8);
    assert!(!quarantine.record_failure("handler-digest".to_owned(), now));
    assert!(quarantine.record_failure("handler-digest".to_owned(), now));
    assert!(quarantine.is_quarantined("handler-digest", now));

    let mut preview = PreviewCoordinator::new(Duration::from_millis(1));
    preview.open().expect("open preview");
    preview
        .select(
            &PreviewEligibility::SingleEligible(PreviewSelection {
                item_id: ShellItemId::from_provider_bytes([1]).expect("item"),
                location: LocationDescriptor::file_system(r"C:\unsupported.bin"),
                display_name: "unsupported.bin".to_owned(),
            }),
            Duration::ZERO,
        )
        .expect("select");
    let PreviewCoordinatorAction::Start { generation, .. } = preview
        .poll(Duration::from_millis(1))
        .expect("poll")
        .expect("start")
    else {
        panic!("expected start")
    };
    assert!(preview.finish(generation, false, true));

    let filesystem = ExplorerWindowState::new(HistoryEntry::new(
        LocationDescriptor::file_system(r"C:\"),
        "C:",
    ));
    assert_eq!(filesystem.active_tab().generation, Generation::default());
    assert!(filesystem.active_tab().history.current().is_some());
}
