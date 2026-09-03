//! App-owned routing boundary for extension-backed Shell work.

use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    mem::size_of,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, AtomicUsize, Ordering},
        mpsc::{Receiver, SyncSender, TryRecvError, TrySendError},
    },
};

use abi_stable::std_types::RVec;
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
            .and_then(Path::extension)
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
    virtual_folder: Option<Arc<Mutex<explorer_extension_host::SinglePluginVirtualFolderRuntimeV1>>>,
    virtual_containers: Arc<Mutex<HashMap<[u8; 16], VirtualContainerRecordV1>>>,
    /// One-shot old-to-new generation bridges minted only by a successful
    /// local mutation. They let the operation-triggered Refresh commit the
    /// new location while unrelated stale locations remain rejected.
    virtual_refresh_remaps: Arc<Mutex<HashMap<([u8; 16], u64), u64>>>,
    virtual_mutation_undo: Arc<Mutex<HashMap<[u8; 16], (PathBuf, u64)>>>,
    virtual_materializations: Arc<VirtualMaterializationStoreV1>,
    virtual_icon_requests: Mutex<HashMap<explorer_common::RequestId, explorer_model::ShellIconKey>>,
}

#[derive(Clone)]
struct VirtualContainerRecordV1 {
    path: PathBuf,
    generation: u64,
    title: String,
    entries: Arc<Mutex<HashMap<u64, VirtualEntryRecordV1>>>,
    secret: Arc<VirtualContainerSecretV1>,
}

#[derive(Default)]
struct VirtualContainerSecretV1(Mutex<Option<Vec<u16>>>);

impl VirtualContainerSecretV1 {
    fn snapshot(&self) -> Option<Vec<u16>> {
        self.0
            .lock()
            .ok()
            .and_then(|secret| secret.as_ref().cloned())
    }

    fn mint(&self) -> abi_stable::std_types::ROption<explorer_extension_api::VirtualSecretV1> {
        self.0
            .lock()
            .ok()
            .and_then(|secret| secret.as_ref().cloned())
            .and_then(explorer_extension_host::mint_virtual_secret_v1)
            .map(abi_stable::std_types::ROption::RSome)
            .unwrap_or(abi_stable::std_types::ROption::RNone)
    }

    fn replace(&self, mut replacement: Vec<u16>) {
        if let Ok(mut secret) = self.0.lock() {
            if let Some(previous) = secret.as_mut() {
                previous.fill(0);
                std::hint::black_box(previous);
            }
            *secret = Some(std::mem::take(&mut replacement));
        }
        replacement.fill(0);
        std::hint::black_box(&mut replacement);
    }
}

impl Drop for VirtualContainerSecretV1 {
    fn drop(&mut self) {
        if let Ok(secret) = self.0.get_mut()
            && let Some(secret) = secret.as_mut()
        {
            secret.fill(0);
            std::hint::black_box(secret);
        }
    }
}

#[derive(Clone)]
struct VirtualEntryRecordV1 {
    id: explorer_extension_api::StableIdV1,
    is_container: bool,
    size: u64,
    name: String,
    components: Vec<String>,
}

#[derive(Default)]
struct VirtualMaterializationStoreV1(Mutex<Vec<PathBuf>>);

impl Drop for VirtualMaterializationStoreV1 {
    fn drop(&mut self) {
        if let Ok(paths) = self.0.get_mut() {
            for path in paths.drain(..) {
                let _ = std::fs::remove_dir_all(path);
            }
        }
    }
}

const SEVEN_Z_PROVIDER_V1: &str = "rust-7z:resource";
const MAX_VIRTUAL_MATERIALIZATION_BYTES_V1: u64 = 512 * 1024 * 1024;
static VIRTUAL_MATERIALIZATION_NONCE_V1: AtomicU64 = AtomicU64::new(1);

fn virtual_failed(context: &explorer_model::RequestContext, message: &str) -> ExplorerEvent {
    ExplorerEvent::Failed {
        context: context.clone(),
        error: explorer_common::ExplorerError::new(
            explorer_common::ExplorerErrorKind::Extension,
            "virtual archive navigation",
            true,
            message,
            "virtual-folder provider did not return a usable result",
        ),
    }
}

fn is_7z_path(path: &Path) -> bool {
    path.extension()
        .and_then(std::ffi::OsStr::to_str)
        .is_some_and(|extension| extension.eq_ignore_ascii_case("7z"))
}

fn virtual_container_record(path: &Path) -> std::io::Result<([u8; 16], VirtualContainerRecordV1)> {
    let metadata = std::fs::metadata(path)?;
    let modified = metadata
        .modified()?
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    let generation = (modified ^ metadata.len()).max(1);
    let canonical = path.canonicalize()?;
    let mut first = std::collections::hash_map::DefaultHasher::new();
    canonical.hash(&mut first);
    let mut second = std::collections::hash_map::DefaultHasher::new();
    canonical.hash(&mut second);
    canonical.as_os_str().len().hash(&mut second);
    "virtual-container-v1".hash(&mut second);
    let mut identity = [0_u8; 16];
    identity[..8].copy_from_slice(&first.finish().to_le_bytes());
    identity[8..].copy_from_slice(&second.finish().to_le_bytes());
    let title = canonical
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or("7z archive")
        .to_owned();
    Ok((
        identity,
        VirtualContainerRecordV1 {
            path: canonical,
            generation,
            title,
            entries: Arc::new(Mutex::new(HashMap::new())),
            secret: Arc::new(VirtualContainerSecretV1::default()),
        },
    ))
}

#[cfg(windows)]
fn prompt_archive_password(title: &str, incorrect: bool) -> Option<Vec<u16>> {
    use windows::{
        Win32::{
            Foundation::{ERROR_CANCELLED, ERROR_SUCCESS},
            Security::Credentials::{
                CREDUI_FLAGS_ALWAYS_SHOW_UI, CREDUI_FLAGS_DO_NOT_PERSIST,
                CREDUI_FLAGS_GENERIC_CREDENTIALS, CREDUI_FLAGS_INCORRECT_PASSWORD,
                CREDUI_FLAGS_PASSWORD_ONLY_OK, CREDUI_INFOW, CredUIPromptForCredentialsW,
            },
        },
        core::PCWSTR,
    };

    let mut caption = "SuperExplorer archive password"
        .encode_utf16()
        .collect::<Vec<_>>();
    caption.push(0);
    let mut message = format!("Enter the password for {title}")
        .encode_utf16()
        .collect::<Vec<_>>();
    message.push(0);
    let mut target = format!("SuperExplorer:7z:{title}")
        .encode_utf16()
        .collect::<Vec<_>>();
    target.push(0);
    let info = CREDUI_INFOW {
        cbSize: size_of::<CREDUI_INFOW>() as u32,
        pszMessageText: PCWSTR(message.as_ptr()),
        pszCaptionText: PCWSTR(caption.as_ptr()),
        ..Default::default()
    };
    let mut username = vec![0_u16; 514];
    let mut password = vec![0_u16; explorer_extension_api::MAX_VIRTUAL_SECRET_UTF16_V1 + 1];
    let mut flags = CREDUI_FLAGS_ALWAYS_SHOW_UI
        | CREDUI_FLAGS_DO_NOT_PERSIST
        | CREDUI_FLAGS_GENERIC_CREDENTIALS
        | CREDUI_FLAGS_PASSWORD_ONLY_OK;
    if incorrect {
        flags |= CREDUI_FLAGS_INCORRECT_PASSWORD;
    }
    // SAFETY: all pointers reference live, NUL-terminated buffers for the
    // duration of this modal OS call; persistence is explicitly disabled.
    #[expect(
        unsafe_code,
        reason = "the Windows credential dialog is exposed through a raw Win32 buffer API"
    )]
    let result = unsafe {
        CredUIPromptForCredentialsW(
            Some(&raw const info),
            PCWSTR(target.as_ptr()),
            None,
            0,
            &mut username,
            &mut password,
            None,
            flags,
        )
    };
    username.fill(0);
    std::hint::black_box(&mut username);
    if result == ERROR_CANCELLED || result != ERROR_SUCCESS {
        password.fill(0);
        std::hint::black_box(&mut password);
        return None;
    }
    let length = password
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(password.len());
    let secret = password[..length].to_vec();
    password.fill(0);
    std::hint::black_box(&mut password);
    (!secret.is_empty()).then_some(secret)
}

#[cfg(not(windows))]
fn prompt_archive_password(_: &str, _: bool) -> Option<Vec<u16>> {
    None
}

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "replays one validated app-local Windows mouse gesture"
)]
fn replay_context_menu_gesture(owner_window: u64, x: i32, y: i32) {
    std::thread::spawn(move || {
        use windows::Win32::{
            Foundation::{HWND, POINT},
            System::Threading::GetCurrentProcessId,
            UI::{
                Input::KeyboardAndMouse::{
                    INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP,
                    MOUSEINPUT, SendInput,
                },
                WindowsAndMessaging::{
                    GA_ROOT, GetAncestor, GetWindowThreadProcessId, IsWindow, SetCursorPos,
                    WindowFromPoint,
                },
            },
        };
        std::thread::sleep(std::time::Duration::from_millis(50));
        let Ok(owner_window) = usize::try_from(owner_window) else {
            return;
        };
        let owner = HWND(owner_window as *mut std::ffi::c_void);
        let mut owner_process = 0;
        if owner.0.is_null()
            || !unsafe { IsWindow(Some(owner)).as_bool() }
            || unsafe { GetWindowThreadProcessId(owner, Some(&raw mut owner_process)) } == 0
            || owner_process != unsafe { GetCurrentProcessId() }
        {
            return;
        }
        let point = POINT { x, y };
        let owner_root = unsafe { GetAncestor(owner, GA_ROOT) };
        let target_root = unsafe { GetAncestor(WindowFromPoint(point), GA_ROOT) };
        if target_root != owner && target_root != owner_root {
            return;
        }
        if unsafe { SetCursorPos(x, y) }.is_err() {
            return;
        }
        let target_root = unsafe { GetAncestor(WindowFromPoint(point), GA_ROOT) };
        if target_root != owner && target_root != owner_root {
            return;
        }
        let mouse = |flags| INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dwFlags: flags,
                    ..MOUSEINPUT::default()
                },
            },
        };
        let inputs = [mouse(MOUSEEVENTF_RIGHTDOWN), mouse(MOUSEEVENTF_RIGHTUP)];
        if let Ok(input_size) = i32::try_from(size_of::<INPUT>()) {
            let _ = unsafe { SendInput(&inputs, input_size) };
        }
    });
}

#[cfg(not(windows))]
fn replay_context_menu_gesture(_: u64, _: i32, _: i32) {}

fn execute_adb_context_action(
    outcome: explorer_model::ContextMenuOutcome,
    cancellation: &explorer_model::CancellationToken,
    context: &explorer_model::RequestContext,
    events: &SyncSender<ExplorerEvent>,
) -> explorer_model::ContextMenuOutcome {
    let managed = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("RustGpuiExplorer")
        .join("tools")
        .join("adb");
    match &outcome {
        explorer_model::ContextMenuOutcome::DownloadAdb { .. } => {
            let result = explorer_remote::AdbToolInstaller::new(managed)
                .install_official(cancellation, |_, _| ())
                .map(|_| ());
            return match result {
                Ok(()) => explorer_model::ContextMenuOutcome::Invoked { command_offset: 0 },
                Err(error) => explorer_model::ContextMenuOutcome::Failed {
                    error: explorer_common::ExplorerError::new(
                        explorer_common::ExplorerErrorKind::Availability,
                        "install ADB",
                        true,
                        "ADB installation failed. Check the network connection and try again.",
                        error.to_string().chars().take(2048).collect::<String>(),
                    ),
                },
            };
        }
        explorer_model::ContextMenuOutcome::InstallApk {
            serial,
            device_name,
            target,
        } => {
            let apk = match target {
                explorer_model::ShellContextMenuTarget::Items { items, .. } if items.len() == 1 => {
                    match &items[0].location {
                        explorer_model::LocationDescriptor::FileSystem(path) => {
                            Some(path.to_path_buf())
                        }
                        _ => None,
                    }
                }
                _ => None,
            };
            let apk_name = apk
                .as_deref()
                .and_then(Path::file_name)
                .map(|name| explorer_model::normalize_apk_notice_text(&name.to_string_lossy(), 160))
                .unwrap_or_else(|| "APK".to_owned());
            let started = ExplorerEvent::ApkInstallStatus {
                context: context.clone(),
                apk_name: apk_name.clone(),
                device_name: explorer_model::normalize_apk_notice_text(device_name, 160),
                serial: explorer_model::normalize_apk_notice_text(serial, 160),
                status: explorer_model::ApkInstallStatus::Started,
            };
            if events.send(started).is_err() {
                return explorer_model::ContextMenuOutcome::Failed {
                    error: explorer_common::ExplorerError::new(
                        explorer_common::ExplorerErrorKind::Availability,
                        "start APK install",
                        true,
                        "Unable to start APK installation because the app notification channel is unavailable.",
                        "APK install was rejected before spawning adb",
                    ),
                };
            }
            let install_events = events.clone();
            let install_context = context.clone();
            let install_cancellation = cancellation.clone();
            let install_serial = serial.clone();
            let install_device_name = device_name.clone();
            std::thread::spawn(move || {
                let result = apk
                    .ok_or_else(|| anyhow::anyhow!("APK context target is no longer valid"))
                    .and_then(|apk| {
                        let resolver = explorer_remote::AdbToolResolver::new(managed);
                        let (tool, _) = resolver.resolve(&install_cancellation)?;
                        explorer_remote::adb_tools::install_apk(
                            &tool,
                            explorer_remote::adb::SystemAdbCommandRunner,
                            &install_serial,
                            &apk,
                            &install_cancellation,
                        )?;
                        Ok(())
                    });
                let terminal = match &result {
                    Ok(()) => explorer_model::ApkInstallStatus::Succeeded,
                    Err(_) if install_cancellation.is_cancelled() => {
                        explorer_model::ApkInstallStatus::Cancelled
                    }
                    Err(error) if error.to_string().to_ascii_lowercase().contains("timed out") => {
                        explorer_model::ApkInstallStatus::TimedOut
                    }
                    Err(_) => explorer_model::ApkInstallStatus::Failed {
                        message: "請檢查裝置連線與 APK 後再試一次".to_owned(),
                    },
                };
                let _ = install_events.send(ExplorerEvent::ApkInstallStatus {
                    context: install_context,
                    apk_name,
                    device_name: explorer_model::normalize_apk_notice_text(
                        &install_device_name,
                        160,
                    ),
                    serial: explorer_model::normalize_apk_notice_text(&install_serial, 160),
                    status: terminal,
                });
            });
            return explorer_model::ContextMenuOutcome::Invoked { command_offset: 0 };
        }
        _ => return outcome,
    }
}

impl BrokeredExplorerService {
    pub fn new(
        shell: Arc<explorer_shell_win::ShellStaHandle>,
        broker: explorer_extension_broker::BrokerClient,
        virtual_folder: Option<explorer_extension_host::SinglePluginVirtualFolderRuntimeV1>,
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
                let mut outcome = if context.cancellation.is_cancelled() {
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
                outcome = execute_adb_context_action(
                    outcome,
                    &context.cancellation,
                    &context,
                    &context_events,
                );
                if let Ok(mut active) = context_active.lock()
                    && let Some(index) = active.iter().position(|candidate| candidate == &context)
                {
                    active.remove(index);
                }
                let replay = match &outcome {
                    explorer_model::ContextMenuOutcome::ReplayRequested { x, y } => {
                        Some((request.owner_window, *x, *y))
                    }
                    _ => None,
                };
                let _ =
                    context_events.send(ExplorerEvent::ContextMenuFinished { context, outcome });
                if let Some((owner_window, x, y)) = replay {
                    replay_context_menu_gesture(owner_window, x, y);
                }
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
            virtual_folder: virtual_folder.map(|runtime| Arc::new(Mutex::new(runtime))),
            virtual_containers: Arc::new(Mutex::new(HashMap::new())),
            virtual_refresh_remaps: Arc::new(Mutex::new(HashMap::new())),
            virtual_mutation_undo: Arc::new(Mutex::new(HashMap::new())),
            virtual_materializations: Arc::new(VirtualMaterializationStoreV1::default()),
            virtual_icon_requests: Mutex::new(HashMap::new()),
        }
    }

    fn submit_virtual_icon(
        &self,
        context: explorer_model::RequestContext,
        key: explorer_model::ShellIconKey,
    ) -> Result<(), ExplorerServiceError> {
        let (_, entry) = self
            .virtual_entry(&key.location)
            .ok_or(ExplorerServiceError::Internal)?;
        let synthetic = if entry.is_container {
            PathBuf::from(r"C:\__super_explorer_folder_base__")
        } else if let Some(extension) = Path::new(&entry.name).extension() {
            PathBuf::from(format!(
                r"C:\__super_explorer_virtual__.{}",
                extension.to_string_lossy()
            ))
        } else {
            PathBuf::from(r"C:\__super_explorer_virtual_extensionless__")
        };
        let mut proxy = key.clone();
        proxy.item_id = None;
        proxy.location = explorer_model::LocationDescriptor::file_system(synthetic);
        self.virtual_icon_requests
            .lock()
            .map_err(|_| ExplorerServiceError::Internal)?
            .insert(context.request_id, key);
        if let Err(error) = ExplorerService::submit(
            self.shell.as_ref(),
            ExplorerCommand::LoadShellIcon {
                context: context.clone(),
                key: proxy,
            },
        ) {
            if let Ok(mut requests) = self.virtual_icon_requests.lock() {
                requests.remove(&context.request_id);
            }
            return Err(error);
        }
        Ok(())
    }

    fn restore_virtual_icon_key(&self, event: ExplorerEvent) -> ExplorerEvent {
        let request_id = match &event {
            ExplorerEvent::ShellIconLoaded { context, .. }
            | ExplorerEvent::ShellIconFailed { context, .. } => context.request_id,
            _ => return event,
        };
        let original = self
            .virtual_icon_requests
            .lock()
            .ok()
            .and_then(|mut requests| requests.remove(&request_id));
        let Some(original) = original else {
            return event;
        };
        match event {
            ExplorerEvent::ShellIconLoaded {
                context,
                mut payload,
            } => {
                payload.key = original;
                ExplorerEvent::ShellIconLoaded { context, payload }
            }
            ExplorerEvent::ShellIconFailed {
                context, reason, ..
            } => ExplorerEvent::ShellIconFailed {
                context,
                key: original,
                reason,
            },
            event => event,
        }
    }

    fn virtual_entry(
        &self,
        location: &explorer_model::LocationDescriptor,
    ) -> Option<(VirtualContainerRecordV1, VirtualEntryRecordV1)> {
        let explorer_model::LocationDescriptor::Virtual(location) = location else {
            return None;
        };
        let entry_id = location.entry_id?;
        let record = self
            .virtual_containers
            .lock()
            .ok()?
            .get(&location.container_identity)
            .cloned()?;
        if record.generation != location.container_generation {
            return None;
        }
        let entry = record.entries.lock().ok()?.get(&entry_id).cloned()?;
        Some((record, entry))
    }

    fn submit_virtual_file_open(
        &self,
        context: explorer_model::RequestContext,
        item: explorer_model::ItemDescriptor,
        disposition: explorer_model::OpenDisposition,
        record: VirtualContainerRecordV1,
        entry: VirtualEntryRecordV1,
    ) -> Result<(), ExplorerServiceError> {
        if entry.size > MAX_VIRTUAL_MATERIALIZATION_BYTES_V1 {
            return Err(ExplorerServiceError::Internal);
        }
        let Some(runtime) = self.virtual_folder.as_ref().cloned() else {
            return Err(ExplorerServiceError::Disconnected);
        };
        let sender = self.sender.clone();
        let shell = Arc::clone(&self.shell);
        let retained = Arc::clone(&self.virtual_materializations);
        std::thread::spawn(move || {
            let fail = |message| {
                let _ = sender.send(virtual_failed(&context, message));
            };
            let nonce = VIRTUAL_MATERIALIZATION_NONCE_V1.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir()
                .join("SuperExplorer")
                .join(format!("virtual-{}-{nonce:016x}", std::process::id()));
            if std::fs::create_dir_all(&root).is_err() {
                fail("Unable to prepare a temporary archive item.");
                return;
            }
            let target = root.join(&entry.name);
            let mut file = match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)
            {
                Ok(file) => file,
                Err(_) => {
                    let _ = std::fs::remove_dir_all(&root);
                    fail("Unable to prepare a temporary archive item.");
                    return;
                }
            };
            let mut offset = 0_u64;
            let result = (|| -> Result<(), ()> {
                use std::io::Write as _;
                while offset < entry.size {
                    if context.cancellation.is_cancelled() {
                        return Err(());
                    }
                    let input =
                        explorer_extension_host::open_virtual_container_input_with_cancellation_v1(
                            &record.path,
                            record.generation,
                            Some(context.cancellation.clone()),
                        )
                        .map_err(|_| ())?;
                    let outcome = runtime
                        .lock()
                        .map_err(|_| ())?
                        .read(
                            SEVEN_Z_PROVIDER_V1,
                            explorer_extension_api::VirtualReadRequestV1 {
                                container: input,
                                container_generation: record.generation,
                                source_generation: record.generation,
                                entry_id: entry.id,
                                offset,
                                maximum_bytes: explorer_extension_api::MAX_VIRTUAL_READ_BYTES_V1
                                    as u32,
                                reserved: 0,
                                secret: record.secret.mint(),
                            },
                        )
                        .map_err(|_| ())?;
                    if outcome.status != explorer_extension_api::VirtualProviderStatusV1::READY
                        || outcome.container_generation != record.generation
                        || outcome.next_offset <= offset
                        || outcome.bytes.is_empty()
                    {
                        return Err(());
                    }
                    file.write_all(&outcome.bytes).map_err(|_| ())?;
                    offset = outcome.next_offset;
                    if outcome.end_of_entry {
                        break;
                    }
                }
                if offset != entry.size {
                    return Err(());
                }
                file.flush().map_err(|_| ())?;
                file.sync_all().map_err(|_| ())
            })();
            drop(file);
            if result.is_err() {
                let _ = std::fs::remove_dir_all(&root);
                if !context.cancellation.is_cancelled() {
                    fail("Unable to extract the archive item.");
                }
                return;
            }
            if retained
                .0
                .lock()
                .map(|mut paths| paths.push(root.clone()))
                .is_err()
            {
                let _ = std::fs::remove_dir_all(&root);
                fail("Unable to retain the temporary archive item.");
                return;
            }
            let materialized = explorer_model::ItemDescriptor {
                id: item.id,
                location: explorer_model::LocationDescriptor::file_system(target),
            };
            if ExplorerService::submit(
                shell.as_ref(),
                ExplorerCommand::OpenItem {
                    context: context.clone(),
                    item: materialized,
                    disposition,
                },
            )
            .is_err()
            {
                fail("Unable to open the extracted archive item.");
            }
        });
        Ok(())
    }

    fn submit_virtual_begin_drag(
        &self,
        context: explorer_model::RequestContext,
        request: &explorer_model::DataTransferRequest,
    ) -> Option<Result<(), ExplorerServiceError>> {
        let explorer_model::DataTransferRequest::BeginDrag {
            items,
            allowed_effects,
            button,
        } = request
        else {
            return None;
        };
        let [item] = items.as_slice() else {
            return items
                .iter()
                .any(|item| {
                    matches!(
                        item.location,
                        explorer_model::LocationDescriptor::Virtual(_)
                    )
                })
                .then_some(Err(ExplorerServiceError::Internal));
        };
        let (record, entry) = self.virtual_entry(&item.location)?;
        if entry.is_container || entry.size > MAX_VIRTUAL_MATERIALIZATION_BYTES_V1 {
            return Some(Err(ExplorerServiceError::Internal));
        }
        let runtime = self.virtual_folder.as_ref()?.clone();
        let shell = Arc::clone(&self.shell);
        let sender = self.sender.clone();
        let retained = Arc::clone(&self.virtual_materializations);
        let item = item.clone();
        let _requested_effects = *allowed_effects;
        // A virtual archive entry is extracted to a retained temporary file.
        // Moving that implementation detail must never be offered to the drop
        // target; Explorer treats drag-out from an archive as a copy.
        let allowed_effects = explorer_model::TransferEffects {
            copy: true,
            move_item: false,
            link: false,
        };
        let button = *button;
        std::thread::spawn(move || {
            let result = (|| -> Result<(PathBuf, PathBuf), String> {
                use std::io::Write as _;
                let nonce = VIRTUAL_MATERIALIZATION_NONCE_V1.fetch_add(1, Ordering::Relaxed);
                let root = std::env::temp_dir()
                    .join("SuperExplorer")
                    .join(format!("virtual-drag-{}-{nonce:016x}", std::process::id()));
                std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
                let target = root.join(&entry.name);
                let mut file = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&target)
                    .map_err(|error| error.to_string())?;
                let mut offset = 0_u64;
                while offset < entry.size {
                    if context.cancellation.is_cancelled() {
                        return Err("virtual drag cancelled".to_owned());
                    }
                    let input =
                        explorer_extension_host::open_virtual_container_input_with_cancellation_v1(
                            &record.path,
                            record.generation,
                            Some(context.cancellation.clone()),
                        )
                        .map_err(|error| error.to_string())?;
                    let outcome = runtime
                        .lock()
                        .map_err(|_| "virtual provider closed".to_owned())?
                        .read(
                            SEVEN_Z_PROVIDER_V1,
                            explorer_extension_api::VirtualReadRequestV1 {
                                container: input,
                                container_generation: record.generation,
                                source_generation: record.generation,
                                entry_id: entry.id,
                                offset,
                                maximum_bytes: explorer_extension_api::MAX_VIRTUAL_READ_BYTES_V1
                                    as u32,
                                reserved: 0,
                                secret: record.secret.mint(),
                            },
                        )
                        .map_err(|error| error.to_string())?;
                    if outcome.status != explorer_extension_api::VirtualProviderStatusV1::READY
                        || outcome.container_generation != record.generation
                        || outcome.next_offset <= offset
                        || outcome.bytes.is_empty()
                    {
                        return Err("virtual provider returned an invalid drag stream".to_owned());
                    }
                    file.write_all(&outcome.bytes)
                        .map_err(|error| error.to_string())?;
                    offset = outcome.next_offset;
                    if outcome.end_of_entry {
                        break;
                    }
                }
                if offset != entry.size {
                    return Err("virtual drag stream length mismatch".to_owned());
                }
                file.flush().map_err(|error| error.to_string())?;
                file.sync_all().map_err(|error| error.to_string())?;
                Ok((root, target))
            })();
            let Ok((root, target)) = result else {
                let _ = sender.send(virtual_failed(
                    &context,
                    "Unable to prepare the archive item for dragging.",
                ));
                return;
            };
            if retained
                .0
                .lock()
                .map(|mut roots| roots.push(root.clone()))
                .is_err()
            {
                let _ = std::fs::remove_dir_all(root);
                let _ = sender.send(virtual_failed(
                    &context,
                    "Unable to retain the dragged archive item.",
                ));
                return;
            }
            let materialized = explorer_model::ItemDescriptor {
                id: item.id,
                location: explorer_model::LocationDescriptor::file_system(target),
            };
            if ExplorerService::submit(
                shell.as_ref(),
                ExplorerCommand::DataTransfer {
                    context: context.clone(),
                    request: explorer_model::DataTransferRequest::BeginDrag {
                        items: vec![materialized],
                        allowed_effects,
                        button,
                    },
                },
            )
            .is_err()
            {
                let _ = sender.send(virtual_failed(
                    &context,
                    "Unable to start the archive item drag.",
                ));
            }
        });
        Some(Ok(()))
    }

    fn submit_virtual_navigation(
        &self,
        context: explorer_model::RequestContext,
        requested: explorer_model::LocationDescriptor,
    ) -> Option<Result<(), ExplorerServiceError>> {
        let runtime = self.virtual_folder.as_ref()?.clone();
        let (location, record) = match &requested {
            explorer_model::LocationDescriptor::FileSystem(path) if is_7z_path(path) => {
                let (identity, record) = match virtual_container_record(path) {
                    Ok(value) => value,
                    Err(_) => return Some(Err(ExplorerServiceError::Internal)),
                };
                let location = match explorer_model::LocationDescriptor::try_virtual(
                    SEVEN_Z_PROVIDER_V1,
                    identity,
                    record.generation,
                    None,
                    Vec::new(),
                ) {
                    Ok(location) => location,
                    Err(_) => return Some(Err(ExplorerServiceError::Internal)),
                };
                if let Ok(mut containers) = self.virtual_containers.lock() {
                    containers.insert(identity, record.clone());
                } else {
                    return Some(Err(ExplorerServiceError::Internal));
                }
                (location, record)
            }
            explorer_model::LocationDescriptor::Virtual(virtual_location)
                if virtual_location.provider_id == SEVEN_Z_PROVIDER_V1 =>
            {
                let record = match self.virtual_containers.lock().ok().and_then(|containers| {
                    containers
                        .get(&virtual_location.container_identity)
                        .cloned()
                }) {
                    Some(record) if record.generation == virtual_location.container_generation => {
                        record
                    }
                    Some(record)
                        if self.virtual_refresh_remaps.lock().ok().and_then(|remaps| {
                            let mut generation = virtual_location.container_generation;
                            for _ in 0..=remaps.len() {
                                if generation == record.generation {
                                    return Some(generation);
                                }
                                generation = *remaps
                                    .get(&(virtual_location.container_identity, generation))?;
                            }
                            None
                        }) == Some(record.generation) =>
                    {
                        record
                    }
                    _ => return Some(Err(ExplorerServiceError::Internal)),
                };
                let location = if record.generation == virtual_location.container_generation {
                    requested
                } else {
                    match explorer_model::LocationDescriptor::try_virtual(
                        SEVEN_Z_PROVIDER_V1,
                        virtual_location.container_identity,
                        record.generation,
                        virtual_location.entry_id,
                        virtual_location.components.clone(),
                    ) {
                        Ok(location) => location,
                        Err(_) => return Some(Err(ExplorerServiceError::Internal)),
                    }
                };
                (location, record)
            }
            _ => return None,
        };
        let sender = self.sender.clone();
        std::thread::spawn(move || {
            if context.cancellation.is_cancelled() {
                return;
            }
            let explorer_model::LocationDescriptor::Virtual(virtual_location) = &location else {
                return;
            };
            let mut incorrect = false;
            let mut outcome = None;
            for attempt in 0..=3 {
                let input =
                    match explorer_extension_host::open_virtual_container_input_with_cancellation_v1(
                        &record.path,
                        record.generation,
                        Some(context.cancellation.clone()),
                    ) {
                        Ok(input) => input,
                        Err(_) => {
                            let _ = sender
                                .send(virtual_failed(&context, "Unable to open the archive."));
                            return;
                        }
                    };
                let secret = record.secret.mint();
                let supplied_secret = secret.is_some();
                let request = explorer_extension_api::VirtualEnumerateRequestV1 {
                    container: input,
                    container_generation: record.generation,
                    source_generation: record.generation,
                    parent_components: virtual_location
                        .components
                        .iter()
                        .cloned()
                        .map(abi_stable::std_types::RString::from)
                        .collect::<Vec<_>>()
                        .into(),
                    maximum_entries: explorer_extension_api::MAX_VIRTUAL_ENTRIES_V1 as u32,
                    reserved: 0,
                    secret,
                };
                outcome = runtime
                    .lock()
                    .ok()
                    .and_then(|runtime| runtime.enumerate(SEVEN_Z_PROVIDER_V1, request).ok());
                let needs_password = outcome.as_ref().is_some_and(|outcome| {
                    outcome.status
                        == explorer_extension_api::VirtualProviderStatusV1::PASSWORD_REQUIRED
                        || (supplied_secret
                            && outcome.status
                                == explorer_extension_api::VirtualProviderStatusV1::FAILED)
                });
                if !needs_password || attempt == 3 {
                    break;
                }
                let Some(secret) = prompt_archive_password(&record.title, incorrect) else {
                    return;
                };
                record.secret.replace(secret);
                incorrect = true;
            }
            let Some(outcome) = outcome.filter(|outcome| {
                outcome.status == explorer_extension_api::VirtualProviderStatusV1::READY
                    && outcome.container_generation == record.generation
            }) else {
                let _ = sender.send(virtual_failed(&context, "Unable to read the archive."));
                return;
            };
            if context.cancellation.is_cancelled() {
                return;
            }
            let metadata = explorer_model::LocationMetadata {
                descriptor: location.clone(),
                display_title: if virtual_location.components.is_empty() {
                    record.title.clone()
                } else {
                    virtual_location
                        .components
                        .last()
                        .cloned()
                        .unwrap_or(record.title.clone())
                },
                can_go_up: true,
                can_write: true,
            };
            if sender
                .send(ExplorerEvent::LocationResolved {
                    context: context.clone(),
                    metadata,
                })
                .is_err()
            {
                return;
            }
            let entry_records = Arc::clone(&record.entries);
            let entries = outcome
                .entries
                .into_iter()
                .filter_map(|entry| {
                    let name = entry.name.into_string();
                    let is_container =
                        entry.kind == explorer_extension_api::VirtualEntryKindV1::DIRECTORY;
                    entry_records.lock().ok()?.insert(
                        entry.id.value,
                        VirtualEntryRecordV1 {
                            id: entry.id,
                            is_container,
                            size: entry.uncompressed_size,
                            name: name.clone(),
                            components: entry
                                .components
                                .iter()
                                .map(|component| component.to_string())
                                .collect(),
                        },
                    );
                    let components = entry
                        .components
                        .into_iter()
                        .map(|component| component.into_string())
                        .collect::<Vec<_>>();
                    let location = explorer_model::LocationDescriptor::try_virtual(
                        SEVEN_Z_PROVIDER_V1,
                        virtual_location.container_identity,
                        record.generation,
                        Some(entry.id.value),
                        components,
                    )
                    .ok()?;
                    let mut id = Vec::with_capacity(28);
                    id.extend_from_slice(&virtual_location.container_identity);
                    id.extend_from_slice(&entry.id.namespace.into_raw().to_le_bytes());
                    id.extend_from_slice(&entry.id.value.to_le_bytes());
                    Some(explorer_model::FileEntry {
                        id: explorer_model::ShellItemId::from_provider_bytes(id)?,
                        display_name: name,
                        location,
                        is_container,
                        metadata: explorer_model::FileEntryMetadata {
                            size_bytes: (entry.kind
                                == explorer_extension_api::VirtualEntryKindV1::FILE)
                                .then_some(entry.uncompressed_size),
                            type_display: Some(
                                if entry.kind
                                    == explorer_extension_api::VirtualEntryKindV1::DIRECTORY
                                {
                                    "File folder".to_owned()
                                } else {
                                    "7z archive entry".to_owned()
                                },
                            ),
                            namespace_capabilities:
                                explorer_model::NamespaceCapabilities::from_public_bits(
                                    explorer_model::NamespaceCapabilities::OPEN
                                        | explorer_model::NamespaceCapabilities::COPY
                                        | explorer_model::NamespaceCapabilities::RENAME
                                        | explorer_model::NamespaceCapabilities::DELETE,
                                ),
                            ..Default::default()
                        },
                    })
                })
                .collect::<Vec<_>>();
            if !entries.is_empty() {
                let _ = sender.send(ExplorerEvent::DirectoryBatch {
                    context: context.clone(),
                    entries,
                });
            }
            let _ = sender.send(ExplorerEvent::DirectoryFinished { context });
        });
        Some(Ok(()))
    }

    fn submit_virtual_mutation_steps(
        &self,
        context: explorer_model::RequestContext,
        identity: [u8; 16],
        record: VirtualContainerRecordV1,
        steps: Vec<explorer_extension_api::VirtualMutationStepV1>,
    ) -> Option<Result<(), ExplorerServiceError>> {
        let runtime = self.virtual_folder.as_ref()?.clone();
        let sender = self.sender.clone();
        let containers = Arc::clone(&self.virtual_containers);
        let refresh_remaps = Arc::clone(&self.virtual_refresh_remaps);
        let undo = Arc::clone(&self.virtual_mutation_undo);
        std::thread::spawn(move || {
            let maximum_staging = std::fs::metadata(&record.path)
                .map(|metadata| metadata.len().saturating_mul(4).max(1024 * 1024))
                .unwrap_or(1024 * 1024);
            let result = runtime
                .lock()
                .map_err(|_| "virtual provider closed".to_owned())
                .and_then(|runtime| {
                    explorer_extension_host::commit_virtual_container_mutation_v1(
                        &runtime,
                        SEVEN_Z_PROVIDER_V1,
                        &record.path,
                        record.generation,
                        steps,
                        maximum_staging,
                        Some(context.cancellation.clone()),
                        record.secret.snapshot(),
                    )
                });
            match result {
                Ok(commit) => {
                    if let Ok(mut remaps) = refresh_remaps.lock() {
                        remaps.retain(|(container, _), _| *container != identity);
                        remaps.insert((identity, record.generation), commit.new_generation);
                    }
                    if let Ok(mut undo) = undo.lock() {
                        if let Some((old_backup, _)) =
                            undo.insert(identity, (commit.backup.clone(), commit.new_generation))
                        {
                            let _ = std::fs::remove_file(old_backup);
                        }
                    }
                    if let Ok(mut containers) = containers.lock()
                        && let Some(current) = containers.get_mut(&identity)
                    {
                        current.generation = commit.new_generation;
                        if let Ok(mut entries) = current.entries.lock() {
                            entries.clear();
                        }
                    }
                    let _ = sender.send(ExplorerEvent::OperationFinished {
                        context,
                        outcome: explorer_model::OperationTerminal::Finished,
                    });
                }
                Err(message) => {
                    tracing::warn!(
                        operation = "virtual archive mutation",
                        error = %message,
                        "virtual archive mutation failed"
                    );
                    let _ = sender.send(ExplorerEvent::OperationFinished {
                        context,
                        outcome: explorer_model::OperationTerminal::Failed(
                            explorer_common::ExplorerError::new(
                                explorer_common::ExplorerErrorKind::Extension,
                                "virtual archive mutation",
                                true,
                                "The archive could not be updated.",
                                message,
                            ),
                        ),
                    });
                }
            }
        });
        Some(Ok(()))
    }

    fn submit_virtual_file_operation(
        &self,
        context: explorer_model::RequestContext,
        request: explorer_model::FileOperationRequest,
    ) -> Option<Result<(), ExplorerServiceError>> {
        let (items, step_kind, rename, move_parent) = match &request.kind {
            explorer_model::FileOperationKind::Rename { item, new_name } => (
                vec![item.clone()],
                explorer_extension_api::VirtualMutationKindV1::RENAME,
                Some(new_name.clone()),
                None,
            ),
            explorer_model::FileOperationKind::RecycleDelete { items }
            | explorer_model::FileOperationKind::PermanentDelete { items, .. } => (
                items.clone(),
                explorer_extension_api::VirtualMutationKindV1::DELETE,
                None,
                None,
            ),
            explorer_model::FileOperationKind::Move { items, destination } => {
                let explorer_model::LocationDescriptor::Virtual(destination) = destination else {
                    return None;
                };
                (
                    items.clone(),
                    explorer_extension_api::VirtualMutationKindV1::MOVE,
                    None,
                    Some(destination.clone()),
                )
            }
            _ => return None,
        };
        let first = items.first()?;
        let explorer_model::LocationDescriptor::Virtual(first_location) = &first.location else {
            return None;
        };
        let identity = first_location.container_identity;
        let record = self
            .virtual_containers
            .lock()
            .ok()?
            .get(&identity)
            .cloned()?;
        if record.generation != first_location.container_generation {
            return Some(Err(ExplorerServiceError::Internal));
        }
        let mut steps = Vec::with_capacity(items.len());
        for item in &items {
            let explorer_model::LocationDescriptor::Virtual(location) = &item.location else {
                return Some(Err(ExplorerServiceError::Internal));
            };
            if location.container_identity != identity
                || location.container_generation != record.generation
            {
                return Some(Err(ExplorerServiceError::Internal));
            }
            let Some(entry_id) = location.entry_id else {
                return Some(Err(ExplorerServiceError::Internal));
            };
            let Some(entry) = record.entries.lock().ok()?.get(&entry_id).cloned() else {
                return Some(Err(ExplorerServiceError::Internal));
            };
            if entry.is_container {
                return Some(Err(ExplorerServiceError::Internal));
            }
            let destination_components = if let Some(new_name) = rename.as_ref() {
                if new_name.is_empty() || new_name.contains(['/', '\\', '\0']) {
                    return Some(Err(ExplorerServiceError::Internal));
                }
                let mut components = entry.components.clone();
                let Some(last) = components.last_mut() else {
                    return Some(Err(ExplorerServiceError::Internal));
                };
                *last = new_name.clone();
                components
                    .into_iter()
                    .map(Into::into)
                    .collect::<Vec<_>>()
                    .into()
            } else if let Some(parent) = move_parent.as_ref() {
                if parent.container_identity != identity
                    || parent.container_generation != record.generation
                {
                    return Some(Err(ExplorerServiceError::Internal));
                }
                let mut components = parent.components.clone();
                components.push(entry.name.clone());
                components
                    .into_iter()
                    .map(Into::into)
                    .collect::<Vec<_>>()
                    .into()
            } else {
                RVec::new()
            };
            steps.push(explorer_extension_api::VirtualMutationStepV1 {
                kind: step_kind,
                entry_id: entry.id,
                destination_components,
                source: abi_stable::std_types::ROption::RNone,
                source_generation: 0,
            });
        }
        self.submit_virtual_mutation_steps(context, identity, record, steps)
    }

    fn submit_virtual_create_operation(
        &self,
        context: explorer_model::RequestContext,
        request: &explorer_model::FileOperationRequest,
    ) -> Option<Result<(), ExplorerServiceError>> {
        use abi_stable::std_types::ROption;
        use explorer_model::{FileOperationKind, LocationDescriptor, ShellNewItemRecipe};

        let (parent, additions) = match &request.kind {
            FileOperationKind::CreateFolder { parent, name } => {
                (parent, vec![(name.clone(), true, ROption::RNone, 0)])
            }
            FileOperationKind::CreateItem {
                parent,
                name,
                recipe,
            } => {
                let (directory, source, generation) = match recipe {
                    ShellNewItemRecipe::Folder => (true, ROption::RNone, 0),
                    ShellNewItemRecipe::EmptyFile => (false, ROption::RNone, 0),
                    ShellNewItemRecipe::Data(bytes) => (
                        false,
                        explorer_extension_host::open_virtual_memory_input_v1(bytes.clone(), 1)
                            .map(ROption::RSome)
                            .unwrap_or(ROption::RNone),
                        1,
                    ),
                    ShellNewItemRecipe::TemplateFile(path) => (
                        false,
                        explorer_extension_host::open_virtual_container_input_v1(path, 1)
                            .ok()
                            .map(ROption::RSome)
                            .unwrap_or(ROption::RNone),
                        1,
                    ),
                };
                (parent, vec![(name.clone(), directory, source, generation)])
            }
            FileOperationKind::Copy { items, destination } => {
                let LocationDescriptor::Virtual(_) = destination else {
                    return None;
                };
                let mut additions = Vec::with_capacity(items.len());
                for item in items {
                    let LocationDescriptor::FileSystem(path) = &item.location else {
                        return Some(Err(ExplorerServiceError::Internal));
                    };
                    if !path.is_file() {
                        return Some(Err(ExplorerServiceError::Internal));
                    }
                    let name = path.file_name()?.to_str()?.to_owned();
                    let generation = std::fs::metadata(path)
                        .map(|metadata| metadata.len().max(1))
                        .ok()?;
                    let source =
                        explorer_extension_host::open_virtual_container_input_v1(path, generation)
                            .ok()?;
                    additions.push((name, false, ROption::RSome(source), generation));
                }
                (destination, additions)
            }
            _ => return None,
        };
        let LocationDescriptor::Virtual(parent) = parent else {
            return None;
        };
        let identity = parent.container_identity;
        let record = self
            .virtual_containers
            .lock()
            .ok()?
            .get(&identity)
            .cloned()?;
        if record.generation != parent.container_generation {
            return Some(Err(ExplorerServiceError::Internal));
        }
        let mut steps = Vec::with_capacity(additions.len());
        for (name, directory, source, source_generation) in additions {
            if explorer_model::validate_windows_file_name(&name).is_err() {
                return Some(Err(ExplorerServiceError::Internal));
            }
            let mut components = parent.components.clone();
            components.push(name);
            steps.push(explorer_extension_api::VirtualMutationStepV1 {
                kind: if directory {
                    explorer_extension_api::VirtualMutationKindV1::CREATE_DIRECTORY
                } else {
                    explorer_extension_api::VirtualMutationKindV1::ADD_FILE
                },
                entry_id: explorer_extension_api::StableIdV1::new(
                    explorer_extension_api::EXTENSION_ID_NAMESPACE_V1,
                    1,
                ),
                destination_components: components
                    .into_iter()
                    .map(Into::into)
                    .collect::<Vec<_>>()
                    .into(),
                source,
                source_generation,
            });
        }
        self.submit_virtual_mutation_steps(context, identity, record, steps)
    }

    fn submit_virtual_extract_operation(
        &self,
        context: explorer_model::RequestContext,
        request: &explorer_model::FileOperationRequest,
    ) -> Option<Result<(), ExplorerServiceError>> {
        let explorer_model::FileOperationKind::Copy { items, destination } = &request.kind else {
            return None;
        };
        let explorer_model::LocationDescriptor::FileSystem(destination) = destination else {
            return None;
        };
        let first = items.first()?;
        let explorer_model::LocationDescriptor::Virtual(first_location) = &first.location else {
            return None;
        };
        let identity = first_location.container_identity;
        let record = self
            .virtual_containers
            .lock()
            .ok()?
            .get(&identity)
            .cloned()?;
        let mut entries = Vec::with_capacity(items.len());
        let mut total = 0_u64;
        for item in items {
            let explorer_model::LocationDescriptor::Virtual(location) = &item.location else {
                return Some(Err(ExplorerServiceError::Internal));
            };
            if location.container_identity != identity
                || location.container_generation != record.generation
            {
                return Some(Err(ExplorerServiceError::Internal));
            }
            let entry = record
                .entries
                .lock()
                .ok()?
                .get(&location.entry_id?)
                .cloned()?;
            if entry.is_container || entry.name.contains(['/', '\\', '\0']) {
                return Some(Err(ExplorerServiceError::Internal));
            }
            total = total.checked_add(entry.size)?;
            if total > MAX_VIRTUAL_MATERIALIZATION_BYTES_V1 {
                return Some(Err(ExplorerServiceError::Internal));
            }
            entries.push(entry);
        }
        let runtime = self.virtual_folder.as_ref()?.clone();
        let sender = self.sender.clone();
        let destination = destination.clone();
        std::thread::spawn(move || {
            let result = (|| -> Result<(), String> {
                use std::io::Write as _;
                let metadata =
                    std::fs::metadata(&destination).map_err(|error| error.to_string())?;
                if !metadata.is_dir() {
                    return Err("extract destination is not a directory".to_owned());
                }
                for entry in &entries {
                    if context.cancellation.is_cancelled() {
                        return Err("extract cancelled".to_owned());
                    }
                    let target = destination.join(&entry.name);
                    if target.exists() {
                        return Err("extract destination already exists".to_owned());
                    }
                    let partial = destination.join(format!(
                        ".{}.superexplorer-{:016x}.partial",
                        entry.name,
                        VIRTUAL_MATERIALIZATION_NONCE_V1.fetch_add(1, Ordering::Relaxed)
                    ));
                    let mut file = std::fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&partial)
                        .map_err(|error| error.to_string())?;
                    let write_result = (|| -> Result<(), String> {
                        let mut offset = 0_u64;
                        while offset < entry.size {
                            let input = explorer_extension_host::open_virtual_container_input_with_cancellation_v1(
                                &record.path,
                                record.generation,
                                Some(context.cancellation.clone()),
                            )
                            .map_err(|error| error.to_string())?;
                            let outcome =
                                runtime
                                    .lock()
                                    .map_err(|_| "virtual provider closed".to_owned())?
                                    .read(
                                        SEVEN_Z_PROVIDER_V1,
                                        explorer_extension_api::VirtualReadRequestV1 {
                                            container: input,
                                            container_generation: record.generation,
                                            source_generation: record.generation,
                                            entry_id: entry.id,
                                            offset,
                                            maximum_bytes:
                                                explorer_extension_api::MAX_VIRTUAL_READ_BYTES_V1
                                                    as u32,
                                            reserved: 0,
                                            secret: record.secret.mint(),
                                        },
                                    )
                                    .map_err(|error| error.to_string())?;
                            if outcome.status
                                != explorer_extension_api::VirtualProviderStatusV1::READY
                                || outcome.next_offset <= offset
                                || outcome.bytes.is_empty()
                            {
                                return Err("archive entry read failed".to_owned());
                            }
                            file.write_all(&outcome.bytes)
                                .map_err(|error| error.to_string())?;
                            offset = outcome.next_offset;
                            if outcome.end_of_entry {
                                break;
                            }
                        }
                        if offset != entry.size {
                            return Err("archive entry size mismatch".to_owned());
                        }
                        file.flush()
                            .and_then(|()| file.sync_all())
                            .map_err(|error| error.to_string())
                    })();
                    drop(file);
                    if let Err(error) = write_result {
                        let _ = std::fs::remove_file(&partial);
                        return Err(error);
                    }
                    if let Err(error) = std::fs::rename(&partial, &target) {
                        let _ = std::fs::remove_file(&partial);
                        return Err(error.to_string());
                    }
                }
                Ok(())
            })();
            let outcome = match result {
                Ok(()) => explorer_model::OperationTerminal::Finished,
                Err(_) if context.cancellation.is_cancelled() => {
                    explorer_model::OperationTerminal::Cancelled
                }
                Err(message) => {
                    explorer_model::OperationTerminal::Failed(explorer_common::ExplorerError::new(
                        explorer_common::ExplorerErrorKind::Extension,
                        "virtual archive extract",
                        true,
                        "The archive item could not be extracted.",
                        message,
                    ))
                }
            };
            let _ = sender.send(ExplorerEvent::OperationFinished { context, outcome });
        });
        Some(Ok(()))
    }

    fn submit_virtual_undo(
        &self,
        context: explorer_model::RequestContext,
        request: &explorer_model::ContextMenuRequest,
    ) -> Option<Result<(), ExplorerServiceError>> {
        if !request
            .requested_verb
            .as_deref()
            .is_some_and(|verb| verb.eq_ignore_ascii_case("undo"))
        {
            return None;
        }
        let explorer_model::ShellContextMenuTarget::Background { parent } = &request.target else {
            return None;
        };
        let explorer_model::LocationDescriptor::Virtual(location) = parent else {
            return None;
        };
        let identity = location.container_identity;
        let record = self
            .virtual_containers
            .lock()
            .ok()?
            .get(&identity)
            .cloned()?;
        let (backup, generation) = self
            .virtual_mutation_undo
            .lock()
            .ok()?
            .get(&identity)
            .cloned()?;
        let runtime = self.virtual_folder.as_ref()?.clone();
        let sender = self.sender.clone();
        let containers = Arc::clone(&self.virtual_containers);
        let refresh_remaps = Arc::clone(&self.virtual_refresh_remaps);
        let undo = Arc::clone(&self.virtual_mutation_undo);
        std::thread::spawn(move || {
            let result = runtime
                .lock()
                .map_err(|_| "virtual provider closed".to_owned())
                .and_then(|runtime| {
                    explorer_extension_host::undo_virtual_container_mutation_v1(
                        &runtime,
                        SEVEN_Z_PROVIDER_V1,
                        &record.path,
                        &backup,
                        generation,
                        record.secret.snapshot(),
                    )
                });
            let outcome = match result {
                Ok(new_generation) => {
                    if let Ok(mut remaps) = refresh_remaps.lock() {
                        remaps.retain(|(container, _), _| *container != identity);
                        remaps.insert((identity, record.generation), new_generation);
                    }
                    if let Ok(mut undo) = undo.lock() {
                        undo.remove(&identity);
                    }
                    if let Ok(mut containers) = containers.lock()
                        && let Some(current) = containers.get_mut(&identity)
                    {
                        current.generation = new_generation;
                        if let Ok(mut entries) = current.entries.lock() {
                            entries.clear();
                        }
                    }
                    explorer_model::ContextMenuOutcome::Invoked { command_offset: 0 }
                }
                Err(message) => explorer_model::ContextMenuOutcome::Failed {
                    error: explorer_common::ExplorerError::new(
                        explorer_common::ExplorerErrorKind::Extension,
                        "virtual archive undo",
                        true,
                        "The archive change could not be undone.",
                        message,
                    ),
                },
            };
            let _ = sender.send(ExplorerEvent::ContextMenuFinished { context, outcome });
        });
        Some(Ok(()))
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
    fn cache_telemetry_snapshot(&self) -> explorer_model::CacheTelemetrySnapshotV1 {
        let configuration_pending = crate::application::mft_budget_configuration_pending_v1();
        let diagnostics_result = (!configuration_pending).then(crate::mft_query::query_diagnostics);
        let diagnostics = diagnostics_result
            .as_ref()
            .and_then(|result| result.as_ref().ok())
            .copied();
        let missing_availability = mft_missing_telemetry_availability(
            diagnostics_result
                .as_ref()
                .and_then(|result| result.as_ref().err())
                .map(String::as_str),
        );
        let availability = diagnostics.map_or(missing_availability, |diagnostics| {
            explorer_model::CacheTelemetryAvailabilityV1::Available(
                explorer_model::CacheTelemetryValueV1 {
                    bytes: diagnostics.lru_bytes,
                    limit_bytes: Some(diagnostics.limit_bytes),
                    entry_count: diagnostics.entry_count,
                    counters: Some(explorer_model::CacheTelemetryCountersV1 {
                        hits: diagnostics.hits,
                        misses: diagnostics.misses,
                    }),
                },
            )
        });
        let (extension_bytes, extension_entries) =
            crate::application::host_extension_cache_telemetry_v1();
        let (extension_disk_bytes, extension_disk_entries) =
            crate::application::host_extension_persistent_cache_telemetry_v1();
        let icon_disk = explorer_shell_win::icon_disk_cache_stats();
        let thumbnail_disk = explorer_shell_win::thumbnail_disk_cache_stats();
        let (icon_gpu, thumbnail_gpu) = gpui::compressed_gpu_cache_stats();
        let bc7_pipeline = explorer_shell_win::bc7_pipeline_stats();
        let bc7_jobs = explorer_shell_win::bc7_job_stats();
        explorer_model::CacheTelemetrySnapshotV1::new(vec![
            explorer_model::CacheTelemetryEntryV1 {
                id: explorer_model::CacheTelemetryIdV1::Bc7Pipeline,
                category: explorer_model::CacheTelemetryCategoryV1::Pipeline,
                availability: explorer_model::CacheTelemetryAvailabilityV1::Available(
                    explorer_model::CacheTelemetryValueV1 {
                        bytes: bc7_pipeline.active_staging_bytes,
                        limit_bytes: Some(bc7_pipeline.staging_limit_bytes),
                        entry_count: bc7_pipeline.active_encoders,
                        counters: Some(explorer_model::CacheTelemetryCountersV1 {
                            hits: bc7_pipeline.encode_count,
                            misses: bc7_pipeline.encode_errors,
                        }),
                    },
                ),
            },
            explorer_model::CacheTelemetryEntryV1 {
                id: explorer_model::CacheTelemetryIdV1::ExtensionColumnsMemory,
                category: explorer_model::CacheTelemetryCategoryV1::Memory,
                availability: explorer_model::CacheTelemetryAvailabilityV1::Available(
                    explorer_model::CacheTelemetryValueV1 {
                        bytes: extension_bytes,
                        limit_bytes: Some(crate::application::host_extension_cache_limit_v1()),
                        entry_count: extension_entries,
                        counters: None,
                    },
                ),
            },
            explorer_model::CacheTelemetryEntryV1 {
                id: explorer_model::CacheTelemetryIdV1::IconsDisk,
                category: explorer_model::CacheTelemetryCategoryV1::Disk,
                availability: explorer_model::CacheTelemetryAvailabilityV1::Available(
                    explorer_model::CacheTelemetryValueV1 {
                        bytes: icon_disk.bytes,
                        limit_bytes: Some(icon_disk.limit_bytes),
                        entry_count: icon_disk.entries,
                        counters: Some(explorer_model::CacheTelemetryCountersV1 {
                            hits: icon_disk.hits,
                            misses: icon_disk.misses,
                        }),
                    },
                ),
            },
            explorer_model::CacheTelemetryEntryV1 {
                id: explorer_model::CacheTelemetryIdV1::ThumbnailsDisk,
                category: explorer_model::CacheTelemetryCategoryV1::Disk,
                availability: explorer_model::CacheTelemetryAvailabilityV1::Available(
                    explorer_model::CacheTelemetryValueV1 {
                        bytes: thumbnail_disk.bytes,
                        limit_bytes: Some(thumbnail_disk.limit_bytes),
                        entry_count: thumbnail_disk.entries,
                        counters: Some(explorer_model::CacheTelemetryCountersV1 {
                            hits: thumbnail_disk.hits,
                            misses: thumbnail_disk.misses,
                        }),
                    },
                ),
            },
            explorer_model::CacheTelemetryEntryV1 {
                id: explorer_model::CacheTelemetryIdV1::ExtensionColumnsDisk,
                category: explorer_model::CacheTelemetryCategoryV1::Disk,
                availability: explorer_model::CacheTelemetryAvailabilityV1::Available(
                    explorer_model::CacheTelemetryValueV1 {
                        bytes: extension_disk_bytes,
                        limit_bytes: Some(
                            crate::application::host_extension_persistent_cache_limit_v1(),
                        ),
                        entry_count: extension_disk_entries,
                        counters: None,
                    },
                ),
            },
            explorer_model::CacheTelemetryEntryV1 {
                id: explorer_model::CacheTelemetryIdV1::MftPersistedIndex,
                category: explorer_model::CacheTelemetryCategoryV1::MftService,
                availability: diagnostics.map_or(missing_availability, |diagnostics| {
                    explorer_model::CacheTelemetryAvailabilityV1::Available(
                        explorer_model::CacheTelemetryValueV1 {
                            bytes: diagnostics.persisted_index_bytes,
                            limit_bytes: diagnostics.persisted_index_limit_bytes,
                            entry_count: 0,
                            counters: None,
                        },
                    )
                }),
            },
            explorer_model::CacheTelemetryEntryV1 {
                id: explorer_model::CacheTelemetryIdV1::MftVolumeIndexMemory,
                category: explorer_model::CacheTelemetryCategoryV1::MftService,
                availability: diagnostics
                    .and_then(|diagnostics| {
                        diagnostics
                            .volume_index_bytes
                            .map(|bytes| (bytes, diagnostics.volume_index_limit_bytes))
                    })
                    .map_or(
                        explorer_model::CacheTelemetryAvailabilityV1::Pending,
                        |(bytes, limit_bytes)| {
                            explorer_model::CacheTelemetryAvailabilityV1::Available(
                                explorer_model::CacheTelemetryValueV1 {
                                    bytes,
                                    limit_bytes,
                                    entry_count: 0,
                                    counters: None,
                                },
                            )
                        },
                    ),
            },
            explorer_model::CacheTelemetryEntryV1 {
                id: explorer_model::CacheTelemetryIdV1::MftFileDataMemory,
                category: explorer_model::CacheTelemetryCategoryV1::MftService,
                availability: diagnostics
                    .and_then(|diagnostics| {
                        diagnostics
                            .file_data_bytes
                            .map(|bytes| (bytes, diagnostics.file_data_limit_bytes))
                    })
                    .map_or(
                        explorer_model::CacheTelemetryAvailabilityV1::Pending,
                        |(bytes, limit_bytes)| {
                            explorer_model::CacheTelemetryAvailabilityV1::Available(
                                explorer_model::CacheTelemetryValueV1 {
                                    bytes,
                                    limit_bytes,
                                    entry_count: 0,
                                    counters: None,
                                },
                            )
                        },
                    ),
            },
            explorer_model::CacheTelemetryEntryV1 {
                id: explorer_model::CacheTelemetryIdV1::MftAggregateMemory,
                category: explorer_model::CacheTelemetryCategoryV1::MftService,
                availability: diagnostics
                    .and_then(|diagnostics| {
                        diagnostics
                            .aggregate_bytes
                            .map(|bytes| (bytes, diagnostics.aggregate_limit_bytes))
                    })
                    .map_or(
                        explorer_model::CacheTelemetryAvailabilityV1::Pending,
                        |(bytes, limit_bytes)| {
                            explorer_model::CacheTelemetryAvailabilityV1::Available(
                                explorer_model::CacheTelemetryValueV1 {
                                    bytes,
                                    limit_bytes,
                                    entry_count: 0,
                                    counters: None,
                                },
                            )
                        },
                    ),
            },
            explorer_model::CacheTelemetryEntryV1 {
                id: explorer_model::CacheTelemetryIdV1::IconsGpu,
                category: explorer_model::CacheTelemetryCategoryV1::Gpu,
                availability: explorer_model::CacheTelemetryAvailabilityV1::Available(
                    explorer_model::CacheTelemetryValueV1 {
                        bytes: icon_gpu.bytes,
                        limit_bytes: Some(icon_gpu.limit_bytes),
                        entry_count: icon_gpu.entries,
                        counters: None,
                    },
                ),
            },
            explorer_model::CacheTelemetryEntryV1 {
                id: explorer_model::CacheTelemetryIdV1::ThumbnailsGpu,
                category: explorer_model::CacheTelemetryCategoryV1::Gpu,
                availability: explorer_model::CacheTelemetryAvailabilityV1::Available(
                    explorer_model::CacheTelemetryValueV1 {
                        bytes: thumbnail_gpu.bytes,
                        limit_bytes: Some(thumbnail_gpu.limit_bytes),
                        entry_count: thumbnail_gpu.entries,
                        counters: None,
                    },
                ),
            },
            explorer_model::CacheTelemetryEntryV1 {
                id: explorer_model::CacheTelemetryIdV1::MftServiceLru,
                category: explorer_model::CacheTelemetryCategoryV1::MftService,
                availability,
            },
        ])
        .map(|snapshot| {
            snapshot.with_bc7_pipeline(explorer_model::Bc7PipelineTelemetryV1 {
                queued_jobs: bc7_jobs.queued_jobs,
                queue_limit: bc7_jobs.queue_limit,
                peak_queued_jobs: bc7_jobs.peak_queued_jobs,
                active_jobs: bc7_jobs.active_jobs,
                concurrency_limit: bc7_jobs.concurrency_limit,
                reserved_staging_bytes: bc7_jobs.reserved_staging_bytes,
                staging_limit_bytes: bc7_jobs.staging_limit_bytes,
                submitted_jobs: bc7_jobs.submitted_jobs,
                completed_jobs: bc7_jobs.completed_jobs,
                duplicate_jobs: bc7_jobs.duplicate_jobs,
                overload_rejections: bc7_jobs.overload_rejections,
                oversized_rejections: bc7_jobs.oversized_rejections,
                cancelled_jobs: bc7_jobs.cancelled_jobs,
                stale_jobs: bc7_jobs.stale_jobs,
                persist_errors: bc7_jobs.persist_errors,
                fallbacks: bc7_jobs.fallbacks,
                icon_gpu_uploads: icon_gpu.uploads,
                icon_gpu_evictions: icon_gpu.evictions,
                thumbnail_gpu_uploads: thumbnail_gpu.uploads,
                thumbnail_gpu_evictions: thumbnail_gpu.evictions,
                gpu_supported: icon_gpu.supported,
            })
        })
        .unwrap_or_default()
    }

    fn submit(&self, command: ExplorerCommand) -> Result<(), ExplorerServiceError> {
        match &command {
            ExplorerCommand::Navigate { context, location }
            | ExplorerCommand::Refresh { context, location } => {
                if let Some(result) =
                    self.submit_virtual_navigation(context.clone(), location.clone())
                {
                    return result;
                }
            }
            ExplorerCommand::OpenItem {
                context,
                item,
                disposition,
            } => {
                if let Some((record, entry)) = self.virtual_entry(&item.location) {
                    if entry.is_container {
                        if let Some(result) =
                            self.submit_virtual_navigation(context.clone(), item.location.clone())
                        {
                            return result;
                        }
                    } else {
                        return self.submit_virtual_file_open(
                            context.clone(),
                            item.clone(),
                            *disposition,
                            record,
                            entry,
                        );
                    }
                } else if let Some(result) =
                    self.submit_virtual_navigation(context.clone(), item.location.clone())
                {
                    return result;
                }
            }
            ExplorerCommand::ResolveAncestry { context, location }
                if matches!(location, explorer_model::LocationDescriptor::Virtual(_)) =>
            {
                let segments =
                    if let explorer_model::LocationDescriptor::Virtual(virtual_location) = location
                    {
                        self.virtual_containers
                            .lock()
                            .ok()
                            .and_then(|containers| {
                                containers
                                    .get(&virtual_location.container_identity)
                                    .cloned()
                            })
                            .map_or_else(
                                || explorer_model::location_breadcrumbs(location),
                                |record| {
                                    let mut segments = explorer_model::location_breadcrumbs(
                                        &explorer_model::LocationDescriptor::file_system(
                                            record.path,
                                        ),
                                    );
                                    segments.extend(
                                        explorer_model::location_breadcrumbs(location)
                                            .into_iter()
                                            .skip(1),
                                    );
                                    segments
                                },
                            )
                    } else {
                        Vec::new()
                    };
                let sender = self.sender.clone();
                let context = context.clone();
                std::thread::spawn(move || {
                    if sender
                        .send(ExplorerEvent::AncestryBatch {
                            context: context.clone(),
                            segments,
                        })
                        .is_ok()
                    {
                        let _ = sender.send(ExplorerEvent::AncestryFinished {
                            context,
                            outcome: explorer_model::BreadcrumbTerminal::Finished,
                        });
                    }
                });
                return Ok(());
            }
            ExplorerCommand::ExecuteFileOperation { context, request } => {
                if let Some(result) = self.submit_virtual_create_operation(context.clone(), request)
                {
                    return result;
                }
                if let Some(result) =
                    self.submit_virtual_extract_operation(context.clone(), request)
                {
                    return result;
                }
                if let Some(result) =
                    self.submit_virtual_file_operation(context.clone(), request.clone())
                {
                    return result;
                }
            }
            ExplorerCommand::ShowContextMenu { context, request } => {
                if let Some(result) = self.submit_virtual_undo(context.clone(), request) {
                    return result;
                }
            }
            ExplorerCommand::DataTransfer { context, request } => {
                if let Some(result) = self.submit_virtual_begin_drag(context.clone(), request) {
                    return result;
                }
            }
            _ => {}
        }
        match command {
            ExplorerCommand::LoadShellIcon { context, key }
                if matches!(key.location, explorer_model::LocationDescriptor::Virtual(_)) =>
            {
                self.submit_virtual_icon(context, key)
            }
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
            Err(TryRecvError::Empty) => ExplorerService::try_recv(self.shell.as_ref())
                .map(|event| event.map(|event| self.restore_virtual_icon_key(event))),
            Err(TryRecvError::Disconnected) => Err(ExplorerServiceError::Disconnected),
        }
    }
}

fn mft_missing_telemetry_availability(
    error: Option<&str>,
) -> explorer_model::CacheTelemetryAvailabilityV1 {
    if error.is_some_and(|error| error.contains("schema mismatch") || error.contains("rejected")) {
        explorer_model::CacheTelemetryAvailabilityV1::Unavailable
    } else {
        explorer_model::CacheTelemetryAvailabilityV1::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::{
        decode_trusted_raster, is_dedicated_raster_preview, is_host_owned_context_verb,
        virtual_container_record,
    };

    #[test]
    fn virtual_container_identity_survives_content_change_while_generation_advances() {
        let root = std::env::temp_dir().join(format!(
            "superexplorer-container-identity-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let archive = root.join("fixture.7z");
        std::fs::write(&archive, b"first").unwrap();
        let (first_identity, first) = virtual_container_record(&archive).unwrap();
        std::fs::write(&archive, b"second-generation").unwrap();
        let (second_identity, second) = virtual_container_record(&archive).unwrap();
        assert_eq!(first_identity, second_identity);
        assert_ne!(first.generation, second.generation);
        std::fs::remove_dir_all(root).unwrap();
    }

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

    #[test]
    fn mft_missing_telemetry_is_pending_until_a_terminal_protocol_failure() {
        assert_eq!(
            super::mft_missing_telemetry_availability(None),
            explorer_model::CacheTelemetryAvailabilityV1::Pending
        );
        assert_eq!(
            super::mft_missing_telemetry_availability(Some("MFT query pipe unavailable (2)")),
            explorer_model::CacheTelemetryAvailabilityV1::Pending
        );
        assert_eq!(
            super::mft_missing_telemetry_availability(Some(
                "MFT diagnostics response schema mismatch"
            )),
            explorer_model::CacheTelemetryAvailabilityV1::Unavailable
        );
    }
}
