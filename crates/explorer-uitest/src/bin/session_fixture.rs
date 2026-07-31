use std::{env, fs, path::PathBuf};

use anyhow::{Context as _, Result, bail};
use explorer_common::RoadmapLimits;
use explorer_model::{
    LocationDescriptor, PersistedHistoryEntry, PersistedRect, PersistedSessionEnvelope,
    PersistedSessionPayload, PersistedTab, PersistedViewSettings, PersistedWindowPlacement,
    SessionProvenance, TabId,
};

fn history(location: LocationDescriptor, title: impl Into<String>) -> PersistedHistoryEntry {
    PersistedHistoryEntry {
        location,
        display_title: title.into(),
        anchor_item: None,
        anchor_offset_logical_pixels: 0,
    }
}

fn main() -> Result<()> {
    let mut args = env::args_os().skip(1);
    let Some(output) = args.next().map(PathBuf::from) else {
        bail!("usage: explorer-session-fixture <output> <drive-designator> <other-tab-path>");
    };
    let Some(drive) = args.next().map(PathBuf::from) else {
        bail!("missing drive designator");
    };
    let Some(other) = args.next().map(PathBuf::from) else {
        bail!("missing other tab path");
    };
    if args.next().is_some() {
        bail!("unexpected extra argument");
    }

    let first_id = TabId::new();
    let active_id = TabId::new();
    let view_settings = PersistedViewSettings {
        file_name_extensions: true,
        ..PersistedViewSettings::default()
    };
    let payload = PersistedSessionPayload {
        restore_enabled: true,
        window: PersistedWindowPlacement {
            normal_bounds: PersistedRect {
                left: 96,
                top: 72,
                width: 1426,
                height: 873,
            },
            source_work_area: PersistedRect {
                left: 0,
                top: 0,
                width: 2194,
                height: 1234,
            },
            source_dpi: 168,
            maximized: false,
        },
        tabs: vec![
            PersistedTab {
                tab_id: first_id,
                current: history(LocationDescriptor::file_system(&other), "fixture"),
                back: Vec::new(),
                forward: Vec::new(),
                view_settings: view_settings.clone(),
            },
            PersistedTab {
                tab_id: active_id,
                current: history(LocationDescriptor::file_system(&drive), "mapped drive"),
                back: vec![history(LocationDescriptor::file_system(other), "fixture")],
                forward: Vec::new(),
                view_settings,
            },
        ],
        active_tab_id: active_id,
        quick_access: Vec::new(),
    };
    let limits = RoadmapLimits::default();
    let envelope = PersistedSessionEnvelope::new(
        1,
        SessionProvenance {
            app_version: env!("CARGO_PKG_VERSION").to_owned(),
            app_revision: "uitest-fixture".to_owned(),
            windows_build: "Windows_NT".to_owned(),
        },
        payload,
        limits,
    )?;
    let parent = output.parent().context("session output has no parent")?;
    fs::create_dir_all(parent).context("create session fixture parent")?;
    fs::write(&output, envelope.encode_pretty(limits)?).context("write session fixture")?;
    Ok(())
}
