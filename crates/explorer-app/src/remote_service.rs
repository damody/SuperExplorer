//! App-owned routing for ADB/SFTP navigation and cross-filesystem transfers.

use std::{
    hash::{Hash as _, Hasher as _},
    sync::{
        Arc, Mutex,
        mpsc::{Receiver, SyncSender, TryRecvError, TrySendError},
    },
};

use explorer_model::{
    ClipboardMode, ClipboardState, DataTransferRequest, ExplorerCommand, ExplorerEvent,
    ExplorerService, ExplorerServiceError, FileEntry, FileEntryMetadata, FileOperationKind,
    ItemDescriptor, LocationDescriptor, LocationMetadata, NamespaceCapabilities,
    OperationItemOutcome, OperationItemResult, OperationTerminal, TransferEffects,
};
use explorer_remote::{
    RemoteEntry, RemoteProvider, RemoteProviderRegistry, TransferEngine, TransferMode,
    TransferResult,
};

fn remote_file_entry(entry: RemoteEntry) -> Option<FileEntry> {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    entry.location.hash(&mut hasher);
    Some(FileEntry {
        id: explorer_model::ShellItemId::from_provider_bytes(hasher.finish().to_le_bytes())?,
        display_name: entry.name,
        location: entry.location,
        is_container: entry.kind.is_container(),
        metadata: FileEntryMetadata {
            size_bytes: entry.size,
            unix_mode: entry.unix_mode,
            type_display: Some(entry.kind.type_display().to_owned()),
            namespace_capabilities: NamespaceCapabilities::from_public_bits(
                NamespaceCapabilities::OPEN
                    | NamespaceCapabilities::COPY
                    | NamespaceCapabilities::RENAME
                    | NamespaceCapabilities::DELETE,
            ),
            ..FileEntryMetadata::default()
        },
    })
}

fn remote_child_container(entry: RemoteEntry) -> Option<explorer_model::BreadcrumbMenuItem> {
    entry
        .kind
        .is_container()
        .then_some(explorer_model::BreadcrumbMenuItem {
            display_name: entry.name,
            location: entry.location,
        })
}

pub struct ConfiguredRemoteRuntime {
    pub providers: Arc<RemoteProviderRegistry>,
    sftp: Option<Arc<explorer_remote::SftpProvider>>,
}

pub fn configured_remote_runtime() -> Arc<ConfiguredRemoteRuntime> {
    let mut registry = RemoteProviderRegistry::default();
    let mut sftp_runtime = None;

    if let Ok(client) = explorer_remote::AdbClient::discover() {
        let provider = Arc::new(explorer_remote::AdbProvider::new(client));
        if let Ok(devices) = provider.client_devices() {
            for device in devices
                .into_iter()
                .filter(|device| device.state == explorer_remote::AdbDeviceState::Device)
            {
                let identity = explorer_model::remote_container_identity(
                    explorer_model::RemoteProviderKind::Adb,
                    &device.serial,
                );
                let _ = provider.register_device(identity, device.serial);
            }
        }
        let _ = registry.register(provider);
    }

    if let Ok(provider) = explorer_remote::SftpProvider::new() {
        let provider = Arc::new(provider);
        for profile in load_sftp_profiles() {
            if let Ok(Some(password)) =
                explorer_automation_win::load_windows_credential(&profile.credential_target())
            {
                let _ = provider.register_profile(profile, password);
            }
        }
        let _ = registry.register(provider.clone());
        sftp_runtime = Some(provider);
    }

    Arc::new(ConfiguredRemoteRuntime {
        providers: Arc::new(registry),
        sftp: sftp_runtime,
    })
}

impl ConfiguredRemoteRuntime {
    pub fn login_address(&self, input: &str) -> Result<Option<LocationDescriptor>, String> {
        let parsed =
            explorer_model::SftpAddressInput::parse(input).map_err(|error| error.to_string())?;
        let host = parsed.address.authority.clone();
        let saved = load_sftp_profiles()
            .into_iter()
            .find(|profile| profile.alias == host);
        if let Some(profile) = saved.as_ref()
            && profile.host_key_fingerprint.is_some()
            && explorer_automation_win::load_windows_credential(&profile.credential_target())
                .ok()
                .flatten()
                .is_some()
        {
            return parsed
                .address
                .to_location(profile.container_identity, 1)
                .map(Some)
                .map_err(|error| error.to_string());
        }
        let suggested_user = parsed
            .username_hint
            .or_else(|| saved.as_ref().map(|profile| profile.username.clone()))
            .unwrap_or_default();
        let Some((username, password)) = prompt_sftp_login(&host, &suggested_user)? else {
            return Ok(None);
        };
        let provider = self
            .sftp
            .as_ref()
            .ok_or_else(|| "SFTP runtime is unavailable.".to_owned())?;
        let fingerprint = provider
            .probe_host_key(&host, 22)
            .map_err(|_| "Unable to read the SFTP server host key.".to_owned())?;
        if saved
            .as_ref()
            .and_then(|profile| profile.host_key_fingerprint.as_ref())
            .is_some_and(|expected| expected != &fingerprint)
        {
            return Err("The SFTP server host key changed; login was blocked.".to_owned());
        }
        let identity = saved.as_ref().map_or_else(
            || {
                explorer_model::remote_container_identity(
                    explorer_model::RemoteProviderKind::Sftp,
                    &host,
                )
            },
            |profile| profile.container_identity,
        );
        let mut profile =
            explorer_model::SftpProfile::new(host.clone(), host.clone(), 22, username, identity)
                .map_err(|error| error.to_string())?;
        profile.host_key_fingerprint = Some(fingerprint);
        provider
            .register_profile(profile.clone(), password.clone())
            .map_err(|_| "SFTP login information is invalid.".to_owned())?;
        let location = parsed
            .address
            .to_location(identity, 1)
            .map_err(|error| error.to_string())?;
        let LocationDescriptor::Virtual(remote) = &location else {
            return Err("SFTP address is invalid.".to_owned());
        };
        if provider
            .list(remote, &explorer_model::CancellationToken::new())
            .is_err()
        {
            provider.remove_profile(identity);
            if let Some(previous) = saved
                && let Ok(Some(previous_password)) =
                    explorer_automation_win::load_windows_credential(&previous.credential_target())
            {
                let _ = provider.register_profile(previous, previous_password);
            }
            return Err("SFTP authentication failed.".to_owned());
        }
        explorer_automation_win::store_windows_credential(&profile.credential_target(), password)
            .map_err(|_| "Unable to save the SFTP credential.".to_owned())?;
        if let Err(error) = persist_sftp_profile(profile.clone()) {
            let _ =
                explorer_automation_win::remove_windows_credential(&profile.credential_target());
            return Err(error);
        }
        explorer_ui::navigation_pane::configure_sftp_navigation_profiles(
            configured_sftp_navigation_profiles(),
        );
        Ok(Some(location))
    }
}

fn persist_sftp_profile(profile: explorer_model::SftpProfile) -> Result<(), String> {
    let local = std::env::var_os("LOCALAPPDATA")
        .ok_or_else(|| "LOCALAPPDATA is unavailable.".to_owned())?;
    let directory = std::path::PathBuf::from(local)
        .join("RustGpuiExplorer")
        .join("remote");
    std::fs::create_dir_all(&directory)
        .map_err(|_| "Unable to create the SFTP profile directory.".to_owned())?;
    let path = directory.join("sftp-profiles.json");
    let mut profiles = load_sftp_profiles();
    if let Some(existing) = profiles.iter_mut().find(|item| item.alias == profile.alias) {
        *existing = profile;
    } else {
        profiles.push(profile);
    }
    let bytes = serde_json::to_vec_pretty(&profiles)
        .map_err(|_| "Unable to encode the SFTP profile.".to_owned())?;
    let temporary = directory.join("sftp-profiles.json.tmp");
    std::fs::write(&temporary, bytes)
        .map_err(|_| "Unable to write the SFTP profile.".to_owned())?;
    replace_profile_file(&temporary, &path)
}

#[cfg(windows)]
#[expect(
    unsafe_code,
    reason = "atomic SFTP profile activation requires declaring and invoking Win32 MoveFileExW"
)]
fn replace_profile_file(
    source: &std::path::Path,
    destination: &std::path::Path,
) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt as _;
    // SAFETY: This declaration matches kernel32's documented MoveFileExW
    // system ABI; callers provide NUL-terminated UTF-16 path buffers.
    unsafe extern "system" {
        fn MoveFileExW(existing: *const u16, replacement: *const u16, flags: u32) -> i32;
    }
    let source = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    if unsafe { MoveFileExW(source.as_ptr(), destination.as_ptr(), 0x1 | 0x8) } == 0 {
        return Err("Unable to activate the SFTP profile.".to_owned());
    }
    Ok(())
}

#[cfg(not(windows))]
fn replace_profile_file(
    source: &std::path::Path,
    destination: &std::path::Path,
) -> Result<(), String> {
    std::fs::rename(source, destination)
        .map_err(|_| "Unable to activate the SFTP profile.".to_owned())
}

#[cfg(windows)]
fn prompt_sftp_login(host: &str, suggested_user: &str) -> Result<Option<(String, String)>, String> {
    use windows::{
        Win32::{
            Foundation::{ERROR_CANCELLED, ERROR_SUCCESS},
            Security::Credentials::{
                CREDUI_FLAGS_ALWAYS_SHOW_UI, CREDUI_FLAGS_DO_NOT_PERSIST,
                CREDUI_FLAGS_GENERIC_CREDENTIALS, CREDUI_INFOW, CredUIPromptForCredentialsW,
            },
        },
        core::PCWSTR,
    };
    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }
    fn wide_input_buffer(value: &str, max_chars: usize) -> Vec<u16> {
        let mut buffer = value.encode_utf16().take(max_chars).collect::<Vec<_>>();
        buffer.resize(max_chars + 1, 0);
        buffer
    }
    let caption = wide("SuperExplorer SFTP Login");
    let message = wide(&format!("Sign in to {host}"));
    let target = wide(&format!("SuperExplorer/SFTP/{host}"));
    let info = CREDUI_INFOW {
        cbSize: std::mem::size_of::<CREDUI_INFOW>() as u32,
        pszMessageText: PCWSTR(message.as_ptr()),
        pszCaptionText: PCWSTR(caption.as_ptr()),
        ..Default::default()
    };
    // wincred.h defines these limits without the terminating NUL:
    // CREDUI_MAX_USERNAME_LENGTH = 513 and CREDUI_MAX_PASSWORD_LENGTH = 256.
    // Passing a larger password buffer trips the Universal CRT invalid-parameter
    // handler and terminates the process with 0xc0000409 instead of returning an
    // error from CredUIPromptForCredentialsW.
    let mut username = wide_input_buffer(suggested_user, 513);
    let mut password = vec![0_u16; 257];
    // SAFETY: The descriptor and target buffers remain live and NUL-terminated,
    // and the mutable username/password buffers retain their full capacity for
    // the duration of this synchronous credential dialog call.
    #[expect(
        unsafe_code,
        reason = "the Windows SFTP credential dialog uses raw UTF-16 buffer pointers"
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
            CREDUI_FLAGS_ALWAYS_SHOW_UI
                | CREDUI_FLAGS_DO_NOT_PERSIST
                | CREDUI_FLAGS_GENERIC_CREDENTIALS,
        )
    };
    if result == ERROR_CANCELLED {
        password.fill(0);
        return Ok(None);
    }
    if result != ERROR_SUCCESS {
        password.fill(0);
        return Err("Unable to open the SFTP login page.".to_owned());
    }
    let decode = |value: &[u16]| {
        String::from_utf16_lossy(
            &value[..value
                .iter()
                .position(|unit| *unit == 0)
                .unwrap_or(value.len())],
        )
    };
    let username_value = decode(&username);
    let password_value = decode(&password);
    password.fill(0);
    std::hint::black_box(&mut password);
    if username_value.is_empty() || password_value.is_empty() {
        return Err("Username and password are required.".to_owned());
    }
    Ok(Some((username_value, password_value)))
}

#[cfg(not(windows))]
fn prompt_sftp_login(_: &str, _: &str) -> Result<Option<(String, String)>, String> {
    Err("SFTP login is available only on Windows.".to_owned())
}

pub fn discover_adb_navigation_devices() -> Vec<explorer_ui::navigation_pane::AdbNavigationDevice> {
    let Ok(client) = explorer_remote::AdbClient::discover() else {
        return Vec::new();
    };
    let Ok(devices) = client.devices() else {
        return Vec::new();
    };
    devices
        .into_iter()
        .map(|device| {
            let available = device.state == explorer_remote::AdbDeviceState::Device;
            let base = device.model.unwrap_or_else(|| device.serial.clone());
            let label = if base == device.serial {
                base
            } else {
                format!("{base} ({})", device.serial)
            };
            let label = match device.state {
                explorer_remote::AdbDeviceState::Offline => format!("{label} — 離線"),
                explorer_remote::AdbDeviceState::Unauthorized => format!("{label} — 未授權"),
                explorer_remote::AdbDeviceState::Other => format!("{label} — 無法使用"),
                explorer_remote::AdbDeviceState::Device => label,
            };
            explorer_ui::navigation_pane::AdbNavigationDevice {
                serial: device.serial,
                label,
                available,
            }
        })
        .collect()
}

pub fn configured_sftp_navigation_profiles()
-> Vec<explorer_ui::navigation_pane::SftpNavigationProfile> {
    load_sftp_profiles()
        .into_iter()
        .map(|profile| {
            let available = profile.host_key_fingerprint.is_some()
                && explorer_automation_win::load_windows_credential(&profile.credential_target())
                    .ok()
                    .flatten()
                    .is_some();
            explorer_ui::navigation_pane::SftpNavigationProfile {
                alias: profile.alias.clone(),
                label: if available {
                    profile.alias
                } else {
                    format!("{} — 尚未連線", profile.alias)
                },
                container_identity: profile.container_identity,
                available,
            }
        })
        .collect()
}

pub fn start_adb_navigation_refresh() {
    static STARTED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    STARTED.get_or_init(|| {
        std::thread::spawn(|| {
            loop {
                std::thread::sleep(std::time::Duration::from_secs(5));
                explorer_ui::navigation_pane::configure_adb_navigation_devices(
                    discover_adb_navigation_devices(),
                );
            }
        });
    });
}

fn load_sftp_profiles() -> Vec<explorer_model::SftpProfile> {
    let Some(local) = std::env::var_os("LOCALAPPDATA") else {
        return Vec::new();
    };
    let path = std::path::PathBuf::from(local)
        .join("RustGpuiExplorer")
        .join("remote")
        .join("sftp-profiles.json");
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

#[derive(Clone)]
struct RemoteClipboard {
    mode: ClipboardMode,
    items: Vec<ItemDescriptor>,
}

pub struct RemoteExplorerService {
    inner: Arc<dyn ExplorerService>,
    providers: Arc<RemoteProviderRegistry>,
    sender: SyncSender<ExplorerEvent>,
    receiver: Mutex<Receiver<ExplorerEvent>>,
    clipboard: Arc<Mutex<Option<RemoteClipboard>>>,
    clipboard_generation: Arc<Mutex<u64>>,
    drag_staging: Arc<Mutex<Vec<tempfile::TempDir>>>,
}

impl RemoteExplorerService {
    pub fn new(inner: Arc<dyn ExplorerService>, providers: Arc<RemoteProviderRegistry>) -> Self {
        let (sender, receiver) = std::sync::mpsc::sync_channel(256);
        Self {
            inner,
            providers,
            sender,
            receiver: Mutex::new(receiver),
            clipboard: Arc::new(Mutex::new(None)),
            clipboard_generation: Arc::new(Mutex::new(0)),
            drag_staging: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn is_remote(location: &LocationDescriptor) -> bool {
        matches!(location, LocationDescriptor::Virtual(location) if matches!(location.provider_id.as_str(), "adb" | "sftp"))
    }

    fn submit_navigation(
        &self,
        context: explorer_model::RequestContext,
        location: LocationDescriptor,
    ) -> Result<(), ExplorerServiceError> {
        let providers = Arc::clone(&self.providers);
        let sender = self.sender.clone();
        std::thread::spawn(move || {
            let outcome = (|| {
                let LocationDescriptor::Virtual(remote) = &location else {
                    return Err(anyhow::anyhow!("remote location is invalid"));
                };
                let provider = providers.resolve(&location)?;
                let entries = provider.list(remote, &context.cancellation)?;
                if context.cancellation.is_cancelled() {
                    return Ok(());
                }
                let title = remote
                    .components
                    .last()
                    .cloned()
                    .or_else(|| remote.public_authority.clone())
                    .unwrap_or_else(|| remote.provider_id.to_uppercase());
                sender
                    .send(ExplorerEvent::LocationResolved {
                        context: context.clone(),
                        metadata: LocationMetadata {
                            descriptor: location.clone(),
                            display_title: title,
                            can_go_up: !remote.components.is_empty(),
                            can_write: true,
                        },
                    })
                    .map_err(|_| anyhow::anyhow!("remote event receiver disconnected"))?;
                let rows = entries.into_iter().filter_map(remote_file_entry).collect();
                sender
                    .send(ExplorerEvent::DirectoryBatch {
                        context: context.clone(),
                        entries: rows,
                    })
                    .map_err(|_| anyhow::anyhow!("remote event receiver disconnected"))?;
                sender
                    .send(ExplorerEvent::DirectoryFinished {
                        context: context.clone(),
                    })
                    .map_err(|_| anyhow::anyhow!("remote event receiver disconnected"))?;
                Ok::<_, anyhow::Error>(())
            })();
            if let Err(error) = outcome
                && !context.cancellation.is_cancelled()
            {
                let _ = sender.send(remote_failed(
                    context,
                    "Remote directory is unavailable.",
                    error,
                ));
            }
        });
        Ok(())
    }

    fn submit_ancestry(
        &self,
        context: explorer_model::RequestContext,
        location: LocationDescriptor,
    ) -> Result<(), ExplorerServiceError> {
        let sender = self.sender.clone();
        std::thread::spawn(move || {
            let LocationDescriptor::Virtual(remote) = location else {
                return;
            };
            let mut segments = Vec::with_capacity(remote.components.len().saturating_add(1));
            for count in 0..=remote.components.len() {
                let mut segment_location = remote.clone();
                segment_location.components.truncate(count);
                segment_location.entry_id = None;
                let location = LocationDescriptor::Virtual(segment_location);
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                location.hash(&mut hasher);
                segments.push(explorer_model::BreadcrumbSegment {
                    id: explorer_model::BreadcrumbSegmentId(hasher.finish()),
                    display_name: if count == 0 {
                        remote
                            .public_authority
                            .clone()
                            .unwrap_or_else(|| remote.provider_id.to_uppercase())
                    } else {
                        remote.components[count - 1].clone()
                    },
                    location,
                    icon_hint: if count == 0 {
                        explorer_model::BreadcrumbIconHint::Namespace
                    } else {
                        explorer_model::BreadcrumbIconHint::Folder
                    },
                    is_container: true,
                });
            }
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
        Ok(())
    }

    fn submit_child_containers(
        &self,
        context: explorer_model::RequestContext,
        parent: LocationDescriptor,
        segment_id: explorer_model::BreadcrumbSegmentId,
        menu_generation: u64,
    ) -> Result<(), ExplorerServiceError> {
        let providers = Arc::clone(&self.providers);
        let sender = self.sender.clone();
        std::thread::spawn(move || {
            let result = (|| {
                let LocationDescriptor::Virtual(remote) = &parent else {
                    anyhow::bail!("remote parent is invalid");
                };
                providers
                    .resolve(&parent)?
                    .list(remote, &context.cancellation)
                    .map(|entries| {
                        entries
                            .into_iter()
                            .filter_map(remote_child_container)
                            .collect::<Vec<_>>()
                    })
            })();
            let outcome = match result {
                Ok(children) => {
                    let _ = sender.send(ExplorerEvent::ChildContainersBatch {
                        context: context.clone(),
                        segment_id,
                        menu_generation,
                        children,
                    });
                    explorer_model::BreadcrumbTerminal::Finished
                }
                Err(_) if context.cancellation.is_cancelled() => {
                    explorer_model::BreadcrumbTerminal::Cancelled
                }
                Err(error) => explorer_model::BreadcrumbTerminal::Failed(remote_error(
                    "remote child enumeration",
                    "Remote folders could not be expanded.",
                    error,
                )),
            };
            let _ = sender.send(ExplorerEvent::ChildContainersFinished {
                context,
                segment_id,
                menu_generation,
                outcome,
            });
        });
        Ok(())
    }

    fn submit_operation(
        &self,
        context: explorer_model::RequestContext,
        kind: FileOperationKind,
    ) -> Result<(), ExplorerServiceError> {
        let providers = Arc::clone(&self.providers);
        let sender = self.sender.clone();
        std::thread::spawn(move || {
            let result = execute_operation(&providers, &kind, &context.cancellation);
            let outcome = match result {
                Ok(outcome) => outcome,
                Err(error) if context.cancellation.is_cancelled() => OperationTerminal::Cancelled,
                Err(error) => OperationTerminal::Failed(remote_error(
                    "remote file operation",
                    "The remote file operation failed.",
                    error,
                )),
            };
            let _ = sender.send(ExplorerEvent::OperationFinished { context, outcome });
        });
        Ok(())
    }

    fn submit_copy_or_cut(
        &self,
        items: Vec<ItemDescriptor>,
        mode: ClipboardMode,
    ) -> Result<(), ExplorerServiceError> {
        if items.is_empty() {
            return Err(ExplorerServiceError::Internal);
        }
        *self
            .clipboard
            .lock()
            .map_err(|_| ExplorerServiceError::Internal)? = Some(RemoteClipboard {
            mode,
            items: items.clone(),
        });
        let mut generation = self
            .clipboard_generation
            .lock()
            .map_err(|_| ExplorerServiceError::Internal)?;
        *generation = generation.saturating_add(1);
        self.sender
            .try_send(ExplorerEvent::ClipboardChanged {
                state: ClipboardState::Owned {
                    mode,
                    items: items.clone(),
                    effects: if mode == ClipboardMode::Copy {
                        TransferEffects::COPY
                    } else {
                        TransferEffects::MOVE
                    },
                    generation: *generation,
                },
            })
            .map_err(map_send_error)?;

        let providers = Arc::clone(&self.providers);
        let staging = Arc::clone(&self.drag_staging);
        std::thread::spawn(move || {
            let cancellation = explorer_model::CancellationToken::new();
            let root = match tempfile::Builder::new()
                .prefix("superexplorer-remote-clipboard-")
                .tempdir()
            {
                Ok(root) => root,
                Err(_) => return,
            };
            let mut native_items = Vec::with_capacity(items.len());
            for item in items {
                if let LocationDescriptor::Virtual(remote) = &item.location {
                    let Some(name) = remote.components.last() else {
                        return;
                    };
                    let target = root.path().join(name);
                    let Ok(provider) = providers.resolve(&item.location) else {
                        return;
                    };
                    if provider.download(remote, &target, &cancellation).is_err() {
                        return;
                    }
                    native_items.push(ItemDescriptor {
                        id: item.id,
                        location: LocationDescriptor::file_system(target),
                    });
                } else {
                    native_items.push(item);
                }
            }
            // External consumers receive a copy. A remote cut remains a move only when pasted
            // back through SuperExplorer, where completion can be observed before deletion.
            if explorer_shell_win::publish_native_file_clipboard(native_items, ClipboardMode::Copy)
                .is_ok()
            {
                if let Ok(mut roots) = staging.lock() {
                    roots.push(root);
                }
            }
        });
        Ok(())
    }

    fn submit_paste(
        &self,
        context: explorer_model::RequestContext,
        destination: LocationDescriptor,
    ) -> Result<(), ExplorerServiceError> {
        let internal = self
            .clipboard
            .lock()
            .map_err(|_| ExplorerServiceError::Internal)?
            .clone();
        let providers = Arc::clone(&self.providers);
        let sender = self.sender.clone();
        let clipboard = Arc::clone(&self.clipboard);
        let generation = Arc::clone(&self.clipboard_generation);
        std::thread::spawn(move || {
            let sources = if let Some(value) = internal.clone() {
                Ok((value.items, value.mode))
            } else {
                explorer_shell_win::read_native_file_clipboard()
                    .map(|value| {
                        value.map_or_else(|| (Vec::new(), ClipboardMode::Copy), |value| value)
                    })
                    .map_err(|error| anyhow::anyhow!(error.to_string()))
            };
            let outcome = match sources {
                Ok((items, mode)) if !items.is_empty() => {
                    transfer_items(&providers, items, destination, mode, &context.cancellation)
                }
                Ok(_) => OperationTerminal::Failed(remote_error(
                    "remote clipboard paste",
                    "The clipboard does not contain files.",
                    anyhow::anyhow!("no CF_HDROP or remote items"),
                )),
                Err(error) => OperationTerminal::Failed(remote_error(
                    "remote clipboard paste",
                    "The file clipboard could not be read.",
                    error,
                )),
            };
            let finished_move = internal
                .as_ref()
                .is_some_and(|value| value.mode == ClipboardMode::Cut)
                && matches!(outcome, OperationTerminal::Finished);
            let _ = sender.send(ExplorerEvent::OperationFinished { context, outcome });
            if finished_move {
                if let Ok(mut clipboard) = clipboard.lock() {
                    *clipboard = None;
                }
                if let Ok(mut generation) = generation.lock() {
                    *generation = generation.saturating_add(1);
                    let _ = sender.send(ExplorerEvent::ClipboardChanged {
                        state: ClipboardState::None {
                            generation: *generation,
                        },
                    });
                }
            }
        });
        Ok(())
    }

    fn submit_remote_drag(
        &self,
        context: explorer_model::RequestContext,
        items: Vec<ItemDescriptor>,
        button: explorer_model::DragButton,
    ) -> Result<(), ExplorerServiceError> {
        let providers = Arc::clone(&self.providers);
        let inner = Arc::clone(&self.inner);
        let staging = Arc::clone(&self.drag_staging);
        let sender = self.sender.clone();
        std::thread::spawn(move || {
            let result = (|| {
                let root = tempfile::Builder::new()
                    .prefix("superexplorer-remote-drag-")
                    .tempdir()?;
                let mut local_items = Vec::with_capacity(items.len());
                for item in items {
                    let LocationDescriptor::Virtual(remote) = &item.location else {
                        continue;
                    };
                    let name = remote
                        .components
                        .last()
                        .ok_or_else(|| anyhow::anyhow!("remote drag item has no name"))?;
                    let target = root.path().join(name);
                    providers.resolve(&item.location)?.download(
                        remote,
                        &target,
                        &context.cancellation,
                    )?;
                    local_items.push(ItemDescriptor {
                        id: item.id,
                        location: LocationDescriptor::file_system(target),
                    });
                }
                staging
                    .lock()
                    .map_err(|_| anyhow::anyhow!("remote drag staging lock failed"))?
                    .push(root);
                inner
                    .submit(ExplorerCommand::DataTransfer {
                        context: context.clone(),
                        request: DataTransferRequest::BeginDrag {
                            items: local_items,
                            allowed_effects: TransferEffects::COPY,
                            button,
                        },
                    })
                    .map_err(|_| anyhow::anyhow!("native drag service rejected request"))?;
                Ok::<_, anyhow::Error>(())
            })();
            if let Err(error) = result {
                let _ = sender.send(remote_failed(
                    context,
                    "The remote item could not be prepared for dragging.",
                    error,
                ));
            }
        });
        Ok(())
    }

    fn submit_remote_open(
        &self,
        context: explorer_model::RequestContext,
        item: ItemDescriptor,
    ) -> Result<(), ExplorerServiceError> {
        let providers = Arc::clone(&self.providers);
        let inner = Arc::clone(&self.inner);
        let staging = Arc::clone(&self.drag_staging);
        let sender = self.sender.clone();
        std::thread::spawn(move || {
            let result = (|| {
                let LocationDescriptor::Virtual(remote) = &item.location else {
                    anyhow::bail!("remote item is invalid");
                };
                let name = remote
                    .components
                    .last()
                    .ok_or_else(|| anyhow::anyhow!("remote item has no name"))?;
                let root = tempfile::Builder::new()
                    .prefix("superexplorer-remote-open-")
                    .tempdir()?;
                let target = root.path().join(name);
                providers.resolve(&item.location)?.download(
                    remote,
                    &target,
                    &context.cancellation,
                )?;
                let local_item = ItemDescriptor {
                    id: item.id,
                    location: LocationDescriptor::file_system(target),
                };
                staging
                    .lock()
                    .map_err(|_| anyhow::anyhow!("remote open staging lock failed"))?
                    .push(root);
                inner
                    .submit(ExplorerCommand::OpenItem {
                        context: context.clone(),
                        item: local_item,
                        disposition: explorer_model::OpenDisposition::DefaultApplication,
                    })
                    .map_err(|_| anyhow::anyhow!("native open service rejected request"))?;
                Ok::<_, anyhow::Error>(())
            })();
            if let Err(error) = result {
                let _ = sender.send(remote_failed(
                    context,
                    "The remote file could not be opened.",
                    error,
                ));
            }
        });
        Ok(())
    }
}

impl ExplorerService for RemoteExplorerService {
    fn cache_telemetry_snapshot(&self) -> explorer_model::CacheTelemetrySnapshotV1 {
        self.inner.cache_telemetry_snapshot()
    }

    fn submit(&self, command: ExplorerCommand) -> Result<(), ExplorerServiceError> {
        match command {
            ExplorerCommand::Navigate { context, location }
            | ExplorerCommand::Refresh { context, location }
                if Self::is_remote(&location) =>
            {
                self.submit_navigation(context, location)
            }
            ExplorerCommand::ResolveAncestry { context, location }
                if Self::is_remote(&location) =>
            {
                self.submit_ancestry(context, location)
            }
            ExplorerCommand::EnumerateChildContainers {
                context,
                parent,
                segment_id,
                menu_generation,
            } if Self::is_remote(&parent) => {
                self.submit_child_containers(context, parent, segment_id, menu_generation)
            }
            ExplorerCommand::OpenItem {
                context,
                item,
                disposition,
            } if Self::is_remote(&item.location) => {
                if disposition == explorer_model::OpenDisposition::DefaultApplication {
                    self.submit_remote_open(context, item)
                } else {
                    self.submit_navigation(context, item.location)
                }
            }
            ExplorerCommand::ExecuteFileOperation { context, request }
                if operation_is_remote(&request.kind) =>
            {
                self.submit_operation(context, request.kind)
            }
            ExplorerCommand::DataTransfer {
                context: _,
                request: DataTransferRequest::Copy { items },
            } if items.iter().any(|item| Self::is_remote(&item.location)) => {
                self.submit_copy_or_cut(items, ClipboardMode::Copy)
            }
            ExplorerCommand::DataTransfer {
                context: _,
                request: DataTransferRequest::Cut { items },
            } if items.iter().any(|item| Self::is_remote(&item.location)) => {
                self.submit_copy_or_cut(items, ClipboardMode::Cut)
            }
            ExplorerCommand::DataTransfer {
                context,
                request: DataTransferRequest::Paste { destination, .. },
            } if Self::is_remote(&destination)
                || self
                    .clipboard
                    .lock()
                    .ok()
                    .is_some_and(|value| value.is_some()) =>
            {
                self.submit_paste(context, destination)
            }
            ExplorerCommand::DataTransfer {
                context,
                request:
                    DataTransferRequest::DropExternal {
                        sources,
                        destination,
                        effect,
                        ..
                    },
            } if Self::is_remote(&destination) => {
                let items = sources
                    .into_iter()
                    .filter_map(|location| {
                        let path = location.path()?;
                        let id = explorer_model::ShellItemId::from_provider_bytes(
                            path.as_os_str().to_string_lossy().as_bytes().to_vec(),
                        )?;
                        Some(ItemDescriptor { id, location })
                    })
                    .collect();
                let mode = if effect == explorer_model::DragEffect::Move {
                    ClipboardMode::Cut
                } else {
                    ClipboardMode::Copy
                };
                let providers = Arc::clone(&self.providers);
                let sender = self.sender.clone();
                std::thread::spawn(move || {
                    let outcome =
                        transfer_items(&providers, items, destination, mode, &context.cancellation);
                    let _ = sender.send(ExplorerEvent::OperationFinished { context, outcome });
                });
                Ok(())
            }
            ExplorerCommand::DataTransfer {
                context,
                request: DataTransferRequest::BeginDrag { items, button, .. },
            } if items.iter().any(|item| Self::is_remote(&item.location)) => {
                self.submit_remote_drag(context, items, button)
            }
            ExplorerCommand::LoadShellIcon { context, key } if Self::is_remote(&key.location) => {
                self.sender
                    .try_send(ExplorerEvent::ShellIconFailed {
                        context,
                        key,
                        reason: explorer_model::ShellIconFallbackReason::UnsupportedItem,
                    })
                    .map_err(map_send_error)
            }
            ExplorerCommand::LoadThumbnail {
                context,
                key,
                location,
                ..
            } if Self::is_remote(&location) => self
                .sender
                .try_send(ExplorerEvent::ThumbnailFinished {
                    context,
                    key,
                    outcome: explorer_model::ThumbnailTerminal::Fallback(
                        explorer_model::ThumbnailFallbackReason::Unsupported,
                    ),
                })
                .map_err(map_send_error),
            command => self.inner.submit(command),
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
            Err(TryRecvError::Empty) => self.inner.try_recv(),
            Err(TryRecvError::Disconnected) => Err(ExplorerServiceError::Disconnected),
        }
    }
}

fn operation_is_remote(kind: &FileOperationKind) -> bool {
    match kind {
        FileOperationKind::CreateFolder { parent, .. }
        | FileOperationKind::CreateItem { parent, .. } => RemoteExplorerService::is_remote(parent),
        FileOperationKind::Rename { item, .. } => RemoteExplorerService::is_remote(&item.location),
        FileOperationKind::Copy { items, destination }
        | FileOperationKind::Move { items, destination } => {
            RemoteExplorerService::is_remote(destination)
                || items
                    .iter()
                    .any(|item| RemoteExplorerService::is_remote(&item.location))
        }
        FileOperationKind::RecycleDelete { items }
        | FileOperationKind::PermanentDelete { items, .. } => items
            .iter()
            .any(|item| RemoteExplorerService::is_remote(&item.location)),
        FileOperationKind::CreateShortcut { .. } => false,
    }
}

fn execute_operation(
    providers: &RemoteProviderRegistry,
    kind: &FileOperationKind,
    cancellation: &explorer_model::CancellationToken,
) -> anyhow::Result<OperationTerminal> {
    match kind {
        FileOperationKind::CreateFolder {
            parent: LocationDescriptor::Virtual(parent),
            name,
        } => {
            let mut child = parent.clone();
            child.components.push(name.clone());
            providers
                .resolve(&LocationDescriptor::Virtual(parent.clone()))?
                .create_directory(&child, cancellation)?;
            Ok(OperationTerminal::Finished)
        }
        FileOperationKind::Rename {
            item:
                ItemDescriptor {
                    location: LocationDescriptor::Virtual(source),
                    ..
                },
            new_name,
        } => {
            let mut destination = source.clone();
            *destination
                .components
                .last_mut()
                .ok_or_else(|| anyhow::anyhow!("remote item has no name"))? = new_name.clone();
            providers
                .resolve(&LocationDescriptor::Virtual(source.clone()))?
                .rename(source, &destination, cancellation)?;
            Ok(OperationTerminal::Finished)
        }
        FileOperationKind::PermanentDelete { items, .. }
        | FileOperationKind::RecycleDelete { items } => {
            for item in items {
                let LocationDescriptor::Virtual(location) = &item.location else {
                    return bail_mixed();
                };
                providers
                    .resolve(&item.location)?
                    .delete(location, true, cancellation)?;
            }
            Ok(OperationTerminal::Finished)
        }
        FileOperationKind::Copy { items, destination } => Ok(transfer_items(
            providers,
            items.clone(),
            destination.clone(),
            ClipboardMode::Copy,
            cancellation,
        )),
        FileOperationKind::Move { items, destination } => Ok(transfer_items(
            providers,
            items.clone(),
            destination.clone(),
            ClipboardMode::Cut,
            cancellation,
        )),
        _ => Err(anyhow::anyhow!("remote operation is unsupported")),
    }
}

fn bail_mixed<T>() -> anyhow::Result<T> {
    Err(anyhow::anyhow!("mixed local/remote delete is unsupported"))
}

fn transfer_items(
    providers: &RemoteProviderRegistry,
    items: Vec<ItemDescriptor>,
    destination: LocationDescriptor,
    mode: ClipboardMode,
    cancellation: &explorer_model::CancellationToken,
) -> OperationTerminal {
    let engine = TransferEngine::new(providers);
    let mut outcomes = Vec::new();
    for item in items {
        let result = engine.transfer(
            item.location.clone(),
            destination.clone(),
            if mode == ClipboardMode::Cut {
                TransferMode::Move
            } else {
                TransferMode::Copy
            },
            cancellation,
        );
        match result.result {
            TransferResult::Succeeded => {}
            TransferResult::Cancelled => return OperationTerminal::Cancelled,
            TransferResult::Partial { diagnostic } | TransferResult::Failed { diagnostic } => {
                outcomes.push(OperationItemOutcome {
                    item: Some(item),
                    destination: Some(destination.clone()),
                    result: OperationItemResult::Failed(remote_error(
                        "remote transfer",
                        "A file could not be transferred.",
                        anyhow::anyhow!(diagnostic),
                    )),
                })
            }
        }
    }
    if outcomes.is_empty() {
        OperationTerminal::Finished
    } else {
        OperationTerminal::Partial { outcomes }
    }
}

fn remote_error(
    operation: &'static str,
    user: &'static str,
    error: impl std::fmt::Display,
) -> explorer_common::ExplorerError {
    explorer_common::ExplorerError::new(
        explorer_common::ExplorerErrorKind::Availability,
        operation,
        true,
        user,
        error.to_string(),
    )
}

fn remote_failed(
    context: explorer_model::RequestContext,
    user: &'static str,
    error: impl std::fmt::Display,
) -> ExplorerEvent {
    ExplorerEvent::Failed {
        context,
        error: remote_error("remote provider", user, error),
    }
}

fn map_send_error<T>(error: TrySendError<T>) -> ExplorerServiceError {
    match error {
        TrySendError::Full(_) => ExplorerServiceError::Overloaded,
        TrySendError::Disconnected(_) => ExplorerServiceError::Disconnected,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use explorer_remote::RemoteEntryKind;

    fn remote_location() -> LocationDescriptor {
        LocationDescriptor::Virtual(explorer_model::VirtualLocationDescriptor {
            provider_id: "adb".to_owned(),
            public_authority: Some("HA245TSY".to_owned()),
            container_identity: [7; 16],
            container_generation: 1,
            entry_id: None,
            components: vec!["data".to_owned()],
        })
    }

    struct TelemetryService;

    impl ExplorerService for TelemetryService {
        fn submit(&self, _command: ExplorerCommand) -> Result<(), ExplorerServiceError> {
            Ok(())
        }

        fn try_recv(&self) -> Result<Option<ExplorerEvent>, ExplorerServiceError> {
            Ok(None)
        }

        fn cache_telemetry_snapshot(&self) -> explorer_model::CacheTelemetrySnapshotV1 {
            explorer_model::CacheTelemetrySnapshotV1::new(vec![
                explorer_model::CacheTelemetryEntryV1 {
                    id: explorer_model::CacheTelemetryIdV1::MftServiceLru,
                    category: explorer_model::CacheTelemetryCategoryV1::MftService,
                    availability: explorer_model::CacheTelemetryAvailabilityV1::Available(
                        explorer_model::CacheTelemetryValueV1 {
                            bytes: 41,
                            limit_bytes: Some(512),
                            entry_count: 3,
                            counters: None,
                        },
                    ),
                },
            ])
            .unwrap()
        }
    }

    #[test]
    fn remote_decorator_forwards_local_cache_telemetry() {
        let service = RemoteExplorerService::new(
            Arc::new(TelemetryService),
            Arc::new(RemoteProviderRegistry::default()),
        );
        let snapshot = service.cache_telemetry_snapshot();
        let entry = snapshot
            .entry(explorer_model::CacheTelemetryIdV1::MftServiceLru)
            .unwrap();
        assert!(matches!(
            entry.availability,
            explorer_model::CacheTelemetryAvailabilityV1::Available(value)
                if value.bytes == 41 && value.entry_count == 3
        ));
    }

    #[test]
    fn remote_shell_icon_uses_fallback_without_entering_local_shell_service() {
        let service = RemoteExplorerService::new(
            Arc::new(TelemetryService),
            Arc::new(RemoteProviderRegistry::default()),
        );
        let context = explorer_model::RequestContext::new(
            explorer_model::TabId::new(),
            explorer_model::Generation::new(1),
        );
        let key = explorer_model::ShellIconKey {
            item_id: None,
            location: remote_location(),
            size_bucket: 20,
            dpi: 96,
            theme: explorer_model::ShellIconTheme::Light,
            association_generation: 0,
            overlay_generation: 0,
        };

        service
            .submit(ExplorerCommand::LoadShellIcon {
                context: context.clone(),
                key: key.clone(),
            })
            .unwrap();

        assert!(matches!(
            service.try_recv().unwrap(),
            Some(ExplorerEvent::ShellIconFailed {
                context: event_context,
                key: event_key,
                reason: explorer_model::ShellIconFallbackReason::UnsupportedItem,
            }) if event_context == context && event_key == key
        ));
    }

    #[test]
    fn remote_entry_kinds_drive_rows_and_child_container_filtering() {
        let kinds = [
            (RemoteEntryKind::File, false, "Remote file"),
            (RemoteEntryKind::Directory, true, "Remote folder"),
            (RemoteEntryKind::FileSymlink, false, "Remote file link"),
            (
                RemoteEntryKind::DirectorySymlink,
                true,
                "Remote folder link",
            ),
            (RemoteEntryKind::BrokenSymlink, false, "Broken remote link"),
            (
                RemoteEntryKind::CircularSymlink,
                false,
                "Circular remote link",
            ),
        ];

        for (index, (kind, is_container, type_display)) in kinds.into_iter().enumerate() {
            let mut location = remote_location();
            let LocationDescriptor::Virtual(remote) = &mut location else {
                unreachable!();
            };
            remote.components.push(format!("entry-{index}"));
            let remote_entry = RemoteEntry {
                name: format!("entry-{index}"),
                location: location.clone(),
                kind,
                size: Some(12),
                unix_mode: Some(0o100644),
            };
            let row = remote_file_entry(remote_entry.clone()).expect("row identity");
            assert_eq!(row.location, location);
            assert_eq!(row.is_container, is_container);
            assert_eq!(row.metadata.type_display.as_deref(), Some(type_display));
            assert_eq!(
                remote_child_container(remote_entry).is_some(),
                is_container,
                "child menus must share the row container decision for {kind:?}",
            );
        }
    }
}
