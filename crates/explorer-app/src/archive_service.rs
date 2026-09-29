//! Archive navigation shares the ordinary directory/column event contract.

use explorer_common::archive::{
    ArchivePath, children, list_archive, read_member, supported_archive,
};
use explorer_model::{
    ExplorerCommand, ExplorerEvent, ExplorerService, ExplorerServiceError, FileEntry,
    FileEntryMetadata, LocationDescriptor, RequestContext,
};
use std::{
    io::Write,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc::SyncSender,
    },
};

pub(crate) struct ArchiveBrowser {
    sender: SyncSender<ExplorerEvent>,
    shell: Arc<dyn ExplorerService>,
    workers: Arc<AtomicUsize>,
    materializations: Arc<Mutex<Vec<tempfile::NamedTempFile>>>,
}

fn potential_archive(path: &Path) -> bool {
    path.ancestors().any(supported_archive)
}

fn archive_error(error: impl ToString) -> explorer_common::ExplorerError {
    explorer_common::ExplorerError::new(
        explorer_common::ExplorerErrorKind::Availability,
        "archive browsing",
        true,
        "無法讀取壓縮檔；請檢查檔案是否損壞、需要密碼，或超過預覽限制。",
        error.to_string(),
    )
}

fn operation_locations(command: &ExplorerCommand) -> Vec<(&LocationDescriptor, bool)> {
    use explorer_model::{DataTransferRequest as D, FileOperationKind as F};
    match command {
        ExplorerCommand::ExecuteFileOperation { request, .. } => match &request.kind {
            F::CreateFolder { parent, .. } | F::CreateItem { parent, .. } => vec![(parent, true)],
            F::Rename { item, .. } | F::SetUnixMode { item, .. } => vec![(&item.location, false)],
            F::Copy { items, destination } | F::Move { items, destination } => items
                .iter()
                .map(|item| (&item.location, false))
                .chain(std::iter::once((destination, true)))
                .collect(),
            F::RecycleDelete { items }
            | F::PermanentDelete { items, .. }
            | F::CreateShortcut { items } => {
                items.iter().map(|item| (&item.location, false)).collect()
            }
        },
        ExplorerCommand::DataTransfer { request, .. } => match request {
            D::Copy { items } | D::Cut { items } | D::BeginDrag { items, .. } => {
                items.iter().map(|item| (&item.location, false)).collect()
            }
            D::Paste { destination, .. } => vec![(destination, true)],
            D::DropExternal {
                sources,
                destination,
                ..
            } => sources
                .iter()
                .map(|location| (location, false))
                .chain(std::iter::once((destination, true)))
                .collect(),
        },
        _ => Vec::new(),
    }
}

fn entries(path: &ArchivePath, context: &RequestContext) -> std::io::Result<Vec<FileEntry>> {
    let listing = list_archive(&path.archive, &|| context.cancellation.is_cancelled())?;
    children(&listing, &path.member)?
        .into_iter()
        .map(|entry| {
            let location = LocationDescriptor::file_system(path.member_path(&entry.member));
            // Prefix separates these provider ids from Shell PIDLs and filesystem identities.
            let mut bytes = b"superexplorer:archive-member:".to_vec();
            bytes.extend_from_slice(location.editable_text().as_bytes());
            let id = explorer_model::ShellItemId::from_provider_bytes(bytes)
                .ok_or_else(|| std::io::Error::other("Invalid archive identity"))?;
            Ok(FileEntry {
                id,
                display_name: entry.name,
                location,
                is_container: entry.directory,
                metadata: FileEntryMetadata {
                    archive_member: true,
                    size_bytes: entry.size,
                    type_display: Some(
                        if entry.directory {
                            "壓縮檔內的資料夾"
                        } else {
                            "壓縮檔內的檔案"
                        }
                        .to_owned(),
                    ),
                    namespace_capabilities: explorer_model::NamespaceCapabilities::from_public_bits(
                        explorer_model::NamespaceCapabilities::OPEN,
                    ),
                    ..Default::default()
                },
            })
        })
        .collect()
}

impl ArchiveBrowser {
    pub(crate) fn new(sender: SyncSender<ExplorerEvent>, shell: Arc<dyn ExplorerService>) -> Self {
        Self {
            sender,
            shell,
            workers: Arc::new(AtomicUsize::new(0)),
            materializations: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub(crate) fn submit(
        &self,
        command: &ExplorerCommand,
    ) -> Option<Result<(), ExplorerServiceError>> {
        let operations = operation_locations(command);
        if command
            .context()
            .is_some_and(|context| context.archive_browsing)
            && operations
                .iter()
                .any(|(location, _)| location.path().is_some_and(potential_archive))
        {
            let command = command.clone();
            let sender = self.sender.clone();
            let shell = self.shell.clone();
            std::thread::spawn(move || {
                let blocked =
                    operation_locations(&command)
                        .iter()
                        .any(|(location, destination)| {
                            location
                                .path()
                                .and_then(ArchivePath::resolve)
                                .is_some_and(|path| *destination || !path.member.is_empty())
                        });
                if blocked {
                    if let Some(context) = command.context() {
                        let error = explorer_common::ExplorerError::new(
                            explorer_common::ExplorerErrorKind::Authorization,
                            "archive operation",
                            false,
                            "壓縮檔內採唯讀瀏覽；請先解壓縮後再修改或複製檔案。",
                            "Archive member filesystem operations are not supported",
                        );
                        let event =
                            if matches!(command, ExplorerCommand::ExecuteFileOperation { .. }) {
                                ExplorerEvent::OperationFinished {
                                    context: context.clone(),
                                    outcome: explorer_model::OperationTerminal::Failed(error),
                                }
                            } else {
                                ExplorerEvent::Failed {
                                    context: context.clone(),
                                    error,
                                }
                            };
                        let _ = sender.send(event);
                    }
                } else if ExplorerService::submit(shell.as_ref(), command.clone()).is_err()
                    && let Some(context) = command.context()
                {
                    let _ = sender.send(ExplorerEvent::Failed {
                        context: context.clone(),
                        error: archive_error("Shell operation submission failed"),
                    });
                }
            });
            return Some(Ok(()));
        }
        let (context, location, revision) = match command {
            ExplorerCommand::Navigate { context, location }
            | ExplorerCommand::Refresh { context, location } => (context, location, None),
            ExplorerCommand::EnumerateColumn {
                context,
                location,
                branch_revision,
            } => (context, location, Some(*branch_revision)),
            ExplorerCommand::OpenItem { context, item, .. } => (context, &item.location, None),
            _ => return None,
        };
        let path = location.path()?;
        if !potential_archive(path) {
            return None;
        }
        // Other views retain native ZIP and extension 7z providers. Member paths from
        // a column session continue to resolve if that session later changes its view.
        if !context.archive_browsing
            && revision.is_none()
            && supported_archive(path)
            && !path.ancestors().skip(1).any(supported_archive)
        {
            return None;
        }
        if self.workers.fetch_add(1, Ordering::AcqRel) >= 4 {
            self.workers.fetch_sub(1, Ordering::AcqRel);
            return Some(Err(ExplorerServiceError::Overloaded));
        }
        let command = command.clone();
        let context = context.clone();
        let location = location.clone();
        let sender = self.sender.clone();
        let shell = self.shell.clone();
        let workers = self.workers.clone();
        let materializations = self.materializations.clone();
        std::thread::spawn(move || {
            struct Slot(Arc<AtomicUsize>);
            impl Drop for Slot {
                fn drop(&mut self) {
                    self.0.fetch_sub(1, Ordering::AcqRel);
                }
            }
            let _slot = Slot(workers);
            if context.cancellation.is_cancelled() {
                return;
            }
            let Some(path) = location.path().and_then(ArchivePath::resolve) else {
                if ExplorerService::submit(shell.as_ref(), command).is_err() {
                    let _ = sender.send(ExplorerEvent::Failed {
                        context,
                        error: archive_error("Shell navigation submission failed"),
                    });
                }
                return;
            };
            // Opening a file member uses a private bounded temporary file. Browsing itself
            // never extracts to disk, and the retained files are removed when service drops.
            if let ExplorerCommand::OpenItem { disposition, .. } = &command
                && !path.member.is_empty()
            {
                let listing = list_archive(&path.archive, &|| context.cancellation.is_cancelled());
                if listing.as_ref().is_ok_and(|entries| {
                    entries
                        .iter()
                        .any(|entry| entry.member == path.member && !entry.directory)
                }) {
                    let result = (|| -> std::io::Result<()> {
                        let bytes = read_member(&path, 64 * 1024 * 1024, false, &|| {
                            context.cancellation.is_cancelled()
                        })?;
                        let suffix = Path::new(&path.member)
                            .extension()
                            .and_then(|s| s.to_str())
                            .map(|s| format!(".{s}"))
                            .unwrap_or_default();
                        let mut file = tempfile::Builder::new()
                            .prefix("superexplorer-archive-")
                            .suffix(&suffix)
                            .tempfile()?;
                        file.write_all(&bytes)?;
                        file.flush()?;
                        let mut retained = materializations.lock().map_err(|_| {
                            std::io::Error::other("Temporary preview storage unavailable")
                        })?;
                        let used: u64 = retained
                            .iter()
                            .filter_map(|file| file.as_file().metadata().ok().map(|m| m.len()))
                            .sum();
                        if retained.len() >= 32
                            || used.saturating_add(bytes.len() as u64) > 128 * 1024 * 1024
                        {
                            return Err(std::io::Error::other("Temporary open-file quota reached"));
                        }
                        let local = LocationDescriptor::file_system(file.path());
                        retained.push(file);
                        if context.cancellation.is_cancelled() {
                            return Ok(());
                        }
                        ExplorerService::submit(
                            shell.as_ref(),
                            ExplorerCommand::OpenItem {
                                context: context.clone(),
                                item: explorer_model::ItemDescriptor {
                                    id: explorer_model::ShellItemId::from_provider_bytes(
                                        local.editable_text().into_bytes(),
                                    )
                                    .ok_or_else(|| std::io::Error::other("Invalid item"))?,
                                    location: local,
                                },
                                disposition: *disposition,
                            },
                        )
                        .map_err(|error| std::io::Error::other(format!("{error:?}")))
                    })();
                    if let Err(error) = result {
                        let _ = sender.send(ExplorerEvent::Failed {
                            context,
                            error: archive_error(error),
                        });
                    }
                    return;
                }
            }
            let result = entries(&path, &context);
            if context.cancellation.is_cancelled() {
                return;
            }
            if let Some(branch_revision) = revision {
                let outcome = match result {
                    Ok(entries) => {
                        let empty = entries.is_empty();
                        if !empty {
                            let _ = sender.send(ExplorerEvent::ColumnDirectoryBatch {
                                context: context.clone(),
                                branch_revision,
                                location: location.clone(),
                                entries,
                            });
                        }
                        if empty {
                            explorer_model::ColumnListingTerminal::Empty
                        } else {
                            explorer_model::ColumnListingTerminal::Finished
                        }
                    }
                    Err(error) => {
                        explorer_model::ColumnListingTerminal::Failed(archive_error(error))
                    }
                };
                let _ = sender.send(ExplorerEvent::ColumnDirectoryFinished {
                    context,
                    branch_revision,
                    location,
                    outcome,
                });
            } else {
                match result {
                    Ok(entries) => {
                        let title = location
                            .path()
                            .and_then(Path::file_name)
                            .map(|s| s.to_string_lossy().into_owned())
                            .unwrap_or_else(|| location.editable_text());
                        let _ = sender.send(ExplorerEvent::LocationResolved {
                            context: context.clone(),
                            metadata: explorer_model::LocationMetadata {
                                descriptor: location,
                                display_title: title,
                                can_go_up: true,
                                can_write: false,
                            },
                        });
                        if !entries.is_empty() {
                            let _ = sender.send(ExplorerEvent::DirectoryBatch {
                                context: context.clone(),
                                entries,
                            });
                        }
                        let _ = sender.send(ExplorerEvent::DirectoryFinished { context });
                    }
                    Err(error) => {
                        let _ = sender.send(ExplorerEvent::Failed {
                            context,
                            error: archive_error(error),
                        });
                    }
                }
            }
        });
        Some(Ok(()))
    }
}

pub(crate) fn mark_archive_containers(event: &mut ExplorerEvent) {
    let entries = match event {
        ExplorerEvent::DirectoryBatch { entries, .. }
        | ExplorerEvent::ColumnDirectoryBatch { entries, .. } => entries,
        _ => return,
    };
    for entry in entries {
        if !entry.metadata.archive_member
            && entry.metadata.filesystem_attributes & 0x10 == 0
            && entry.location.path().is_some_and(supported_archive)
        {
            entry.is_container = true;
            entry.metadata.archive_member = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::mpsc, time::Duration};

    #[derive(Default)]
    struct ShellRecorder(Mutex<Vec<ExplorerCommand>>);
    impl ExplorerService for ShellRecorder {
        fn submit(&self, command: ExplorerCommand) -> Result<(), ExplorerServiceError> {
            self.0.lock().unwrap().push(command);
            Ok(())
        }
        fn try_recv(&self) -> Result<Option<ExplorerEvent>, ExplorerServiceError> {
            Ok(None)
        }
    }

    fn context() -> RequestContext {
        let mut context = RequestContext::new(
            explorer_model::TabId::new(),
            explorer_model::Generation::new(1),
        );
        context.archive_browsing = true;
        context
    }

    #[cfg(windows)]
    #[test]
    fn archive_service_navigation_and_column_listing_emit_typed_read_only_results() {
        let directory = tempfile::tempdir().unwrap();
        let archive = directory.path().join("test.rar");
        std::fs::write(
            &archive,
            include_bytes!("../../explorer-common/tests/fixtures/browse.rar"),
        )
        .unwrap();
        let (sender, receiver) = mpsc::sync_channel(32);
        let shell = Arc::new(ShellRecorder::default());
        let browser = ArchiveBrowser::new(sender, shell.clone());
        let request = context();
        let location = LocationDescriptor::file_system(&archive);
        browser
            .submit(&ExplorerCommand::Navigate {
                context: request.clone(),
                location: location.clone(),
            })
            .unwrap()
            .unwrap();
        let ExplorerEvent::LocationResolved {
            context: actual,
            metadata,
        } = receiver.recv_timeout(Duration::from_secs(5)).unwrap()
        else {
            panic!("location not resolved")
        };
        assert_eq!(actual, request);
        assert_eq!(metadata.descriptor, location);
        assert!(!metadata.can_write);
        let ExplorerEvent::DirectoryBatch { entries, .. } =
            receiver.recv_timeout(Duration::from_secs(5)).unwrap()
        else {
            panic!("no listing")
        };
        assert!(entries.iter().all(|e| e.metadata.archive_member));
        assert!(
            entries
                .iter()
                .any(|e| e.display_name == "testdir" && e.is_container)
        );
        assert!(
            entries
                .iter()
                .any(|e| e.display_name == "test.txt" && e.metadata.size_bytes == Some(20))
        );
        assert!(matches!(
            receiver.recv_timeout(Duration::from_secs(5)).unwrap(),
            ExplorerEvent::DirectoryFinished { .. }
        ));
        let empty = LocationDescriptor::file_system(archive.join("testemptydir"));
        browser
            .submit(&ExplorerCommand::EnumerateColumn {
                context: request.clone(),
                location: empty.clone(),
                branch_revision: 7,
            })
            .unwrap()
            .unwrap();
        assert!(
            matches!(receiver.recv_timeout(Duration::from_secs(5)).unwrap(), ExplorerEvent::ColumnDirectoryFinished { branch_revision: 7, location, outcome: explorer_model::ColumnListingTerminal::Empty, .. } if location == empty)
        );
        let nested = LocationDescriptor::file_system(archive.join("testdir"));
        browser
            .submit(&ExplorerCommand::EnumerateColumn {
                context: request.clone(),
                location: nested.clone(),
                branch_revision: 8,
            })
            .unwrap()
            .unwrap();
        assert!(
            matches!(receiver.recv_timeout(Duration::from_secs(5)).unwrap(), ExplorerEvent::ColumnDirectoryBatch { branch_revision: 8, entries, .. } if entries.len() == 1 && entries[0].display_name == "test.txt")
        );
        assert!(matches!(
            receiver.recv_timeout(Duration::from_secs(5)).unwrap(),
            ExplorerEvent::ColumnDirectoryFinished {
                branch_revision: 8,
                outcome: explorer_model::ColumnListingTerminal::Finished,
                ..
            }
        ));
        assert!(
            shell.0.lock().unwrap().is_empty(),
            "members must not be delegated as native filesystem directories"
        );
        let create = ExplorerCommand::ExecuteFileOperation {
            context: request,
            request: explorer_model::FileOperationRequest {
                kind: explorer_model::FileOperationKind::CreateFolder {
                    parent: nested,
                    name: "denied".to_owned(),
                },
                flags: Default::default(),
            },
        };
        browser.submit(&create).unwrap().unwrap();
        assert!(matches!(
            receiver.recv_timeout(Duration::from_secs(5)).unwrap(),
            ExplorerEvent::OperationFinished {
                outcome: explorer_model::OperationTerminal::Failed(_),
                ..
            }
        ));
        assert_eq!(
            std::fs::read(archive).unwrap(),
            include_bytes!("../../explorer-common/tests/fixtures/browse.rar")
        );
        assert!(shell.0.lock().unwrap().is_empty());
    }

    #[test]
    fn archive_service_marks_archive_files_but_preserves_real_directory_extensions() {
        let mut file = FileEntry {
            id: explorer_model::ShellItemId::from_provider_bytes([1]).unwrap(),
            display_name: "archive.7z".to_owned(),
            location: LocationDescriptor::file_system(r"C:\fixture\archive.7z"),
            is_container: false,
            metadata: Default::default(),
        };
        let mut directory = file.clone();
        directory.display_name = "folder.zip".to_owned();
        directory.location = LocationDescriptor::file_system(r"C:\fixture\folder.zip");
        directory.is_container = true;
        directory.metadata.filesystem_attributes = 0x10;
        let mut event = ExplorerEvent::DirectoryBatch {
            context: context(),
            entries: vec![file.clone(), directory],
        };
        mark_archive_containers(&mut event);
        let ExplorerEvent::DirectoryBatch { entries, .. } = event else {
            unreachable!()
        };
        file = entries[0].clone();
        assert!(file.is_container && file.metadata.archive_member);
        assert!(entries[1].is_container && !entries[1].metadata.archive_member);
    }

    #[test]
    fn archive_service_preserves_other_views_native_and_extension_archive_routes() {
        let (sender, _) = mpsc::sync_channel(8);
        let browser = ArchiveBrowser::new(sender, Arc::new(ShellRecorder::default()));
        let request = RequestContext::new(
            explorer_model::TabId::new(),
            explorer_model::Generation::new(1),
        );
        for extension in ["zip", "7z", "rar"] {
            let location =
                LocationDescriptor::file_system(format!(r"C:\archives\bundle.{extension}"));
            assert!(
                browser
                    .submit(&ExplorerCommand::Navigate {
                        context: request.clone(),
                        location
                    })
                    .is_none()
            );
        }
    }
}
