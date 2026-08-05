//! Host-owned, quota-bounded same-volume staging for virtual-container rebuilds.

use std::{
    fs::{File, OpenOptions},
    io::{Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use explorer_extension_api::{
    AbiVirtualOutputServicesV1, InputStreamSeekOriginV1, InputStreamSeekRequestV1,
    VirtualOutputOutcomeV1, VirtualOutputStatusV1, VirtualOutputStreamV1,
};

#[derive(Clone)]
struct FileOutputServicesV1 {
    file: Arc<Mutex<File>>,
    generation: u64,
    maximum_bytes: u64,
    cancellation: Option<explorer_model::CancellationToken>,
}

impl FileOutputServicesV1 {
    fn failed(&self, status: VirtualOutputStatusV1, position: u64) -> VirtualOutputOutcomeV1 {
        VirtualOutputOutcomeV1 {
            status,
            reserved: 0,
            generation: self.generation,
            position,
        }
    }

    fn cancelled(&self) -> bool {
        self.cancellation
            .as_ref()
            .is_some_and(explorer_model::CancellationToken::is_cancelled)
    }
}

impl AbiVirtualOutputServicesV1 for FileOutputServicesV1 {
    fn write(&self, bytes: abi_stable::std_types::RVec<u8>) -> VirtualOutputOutcomeV1 {
        if self.cancelled() {
            return self.failed(VirtualOutputStatusV1::CANCELLED, 0);
        }
        let Ok(mut file) = self.file.lock() else {
            return self.failed(VirtualOutputStatusV1::CLOSED, 0);
        };
        let position = file.stream_position().unwrap_or(0);
        let Some(end) = position.checked_add(bytes.len() as u64) else {
            return self.failed(VirtualOutputStatusV1::RESOURCE_LIMITED, position);
        };
        if end > self.maximum_bytes {
            return self.failed(VirtualOutputStatusV1::RESOURCE_LIMITED, position);
        }
        match file.write_all(&bytes) {
            Ok(()) => VirtualOutputOutcomeV1 {
                status: VirtualOutputStatusV1::OK,
                reserved: 0,
                generation: self.generation,
                position: end,
            },
            Err(_) => self.failed(VirtualOutputStatusV1::CLOSED, position),
        }
    }

    fn seek(&self, request: InputStreamSeekRequestV1) -> VirtualOutputOutcomeV1 {
        if self.cancelled() {
            return self.failed(VirtualOutputStatusV1::CANCELLED, 0);
        }
        if request.reserved != 0 {
            return self.failed(VirtualOutputStatusV1::INVALID, 0);
        }
        let position = if request.origin == InputStreamSeekOriginV1::START {
            let Ok(offset) = u64::try_from(request.offset) else {
                return self.failed(VirtualOutputStatusV1::INVALID, 0);
            };
            SeekFrom::Start(offset)
        } else if request.origin == InputStreamSeekOriginV1::CURRENT {
            SeekFrom::Current(request.offset)
        } else if request.origin == InputStreamSeekOriginV1::END {
            SeekFrom::End(request.offset)
        } else {
            return self.failed(VirtualOutputStatusV1::INVALID, 0);
        };
        let Ok(mut file) = self.file.lock() else {
            return self.failed(VirtualOutputStatusV1::CLOSED, 0);
        };
        match file.seek(position) {
            Ok(position) if position <= self.maximum_bytes => VirtualOutputOutcomeV1 {
                status: VirtualOutputStatusV1::OK,
                reserved: 0,
                generation: self.generation,
                position,
            },
            _ => self.failed(VirtualOutputStatusV1::INVALID, 0),
        }
    }

    fn flush(&self) -> VirtualOutputOutcomeV1 {
        if self.cancelled() {
            return self.failed(VirtualOutputStatusV1::CANCELLED, 0);
        }
        let Ok(mut file) = self.file.lock() else {
            return self.failed(VirtualOutputStatusV1::CLOSED, 0);
        };
        let position = file.stream_position().unwrap_or(0);
        match file.flush().and_then(|()| file.sync_all()) {
            Ok(()) => VirtualOutputOutcomeV1 {
                status: VirtualOutputStatusV1::OK,
                reserved: 0,
                generation: self.generation,
                position,
            },
            Err(_) => self.failed(VirtualOutputStatusV1::CLOSED, position),
        }
    }
}

/// Host handle for one unpublished staging file. Drop removes every
/// non-committed staging artifact.
pub struct VirtualContainerStagingV1 {
    path: PathBuf,
    file: Arc<Mutex<File>>,
    retained: bool,
}

impl VirtualContainerStagingV1 {
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn sync(&self) -> std::io::Result<()> {
        self.file
            .lock()
            .map_err(|_| std::io::Error::other("staging file closed"))?
            .sync_all()
    }

    /// Transfers cleanup ownership to the caller after verification.
    #[must_use]
    pub fn retain(mut self) -> PathBuf {
        self.retained = true;
        self.path.clone()
    }
}

impl Drop for VirtualContainerStagingV1 {
    fn drop(&mut self) {
        if !self.retained {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

/// Creates a unique same-volume staging file beside the selected container.
pub fn create_virtual_container_staging_v1(
    container: &Path,
    generation: u64,
    maximum_bytes: u64,
    cancellation: Option<explorer_model::CancellationToken>,
) -> std::io::Result<(VirtualOutputStreamV1, VirtualContainerStagingV1)> {
    if generation == 0 || maximum_bytes == 0 {
        return Err(std::io::Error::other("invalid staging authority"));
    }
    let parent = container
        .parent()
        .ok_or_else(|| std::io::Error::other("container has no parent"))?;
    let stem = container
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("archive");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |value| value.as_nanos());
    let path = parent.join(format!(
        ".{stem}.superexplorer-{generation}-{nonce}.staging"
    ));
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)?;
    let file = Arc::new(Mutex::new(file));
    let services = FileOutputServicesV1 {
        file: Arc::clone(&file),
        generation,
        maximum_bytes,
        cancellation,
    };
    Ok((
        VirtualOutputStreamV1::from_host(generation, services),
        VirtualContainerStagingV1 {
            path,
            file,
            retained: false,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn staging_is_quota_bounded_seekable_and_drop_cleans_it() {
        let root = tempfile::tempdir().unwrap();
        let container = root.path().join("fixture.7z");
        std::fs::write(&container, b"container").unwrap();
        let (output, staging) =
            create_virtual_container_staging_v1(&container, 1, 4, None).unwrap();
        let path = staging.path().to_path_buf();
        assert_eq!(
            output.write(vec![1_u8, 2, 3, 4].into()).status,
            VirtualOutputStatusV1::OK
        );
        assert_eq!(
            output.write(vec![5_u8].into()).status,
            VirtualOutputStatusV1::RESOURCE_LIMITED
        );
        assert!(path.exists());
        drop(staging);
        assert!(!path.exists());
    }
}
