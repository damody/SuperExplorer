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
    RemoteEntry, RemoteMetadata, RemoteProvider, RemoteProviderRegistry, TransferEngine,
    TransferMode, TransferResult,
};

fn arm_request_deadline(context: &explorer_model::RequestContext) {
    let Some(remaining) = context.deadline.remaining_at(std::time::Instant::now()) else {
        return;
    };
    if remaining.is_zero() {
        context.cancellation.cancel();
        return;
    }
    let cancellation = context.cancellation.clone();
    std::thread::spawn(move || {
        std::thread::park_timeout(remaining);
        cancellation.cancel();
    });
}

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

fn remote_metadata_file_entry(metadata: RemoteMetadata) -> Option<FileEntry> {
    let display_name = match &metadata.location {
        LocationDescriptor::Virtual(remote) => remote
            .components
            .last()
            .cloned()
            .or_else(|| remote.public_authority.clone())
            .unwrap_or_else(|| "/".to_owned()),
        _ => return None,
    };
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    metadata.location.hash(&mut hasher);
    Some(FileEntry {
        id: explorer_model::ShellItemId::from_provider_bytes(hasher.finish().to_le_bytes())?,
        display_name,
        location: metadata.location,
        is_container: metadata.kind.is_container(),
        metadata: FileEntryMetadata {
            modified_display: metadata
                .modified_unix_seconds
                .map(|seconds| format!("Unix {seconds}")),
            modified_sort_key: metadata.modified_unix_seconds,
            size_bytes: metadata.size,
            unix_mode: metadata.unix_mode,
            type_display: Some(metadata.kind.type_display().to_owned()),
            namespace_capabilities: NamespaceCapabilities::from_public_bits(
                NamespaceCapabilities::OPEN | NamespaceCapabilities::PROPERTIES,
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
    pub fn create_symlink(
        &self,
        parent: LocationDescriptor,
        name: String,
        target: String,
        cancellation: explorer_model::CancellationToken,
    ) -> Result<FileEntry, String> {
        let LocationDescriptor::Virtual(parent_remote) = &parent else {
            return Err("Remote folder is invalid.".to_owned());
        };
        let mut destination = parent_remote.clone();
        destination.components.push(name);
        destination.entry_id = None;
        let destination_location = LocationDescriptor::Virtual(destination.clone());
        self.providers
            .resolve(&parent)
            .and_then(|provider| provider.create_symlink(&destination, &target, &cancellation))
            .map_err(|error| error.to_string())?;
        let metadata = self
            .providers
            .resolve(&destination_location)
            .and_then(|provider| provider.metadata(&destination, &cancellation))
            .map_err(|error| error.to_string())?;
        remote_metadata_file_entry(metadata)
            .ok_or_else(|| "The created remote link has no stable identity.".to_owned())
    }

    pub fn metadata(
        &self,
        location: LocationDescriptor,
        cancellation: explorer_model::CancellationToken,
    ) -> Result<FileEntry, String> {
        let LocationDescriptor::Virtual(remote) = &location else {
            return Err("Remote folder is invalid.".to_owned());
        };
        let metadata = self
            .providers
            .resolve(&location)
            .and_then(|provider| provider.metadata(remote, &cancellation))
            .map_err(|error| error.to_string())?;
        remote_metadata_file_entry(metadata)
            .ok_or_else(|| "Remote folder metadata has no stable identity.".to_owned())
    }

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
        cbSize: size_of::<CREDUI_INFOW>() as u32,
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
    token: [u8; 32],
}

fn mint_clipboard_token() -> [u8; 32] {
    let first = uuid::Uuid::new_v4();
    let second = uuid::Uuid::new_v4();
    let mut token = [0_u8; 32];
    token[..16].copy_from_slice(first.as_bytes());
    token[16..].copy_from_slice(second.as_bytes());
    token
}

pub struct RemoteExplorerService {
    inner: Arc<dyn ExplorerService>,
    providers: Arc<RemoteProviderRegistry>,
    sender: SyncSender<ExplorerEvent>,
    receiver: Mutex<Receiver<ExplorerEvent>>,
    clipboard: Arc<Mutex<Option<RemoteClipboard>>>,
    clipboard_generation: Arc<Mutex<u64>>,
    clipboard_staging: Arc<Mutex<Option<tempfile::TempDir>>>,
    open_staging: Arc<Mutex<Vec<tempfile::TempDir>>>,
    active_drag_staging:
        Arc<Mutex<std::collections::HashMap<explorer_common::RequestId, tempfile::TempDir>>>,
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
            clipboard_staging: Arc::new(Mutex::new(None)),
            open_staging: Arc::new(Mutex::new(Vec::new())),
            active_drag_staging: Arc::new(Mutex::new(std::collections::HashMap::new())),
        }
    }

    fn is_remote(location: &LocationDescriptor) -> bool {
        matches!(location, LocationDescriptor::Virtual(location) if matches!(location.provider_id.as_str(), "adb" | "sftp"))
    }

    fn invalidate_remote_clipboard_for_local_replacement(
        &self,
    ) -> Result<(), ExplorerServiceError> {
        let mut clipboard = self
            .clipboard
            .lock()
            .map_err(|_| ExplorerServiceError::Internal)?;
        let mut staging = self
            .clipboard_staging
            .lock()
            .map_err(|_| ExplorerServiceError::Internal)?;
        *clipboard = None;
        *staging = None;
        Ok(())
    }

    fn submit_navigation(
        &self,
        context: explorer_model::RequestContext,
        location: LocationDescriptor,
    ) -> Result<(), ExplorerServiceError> {
        arm_request_deadline(&context);
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
        arm_request_deadline(&context);
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
        arm_request_deadline(&context);
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
        arm_request_deadline(&context);
        let providers = Arc::clone(&self.providers);
        let sender = self.sender.clone();
        std::thread::spawn(move || {
            let result = execute_operation(&providers, &kind, &context.cancellation);
            let outcome = match result {
                Ok(outcome) => outcome,
                Err(_) if context.cancellation.is_cancelled() => OperationTerminal::Cancelled,
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
        let token = mint_clipboard_token();
        *self
            .clipboard
            .lock()
            .map_err(|_| ExplorerServiceError::Internal)? = Some(RemoteClipboard {
            mode,
            items: items.clone(),
            token,
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
        let staging = Arc::clone(&self.clipboard_staging);
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
            if explorer_shell_win::publish_native_file_clipboard_with_token(
                native_items,
                ClipboardMode::Copy,
                Some(token),
            )
            .is_ok()
            {
                if let Ok(mut roots) = staging.lock() {
                    *roots = Some(root);
                }
            }
        });
        Ok(())
    }

    fn submit_paste(
        &self,
        context: explorer_model::RequestContext,
        destination: LocationDescriptor,
        conflict: explorer_model::ConflictDecision,
    ) -> Result<(), ExplorerServiceError> {
        arm_request_deadline(&context);
        let internal = self
            .clipboard
            .lock()
            .map_err(|_| ExplorerServiceError::Internal)?
            .clone();
        let providers = Arc::clone(&self.providers);
        let sender = self.sender.clone();
        let clipboard = Arc::clone(&self.clipboard);
        let generation = Arc::clone(&self.clipboard_generation);
        let clipboard_staging = Arc::clone(&self.clipboard_staging);
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
                Ok((items, mode)) if !items.is_empty() => transfer_items(
                    &providers,
                    items,
                    destination,
                    mode,
                    conflict,
                    &context.cancellation,
                ),
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
            let cut_outcome = internal
                .as_ref()
                .filter(|value| value.mode == ClipboardMode::Cut)
                .map(|_| match &outcome {
                    OperationTerminal::Finished => Some(Vec::new()),
                    OperationTerminal::Partial { outcomes } => Some(
                        outcomes
                            .iter()
                            .filter(|outcome| outcome.result != OperationItemResult::Succeeded)
                            .filter_map(|outcome| outcome.item.clone())
                            .collect::<Vec<_>>(),
                    ),
                    OperationTerminal::Cancelled | OperationTerminal::Failed(_) => None,
                })
                .flatten();
            let _ = sender.send(ExplorerEvent::OperationFinished { context, outcome });
            if let Some(remaining) = cut_outcome {
                if let Ok(mut clipboard) = clipboard.lock() {
                    *clipboard = (!remaining.is_empty()).then_some(RemoteClipboard {
                        mode: ClipboardMode::Cut,
                        items: remaining.clone(),
                        token: internal
                            .as_ref()
                            .map_or_else(mint_clipboard_token, |value| value.token),
                    });
                }
                if remaining.is_empty()
                    && let Ok(mut staging) = clipboard_staging.lock()
                {
                    *staging = None;
                }
                if let Ok(mut generation) = generation.lock() {
                    *generation = generation.saturating_add(1);
                    let _ = sender.send(ExplorerEvent::ClipboardChanged {
                        state: if remaining.is_empty() {
                            ClipboardState::None {
                                generation: *generation,
                            }
                        } else {
                            ClipboardState::Owned {
                                mode: ClipboardMode::Cut,
                                items: remaining,
                                effects: TransferEffects::MOVE,
                                generation: *generation,
                            }
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
        arm_request_deadline(&context);
        let providers = Arc::clone(&self.providers);
        let inner = Arc::clone(&self.inner);
        let staging = Arc::clone(&self.active_drag_staging);
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
                    .insert(context.request_id, root);
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
                if let Ok(mut staging) = staging.lock() {
                    staging.remove(&context.request_id);
                }
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
        arm_request_deadline(&context);
        let providers = Arc::clone(&self.providers);
        let inner = Arc::clone(&self.inner);
        let staging = Arc::clone(&self.open_staging);
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
        let local_clipboard_replacement = matches!(
            &command,
            ExplorerCommand::DataTransfer {
                request: DataTransferRequest::Copy { items }
                    | DataTransferRequest::Cut { items },
                ..
            } if !items.is_empty() && items.iter().all(|item| !Self::is_remote(&item.location))
        );
        if local_clipboard_replacement {
            // The next Shell clipboard object is authoritative. Invalidate the previous remote
            // token/intent before delegating so a fast paste cannot observe stale remote sources.
            self.invalidate_remote_clipboard_for_local_replacement()?;
        }
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
                request:
                    DataTransferRequest::Paste {
                        destination,
                        conflict,
                    },
            } if Self::is_remote(&destination)
                || self
                    .clipboard
                    .lock()
                    .ok()
                    .is_some_and(|value| value.is_some()) =>
            {
                self.submit_paste(context, destination, conflict)
            }
            ExplorerCommand::DataTransfer {
                context,
                request:
                    DataTransferRequest::DropExternal {
                        sources,
                        destination,
                        effect,
                        conflict,
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
                    let outcome = transfer_items(
                        &providers,
                        items,
                        destination,
                        mode,
                        conflict,
                        &context.cancellation,
                    );
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
            Err(TryRecvError::Empty) => {
                let event = self.inner.try_recv()?;
                if let Some(ExplorerEvent::OperationFinished { context, .. }) = event.as_ref()
                    && let Ok(mut staging) = self.active_drag_staging.lock()
                {
                    staging.remove(&context.request_id);
                }
                Ok(event)
            }
            Err(TryRecvError::Disconnected) => Err(ExplorerServiceError::Disconnected),
        }
    }
}

fn operation_is_remote(kind: &FileOperationKind) -> bool {
    match kind {
        FileOperationKind::CreateFolder { parent, .. }
        | FileOperationKind::CreateItem { parent, .. } => RemoteExplorerService::is_remote(parent),
        FileOperationKind::Rename { item, .. } | FileOperationKind::SetUnixMode { item, .. } => {
            RemoteExplorerService::is_remote(&item.location)
        }
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
        FileOperationKind::SetUnixMode {
            item:
                ItemDescriptor {
                    location: LocationDescriptor::Virtual(location),
                    ..
                },
            mode,
        } => {
            providers
                .resolve(&LocationDescriptor::Virtual(location.clone()))?
                .set_unix_mode(location, *mode, cancellation)?;
            Ok(OperationTerminal::Finished)
        }
        FileOperationKind::PermanentDelete { items, .. }
        | FileOperationKind::RecycleDelete { items } => {
            let mut outcomes = Vec::with_capacity(items.len());
            for (index, item) in items.iter().enumerate() {
                if cancellation.is_cancelled() {
                    if outcomes.is_empty() {
                        return Ok(OperationTerminal::Cancelled);
                    }
                    outcomes.extend(items[index..].iter().cloned().map(|item| {
                        OperationItemOutcome {
                            item: Some(item),
                            destination: None,
                            result: OperationItemResult::Cancelled,
                        }
                    }));
                    return Ok(OperationTerminal::Partial { outcomes });
                }
                let result = match &item.location {
                    LocationDescriptor::Virtual(location) => providers
                        .resolve(&item.location)
                        .and_then(|provider| provider.delete(location, true, cancellation)),
                    _ => bail_mixed(),
                };
                outcomes.push(OperationItemOutcome {
                    item: Some(item.clone()),
                    destination: None,
                    result: match result {
                        Ok(()) => OperationItemResult::Succeeded,
                        Err(_) if cancellation.is_cancelled() => OperationItemResult::Cancelled,
                        Err(error) => OperationItemResult::Failed(remote_error(
                            "remote permanent delete",
                            "A remote item could not be deleted.",
                            error,
                        )),
                    },
                });
            }
            if outcomes
                .iter()
                .all(|outcome| outcome.result == OperationItemResult::Succeeded)
            {
                Ok(OperationTerminal::Finished)
            } else {
                Ok(OperationTerminal::Partial { outcomes })
            }
        }
        FileOperationKind::Copy { items, destination } => Ok(transfer_items(
            providers,
            items.clone(),
            destination.clone(),
            ClipboardMode::Copy,
            explorer_model::ConflictDecision::Prompt,
            cancellation,
        )),
        FileOperationKind::Move { items, destination } => Ok(transfer_items(
            providers,
            items.clone(),
            destination.clone(),
            ClipboardMode::Cut,
            explorer_model::ConflictDecision::Prompt,
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
    conflict: explorer_model::ConflictDecision,
    cancellation: &explorer_model::CancellationToken,
) -> OperationTerminal {
    let engine = TransferEngine::new(providers);
    let mut outcomes = Vec::with_capacity(items.len());
    for (index, item) in items.iter().cloned().enumerate() {
        if cancellation.is_cancelled() {
            if outcomes.is_empty() {
                return OperationTerminal::Cancelled;
            }
            outcomes.extend(
                items[index..]
                    .iter()
                    .cloned()
                    .map(|item| OperationItemOutcome {
                        item: Some(item),
                        destination: Some(destination.clone()),
                        result: OperationItemResult::Cancelled,
                    }),
            );
            return OperationTerminal::Partial { outcomes };
        }
        let result = engine.transfer_with_conflict(
            item.location.clone(),
            destination.clone(),
            if mode == ClipboardMode::Cut {
                TransferMode::Move
            } else {
                TransferMode::Copy
            },
            conflict,
            cancellation,
        );
        let item_result = match result.result {
            TransferResult::Succeeded => OperationItemResult::Succeeded,
            TransferResult::Skipped => OperationItemResult::Skipped,
            TransferResult::Cancelled => OperationItemResult::Cancelled,
            TransferResult::Partial { stage, diagnostic } => {
                OperationItemResult::Partial(remote_transfer_error(
                    stage.user_label(),
                    diagnostic,
                    "remote transfer partially completed",
                ))
            }
            TransferResult::Failed { stage, diagnostic } => OperationItemResult::Failed(
                remote_transfer_error(stage.user_label(), diagnostic, "remote transfer failed"),
            ),
        };
        outcomes.push(OperationItemOutcome {
            item: Some(item),
            destination: Some(destination.clone()),
            result: item_result,
        });
    }
    if outcomes
        .iter()
        .all(|outcome| outcome.result == OperationItemResult::Succeeded)
    {
        OperationTerminal::Finished
    } else {
        OperationTerminal::Partial { outcomes }
    }
}

fn remote_error(
    operation: &'static str,
    user: &'static str,
    _error: impl std::fmt::Display,
) -> explorer_common::ExplorerError {
    explorer_common::ExplorerError::new(
        explorer_common::ExplorerErrorKind::Availability,
        operation,
        true,
        user,
        "remote provider operation failed",
    )
}

fn remote_transfer_error(
    stage: &'static str,
    diagnostic: String,
    technical_detail: &'static str,
) -> explorer_common::ExplorerError {
    explorer_common::ExplorerError::new(
        explorer_common::ExplorerErrorKind::Availability,
        stage,
        true,
        diagnostic,
        technical_detail,
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

    struct DownloadProvider;

    #[derive(Clone, Debug)]
    struct UploadObservation {
        destination: explorer_model::VirtualLocationDescriptor,
        staged_path: std::path::PathBuf,
        bytes: Vec<u8>,
    }

    struct UploadProvider {
        provider_id: &'static str,
        fail_upload: bool,
        observations: Arc<Mutex<Vec<UploadObservation>>>,
    }

    impl RemoteProvider for DownloadProvider {
        fn provider_id(&self) -> &'static str {
            "adb"
        }

        fn list(
            &self,
            _: &explorer_model::VirtualLocationDescriptor,
            _: &explorer_model::CancellationToken,
        ) -> anyhow::Result<Vec<RemoteEntry>> {
            Ok(Vec::new())
        }

        fn download(
            &self,
            _: &explorer_model::VirtualLocationDescriptor,
            local_destination: &std::path::Path,
            _: &explorer_model::CancellationToken,
        ) -> anyhow::Result<()> {
            std::fs::write(local_destination, b"remote clipboard fixture")?;
            Ok(())
        }

        fn upload(
            &self,
            _: &std::path::Path,
            _: &explorer_model::VirtualLocationDescriptor,
            _: &explorer_model::CancellationToken,
        ) -> anyhow::Result<()> {
            anyhow::bail!("unused upload")
        }

        fn create_directory(
            &self,
            _: &explorer_model::VirtualLocationDescriptor,
            _: &explorer_model::CancellationToken,
        ) -> anyhow::Result<()> {
            anyhow::bail!("unused create directory")
        }

        fn rename(
            &self,
            _: &explorer_model::VirtualLocationDescriptor,
            _: &explorer_model::VirtualLocationDescriptor,
            _: &explorer_model::CancellationToken,
        ) -> anyhow::Result<()> {
            anyhow::bail!("unused rename")
        }

        fn delete(
            &self,
            _: &explorer_model::VirtualLocationDescriptor,
            _: bool,
            _: &explorer_model::CancellationToken,
        ) -> anyhow::Result<()> {
            anyhow::bail!("copy must not delete its source")
        }
    }

    impl RemoteProvider for UploadProvider {
        fn provider_id(&self) -> &'static str {
            self.provider_id
        }

        fn list(
            &self,
            _: &explorer_model::VirtualLocationDescriptor,
            _: &explorer_model::CancellationToken,
        ) -> anyhow::Result<Vec<RemoteEntry>> {
            Ok(Vec::new())
        }

        fn download(
            &self,
            _: &explorer_model::VirtualLocationDescriptor,
            _: &std::path::Path,
            _: &explorer_model::CancellationToken,
        ) -> anyhow::Result<()> {
            anyhow::bail!("destination provider must not download")
        }

        fn upload(
            &self,
            local_source: &std::path::Path,
            destination: &explorer_model::VirtualLocationDescriptor,
            _: &explorer_model::CancellationToken,
        ) -> anyhow::Result<()> {
            self.observations.lock().unwrap().push(UploadObservation {
                destination: destination.clone(),
                staged_path: local_source.to_path_buf(),
                bytes: std::fs::read(local_source)?,
            });
            if self.fail_upload {
                anyhow::bail!("fixture upload failure")
            }
            Ok(())
        }

        fn create_directory(
            &self,
            _: &explorer_model::VirtualLocationDescriptor,
            _: &explorer_model::CancellationToken,
        ) -> anyhow::Result<()> {
            anyhow::bail!("unused create directory")
        }

        fn rename(
            &self,
            _: &explorer_model::VirtualLocationDescriptor,
            _: &explorer_model::VirtualLocationDescriptor,
            _: &explorer_model::CancellationToken,
        ) -> anyhow::Result<()> {
            anyhow::bail!("unused rename")
        }

        fn delete(
            &self,
            _: &explorer_model::VirtualLocationDescriptor,
            _: bool,
            _: &explorer_model::CancellationToken,
        ) -> anyhow::Result<()> {
            anyhow::bail!("copy must not delete its destination")
        }
    }

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

    fn local_item() -> ItemDescriptor {
        ItemDescriptor {
            id: explorer_model::ShellItemId::from_provider_bytes([3; 8]).unwrap(),
            location: LocationDescriptor::file_system(r"C:\Users\fixture\Downloads\local.txt"),
        }
    }

    fn virtual_destination(provider_id: &str) -> LocationDescriptor {
        LocationDescriptor::Virtual(explorer_model::VirtualLocationDescriptor {
            provider_id: provider_id.to_owned(),
            public_authority: Some("destination".to_owned()),
            container_identity: [8; 16],
            container_generation: 1,
            entry_id: None,
            components: vec!["incoming".to_owned()],
        })
    }

    fn wait_for_operation(
        service: &RemoteExplorerService,
        expected_context: &explorer_model::RequestContext,
    ) -> OperationTerminal {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            if let Some(ExplorerEvent::OperationFinished { context, outcome }) =
                service.try_recv().unwrap()
                && context == *expected_context
            {
                return outcome;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "paste terminal timeout"
            );
            std::thread::yield_now();
        }
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
    fn local_copy_or_cut_invalidates_stale_remote_clipboard_and_staging() {
        for request in [
            DataTransferRequest::Copy {
                items: vec![local_item()],
            },
            DataTransferRequest::Cut {
                items: vec![local_item()],
            },
        ] {
            let service = RemoteExplorerService::new(
                Arc::new(TelemetryService),
                Arc::new(RemoteProviderRegistry::default()),
            );
            *service.clipboard.lock().unwrap() = Some(RemoteClipboard {
                mode: ClipboardMode::Cut,
                items: vec![ItemDescriptor {
                    id: explorer_model::ShellItemId::from_provider_bytes([9; 8]).unwrap(),
                    location: remote_location(),
                }],
                token: [5; 32],
            });
            *service.clipboard_staging.lock().unwrap() = Some(tempfile::tempdir().unwrap());
            let context = explorer_model::RequestContext::new(
                explorer_model::TabId::new(),
                explorer_model::Generation::new(1),
            );

            service
                .submit(ExplorerCommand::DataTransfer { context, request })
                .unwrap();

            assert!(service.clipboard.lock().unwrap().is_none());
            assert!(service.clipboard_staging.lock().unwrap().is_none());
        }
    }

    #[test]
    fn remote_copy_keeps_internal_token_authority() {
        let service = RemoteExplorerService::new(
            Arc::new(TelemetryService),
            Arc::new(RemoteProviderRegistry::default()),
        );
        let context = explorer_model::RequestContext::new(
            explorer_model::TabId::new(),
            explorer_model::Generation::new(1),
        );
        service
            .submit(ExplorerCommand::DataTransfer {
                context,
                request: DataTransferRequest::Copy {
                    items: vec![ItemDescriptor {
                        id: explorer_model::ShellItemId::from_provider_bytes([9; 8]).unwrap(),
                        location: remote_location(),
                    }],
                },
            })
            .unwrap();
        assert!(service.clipboard.lock().unwrap().is_some());
    }

    #[test]
    fn remote_copy_immediately_pastes_through_internal_clipboard_to_local_folder() {
        let mut providers = RemoteProviderRegistry::default();
        providers.register(Arc::new(DownloadProvider)).unwrap();
        let service = RemoteExplorerService::new(Arc::new(TelemetryService), Arc::new(providers));
        let item = ItemDescriptor {
            id: explorer_model::ShellItemId::from_provider_bytes([9; 8]).unwrap(),
            location: remote_location(),
        };
        service
            .submit(ExplorerCommand::DataTransfer {
                context: explorer_model::RequestContext::new(
                    explorer_model::TabId::new(),
                    explorer_model::Generation::new(1),
                ),
                request: DataTransferRequest::Copy { items: vec![item] },
            })
            .unwrap();

        let destination = tempfile::tempdir().unwrap();
        let paste_context = explorer_model::RequestContext::new(
            explorer_model::TabId::new(),
            explorer_model::Generation::new(1),
        );
        service
            .submit(ExplorerCommand::DataTransfer {
                context: paste_context.clone(),
                request: DataTransferRequest::Paste {
                    destination: LocationDescriptor::file_system(destination.path()),
                    conflict: explorer_model::ConflictDecision::Prompt,
                },
            })
            .unwrap();

        let terminal = wait_for_operation(&service, &paste_context);
        assert_eq!(terminal, OperationTerminal::Finished);
        assert_eq!(
            std::fs::read(destination.path().join("data")).unwrap(),
            b"remote clipboard fixture"
        );
        assert!(service.clipboard.lock().unwrap().is_some());
    }

    #[test]
    fn adb_clipboard_pastes_to_sftp_and_other_registered_virtual_providers() {
        for provider_id in ["sftp", "archive"] {
            let observations = Arc::new(Mutex::new(Vec::new()));
            let mut providers = RemoteProviderRegistry::default();
            providers.register(Arc::new(DownloadProvider)).unwrap();
            providers
                .register(Arc::new(UploadProvider {
                    provider_id,
                    fail_upload: false,
                    observations: Arc::clone(&observations),
                }))
                .unwrap();
            let service =
                RemoteExplorerService::new(Arc::new(TelemetryService), Arc::new(providers));
            service
                .submit(ExplorerCommand::DataTransfer {
                    context: explorer_model::RequestContext::new(
                        explorer_model::TabId::new(),
                        explorer_model::Generation::new(1),
                    ),
                    request: DataTransferRequest::Copy {
                        items: vec![ItemDescriptor {
                            id: explorer_model::ShellItemId::from_provider_bytes([9; 8]).unwrap(),
                            location: remote_location(),
                        }],
                    },
                })
                .unwrap();
            let paste_context = explorer_model::RequestContext::new(
                explorer_model::TabId::new(),
                explorer_model::Generation::new(1),
            );
            let destination = virtual_destination(provider_id);
            service
                .submit(ExplorerCommand::DataTransfer {
                    context: paste_context.clone(),
                    request: DataTransferRequest::Paste {
                        destination: destination.clone(),
                        conflict: explorer_model::ConflictDecision::Prompt,
                    },
                })
                .unwrap();

            assert_eq!(
                wait_for_operation(&service, &paste_context),
                OperationTerminal::Finished
            );
            let observations = observations.lock().unwrap();
            assert_eq!(observations.len(), 1);
            assert_eq!(
                LocationDescriptor::Virtual(observations[0].destination.clone()),
                destination
            );
            assert_eq!(observations[0].bytes, b"remote clipboard fixture");
            assert!(!observations[0].staged_path.exists());
        }
    }

    #[test]
    fn failed_virtual_upload_keeps_copy_clipboard_and_cleans_staging() {
        let observations = Arc::new(Mutex::new(Vec::new()));
        let mut providers = RemoteProviderRegistry::default();
        providers.register(Arc::new(DownloadProvider)).unwrap();
        providers
            .register(Arc::new(UploadProvider {
                provider_id: "archive",
                fail_upload: true,
                observations: Arc::clone(&observations),
            }))
            .unwrap();
        let service = RemoteExplorerService::new(Arc::new(TelemetryService), Arc::new(providers));
        service
            .submit(ExplorerCommand::DataTransfer {
                context: explorer_model::RequestContext::new(
                    explorer_model::TabId::new(),
                    explorer_model::Generation::new(1),
                ),
                request: DataTransferRequest::Copy {
                    items: vec![ItemDescriptor {
                        id: explorer_model::ShellItemId::from_provider_bytes([9; 8]).unwrap(),
                        location: remote_location(),
                    }],
                },
            })
            .unwrap();
        let paste_context = explorer_model::RequestContext::new(
            explorer_model::TabId::new(),
            explorer_model::Generation::new(1),
        );
        service
            .submit(ExplorerCommand::DataTransfer {
                context: paste_context.clone(),
                request: DataTransferRequest::Paste {
                    destination: virtual_destination("archive"),
                    conflict: explorer_model::ConflictDecision::Prompt,
                },
            })
            .unwrap();

        let terminal = wait_for_operation(&service, &paste_context);
        let OperationTerminal::Partial { outcomes } = terminal else {
            panic!("failed upload must produce item outcomes")
        };
        let [
            OperationItemOutcome {
                item: Some(item),
                destination: Some(destination),
                result: OperationItemResult::Failed(error),
            },
        ] = outcomes.as_slice()
        else {
            panic!("failed upload must retain its item and destination")
        };
        assert_eq!(item.location, remote_location());
        assert_eq!(*destination, virtual_destination("archive"));
        assert_eq!(error.operation, "目的地上傳");
        assert!(error.user_message.contains("fixture upload failure"));
        assert!(
            !error
                .user_message
                .contains("A file could not be transferred")
        );
        assert!(service.clipboard.lock().unwrap().is_some());
        let observations = observations.lock().unwrap();
        assert_eq!(observations.len(), 1);
        assert!(!observations[0].staged_path.exists());
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
