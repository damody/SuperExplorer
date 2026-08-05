//! Host-opened seekable container input for virtual-folder providers.

use std::{
    fs::File,
    io::{Cursor, Read, Seek, SeekFrom},
    path::Path,
    sync::{Arc, Mutex},
};

use abi_stable::std_types::RVec;
use explorer_extension_api::{
    AbiInputStreamServicesV1, InputStreamCapabilityV1, InputStreamLengthOutcomeV1,
    InputStreamReadOutcomeV1, InputStreamReadRequestV1, InputStreamSeekOriginV1,
    InputStreamSeekOutcomeV1, InputStreamSeekRequestV1, InputStreamStatusV1, InputStreamV1,
    MAX_INPUT_STREAM_READ_BYTES_V1,
};

#[derive(Clone)]
struct FileInputServicesV1 {
    file: Arc<Mutex<File>>,
    length: u64,
    generation: u64,
    cancellation: Option<explorer_model::CancellationToken>,
}

#[derive(Clone)]
struct MemoryInputServicesV1 {
    cursor: Arc<Mutex<Cursor<Vec<u8>>>>,
    generation: u64,
}

impl AbiInputStreamServicesV1 for MemoryInputServicesV1 {
    fn read(&self, request: InputStreamReadRequestV1) -> InputStreamReadOutcomeV1 {
        let failed = |status, position| InputStreamReadOutcomeV1 {
            status,
            reserved: 0,
            source_generation: self.generation,
            position,
            data: RVec::new(),
        };
        if request.reserved != 0
            || request.maximum_bytes == 0
            || request.maximum_bytes > MAX_INPUT_STREAM_READ_BYTES_V1
        {
            return failed(InputStreamStatusV1::INVALID, 0);
        }
        let Ok(mut cursor) = self.cursor.lock() else {
            return failed(InputStreamStatusV1::CLOSED, 0);
        };
        let position = cursor.position();
        let mut bytes = vec![0; request.maximum_bytes as usize];
        match cursor.read(&mut bytes) {
            Ok(0) => failed(InputStreamStatusV1::EOF, position),
            Ok(count) => {
                bytes.truncate(count);
                InputStreamReadOutcomeV1 {
                    status: InputStreamStatusV1::OK,
                    reserved: 0,
                    source_generation: self.generation,
                    position: position + count as u64,
                    data: bytes.into(),
                }
            }
            Err(_) => failed(InputStreamStatusV1::CLOSED, position),
        }
    }

    fn seek(&self, request: InputStreamSeekRequestV1) -> InputStreamSeekOutcomeV1 {
        let failed = || InputStreamSeekOutcomeV1 {
            status: InputStreamStatusV1::INVALID,
            reserved: 0,
            source_generation: self.generation,
            position: 0,
        };
        if request.reserved != 0 {
            return failed();
        }
        let position = if request.origin == InputStreamSeekOriginV1::START {
            let Ok(offset) = u64::try_from(request.offset) else {
                return failed();
            };
            SeekFrom::Start(offset)
        } else if request.origin == InputStreamSeekOriginV1::CURRENT {
            SeekFrom::Current(request.offset)
        } else if request.origin == InputStreamSeekOriginV1::END {
            SeekFrom::End(request.offset)
        } else {
            return failed();
        };
        let Ok(mut cursor) = self.cursor.lock() else {
            return failed();
        };
        match cursor.seek(position) {
            Ok(position) if position <= cursor.get_ref().len() as u64 => InputStreamSeekOutcomeV1 {
                status: InputStreamStatusV1::OK,
                reserved: 0,
                source_generation: self.generation,
                position,
            },
            _ => failed(),
        }
    }

    fn length(&self) -> InputStreamLengthOutcomeV1 {
        let length = self
            .cursor
            .lock()
            .map(|cursor| cursor.get_ref().len() as u64)
            .unwrap_or(0);
        InputStreamLengthOutcomeV1 {
            status: InputStreamStatusV1::OK,
            reserved: 0,
            source_generation: self.generation,
            length,
        }
    }
}

impl AbiInputStreamServicesV1 for FileInputServicesV1 {
    fn read(&self, request: InputStreamReadRequestV1) -> InputStreamReadOutcomeV1 {
        let failed = |status, position| InputStreamReadOutcomeV1 {
            status,
            reserved: 0,
            source_generation: self.generation,
            position,
            data: RVec::new(),
        };
        if self
            .cancellation
            .as_ref()
            .is_some_and(explorer_model::CancellationToken::is_cancelled)
        {
            return failed(InputStreamStatusV1::CANCELLED, 0);
        }
        if request.reserved != 0
            || request.maximum_bytes == 0
            || request.maximum_bytes > MAX_INPUT_STREAM_READ_BYTES_V1
        {
            return failed(InputStreamStatusV1::INVALID, 0);
        }
        let Ok(mut file) = self.file.lock() else {
            return failed(InputStreamStatusV1::CLOSED, 0);
        };
        let position = file.stream_position().unwrap_or(0);
        let mut bytes = vec![0; request.maximum_bytes as usize];
        match file.read(&mut bytes) {
            Ok(0) => failed(InputStreamStatusV1::EOF, position),
            Ok(count) => {
                bytes.truncate(count);
                InputStreamReadOutcomeV1 {
                    status: InputStreamStatusV1::OK,
                    reserved: 0,
                    source_generation: self.generation,
                    position: position + count as u64,
                    data: RVec::from(bytes),
                }
            }
            Err(_) => failed(InputStreamStatusV1::CLOSED, position),
        }
    }

    fn seek(&self, request: InputStreamSeekRequestV1) -> InputStreamSeekOutcomeV1 {
        let failed = |status| InputStreamSeekOutcomeV1 {
            status,
            reserved: 0,
            source_generation: self.generation,
            position: 0,
        };
        if self
            .cancellation
            .as_ref()
            .is_some_and(explorer_model::CancellationToken::is_cancelled)
        {
            return failed(InputStreamStatusV1::CANCELLED);
        }
        if request.reserved != 0 {
            return failed(InputStreamStatusV1::INVALID);
        }
        let position = if request.origin == InputStreamSeekOriginV1::START {
            let Ok(offset) = u64::try_from(request.offset) else {
                return failed(InputStreamStatusV1::INVALID);
            };
            SeekFrom::Start(offset)
        } else if request.origin == InputStreamSeekOriginV1::CURRENT {
            SeekFrom::Current(request.offset)
        } else if request.origin == InputStreamSeekOriginV1::END {
            SeekFrom::End(request.offset)
        } else {
            return failed(InputStreamStatusV1::INVALID);
        };
        let Ok(mut file) = self.file.lock() else {
            return failed(InputStreamStatusV1::CLOSED);
        };
        match file.seek(position) {
            Ok(position) if position <= self.length => InputStreamSeekOutcomeV1 {
                status: InputStreamStatusV1::OK,
                reserved: 0,
                source_generation: self.generation,
                position,
            },
            _ => failed(InputStreamStatusV1::INVALID),
        }
    }

    fn length(&self) -> InputStreamLengthOutcomeV1 {
        InputStreamLengthOutcomeV1 {
            status: InputStreamStatusV1::OK,
            reserved: 0,
            source_generation: self.generation,
            length: self.length,
        }
    }
}

/// Opens a host-selected container and returns only its bounded stream capability.
pub fn open_virtual_container_input_v1(
    path: &Path,
    generation: u64,
) -> std::io::Result<InputStreamV1> {
    open_virtual_container_input_with_cancellation_v1(path, generation, None)
}

/// Opens the same bounded stream and interrupts decoder I/O when the owning
/// request is cancelled.
pub fn open_virtual_container_input_with_cancellation_v1(
    path: &Path,
    generation: u64,
    cancellation: Option<explorer_model::CancellationToken>,
) -> std::io::Result<InputStreamV1> {
    if generation == 0 {
        return Err(std::io::Error::other("zero container generation"));
    }
    let file = File::open(path)?;
    let length = file.metadata()?.len();
    let mut nonce = [0_u8; 16];
    nonce[..8].copy_from_slice(&generation.to_le_bytes());
    nonce[8..].copy_from_slice(&length.to_le_bytes());
    Ok(InputStreamV1::from_host(
        InputStreamCapabilityV1::from_host(nonce),
        FileInputServicesV1 {
            file: Arc::new(Mutex::new(file)),
            length,
            generation,
            cancellation,
        },
    ))
}

/// Mints a bounded in-memory stream for a host-approved inline new-file recipe.
#[must_use]
pub fn open_virtual_memory_input_v1(bytes: Vec<u8>, generation: u64) -> Option<InputStreamV1> {
    if generation == 0 || bytes.len() > 64 * 1024 {
        return None;
    }
    let mut nonce = [0_u8; 16];
    nonce[..8].copy_from_slice(&generation.to_le_bytes());
    nonce[8..].copy_from_slice(&(bytes.len() as u64).to_le_bytes());
    Some(InputStreamV1::from_host(
        InputStreamCapabilityV1::from_host(nonce),
        MemoryInputServicesV1 {
            cursor: Arc::new(Mutex::new(Cursor::new(bytes))),
            generation,
        },
    ))
}
