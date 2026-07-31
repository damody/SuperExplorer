//! App-owned routing boundary for extension-backed Shell work.

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
    mpsc::{Receiver, SyncSender, TryRecvError, TrySendError},
};

use explorer_model::{ExplorerCommand, ExplorerEvent, ExplorerService, ExplorerServiceError};

fn is_host_owned_context_verb(verb: Option<&str>) -> bool {
    verb.is_some_and(|verb| {
        verb.eq_ignore_ascii_case("properties") || verb.eq_ignore_ascii_case("PinToStartScreen")
    })
}

fn decode_trusted_raster(
    key: &explorer_model::ThumbnailRequestKey,
    location: &explorer_model::LocationDescriptor,
    cache_only: bool,
) -> Option<explorer_model::ThumbnailTerminal> {
    use image::ImageDecoder as _;

    if cache_only {
        return None;
    }
    let path = location.path()?;
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    if !matches!(
        extension.as_str(),
        "jpg" | "jpeg" | "png" | "bmp" | "gif" | "webp" | "tif" | "tiff"
    ) {
        return None;
    }
    let mut reader = image::ImageReader::open(path).ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(32_768);
    limits.max_image_height = Some(32_768);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().ok()?;
    let orientation = decoder
        .orientation()
        .unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut image_frame = image::DynamicImage::from_decoder(decoder).ok()?;
    image_frame.apply_orientation(orientation);
    let size = u32::from(key.physical_size.max(1));
    let rgba = image_frame.thumbnail(size, size).to_rgba8();
    let (width, height) = rgba.dimensions();
    let stride = width.checked_mul(4)?;
    Some(explorer_model::ThumbnailTerminal::Ready {
        source: explorer_model::ThumbnailSource::Provider,
        pixels: explorer_model::ThumbnailPixels {
            width,
            height,
            stride,
            bytes: rgba.into_raw(),
        },
    })
}

fn is_dedicated_raster_preview(
    key: &explorer_model::ThumbnailRequestKey,
    location: &explorer_model::LocationDescriptor,
    cache_only: bool,
) -> bool {
    !cache_only
        && key.physical_size > 128
        && location
            .path()
            .and_then(std::path::Path::extension)
            .and_then(std::ffi::OsStr::to_str)
            .is_some_and(|extension| {
                matches!(
                    extension.to_ascii_lowercase().as_str(),
                    "jpg" | "jpeg" | "png" | "bmp" | "gif" | "webp" | "tif" | "tiff"
                )
            })
}

/// Routes context-menu provider activation through the disposable broker while retaining the
/// existing Shell STA for Windows-owned filesystem and namespace operations.
pub struct BrokeredExplorerService {
    shell: Arc<explorer_shell_win::ShellStaHandle>,
    broker: explorer_extension_broker::BrokerClient,
    sender: SyncSender<ExplorerEvent>,
    receiver: Mutex<Receiver<ExplorerEvent>>,
    in_flight: Arc<AtomicUsize>,
    preview_in_flight: Arc<AtomicUsize>,
    active_context_menus: Arc<Mutex<Vec<explorer_model::RequestContext>>>,
    context_menu_sender: SyncSender<(
        explorer_model::RequestContext,
        explorer_model::ContextMenuRequest,
    )>,
    preview_sender: SyncSender<(
        explorer_model::RequestContext,
        explorer_model::PreviewHostCommand,
    )>,
    active_preview:
        Arc<Mutex<Option<(explorer_model::RequestContext, explorer_model::Generation)>>>,
    maximum_in_flight: usize,
}

impl BrokeredExplorerService {
    pub fn new(
        shell: Arc<explorer_shell_win::ShellStaHandle>,
        broker: explorer_extension_broker::BrokerClient,
    ) -> Self {
        let (sender, receiver) = std::sync::mpsc::sync_channel(64);
        let (context_menu_sender, context_menu_receiver) = std::sync::mpsc::sync_channel::<(
            explorer_model::RequestContext,
            explorer_model::ContextMenuRequest,
        )>(1);
        let active_context_menus = Arc::new(Mutex::new(Vec::with_capacity(2)));
        let context_active = Arc::clone(&active_context_menus);
        let context_events = sender.clone();
        let context_broker = broker.clone();
        std::thread::spawn(move || {
            while let Ok((context, request)) = context_menu_receiver.recv() {
                let outcome = if context.cancellation.is_cancelled() {
                    explorer_model::ContextMenuOutcome::Cancelled
                } else {
                    context_broker
                        .show_context_menu(&request, &context.cancellation)
                        .unwrap_or_else(|error| {
                            if context.cancellation.is_cancelled() {
                                return explorer_model::ContextMenuOutcome::Cancelled;
                            }
                            explorer_model::ContextMenuOutcome::Failed {
                                error: explorer_common::ExplorerError::new(
                                    explorer_common::ExplorerErrorKind::Extension,
                                    "brokered context menu",
                                    true,
                                    "The extension menu is unavailable. Try again.",
                                    format!("privacy-safe broker category: {error}"),
                                ),
                            }
                        })
                };
                if let Ok(mut active) = context_active.lock()
                    && let Some(index) = active.iter().position(|candidate| candidate == &context)
                {
                    active.remove(index);
                }
                let _ =
                    context_events.send(ExplorerEvent::ContextMenuFinished { context, outcome });
            }
        });
        let (preview_sender, preview_receiver) = std::sync::mpsc::sync_channel::<(
            explorer_model::RequestContext,
            explorer_model::PreviewHostCommand,
        )>(16);
        let active_preview = Arc::new(Mutex::new(None));
        let preview_active = Arc::clone(&active_preview);
        let preview_events = sender.clone();
        let preview_broker = broker.clone();
        std::thread::spawn(move || {
            while let Ok((context, command)) = preview_receiver.recv() {
                let generation = command.generation();
                let result = match &command {
                    explorer_model::PreviewHostCommand::Start {
                        selection,
                        parent_window,
                        bounds,
                    } => isize::try_from(*parent_window)
                        .map_err(|_| explorer_extension_broker::BrokerClientError::Protocol)
                        .and_then(|parent_window| {
                            preview_broker.start_preview_session(
                                &selection.location,
                                parent_window,
                                *bounds,
                                &context.cancellation,
                            )
                        })
                        .map(|mode| explorer_model::PreviewHostTerminal::Ready {
                            generation,
                            mode,
                        }),
                    _ => preview_broker.update_preview_session(&command).map(|()| {
                        if matches!(command, explorer_model::PreviewHostCommand::Unload { .. }) {
                            explorer_model::PreviewHostTerminal::Unloaded { generation }
                        } else {
                            explorer_model::PreviewHostTerminal::Updated { generation }
                        }
                    }),
                };
                let outcome = result.unwrap_or_else(|error| {
                    let error = match error {
                        explorer_extension_broker::BrokerClientError::Timeout => {
                            explorer_model::PreviewHostError::Timeout(
                                explorer_model::PreviewOperation::Render,
                            )
                        }
                        explorer_extension_broker::BrokerClientError::Disconnected
                        | explorer_extension_broker::BrokerClientError::Start => {
                            explorer_model::PreviewHostError::Disconnected
                        }
                        explorer_extension_broker::BrokerClientError::Unavailable
                        | explorer_extension_broker::BrokerClientError::VersionMismatch => {
                            explorer_model::PreviewHostError::Unsupported
                        }
                        explorer_extension_broker::BrokerClientError::Protocol => {
                            explorer_model::PreviewHostError::Initialization
                        }
                    };
                    explorer_model::PreviewHostTerminal::Failed { generation, error }
                });
                if let Ok(mut active) = preview_active.lock() {
                    match outcome {
                        explorer_model::PreviewHostTerminal::Ready { .. } => {
                            *active = Some((context.clone(), generation));
                        }
                        explorer_model::PreviewHostTerminal::Unloaded { .. }
                        | explorer_model::PreviewHostTerminal::Failed { .. } => {
                            if active
                                .as_ref()
                                .is_some_and(|(_, current)| *current == generation)
                            {
                                *active = None;
                            }
                        }
                        explorer_model::PreviewHostTerminal::Updated { .. } => {}
                    }
                }
                let _ =
                    preview_events.send(ExplorerEvent::PreviewHostFinished { context, outcome });
            }
        });
        Self {
            shell,
            broker,
            sender,
            receiver: Mutex::new(receiver),
            in_flight: Arc::new(AtomicUsize::new(0)),
            preview_in_flight: Arc::new(AtomicUsize::new(0)),
            active_context_menus,
            context_menu_sender,
            preview_sender,
            active_preview,
            maximum_in_flight: 4,
        }
    }

    fn submit_context_menu(
        &self,
        context: explorer_model::RequestContext,
        request: explorer_model::ContextMenuRequest,
    ) -> Result<(), ExplorerServiceError> {
        {
            let mut active = self
                .active_context_menus
                .lock()
                .map_err(|_| ExplorerServiceError::Internal)?;
            // One request may wait behind the currently modal native menu. This is specifically
            // needed for Explorer-style right-click retargeting: the replacement gesture can
            // reach the host just before the old broker result clears its activity record.
            if active.len() >= 2 {
                return Err(ExplorerServiceError::Overloaded);
            }
            active.push(context.clone());
        }
        match self.context_menu_sender.try_send((context, request)) {
            Ok(()) => Ok(()),
            Err(error) => {
                let (context, result) = match error {
                    TrySendError::Full((context, _)) => (context, ExplorerServiceError::Overloaded),
                    TrySendError::Disconnected((context, _)) => {
                        (context, ExplorerServiceError::Disconnected)
                    }
                };
                if let Ok(mut active) = self.active_context_menus.lock()
                    && let Some(index) = active.iter().position(|candidate| candidate == &context)
                {
                    active.remove(index);
                }
                Err(result)
            }
        }
    }

    fn try_reserve(&self) -> bool {
        self.in_flight
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                (current < self.maximum_in_flight).then_some(current.saturating_add(1))
            })
            .is_ok()
    }

    fn submit_thumbnail(
        &self,
        context: explorer_model::RequestContext,
        key: explorer_model::ThumbnailRequestKey,
        location: explorer_model::LocationDescriptor,
        cache_only: bool,
    ) -> Result<(), ExplorerServiceError> {
        let dedicated_preview = is_dedicated_raster_preview(&key, &location, cache_only);
        let reserved = if dedicated_preview {
            self.preview_in_flight
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                    (current < 1).then_some(current.saturating_add(1))
                })
                .is_ok()
        } else {
            self.try_reserve()
        };
        if !reserved {
            return Err(ExplorerServiceError::Overloaded);
        }
        let broker = self.broker.clone();
        let sender = self.sender.clone();
        let in_flight = Arc::clone(&self.in_flight);
        let preview_in_flight = Arc::clone(&self.preview_in_flight);
        std::thread::spawn(move || {
            let outcome = decode_trusted_raster(&key, &location, cache_only).unwrap_or_else(|| {
                broker
                    .load_thumbnail(&key, &location, cache_only)
                    .unwrap_or(explorer_model::ThumbnailTerminal::Fallback(
                        explorer_model::ThumbnailFallbackReason::ProviderFailure,
                    ))
            });
            let _ = sender.send(ExplorerEvent::ThumbnailFinished {
                context,
                key,
                outcome,
            });
            if dedicated_preview {
                preview_in_flight.fetch_sub(1, Ordering::AcqRel);
            } else {
                in_flight.fetch_sub(1, Ordering::AcqRel);
            }
        });
        Ok(())
    }
}

impl ExplorerService for BrokeredExplorerService {
    fn submit(&self, command: ExplorerCommand) -> Result<(), ExplorerServiceError> {
        match command {
            ExplorerCommand::ShowContextMenu { context, request } => {
                if is_host_owned_context_verb(request.requested_verb.as_deref()) {
                    ExplorerService::submit(
                        self.shell.as_ref(),
                        ExplorerCommand::ShowContextMenu { context, request },
                    )
                } else {
                    self.submit_context_menu(context, request)
                }
            }
            ExplorerCommand::LoadThumbnail {
                context,
                key,
                location,
                cache_only,
            } => self.submit_thumbnail(context, key, location, cache_only),
            ExplorerCommand::PreviewHost { context, command } => self
                .preview_sender
                .try_send((context, command))
                .map_err(|error| match error {
                    TrySendError::Full(_) => ExplorerServiceError::Overloaded,
                    TrySendError::Disconnected(_) => ExplorerServiceError::Disconnected,
                }),
            ExplorerCommand::Cancel { request_id } => {
                let cancelled_context_menu =
                    self.active_context_menus.lock().ok().and_then(|active| {
                        active
                            .iter()
                            .position(|context| context.request_id == request_id)
                            .map(|index| (active[index].clone(), index == 0))
                    });
                let cancelled_preview = self
                    .active_preview
                    .lock()
                    .ok()
                    .and_then(|active| active.clone())
                    .filter(|(context, _)| context.request_id == request_id);
                if let Some((context, is_current)) = cancelled_context_menu {
                    context.cancellation.cancel();
                    if is_current {
                        self.broker.cancel_active_worker();
                    }
                    Ok(())
                } else if let Some((context, _)) = cancelled_preview {
                    context.cancellation.cancel();
                    self.broker.cancel_active_worker();
                    Ok(())
                } else {
                    ExplorerService::submit(
                        self.shell.as_ref(),
                        ExplorerCommand::Cancel { request_id },
                    )
                }
            }
            command => ExplorerService::submit(self.shell.as_ref(), command),
        }
    }

    fn try_recv(&self) -> Result<Option<ExplorerEvent>, ExplorerServiceError> {
        match self
            .receiver
            .lock()
            .map_err(|_| ExplorerServiceError::Internal)?
            .try_recv()
        {
            Ok(event) => Ok(Some(event)),
            Err(TryRecvError::Empty) => ExplorerService::try_recv(self.shell.as_ref()),
            Err(TryRecvError::Disconnected) => Err(ExplorerServiceError::Disconnected),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{decode_trusted_raster, is_dedicated_raster_preview, is_host_owned_context_verb};

    #[test]
    fn host_owned_system_commands_use_the_long_lived_shell_sta() {
        assert!(is_host_owned_context_verb(Some("properties")));
        assert!(is_host_owned_context_verb(Some("PROPERTIES")));
        assert!(is_host_owned_context_verb(Some("PinToStartScreen")));
        assert!(!is_host_owned_context_verb(Some("open")));
        assert!(!is_host_owned_context_verb(None));
    }

    fn key(size: u16) -> explorer_model::ThumbnailRequestKey {
        explorer_model::ThumbnailRequestKey {
            item_id: explorer_model::ShellItemId::from_provider_bytes(
                b"trusted-raster-test".to_vec(),
            )
            .expect("bounded id"),
            physical_size: size,
            dpi: 96,
            mode: explorer_model::ThumbnailMode::Thumbnail,
            source_generation: 1,
            theme: explorer_model::ShellIconTheme::Light,
            association_generation: 1,
            overlay_generation: 0,
        }
    }

    #[test]
    fn supplied_jpeg_uses_bounded_trusted_background_decoder_when_present() {
        let path = std::path::Path::new(r"E:\av_out\326KJN-003.mp4.jpg");
        if !path.is_file() {
            return;
        }
        let outcome = decode_trusted_raster(
            &key(512),
            &explorer_model::LocationDescriptor::file_system(path),
            false,
        )
        .expect("supported raster");
        let explorer_model::ThumbnailTerminal::Ready { pixels, .. } = outcome else {
            panic!("trusted raster decoder did not return pixels");
        };
        assert!(pixels.width <= 512 && pixels.height <= 512);
        pixels.validate(128 * 1024 * 1024).expect("bounded pixels");
    }

    #[test]
    fn cache_only_requests_remain_inside_the_broker_policy() {
        assert!(
            decode_trusted_raster(
                &key(96),
                &explorer_model::LocationDescriptor::file_system("photo.jpg"),
                true,
            )
            .is_none()
        );
    }

    #[test]
    fn only_large_local_rasters_use_the_bounded_preview_lane() {
        let location = explorer_model::LocationDescriptor::file_system("photo.jpg");
        assert!(is_dedicated_raster_preview(&key(512), &location, false));
        assert!(!is_dedicated_raster_preview(&key(96), &location, false));
        assert!(!is_dedicated_raster_preview(&key(512), &location, true));
        assert!(!is_dedicated_raster_preview(
            &key(512),
            &explorer_model::LocationDescriptor::file_system("document.pdf"),
            false,
        ));
    }
}
