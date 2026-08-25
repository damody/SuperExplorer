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
use explorer_model::{CancellationToken, LocationDescriptor, VirtualLocationDescriptor};

use crate::{RemoteEntry, RemoteProvider};

const DEFAULT_COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
const TRANSFER_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const MAX_CAPTURE_BYTES: u64 = 8 * 1024 * 1024;

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
        let mut child = Command::new(executable)
            .args(arguments)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
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
    pub fn list_directory(&self, serial: &str, path: &str) -> Result<Vec<String>> {
        self.list_directory_cancellable(serial, path, &CancellationToken::new())
    }

    pub fn list_directory_cancellable(
        &self,
        serial: &str,
        path: &str,
        cancellation: &CancellationToken,
    ) -> Result<Vec<String>> {
        validate_serial(serial)?;
        validate_remote_path(path)?;
        let output = self.runner.run(
            &self.executable,
            &[
                OsString::from("-s"),
                OsString::from(serial),
                OsString::from("shell"),
                OsString::from("ls"),
                OsString::from("-1Ap"),
                OsString::from("--"),
                OsString::from(path),
            ],
            cancellation,
            DEFAULT_COMMAND_TIMEOUT,
        )?;
        ensure_success(&output, "list directory")?;
        Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|entry| !entry.is_empty())
            .map(str::to_owned)
            .collect())
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
                OsString::from(remote),
            ],
            cancellation,
            DEFAULT_COMMAND_TIMEOUT,
            "create directory",
        )
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
                OsString::from(old),
                OsString::from(new),
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
        arguments.extend([OsString::from("--"), OsString::from(remote)]);
        self.device_command(
            serial,
            arguments,
            cancellation,
            DEFAULT_COMMAND_TIMEOUT,
            "delete item",
        )
    }
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
            return Ok(serial);
        }
        for device in self.client.devices()? {
            if device.state == AdbDeviceState::Device
                && explorer_model::remote_container_identity(
                    explorer_model::RemoteProviderKind::Adb,
                    &device.serial,
                ) == location.container_identity
            {
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
        let serial = self.serial(location)?;
        let parent = remote_path(location);
        self.client
            .list_directory_cancellable(&serial, &parent, cancellation)?
            .into_iter()
            .map(|raw| {
                let is_directory = raw.ends_with('/');
                let name = raw.trim_end_matches('/').to_owned();
                let mut child = location.clone();
                child.components.push(name.clone());
                child.entry_id = None;
                Ok(RemoteEntry {
                    name,
                    location: LocationDescriptor::Virtual(child),
                    is_directory,
                    size: None,
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
        self.client.pull(
            &self.serial(source)?,
            &remote_path(source),
            local_destination,
            cancellation,
        )
    }

    fn upload(
        &self,
        local_source: &Path,
        destination: &VirtualLocationDescriptor,
        cancellation: &CancellationToken,
    ) -> Result<()> {
        self.client.push(
            &self.serial(destination)?,
            local_source,
            &remote_path(destination),
            cancellation,
        )
    }

    fn create_directory(
        &self,
        location: &VirtualLocationDescriptor,
        cancellation: &CancellationToken,
    ) -> Result<()> {
        self.client.mkdir(
            &self.serial(location)?,
            &remote_path(location),
            cancellation,
        )
    }

    fn rename(
        &self,
        source: &VirtualLocationDescriptor,
        destination: &VirtualLocationDescriptor,
        cancellation: &CancellationToken,
    ) -> Result<()> {
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
        self.client.delete(
            &self.serial(location)?,
            &remote_path(location),
            recursive,
            cancellation,
        )
    }
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
    use super::*;

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
}
