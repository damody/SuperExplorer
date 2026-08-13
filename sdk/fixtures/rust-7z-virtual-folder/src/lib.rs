//! Pure-Rust 7z virtual-folder example with bounded reads and transactional mutation.

use abi_stable::{
    export_root_module,
    prefix_type::PrefixTypeTrait,
    std_types::{ROption, RResult, RString, RVec},
};
use explorer_extension_api::*;
use sevenz_rust::{
    AesEncoderOptions, Archive, Error as SevenZError, Password, SevenZMethod, SevenZReader,
    SevenZWriter,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    hint::black_box,
    io::{Read, Seek, SeekFrom, Write},
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const PLUGIN_ID: StableIdV1 = StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 8_101);
const INTERFACE_ID: StableIdV1 = StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 8_102);

struct HostStreamReader {
    stream: InputStreamV1,
    source_generation: u64,
}

impl Read for HostStreamReader {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        let outcome = self.stream.read(InputStreamReadRequestV1 {
            maximum_bytes: output.len().min(MAX_INPUT_STREAM_READ_BYTES_V1 as usize) as u32,
            reserved: 0,
        });
        if outcome.source_generation != self.source_generation {
            return Err(std::io::Error::other("stale archive stream"));
        }
        if outcome.status == InputStreamStatusV1::EOF {
            return Ok(0);
        }
        if outcome.status != InputStreamStatusV1::OK || outcome.data.len() > output.len() {
            return Err(std::io::Error::other("archive stream read failed"));
        }
        output[..outcome.data.len()].copy_from_slice(&outcome.data);
        Ok(outcome.data.len())
    }
}

impl Seek for HostStreamReader {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        let (origin, offset) = match position {
            SeekFrom::Start(value) => (
                InputStreamSeekOriginV1::START,
                i64::try_from(value).map_err(|_| std::io::Error::other("seek overflow"))?,
            ),
            SeekFrom::Current(value) => (InputStreamSeekOriginV1::CURRENT, value),
            SeekFrom::End(value) => (InputStreamSeekOriginV1::END, value),
        };
        let outcome = self.stream.seek(InputStreamSeekRequestV1 {
            origin,
            reserved: 0,
            offset,
        });
        if outcome.source_generation != self.source_generation {
            return Err(std::io::Error::other("stale archive stream"));
        }
        if outcome.status != InputStreamStatusV1::OK {
            return Err(std::io::Error::other("archive stream seek failed"));
        }
        Ok(outcome.position)
    }
}

struct HostStreamWriter {
    stream: VirtualOutputStreamV1,
    position: u64,
}

impl Write for HostStreamWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        let count = bytes.len().min(MAX_VIRTUAL_WRITE_BYTES_V1);
        let outcome = self.stream.write(bytes[..count].to_vec().into());
        if outcome.status != VirtualOutputStatusV1::OK || outcome.position < self.position {
            return Err(std::io::Error::other("staging write failed"));
        }
        let written = usize::try_from(outcome.position - self.position)
            .map_err(|_| std::io::Error::other("staging write overflow"))?;
        if written == 0 || written > count {
            return Err(std::io::Error::other("invalid staging write count"));
        }
        self.position = outcome.position;
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let outcome = self.stream.flush();
        if outcome.status != VirtualOutputStatusV1::OK || outcome.position != self.position {
            return Err(std::io::Error::other("staging flush failed"));
        }
        Ok(())
    }
}

impl Seek for HostStreamWriter {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        let (origin, offset) = match position {
            SeekFrom::Start(value) => (
                InputStreamSeekOriginV1::START,
                i64::try_from(value).map_err(|_| std::io::Error::other("seek overflow"))?,
            ),
            SeekFrom::Current(value) => (InputStreamSeekOriginV1::CURRENT, value),
            SeekFrom::End(value) => (InputStreamSeekOriginV1::END, value),
        };
        let outcome = self.stream.seek(InputStreamSeekRequestV1 {
            origin,
            reserved: 0,
            offset,
        });
        if outcome.status != VirtualOutputStatusV1::OK {
            return Err(std::io::Error::other("staging seek failed"));
        }
        self.position = outcome.position;
        Ok(outcome.position)
    }
}

fn open_host_archive(
    stream: InputStreamV1,
    source_generation: u64,
    secret: ROption<VirtualSecretV1>,
) -> Result<(SevenZReader<HostStreamReader>, Option<Password>), VirtualProviderStatusV1> {
    let length = stream.length();
    if length.source_generation != source_generation {
        return Err(VirtualProviderStatusV1::STALE);
    }
    if length.status != InputStreamStatusV1::OK {
        return Err(VirtualProviderStatusV1::UNSUPPORTED);
    }
    let supplied = secret.is_some();
    let mut utf16 = match secret {
        ROption::RSome(secret) => {
            let material = secret.take();
            if material.status != VirtualSecretStatusV1::READY || material.utf16.is_empty() {
                return Err(VirtualProviderStatusV1::FAILED);
            }
            material.utf16.into_vec()
        }
        ROption::RNone => Vec::new(),
    };
    let password = if supplied {
        Some(Password::from(utf16.as_slice()))
    } else {
        None
    };
    let opened = SevenZReader::new(
        HostStreamReader {
            stream,
            source_generation,
        },
        length.length,
        password.clone().unwrap_or_else(Password::empty),
    );
    utf16.fill(0);
    black_box(&mut utf16);
    opened
        .map(|reader| (reader, password))
        .map_err(|_| {
            if supplied {
                VirtualProviderStatusV1::FAILED
            } else {
                VirtualProviderStatusV1::PASSWORD_REQUIRED
            }
        })
}

struct SevenZVirtualFolderProvider;

fn apply_mutation_name(
    name: &str,
    changes: &BTreeMap<String, Option<String>>,
) -> Option<String> {
    let matched = changes
        .iter()
        .filter(|(source, _)| name == source.as_str() || name.starts_with(&format!("{source}/")))
        .max_by_key(|(source, _)| source.len());
    let Some((source, destination)) = matched else {
        return Some(name.to_owned());
    };
    let destination = destination.as_ref()?;
    let suffix = &name[source.len()..];
    Some(format!("{destination}{suffix}"))
}

impl VirtualFolderProviderImplementationV1 for SevenZVirtualFolderProvider {
    fn enumerate(&self, request: VirtualEnumerateRequestV1) -> VirtualEnumerationOutcomeV1 {
        let reader = match open_host_archive(
            request.container.clone(),
            request.source_generation,
            request.secret.clone(),
        ) {
            Ok((reader, _)) => reader,
            Err(status) => return VirtualEnumerationOutcomeV1::terminal(status, &request),
        };
        let parent = request
            .parent_components
            .iter()
            .map(RString::as_str)
            .collect::<Vec<_>>();
        let mut entries = Vec::new();
        let archive_encrypted = reader.archive().folders.iter().any(|folder| {
            folder.coders.iter().any(|coder| {
                coder.decompression_method_id() == SevenZMethod::ID_AES256SHA256
            })
        });
        for (index, entry) in reader.archive().files.iter().enumerate() {
            if entry.is_directory && entry.name.is_empty() {
                continue;
            }
            let Ok(normalized) = normalize_entry(&entry.name) else {
                continue;
            };
            let components = normalized.split('/').collect::<Vec<_>>();
            if components.len() != parent.len() + 1 || components[..parent.len()] != parent[..] {
                continue;
            }
            entries.push(VirtualEntryV1 {
                id: StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, index as u64 + 1),
                name: components.last().copied().unwrap_or_default().into(),
                components: components
                    .into_iter()
                    .map(RString::from)
                    .collect::<Vec<_>>()
                    .into(),
                kind: if entry.is_directory {
                    VirtualEntryKindV1::DIRECTORY
                } else {
                    VirtualEntryKindV1::FILE
                },
                uncompressed_size: entry.size,
                compressed_size: entry.compressed_size,
                crc32: if entry.has_crc {
                    ROption::RSome(entry.crc as u32)
                } else {
                    ROption::RNone
                },
                modified_unix_seconds: if entry.has_last_modified_date {
                    SystemTime::from(entry.last_modified_date)
                        .duration_since(UNIX_EPOCH)
                        .ok()
                        .and_then(|duration| i64::try_from(duration.as_secs()).ok())
                        .map_or(ROption::RNone, ROption::RSome)
                } else {
                    ROption::RNone
                },
                encrypted: archive_encrypted,
                allowed_operations: VirtualAllowedOperationsV1::from_bits(
                    VirtualAllowedOperationsV1::READ.bits()
                        | VirtualAllowedOperationsV1::EXTRACT.bits()
                        | VirtualAllowedOperationsV1::DELETE.bits()
                        | VirtualAllowedOperationsV1::RENAME.bits()
                        | VirtualAllowedOperationsV1::MOVE.bits(),
                ),
            });
            if entries.len() >= request.maximum_entries as usize {
                break;
            }
        }
        VirtualEnumerationOutcomeV1 {
            status: VirtualProviderStatusV1::READY,
            reserved: 0,
            container_generation: request.container_generation,
            source_generation: request.source_generation,
            entries: RVec::from(entries),
        }
    }

    fn read(&self, request: VirtualReadRequestV1) -> VirtualReadOutcomeV1 {
        let wanted = request
            .entry_id
            .value
            .checked_sub(1)
            .map(|value| value as usize);
        let Some(wanted) = wanted else {
            return VirtualReadOutcomeV1::terminal(VirtualProviderStatusV1::INVALID, &request);
        };
        let mut reader = match open_host_archive(
            request.container.clone(),
            request.source_generation,
            request.secret.clone(),
        ) {
            Ok((reader, _)) => reader,
                Err(status) => return VirtualReadOutcomeV1::terminal(status, &request),
        };
        let Some(wanted_entry) = reader.archive().files.get(wanted) else {
            return VirtualReadOutcomeV1::terminal(VirtualProviderStatusV1::INVALID, &request);
        };
        if wanted_entry.is_directory {
            return VirtualReadOutcomeV1::terminal(VirtualProviderStatusV1::INVALID, &request);
        }
        let Ok(wanted_name) = normalize_entry(&wanted_entry.name) else {
            return VirtualReadOutcomeV1::terminal(VirtualProviderStatusV1::INVALID, &request);
        };
        let mut result = None;
        let decode = reader.for_each_entries(|entry, source| {
            if normalize_entry(entry.name()).ok().as_deref() != Some(wanted_name.as_str()) {
                return Ok(true);
            }
            if entry.is_directory {
                return Err(SevenZError::other("cannot read directory"));
            }
            let mut skipped = source.take(request.offset);
            std::io::copy(&mut skipped, &mut std::io::sink()).map_err(SevenZError::io)?;
            let mut bytes = Vec::with_capacity(request.maximum_bytes as usize);
            source
                .take(request.maximum_bytes as u64)
                .read_to_end(&mut bytes)
                .map_err(SevenZError::io)?;
            let next = request.offset.saturating_add(bytes.len() as u64);
            result = Some((bytes, next >= entry.size));
            Ok(false)
        });
        if decode.is_err() {
            return VirtualReadOutcomeV1::terminal(VirtualProviderStatusV1::FAILED, &request);
        }
        let Some((bytes, end_of_entry)) = result else {
            return VirtualReadOutcomeV1::terminal(VirtualProviderStatusV1::INVALID, &request);
        };
        VirtualReadOutcomeV1 {
            status: VirtualProviderStatusV1::READY,
            reserved: 0,
            container_generation: request.container_generation,
            source_generation: request.source_generation,
            next_offset: request.offset + bytes.len() as u64,
            end_of_entry,
            bytes: RVec::from(bytes),
        }
    }

    fn mutate(&self, request: VirtualMutationRequestV1) -> VirtualMutationOutcomeV1 {
        let (mut reader, encryption_password) = match open_host_archive(
            request.container.clone(),
            request.source_generation,
            request.secret.clone(),
        ) {
                Ok(reader) => reader,
                Err(status) => return VirtualMutationOutcomeV1::terminal(status, &request),
        };
        let mut changes = BTreeMap::<String, Option<String>>::new();
        let mut additions = Vec::<(String, bool, ROption<InputStreamV1>, u64)>::new();
        for step in &request.steps {
            if step.kind == VirtualMutationKindV1::ADD_FILE
                || step.kind == VirtualMutationKindV1::CREATE_DIRECTORY
            {
                let candidate = step
                    .destination_components
                    .iter()
                    .map(RString::as_str)
                    .collect::<Vec<_>>()
                    .join("/");
                let destination = match normalize_entry(&candidate) {
                    Ok(destination) => destination,
                    Err(_) => {
                        return VirtualMutationOutcomeV1::terminal(
                            VirtualProviderStatusV1::INVALID,
                            &request,
                        );
                    }
                };
                let is_directory = step.kind == VirtualMutationKindV1::CREATE_DIRECTORY;
                if is_directory && step.source.is_some() {
                    return VirtualMutationOutcomeV1::terminal(
                        VirtualProviderStatusV1::INVALID,
                        &request,
                    );
                }
                additions.push((
                    destination,
                    is_directory,
                    step.source.clone(),
                    step.source_generation,
                ));
                continue;
            }
            let Some(index) = step
                .entry_id
                .value
                .checked_sub(1)
                .map(|value| value as usize)
            else {
                return VirtualMutationOutcomeV1::terminal(
                    VirtualProviderStatusV1::INVALID,
                    &request,
                );
            };
            let Some(entry) = reader.archive().files.get(index) else {
                return VirtualMutationOutcomeV1::terminal(
                    VirtualProviderStatusV1::INVALID,
                    &request,
                );
            };
            let Ok(source_name) = normalize_entry(&entry.name) else {
                return VirtualMutationOutcomeV1::terminal(
                    VirtualProviderStatusV1::INVALID,
                    &request,
                );
            };
            let destination = if step.kind == VirtualMutationKindV1::DELETE {
                if !step.destination_components.is_empty() {
                    return VirtualMutationOutcomeV1::terminal(
                        VirtualProviderStatusV1::INVALID,
                        &request,
                    );
                }
                None
            } else if step.kind == VirtualMutationKindV1::RENAME
                || step.kind == VirtualMutationKindV1::MOVE
            {
                let candidate = step
                    .destination_components
                    .iter()
                    .map(RString::as_str)
                    .collect::<Vec<_>>()
                    .join("/");
                match normalize_entry(&candidate) {
                    Ok(destination) => Some(destination),
                    Err(_) => {
                        return VirtualMutationOutcomeV1::terminal(
                            VirtualProviderStatusV1::INVALID,
                            &request,
                        );
                    }
                }
            } else {
                return VirtualMutationOutcomeV1::terminal(
                    VirtualProviderStatusV1::INVALID,
                    &request,
                );
            };
            if changes.insert(source_name, destination).is_some() {
                return VirtualMutationOutcomeV1::terminal(
                    VirtualProviderStatusV1::INVALID,
                    &request,
                );
            }
        }
        let mut final_names = BTreeSet::new();
        for entry in &reader.archive().files {
            let Ok(name) = normalize_entry(&entry.name) else {
                continue;
            };
            let Some(name) = apply_mutation_name(&name, &changes) else {
                continue;
            };
            let folded = name.to_lowercase();
            if !final_names.insert(folded) {
                return VirtualMutationOutcomeV1::terminal(
                    VirtualProviderStatusV1::INVALID,
                    &request,
                );
            }
        }
        for (name, _, _, _) in &additions {
            if !final_names.insert(name.to_lowercase()) {
                return VirtualMutationOutcomeV1::terminal(
                    VirtualProviderStatusV1::INVALID,
                    &request,
                );
            }
        }
        let writer = HostStreamWriter {
            stream: request.staging.clone(),
            position: 0,
        };
        let mut writer = match SevenZWriter::new(writer) {
            Ok(writer) => writer,
            Err(_) => {
                return VirtualMutationOutcomeV1::terminal(
                    VirtualProviderStatusV1::FAILED,
                    &request,
                );
            }
        };
        if let Some(password) = encryption_password {
            writer.set_content_methods(vec![
                AesEncoderOptions::new(password).into(),
                SevenZMethod::LZMA2.into(),
            ]);
        }
        let mut written_entries = 0_u32;
        let rebuild = reader.for_each_entries(|entry, source| {
            if entry.is_directory && entry.name.is_empty() {
                return Ok(true);
            }
            let normalized = normalize_entry(entry.name())
                .map_err(|error| SevenZError::other(error.to_owned()))?;
            let Some(destination) = apply_mutation_name(&normalized, &changes) else {
                return Ok(true);
            };
            // Start from a writer-owned entry so decoder method descriptors
            // from the source archive never leak into the encoder.
            let mut rebuilt = sevenz_rust::SevenZArchiveEntry::new();
            rebuilt.name = destination;
            rebuilt.has_stream = entry.has_stream;
            rebuilt.is_directory = entry.is_directory;
            rebuilt.is_anti_item = entry.is_anti_item;
            rebuilt.has_creation_date = entry.has_creation_date;
            rebuilt.has_last_modified_date = entry.has_last_modified_date;
            rebuilt.has_access_date = entry.has_access_date;
            rebuilt.creation_date = entry.creation_date;
            rebuilt.last_modified_date = entry.last_modified_date;
            rebuilt.access_date = entry.access_date;
            rebuilt.has_windows_attributes = entry.has_windows_attributes;
            rebuilt.windows_attributes = entry.windows_attributes;
            if rebuilt.is_directory {
                writer.push_archive_entry::<std::io::Empty>(rebuilt, None)?;
            } else {
                if entry.size > 64 * 1024 * 1024 {
                    return Err(SevenZError::other("entry exceeds mutation memory quota"));
                }
                let mut bytes = Vec::with_capacity(entry.size as usize);
                source
                    .read_to_end(&mut bytes)
                    .map_err(|error| SevenZError::other(format!("source read: {error}")))?;
                if bytes.len() as u64 != entry.size {
                    return Err(SevenZError::other("decoded entry size mismatch"));
                }
                writer
                    .push_archive_entry(rebuilt, Some(std::io::Cursor::new(bytes)))
                    .map_err(|error| SevenZError::other(format!("writer push: {error}")))?;
            }
            written_entries = written_entries.saturating_add(1);
            Ok(true)
        });
        if rebuild.is_err() {
            return VirtualMutationOutcomeV1::terminal(VirtualProviderStatusV1::FAILED, &request);
        }
        for (name, is_directory, source, source_generation) in additions {
            let mut entry = sevenz_rust::SevenZArchiveEntry::new();
            entry.name = name;
            entry.is_directory = is_directory;
            entry.has_stream = !is_directory;
            if is_directory {
                if writer
                    .push_archive_entry::<std::io::Empty>(entry, None)
                    .is_err()
                {
                    return VirtualMutationOutcomeV1::terminal(
                        VirtualProviderStatusV1::FAILED,
                        &request,
                    );
                }
            } else {
                let bytes = match source {
                    ROption::RSome(source) => {
                        let length = source.length();
                        if length.status != InputStreamStatusV1::OK
                            || length.source_generation != source_generation
                            || length.length > 64 * 1024 * 1024
                        {
                            return VirtualMutationOutcomeV1::terminal(
                                VirtualProviderStatusV1::RESOURCE_LIMITED,
                                &request,
                            );
                        }
                        let mut bytes = Vec::with_capacity(length.length as usize);
                        let mut source = HostStreamReader {
                            stream: source,
                            source_generation,
                        };
                        if source.read_to_end(&mut bytes).is_err()
                            || bytes.len() as u64 != length.length
                        {
                            return VirtualMutationOutcomeV1::terminal(
                                VirtualProviderStatusV1::FAILED,
                                &request,
                            );
                        }
                        bytes
                    }
                    ROption::RNone => Vec::new(),
                };
                if writer
                    .push_archive_entry(entry, Some(std::io::Cursor::new(bytes)))
                    .is_err()
                {
                    return VirtualMutationOutcomeV1::terminal(
                        VirtualProviderStatusV1::FAILED,
                        &request,
                    );
                }
            }
            written_entries = written_entries.saturating_add(1);
        }
        if writer
            .finish()
            .and_then(|mut output| output.flush())
            .is_err()
        {
            return VirtualMutationOutcomeV1::terminal(VirtualProviderStatusV1::FAILED, &request);
        }
        VirtualMutationOutcomeV1 {
            status: VirtualProviderStatusV1::READY,
            reserved: 0,
            container_generation: request.container_generation,
            source_generation: request.source_generation,
            written_entries,
            reserved_tail: 0,
        }
    }
}

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

/// Reopens and fully decodes every file so header, inventory, sizes, and CRCs
/// are proven before a staged container may replace the original.
pub fn verify_archive_contents(
    archive_path: &Path,
    policy: ResourcePolicy,
) -> Result<Vec<VirtualEntry>, String> {
    let entries = enumerate_archive(archive_path, policy)?;
    let mut reader =
        SevenZReader::open(archive_path, Password::empty()).map_err(|error| error.to_string())?;
    let mut decoded = BTreeSet::new();
    reader
        .for_each_entries(|entry, source| {
            if entry.is_directory() {
                return Ok(true);
            }
            let normalized = normalize_entry(entry.name()).map_err(SevenZError::other)?;
            let expected = entries
                .iter()
                .find(|candidate| candidate.path == normalized && !candidate.is_directory)
                .ok_or_else(|| SevenZError::other("decoded entry missing from inventory"))?;
            let mut limited = source.take(expected.size.saturating_add(1));
            let copied =
                std::io::copy(&mut limited, &mut std::io::sink()).map_err(SevenZError::io)?;
            if copied != expected.size {
                return Err(SevenZError::other("decoded entry size mismatch"));
            }
            if !decoded.insert(normalized) {
                return Err(SevenZError::other("decoded entry collision"));
            }
            Ok(true)
        })
        .map_err(|error| error.to_string())?;
    if decoded.len() != entries.iter().filter(|entry| !entry.is_directory).count() {
        return Err("decoded inventory mismatch".to_owned());
    }
    Ok(entries)
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ContainerIdentity {
    length: u64,
    modified: u64,
    volume: u64,
    file: u64,
}

#[cfg(windows)]
fn container_identity(path: &Path) -> Result<ContainerIdentity, String> {
    use std::os::windows::io::AsRawHandle as _;
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
    };

    let file = File::open(path).map_err(|error| error.to_string())?;
    // SAFETY: this all-zero value is only the output buffer for the immediate
    // Win32 query and contains no owned resource.
    let mut information: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: `file` keeps the valid handle live and `information` is writable
    // for the duration of this synchronous call.
    if unsafe { GetFileInformationByHandle(file.as_raw_handle() as _, &mut information) } == 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(ContainerIdentity {
        length: (u64::from(information.nFileSizeHigh) << 32) | u64::from(information.nFileSizeLow),
        modified: (u64::from(information.ftLastWriteTime.dwHighDateTime) << 32)
            | u64::from(information.ftLastWriteTime.dwLowDateTime),
        volume: u64::from(information.dwVolumeSerialNumber),
        file: (u64::from(information.nFileIndexHigh) << 32) | u64::from(information.nFileIndexLow),
    })
}

#[cfg(not(windows))]
fn container_identity(path: &Path) -> Result<ContainerIdentity, String> {
    use std::os::unix::fs::MetadataExt as _;
    let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
    Ok(ContainerIdentity {
        length: metadata.len(),
        modified: metadata.mtime_nsec() as u64,
        volume: metadata.dev(),
        file: metadata.ino(),
    })
}

#[cfg(windows)]
fn atomic_replace_with_backup(
    destination: &Path,
    replacement: &Path,
    backup: &Path,
) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt as _;
    use windows_sys::Win32::Storage::FileSystem::{REPLACEFILE_WRITE_THROUGH, ReplaceFileW};

    let wide = |path: &Path| {
        path.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>()
    };
    let destination = wide(destination);
    let replacement = wide(replacement);
    let backup = wide(backup);
    // SAFETY: all three buffers are live, NUL-terminated Windows paths for
    // the duration of this synchronous call; no pointer is retained.
    let replaced = unsafe {
        ReplaceFileW(
            destination.as_ptr(),
            replacement.as_ptr(),
            backup.as_ptr(),
            REPLACEFILE_WRITE_THROUGH,
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    if replaced == 0 {
        Err(std::io::Error::last_os_error().to_string())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn atomic_replace_with_backup(
    destination: &Path,
    replacement: &Path,
    backup: &Path,
) -> Result<(), String> {
    fs::rename(destination, backup).map_err(|error| error.to_string())?;
    if let Err(error) = fs::rename(replacement, destination) {
        let _ = fs::rename(backup, destination);
        return Err(error.to_string());
    }
    Ok(())
}

/// Extracts, mutates, repacks, verifies, rechecks the original, then atomically replaces it.
/// The returned sibling backup is the conservative whole-container undo token.
pub fn mutate_archive(
    container: &Path,
    policy: ResourcePolicy,
    mutation: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<PathBuf, String> {
    let original_identity = container_identity(container)?;
    let original = fs::read(container).map_err(|error| error.to_string())?;
    let work = sibling_work_dir(container)?;
    fs::create_dir(&work).map_err(|error| error.to_string())?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let staging = container.with_extension(format!("7z.superexplorer-stage-{nonce}"));
    let result = (|| {
        extract_archive(container, &work, policy)?;
        mutation(&work)?;
        sevenz_rust::compress_to_path(&work, &staging).map_err(|error| error.to_string())?;
        verify_archive_contents(&staging, policy)?;
        if container_identity(container)? != original_identity
            || fs::read(container).map_err(|error| error.to_string())? != original
        {
            return Err("original changed".to_owned());
        }
        let backup = container.with_extension(format!("7z.superexplorer-undo-{nonce}"));
        if backup.exists() {
            return Err("undo backup already exists".to_owned());
        }
        atomic_replace_with_backup(container, &staging, &backup)?;
        Ok(backup)
    })();
    let _ = fs::remove_dir_all(&work);
    if result.is_err() {
        let _ = fs::remove_file(&staging);
    }
    result
}

/// Restores a whole-container backup conservatively and leaves the current
/// container untouched if either preflight or the replacement step fails.
pub fn undo_archive(container: &Path, backup: &Path, policy: ResourcePolicy) -> Result<(), String> {
    verify_archive_contents(container, policy)?;
    verify_archive_contents(backup, policy)?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let displaced = container.with_extension(format!("7z.superexplorer-displaced-{nonce}"));
    atomic_replace_with_backup(container, backup, &displaced)?;
    fs::remove_file(displaced).map_err(|error| error.to_string())
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
                    folder_admission: ROption::RNone,
                    provider: ROption::RNone,
                    visual_column: ROption::RNone,
                    size_map_view: ROption::RNone,
                    virtual_folder_provider: if kind == RegisteredContributionKindV1::RESOURCE {
                        ROption::RSome(VirtualFolderProviderObjectV1::new(
                            SevenZVirtualFolderProvider,
                        ))
                    } else {
                        ROption::RNone
                    },
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
        if let Some(output) = std::env::var_os("SUPEREXPLORER_7Z_SMOKE_OUTPUT") {
            if let Ok(password) = std::env::var("SUPEREXPLORER_7Z_SMOKE_PASSWORD") {
                sevenz_rust::compress_to_path_encrypted(
                    root.join("source"),
                    output,
                    Password::from(password.as_str()),
                )
                .unwrap();
            } else {
                fs::copy(&archive, output).unwrap();
            }
        }
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
        assert!(
            entries
                .iter()
                .any(|entry| entry.path.ends_with("added.txt"))
        );
        undo_archive(&archive, &backup, policy()).unwrap();
        assert_eq!(fs::read(&archive).unwrap(), original);
        assert!(!backup.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn mutation_rejects_original_identity_race_and_cleans_staging() {
        let (root, archive) = fixture();
        let result = mutate_archive(&archive, policy(), |work| {
            fs::write(work.join("added.txt"), b"new").map_err(|error| error.to_string())?;
            fs::write(&archive, b"external replacement").map_err(|error| error.to_string())
        });
        assert_eq!(result.unwrap_err(), "original changed");
        assert_eq!(fs::read(&archive).unwrap(), b"external replacement");
        assert!(!fs::read_dir(&root).unwrap().any(|entry| {
            entry
                .ok()
                .and_then(|entry| entry.file_name().into_string().ok())
                .is_some_and(|name| name.contains("superexplorer-stage"))
        }));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn archive_corpus_covers_unicode_empty_deep_solid_aes_and_corruption() {
        let (root, _) = fixture();
        fs::write(root.join("source/空白.txt"), []).unwrap();
        fs::create_dir_all(root.join("source/a/b/c/d/e/f/g/h/i")).unwrap();
        fs::write(root.join("source/a/b/c/d/e/f/g/h/i/deep.txt"), b"deep").unwrap();

        let solid = root.join("solid.7z");
        let mut writer = SevenZWriter::create(&solid).unwrap();
        writer.push_source_path(root.join("source"), |_| true).unwrap();
        writer.finish().unwrap();
        let mut permissive = policy();
        permissive.max_depth = 16;
        let solid_entries = enumerate_archive(&solid, permissive).unwrap();
        assert!(solid_entries.iter().any(|entry| entry.path.ends_with("空白.txt")));
        assert_eq!(
            read_entry(&solid, "空白.txt", permissive).unwrap(),
            Vec::<u8>::new()
        );
        assert!(enumerate_archive(&solid, policy()).is_err());

        let encrypted = root.join("aes.7z");
        sevenz_rust::compress_to_path_encrypted(
            root.join("source"),
            &encrypted,
            Password::from("corpus-secret"),
        )
        .unwrap();
        assert!(enumerate_archive(&encrypted, permissive).is_err());
        let secret = ArchiveSecret::new("corpus-secret");
        assert!(enumerate_archive_with_secret(&encrypted, permissive, Some(&secret)).is_ok());
        assert!(enumerate_archive_with_secret(
            &encrypted,
            permissive,
            Some(&ArchiveSecret::new("wrong")),
        )
        .is_err());

        let traversal = root.join("traversal.7z");
        let mut writer = SevenZWriter::create(&traversal).unwrap();
        let mut entry = sevenz_rust::SevenZArchiveEntry::new();
        entry.name = "../escape.txt".to_owned();
        writer
            .push_archive_entry(entry, Some(std::io::Cursor::new(b"escape")))
            .unwrap();
        writer.finish().unwrap();
        assert!(enumerate_archive(&traversal, permissive).is_err());

        let corrupt = root.join("corrupt.7z");
        fs::copy(&solid, &corrupt).unwrap();
        let mut bytes = fs::read(&corrupt).unwrap();
        let middle = bytes.len() / 2;
        bytes[middle] ^= 0x5a;
        fs::write(&corrupt, bytes).unwrap();
        assert!(verify_archive_contents(&corrupt, permissive).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
