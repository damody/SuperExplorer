//! Best-effort search cache writes must never hold the Shell navigation STA.

use std::{
    path::PathBuf,
    sync::mpsc::{self, SyncSender},
    thread,
};

use explorer_model::FileEntry;

// At most one active snapshot and two queued snapshots, each capped by the caller.
const QUEUE_CAPACITY: usize = 2;
pub(crate) const SNAPSHOT_BYTE_CAP: usize = 16 * 1024 * 1024;

#[derive(Debug)]
pub(crate) struct DirectoryObservation {
    pub path: PathBuf,
    pub entries: Vec<FileEntry>,
}

pub(crate) struct DirectoryIndexer {
    sender: SyncSender<DirectoryObservation>,
}

impl DirectoryIndexer {
    pub(crate) fn start() -> std::io::Result<Self> {
        let mut index = None;
        Self::start_with(move |observation| {
            if index.is_none() {
                index = explorer_search::LazyIndex::open_default().ok();
            }
            if let Some(cache) = index.as_mut()
                && let Err(error) = cache.observe_directory(&observation.path, &observation.entries)
            {
                tracing::debug!(%error, "background directory index update failed");
                index = None;
            }
        })
    }

    fn start_with(
        mut observe: impl FnMut(DirectoryObservation) + Send + 'static,
    ) -> std::io::Result<Self> {
        let (sender, receiver) = mpsc::sync_channel(QUEUE_CAPACITY);
        thread::Builder::new()
            .name("explorer-directory-index".to_owned())
            .spawn(move || {
                // Dropping the STA's sender closes the queue; the worker then exits.
                // No Shell interfaces or PIDLs cross this boundary.
                while let Ok(observation) = receiver.recv() {
                    observe(observation);
                }
            })?;
        Ok(Self { sender })
    }

    pub(crate) fn observe(&self, observation: DirectoryObservation) {
        // A full cache queue may skip an observation; search still scans live files.
        // Never wait for slow storage, a SQLite lock, or another navigation's write.
        let _ = self.sender.try_send(observation);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::mpsc::TrySendError, time::Duration};

    #[test]
    fn stalled_index_write_does_not_block_navigation_and_queue_is_bounded() {
        let (started_tx, started_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel::<()>(1);
        let (done_tx, done_rx) = mpsc::sync_channel(1);
        let worker = DirectoryIndexer::start_with(move |_| {
            let _ = started_tx.try_send(());
            let _ = release_rx.recv();
            let _ = done_tx.try_send(());
        })
        .unwrap();
        let observation = || DirectoryObservation {
            path: PathBuf::from("fixture"),
            entries: vec![],
        };
        worker.observe(observation());
        started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        for _ in 0..QUEUE_CAPACITY {
            worker.sender.try_send(observation()).unwrap();
        }
        assert!(matches!(
            worker.sender.try_send(observation()),
            Err(TrySendError::Full(_))
        ));
        worker.observe(observation()); // Must return while the writer remains blocked.
        drop(worker);
        drop(release_tx);
        done_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    }
}
