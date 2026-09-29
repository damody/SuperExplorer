//! Read-only archive browsing. Member paths are never extracted onto the filesystem.

use std::{
    collections::BTreeMap,
    io::{self, Read},
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};

pub const MAX_ARCHIVE_ENTRIES: usize = 50_000;
const MAX_LIST_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchivePath {
    pub archive: PathBuf,
    pub member: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchiveEntry {
    pub member: String,
    pub name: String,
    pub directory: bool,
    pub size: Option<u64>,
}

pub fn supported_archive(path: &Path) -> bool {
    path.extension().and_then(|s| s.to_str()).is_some_and(|s| {
        matches!(
            s.to_ascii_lowercase().as_str(),
            "zip" | "7z" | "rar" | "tar" | "tgz" | "gz" | "bz2" | "xz" | "cab"
        )
    })
}

impl ArchivePath {
    /// Resolve only real archive files, so a directory named `photos.zip` stays a directory.
    /// Call on a worker, since this probes the filesystem.
    pub fn resolve(path: &Path) -> Option<Self> {
        for ancestor in path.ancestors() {
            if supported_archive(ancestor) && ancestor.is_file() {
                let suffix = path.strip_prefix(ancestor).ok()?;
                let mut parts = Vec::new();
                for component in suffix.components() {
                    match component {
                        Component::Normal(s) => parts.push(s.to_str()?.to_owned()),
                        _ => return None,
                    }
                }
                let member = parts.join("/");
                if !member.is_empty() && normalize_member(&member).is_err() {
                    return None;
                }
                return Some(Self {
                    archive: ancestor.to_owned(),
                    member,
                });
            }
        }
        None
    }

    pub fn member_path(&self, member: &str) -> PathBuf {
        let mut path = self.archive.clone();
        for component in member.split('/') {
            path.push(component);
        }
        path
    }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

pub fn normalize_member(raw: &str) -> io::Result<String> {
    let raw = raw.replace('\\', "/");
    let raw = raw.strip_prefix("./").unwrap_or(&raw).trim_end_matches('/');
    if raw.is_empty() || raw.starts_with('/') || raw.chars().any(|c| c.is_control() || c == ':') {
        return Err(invalid("Unsafe archive member name"));
    }
    let parts: Vec<_> = raw.split('/').collect();
    if parts.len() > 128
        || parts
            .iter()
            .any(|s| s.is_empty() || *s == "." || *s == ".." || s.ends_with([' ', '.']))
    {
        return Err(invalid("Unsafe archive member path"));
    }
    if raw.len() > 16_384 {
        return Err(invalid("Archive member path is too long"));
    }
    Ok(raw.to_owned())
}

#[derive(Clone)]
enum Backend {
    SevenZip(PathBuf),
    Tar(PathBuf),
}

fn backend() -> io::Result<Backend> {
    if let Ok(exe) = std::env::current_exe()
        && let Some(parent) = exe.parent()
        && parent.join("7z.exe").is_file()
    {
        return Ok(Backend::SevenZip(parent.join("7z.exe")));
    }
    for variable in ["ProgramW6432", "ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(root) = std::env::var_os(variable) {
            let exe = PathBuf::from(root).join("7-Zip/7z.exe");
            if exe.is_file() {
                return Ok(Backend::SevenZip(exe));
            }
        }
    }
    #[cfg(windows)]
    if let Some(root) = std::env::var_os("SystemRoot") {
        let exe = PathBuf::from(root).join("System32/tar.exe");
        if exe.is_file() {
            return Ok(Backend::Tar(exe));
        }
    }
    #[cfg(not(windows))]
    return Ok(Backend::Tar(PathBuf::from("bsdtar")));
    #[cfg(windows)]
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "Archive reader unavailable. Install 7-Zip or enable Windows tar.",
    ))
}

/// Read bounded stdout, drain bounded stderr, and kill stalled/cancelled children.
fn run(
    mut command: Command,
    cap: usize,
    prefix: bool,
    cancelled: &dyn Fn() -> bool,
) -> io::Result<Vec<u8>> {
    if cancelled() {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "Archive read cancelled",
        ));
    }
    crate::configure_background_command(&mut command);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn()?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| invalid("Missing archive output"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| invalid("Missing archive errors"))?;
    let (sender, receiver) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let outcome = stdout
            .take(cap.saturating_add(1) as u64)
            .read_to_end(&mut bytes)
            .map(|_| bytes);
        let _ = sender.send(outcome);
    });
    // Discard after 64 KiB while continuing to drain, so stderr cannot block the child.
    std::thread::spawn(move || {
        let _ = io::copy(&mut { stderr }, &mut io::sink());
    });
    let start = Instant::now();
    let output = loop {
        if cancelled() || start.elapsed() > Duration::from_secs(30) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(io::Error::new(
                if cancelled() {
                    io::ErrorKind::Interrupted
                } else {
                    io::ErrorKind::TimedOut
                },
                "Archive read cancelled or timed out",
            ));
        }
        match receiver.recv_timeout(Duration::from_millis(20)) {
            Ok(output) => break output,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(invalid("Archive reader stopped"));
            }
        }
    };
    let mut bytes = match output {
        Ok(bytes) => bytes,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
    };
    if bytes.len() > cap {
        let _ = child.kill();
        let _ = child.wait();
        if prefix {
            bytes.truncate(cap);
            return Ok(bytes);
        }
        return Err(invalid("Archive exceeds the browsing or preview limit"));
    }
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if cancelled() || start.elapsed() > Duration::from_secs(30) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Archive read timed out",
            ));
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    if !status.success() {
        return Err(invalid(
            "Unable to read archive (damaged, encrypted, or unsupported format)",
        ));
    }
    Ok(bytes)
}

fn parse_seven_zip(text: &str) -> io::Result<Vec<ArchiveEntry>> {
    let mut entries = Vec::new();
    for block in text.replace("\r\n", "\n").split("\n\n") {
        if block.trim().is_empty() {
            continue;
        }
        let mut path = None;
        let mut directory = false;
        let mut size = None;
        let mut link = false;
        for line in block.lines() {
            if let Some(value) = line.strip_prefix("Path = ") {
                path = Some(value);
            } else if let Some(value) = line.strip_prefix("Folder = ") {
                directory = value == "+";
            } else if let Some(value) = line.strip_prefix("Size = ") {
                size = value.parse().ok();
            } else if let Some(value) = line
                .strip_prefix("Symbolic Link = ")
                .or_else(|| line.strip_prefix("Hard Link = "))
                .or_else(|| line.strip_prefix("Copy Link = "))
            {
                link |= !value.trim().is_empty();
            } else if let Some(value) = line.strip_prefix("Attributes = ") {
                directory |= value.starts_with('D');
                link |= value.trim_start().starts_with('l');
            }
        }
        if link {
            continue;
        }
        let member = normalize_member(path.ok_or_else(|| invalid("Malformed archive listing"))?)?;
        let name = member.rsplit('/').next().unwrap_or_default().to_owned();
        entries.push(ArchiveEntry {
            member,
            name,
            directory,
            size,
        });
        if entries.len() > MAX_ARCHIVE_ENTRIES {
            return Err(invalid("Too many archive entries"));
        }
    }
    Ok(entries)
}

fn parse_tar(text: &str) -> io::Result<Vec<ArchiveEntry>> {
    let mut entries = Vec::new();
    for line in text.lines() {
        // bsdtar: permissions, links, owner, group, size, month, day, time/year, name.
        if !line.starts_with(['d', '-']) {
            continue;
        }
        let mut rest = line;
        let mut fields = Vec::new();
        for _ in 0..8 {
            rest = rest.trim_start();
            let end = rest
                .find(char::is_whitespace)
                .ok_or_else(|| invalid("Malformed archive listing"))?;
            fields.push(&rest[..end]);
            rest = &rest[end..];
        }
        let raw_member = rest.trim_start();
        // bsdtar escapes special names in its listing. Do not treat escapes as real paths.
        if raw_member.contains("\\") {
            return Err(invalid("Unrepresentable archive member name"));
        }
        let member = normalize_member(raw_member)?;
        let name = member.rsplit('/').next().unwrap_or_default().to_owned();
        entries.push(ArchiveEntry {
            member,
            name,
            directory: line.starts_with('d'),
            size: fields[4].parse().ok(),
        });
        if entries.len() > MAX_ARCHIVE_ENTRIES {
            return Err(invalid("Too many archive entries"));
        }
    }
    Ok(entries)
}

fn decode_tar_output(bytes: Vec<u8>) -> io::Result<String> {
    match String::from_utf8(bytes) {
        Ok(text) => Ok(text),
        Err(error) => decode_system_text(error.as_bytes()),
    }
}

/// Decode legacy plain text using the same Windows system code page as native tools.
pub fn decode_legacy_text(bytes: &[u8]) -> io::Result<String> {
    decode_system_text(bytes)
}

#[cfg(windows)]
#[expect(
    unsafe_code,
    reason = "bounded conversion of Windows bsdtar ANSI output through MultiByteToWideChar"
)]
fn decode_system_text(bytes: &[u8]) -> io::Result<String> {
    // Windows bsdtar prints names in the system ANSI code page, not UTF-8.
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MultiByteToWideChar(
            code_page: u32,
            flags: u32,
            bytes: *const u8,
            length: i32,
            output: *mut u16,
            capacity: i32,
        ) -> i32;
    }
    let length = i32::try_from(bytes.len()).map_err(|_| invalid("Archive listing is too large"))?;
    // SAFETY: inputs are bounded slices; the first call measures, the second writes into
    // a buffer of the measured length. CP_ACP is the encoding used by Windows bsdtar.
    let needed =
        unsafe { MultiByteToWideChar(0, 8, bytes.as_ptr(), length, std::ptr::null_mut(), 0) };
    if needed <= 0 {
        return Err(invalid("Cannot decode archive member names"));
    }
    let mut wide = vec![0; needed as usize];
    let written =
        unsafe { MultiByteToWideChar(0, 8, bytes.as_ptr(), length, wide.as_mut_ptr(), needed) };
    if written != needed {
        return Err(invalid("Cannot decode archive member names"));
    }
    String::from_utf16(&wide).map_err(|_| invalid("Invalid archive member names"))
}

#[cfg(not(windows))]
fn decode_system_text(_: &[u8]) -> io::Result<String> {
    Err(invalid("Archive listing is not UTF-8"))
}

pub fn list_archive(path: &Path, cancelled: &dyn Fn() -> bool) -> io::Result<Vec<ArchiveEntry>> {
    list_with_backend(path, backend()?, cancelled)
}

fn list_with_backend(
    path: &Path,
    backend: Backend,
    cancelled: &dyn Fn() -> bool,
) -> io::Result<Vec<ArchiveEntry>> {
    let entries = match backend {
        Backend::SevenZip(exe) => {
            let mut command = Command::new(exe);
            command
                .args(["l", "-slt", "-ba", "-sccUTF-8", "--"])
                .arg(path);
            let bytes = run(command, MAX_LIST_BYTES, false, cancelled)?;
            parse_seven_zip(
                &String::from_utf8(bytes)
                    .map_err(|_| invalid("Invalid archive listing encoding"))?,
            )?
        }
        Backend::Tar(exe) => {
            let mut command = Command::new(exe);
            command.arg("-tvf").arg(path);
            parse_tar(&decode_tar_output(run(
                command,
                MAX_LIST_BYTES,
                false,
                cancelled,
            )?)?)?
        }
    };
    let mut names = std::collections::HashSet::new();
    for entry in &entries {
        if !names.insert(entry.member.to_lowercase()) {
            return Err(invalid("Archive has ambiguous duplicate member paths"));
        }
    }
    Ok(entries)
}

pub fn children(entries: &[ArchiveEntry], parent: &str) -> io::Result<Vec<ArchiveEntry>> {
    let prefix = if parent.is_empty() {
        String::new()
    } else {
        format!("{}/", normalize_member(parent)?)
    };
    let mut children = BTreeMap::<String, ArchiveEntry>::new();
    for entry in entries {
        let Some(relative) = entry.member.strip_prefix(&prefix) else {
            continue;
        };
        let Some(name) = relative.split('/').next() else {
            continue;
        };
        if name.is_empty() {
            continue;
        }
        let directory = entry.directory || relative.contains('/');
        let child = ArchiveEntry {
            member: format!("{prefix}{name}"),
            name: name.to_owned(),
            directory,
            size: (!directory).then_some(entry.size).flatten(),
        };
        match children.get(&name.to_lowercase()) {
            Some(previous) if previous.directory != directory => {
                return Err(invalid("Archive contains a file/folder path conflict"));
            }
            _ => {
                children.insert(name.to_lowercase(), child);
            }
        }
    }
    if !parent.is_empty()
        && children.is_empty()
        && !entries.iter().any(|e| e.member == parent && e.directory)
    {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Archive folder does not exist",
        ));
    }
    Ok(children.into_values().collect())
}

/// Preview a member prefix or read a complete member with a strict byte quota.
pub fn read_member(
    path: &ArchivePath,
    cap: usize,
    prefix: bool,
    cancelled: &dyn Fn() -> bool,
) -> io::Result<Vec<u8>> {
    read_with_backend(path, cap, prefix, backend()?, cancelled)
}

fn read_with_backend(
    path: &ArchivePath,
    cap: usize,
    prefix: bool,
    backend: Backend,
    cancelled: &dyn Fn() -> bool,
) -> io::Result<Vec<u8>> {
    let normalized = normalize_member(&path.member)?;
    let entries = list_with_backend(&path.archive, backend.clone(), cancelled)?;
    let entry = entries
        .iter()
        .find(|e| e.member == normalized && !e.directory)
        .ok_or_else(|| invalid("Archive member does not exist"))?;
    if !prefix && entry.size.is_some_and(|size| size > cap as u64) {
        return Err(invalid("Archive member exceeds the preview limit"));
    }
    let command = match backend {
        Backend::SevenZip(exe) => {
            let mut command = Command::new(exe);
            command
                .args(["x", "-so", "-spd", "-y", "--"])
                .arg(&path.archive)
                .arg(&entry.member);
            command
        }
        Backend::Tar(exe) => {
            let mut command = Command::new(exe);
            let mut escaped = String::new();
            for character in entry.member.chars() {
                match character {
                    '[' => escaped.push_str("[[]"),
                    '*' => escaped.push_str("[*]"),
                    '?' => escaped.push_str("[?]"),
                    c => escaped.push(c),
                }
            }
            command
                .arg("-xOf")
                .arg(&path.archive)
                .arg("--")
                .arg(escaped);
            command
        }
    };
    run(command, cap, prefix, cancelled)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_members_reject_escape_and_preserve_unicode() {
        for name in [
            "../x", "/x", "C:\\x", "a/../b", "a//b", "a\nname", "a/thing.",
        ] {
            assert!(normalize_member(name).is_err(), "{name}");
        }
        assert_eq!(
            normalize_member("資料夾\\說明 文件.md").unwrap(),
            "資料夾/說明 文件.md"
        );
    }

    #[test]
    fn archive_listing_synthesizes_missing_parents_and_empty_folders() {
        let entries = parse_seven_zip(
            "Path = a/b/readme.md\nFolder = -\nSize = 12\n\nPath = empty\nFolder = +\nSize = 0\n",
        )
        .unwrap();
        let root = children(&entries, "").unwrap();
        assert_eq!(
            root.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
            ["a", "empty"]
        );
        assert!(root.iter().all(|e| e.directory));
        assert_eq!(children(&entries, "a/b").unwrap()[0].size, Some(12));
        assert!(children(&entries, "empty").unwrap().is_empty());
        assert!(children(&entries, "missing").is_err());
    }

    #[test]
    fn archive_tar_listing_preserves_spaces_and_sizes() {
        let entries = parse_tar("-rw-r--r-- 0 0 0 123 Sep 29 12:00 folder/name with spaces.txt\ndrwxr-xr-x 0 0 0 0 Sep 29 12:00 empty/\n").unwrap();
        assert_eq!(entries[0].member, "folder/name with spaces.txt");
        assert_eq!(entries[0].size, Some(123));
        assert!(entries[1].directory);
    }

    #[cfg(windows)]
    #[test]
    fn archive_real_zip_7z_rar_browse_and_read_with_native_and_seven_zip_backends() {
        use std::io::Write as _;
        let temporary = tempfile::tempdir().unwrap();
        let zip_path = temporary.path().join("unicode.zip");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&zip_path).unwrap());
        zip.start_file(
            "資料夾/說明 文件.md",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        let source = format!("# 標題\n{}", "文字😀".repeat(2_000));
        zip.write_all(source.as_bytes()).unwrap();
        zip.start_file("literal[1]*?.txt", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"exact-member").unwrap();
        zip.add_directory("empty/", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.finish().unwrap();
        let source_dir = temporary.path().join("source");
        std::fs::create_dir(&source_dir).unwrap();
        std::fs::write(source_dir.join("note.txt"), b"seven-zip member").unwrap();
        let seven_path = temporary.path().join("archive.7z");
        let tar = Backend::Tar(
            PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join("System32/tar.exe"),
        );
        let Backend::Tar(tar_exe) = &tar else {
            unreachable!()
        };
        let mut create = Command::new(tar_exe);
        create
            .arg("-cf")
            .arg(&seven_path)
            .arg("--format=7zip")
            .arg("-C")
            .arg(&source_dir)
            .arg("note.txt");
        crate::configure_background_command(&mut create);
        assert!(create.output().unwrap().status.success());
        let rar_path = temporary.path().join("archive.rar");
        std::fs::write(&rar_path, include_bytes!("../tests/fixtures/browse.rar")).unwrap();
        let rar5_path = temporary.path().join("archive-rar5.rar");
        std::fs::write(
            &rar5_path,
            include_bytes!("../tests/fixtures/browse-rar5.rar"),
        )
        .unwrap();
        for reader in [backend().unwrap(), tar] {
            let root = list_with_backend(&zip_path, reader.clone(), &|| false).unwrap();
            assert!(
                children(&root, "")
                    .unwrap()
                    .iter()
                    .any(|entry| entry.name == "資料夾" && entry.directory)
            );
            assert_eq!(
                children(&root, "資料夾").unwrap()[0].size,
                Some(source.len() as u64)
            );
            assert!(children(&root, "empty").unwrap().is_empty());
            let member = ArchivePath {
                archive: zip_path.clone(),
                member: "資料夾/說明 文件.md".to_owned(),
            };
            assert_eq!(
                read_with_backend(&member, 23, true, reader.clone(), &|| false).unwrap(),
                source.as_bytes()[..23]
            );
            assert!(read_with_backend(&member, 23, false, reader.clone(), &|| false).is_err());
            assert!(read_with_backend(&member, 23, true, reader.clone(), &|| true).is_err());
            let literal = ArchivePath {
                archive: zip_path.clone(),
                member: "literal[1]*?.txt".to_owned(),
            };
            assert_eq!(
                read_with_backend(&literal, 100, false, reader.clone(), &|| false).unwrap(),
                b"exact-member"
            );
            let seven = ArchivePath {
                archive: seven_path.clone(),
                member: "note.txt".to_owned(),
            };
            assert_eq!(
                read_with_backend(&seven, 100, false, reader.clone(), &|| false).unwrap(),
                b"seven-zip member"
            );
            let rar = list_with_backend(&rar_path, reader.clone(), &|| false).unwrap();
            assert!(children(&rar, "testemptydir").unwrap().is_empty());
            assert!(rar.iter().all(|entry| entry.name != "testlink"));
            let member = ArchivePath {
                archive: rar_path.clone(),
                member: "testdir/test.txt".to_owned(),
            };
            assert_eq!(
                read_with_backend(&member, 100, false, reader.clone(), &|| false).unwrap(),
                b"test text document\r\n"
            );
            let rar5 = list_with_backend(&rar5_path, reader.clone(), &|| false).unwrap();
            assert_eq!(
                rar5.len(),
                4,
                "empty link metadata must not hide ordinary RAR5 files"
            );
            assert!(
                rar5.iter()
                    .all(|entry| !entry.directory && entry.size == Some(4_096))
            );
            let member = ArchivePath {
                archive: rar5_path.clone(),
                member: "test1.bin".to_owned(),
            };
            let full = read_with_backend(&member, 8_192, false, reader.clone(), &|| false).unwrap();
            assert_eq!(full.len(), 4_096);
            assert_eq!(
                read_with_backend(&member, 37, true, reader, &|| false).unwrap(),
                full[..37]
            );
        }
        let fake_directory = temporary.path().join("folder.zip");
        std::fs::create_dir(&fake_directory).unwrap();
        assert!(ArchivePath::resolve(&fake_directory).is_none());
        assert_eq!(
            ArchivePath::resolve(&zip_path.join("資料夾/說明 文件.md"))
                .unwrap()
                .member,
            "資料夾/說明 文件.md"
        );
    }
}
