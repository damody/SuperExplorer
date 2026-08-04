//! Pure-Rust 7z virtual-folder example with bounded reads and transactional mutation.

use abi_stable::{
    export_root_module,
    prefix_type::PrefixTypeTrait,
    std_types::{ROption, RResult},
};
use explorer_extension_api::*;
use sevenz_rust::{Archive, Error as SevenZError, Password, SevenZReader};
use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const PLUGIN_ID: StableIdV1 = StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 8_101);
const INTERFACE_ID: StableIdV1 = StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 8_102);

/// Opaque, non-serializable secret owned only for the active archive session.
pub struct ArchiveSecret(Vec<u16>);
impl ArchiveSecret {
    pub fn new(value: &str) -> Self {
        Self(value.encode_utf16().collect())
    }
    fn password(&self) -> Password {
        Password::from(self.0.as_slice())
    }
}
impl std::fmt::Debug for ArchiveSecret {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ArchiveSecret([redacted])")
    }
}
impl Drop for ArchiveSecret {
    fn drop(&mut self) {
        self.0.fill(0);
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VirtualEntry {
    pub id: u64,
    pub path: String,
    pub size: u64,
    pub compressed_size: u64,
    pub crc: Option<u32>,
    pub encrypted: bool,
    pub is_directory: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct ResourcePolicy {
    pub max_entries: usize,
    pub max_depth: usize,
    pub max_total: u64,
    pub max_ratio: u64,
    pub max_single_read: usize,
}

pub fn normalize_entry(path: &str) -> Result<String, &'static str> {
    if path.contains('\0') {
        return Err("NUL");
    }
    let path = Path::new(path);
    if path.is_absolute() {
        return Err("absolute");
    }
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => {
                let value = value.to_str().ok_or("encoding")?;
                if value.is_empty() {
                    return Err("empty");
                }
                parts.push(value);
            }
            Component::CurDir => {}
            _ => return Err("traversal"),
        }
    }
    if parts.is_empty() {
        return Err("empty");
    }
    Ok(parts.join("/"))
}

pub fn validate_entries(
    entries: &[VirtualEntry],
    policy: ResourcePolicy,
) -> Result<(), &'static str> {
    if entries.len() > policy.max_entries {
        return Err("entry limit");
    }
    let mut names = BTreeSet::new();
    let mut total = 0_u64;
    for entry in entries {
        let normalized = normalize_entry(&entry.path)?;
        if normalized.split('/').count() > policy.max_depth {
            return Err("depth");
        }
        if !names.insert(normalized.to_lowercase()) {
            return Err("collision");
        }
        total = total.checked_add(entry.size).ok_or("size")?;
        if entry.compressed_size > 0 && entry.size / entry.compressed_size > policy.max_ratio {
            return Err("ratio");
        }
    }
    if total > policy.max_total {
        return Err("total");
    }
    Ok(())
}

/// Enumerates the actual 7z central metadata and applies all resource limits before exposing it.
pub fn enumerate_archive(path: &Path, policy: ResourcePolicy) -> Result<Vec<VirtualEntry>, String> {
    enumerate_archive_with_secret(path, policy, None)
}

pub fn enumerate_archive_with_secret(
    path: &Path,
    policy: ResourcePolicy,
    secret: Option<&ArchiveSecret>,
) -> Result<Vec<VirtualEntry>, String> {
    let password = secret.map_or_else(Password::empty, ArchiveSecret::password);
    let archive =
        Archive::open_with_password(path, &password).map_err(|error| error.to_string())?;
    let entries = archive
        .files
        .iter()
        .enumerate()
        // `sevenz-rust` records the compressed root directory as an empty-name
        // directory. It is an archive implementation detail, not a child item.
        .filter(|(_, entry)| !(entry.is_directory && entry.name.is_empty()))
        .map(|(index, entry)| VirtualEntry {
            id: index as u64,
            path: entry.name.clone(),
            size: entry.size,
            compressed_size: entry.compressed_size,
            crc: entry.has_crc.then_some(entry.crc as u32),
            encrypted: secret.is_some(),
            is_directory: entry.is_directory,
        })
        .collect::<Vec<_>>();
    validate_entries(&entries, policy).map_err(str::to_owned)?;
    Ok(entries)
}

/// Reads one archive member without extracting sibling files and never buffers beyond the quota.
pub fn read_entry(
    archive_path: &Path,
    wanted: &str,
    policy: ResourcePolicy,
) -> Result<Vec<u8>, String> {
    read_entry_with_secret(archive_path, wanted, policy, None)
}

pub fn read_entry_with_secret(
    archive_path: &Path,
    wanted: &str,
    policy: ResourcePolicy,
    secret: Option<&ArchiveSecret>,
) -> Result<Vec<u8>, String> {
    let wanted = normalize_entry(wanted).map_err(str::to_owned)?;
    let metadata = enumerate_archive_with_secret(archive_path, policy, secret)?;
    let entry = metadata
        .iter()
        .find(|entry| entry.path == wanted && !entry.is_directory)
        .ok_or_else(|| "entry not found".to_owned())?;
    if entry.size > policy.max_single_read as u64 {
        return Err("read quota".to_owned());
    }
    let mut reader = SevenZReader::open(
        archive_path,
        secret.map_or_else(Password::empty, ArchiveSecret::password),
    )
    .map_err(|error| error.to_string())?;
    let mut output = None;
    reader
        .for_each_entries(|candidate, source| {
            if normalize_entry(candidate.name()).ok().as_deref() != Some(wanted.as_str()) {
                return Ok(true);
            }
            let mut limited = source.take(policy.max_single_read as u64 + 1);
            let mut bytes =
                Vec::with_capacity(candidate.size.min(policy.max_single_read as u64) as usize);
            limited.read_to_end(&mut bytes).map_err(SevenZError::io)?;
            if bytes.len() > policy.max_single_read {
                return Err(SevenZError::other("read quota"));
            }
            output = Some(bytes);
            Ok(false)
        })
        .map_err(|error| error.to_string())?;
    output.ok_or_else(|| "entry not found".to_owned())
}

/// Safely extracts an archive after validating every normalized destination and size bound.
pub fn extract_archive(
    archive_path: &Path,
    destination: &Path,
    policy: ResourcePolicy,
) -> Result<Vec<PathBuf>, String> {
    extract_archive_with_secret(archive_path, destination, policy, None)
}

pub fn extract_archive_with_secret(
    archive_path: &Path,
    destination: &Path,
    policy: ResourcePolicy,
    secret: Option<&ArchiveSecret>,
) -> Result<Vec<PathBuf>, String> {
    let entries = enumerate_archive_with_secret(archive_path, policy, secret)?;
    fs::create_dir_all(destination).map_err(|error| error.to_string())?;
    let mut reader = SevenZReader::open(
        archive_path,
        secret.map_or_else(Password::empty, ArchiveSecret::password),
    )
    .map_err(|error| error.to_string())?;
    let mut written = Vec::new();
    reader
        .for_each_entries(|entry, source| {
            if entry.is_directory() && entry.name().is_empty() {
                return Ok(true);
            }
            let normalized = normalize_entry(entry.name()).map_err(SevenZError::other)?;
            let target = normalized
                .split('/')
                .fold(destination.to_path_buf(), |path, part| path.join(part));
            if entry.is_directory() {
                fs::create_dir_all(&target).map_err(SevenZError::io)?;
            } else {
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent).map_err(SevenZError::io)?;
                }
                let mut file = File::create(&target).map_err(SevenZError::io)?;
                let copied = std::io::copy(source, &mut file).map_err(SevenZError::io)?;
                if copied != entry.size {
                    return Err(SevenZError::other("entry size mismatch"));
                }
                file.flush().map_err(SevenZError::io)?;
                written.push(target);
            }
            Ok(true)
        })
        .map_err(|error| error.to_string())?;
    // The initial metadata pass is the authoritative security decision.
    debug_assert_eq!(
        entries.iter().filter(|entry| !entry.is_directory).count(),
        written.len()
    );
    Ok(written)
}

fn sibling_work_dir(container: &Path) -> Result<PathBuf, String> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let parent = container
        .parent()
        .ok_or_else(|| "container has no parent".to_owned())?;
    Ok(parent.join(format!(".superexplorer-7z-{nonce}")))
}

/// Extracts, mutates, repacks, verifies, rechecks the original, then atomically replaces it.
/// The returned sibling backup is the conservative whole-container undo token.
pub fn mutate_archive(
    container: &Path,
    policy: ResourcePolicy,
    mutation: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<PathBuf, String> {
    let original = fs::read(container).map_err(|error| error.to_string())?;
    let work = sibling_work_dir(container)?;
    fs::create_dir(&work).map_err(|error| error.to_string())?;
    let result = (|| {
        extract_archive(container, &work, policy)?;
        mutation(&work)?;
        let staging = container.with_extension("7z.superexplorer-stage");
        sevenz_rust::compress_to_path(&work, &staging).map_err(|error| error.to_string())?;
        enumerate_archive(&staging, policy)?;
        if fs::read(container).map_err(|error| error.to_string())? != original {
            return Err("original changed".to_owned());
        }
        let backup = container.with_extension("7z.superexplorer-undo");
        if backup.exists() {
            return Err("undo backup already exists".to_owned());
        }
        fs::rename(container, &backup).map_err(|error| error.to_string())?;
        if let Err(error) = fs::rename(&staging, container) {
            let _ = fs::rename(&backup, container);
            return Err(error.to_string());
        }
        Ok(backup)
    })();
    let _ = fs::remove_dir_all(&work);
    result
}

struct Registrar;
impl ExtensionRegistrarImplementationV1 for Registrar {
    fn create() -> Self {
        Self
    }
    fn register(&self, _: RegistrarRequestV1) -> RegistrarOutputResultV1 {
        let kinds = [
            ("rust-7z:resource", RegisteredContributionKindV1::RESOURCE),
            (
                "rust-7z:mutate",
                RegisteredContributionKindV1::OPERATION_PLAN,
            ),
        ];
        RResult::ROk(RegistrarOutputV1 {
            outcome: RegistrationOutcomeV1::accepted(2),
            contributions: kinds
                .into_iter()
                .map(|(id, kind)| RegisteredContributionV1 {
                    feature_id: "rust-7z".into(),
                    contribution_id: id.into(),
                    kind,
                    required_capabilities: vec![
                        "filesystem.read".into(),
                        "filesystem.write".into(),
                    ]
                    .into(),
                    interface_id: INTERFACE_ID,
                    expected_sort: ROption::RNone,
                    opaque_contract: ROption::RNone,
                    renderer_contribution_id: ROption::RNone,
                    provider: ROption::RNone,
                    visual_column: ROption::RNone,
                    size_map_view: ROption::RNone,
                    batch_column_provider: ROption::RNone,
                })
                .collect::<Vec<_>>()
                .into(),
        })
    }
}

#[export_root_module]
pub fn plugin_root() -> ExtensionRootModuleV1_Ref {
    ExtensionRootModuleV1::new::<Registrar>(
        PluginMetadataV1 {
            plugin_id: PLUGIN_ID,
            primary_interface_id: INTERFACE_ID,
        },
        ROption::RNone,
    )
    .leak_into_prefix()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static FIXTURE_ID: AtomicU64 = AtomicU64::new(1);

    fn policy() -> ResourcePolicy {
        ResourcePolicy {
            max_entries: 20,
            max_depth: 8,
            max_total: 1_000_000,
            max_ratio: 1_000,
            max_single_read: 1024,
        }
    }

    fn fixture() -> (PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "superexplorer-7z-test-{}-{}",
            std::process::id(),
            FIXTURE_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("source/nested")).unwrap();
        fs::write(root.join("source/nested/hello.txt"), b"hello archive").unwrap();
        let archive = root.join("sample.7z");
        sevenz_rust::compress_to_path(root.join("source"), &archive).unwrap();
        (root, archive)
    }

    #[test]
    fn rejects_traversal_collision_and_bombs() {
        assert_eq!(
            format!("{:?}", ArchiveSecret::new("never-log-me")),
            "ArchiveSecret([redacted])"
        );
        assert!(normalize_entry("../../x").is_err());
        let mut p = policy();
        p.max_ratio = 10;
        let entry = |path: &str, size, compressed| VirtualEntry {
            id: 1,
            path: path.into(),
            size,
            compressed_size: compressed,
            crc: None,
            encrypted: false,
            is_directory: false,
        };
        assert!(validate_entries(&[entry("A.txt", 1, 1), entry("a.TXT", 1, 1)], p).is_err());
        assert!(validate_entries(&[entry("bomb", 100, 1)], p).is_err());
    }

    #[test]
    fn real_archive_enumerates_reads_and_extracts() {
        let (root, archive) = fixture();
        let entries = enumerate_archive(&archive, policy()).unwrap();
        let name = entries
            .iter()
            .find(|entry| entry.path.ends_with("hello.txt"))
            .unwrap()
            .path
            .clone();
        assert_eq!(
            read_entry(&archive, &name, policy()).unwrap(),
            b"hello archive"
        );
        let output = root.join("output");
        let files = extract_archive(&archive, &output, policy()).unwrap();
        assert_eq!(fs::read(&files[0]).unwrap(), b"hello archive");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn mutation_repackages_and_preserves_whole_container_undo() {
        let (root, archive) = fixture();
        let original = fs::read(&archive).unwrap();
        let backup = mutate_archive(&archive, policy(), |work| {
            fs::write(work.join("added.txt"), b"new").map_err(|error| error.to_string())
        })
        .unwrap();
        assert_eq!(fs::read(&backup).unwrap(), original);
        let entries = enumerate_archive(&archive, policy()).unwrap();
        assert!(entries
            .iter()
            .any(|entry| entry.path.ends_with("added.txt")));
        fs::remove_dir_all(root).unwrap();
    }
}
