//! System-first ADB resolution, device snapshots, APK installation, and managed Platform-Tools.

use std::{
    ffi::OsString,
    fs,
    io::{Cursor, Read as _, Write as _},
    path::{Component, Path, PathBuf},
    process::Output,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use anyhow::{Context as _, Result, bail};
use explorer_model::CancellationToken;

use crate::adb::{AdbClient, AdbCommandRunner, AdbDevice, SystemAdbCommandRunner};

pub const GOOGLE_PLATFORM_TOOLS_WINDOWS_URL: &str =
    "https://dl.google.com/android/repository/platform-tools-latest-windows.zip";
const MAX_DOWNLOAD_BYTES: u64 = 256 * 1024 * 1024;
const MAX_EXPANDED_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 4096;
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdbToolProvenance {
    Path,
    AndroidSdk,
    Managed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedAdbTool {
    pub executable: PathBuf,
    pub provenance: AdbToolProvenance,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdbCandidateRejection {
    pub path: PathBuf,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdbDeviceSnapshot {
    pub generation: u64,
    pub tool: ResolvedAdbTool,
    pub devices: Vec<AdbDevice>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdbInstallOutcome {
    pub serial: String,
    pub apk: PathBuf,
}

pub struct AdbToolResolver<R = SystemAdbCommandRunner> {
    managed_root: PathBuf,
    runner: R,
    generation: AtomicU64,
}

impl AdbToolResolver {
    pub fn new(managed_root: PathBuf) -> Self {
        Self::with_runner(managed_root, SystemAdbCommandRunner)
    }
}

impl<R: AdbCommandRunner> AdbToolResolver<R> {
    pub const fn with_runner(managed_root: PathBuf, runner: R) -> Self {
        Self {
            managed_root,
            runner,
            generation: AtomicU64::new(1),
        }
    }

    pub fn invalidate(&self) -> u64 {
        self.generation.fetch_add(1, Ordering::AcqRel) + 1
    }

    pub fn resolve(
        &self,
        cancellation: &CancellationToken,
    ) -> Result<(ResolvedAdbTool, Vec<AdbCandidateRejection>)> {
        self.resolve_candidates(system_candidates(&self.managed_root), cancellation)
    }

    pub fn resolve_candidates(
        &self,
        candidates: Vec<(PathBuf, AdbToolProvenance)>,
        cancellation: &CancellationToken,
    ) -> Result<(ResolvedAdbTool, Vec<AdbCandidateRejection>)> {
        let mut rejected = Vec::new();
        for (path, provenance) in candidates {
            if !path.is_file() {
                rejected.push(AdbCandidateRejection {
                    path,
                    reason: "not a regular file".into(),
                });
                continue;
            }
            match self.runner.run(
                &path,
                &[OsString::from("version")],
                cancellation,
                PROBE_TIMEOUT,
            ) {
                Ok(output) if output.status.success() && output_contains_version(&output) => {
                    return Ok((
                        ResolvedAdbTool {
                            executable: path,
                            provenance,
                        },
                        rejected,
                    ));
                }
                Ok(_) => rejected.push(AdbCandidateRejection {
                    path,
                    reason: "version probe failed".into(),
                }),
                Err(error) => rejected.push(AdbCandidateRejection {
                    path,
                    reason: bounded_error(&error),
                }),
            }
        }
        bail!("Android Debug Bridge (adb.exe) was not found or usable")
    }

    pub fn discover_devices(
        &self,
        tool: ResolvedAdbTool,
        cancellation: &CancellationToken,
    ) -> Result<AdbDeviceSnapshot> {
        let generation = self.generation.load(Ordering::Acquire);
        let devices = AdbClient::new(tool.executable.clone(), &self.runner)
            .devices_cancellable(cancellation)?;
        Ok(AdbDeviceSnapshot {
            generation,
            tool,
            devices,
        })
    }
}

impl<T: AdbCommandRunner + ?Sized> AdbCommandRunner for &T {
    fn run(
        &self,
        executable: &Path,
        arguments: &[OsString],
        cancellation: &CancellationToken,
        timeout: Duration,
    ) -> Result<Output> {
        (**self).run(executable, arguments, cancellation, timeout)
    }
}

pub fn install_apk<R: AdbCommandRunner>(
    tool: &ResolvedAdbTool,
    runner: R,
    serial: &str,
    apk: &Path,
    cancellation: &CancellationToken,
) -> Result<AdbInstallOutcome> {
    let canonical = apk.canonicalize().context("canonicalize APK")?;
    AdbClient::new(tool.executable.clone(), runner).install_apk(
        serial,
        &canonical,
        cancellation,
    )?;
    Ok(AdbInstallOutcome {
        serial: serial.to_owned(),
        apk: canonical,
    })
}

pub struct AdbToolInstaller {
    managed_root: PathBuf,
}

impl AdbToolInstaller {
    pub const fn new(managed_root: PathBuf) -> Self {
        Self { managed_root }
    }

    pub fn install_official(
        &self,
        cancellation: &CancellationToken,
        mut progress: impl FnMut(u64, Option<u64>),
    ) -> Result<PathBuf> {
        validate_official_url(GOOGLE_PLATFORM_TOOLS_WINDOWS_URL)?;
        let client = reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(120))
            .build()?;
        let mut response = client.get(GOOGLE_PLATFORM_TOOLS_WINDOWS_URL).send()?;
        if response.status().is_redirection() {
            bail!("Google Platform-Tools redirect was rejected by policy")
        }
        if !response.status().is_success() {
            bail!(
                "Google Platform-Tools download failed with {}",
                response.status()
            )
        }
        let total = response.content_length();
        if total.is_some_and(|size| size > MAX_DOWNLOAD_BYTES) {
            bail!("Google Platform-Tools archive exceeds the download limit")
        }
        let mut bytes = Vec::new();
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            if cancellation.is_cancelled() {
                bail!("ADB download cancelled")
            }
            let read = response.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            if bytes.len().saturating_add(read) as u64 > MAX_DOWNLOAD_BYTES {
                bail!("Google Platform-Tools archive exceeds the download limit")
            }
            bytes.extend_from_slice(&buffer[..read]);
            progress(bytes.len() as u64, total);
        }
        self.install_archive(&bytes, cancellation)
    }

    pub fn install_archive(
        &self,
        bytes: &[u8],
        cancellation: &CancellationToken,
    ) -> Result<PathBuf> {
        if bytes.len() as u64 > MAX_DOWNLOAD_BYTES {
            bail!("Platform-Tools archive exceeds the download limit")
        }
        fs::create_dir_all(&self.managed_root)?;
        let transaction = tempfile::Builder::new()
            .prefix("adb-install-")
            .tempdir_in(&self.managed_root)?;
        extract_safe_zip(bytes, transaction.path(), cancellation)?;
        let adb = transaction.path().join("platform-tools").join("adb.exe");
        if !adb.is_file() {
            bail!("Platform-Tools archive does not contain platform-tools/adb.exe")
        }
        let probe = SystemAdbCommandRunner.run(
            &adb,
            &[OsString::from("version")],
            cancellation,
            PROBE_TIMEOUT,
        )?;
        if !probe.status.success() || !output_contains_version(&probe) {
            bail!("extracted Android Debug Bridge failed its version probe")
        }
        let active = self.managed_root.join("active");
        let incoming = self.managed_root.join("incoming");
        if incoming.exists() {
            fs::remove_dir_all(&incoming)?;
        }
        fs::rename(transaction.path().join("platform-tools"), &incoming)?;
        let backup = self.managed_root.join("previous");
        if backup.exists() {
            fs::remove_dir_all(&backup)?;
        }
        if active.exists() {
            fs::rename(&active, &backup)?;
        }
        if let Err(error) = fs::rename(&incoming, &active) {
            if backup.exists() {
                let _ = fs::rename(&backup, &active);
            }
            return Err(error.into());
        }
        if backup.exists() {
            fs::remove_dir_all(backup)?;
        }
        Ok(active.join("adb.exe"))
    }
}

fn system_candidates(managed_root: &Path) -> Vec<(PathBuf, AdbToolProvenance)> {
    let mut candidates = Vec::new();
    if let Some(path) = super::adb::find_on_path("adb.exe") {
        candidates.push((path, AdbToolProvenance::Path));
    }
    for variable in ["ANDROID_HOME", "ANDROID_SDK_ROOT"] {
        if let Some(root) = std::env::var_os(variable) {
            candidates.push((
                PathBuf::from(root).join("platform-tools").join("adb.exe"),
                AdbToolProvenance::AndroidSdk,
            ));
        }
    }
    candidates.push((
        managed_root.join("active").join("adb.exe"),
        AdbToolProvenance::Managed,
    ));
    candidates
}

fn output_contains_version(output: &Output) -> bool {
    output.stdout.windows(20).any(|w| {
        String::from_utf8_lossy(w)
            .to_ascii_lowercase()
            .contains("android debug bridge")
    }) || String::from_utf8_lossy(&output.stderr)
        .to_ascii_lowercase()
        .contains("android debug bridge")
}

fn bounded_error(error: &anyhow::Error) -> String {
    error.to_string().chars().take(512).collect()
}

fn validate_official_url(url: &str) -> Result<()> {
    let parsed = reqwest::Url::parse(url)?;
    if parsed.scheme() != "https"
        || parsed.host_str() != Some("dl.google.com")
        || parsed.path() != "/android/repository/platform-tools-latest-windows.zip"
    {
        bail!("Platform-Tools URL is not allowlisted")
    }
    Ok(())
}

fn extract_safe_zip(bytes: &[u8], root: &Path, cancellation: &CancellationToken) -> Result<()> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
    if archive.len() > MAX_ARCHIVE_ENTRIES {
        bail!("Platform-Tools archive has too many entries")
    }
    let mut expanded = 0_u64;
    for index in 0..archive.len() {
        if cancellation.is_cancelled() {
            bail!("ADB extraction cancelled")
        }
        let mut entry = archive.by_index(index)?;
        let enclosed = entry
            .enclosed_name()
            .context("Platform-Tools archive contains an unsafe path")?
            .to_owned();
        if enclosed.components().any(|part| {
            matches!(
                part,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        }) {
            bail!("Platform-Tools archive contains an unsafe path")
        }
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            bail!("Platform-Tools archive contains a link")
        }
        expanded = expanded
            .checked_add(entry.size())
            .context("expanded size overflow")?;
        if expanded > MAX_EXPANDED_BYTES {
            bail!("Platform-Tools archive exceeds the expanded-size limit")
        }
        let destination = root.join(&enclosed);
        if entry.is_dir() {
            fs::create_dir_all(&destination)?;
            continue;
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = fs::File::create(destination)?;
        std::io::copy(&mut entry, &mut file)?;
        file.flush()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::VecDeque, process::ExitStatus, sync::Mutex};

    struct FakeRunner(Mutex<VecDeque<Result<Output>>>);
    impl AdbCommandRunner for FakeRunner {
        fn run(
            &self,
            _: &Path,
            _: &[OsString],
            _: &CancellationToken,
            _: Duration,
        ) -> Result<Output> {
            self.0.lock().unwrap().pop_front().unwrap()
        }
    }
    fn output(success: bool, stdout: &str) -> Output {
        #[cfg(not(windows))]
        use std::os::unix::process::ExitStatusExt as _;
        #[cfg(windows)]
        use std::os::windows::process::ExitStatusExt as _;
        Output {
            status: ExitStatus::from_raw(if success { 0 } else { 1 }),
            stdout: stdout.as_bytes().to_vec(),
            stderr: Vec::new(),
        }
    }

    #[test]
    fn resolver_skips_invalid_candidate_and_preserves_precedence() {
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("first.exe");
        let second = dir.path().join("second.exe");
        fs::write(&first, b"x").unwrap();
        fs::write(&second, b"x").unwrap();
        let resolver = AdbToolResolver::with_runner(
            dir.path().into(),
            FakeRunner(Mutex::new(VecDeque::from([
                Ok(output(false, "")),
                Ok(output(true, "Android Debug Bridge version 1")),
            ]))),
        );
        let (tool, rejected) = resolver
            .resolve_candidates(
                vec![
                    (first, AdbToolProvenance::Path),
                    (second.clone(), AdbToolProvenance::AndroidSdk),
                ],
                &CancellationToken::new(),
            )
            .unwrap();
        assert_eq!(tool.executable, second);
        assert_eq!(rejected.len(), 1);
    }

    #[test]
    fn device_snapshot_uses_model_fallback_and_states() {
        let dir = tempfile::tempdir().unwrap();
        let adb = dir.path().join("adb.exe");
        fs::write(&adb, b"x").unwrap();
        let resolver = AdbToolResolver::with_runner(
            dir.path().into(),
            FakeRunner(Mutex::new(VecDeque::from([Ok(output(
                true,
                "List of devices attached\nA\tdevice model:Pixel_9\nB\tunauthorized device:foo\n",
            ))]))),
        );
        let snapshot = resolver
            .discover_devices(
                ResolvedAdbTool {
                    executable: adb,
                    provenance: AdbToolProvenance::Path,
                },
                &CancellationToken::new(),
            )
            .unwrap();
        assert_eq!(snapshot.devices[0].display_name(), "Pixel 9");
        assert!(!snapshot.devices[1].is_installable());
    }

    #[test]
    fn safe_archive_rejects_parent_traversal() {
        let mut bytes = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(Cursor::new(&mut bytes));
            zip.start_file("../adb.exe", zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(b"bad").unwrap();
            zip.finish().unwrap();
        }
        let dir = tempfile::tempdir().unwrap();
        assert!(
            AdbToolInstaller::new(dir.path().into())
                .install_archive(&bytes, &CancellationToken::new())
                .is_err()
        );
        assert!(!dir.path().join("active").exists());
    }
}
