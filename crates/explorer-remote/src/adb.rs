//! Android Debug Bridge process adapter.

use std::{
    collections::HashMap,
    ffi::OsString,
    io::Read as _,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::{Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context as _, Result, bail};
use explorer_common::configure_background_command;
use explorer_model::{CancellationToken, LocationDescriptor, VirtualLocationDescriptor};

use crate::{RemoteEntry, RemoteEntryKind, RemoteMetadata, RemoteProvider};

const DEFAULT_COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
const TRANSFER_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const MAX_CAPTURE_BYTES: u64 = 8 * 1024 * 1024;
const ADB_LIST_SCRIPT: &str = r#"
parent_path=$(printf '%s' "$parent_b64" | base64 -d) || exit 22
emit_entry() {
    entry_kind="$1"
    entry_path="$2"
    entry_name=${entry_path##*/}
    entry_hex=$(printf '%s' "$entry_name" | od -An -tx1 | tr -d '[:space:]') || exit 21
    entry_mode=$(stat -c '%f' "$entry_path" 2>/dev/null || true)
    printf '%s\t%s\t%s\n' "$entry_kind" "$entry_hex" "$entry_mode"
}

[ -d "$parent_path" ] || exit 20
for entry_path in "$parent_path"/* "$parent_path"/.[!.]* "$parent_path"/..?*; do
    [ -e "$entry_path" ] || [ -L "$entry_path" ] || continue
    if [ ! -L "$entry_path" ]; then
        if [ -d "$entry_path" ]; then
            emit_entry d "$entry_path"
        else
            emit_entry f "$entry_path"
        fi
        continue
    fi

    current_path="$entry_path"
    visited_paths="
$current_path
"
    link_hops=0
    while [ -L "$current_path" ]; do
        if [ "$link_hops" -ge 40 ]; then
            emit_entry c "$entry_path"
            continue 2
        fi
        link_target=$(readlink "$current_path") || {
            emit_entry b "$entry_path"
            continue 2
        }
        case "$link_target" in
            /*) next_path="$link_target" ;;
            *) next_path="${current_path%/*}/$link_target" ;;
        esac
        case "$visited_paths" in
            *"
$next_path
"*)
                emit_entry c "$entry_path"
                continue 2
                ;;
        esac
        visited_paths="$visited_paths$next_path
"
        current_path="$next_path"
        link_hops=$((link_hops + 1))
    done

    if [ -d "$current_path" ]; then
        emit_entry ld "$entry_path"
    elif [ -e "$current_path" ]; then
        emit_entry lf "$entry_path"
    else
        emit_entry b "$entry_path"
    fi
done
"#;
const ADB_SYMLINK_SCRIPT: &str = r#"ln -s -- "$1" "$2""#;
const ADB_METADATA_SCRIPT: &str = r#"stat -c '%f|%s|%Y' -- "$1""#;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdbDirectoryEntry {
    pub name: String,
    pub kind: RemoteEntryKind,
    pub unix_mode: Option<u32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct AdbMetadata {
    unix_mode: u32,
    size: u64,
    modified_unix_seconds: u64,
}

/// Non-secret ADB device state reported by `adb devices -l`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdbDevice {
    pub serial: String,
    pub model: Option<String>,
    pub state: AdbDeviceState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdbDeviceState {
    Device,
    Offline,
    Unauthorized,
    Other,
}

/// Typed command runner so production process I/O is replaceable in tests.
pub trait AdbCommandRunner: Send + Sync {
    fn run(
        &self,
        executable: &Path,
        arguments: &[OsString],
        cancellation: &CancellationToken,
        timeout: Duration,
    ) -> Result<Output>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemAdbCommandRunner;

impl AdbCommandRunner for SystemAdbCommandRunner {
    fn run(
        &self,
        executable: &Path,
        arguments: &[OsString],
        cancellation: &CancellationToken,
        timeout: Duration,
    ) -> Result<Output> {
        let mut command = Command::new(executable);
        command
            .args(arguments)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        configure_background_command(&mut command);
        let mut child = command
            .spawn()
            .with_context(|| format!("could not start adb at {}", executable.display()))?;
        let stdout = bounded_reader(child.stdout.take().context("ADB stdout was unavailable")?);
        let stderr = bounded_reader(child.stderr.take().context("ADB stderr was unavailable")?);
        let started = Instant::now();
        let status = loop {
            if cancellation.is_cancelled() {
                let _ = child.kill();
                let _ = child.wait();
                bail!("ADB command cancelled");
            }
            if started.elapsed() >= timeout {
                let _ = child.kill();
                let _ = child.wait();
                bail!("ADB command timed out");
            }
            if let Some(status) = child.try_wait().context("poll ADB process")? {
                break status;
            }
            thread::sleep(Duration::from_millis(20));
        };
        Ok(Output {
            status,
            stdout: stdout.recv().unwrap_or_default(),
            stderr: stderr.recv().unwrap_or_default(),
        })
    }
}

fn bounded_reader(mut reader: impl std::io::Read + Send + 'static) -> mpsc::Receiver<Vec<u8>> {
    let (sender, receiver) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = reader
            .by_ref()
            .take(MAX_CAPTURE_BYTES)
            .read_to_end(&mut bytes);
        let _ = sender.send(bytes);
    });
    receiver
}

/// ADB client that always supplies argument arrays rather than a shell command line.
pub struct AdbClient<R = SystemAdbCommandRunner> {
    executable: PathBuf,
    runner: R,
}

impl AdbClient {
    pub fn discover() -> Result<Self> {
        let executable = std::env::var_os("ANDROID_HOME")
            .map(PathBuf::from)
            .map(|root| root.join("platform-tools").join("adb.exe"))
            .filter(|path| path.is_file())
            .or_else(|| find_on_path("adb.exe"))
            .context("Android Debug Bridge (adb.exe) was not found")?;
        Ok(Self::new(executable, SystemAdbCommandRunner))
    }
}

impl<R: AdbCommandRunner> AdbClient<R> {
    pub const fn new(executable: PathBuf, runner: R) -> Self {
        Self { executable, runner }
    }

    pub fn devices(&self) -> Result<Vec<AdbDevice>> {
        self.devices_cancellable(&CancellationToken::new())
    }

    pub fn devices_cancellable(&self, cancellation: &CancellationToken) -> Result<Vec<AdbDevice>> {
        let output = self.runner.run(
            &self.executable,
            &[OsString::from("devices"), OsString::from("-l")],
            cancellation,
            DEFAULT_COMMAND_TIMEOUT,
        )?;
        ensure_success(&output, "list devices")?;
        parse_devices(&String::from_utf8_lossy(&output.stdout))
    }

    /// Lists direct children of an Android directory. The directory is supplied as one argv
    /// element and never interpolated into a host shell command.
    pub fn list_directory(&self, serial: &str, path: &str) -> Result<Vec<AdbDirectoryEntry>> {
        self.list_directory_cancellable(serial, path, &CancellationToken::new())
    }

    pub fn list_directory_cancellable(
        &self,
        serial: &str,
        path: &str,
        cancellation: &CancellationToken,
    ) -> Result<Vec<AdbDirectoryEntry>> {
        validate_serial(serial)?;
        validate_remote_path(path)?;
        let output = self.runner.run(
            &self.executable,
            &[
                OsString::from("-s"),
                OsString::from(serial),
                OsString::from("shell"),
                OsString::from(format!(
                    "parent_b64={};\n{ADB_LIST_SCRIPT}",
                    encode_base64(path.as_bytes())
                )),
            ],
            cancellation,
            DEFAULT_COMMAND_TIMEOUT,
        )?;
        ensure_success(&output, "list directory")?;
        parse_directory_entries(&output.stdout)
    }

    fn device_command(
        &self,
        serial: &str,
        arguments: impl IntoIterator<Item = OsString>,
        cancellation: &CancellationToken,
        timeout: Duration,
        operation: &str,
    ) -> Result<()> {
        validate_serial(serial)?;
        let mut full = vec![OsString::from("-s"), OsString::from(serial)];
        full.extend(arguments);
        let output = self
            .runner
            .run(&self.executable, &full, cancellation, timeout)?;
        ensure_success(&output, operation)
    }

    pub fn push(
        &self,
        serial: &str,
        local: &Path,
        remote: &str,
        cancellation: &CancellationToken,
    ) -> Result<()> {
        validate_remote_path(remote)?;
        self.device_command(
            serial,
            [
                OsString::from("push"),
                local.as_os_str().to_owned(),
                OsString::from(remote),
            ],
            cancellation,
            TRANSFER_TIMEOUT,
            "push file",
        )
    }

    pub fn pull(
        &self,
        serial: &str,
        remote: &str,
        local: &Path,
        cancellation: &CancellationToken,
    ) -> Result<()> {
        validate_remote_path(remote)?;
        self.device_command(
            serial,
            [
                OsString::from("pull"),
                OsString::from(remote),
                local.as_os_str().to_owned(),
            ],
            cancellation,
            TRANSFER_TIMEOUT,
            "pull file",
        )
    }

    pub fn mkdir(
        &self,
        serial: &str,
        remote: &str,
        cancellation: &CancellationToken,
    ) -> Result<()> {
        validate_remote_path(remote)?;
        self.device_command(
            serial,
            [
                OsString::from("shell"),
                OsString::from("mkdir"),
                OsString::from("-p"),
                OsString::from("--"),
                OsString::from(shell_quote(remote)),
            ],
            cancellation,
            DEFAULT_COMMAND_TIMEOUT,
            "create directory",
        )
    }

    pub fn symlink(
        &self,
        serial: &str,
        target: &str,
        remote: &str,
        cancellation: &CancellationToken,
    ) -> Result<()> {
        validate_remote_path(remote)?;
        validate_symlink_target(target)?;
        self.device_command(
            serial,
            [
                OsString::from("shell"),
                OsString::from("sh"),
                OsString::from("-c"),
                OsString::from(shell_quote(ADB_SYMLINK_SCRIPT)),
                OsString::from("superexplorer-symlink"),
                OsString::from(shell_quote(target)),
                OsString::from(shell_quote(remote)),
            ],
            cancellation,
            DEFAULT_COMMAND_TIMEOUT,
            "create symbolic link",
        )
    }

    fn metadata(
        &self,
        serial: &str,
        remote: &str,
        cancellation: &CancellationToken,
    ) -> Result<AdbMetadata> {
        validate_serial(serial)?;
        validate_remote_path(remote)?;
        let output = self.runner.run(
            &self.executable,
            &[
                OsString::from("-s"),
                OsString::from(serial),
                OsString::from("shell"),
                OsString::from("sh"),
                OsString::from("-c"),
                OsString::from(shell_quote(ADB_METADATA_SCRIPT)),
                OsString::from("superexplorer-metadata"),
                OsString::from(shell_quote(remote)),
            ],
            cancellation,
            DEFAULT_COMMAND_TIMEOUT,
        )?;
        ensure_success(&output, "read item metadata")?;
        parse_metadata(&output.stdout)
    }

    pub fn rename(
        &self,
        serial: &str,
        old: &str,
        new: &str,
        cancellation: &CancellationToken,
    ) -> Result<()> {
        validate_remote_path(old)?;
        validate_remote_path(new)?;
        self.device_command(
            serial,
            [
                OsString::from("shell"),
                OsString::from("mv"),
                OsString::from("--"),
                OsString::from(shell_quote(old)),
                OsString::from(shell_quote(new)),
            ],
            cancellation,
            DEFAULT_COMMAND_TIMEOUT,
            "rename item",
        )
    }

    pub fn delete(
        &self,
        serial: &str,
        remote: &str,
        recursive: bool,
        cancellation: &CancellationToken,
    ) -> Result<()> {
        validate_remote_path(remote)?;
        let mut arguments = vec![OsString::from("shell"), OsString::from("rm")];
        if recursive {
            arguments.push(OsString::from("-rf"));
        } else {
            arguments.push(OsString::from("-f"));
        }
        arguments.extend([OsString::from("--"), OsString::from(shell_quote(remote))]);
        self.device_command(
            serial,
            arguments,
            cancellation,
            DEFAULT_COMMAND_TIMEOUT,
            "delete item",
        )
    }

    pub fn chmod(
        &self,
        serial: &str,
        remote: &str,
        mode: u32,
        cancellation: &CancellationToken,
    ) -> Result<()> {
        validate_remote_path(remote)?;
        if mode & !0o7777 != 0 {
            bail!("ADB chmod mode contains non-permission bits");
        }
        self.device_command(
            serial,
            [
                OsString::from("shell"),
                OsString::from("chmod"),
                OsString::from(format!("{mode:04o}")),
                OsString::from("--"),
                OsString::from(shell_quote(remote)),
            ],
            cancellation,
            DEFAULT_COMMAND_TIMEOUT,
            "change item permissions",
        )
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn encode_base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let bits = u32::from(chunk[0]) << 16
            | u32::from(chunk.get(1).copied().unwrap_or(0)) << 8
            | u32::from(chunk.get(2).copied().unwrap_or(0));
        output.push(TABLE[((bits >> 18) & 0x3f) as usize] as char);
        output.push(TABLE[((bits >> 12) & 0x3f) as usize] as char);
        output.push(if chunk.len() > 1 {
            TABLE[((bits >> 6) & 0x3f) as usize] as char
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            TABLE[(bits & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    output
}

pub struct AdbProvider<R = SystemAdbCommandRunner> {
    client: AdbClient<R>,
    devices: Mutex<HashMap<[u8; 16], String>>,
}

impl<R: AdbCommandRunner> AdbProvider<R> {
    pub fn new(client: AdbClient<R>) -> Self {
        Self {
            client,
            devices: Mutex::new(HashMap::new()),
        }
    }

    pub fn register_device(&self, identity: [u8; 16], serial: String) -> Result<()> {
        if identity == [0; 16] {
            bail!("ADB container identity is invalid");
        }
        validate_serial(&serial)?;
        self.devices
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(identity, serial);
        Ok(())
    }

    pub fn client_devices(&self) -> Result<Vec<AdbDevice>> {
        self.client.devices()
    }

    fn serial(&self, location: &VirtualLocationDescriptor) -> Result<String> {
        if let Some(serial) = self
            .devices
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&location.container_identity)
            .cloned()
        {
            if location.public_authority.as_deref() != Some(serial.as_str()) {
                bail!("ADB location authority does not match the registered device");
            }
            return Ok(serial);
        }
        for device in self.client.devices()? {
            if device.state == AdbDeviceState::Device
                && explorer_model::remote_container_identity(
                    explorer_model::RemoteProviderKind::Adb,
                    &device.serial,
                ) == location.container_identity
            {
                if location.public_authority.as_deref() != Some(device.serial.as_str()) {
                    bail!("ADB location authority does not match the discovered device");
                }
                self.register_device(location.container_identity, device.serial.clone())?;
                return Ok(device.serial);
            }
        }
        bail!("ADB device identity is not registered or the device is unavailable")
    }
}

impl<R: AdbCommandRunner> RemoteProvider for AdbProvider<R> {
    fn provider_id(&self) -> &'static str {
        "adb"
    }

    fn list(
        &self,
        location: &VirtualLocationDescriptor,
        cancellation: &CancellationToken,
    ) -> Result<Vec<RemoteEntry>> {
        crate::provider::validate_remote_location(location, "adb", true)?;
        let serial = self.serial(location)?;
        let parent = remote_path(location);
        self.client
            .list_directory_cancellable(&serial, &parent, cancellation)?
            .into_iter()
            .map(|raw| {
                let name = raw.name;
                let mut child = location.clone();
                child.components.push(name.clone());
                child.entry_id = None;
                Ok(RemoteEntry {
                    name,
                    location: LocationDescriptor::Virtual(child),
                    kind: raw.kind,
                    size: None,
                    unix_mode: raw.unix_mode,
                })
            })
            .collect()
    }

    fn download(
        &self,
        source: &VirtualLocationDescriptor,
        local_destination: &Path,
        cancellation: &CancellationToken,
    ) -> Result<()> {
        crate::provider::validate_remote_location(source, "adb", false)?;
        self.client.pull(
            &self.serial(source)?,
            &remote_path(source),
            local_destination,
            cancellation,
        )
    }

    fn download_with_progress(
        &self,
        source: &VirtualLocationDescriptor,
        local_destination: &Path,
        cancellation: &CancellationToken,
        progress: &(dyn Fn(u64) + Send + Sync),
    ) -> Result<()> {
        crate::provider::validate_remote_location(source, "adb", false)?;
        let serial = self.serial(source)?;
        let remote = remote_path(source);
        let expected = self
            .client
            .metadata(&serial, &remote, cancellation)
            .ok()
            .map(|m| m.size);
        if expected.is_none() {
            return self
                .client
                .pull(&serial, &remote, local_destination, cancellation);
        }
        let destination = local_destination.to_path_buf();
        let result = thread::scope(|scope| {
            let handle = scope.spawn(|| {
                self.client
                    .pull(&serial, &remote, &destination, cancellation)
            });
            let mut reported = 0_u64;
            while !handle.is_finished() {
                if let Ok(size) = fs_file_or_tree_bytes(&destination)
                    && size > reported
                {
                    progress(size - reported);
                    reported = size;
                }
                thread::sleep(Duration::from_millis(75));
            }
            let result = handle
                .join()
                .map_err(|_| anyhow::anyhow!("ADB pull worker panicked"))?;
            result?;
            if let Ok(size) = fs_file_or_tree_bytes(&destination)
                && size > reported
            {
                progress(size - reported);
            }
            Ok(())
        });
        result
    }

    fn upload(
        &self,
        local_source: &Path,
        destination: &VirtualLocationDescriptor,
        cancellation: &CancellationToken,
    ) -> Result<()> {
        crate::provider::validate_remote_location(destination, "adb", true)?;
        self.client.push(
            &self.serial(destination)?,
            local_source,
            &remote_path(destination),
            cancellation,
        )
    }

    fn upload_with_progress(
        &self,
        local_source: &Path,
        destination: &VirtualLocationDescriptor,
        cancellation: &CancellationToken,
        progress: &(dyn Fn(u64) + Send + Sync),
    ) -> Result<()> {
        crate::provider::validate_remote_location(destination, "adb", true)?;
        let serial = self.serial(destination)?;
        let mut target = destination.clone();
        if let Some(name) = local_source.file_name().and_then(|name| name.to_str()) {
            target.components.push(name.to_owned());
        }
        let remote_parent = remote_path(destination);
        let remote_target = remote_path(&target);
        let local = local_source.to_path_buf();
        let expected = fs_file_or_tree_bytes(&local).ok();
        if expected.is_none() {
            return self
                .client
                .push(&serial, &local, &remote_parent, cancellation);
        }
        thread::scope(|scope| {
            let handle = scope.spawn(|| {
                self.client
                    .push(&serial, &local, &remote_parent, cancellation)
            });
            let mut reported = 0_u64;
            while !handle.is_finished() {
                if let Ok(metadata) = self.client.metadata(&serial, &remote_target, cancellation)
                    && metadata.size > reported
                {
                    progress(metadata.size - reported);
                    reported = metadata.size;
                }
                thread::sleep(Duration::from_millis(100));
            }
            let result = handle
                .join()
                .map_err(|_| anyhow::anyhow!("ADB push worker panicked"))?;
            result?;
            if let Some(expected) = expected
                && expected > reported
            {
                progress(expected - reported);
            }
            Ok(())
        })
    }

    fn create_directory(
        &self,
        location: &VirtualLocationDescriptor,
        cancellation: &CancellationToken,
    ) -> Result<()> {
        crate::provider::validate_remote_location(location, "adb", false)?;
        self.client.mkdir(
            &self.serial(location)?,
            &remote_path(location),
            cancellation,
        )
    }

    fn create_symlink(
        &self,
        location: &VirtualLocationDescriptor,
        target: &str,
        cancellation: &CancellationToken,
    ) -> Result<()> {
        crate::provider::validate_remote_location(location, "adb", false)?;
        self.client.symlink(
            &self.serial(location)?,
            target,
            &remote_path(location),
            cancellation,
        )
    }

    fn metadata(
        &self,
        location: &VirtualLocationDescriptor,
        cancellation: &CancellationToken,
    ) -> Result<RemoteMetadata> {
        crate::provider::validate_remote_location(location, "adb", true)?;
        let raw = self.client.metadata(
            &self.serial(location)?,
            &remote_path(location),
            cancellation,
        )?;
        Ok(RemoteMetadata {
            location: LocationDescriptor::Virtual(location.clone()),
            kind: kind_from_unix_mode(raw.unix_mode),
            size: Some(raw.size),
            unix_mode: Some(raw.unix_mode),
            modified_unix_seconds: Some(raw.modified_unix_seconds),
        })
    }

    fn rename(
        &self,
        source: &VirtualLocationDescriptor,
        destination: &VirtualLocationDescriptor,
        cancellation: &CancellationToken,
    ) -> Result<()> {
        crate::provider::validate_remote_location(source, "adb", false)?;
        crate::provider::validate_remote_location(destination, "adb", false)?;
        if source.container_identity != destination.container_identity {
            bail!("ADB rename cannot cross devices");
        }
        self.client.rename(
            &self.serial(source)?,
            &remote_path(source),
            &remote_path(destination),
            cancellation,
        )
    }

    fn delete(
        &self,
        location: &VirtualLocationDescriptor,
        recursive: bool,
        cancellation: &CancellationToken,
    ) -> Result<()> {
        crate::provider::validate_remote_location(location, "adb", false)?;
        self.client.delete(
            &self.serial(location)?,
            &remote_path(location),
            recursive,
            cancellation,
        )
    }

    fn set_unix_mode(
        &self,
        location: &VirtualLocationDescriptor,
        mode: u32,
        cancellation: &CancellationToken,
    ) -> Result<()> {
        crate::provider::validate_remote_location(location, "adb", false)?;
        self.client.chmod(
            &self.serial(location)?,
            &remote_path(location),
            mode,
            cancellation,
        )
    }
}

fn fs_file_or_tree_bytes(path: &Path) -> Result<u64> {
    crate::transfer::local_tree_bytes(path)
}

fn remote_path(location: &VirtualLocationDescriptor) -> String {
    if location.components.is_empty() {
        "/".to_owned()
    } else {
        format!("/{}", location.components.join("/"))
    }
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH")?
        .to_string_lossy()
        .split(';')
        .find_map(|entry| {
            let candidate = PathBuf::from(entry).join(name);
            candidate.is_file().then_some(candidate)
        })
}

fn ensure_success(output: &Output, operation: &str) -> Result<()> {
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    bail!("ADB {operation} failed: {}", stderr.trim())
}

fn validate_serial(value: &str) -> Result<()> {
    if value.is_empty() || value.len() > 255 || value.contains(['\0', '\r', '\n']) {
        bail!("ADB device serial is invalid");
    }
    Ok(())
}

fn validate_remote_path(value: &str) -> Result<()> {
    if !value.starts_with('/') || value.contains('\0') || value.contains(['\r', '\n']) {
        bail!("ADB path is invalid");
    }
    Ok(())
}

fn validate_symlink_target(value: &str) -> Result<()> {
    if value.is_empty() || value.contains(['\0', '\r', '\n']) {
        bail!("ADB symbolic-link target is invalid");
    }
    Ok(())
}

fn kind_from_unix_mode(mode: u32) -> RemoteEntryKind {
    match mode & 0o170_000 {
        0o040_000 => RemoteEntryKind::Directory,
        0o120_000 => RemoteEntryKind::FileSymlink,
        _ => RemoteEntryKind::File,
    }
}

fn parse_metadata(stdout: &[u8]) -> Result<AdbMetadata> {
    let text = std::str::from_utf8(stdout)
        .context("ADB metadata output is not UTF-8")?
        .trim();
    let mut fields = text.split('|');
    let unix_mode = u32::from_str_radix(fields.next().context("ADB metadata has no mode")?, 16)
        .context("ADB metadata mode is invalid")?;
    let size = fields
        .next()
        .context("ADB metadata has no size")?
        .parse()
        .context("ADB metadata size is invalid")?;
    let modified_unix_seconds = fields
        .next()
        .context("ADB metadata has no modification time")?
        .parse()
        .context("ADB metadata modification time is invalid")?;
    if fields.next().is_some() {
        bail!("ADB metadata contains unexpected fields");
    }
    Ok(AdbMetadata {
        unix_mode,
        size,
        modified_unix_seconds,
    })
}

fn parse_directory_entries(stdout: &[u8]) -> Result<Vec<AdbDirectoryEntry>> {
    let stdout = std::str::from_utf8(stdout).context("ADB directory output is not UTF-8")?;
    stdout
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| {
            let mut fields = line.splitn(3, '\t');
            let token = fields.next().unwrap_or_default();
            let encoded_name = fields
                .next()
                .context("ADB directory record has no kind separator")?;
            let unix_mode = fields
                .next()
                .filter(|value| !value.is_empty())
                .and_then(|value| u32::from_str_radix(value, 16).ok());
            if encoded_name.len() % 2 != 0 || encoded_name.is_empty() {
                bail!("ADB directory record has an invalid encoded name");
            }
            let mut bytes = Vec::with_capacity(encoded_name.len() / 2);
            for pair in encoded_name.as_bytes().chunks_exact(2) {
                let digits = std::str::from_utf8(pair)
                    .context("ADB directory record name is not hexadecimal")?;
                bytes.push(
                    u8::from_str_radix(digits, 16)
                        .context("ADB directory record name is not hexadecimal")?,
                );
            }
            let name = String::from_utf8(bytes).context("ADB entry name is not UTF-8")?;
            if name.is_empty()
                || matches!(name.as_str(), "." | "..")
                || name.contains(['/', '\\', '\0', '\r', '\n'])
            {
                bail!("ADB directory record name is invalid");
            }
            let kind = match token {
                "f" => RemoteEntryKind::File,
                "d" => RemoteEntryKind::Directory,
                "lf" => RemoteEntryKind::FileSymlink,
                "ld" => RemoteEntryKind::DirectorySymlink,
                "b" => RemoteEntryKind::BrokenSymlink,
                "c" => RemoteEntryKind::CircularSymlink,
                _ => bail!("ADB directory record kind is invalid"),
            };
            Ok(AdbDirectoryEntry {
                name,
                kind,
                unix_mode,
            })
        })
        .collect()
}

fn parse_devices(stdout: &str) -> Result<Vec<AdbDevice>> {
    let mut output = Vec::new();
    for line in stdout
        .lines()
        .skip_while(|line| !line.starts_with("List of devices"))
    {
        let mut fields = line.split_whitespace();
        let Some(serial) = fields.next() else {
            continue;
        };
        if serial == "List" || serial.starts_with('*') {
            continue;
        }
        let Some(state) = fields.next() else { continue };
        validate_serial(serial)?;
        let model = fields
            .find_map(|field| field.strip_prefix("model:"))
            .filter(|value| !value.is_empty())
            .map(|value| value.replace('_', " "));
        output.push(AdbDevice {
            serial: serial.to_owned(),
            model,
            state: match state {
                "device" => AdbDeviceState::Device,
                "offline" => AdbDeviceState::Offline,
                "unauthorized" => AdbDeviceState::Unauthorized,
                _ => AdbDeviceState::Other,
            },
        });
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use std::{
        ffi::OsString,
        path::Path,
        sync::{Arc, Mutex},
        time::Duration,
    };

    use explorer_model::CancellationToken;

    use super::*;

    #[derive(Clone)]
    struct FakeRunner {
        stdout: Arc<Vec<u8>>,
        arguments: Arc<Mutex<Vec<OsString>>>,
    }

    impl FakeRunner {
        fn with_stdout(stdout: impl Into<Vec<u8>>) -> Self {
            Self {
                stdout: Arc::new(stdout.into()),
                arguments: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    impl AdbCommandRunner for FakeRunner {
        fn run(
            &self,
            _: &Path,
            arguments: &[OsString],
            cancellation: &CancellationToken,
            _: Duration,
        ) -> Result<Output> {
            if cancellation.is_cancelled() {
                bail!("fixture ADB command cancelled");
            }
            *self.arguments.lock().unwrap() = arguments.to_vec();
            Ok(Output {
                status: std::process::ExitStatus::default(),
                stdout: self.stdout.as_ref().clone(),
                stderr: Vec::new(),
            })
        }
    }

    #[test]
    fn parses_authorized_and_unauthorized_devices() {
        let devices =
            parse_devices("List of devices attached\nabc device product:x\ndef unauthorized\n")
                .unwrap();
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].model.as_deref(), None);
        assert_eq!(devices[0].state, AdbDeviceState::Device);
        assert_eq!(devices[1].state, AdbDeviceState::Unauthorized);
    }

    #[test]
    fn rejects_command_injection_in_path_or_serial() {
        assert!(validate_serial("serial\nother").is_err());
        assert!(validate_remote_path("sdcard/Download").is_err());
    }

    #[test]
    fn adb_shell_mutations_quote_paths_with_spaces_and_apostrophes() {
        assert_eq!(shell_quote("/sdcard/New folder"), "'/sdcard/New folder'");
        assert_eq!(
            shell_quote("/sdcard/owner's folder"),
            "'/sdcard/owner'\\''s folder'"
        );

        let runner = FakeRunner::with_stdout(Vec::new());
        let client = AdbClient::new(PathBuf::from("fixture-adb.exe"), runner.clone());
        client
            .mkdir(
                "emulator-5554",
                "/sdcard/New folder",
                &CancellationToken::new(),
            )
            .unwrap();
        let arguments = runner.arguments.lock().unwrap();
        assert_eq!(arguments.last().unwrap(), "'/sdcard/New folder'");
    }

    #[test]
    fn adb_symlink_uses_fixed_script_and_separate_quoted_arguments() {
        let runner = FakeRunner::with_stdout(Vec::new());
        let client = AdbClient::new(PathBuf::from("fixture-adb.exe"), runner.clone());
        let target = "../target with ' quote";
        let link = "/sdcard/Download/link with ' quote";
        client
            .symlink("emulator-5554", target, link, &CancellationToken::new())
            .unwrap();

        let arguments = runner.arguments.lock().unwrap();
        assert_eq!(arguments[2], "shell");
        assert_eq!(arguments[3], "sh");
        assert_eq!(arguments[4], "-c");
        assert_eq!(
            arguments[5],
            OsString::from(shell_quote(ADB_SYMLINK_SCRIPT))
        );
        assert!(!arguments[5].to_string_lossy().contains(target));
        assert!(!arguments[5].to_string_lossy().contains(link));
        assert_eq!(arguments[7], OsString::from(shell_quote(target)));
        assert_eq!(arguments[8], OsString::from(shell_quote(link)));
    }

    #[test]
    fn adb_metadata_parses_mode_size_and_modification_time() {
        let runner = FakeRunner::with_stdout(b"41ed|4096|1700000000\n".to_vec());
        let client = AdbClient::new(PathBuf::from("fixture-adb.exe"), runner.clone());
        let metadata = client
            .metadata(
                "emulator-5554",
                "/sdcard/Download",
                &CancellationToken::new(),
            )
            .unwrap();
        assert_eq!(metadata.unix_mode, 0o040755);
        assert_eq!(metadata.size, 4096);
        assert_eq!(metadata.modified_unix_seconds, 1_700_000_000);
        assert_eq!(
            kind_from_unix_mode(metadata.unix_mode),
            RemoteEntryKind::Directory
        );
        let arguments = runner.arguments.lock().unwrap();
        assert_eq!(
            arguments[5],
            OsString::from(shell_quote(ADB_METADATA_SCRIPT))
        );
        assert_eq!(arguments[7], "'/sdcard/Download'");
    }

    #[test]
    fn symbolic_link_targets_allow_dangling_paths_but_reject_control_bytes() {
        for allowed in ["missing", "../missing target", "/absolute/missing"] {
            assert!(validate_symlink_target(allowed).is_ok());
        }
        for rejected in ["", "bad\0target", "bad\ntarget", "bad\rtarget"] {
            assert!(validate_symlink_target(rejected).is_err());
        }
    }

    #[test]
    fn adb_chmod_uses_octal_permission_argument_and_rejects_type_bits() {
        let runner = FakeRunner::with_stdout(Vec::new());
        let client = AdbClient::new(PathBuf::from("fixture-adb.exe"), runner.clone());
        client
            .chmod(
                "emulator-5554",
                "/sdcard/owner's file",
                0o4750,
                &CancellationToken::new(),
            )
            .unwrap();
        let arguments = runner.arguments.lock().unwrap();
        assert_eq!(arguments[3], "chmod");
        assert_eq!(arguments[4], "4750");
        assert_eq!(arguments[5], "--");
        assert_eq!(arguments[6], "'/sdcard/owner'\\''s file'");
        drop(arguments);
        assert!(
            client
                .chmod(
                    "emulator-5554",
                    "/sdcard/file",
                    0o100644,
                    &CancellationToken::new()
                )
                .is_err()
        );
    }

    #[test]
    fn parses_every_structured_directory_kind_and_hostile_safe_names() {
        let entries = parse_directory_entries(
            b"f\t66696c65\nd\t646972\nlf\t66696c652d6c696e6b\nld\t6469722d6c696e6b\nb\t62726f6b656e\nc\t6379636c65\nf\t737061636520616e642009746162\n",
        )
        .unwrap();
        assert_eq!(
            entries,
            vec![
                AdbDirectoryEntry {
                    name: "file".to_owned(),
                    kind: RemoteEntryKind::File,
                    unix_mode: None,
                },
                AdbDirectoryEntry {
                    name: "dir".to_owned(),
                    kind: RemoteEntryKind::Directory,
                    unix_mode: None,
                },
                AdbDirectoryEntry {
                    name: "file-link".to_owned(),
                    kind: RemoteEntryKind::FileSymlink,
                    unix_mode: None,
                },
                AdbDirectoryEntry {
                    name: "dir-link".to_owned(),
                    kind: RemoteEntryKind::DirectorySymlink,
                    unix_mode: None,
                },
                AdbDirectoryEntry {
                    name: "broken".to_owned(),
                    kind: RemoteEntryKind::BrokenSymlink,
                    unix_mode: None,
                },
                AdbDirectoryEntry {
                    name: "cycle".to_owned(),
                    kind: RemoteEntryKind::CircularSymlink,
                    unix_mode: None,
                },
                AdbDirectoryEntry {
                    name: "space and \ttab".to_owned(),
                    kind: RemoteEntryKind::File,
                    unix_mode: None,
                },
            ]
        );
    }

    #[test]
    fn parses_optional_hex_unix_mode_without_failing_the_entry() {
        let entries = parse_directory_entries(
            b"d\t416e64726f6964\t41ed\nf\t62726f6b656e\tnot-hex\nf\t6d697373696e67\n",
        )
        .unwrap();
        assert_eq!(entries[0].unix_mode, Some(0o040755));
        assert_eq!(entries[1].unix_mode, None);
        assert_eq!(entries[2].unix_mode, None);
    }

    #[test]
    fn rejects_malformed_or_unrepresentable_directory_records() {
        assert!(parse_directory_entries(b"wat\t66696c65\n").is_err());
        assert!(parse_directory_entries(b"f\t123\n").is_err());
        assert!(parse_directory_entries(b"f\t6261640a6e616d65\n").is_err());
        assert!(parse_directory_entries(b"f-no-tab\n").is_err());
    }

    #[test]
    fn adb_listing_uses_fixed_script_and_encoded_parent_path() {
        let runner = FakeRunner::with_stdout(b"ld\t70686f746f73\n".to_vec());
        let client = AdbClient::new(PathBuf::from("fixture-adb.exe"), runner.clone());
        let entries = client
            .list_directory("emulator-5554", "/storage/emulated/0")
            .unwrap();
        assert_eq!(entries[0].kind, RemoteEntryKind::DirectorySymlink);

        let arguments = runner.arguments.lock().unwrap();
        assert_eq!(arguments[2], "shell");
        assert_eq!(arguments.len(), 4);
        let remote_command = arguments[3].to_string_lossy();
        assert!(remote_command.contains(ADB_LIST_SCRIPT));
        assert!(remote_command.starts_with("parent_b64=L3N0b3JhZ2UvZW11bGF0ZWQvMA==;"));
        assert!(!remote_command.contains("/storage/emulated/0"));
        assert_eq!(encode_base64(b"/"), "Lw==");
    }

    #[test]
    fn adb_provider_preserves_link_side_location_and_cancellation() {
        let runner = FakeRunner::with_stdout(b"ld\t6c696e6b\n".to_vec());
        let provider = AdbProvider::new(AdbClient::new(PathBuf::from("fixture-adb.exe"), runner));
        provider
            .register_device([9; 16], "device-9".to_owned())
            .unwrap();
        let location = VirtualLocationDescriptor {
            provider_id: "adb".to_owned(),
            public_authority: Some("device-9".to_owned()),
            container_identity: [9; 16],
            container_generation: 1,
            entry_id: None,
            components: vec!["data".to_owned()],
        };
        let entry = provider
            .list(&location, &CancellationToken::new())
            .unwrap()
            .pop()
            .unwrap();
        assert_eq!(entry.kind, RemoteEntryKind::DirectorySymlink);
        assert!(matches!(
            entry.location,
            LocationDescriptor::Virtual(child)
                if child.components == ["data".to_owned(), "link".to_owned()]
        ));

        let cancellation = CancellationToken::new();
        cancellation.cancel();
        assert!(provider.list(&location, &cancellation).is_err());
    }

    #[test]
    fn system_runner_captures_background_console_output() {
        let output = SystemAdbCommandRunner
            .run(
                Path::new("where.exe"),
                &[OsString::from("cmd.exe")],
                &CancellationToken::new(),
                Duration::from_secs(5),
            )
            .expect("run controlled console command");
        assert!(output.status.success());
        assert!(String::from_utf8_lossy(&output.stdout).contains("cmd.exe"));
    }

    #[test]
    fn system_runner_preserves_timeout() {
        let error = SystemAdbCommandRunner
            .run(
                Path::new("ping.exe"),
                &[
                    OsString::from("-n"),
                    OsString::from("30"),
                    OsString::from("127.0.0.1"),
                ],
                &CancellationToken::new(),
                Duration::from_millis(25),
            )
            .expect_err("controlled command must time out");
        assert!(error.to_string().contains("timed out"));
    }

    #[test]
    fn system_runner_preserves_cancellation() {
        let cancellation = CancellationToken::new();
        cancellation.cancel();
        let error = SystemAdbCommandRunner
            .run(
                Path::new("ping.exe"),
                &[
                    OsString::from("-n"),
                    OsString::from("30"),
                    OsString::from("127.0.0.1"),
                ],
                &cancellation,
                Duration::from_secs(5),
            )
            .expect_err("controlled command must be cancelled");
        assert!(error.to_string().contains("cancelled"));
    }

    #[test]
    fn real_adb_discovery_uses_hidden_system_runner_when_available() {
        let Ok(client) = AdbClient::discover() else {
            return;
        };
        let devices = client.devices().expect("query real ADB device inventory");
        assert!(devices.iter().all(|device| !device.serial.is_empty()));
    }

    #[test]
    fn real_adb_root_listing_uses_structured_probe_when_device_is_available() {
        let Ok(client) = AdbClient::discover() else {
            return;
        };
        let Some(device) = client
            .devices()
            .expect("query real ADB device inventory")
            .into_iter()
            .find(|device| device.state == AdbDeviceState::Device)
        else {
            return;
        };
        let entries = client
            .list_directory(&device.serial, "/")
            .expect("list real ADB root with structured probe");
        assert!(entries.iter().all(|entry| !entry.name.is_empty()));
        if let Some(sdcard) = entries.iter().find(|entry| entry.name == "sdcard") {
            assert_eq!(
                sdcard.kind,
                RemoteEntryKind::DirectorySymlink,
                "Android /sdcard must be navigable through its symbolic link",
            );
        }
    }

    #[test]
    fn real_adb_metadata_uses_a_delimiter_preserved_by_adb_shell() {
        let Ok(client) = AdbClient::discover() else {
            return;
        };
        let Some(device) = client
            .devices()
            .expect("query real ADB device inventory")
            .into_iter()
            .find(|device| device.state == AdbDeviceState::Device)
        else {
            return;
        };
        let metadata = client
            .metadata(&device.serial, "/data/local/tmp", &CancellationToken::new())
            .expect("read real ADB metadata through the fixed script");
        assert_eq!(
            kind_from_unix_mode(metadata.unix_mode),
            RemoteEntryKind::Directory
        );
    }
}
