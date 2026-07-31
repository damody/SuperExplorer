//! Per-script configured folder watcher.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use explorer_automation::{
    AutomationError, AutomationErrorKind, AutomationEventData, AutomationResult, CorrelationId,
    EventBridge, EventContext, EventSource, WatchRegistration,
};
use globset::{Glob, GlobSet, GlobSetBuilder};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

/// Owns one native watcher for one Lua `watch` declaration.
pub struct FolderWatchService {
    watcher: RecommendedWatcher,
    root: PathBuf,
    bridge: Arc<EventBridge>,
}

impl FolderWatchService {
    /// Starts watching immediately with recursive and glob settings from the script.
    ///
    /// # Errors
    ///
    /// Returns a typed error for invalid globs or a native watcher failure.
    pub fn start(
        registration: &WatchRegistration,
        bridge: Arc<EventBridge>,
    ) -> AutomationResult<Self> {
        let include = build_globs(&registration.include)?;
        let exclude = build_globs(&registration.exclude)?;
        let root = registration.root.clone();
        let callback_root = root.clone();
        let callback_bridge = Arc::clone(&bridge);
        let mut watcher = notify::recommended_watcher(move |result| match result {
            Ok(event) => {
                publish_event(&callback_bridge, &callback_root, &include, &exclude, &event);
            }
            Err(_) => publish_meta(&callback_bridge, &callback_root, "watch.error"),
        })
        .map_err(|_| watch_error("watch.start"))?;
        watcher
            .watch(
                &root,
                if registration.recursive {
                    RecursiveMode::Recursive
                } else {
                    RecursiveMode::NonRecursive
                },
            )
            .map_err(|_| watch_error("watch.start"))?;
        publish_meta(&bridge, &root, "watch.started");
        Ok(Self {
            watcher,
            root,
            bridge,
        })
    }

    /// Stops the native watch before dropping the service.
    pub fn stop(mut self) {
        let _ = self.watcher.unwatch(&self.root);
        publish_meta(&self.bridge, &self.root, "watch.stopped");
    }
}

impl std::fmt::Debug for FolderWatchService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FolderWatchService")
            .field("root", &self.root)
            .finish_non_exhaustive()
    }
}

fn publish_event(
    bridge: &EventBridge,
    root: &Path,
    include: &GlobSet,
    exclude: &GlobSet,
    event: &Event,
) {
    if event.need_rescan() {
        publish_meta(bridge, root, "watch.overflow");
        return;
    }
    let name = match event.kind {
        EventKind::Create(_) => "fs.created",
        EventKind::Modify(notify::event::ModifyKind::Name(_)) => "fs.renamed",
        EventKind::Modify(notify::event::ModifyKind::Metadata(_)) => "fs.attributes_changed",
        EventKind::Modify(_) => "fs.modified",
        EventKind::Remove(_) => "fs.removed",
        _ => return,
    };
    let previous_path =
        (name == "fs.renamed" && event.paths.len() > 1).then(|| event.paths[0].clone());
    let paths = if previous_path.is_some() {
        &event.paths[1..]
    } else {
        &event.paths[..]
    };
    for path in paths {
        let relative = path.strip_prefix(root).unwrap_or(path);
        if (!include.is_empty() && !include.is_match(relative)) || exclude.is_match(relative) {
            continue;
        }
        let _ = bridge.emit(
            name,
            now_ms(),
            EventSource::FileSystem,
            context(Some(root.to_path_buf())),
            AutomationEventData::Path {
                path: path.clone(),
                previous_path: previous_path.clone(),
                watch_root: Some(root.to_path_buf()),
            },
        );
    }
}

fn publish_meta(bridge: &EventBridge, root: &Path, name: &str) {
    let _ = bridge.emit(
        name,
        now_ms(),
        EventSource::FileSystem,
        context(Some(root.to_path_buf())),
        AutomationEventData::None,
    );
}

fn context(cwd: Option<PathBuf>) -> EventContext {
    EventContext {
        script_id: None,
        handler_id: None,
        task_id: None,
        correlation_id: CorrelationId::new(),
        window_id: None,
        tab_id: None,
        cwd,
    }
}

fn build_globs(patterns: &[String]) -> AutomationResult<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        builder.add(Glob::new(pattern).map_err(|_| watch_error("watch.glob"))?);
    }
    builder.build().map_err(|_| watch_error("watch.glob"))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
        })
}

fn watch_error(operation: &'static str) -> AutomationError {
    AutomationError::new(
        AutomationErrorKind::FileSystem,
        operation,
        true,
        "The configured folder watch could not be started",
    )
}

#[cfg(test)]
mod tests {
    use std::{fs, sync::Arc, thread, time::Duration};

    use explorer_automation::{EventBridge, WatchRegistration, fakes::FakeEventSink};
    use tempfile::tempdir;

    use super::FolderWatchService;

    #[test]
    fn configured_watch_emits_owned_file_events_and_stops_cleanly() {
        let root = tempdir().expect("tempdir");
        let sink = Arc::new(FakeEventSink::new(32).expect("sink"));
        let bridge = Arc::new(EventBridge::new(sink.clone()));
        let watch = FolderWatchService::start(
            &WatchRegistration {
                root: root.path().to_path_buf(),
                recursive: true,
                include: vec!["**/*.txt".into()],
                exclude: vec!["**/ignored/**".into()],
            },
            bridge,
        )
        .expect("watch");
        fs::write(root.path().join("note.txt"), "changed").expect("write");
        let mut observed = false;
        for _ in 0..100 {
            while let Some(event) = sink.pop().expect("pop") {
                if event.name.as_str() == "fs.created" || event.name.as_str() == "fs.modified" {
                    observed = true;
                }
            }
            if observed {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        watch.stop();
        assert!(observed);
    }
}
