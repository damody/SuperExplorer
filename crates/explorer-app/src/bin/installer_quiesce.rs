#![cfg(windows)]
#![expect(
    unsafe_code,
    reason = "installer quiescence enumerates and closes SuperExplorer processes by image path"
)]

use std::{
    env, fs,
    mem::size_of,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE, HWND, LPARAM, WPARAM},
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
                TH32CS_SNAPPROCESS,
            },
            Threading::{
                OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
                PROCESS_TERMINATE, QueryFullProcessImageNameW, TerminateProcess,
            },
        },
        UI::WindowsAndMessaging::{
            EnumWindows, GetWindowThreadProcessId, IsWindowVisible, PostMessageW, WM_CLOSE,
        },
    },
    core::BOOL,
};

const TARGET_FILE_NAME: &str = "SuperExplorer.exe";

fn main() {
    match run(env::args_os().skip(1).collect()) {
        Ok(message) => {
            println!("{message}");
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}

fn run(args: Vec<std::ffi::OsString>) -> Result<String, String> {
    let mut install_directory = None;
    let mut graceful = Duration::from_millis(5000);
    let mut force = Duration::from_millis(5000);
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].to_string_lossy();
        match flag.as_ref() {
            "--install-directory" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "missing --install-directory value".to_owned())?;
                install_directory = Some(PathBuf::from(value));
            }
            "--graceful-ms" => {
                index += 1;
                graceful = parse_millis(&args, index, "--graceful-ms")?;
            }
            "--force-ms" => {
                index += 1;
                force = parse_millis(&args, index, "--force-ms")?;
            }
            other => return Err(format!("unknown argument: {other}")),
        }
        index += 1;
    }
    let install_directory =
        install_directory.ok_or_else(|| "missing --install-directory".to_owned())?;
    let target = normalize_path(&install_directory.join(TARGET_FILE_NAME))?;
    let initial = matching_process_ids(&target)?;
    close_main_windows(&initial);
    if !wait_until_absent(&target, graceful)? {
        for process_id in matching_process_ids(&target)? {
            terminate_process(process_id)?;
        }
        if !wait_until_absent(&target, force)? {
            return Err(format!(
                "target SuperExplorer processes remained after bounded force termination: {}",
                target.display()
            ));
        }
    }
    if !matching_process_ids(&target)?.is_empty() {
        return Err(format!(
            "final target-process absence could not be proven: {}",
            target.display()
        ));
    }
    Ok(format!(
        "SuperExplorer quiescence verified: target={} initial={}",
        target.display(),
        initial.len()
    ))
}

fn parse_millis(
    args: &[std::ffi::OsString],
    index: usize,
    flag: &str,
) -> Result<Duration, String> {
    let value = args
        .get(index)
        .ok_or_else(|| format!("missing {flag} value"))?
        .to_string_lossy();
    let millis = value
        .parse::<u64>()
        .map_err(|_| format!("invalid {flag} value: {value}"))?;
    if millis > 30_000 {
        return Err(format!("{flag} exceeds 30000"));
    }
    Ok(Duration::from_millis(millis))
}

fn normalize_path(path: &Path) -> Result<PathBuf, String> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()
            .map_err(|error| error.to_string())?
            .join(path)
    };
    match fs::canonicalize(&absolute) {
        Ok(canonical) => Ok(strip_verbatim_prefix(canonical)),
        Err(_) => Ok(absolute),
    }
}

fn strip_verbatim_prefix(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        path
    }
}

fn paths_equal(left: &Path, right: &Path) -> bool {
    left.as_os_str().eq_ignore_ascii_case(right.as_os_str())
}

fn matching_process_ids(target: &Path) -> Result<Vec<u32>, String> {
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }
        .map_err(|error| format!("CreateToolhelp32Snapshot failed: {error}"))?;
    struct Snapshot(HANDLE);
    impl Drop for Snapshot {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
    let snapshot = Snapshot(snapshot);
    let mut entry = PROCESSENTRY32W {
        dwSize: size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    unsafe {
        Process32FirstW(snapshot.0, &mut entry)
            .map_err(|error| format!("Process32FirstW failed: {error}"))?;
    }
    let mut matches = Vec::new();
    loop {
        let name = string_from_utf16_nul(&entry.szExeFile);
        if name.eq_ignore_ascii_case(TARGET_FILE_NAME)
            && let Some(image) = process_image_path(entry.th32ProcessID)
            && paths_equal(&image, target)
        {
            matches.push(entry.th32ProcessID);
        }
        unsafe {
            if Process32NextW(snapshot.0, &mut entry).is_err() {
                break;
            }
        }
    }
    Ok(matches)
}

fn string_from_utf16_nul(buffer: &[u16]) -> String {
    let length = buffer.iter().position(|unit| *unit == 0).unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..length])
}

fn process_image_path(process_id: u32) -> Option<PathBuf> {
    let handle = unsafe {
        OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id).ok()?
    };
    struct Process(HANDLE);
    impl Drop for Process {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
    let process = Process(handle);
    let mut buffer = [0u16; 32768];
    let mut length = buffer.len() as u32;
    unsafe {
        QueryFullProcessImageNameW(process.0, PROCESS_NAME_WIN32, windows::core::PWSTR(buffer.as_mut_ptr()), &mut length)
            .ok()?;
    }
    Some(strip_verbatim_prefix(PathBuf::from(String::from_utf16_lossy(
        &buffer[..length as usize],
    ))))
}

fn close_main_windows(process_ids: &[u32]) {
    for process_id in process_ids {
        let mut state = *process_id;
        unsafe {
            let _ = EnumWindows(Some(close_window_for_pid), LPARAM(&mut state as *mut u32 as isize));
        }
    }
}

unsafe extern "system" fn close_window_for_pid(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let target = unsafe { *(lparam.0 as *const u32) };
    let mut window_pid = 0;
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut window_pid));
        if window_pid == target && IsWindowVisible(hwnd).as_bool() {
            let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }
    BOOL(1)
}

fn terminate_process(process_id: u32) -> Result<(), String> {
    let handle = unsafe {
        OpenProcess(
            PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION,
            false,
            process_id,
        )
    }
    .map_err(|error| format!("OpenProcess({process_id}) failed: {error}"))?;
    struct Process(HANDLE);
    impl Drop for Process {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
    let process = Process(handle);
    unsafe {
        TerminateProcess(process.0, 1)
            .map_err(|error| format!("TerminateProcess({process_id}) failed: {error}"))?;
    }
    Ok(())
}

fn wait_until_absent(target: &Path, timeout: Duration) -> Result<bool, String> {
    let deadline = Instant::now() + timeout;
    loop {
        if matching_process_ids(target)?.is_empty() {
            return Ok(true);
        }
        if Instant::now() >= deadline {
            return Ok(false);
        }
        thread::sleep(Duration::from_millis(100));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    #[test]
    fn quiesce_only_matches_install_directory_image() {
        let fixture = tempfile::tempdir().expect("fixture");
        let target_root = fixture.path().join("installed");
        let outside_root = fixture.path().join("outside");
        fs::create_dir_all(&target_root).expect("target root");
        fs::create_dir_all(&outside_root).expect("outside root");
        let cmd = PathBuf::from(env::var("SystemRoot").expect("SystemRoot")).join("System32/cmd.exe");
        let target_exe = target_root.join(TARGET_FILE_NAME);
        let outside_exe = outside_root.join(TARGET_FILE_NAME);
        fs::copy(&cmd, &target_exe).expect("copy target");
        fs::copy(&cmd, &outside_exe).expect("copy outside");

        let message = run(vec![
            "--install-directory".into(),
            target_root.into(),
            "--graceful-ms".into(),
            "0".into(),
            "--force-ms".into(),
            "1000".into(),
        ])
        .expect("empty quiesce");
        assert!(message.contains("initial=0"));

        let mut target = Command::new(&target_exe)
            .args(["/d", "/c", "ping.exe -t 127.0.0.1"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("start target");
        let mut outside = Command::new(&outside_exe)
            .args(["/d", "/c", "ping.exe -t 127.0.0.1"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("start outside");
        thread::sleep(Duration::from_millis(400));
        run(vec![
            "--install-directory".into(),
            fixture.path().join("installed").into(),
            "--graceful-ms".into(),
            "0".into(),
            "--force-ms".into(),
            "2000".into(),
        ])
        .expect("path-scoped quiesce");
        let target_exited = target.wait().expect("wait target").code().is_some();
        let outside_alive = outside.try_wait().expect("poll outside").is_none();
        if outside_alive {
            let _ = outside.kill();
            let _ = outside.wait();
        }
        assert!(target_exited);
        assert!(outside_alive);
    }
}
