#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )
)]
//! Strict length-delimited broker protocol containing no COM, HWND, GPUI, or model entities.

use std::collections::VecDeque;

pub const PROTOCOL_MAGIC: [u8; 8] = *b"RGXBRK01";
pub const PROTOCOL_VERSION: u16 = 1;
pub const HEADER_BYTES: usize = 48;
pub const MAXIMUM_FRAME_BYTES: usize = 4 * 1024 * 1024;
pub const MAXIMUM_DESCRIPTOR_BYTES: usize = 64 * 1024;

/// Extension operation selected before a disposable worker is created.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum OperationClass {
    ContextMenu = 1,
    Thumbnail = 2,
    Namespace = 3,
    Preview = 4,
}

impl TryFrom<u8> for OperationClass {
    type Error = ProtocolError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::ContextMenu),
            2 => Ok(Self::Thumbnail),
            3 => Ok(Self::Namespace),
            4 => Ok(Self::Preview),
            _ => Err(ProtocolError::UnknownOperation(value)),
        }
    }
}

/// Bounded owned start payload. Descriptor bytes never contain live COM pointers or GPUI state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StartPayload {
    pub operation: OperationClass,
    pub flags: u32,
    pub descriptor: Vec<u8>,
}

impl StartPayload {
    /// Encodes a start request without allocating beyond the descriptor bound.
    ///
    /// # Errors
    /// Returns `Oversized` when descriptor data exceeds its protocol cap.
    pub fn encode(&self) -> Result<Vec<u8>, ProtocolError> {
        if self.descriptor.len() > MAXIMUM_DESCRIPTOR_BYTES {
            return Err(ProtocolError::Oversized(self.descriptor.len()));
        }
        let length = u32::try_from(self.descriptor.len())
            .map_err(|_| ProtocolError::Oversized(self.descriptor.len()))?;
        let mut bytes = Vec::with_capacity(9 + self.descriptor.len());
        bytes.push(self.operation as u8);
        bytes.extend_from_slice(&self.flags.to_le_bytes());
        bytes.extend_from_slice(&length.to_le_bytes());
        bytes.extend_from_slice(&self.descriptor);
        Ok(bytes)
    }

    /// Decodes one exact start payload and rejects trailing or malformed bytes.
    ///
    /// # Errors
    /// Returns a protocol error for unknown operations or inconsistent length fields.
    pub fn decode(bytes: &[u8]) -> Result<Self, ProtocolError> {
        if bytes.len() < 9 {
            return Err(ProtocolError::Malformed);
        }
        let operation = OperationClass::try_from(bytes[0])?;
        let flags = u32::from_le_bytes(
            bytes[1..5]
                .try_into()
                .map_err(|_| ProtocolError::Malformed)?,
        );
        let length = u32::from_le_bytes(
            bytes[5..9]
                .try_into()
                .map_err(|_| ProtocolError::Malformed)?,
        ) as usize;
        if length > MAXIMUM_DESCRIPTOR_BYTES || bytes.len() != 9 + length {
            return Err(if length > MAXIMUM_DESCRIPTOR_BYTES {
                ProtocolError::Oversized(length)
            } else {
                ProtocolError::Malformed
            });
        }
        Ok(Self {
            operation,
            flags,
            descriptor: bytes[9..].to_vec(),
        })
    }
}

/// Owned context-menu request data; HWND is an integer contract and never a borrowed window.
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "booleans are stable independent bits in the bounded cross-process wire contract"
)]
pub struct ContextMenuPayload {
    pub version: u8,
    pub background: bool,
    pub owner_hwnd: u64,
    pub point_x: i32,
    pub point_y: i32,
    pub keyboard_invoked: bool,
    /// 0 = Explorer, 1 = Explorer plus Shift-extended verbs.
    pub invocation_profile: u8,
    pub paste_available: bool,
    pub immersive_native_context_menus: bool,
    pub dark_theme: bool,
    pub item_descriptors: Vec<Vec<u8>>,
    pub verb: Option<String>,
}

/// Owned thumbnail extraction request. Only bounded pixels may be returned in a terminal frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThumbnailPayload {
    pub item_descriptor: Vec<u8>,
    pub physical_size: u16,
    pub dpi: u16,
    pub cache_only: bool,
}

/// Bounded owned thumbnail terminal returned by a disposable worker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ThumbnailResultPayload {
    Ready {
        source: u8,
        width: u32,
        height: u32,
        stride: u32,
        pixels: Vec<u8>,
    },
    Fallback {
        reason: u8,
    },
    Failed,
}

/// One bounded namespace enumeration/property request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NamespacePayload {
    pub container_descriptor: Vec<u8>,
    pub generation: u64,
    pub maximum_items: u32,
    pub maximum_bytes: u32,
}

/// Initial persistent Preview Handler request. The descriptor is owned and the HWND is validated
/// again by the worker before it is used as a cross-process child-window parent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreviewStartPayload {
    pub item_descriptor: Vec<u8>,
    pub generation: u64,
    pub parent_hwnd: u64,
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
    pub dpi: u32,
}

/// Explicit least-authority resources transferred for one operation. Numeric
/// handles are duplicated into the destination process by the supervisor; raw
/// app-process handle values are never accepted as usable worker handles.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationCapabilityPayload {
    pub operation: OperationClass,
    pub item_descriptors: Vec<Vec<u8>>,
    pub duplicated_handles: Vec<u64>,
    pub allow_window_hosting: bool,
    pub allow_read_stream: bool,
    pub allow_write: bool,
}

impl OperationCapabilityPayload {
    /// Validates a deny-by-default authority envelope.
    ///
    /// # Errors
    /// Rejects excessive handles/descriptors and authority not valid for an
    /// operation class.
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.item_descriptors.len() > 256 || self.duplicated_handles.len() > 16 {
            return Err(ProtocolError::Oversized(
                self.item_descriptors.len() + self.duplicated_handles.len(),
            ));
        }
        validate_descriptors(&self.item_descriptors)?;
        let valid = match self.operation {
            OperationClass::ContextMenu => {
                !self.allow_read_stream && !self.allow_write && self.duplicated_handles.is_empty()
            }
            OperationClass::Thumbnail => !self.allow_window_hosting && !self.allow_write,
            OperationClass::Namespace => !self.allow_window_hosting && !self.allow_read_stream,
            OperationClass::Preview => !self.allow_write,
        };
        if valid {
            Ok(())
        } else {
            Err(ProtocolError::Authority)
        }
    }
}

/// Preview host protocol. Variant IDs are stable and unknown values are never inferred.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PreviewMessage {
    Lookup {
        descriptor: Vec<u8>,
        generation: u64,
    },
    SetBounds {
        generation: u64,
        left: i32,
        top: i32,
        width: u32,
        height: u32,
        dpi: u32,
    },
    SetFocus {
        generation: u64,
    },
    Accelerator {
        generation: u64,
        virtual_key: u32,
        modifiers: u8,
    },
    Unload {
        generation: u64,
    },
    Attach {
        generation: u64,
        parent_hwnd: u64,
        left: i32,
        top: i32,
        width: u32,
        height: u32,
        dpi: u32,
    },
}

impl ContextMenuPayload {
    pub const VERSION: u8 = 5;
    /// Encodes the complete context request with explicit lengths. Numeric HWND values are
    /// treated only as a cross-process owner contract; no pointer is dereferenced here.
    ///
    /// # Errors
    /// Returns an error when validation fails or a descriptor, verb, or frame exceeds its limit.
    pub fn encode(&self) -> Result<Vec<u8>, ProtocolError> {
        self.validate()?;
        let mut bytes = Vec::new();
        bytes.push(self.version);
        bytes.push(u8::from(self.background));
        bytes.extend_from_slice(&self.owner_hwnd.to_le_bytes());
        bytes.extend_from_slice(&self.point_x.to_le_bytes());
        bytes.extend_from_slice(&self.point_y.to_le_bytes());
        bytes.push(u8::from(self.keyboard_invoked));
        bytes.push(self.invocation_profile);
        bytes.push(u8::from(self.paste_available));
        bytes.push(u8::from(self.immersive_native_context_menus));
        bytes.push(u8::from(self.dark_theme));
        let count = u16::try_from(self.item_descriptors.len())
            .map_err(|_| ProtocolError::Oversized(self.item_descriptors.len()))?;
        bytes.extend_from_slice(&count.to_le_bytes());
        for descriptor in &self.item_descriptors {
            let length = u32::try_from(descriptor.len())
                .map_err(|_| ProtocolError::Oversized(descriptor.len()))?;
            bytes.extend_from_slice(&length.to_le_bytes());
            bytes.extend_from_slice(descriptor);
        }
        match &self.verb {
            Some(verb) => {
                bytes.push(1);
                let value = verb.as_bytes();
                let length = u16::try_from(value.len())
                    .map_err(|_| ProtocolError::Oversized(value.len()))?;
                bytes.extend_from_slice(&length.to_le_bytes());
                bytes.extend_from_slice(value);
            }
            None => bytes.push(0),
        }
        if bytes.len() > MAXIMUM_DESCRIPTOR_BYTES {
            return Err(ProtocolError::Oversized(bytes.len()));
        }
        Ok(bytes)
    }

    /// Decodes one exact payload and rejects truncation, trailing bytes, and invalid UTF-8.
    ///
    /// # Errors
    /// Returns an error when the payload is malformed, truncated, oversized, or invalid UTF-8.
    pub fn decode(bytes: &[u8]) -> Result<Self, ProtocolError> {
        let mut cursor = PayloadCursor::new(bytes);
        let version = cursor.u8()?;
        let background = match cursor.u8()? {
            0 => false,
            1 => true,
            _ => return Err(ProtocolError::Malformed),
        };
        let owner_hwnd = cursor.u64()?;
        let point_x = cursor.i32()?;
        let point_y = cursor.i32()?;
        let keyboard_invoked = match cursor.u8()? {
            0 => false,
            1 => true,
            _ => return Err(ProtocolError::Malformed),
        };
        let invocation_profile = cursor.u8()?;
        let paste_available = match cursor.u8()? {
            0 => false,
            1 => true,
            _ => return Err(ProtocolError::Malformed),
        };
        let immersive_native_context_menus = match cursor.u8()? {
            0 => false,
            1 => true,
            _ => return Err(ProtocolError::Malformed),
        };
        let dark_theme = match cursor.u8()? {
            0 => false,
            1 => true,
            _ => return Err(ProtocolError::Malformed),
        };
        let count = usize::from(cursor.u16()?);
        let mut item_descriptors = Vec::with_capacity(count.min(256));
        for _ in 0..count {
            let length = usize::try_from(cursor.u32()?).map_err(|_| ProtocolError::Malformed)?;
            item_descriptors.push(cursor.take(length)?.to_vec());
        }
        let verb = match cursor.u8()? {
            0 => None,
            1 => {
                let length = usize::from(cursor.u16()?);
                Some(
                    std::str::from_utf8(cursor.take(length)?)
                        .map_err(|_| ProtocolError::Malformed)?
                        .to_owned(),
                )
            }
            _ => return Err(ProtocolError::Malformed),
        };
        if !cursor.is_finished() {
            return Err(ProtocolError::Malformed);
        }
        let payload = Self {
            version,
            background,
            owner_hwnd,
            point_x,
            point_y,
            keyboard_invoked,
            invocation_profile,
            paste_available,
            immersive_native_context_menus,
            dark_theme,
            item_descriptors,
            verb,
        };
        payload.validate()?;
        Ok(payload)
    }

    /// Validates item count, descriptor size, and verb length at the trust boundary.
    ///
    /// # Errors
    /// Returns `Oversized` or `Malformed` for excessive/empty request data.
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.version != Self::VERSION || self.invocation_profile > 1 {
            return Err(ProtocolError::Malformed);
        }
        if self.item_descriptors.len() > 256
            || self.verb.as_ref().is_some_and(|value| value.len() > 1_024)
        {
            return Err(ProtocolError::Oversized(self.item_descriptors.len()));
        }
        if !self.background && self.item_descriptors.is_empty() {
            return Err(ProtocolError::Malformed);
        }
        validate_descriptors(&self.item_descriptors)
    }
}

struct PayloadCursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> PayloadCursor<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], ProtocolError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or(ProtocolError::Malformed)?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or(ProtocolError::Malformed)?;
        self.offset = end;
        Ok(value)
    }

    fn u8(&mut self) -> Result<u8, ProtocolError> {
        Ok(*self.take(1)?.first().ok_or(ProtocolError::Malformed)?)
    }

    fn u16(&mut self) -> Result<u16, ProtocolError> {
        Ok(u16::from_le_bytes(
            self.take(2)?
                .try_into()
                .map_err(|_| ProtocolError::Malformed)?,
        ))
    }

    fn u32(&mut self) -> Result<u32, ProtocolError> {
        Ok(u32::from_le_bytes(
            self.take(4)?
                .try_into()
                .map_err(|_| ProtocolError::Malformed)?,
        ))
    }

    fn i32(&mut self) -> Result<i32, ProtocolError> {
        Ok(i32::from_le_bytes(
            self.take(4)?
                .try_into()
                .map_err(|_| ProtocolError::Malformed)?,
        ))
    }

    fn u64(&mut self) -> Result<u64, ProtocolError> {
        Ok(u64::from_le_bytes(
            self.take(8)?
                .try_into()
                .map_err(|_| ProtocolError::Malformed)?,
        ))
    }

    const fn is_finished(&self) -> bool {
        self.offset == self.bytes.len()
    }
}

impl ThumbnailPayload {
    /// Encodes a validated thumbnail request payload.
    ///
    /// # Errors
    /// Returns an error when the descriptor, requested size, or DPI is invalid or oversized.
    pub fn encode(&self) -> Result<Vec<u8>, ProtocolError> {
        self.validate()?;
        let length = u32::try_from(self.item_descriptor.len())
            .map_err(|_| ProtocolError::Oversized(self.item_descriptor.len()))?;
        let mut bytes = Vec::with_capacity(self.item_descriptor.len().saturating_add(9));
        bytes.extend_from_slice(&self.physical_size.to_le_bytes());
        bytes.extend_from_slice(&self.dpi.to_le_bytes());
        bytes.push(u8::from(self.cache_only));
        bytes.extend_from_slice(&length.to_le_bytes());
        bytes.extend_from_slice(&self.item_descriptor);
        Ok(bytes)
    }

    /// Decodes and validates one exact thumbnail request payload.
    ///
    /// # Errors
    /// Returns an error when the payload is malformed, truncated, trailing, or out of bounds.
    pub fn decode(bytes: &[u8]) -> Result<Self, ProtocolError> {
        let mut cursor = PayloadCursor::new(bytes);
        let physical_size = cursor.u16()?;
        let dpi = cursor.u16()?;
        let cache_only = match cursor.u8()? {
            0 => false,
            1 => true,
            _ => return Err(ProtocolError::Malformed),
        };
        let length = usize::try_from(cursor.u32()?).map_err(|_| ProtocolError::Malformed)?;
        let item_descriptor = cursor.take(length)?.to_vec();
        if !cursor.is_finished() {
            return Err(ProtocolError::Malformed);
        }
        let payload = Self {
            item_descriptor,
            physical_size,
            dpi,
            cache_only,
        };
        payload.validate()?;
        Ok(payload)
    }

    /// Validates descriptor, size, and DPI before provider activation.
    ///
    /// # Errors
    /// Returns a protocol error for invalid bounds.
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_descriptor(&self.item_descriptor)?;
        if !(1..=4_096).contains(&self.physical_size) || !(48..=960).contains(&self.dpi) {
            return Err(ProtocolError::Malformed);
        }
        Ok(())
    }
}

impl ThumbnailResultPayload {
    /// Encodes a thumbnail terminal result and its owned pixel buffer.
    ///
    /// # Errors
    /// Returns an error when dimensions, stride, pixel length, or frame size is invalid.
    pub fn encode(&self) -> Result<Vec<u8>, ProtocolError> {
        let mut bytes = Vec::new();
        match self {
            Self::Ready {
                source,
                width,
                height,
                stride,
                pixels,
            } => {
                let expected = usize::try_from(*stride)
                    .ok()
                    .and_then(|row| {
                        usize::try_from(*height)
                            .ok()
                            .and_then(|h| row.checked_mul(h))
                    })
                    .ok_or(ProtocolError::Malformed)?;
                if *width == 0
                    || *height == 0
                    || *stride < width.saturating_mul(4)
                    || pixels.len() != expected
                    || pixels.len().saturating_add(18) > MAXIMUM_FRAME_BYTES
                {
                    return Err(ProtocolError::Malformed);
                }
                bytes.push(1);
                bytes.push(*source);
                bytes.extend_from_slice(&width.to_le_bytes());
                bytes.extend_from_slice(&height.to_le_bytes());
                bytes.extend_from_slice(&stride.to_le_bytes());
                bytes.extend_from_slice(
                    &u32::try_from(pixels.len())
                        .map_err(|_| ProtocolError::Oversized(pixels.len()))?
                        .to_le_bytes(),
                );
                bytes.extend_from_slice(pixels);
            }
            Self::Fallback { reason } => {
                bytes.push(2);
                bytes.push(*reason);
            }
            Self::Failed => bytes.push(3),
        }
        Ok(bytes)
    }

    /// Decodes one exact thumbnail terminal result.
    ///
    /// # Errors
    /// Returns an error when the result is malformed, truncated, trailing, or inconsistent.
    pub fn decode(bytes: &[u8]) -> Result<Self, ProtocolError> {
        let mut cursor = PayloadCursor::new(bytes);
        let result = match cursor.u8()? {
            1 => {
                let source = cursor.u8()?;
                let width = cursor.u32()?;
                let height = cursor.u32()?;
                let stride = cursor.u32()?;
                let length =
                    usize::try_from(cursor.u32()?).map_err(|_| ProtocolError::Malformed)?;
                let pixels = cursor.take(length)?.to_vec();
                let value = Self::Ready {
                    source,
                    width,
                    height,
                    stride,
                    pixels,
                };
                // Reuse the encoder as the single pixel invariant validator.
                let _ = value.encode()?;
                value
            }
            2 => Self::Fallback {
                reason: cursor.u8()?,
            },
            3 => Self::Failed,
            _ => return Err(ProtocolError::Malformed),
        };
        if !cursor.is_finished() {
            return Err(ProtocolError::Malformed);
        }
        Ok(result)
    }
}

impl NamespacePayload {
    /// Validates descriptor and result budgets.
    ///
    /// # Errors
    /// Returns a protocol error for zero or excessive result budgets.
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_descriptor(&self.container_descriptor)?;
        if self.maximum_items == 0
            || self.maximum_items > 4_096
            || self.maximum_bytes == 0
            || self.maximum_bytes as usize > MAXIMUM_FRAME_BYTES
        {
            return Err(ProtocolError::Malformed);
        }
        Ok(())
    }
}

impl PreviewStartPayload {
    /// Encodes a bounded preview-start payload for transport to the broker.
    ///
    /// # Errors
    ///
    /// Returns [`ProtocolError`] when the item descriptor, parent window, bounds, DPI, or
    /// encoded descriptor length violates the protocol limits.
    pub fn encode(&self) -> Result<Vec<u8>, ProtocolError> {
        self.validate()?;
        let length = u32::try_from(self.item_descriptor.len())
            .map_err(|_| ProtocolError::Oversized(self.item_descriptor.len()))?;
        let mut bytes = Vec::with_capacity(self.item_descriptor.len().saturating_add(44));
        bytes.extend_from_slice(&self.generation.to_le_bytes());
        bytes.extend_from_slice(&self.parent_hwnd.to_le_bytes());
        bytes.extend_from_slice(&self.left.to_le_bytes());
        bytes.extend_from_slice(&self.top.to_le_bytes());
        bytes.extend_from_slice(&self.width.to_le_bytes());
        bytes.extend_from_slice(&self.height.to_le_bytes());
        bytes.extend_from_slice(&self.dpi.to_le_bytes());
        bytes.extend_from_slice(&length.to_le_bytes());
        bytes.extend_from_slice(&self.item_descriptor);
        Ok(bytes)
    }

    /// Decodes and validates a preview-start payload received from the broker protocol.
    ///
    /// # Errors
    ///
    /// Returns [`ProtocolError`] when the payload is truncated, malformed, has trailing data,
    /// or contains descriptor, window, bounds, or DPI values outside the protocol limits.
    pub fn decode(bytes: &[u8]) -> Result<Self, ProtocolError> {
        let mut cursor = PayloadCursor::new(bytes);
        let generation = cursor.u64()?;
        let parent_hwnd = cursor.u64()?;
        let left = cursor.i32()?;
        let top = cursor.i32()?;
        let width = cursor.u32()?;
        let height = cursor.u32()?;
        let dpi = cursor.u32()?;
        let length = usize::try_from(cursor.u32()?).map_err(|_| ProtocolError::Malformed)?;
        let item_descriptor = cursor.take(length)?.to_vec();
        if !cursor.is_finished() {
            return Err(ProtocolError::Malformed);
        }
        let payload = Self {
            item_descriptor,
            generation,
            parent_hwnd,
            left,
            top,
            width,
            height,
            dpi,
        };
        payload.validate()?;
        Ok(payload)
    }

    /// Validates the descriptor and attach-window invariants without encoding the payload.
    ///
    /// # Errors
    ///
    /// Returns [`ProtocolError`] when the descriptor, parent window, bounds, or DPI violates
    /// the protocol limits.
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_descriptor(&self.item_descriptor)?;
        PreviewMessage::Attach {
            generation: self.generation,
            parent_hwnd: self.parent_hwnd,
            left: self.left,
            top: self.top,
            width: self.width,
            height: self.height,
            dpi: self.dpi,
        }
        .validate()
    }
}

impl PreviewMessage {
    /// Encodes one exact, bounded preview lifecycle command.
    ///
    /// # Errors
    /// Returns an error when a descriptor, HWND, bounds, DPI, or accelerator is invalid.
    pub fn encode(&self) -> Result<Vec<u8>, ProtocolError> {
        self.validate()?;
        let mut bytes = Vec::new();
        bytes.push(self.numeric_identity());
        match self {
            Self::Lookup {
                descriptor,
                generation,
            } => {
                bytes.extend_from_slice(&generation.to_le_bytes());
                let length = u32::try_from(descriptor.len())
                    .map_err(|_| ProtocolError::Oversized(descriptor.len()))?;
                bytes.extend_from_slice(&length.to_le_bytes());
                bytes.extend_from_slice(descriptor);
            }
            Self::SetBounds {
                generation,
                left,
                top,
                width,
                height,
                dpi,
            } => {
                bytes.extend_from_slice(&generation.to_le_bytes());
                bytes.extend_from_slice(&left.to_le_bytes());
                bytes.extend_from_slice(&top.to_le_bytes());
                bytes.extend_from_slice(&width.to_le_bytes());
                bytes.extend_from_slice(&height.to_le_bytes());
                bytes.extend_from_slice(&dpi.to_le_bytes());
            }
            Self::SetFocus { generation } | Self::Unload { generation } => {
                bytes.extend_from_slice(&generation.to_le_bytes());
            }
            Self::Accelerator {
                generation,
                virtual_key,
                modifiers,
            } => {
                bytes.extend_from_slice(&generation.to_le_bytes());
                bytes.extend_from_slice(&virtual_key.to_le_bytes());
                bytes.push(*modifiers);
            }
            Self::Attach {
                generation,
                parent_hwnd,
                left,
                top,
                width,
                height,
                dpi,
            } => {
                bytes.extend_from_slice(&generation.to_le_bytes());
                bytes.extend_from_slice(&parent_hwnd.to_le_bytes());
                bytes.extend_from_slice(&left.to_le_bytes());
                bytes.extend_from_slice(&top.to_le_bytes());
                bytes.extend_from_slice(&width.to_le_bytes());
                bytes.extend_from_slice(&height.to_le_bytes());
                bytes.extend_from_slice(&dpi.to_le_bytes());
            }
        }
        Ok(bytes)
    }

    /// Decodes one exact preview command and rejects unknown variants or trailing bytes.
    ///
    /// # Errors
    /// Returns an error for malformed, oversized, or excessive-authority input.
    pub fn decode(bytes: &[u8]) -> Result<Self, ProtocolError> {
        let mut cursor = PayloadCursor::new(bytes);
        let tag = cursor.u8()?;
        let generation = cursor.u64()?;
        let message = match tag {
            1 => {
                let length =
                    usize::try_from(cursor.u32()?).map_err(|_| ProtocolError::Malformed)?;
                Self::Lookup {
                    descriptor: cursor.take(length)?.to_vec(),
                    generation,
                }
            }
            2 => Self::SetBounds {
                generation,
                left: cursor.i32()?,
                top: cursor.i32()?,
                width: cursor.u32()?,
                height: cursor.u32()?,
                dpi: cursor.u32()?,
            },
            3 => Self::SetFocus { generation },
            4 => Self::Accelerator {
                generation,
                virtual_key: cursor.u32()?,
                modifiers: cursor.u8()?,
            },
            5 => Self::Unload { generation },
            6 => Self::Attach {
                generation,
                parent_hwnd: cursor.u64()?,
                left: cursor.i32()?,
                top: cursor.i32()?,
                width: cursor.u32()?,
                height: cursor.u32()?,
                dpi: cursor.u32()?,
            },
            value => return Err(ProtocolError::UnknownOperation(value)),
        };
        if !cursor.is_finished() {
            return Err(ProtocolError::Malformed);
        }
        message.validate()?;
        Ok(message)
    }

    /// Validates descriptor, bounds, DPI, and accelerator fields.
    ///
    /// # Errors
    /// Returns a protocol error before invalid host data reaches native code.
    pub fn validate(&self) -> Result<(), ProtocolError> {
        match self {
            Self::Lookup { descriptor, .. } => validate_descriptor(descriptor),
            Self::SetBounds {
                width, height, dpi, ..
            } if *width > 0
                && *height > 0
                && *width <= 16_384
                && *height <= 16_384
                && (48..=960).contains(dpi) =>
            {
                Ok(())
            }
            Self::SetFocus { .. } | Self::Unload { .. } => Ok(()),
            Self::Accelerator {
                virtual_key,
                modifiers,
                ..
            } if *virtual_key <= 0xffff && *modifiers & !0x0f == 0 => Ok(()),
            Self::Attach {
                parent_hwnd,
                width,
                height,
                dpi,
                ..
            } if *parent_hwnd != 0
                && *width > 0
                && *height > 0
                && *width <= 16_384
                && *height <= 16_384
                && (48..=960).contains(dpi) =>
            {
                Ok(())
            }
            Self::SetBounds { .. } | Self::Accelerator { .. } | Self::Attach { .. } => {
                Err(ProtocolError::Malformed)
            }
        }
    }

    pub const fn numeric_identity(&self) -> u8 {
        match self {
            Self::Lookup { .. } => 1,
            Self::SetBounds { .. } => 2,
            Self::SetFocus { .. } => 3,
            Self::Accelerator { .. } => 4,
            Self::Unload { .. } => 5,
            Self::Attach { .. } => 6,
        }
    }
}

fn validate_descriptors(descriptors: &[Vec<u8>]) -> Result<(), ProtocolError> {
    let mut total = 0_usize;
    for descriptor in descriptors {
        validate_descriptor(descriptor)?;
        total = total
            .checked_add(descriptor.len())
            .ok_or(ProtocolError::Oversized(usize::MAX))?;
    }
    if total > MAXIMUM_FRAME_BYTES.saturating_sub(HEADER_BYTES) {
        Err(ProtocolError::Oversized(total))
    } else {
        Ok(())
    }
}

fn validate_descriptor(descriptor: &[u8]) -> Result<(), ProtocolError> {
    if descriptor.is_empty() {
        Err(ProtocolError::Malformed)
    } else if descriptor.len() > MAXIMUM_DESCRIPTOR_BYTES {
        Err(ProtocolError::Oversized(descriptor.len()))
    } else {
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SessionNonce(pub [u8; 16]);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BrokerRequestId(pub u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u16)]
pub enum MessageKind {
    Hello = 1,
    HelloAck,
    Start,
    Progress,
    Cancel,
    Terminal,
    Heartbeat,
    Shutdown,
}

impl TryFrom<u16> for MessageKind {
    type Error = ProtocolError;
    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Hello),
            2 => Ok(Self::HelloAck),
            3 => Ok(Self::Start),
            4 => Ok(Self::Progress),
            5 => Ok(Self::Cancel),
            6 => Ok(Self::Terminal),
            7 => Ok(Self::Heartbeat),
            8 => Ok(Self::Shutdown),
            _ => Err(ProtocolError::UnknownMessage(value)),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Frame {
    pub version: u16,
    pub kind: MessageKind,
    pub feature_bits: u32,
    pub nonce: SessionNonce,
    pub request_id: BrokerRequestId,
    pub payload: Vec<u8>,
}

impl Frame {
    pub fn new(
        kind: MessageKind,
        feature_bits: u32,
        nonce: SessionNonce,
        request_id: BrokerRequestId,
        payload: Vec<u8>,
    ) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            kind,
            feature_bits,
            nonce,
            request_id,
            payload,
        }
    }

    /// Encodes one complete checked frame.
    ///
    /// # Errors
    /// Returns `Oversized` when payload or full frame exceeds the configured cap.
    pub fn encode(&self, maximum: usize) -> Result<Vec<u8>, ProtocolError> {
        let payload_len = u32::try_from(self.payload.len())
            .map_err(|_| ProtocolError::Oversized(self.payload.len()))?;
        let total = HEADER_BYTES
            .checked_add(self.payload.len())
            .ok_or(ProtocolError::Oversized(self.payload.len()))?;
        if total > maximum {
            return Err(ProtocolError::Oversized(total));
        }
        let mut bytes = Vec::with_capacity(total);
        bytes.extend_from_slice(&PROTOCOL_MAGIC);
        bytes.extend_from_slice(&self.version.to_le_bytes());
        bytes.extend_from_slice(&(self.kind as u16).to_le_bytes());
        bytes.extend_from_slice(&self.feature_bits.to_le_bytes());
        bytes.extend_from_slice(&self.nonce.0);
        bytes.extend_from_slice(&self.request_id.0.to_le_bytes());
        bytes.extend_from_slice(&payload_len.to_le_bytes());
        bytes.extend_from_slice(&checksum(&self.payload).to_le_bytes());
        bytes.extend_from_slice(&self.payload);
        Ok(bytes)
    }
}

#[derive(Debug)]
pub struct FrameDecoder {
    bytes: VecDeque<u8>,
    maximum: usize,
}

impl FrameDecoder {
    pub fn new(maximum: usize) -> Self {
        Self {
            bytes: VecDeque::new(),
            maximum: maximum.max(HEADER_BYTES),
        }
    }

    /// Appends a partial read under the configured frame bound.
    ///
    /// # Errors
    /// Returns `Oversized` before retaining excessive input.
    pub fn push(&mut self, input: &[u8]) -> Result<(), ProtocolError> {
        let total = self
            .bytes
            .len()
            .checked_add(input.len())
            .ok_or(ProtocolError::Oversized(input.len()))?;
        if total > self.maximum {
            return Err(ProtocolError::Oversized(total));
        }
        self.bytes.extend(input.iter().copied());
        Ok(())
    }

    /// Decodes one frame while preserving partial data.
    ///
    /// # Errors
    /// Rejects invalid magic, version, type, size, checksum, or header data.
    pub fn next_frame(&mut self) -> Result<Option<Frame>, ProtocolError> {
        if self.bytes.len() < HEADER_BYTES {
            return Ok(None);
        }
        let h = self
            .bytes
            .iter()
            .take(HEADER_BYTES)
            .copied()
            .collect::<Vec<_>>();
        if h[..8] != PROTOCOL_MAGIC {
            return Err(ProtocolError::BadMagic);
        }
        let version = u16::from_le_bytes([h[8], h[9]]);
        if version != PROTOCOL_VERSION {
            return Err(ProtocolError::UnsupportedVersion(version));
        }
        let kind = MessageKind::try_from(u16::from_le_bytes([h[10], h[11]]))?;
        let feature_bits =
            u32::from_le_bytes(h[12..16].try_into().map_err(|_| ProtocolError::Malformed)?);
        let nonce = SessionNonce(h[16..32].try_into().map_err(|_| ProtocolError::Malformed)?);
        let request_id = BrokerRequestId(u64::from_le_bytes(
            h[32..40].try_into().map_err(|_| ProtocolError::Malformed)?,
        ));
        let length = u32::from_le_bytes(h[40..44].try_into().map_err(|_| ProtocolError::Malformed)?)
            as usize;
        let expected =
            u32::from_le_bytes(h[44..48].try_into().map_err(|_| ProtocolError::Malformed)?);
        let total = HEADER_BYTES
            .checked_add(length)
            .ok_or(ProtocolError::Oversized(length))?;
        if total > self.maximum {
            return Err(ProtocolError::Oversized(total));
        }
        if self.bytes.len() < total {
            return Ok(None);
        }
        let payload = self
            .bytes
            .iter()
            .skip(HEADER_BYTES)
            .take(length)
            .copied()
            .collect::<Vec<_>>();
        if checksum(&payload) != expected {
            return Err(ProtocolError::Checksum);
        }
        self.bytes.drain(..total);
        Ok(Some(Frame {
            version,
            kind,
            feature_bits,
            nonce,
            request_id,
            payload,
        }))
    }

    /// Rejects EOF with a partial frame.
    ///
    /// # Errors
    /// Returns `UnexpectedEof` if a partial header or payload remains.
    pub fn finish(self) -> Result<(), ProtocolError> {
        if self.bytes.is_empty() {
            Ok(())
        } else {
            Err(ProtocolError::UnexpectedEof)
        }
    }
}

/// Verifies inherited per-process session authentication.
///
/// # Errors
/// Returns `Authentication` for a foreign or replayed session nonce.
pub fn authenticate(frame: &Frame, expected: SessionNonce) -> Result<(), ProtocolError> {
    if frame.nonce == expected {
        Ok(())
    } else {
        Err(ProtocolError::Authentication)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ProtocolError {
    #[error("bad broker protocol magic")]
    BadMagic,
    #[error("unsupported broker protocol version {0}")]
    UnsupportedVersion(u16),
    #[error("unknown broker message {0}")]
    UnknownMessage(u16),
    #[error("broker frame is oversized: {0}")]
    Oversized(usize),
    #[error("broker frame checksum mismatch")]
    Checksum,
    #[error("malformed broker frame")]
    Malformed,
    #[error("unexpected EOF in broker frame")]
    UnexpectedEof,
    #[error("broker session authentication failed")]
    Authentication,
    #[error("unknown broker operation {0}")]
    UnknownOperation(u8),
    #[error("broker operation requested excessive authority")]
    Authority,
}

fn checksum(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0_u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_menu_payload_round_trips_multi_item_owner_and_verb_exactly() {
        let payload = ContextMenuPayload {
            version: ContextMenuPayload::VERSION,
            background: false,
            owner_hwnd: 0x1234,
            point_x: -12,
            point_y: 45,
            keyboard_invoked: true,
            invocation_profile: 1,
            paste_available: true,
            immersive_native_context_menus: true,
            dark_theme: true,
            item_descriptors: vec![b"C:\\one.txt".to_vec(), b"C:\\two.txt".to_vec()],
            verb: Some("open".to_owned()),
        };
        let encoded = payload.encode().expect("encode context menu");
        assert_eq!(ContextMenuPayload::decode(&encoded), Ok(payload));
        let mut trailing = encoded;
        trailing.push(0);
        assert_eq!(
            ContextMenuPayload::decode(&trailing),
            Err(ProtocolError::Malformed)
        );
    }

    #[test]
    fn thumbnail_request_and_owned_pixels_round_trip_with_exact_bounds() {
        let request = ThumbnailPayload {
            item_descriptor: b"Fsample.png".to_vec(),
            physical_size: 96,
            dpi: 144,
            cache_only: true,
        };
        assert_eq!(
            ThumbnailPayload::decode(&request.encode().expect("encode request")),
            Ok(request)
        );
        let result = ThumbnailResultPayload::Ready {
            source: 2,
            width: 2,
            height: 2,
            stride: 8,
            pixels: vec![0x7f; 16],
        };
        assert_eq!(
            ThumbnailResultPayload::decode(&result.encode().expect("encode result")),
            Ok(result)
        );
    }
    fn frame() -> Frame {
        Frame::new(
            MessageKind::Start,
            3,
            SessionNonce([7; 16]),
            BrokerRequestId(42),
            b"owned".to_vec(),
        )
    }

    #[test]
    fn partial_round_trip_and_authentication() {
        let bytes = frame().encode(1024).expect("encode");
        let mut decoder = FrameDecoder::new(1024);
        for chunk in bytes.chunks(3) {
            decoder.push(chunk).expect("push");
        }
        let frame_out = decoder.next_frame().expect("decode").expect("frame");
        assert_eq!(frame_out, frame());
        assert_eq!(authenticate(&frame_out, SessionNonce([7; 16])), Ok(()));
        assert_eq!(
            authenticate(&frame_out, SessionNonce([8; 16])),
            Err(ProtocolError::Authentication)
        );
        assert_eq!(decoder.finish(), Ok(()));
    }

    #[test]
    fn persistent_stream_decodes_hello_start_terminal_and_shutdown_in_order() {
        let nonce = SessionNonce([9; 16]);
        let frames = [
            Frame::new(MessageKind::Hello, 0, nonce, BrokerRequestId(0), Vec::new()),
            Frame::new(
                MessageKind::Start,
                0,
                nonce,
                BrokerRequestId(1),
                b"request".to_vec(),
            ),
            Frame::new(
                MessageKind::Terminal,
                123,
                nonce,
                BrokerRequestId(1),
                b"success".to_vec(),
            ),
            Frame::new(
                MessageKind::Shutdown,
                0,
                nonce,
                BrokerRequestId(2),
                Vec::new(),
            ),
        ];
        let bytes = frames
            .iter()
            .flat_map(|frame| frame.encode(1024).expect("persistent frame"))
            .collect::<Vec<_>>();
        let mut decoder = FrameDecoder::new(1024);
        let mut decoded_frames = Vec::new();
        for chunk in bytes.chunks(17) {
            decoder.push(chunk).expect("stream chunk");
            while let Some(frame) = decoder.next_frame().expect("stream frame") {
                decoded_frames.push(frame);
            }
        }
        assert_eq!(decoded_frames, frames);
        assert_eq!(decoder.finish(), Ok(()));
    }

    #[test]
    fn malformed_matrix_is_rejected() {
        let bytes = frame().encode(1024).expect("encode");
        for (index, expected) in [
            (0, ProtocolError::BadMagic),
            (8, ProtocolError::UnsupportedVersion(0)),
            (10, ProtocolError::UnknownMessage(0)),
        ] {
            let mut damaged = bytes.clone();
            damaged[index] = 0;
            let mut decoder = FrameDecoder::new(1024);
            decoder.push(&damaged).expect("push");
            assert_eq!(decoder.next_frame(), Err(expected));
        }
        let mut damaged = bytes.clone();
        *damaged.last_mut().expect("body") ^= 1;
        let mut decoder = FrameDecoder::new(1024);
        decoder.push(&damaged).expect("push");
        assert_eq!(decoder.next_frame(), Err(ProtocolError::Checksum));
        let mut decoder = FrameDecoder::new(1024);
        decoder.push(&bytes[..12]).expect("push");
        assert_eq!(decoder.finish(), Err(ProtocolError::UnexpectedEof));
        assert!(frame().encode(HEADER_BYTES).is_err());
    }

    #[test]
    fn deterministic_decoder_corpus_never_panics() {
        let mut seed = 0x1234_5678_u64;
        for length in 0..512 {
            let mut bytes = vec![0; length];
            for byte in &mut bytes {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                *byte = seed.to_le_bytes()[0];
            }
            let mut decoder = FrameDecoder::new(256);
            let _ = decoder.push(&bytes);
            let _ = decoder.next_frame();
        }
    }

    #[test]
    fn start_payload_is_owned_bounded_and_exact() {
        let payload = StartPayload {
            operation: OperationClass::Preview,
            flags: 3,
            descriptor: b"digest-only".to_vec(),
        };
        let bytes = payload.encode().expect("payload");
        assert_eq!(StartPayload::decode(&bytes), Ok(payload));
        assert_eq!(
            StartPayload::decode(&bytes[..4]),
            Err(ProtocolError::Malformed)
        );
        let mut trailing = bytes;
        trailing.push(0);
        assert_eq!(
            StartPayload::decode(&trailing),
            Err(ProtocolError::Malformed)
        );
    }

    #[test]
    fn operation_payloads_reject_excessive_authority_and_host_bounds() {
        assert!(
            ContextMenuPayload {
                version: ContextMenuPayload::VERSION,
                background: false,
                owner_hwnd: 1,
                point_x: 0,
                point_y: 0,
                keyboard_invoked: false,
                invocation_profile: 0,
                paste_available: false,
                immersive_native_context_menus: false,
                dark_theme: false,
                item_descriptors: Vec::new(),
                verb: None
            }
            .validate()
            .is_err()
        );
        assert!(
            ThumbnailPayload {
                item_descriptor: vec![1],
                physical_size: 256,
                dpi: 144,
                cache_only: true
            }
            .validate()
            .is_ok()
        );
        assert!(
            NamespacePayload {
                container_descriptor: vec![1],
                generation: 2,
                maximum_items: 64,
                maximum_bytes: 65_536
            }
            .validate()
            .is_ok()
        );
        assert!(
            PreviewMessage::SetBounds {
                generation: 1,
                left: -10,
                top: 0,
                width: 800,
                height: 600,
                dpi: 120
            }
            .validate()
            .is_ok()
        );
        assert!(
            PreviewMessage::SetBounds {
                generation: 1,
                left: 0,
                top: 0,
                width: 0,
                height: 600,
                dpi: 120
            }
            .validate()
            .is_err()
        );
        assert!(
            OperationCapabilityPayload {
                operation: OperationClass::Thumbnail,
                item_descriptors: vec![vec![1]],
                duplicated_handles: vec![7],
                allow_window_hosting: false,
                allow_read_stream: true,
                allow_write: false,
            }
            .validate()
            .is_ok()
        );
        assert_eq!(
            OperationCapabilityPayload {
                operation: OperationClass::Preview,
                item_descriptors: vec![vec![1]],
                duplicated_handles: Vec::new(),
                allow_window_hosting: true,
                allow_read_stream: false,
                allow_write: true,
            }
            .validate(),
            Err(ProtocolError::Authority)
        );
    }

    #[test]
    fn preview_lifecycle_messages_round_trip_exactly_and_reject_stale_bytes() {
        let start = PreviewStartPayload {
            item_descriptor: b"owned-preview-item".to_vec(),
            generation: 7,
            parent_hwnd: 0x1234,
            left: -20,
            top: 10,
            width: 960,
            height: 540,
            dpi: 144,
        };
        let encoded_start = start.encode().expect("preview start");
        assert_eq!(PreviewStartPayload::decode(&encoded_start), Ok(start));
        let messages = [
            PreviewMessage::Lookup {
                descriptor: b"owned-shell-item".to_vec(),
                generation: 7,
            },
            PreviewMessage::Attach {
                generation: 7,
                parent_hwnd: 0x1234,
                left: -20,
                top: 10,
                width: 960,
                height: 540,
                dpi: 144,
            },
            PreviewMessage::SetBounds {
                generation: 7,
                left: 0,
                top: 0,
                width: 800,
                height: 450,
                dpi: 120,
            },
            PreviewMessage::SetFocus { generation: 7 },
            PreviewMessage::Accelerator {
                generation: 7,
                virtual_key: 0x09,
                modifiers: 1,
            },
            PreviewMessage::Unload { generation: 7 },
        ];
        for message in messages {
            let encoded = message.encode().expect("preview message");
            assert_eq!(PreviewMessage::decode(&encoded), Ok(message));
            let mut trailing = encoded;
            trailing.push(0);
            assert_eq!(
                PreviewMessage::decode(&trailing),
                Err(ProtocolError::Malformed)
            );
        }
        assert!(
            PreviewMessage::Attach {
                generation: 7,
                parent_hwnd: 0,
                left: 0,
                top: 0,
                width: 800,
                height: 450,
                dpi: 96,
            }
            .encode()
            .is_err()
        );
    }
}
