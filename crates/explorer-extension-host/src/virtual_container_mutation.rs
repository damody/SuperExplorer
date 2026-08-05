//! Transactional host commit/undo for plugin-produced virtual-container staging.

use std::{
    collections::VecDeque,
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use abi_stable::std_types::{ROption, RVec};
use explorer_extension_api::{
    MAX_VIRTUAL_READ_BYTES_V1, VirtualEnumerateRequestV1, VirtualMutationRequestV1,
    VirtualMutationStepV1, VirtualProviderStatusV1, VirtualReadRequestV1,
};
use sha2::{Digest, Sha256};

use crate::{
    SinglePluginVirtualFolderRuntimeV1, create_virtual_container_staging_v1,
    open_virtual_container_input_with_cancellation_v1,
};

const VERIFY_MAX_ENTRIES_V1: usize = 16_384;
const VERIFY_MAX_TOTAL_BYTES_V1: u64 = 2 * 1024 * 1024 * 1024;

struct WipeSecretUtf16V1(Option<Vec<u16>>);

impl WipeSecretUtf16V1 {
    fn as_deref(&self) -> Option<&[u16]> {
        self.0.as_deref()
    }
}

impl Drop for WipeSecretUtf16V1 {
    fn drop(&mut self) {
        if let Some(secret) = self.0.as_mut() {
            secret.fill(0);
            std::hint::black_box(secret);
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ContainerPreimageV1 {
    length: u64,
    modified_nanos: u128,
    sha256: [u8; 32],
}

fn container_preimage(path: &Path) -> Result<ContainerPreimageV1, String> {
    let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
    let modified_nanos = metadata
        .modified()
        .map_err(|error| error.to_string())?
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let mut file = File::open(path).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(ContainerPreimageV1 {
        length: metadata.len(),
        modified_nanos,
        sha256: hasher.finalize().into(),
    })
}

fn verify_rebuilt_archive(
    runtime: &SinglePluginVirtualFolderRuntimeV1,
    contribution_id: &str,
    path: &Path,
    generation: u64,
    cancellation: Option<explorer_model::CancellationToken>,
    secret_utf16: Option<&[u16]>,
) -> Result<(), String> {
    let mut parents = VecDeque::from([Vec::<String>::new()]);
    let mut entries = 0_usize;
    let mut total_bytes = 0_u64;
    while let Some(parent) = parents.pop_front() {
        let outcome = runtime
            .enumerate(
                contribution_id,
                VirtualEnumerateRequestV1 {
                    container: open_virtual_container_input_with_cancellation_v1(
                        path,
                        generation,
                        cancellation.clone(),
                    )
                    .map_err(|error| error.to_string())?,
                    container_generation: generation,
                    source_generation: generation,
                    parent_components: parent
                        .into_iter()
                        .map(Into::into)
                        .collect::<Vec<_>>()
                        .into(),
                    maximum_entries: explorer_extension_api::MAX_VIRTUAL_ENTRIES_V1 as u32,
                    reserved: 0,
                    secret: mint_secret(secret_utf16),
                },
            )
            .map_err(|error| error.to_string())?;
        if outcome.status != VirtualProviderStatusV1::READY {
            return Err("staging header or inventory verification failed".to_owned());
        }
        for entry in outcome.entries {
            entries = entries.saturating_add(1);
            if entries > VERIFY_MAX_ENTRIES_V1 {
                return Err("staging entry quota exceeded".to_owned());
            }
            if entry.kind == explorer_extension_api::VirtualEntryKindV1::DIRECTORY {
                parents.push_back(
                    entry
                        .components
                        .into_iter()
                        .map(|value| value.into_string())
                        .collect(),
                );
                continue;
            }
            total_bytes = total_bytes
                .checked_add(entry.uncompressed_size)
                .ok_or_else(|| "staging size overflow".to_owned())?;
            if total_bytes > VERIFY_MAX_TOTAL_BYTES_V1 {
                return Err("staging total quota exceeded".to_owned());
            }
            let mut offset = 0_u64;
            while offset < entry.uncompressed_size {
                let read = runtime
                    .read(
                        contribution_id,
                        VirtualReadRequestV1 {
                            container: open_virtual_container_input_with_cancellation_v1(
                                path,
                                generation,
                                cancellation.clone(),
                            )
                            .map_err(|error| error.to_string())?,
                            container_generation: generation,
                            source_generation: generation,
                            entry_id: entry.id,
                            offset,
                            maximum_bytes: MAX_VIRTUAL_READ_BYTES_V1 as u32,
                            reserved: 0,
                            secret: mint_secret(secret_utf16),
                        },
                    )
                    .map_err(|error| error.to_string())?;
                if read.status != VirtualProviderStatusV1::READY
                    || read.next_offset <= offset
                    || read.bytes.is_empty()
                {
                    return Err("staging CRC or content verification failed".to_owned());
                }
                offset = read.next_offset;
                if read.end_of_entry {
                    break;
                }
            }
            if offset != entry.uncompressed_size {
                return Err("staging entry size verification failed".to_owned());
            }
        }
    }
    Ok(())
}

fn mint_secret(secret_utf16: Option<&[u16]>) -> ROption<explorer_extension_api::VirtualSecretV1> {
    secret_utf16
        .and_then(|secret| crate::mint_virtual_secret_v1(secret.to_vec()))
        .map(ROption::RSome)
        .unwrap_or(ROption::RNone)
}

#[cfg(windows)]
fn atomic_replace_with_backup(
    replacement: &Path,
    destination: &Path,
    backup: &Path,
) -> Result<(), String> {
    use std::{iter, os::windows::ffi::OsStrExt as _};
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn ReplaceFileW(
            replaced: *const u16,
            replacement: *const u16,
            backup: *const u16,
            flags: u32,
            exclude: *mut core::ffi::c_void,
            reserved: *mut core::ffi::c_void,
        ) -> i32;
    }
    let wide = |path: &Path| {
        path.as_os_str()
            .encode_wide()
            .chain(iter::once(0))
            .collect::<Vec<_>>()
    };
    let destination = wide(destination);
    let replacement = wide(replacement);
    let backup = wide(backup);
    if unsafe {
        ReplaceFileW(
            destination.as_ptr(),
            replacement.as_ptr(),
            backup.as_ptr(),
            0x1,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    } == 0
    {
        Err(std::io::Error::last_os_error().to_string())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn atomic_replace_with_backup(
    replacement: &Path,
    destination: &Path,
    backup: &Path,
) -> Result<(), String> {
    fs::rename(destination, backup).map_err(|error| error.to_string())?;
    if let Err(error) = fs::rename(replacement, destination) {
        let _ = fs::rename(backup, destination);
        return Err(error.to_string());
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VirtualMutationCommitV1 {
    pub backup: PathBuf,
    pub new_generation: u64,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VirtualMutationFaultV1 {
    Rebuild,
    Flush,
    Reopen,
    Header,
    Inventory,
    Crc,
}

pub fn commit_virtual_container_mutation_v1(
    runtime: &SinglePluginVirtualFolderRuntimeV1,
    contribution_id: &str,
    container: &Path,
    generation: u64,
    steps: Vec<VirtualMutationStepV1>,
    maximum_staging_bytes: u64,
    cancellation: Option<explorer_model::CancellationToken>,
    secret_utf16: Option<Vec<u16>>,
) -> Result<VirtualMutationCommitV1, String> {
    commit_virtual_container_mutation_inner_v1(
        runtime,
        contribution_id,
        container,
        generation,
        steps,
        maximum_staging_bytes,
        cancellation,
        secret_utf16,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn commit_virtual_container_mutation_inner_v1(
    runtime: &SinglePluginVirtualFolderRuntimeV1,
    contribution_id: &str,
    container: &Path,
    generation: u64,
    steps: Vec<VirtualMutationStepV1>,
    maximum_staging_bytes: u64,
    cancellation: Option<explorer_model::CancellationToken>,
    secret_utf16: Option<Vec<u16>>,
    #[cfg(test)] fault: Option<VirtualMutationFaultV1>,
    #[cfg(not(test))] _fault: Option<()>,
) -> Result<VirtualMutationCommitV1, String> {
    let secret_utf16 = WipeSecretUtf16V1(secret_utf16);
    let original = container_preimage(container)?;
    let (output, staging) = create_virtual_container_staging_v1(
        container,
        generation,
        maximum_staging_bytes,
        cancellation.clone(),
    )
    .map_err(|error| error.to_string())?;
    #[cfg(test)]
    if fault == Some(VirtualMutationFaultV1::Rebuild) {
        return Err("injected rebuild failure".to_owned());
    }
    let outcome = runtime
        .mutate(
            contribution_id,
            VirtualMutationRequestV1 {
                container: open_virtual_container_input_with_cancellation_v1(
                    container,
                    generation,
                    cancellation.clone(),
                )
                .map_err(|error| error.to_string())?,
                staging: output,
                container_generation: generation,
                source_generation: generation,
                steps: RVec::from(steps),
                reserved: 0,
                secret: mint_secret(secret_utf16.as_deref()),
            },
        )
        .map_err(|error| error.to_string())?;
    if outcome.status != VirtualProviderStatusV1::READY {
        return Err("provider did not produce a complete staging archive".to_owned());
    }
    #[cfg(test)]
    if fault == Some(VirtualMutationFaultV1::Flush) {
        return Err("injected flush failure".to_owned());
    }
    staging.sync().map_err(|error| error.to_string())?;
    #[cfg(test)]
    if fault == Some(VirtualMutationFaultV1::Reopen) {
        return Err("injected staging reopen failure".to_owned());
    }
    #[cfg(test)]
    if fault == Some(VirtualMutationFaultV1::Header) {
        return Err("injected header verification failure".to_owned());
    }
    #[cfg(test)]
    if fault == Some(VirtualMutationFaultV1::Inventory) {
        return Err("injected inventory verification failure".to_owned());
    }
    #[cfg(test)]
    if fault == Some(VirtualMutationFaultV1::Crc) {
        return Err("injected CRC verification failure".to_owned());
    }
    let staging_generation = generation.saturating_add(1).max(1);
    verify_rebuilt_archive(
        runtime,
        contribution_id,
        staging.path(),
        staging_generation,
        cancellation,
        secret_utf16.as_deref(),
    )?;
    if container_preimage(container)? != original {
        return Err("original container changed before commit".to_owned());
    }
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let backup = container.with_extension(format!("7z.superexplorer-backup-{nonce}"));
    let staging_path = staging.retain();
    atomic_replace_with_backup(&staging_path, container, &backup)?;
    let metadata = fs::metadata(container).map_err(|error| error.to_string())?;
    let modified = metadata
        .modified()
        .map_err(|error| error.to_string())?
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    Ok(VirtualMutationCommitV1 {
        backup,
        new_generation: (modified ^ metadata.len()).max(1),
    })
}

#[cfg(test)]
pub(crate) fn commit_virtual_container_mutation_with_fault_v1(
    runtime: &SinglePluginVirtualFolderRuntimeV1,
    contribution_id: &str,
    container: &Path,
    generation: u64,
    steps: Vec<VirtualMutationStepV1>,
    maximum_staging_bytes: u64,
    secret_utf16: Option<Vec<u16>>,
    fault: VirtualMutationFaultV1,
) -> Result<VirtualMutationCommitV1, String> {
    commit_virtual_container_mutation_inner_v1(
        runtime,
        contribution_id,
        container,
        generation,
        steps,
        maximum_staging_bytes,
        None,
        secret_utf16,
        Some(fault),
    )
}

pub fn undo_virtual_container_mutation_v1(
    runtime: &SinglePluginVirtualFolderRuntimeV1,
    contribution_id: &str,
    container: &Path,
    backup: &Path,
    generation: u64,
    secret_utf16: Option<Vec<u16>>,
) -> Result<u64, String> {
    let secret_utf16 = WipeSecretUtf16V1(secret_utf16);
    verify_rebuilt_archive(
        runtime,
        contribution_id,
        container,
        generation,
        None,
        secret_utf16.as_deref(),
    )?;
    verify_rebuilt_archive(
        runtime,
        contribution_id,
        backup,
        generation.saturating_add(1),
        None,
        secret_utf16.as_deref(),
    )?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let displaced = container.with_extension(format!("7z.superexplorer-displaced-{nonce}"));
    atomic_replace_with_backup(backup, container, &displaced)?;
    fs::remove_file(displaced).map_err(|error| error.to_string())?;
    let metadata = fs::metadata(container).map_err(|error| error.to_string())?;
    let modified = metadata
        .modified()
        .map_err(|error| error.to_string())?
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    Ok((modified ^ metadata.len()).max(1))
}
