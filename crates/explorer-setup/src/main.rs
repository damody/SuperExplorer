#![cfg(windows)]
#![expect(
    unsafe_code,
    reason = "setup uses COM shortcuts, elevation, and a native installer wizard"
)]

mod ui;

use anyhow::{Context, Result, bail};
use std::{
    env, fs,
    mem::size_of,
    io::{Cursor, Read, Write},
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::Duration,
};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

const MAGIC: &[u8; 8] = b"SESETUP1";
pub(crate) const PRODUCT_NAME: &str = "SuperExplorer";
pub(crate) const PRODUCT_PUBLISHER: &str = "Damody";
const PRODUCT_URL: &str = "https://github.com/damody/SuperExplorer";
const PRODUCT_REG_KEY: &str = r"Software\SuperExplorer";
const PRODUCT_UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\SuperExplorer";
const SERVICE_NAME: &str = "SuperExplorerMft";

fn main() {
    match run() {
        Ok(()) => {}
        Err(error) => {
            let text = format!("{error:#}");
            eprintln!("{text}");
            if !is_silent() {
                message_box("SuperExplorer Setup", &text);
            }
            std::process::exit(1);
        }
    }
}

fn run() -> Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--create-installer") {
        return create_installer(&args);
    }
    if is_uninstall_request(&args) {
        if is_silent() || has_flag(&args, "--skip-service") {
            return uninstall(&args);
        }
        return ui::run_uninstall();
    }
    if is_silent() || has_flag(&args, "--skip-service") || flag_value(&args, "--install-directory").is_some()
    {
        return install(&args);
    }
    ui::run_install()
}

fn is_silent() -> bool {
    env::args().any(|arg| arg.eq_ignore_ascii_case("/s") || arg == "--silent")
}

fn is_uninstall_request(args: &[String]) -> bool {
    args.iter().any(|arg| arg == "--uninstall")
        || env::current_exe()
            .ok()
            .and_then(|path| path.file_name().map(|name| name.to_string_lossy().eq_ignore_ascii_case("Uninstall.exe")))
            .unwrap_or(false)
}

fn flag_value<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter()
        .position(|arg| arg == name)
        .and_then(|index| args.get(index + 1))
        .map(String::as_str)
}

fn has_flag(args: &[String], name: &str) -> bool {
    args.iter().any(|arg| arg == name)
}

fn create_installer(args: &[String]) -> Result<()> {
    let payload = PathBuf::from(
        flag_value(args, "--payload-dir").context("missing --payload-dir")?,
    );
    let output = PathBuf::from(flag_value(args, "--output").context("missing --output")?);
    let stub = env::current_exe().context("current setup stub")?;
    let zip_bytes = zip_directory(&payload)?;
    let mut stub_bytes = fs::read(&stub).with_context(|| format!("read stub {}", stub.display()))?;
    stub_bytes.extend_from_slice(&zip_bytes);
    stub_bytes.extend_from_slice(&(zip_bytes.len() as u64).to_le_bytes());
    stub_bytes.extend_from_slice(MAGIC);
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&output, stub_bytes).with_context(|| format!("write {}", output.display()))?;
    println!("created installer {}", output.display());
    Ok(())
}

fn zip_directory(root: &Path) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    {
        let mut zip = ZipWriter::new(Cursor::new(&mut bytes));
        let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        add_zip_tree(&mut zip, root, Path::new(""), options)?;
        zip.finish()?;
    }
    Ok(bytes)
}

fn add_zip_tree(
    zip: &mut ZipWriter<Cursor<&mut Vec<u8>>>,
    abs: &Path,
    rel: &Path,
    options: SimpleFileOptions,
) -> Result<()> {
    for entry in fs::read_dir(abs).with_context(|| format!("read {}", abs.display()))? {
        let entry = entry?;
        let name = entry.file_name();
        let child_rel = rel.join(&name);
        let child_rel_text = child_rel.to_string_lossy().replace('\\', "/");
        if child_rel_text.contains("..") {
            bail!("payload path is not safe: {child_rel_text}");
        }
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            add_zip_tree(zip, &entry.path(), &child_rel, options)?;
        } else if file_type.is_file() {
            zip.start_file(&child_rel_text, options)?;
            let data = fs::read(entry.path())?;
            zip.write_all(&data)?;
        }
    }
    Ok(())
}

fn read_overlay() -> Result<Vec<u8>> {
    let exe = env::current_exe().context("current installer")?;
    let bytes = fs::read(&exe).with_context(|| format!("read {}", exe.display()))?;
    if bytes.len() < 16 || &bytes[bytes.len() - 8..] != MAGIC {
        bail!("this SuperExplorer setup binary has no payload; rebuild with --create-installer");
    }
    let size_offset = bytes.len() - 16;
    let zip_size = u64::from_le_bytes(bytes[size_offset..size_offset + 8].try_into().unwrap()) as usize;
    let zip_start = size_offset
        .checked_sub(zip_size)
        .context("installer overlay is truncated")?;
    Ok(bytes[zip_start..size_offset].to_vec())
}

fn extract_overlay(dest: &Path) -> Result<PathBuf> {
    let zip_bytes = read_overlay()?;
    let mut archive = ZipArchive::new(Cursor::new(zip_bytes))?;
    for index in 0..archive.len() {
        let mut file = archive.by_index(index)?;
        let name = file
            .enclosed_name()
            .context("zip entry is not safe")?
            .to_path_buf();
        let out = dest.join(&name);
        if file.is_dir() {
            fs::create_dir_all(&out)?;
            continue;
        }
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        fs::write(&out, bytes)?;
    }
    Ok(dest.to_path_buf())
}

pub(crate) fn payload_version() -> String {
    let Ok(bytes) = read_overlay() else {
        return String::new();
    };
    let Ok(mut archive) = ZipArchive::new(Cursor::new(bytes)) else {
        return String::new();
    };
    let Ok(mut file) = archive.by_name("setup-version.txt") else {
        return String::new();
    };
    let mut text = String::new();
    let _ = file.read_to_string(&mut text);
    text.trim().to_owned()
}

pub(crate) fn default_install_dir() -> Result<PathBuf> {
    let program_files = env::var("ProgramW6432")
        .or_else(|_| env::var("ProgramFiles"))
        .context("Program Files is unavailable")?;
    Ok(PathBuf::from(program_files).join(PRODUCT_NAME))
}

fn install(args: &[String]) -> Result<()> {
    let skip_service = has_flag(args, "--skip-service");
    let install_dir = flag_value(args, "--install-directory")
        .map(PathBuf::from)
        .unwrap_or(default_install_dir()?);
    install_to(install_dir, skip_service, |_, _| {})
}

pub(crate) fn install_to(
    install_dir: PathBuf,
    skip_service: bool,
    mut progress: impl FnMut(&str, u32),
) -> Result<()> {
    if !skip_service {
        ensure_administrator(&install_dir)?;
    }
    progress("Preparing files", 8);
    let staging = tempfile::tempdir().context("payload staging")?;
    extract_overlay(staging.path())?;
    progress("Closing SuperExplorer", 18);
    let quiesce = staging.path().join("superexplorer-quiesce.exe");
    if quiesce.is_file() {
        run_checked(
            Command::new(&quiesce).arg("--install-directory").arg(&install_dir),
            "close running SuperExplorer",
        )?;
    }
    if !skip_service {
        progress("Stopping SuperExplorer MFT Service", 32);
        stop_service()?;
    }
    progress("Copying program files", 55);
    fs::create_dir_all(install_dir.join("plugins"))?;
    copy_tree(staging.path(), &install_dir)?;
    let uninstaller = install_dir.join("Uninstall.exe");
    fs::copy(env::current_exe()?, &uninstaller)
        .with_context(|| format!("write {}", uninstaller.display()))?;
    if !skip_service {
        progress("Configuring Windows service", 78);
        configure_service(&install_dir)?;
        write_uninstall_registry(&install_dir)?;
        progress("Creating shortcuts", 90);
        create_shortcut(
            &install_dir.join("SuperExplorer.exe"),
            &desktop_dir()?.join(format!("{PRODUCT_NAME}.lnk")),
        )?;
        let start_menu = start_menu_dir()?.join(PRODUCT_NAME);
        fs::create_dir_all(&start_menu)?;
        create_shortcut(
            &install_dir.join("SuperExplorer.exe"),
            &start_menu.join(format!("{PRODUCT_NAME}.lnk")),
        )?;
    }
    progress("Finishing", 100);
    let version = fs::read_to_string(install_dir.join("setup-version.txt"))
        .unwrap_or_else(|_| "unknown".to_owned());
    println!("installed SuperExplorer {version} to {}", install_dir.display());
    Ok(())
}

pub(crate) fn uninstall(args: &[String]) -> Result<()> {
    let skip_service = has_flag(args, "--skip-service");
    let install_dir = flag_value(args, "--install-directory")
        .map(PathBuf::from)
        .or_else(|| env::current_exe().ok().and_then(|path| path.parent().map(Path::to_path_buf)))
        .context("unable to resolve uninstall directory")?;
    if !skip_service {
        ensure_administrator(&install_dir)?;
    }
    let quiesce = install_dir.join("superexplorer-quiesce.exe");
    if quiesce.is_file() {
        run_checked(
            Command::new(&quiesce).arg("--install-directory").arg(&install_dir),
            "close running SuperExplorer",
        )?;
    }
    if !skip_service {
        stop_service()?;
        delete_service()?;
        let _ = fs::remove_file(desktop_dir()?.join(format!("{PRODUCT_NAME}.lnk")));
        let start_menu = start_menu_dir()?.join(PRODUCT_NAME);
        let _ = fs::remove_file(start_menu.join(format!("{PRODUCT_NAME}.lnk")));
        let _ = fs::remove_dir(&start_menu);
        delete_reg_key(PRODUCT_UNINSTALL_KEY)?;
        delete_reg_key(PRODUCT_REG_KEY)?;
    }
    remove_installed_files(&install_dir)?;
    println!("uninstalled SuperExplorer from {}", install_dir.display());
    Ok(())
}

fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let dest = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            fs::create_dir_all(&dest)?;
            copy_tree(&entry.path(), &dest)?;
        } else {
            fs::copy(entry.path(), &dest)
                .with_context(|| format!("copy {} -> {}", entry.path().display(), dest.display()))?;
        }
    }
    Ok(())
}

fn remove_installed_files(install_dir: &Path) -> Result<()> {
    let files = [
        "SuperExplorer.exe",
        "explorer-extension-broker.exe",
        "superexplorer-mft-helper.exe",
        "superexplorer-mft-service.exe",
        "explorer-extension-worker.exe",
        "superexplorer-quiesce.exe",
        "Everything64.dll",
        "setup-version.txt",
        "Uninstall.exe",
    ];
    for name in files {
        let _ = fs::remove_file(install_dir.join(name));
    }
    let plugins = install_dir.join("plugins");
    if plugins.is_dir() {
        for entry in fs::read_dir(&plugins)? {
            let entry = entry?;
            let _ = fs::remove_file(entry.path());
        }
        let _ = fs::remove_dir(&plugins);
    }
    let _ = fs::remove_dir(install_dir);
    Ok(())
}

fn run_checked(command: &mut Command, action: &str) -> Result<String> {
    let output = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("launch {action}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    if !output.status.success() {
        bail!("{action} failed: {stdout}{stderr}");
    }
    Ok(stdout)
}

fn sc(args: &[&str]) -> Result<(i32, String)> {
    let output = Command::new(r"C:\Windows\System32\sc.exe")
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("launch sc.exe")?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok((output.status.code().unwrap_or(1), text))
}

fn stop_service() -> Result<()> {
    let (code, text) = sc(&["query", SERVICE_NAME])?;
    if code != 0 && text.contains("1060") {
        return Ok(());
    }
    if text.to_ascii_uppercase().contains("STOPPED") {
        return Ok(());
    }
    let (stop_code, stop_text) = sc(&["stop", SERVICE_NAME])?;
    if stop_code != 0 && !stop_text.contains("1061") && !stop_text.contains("1062") && !stop_text.contains("1060")
    {
        bail!("unable to stop {SERVICE_NAME}: {stop_text}");
    }
    for _ in 0..20 {
        let (query_code, query_text) = sc(&["query", SERVICE_NAME])?;
        if query_code != 0 && query_text.contains("1060") {
            return Ok(());
        }
        if query_text.to_ascii_uppercase().contains("STOPPED") {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(500));
    }
    let _ = Command::new(r"C:\Windows\System32\taskkill.exe")
        .args(["/F", "/FI", &format!("SERVICES eq {SERVICE_NAME}")])
        .output();
    for _ in 0..10 {
        let (_, query_text) = sc(&["query", SERVICE_NAME])?;
        if query_text.contains("1060") || query_text.to_ascii_uppercase().contains("STOPPED") {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(500));
    }
    bail!("{SERVICE_NAME} did not enter STOPPED")
}

fn configure_service(install_dir: &Path) -> Result<()> {
    let bin = install_dir.join("superexplorer-mft-service.exe");
    let quoted = format!("\"{}\"", bin.display());
    let (code, text) = sc(&["query", SERVICE_NAME])?;
    if code != 0 {
        let (create_code, create_text) = sc(&[
            "create",
            SERVICE_NAME,
            "binPath=",
            &quoted,
            "start=",
            "auto",
            "obj=",
            "LocalSystem",
            "DisplayName=",
            "SuperExplorer MFT Service",
        ])?;
        if create_code != 0 {
            bail!("unable to create {SERVICE_NAME}: {create_text}");
        }
    } else {
        let (config_code, config_text) = sc(&[
            "config",
            SERVICE_NAME,
            "binPath=",
            &quoted,
            "start=",
            "auto",
            "obj=",
            "LocalSystem",
            "DisplayName=",
            "SuperExplorer MFT Service",
        ])?;
        if config_code != 0 {
            bail!("unable to configure {SERVICE_NAME}: {config_text} {text}");
        }
    }
    let _ = sc(&[
        "description",
        SERVICE_NAME,
        "Read-only NTFS metadata index for SuperExplorer folder snapshots",
    ]);
    let (start_code, start_text) = sc(&["start", SERVICE_NAME])?;
    if start_code != 0 && !start_text.to_ascii_uppercase().contains("RUNNING") {
        bail!("unable to start {SERVICE_NAME}: {start_text}");
    }
    for _ in 0..30 {
        let (_, query_text) = sc(&["query", SERVICE_NAME])?;
        if query_text.to_ascii_uppercase().contains("RUNNING") {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(500));
    }
    bail!("{SERVICE_NAME} did not enter RUNNING")
}

fn delete_service() -> Result<()> {
    let (code, text) = sc(&["delete", SERVICE_NAME])?;
    if code != 0 && !text.contains("1060") {
        bail!("unable to delete {SERVICE_NAME}: {text}");
    }
    Ok(())
}

pub(crate) fn ensure_administrator_for_ui() -> Result<()> {
    ensure_administrator(&default_install_dir()?)
}

fn ensure_administrator(install_dir: &Path) -> Result<()> {
    if is_administrator() {
        return Ok(());
    }
    if is_silent() {
        bail!(
            "SuperExplorer setup must run as administrator to install into {}",
            install_dir.display()
        );
    }
    relaunch_elevated()?;
    std::process::exit(0);
}

fn is_administrator() -> bool {
    use windows::Win32::{
        Foundation::CloseHandle,
        Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation},
        System::Threading::{GetCurrentProcess, OpenProcessToken},
    };
    unsafe {
        let mut token = Default::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION::default();
        let mut returned = 0;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            Some(std::ptr::from_mut(&mut elevation).cast()),
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        )
        .is_ok();
        let _ = CloseHandle(token);
        ok && elevation.TokenIsElevated != 0
    }
}

fn relaunch_elevated() -> Result<()> {
    use windows::{
        Win32::UI::Shell::ShellExecuteW,
        Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
        core::{PCWSTR, w},
    };
    let exe = wide(&env::current_exe()?);
    let params = wide(&env::args().skip(1).collect::<Vec<_>>().join(" "));
    let dir = wide(env::current_dir()?);
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("runas"),
            PCWSTR(exe.as_ptr()),
            PCWSTR(params.as_ptr()),
            PCWSTR(dir.as_ptr()),
            SW_SHOWNORMAL,
        )
    };
    if result.0 as usize <= 32 {
        bail!("unable to elevate SuperExplorer setup");
    }
    Ok(())
}

fn wide(value: impl AsRef<Path>) -> Vec<u16> {
    value.as_ref().as_os_str().encode_wide().chain(Some(0)).collect()
}

fn message_box(title: &str, body: &str) {
    use windows::{
        Win32::UI::WindowsAndMessaging::{MB_OK, MessageBoxW},
        core::PCWSTR,
    };
    let title = wide(Path::new(title));
    let body = wide(Path::new(body));
    unsafe {
        let _ = MessageBoxW(None, PCWSTR(body.as_ptr()), PCWSTR(title.as_ptr()), MB_OK);
    }
}

fn create_shortcut(target: &Path, link: &Path) -> Result<()> {
    use windows::{
        Win32::System::Com::{
            CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
            CoUninitialize, IPersistFile,
        },
        Win32::UI::Shell::{IShellLinkW, ShellLink},
        core::{Interface, PCWSTR},
    };
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let link_obj: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
        link_obj.SetPath(PCWSTR(wide(target).as_ptr()))?;
        let persist: IPersistFile = link_obj.cast()?;
        persist.Save(PCWSTR(wide(link).as_ptr()), true)?;
        CoUninitialize();
    }
    Ok(())
}

fn desktop_dir() -> Result<PathBuf> {
    Ok(PathBuf::from(env::var("USERPROFILE")?).join("Desktop"))
}

fn start_menu_dir() -> Result<PathBuf> {
    Ok(PathBuf::from(env::var("APPDATA")?).join(r"Microsoft\Windows\Start Menu\Programs"))
}

fn write_uninstall_registry(install_dir: &Path) -> Result<()> {
    let version = fs::read_to_string(install_dir.join("setup-version.txt"))
        .unwrap_or_else(|_| "0.0.0.0".to_owned())
        .trim()
        .to_owned();
    let uninstall = format!("\"{}\\Uninstall.exe\"", install_dir.display());
    set_reg_sz(PRODUCT_REG_KEY, "InstallDir", &install_dir.display().to_string())?;
    set_reg_sz(PRODUCT_UNINSTALL_KEY, "DisplayName", PRODUCT_NAME)?;
    set_reg_sz(PRODUCT_UNINSTALL_KEY, "DisplayVersion", &version)?;
    set_reg_sz(PRODUCT_UNINSTALL_KEY, "Publisher", PRODUCT_PUBLISHER)?;
    set_reg_sz(PRODUCT_UNINSTALL_KEY, "URLInfoAbout", PRODUCT_URL)?;
    set_reg_sz(
        PRODUCT_UNINSTALL_KEY,
        "InstallLocation",
        &install_dir.display().to_string(),
    )?;
    set_reg_sz(
        PRODUCT_UNINSTALL_KEY,
        "DisplayIcon",
        &install_dir.join("SuperExplorer.exe").display().to_string(),
    )?;
    set_reg_sz(PRODUCT_UNINSTALL_KEY, "UninstallString", &uninstall)?;
    Ok(())
}

fn set_reg_sz(key: &str, name: &str, value: &str) -> Result<()> {
    let status = Command::new(r"C:\Windows\System32\reg.exe")
        .args(["add", &format!(r"HKLM\{key}"), "/v", name, "/t", "REG_SZ", "/d", value, "/f"])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .status()
        .context("reg.exe add")?;
    if !status.success() {
        bail!("unable to write HKLM\\{key}\\{name}");
    }
    Ok(())
}

fn delete_reg_key(key: &str) -> Result<()> {
    let _ = Command::new(r"C:\Windows\System32\reg.exe")
        .args(["delete", &format!(r"HKLM\{key}"), "/f"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packs_and_extracts_payload_without_service() {
        let payload = tempfile::tempdir().unwrap();
        fs::write(payload.path().join("hello.txt"), b"ok").unwrap();
        fs::write(payload.path().join("setup-version.txt"), "1.2.3.4").unwrap();
        let packed = payload.path().join("packed.exe");
        create_installer(&[
            "--create-installer".into(),
            "--payload-dir".into(),
            payload.path().display().to_string(),
            "--output".into(),
            packed.display().to_string(),
        ])
        .unwrap();
        let bytes = fs::read(&packed).unwrap();
        assert!(bytes.ends_with(MAGIC));
    }
}
