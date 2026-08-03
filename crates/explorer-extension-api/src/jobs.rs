//! FFI-safe synchronous extension-job transport.
//!
//! These types deliberately carry opaque host capabilities, generations, and
//! owned `abi_stable` data only. They never expose a path, native handle,
//! `Instant`, cancellation token, closure, future, or private model object.

use std::panic::{AssertUnwindSafe, catch_unwind};

use abi_stable::{
    StableAbi,
    std_types::{ROption, RString, RVec},
};

use crate::StableIdV1;

/// Maximum entries accepted atomically by one sink call.
pub const MAX_INCREMENTAL_RESULT_ITEMS_V1: usize = 1_024;
/// Maximum aggregate owned payload bytes accepted atomically by one sink call.
pub const MAX_INCREMENTAL_RESULT_BYTES_V1: usize = 1024 * 1024;
/// Maximum encoded payload for one public value.
pub const MAX_PLUGIN_VALUE_BYTES_V1: usize = 64 * 1024;

/// Opaque capability identifying one host-owned job generation.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, StableAbi)]
pub struct JobHandleV1 {
    nonce: [u8; 16],
    generation: u64,
}

impl JobHandleV1 {
    /// Builds a capability supplied by the host. Plugins must treat it as opaque.
    #[must_use]
    pub const fn from_host(nonce: [u8; 16], generation: u64) -> Self {
        Self { nonce, generation }
    }

    /// Returns the job generation for diagnostics and result tagging.
    #[must_use]
    pub const fn generation(self) -> u64 {
        self.generation
    }
}

/// Opaque generation-bound item capability.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, StableAbi)]
pub struct ItemHandleV1 {
    nonce: [u8; 16],
    generation: u64,
}

impl ItemHandleV1 {
    #[must_use]
    pub const fn from_host(nonce: [u8; 16], generation: u64) -> Self {
        Self { nonce, generation }
    }

    #[must_use]
    pub const fn generation(self) -> u64 {
        self.generation
    }
}

/// Opaque generation-bound location capability.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, StableAbi)]
pub struct LocationHandleV1 {
    nonce: [u8; 16],
    generation: u64,
}

/// Opaque per-invocation capability for a result sink.
///
/// The host issues it only while invoking a synchronous provider. A copied sink
/// or batch submitted after that invocation is rejected.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, StableAbi)]
pub struct SinkCapabilityV1 {
    nonce: [u8; 16],
}

impl SinkCapabilityV1 {
    #[must_use]
    pub const fn from_host(nonce: [u8; 16]) -> Self {
        Self { nonce }
    }
}

impl LocationHandleV1 {
    #[must_use]
    pub const fn from_host(nonce: [u8; 16], generation: u64) -> Self {
        Self { nonce, generation }
    }

    #[must_use]
    pub const fn generation(self) -> u64 {
        self.generation
    }
}

/// Non-exhaustive cooperative control state returned by the host.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct JobControlStateV1(u32);

impl JobControlStateV1 {
    pub const ACTIVE: Self = Self(1);
    pub const CANCELLED: Self = Self(2);
    pub const DEADLINE_ELAPSED: Self = Self(3);
    pub const CLOSED: Self = Self(4);

    #[must_use]
    pub const fn into_raw(self) -> u32 {
        self.0
    }

    #[must_use]
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }
}

/// Host function used by synchronous plugins to cooperatively poll a job.
#[repr(transparent)]
#[derive(Clone, Copy, StableAbi)]
pub struct JobControlPollV1(extern "C" fn(JobHandleV1) -> JobControlStateV1);

impl JobControlPollV1 {
    #[must_use]
    pub const fn from_host(callback: extern "C" fn(JobHandleV1) -> JobControlStateV1) -> Self {
        Self(callback)
    }

    #[must_use]
    pub fn poll(self, job: JobHandleV1) -> JobControlStateV1 {
        (self.0)(job)
    }
}

/// Fixed numeric transport kind. Constructors and sort semantics remain task 4.3.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct PluginValueKindV1(u32);

impl PluginValueKindV1 {
    pub const BOOL: Self = Self(1);
    pub const I64: Self = Self(2);
    pub const F64: Self = Self(3);
    pub const BYTES: Self = Self(4);
    /// Signed Unix nanoseconds.
    pub const TIME_UNIX_NANOS: Self = Self(5);
    /// Non-negative nanoseconds.
    pub const DURATION_NANOS: Self = Self(6);
    /// UTF-8 display text in [`PluginValueV1::text`].
    pub const TEXT: Self = Self(7);
    /// Provider-localized UTF-8 display text in [`PluginValueV1::text`].
    ///
    /// V1 carries the resolved string, never a host locale handle or callback.
    pub const LOCALIZED_TEXT: Self = Self(8);
    /// Canonical, whitespace-free JSON UTF-8 bytes in [`PluginValueV1::payload`].
    pub const STRUCTURED: Self = Self(9);
    /// Producer-scoped bytes using the supplied schema/version.
    pub const OPAQUE: Self = Self(10);

    #[must_use]
    pub const fn into_raw(self) -> u32 {
        self.0
    }

    #[must_use]
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }
}

/// Fixed value shell; host validation rejects malformed or oversized transport.
#[repr(C)]
#[derive(Clone, Debug, StableAbi)]
pub struct PluginValueV1 {
    pub kind: PluginValueKindV1,
    pub reserved: u32,
    pub integer: i64,
    pub float: f64,
    pub text: RString,
    pub payload: RVec<u8>,
    pub opaque_schema: StableIdV1,
    pub opaque_schema_version: u32,
    pub reserved_tail: u32,
}

impl PluginValueV1 {
    /// Validates the frozen v1 transport shape, not task-4.3 presentation/sort policy.
    ///
    /// # Errors
    ///
    /// Returns a sanitized code when untrusted ABI data violates the v1 shell.
    pub fn validate_transport(&self) -> Result<(), PluginValueTransportErrorV1> {
        if self.reserved != 0
            || self.reserved_tail != 0
            || self.payload.len() > MAX_PLUGIN_VALUE_BYTES_V1
            || self.text.len() > MAX_PLUGIN_VALUE_BYTES_V1
        {
            return Err(PluginValueTransportErrorV1::ReservedOrOversized);
        }
        let text_empty = self.text.is_empty();
        let payload_empty = self.payload.is_empty();
        let schema_empty = self.opaque_schema.namespace.into_raw() == 0
            && self.opaque_schema.value == 0
            && self.opaque_schema_version == 0;
        let float_zero = self.float.to_bits() == 0;
        match self.kind.into_raw() {
            raw if raw == PluginValueKindV1::BOOL.into_raw() => (self.integer == 0
                || self.integer == 1)
                .then_some(())
                .filter(|()| float_zero && text_empty && payload_empty && schema_empty)
                .ok_or(PluginValueTransportErrorV1::MalformedScalar),
            raw if raw == PluginValueKindV1::I64.into_raw()
                || raw == PluginValueKindV1::TIME_UNIX_NANOS.into_raw() =>
            {
                (float_zero && text_empty && payload_empty && schema_empty)
                    .then_some(())
                    .ok_or(PluginValueTransportErrorV1::MalformedScalar)
            }
            raw if raw == PluginValueKindV1::DURATION_NANOS.into_raw() => {
                (self.integer >= 0 && float_zero && text_empty && payload_empty && schema_empty)
                    .then_some(())
                    .ok_or(PluginValueTransportErrorV1::MalformedScalar)
            }
            raw if raw == PluginValueKindV1::F64.into_raw() => (self.float.is_finite()
                && self.integer == 0
                && text_empty
                && payload_empty
                && schema_empty)
                .then_some(())
                .ok_or(PluginValueTransportErrorV1::MalformedFloat),
            raw if raw == PluginValueKindV1::BYTES.into_raw() => {
                (self.integer == 0 && float_zero && text_empty && schema_empty)
                    .then_some(())
                    .ok_or(PluginValueTransportErrorV1::MalformedBytes)
            }
            raw if raw == PluginValueKindV1::TEXT.into_raw()
                || raw == PluginValueKindV1::LOCALIZED_TEXT.into_raw() =>
            {
                (self.integer == 0 && float_zero && payload_empty && schema_empty)
                    .then_some(())
                    .ok_or(PluginValueTransportErrorV1::MalformedText)
            }
            raw if raw == PluginValueKindV1::STRUCTURED.into_raw() => {
                if self.integer != 0 || !float_zero || !text_empty || !schema_empty {
                    return Err(PluginValueTransportErrorV1::MalformedStructured);
                }
                is_canonical_structured_json(&self.payload)
                    .then_some(())
                    .ok_or(PluginValueTransportErrorV1::MalformedStructured)
            }
            raw if raw == PluginValueKindV1::OPAQUE.into_raw() => (self.integer == 0
                && float_zero
                && text_empty
                && self.opaque_schema.is_valid()
                && self.opaque_schema_version != 0)
                .then_some(())
                .ok_or(PluginValueTransportErrorV1::MalformedOpaque),
            _ => Err(PluginValueTransportErrorV1::UnknownKind),
        }
    }
}

fn is_canonical_structured_json(payload: &[u8]) -> bool {
    let mut parser = CanonicalJsonParserV1::new(payload);
    let Ok(value) = parser.parse_value(0) else {
        return false;
    };
    if parser.position != payload.len() {
        return false;
    }
    let mut canonical = Vec::with_capacity(payload.len());
    encode_canonical_json_v1(&value, &mut canonical);
    canonical == payload
}

/// `SuperExplorer` Canonical JSON V1. It intentionally supports a fixed subset:
/// null, booleans, UTF-8 strings, arrays, byte-sorted UTF-8 object keys, and
/// minimal `i64`/`u64` decimal integers. Whitespace, duplicate keys, `\u`
/// escapes, fractional/exponent numbers, and non-minimal integer spellings are
/// rejected. Strings use raw UTF-8 and only the short escapes `\"`, `\\`,
/// `\b`, `\f`, `\n`, `\r`, and `\t`; encoder output orders object keys by their
/// UTF-8 bytes. This parser/encoder, not a serde version, defines V1 acceptance.
#[derive(Debug)]
enum CanonicalJsonValueV1 {
    Null,
    Bool(bool),
    String(Vec<u8>),
    I64(i64),
    U64(u64),
    Array(Vec<Self>),
    Object(Vec<(Vec<u8>, Self)>),
}

struct CanonicalJsonParserV1<'a> {
    input: &'a [u8],
    position: usize,
    elements: usize,
}

impl<'a> CanonicalJsonParserV1<'a> {
    const MAX_DEPTH: usize = 32;
    const MAX_ELEMENTS: usize = 1_024;

    const fn new(input: &'a [u8]) -> Self {
        Self {
            input,
            position: 0,
            elements: 0,
        }
    }

    fn parse_value(&mut self, depth: usize) -> Result<CanonicalJsonValueV1, ()> {
        if depth > Self::MAX_DEPTH || self.elements >= Self::MAX_ELEMENTS {
            return Err(());
        }
        self.elements += 1;
        match self.peek() {
            Some(b'n') => self.literal(b"null", CanonicalJsonValueV1::Null),
            Some(b't') => self.literal(b"true", CanonicalJsonValueV1::Bool(true)),
            Some(b'f') => self.literal(b"false", CanonicalJsonValueV1::Bool(false)),
            Some(b'\"') => self.parse_string().map(CanonicalJsonValueV1::String),
            Some(b'[') => self.parse_array(depth + 1),
            Some(b'{') => self.parse_object(depth + 1),
            Some(b'-' | b'0'..=b'9') => self.parse_integer(),
            _ => Err(()),
        }
    }

    fn literal(
        &mut self,
        literal: &[u8],
        value: CanonicalJsonValueV1,
    ) -> Result<CanonicalJsonValueV1, ()> {
        self.input
            .get(self.position..self.position + literal.len())
            .filter(|candidate| *candidate == literal)
            .ok_or(())?;
        self.position += literal.len();
        Ok(value)
    }

    fn parse_string(&mut self) -> Result<Vec<u8>, ()> {
        if self.take() != Some(b'\"') {
            return Err(());
        }
        let mut output = Vec::new();
        loop {
            let byte = self.take().ok_or(())?;
            match byte {
                b'\"' => {
                    std::str::from_utf8(&output).map_err(|_| ())?;
                    return Ok(output);
                }
                0..=0x1f => return Err(()),
                b'\\' => match self.take().ok_or(())? {
                    b'\"' => output.push(b'\"'),
                    b'\\' => output.push(b'\\'),
                    b'b' => output.push(0x08),
                    b'f' => output.push(0x0c),
                    b'n' => output.push(b'\n'),
                    b'r' => output.push(b'\r'),
                    b't' => output.push(b'\t'),
                    _ => return Err(()),
                },
                other => output.push(other),
            }
        }
    }

    fn parse_array(&mut self, depth: usize) -> Result<CanonicalJsonValueV1, ()> {
        self.take();
        let mut values = Vec::new();
        if self.peek() == Some(b']') {
            self.take();
            return Ok(CanonicalJsonValueV1::Array(values));
        }
        loop {
            values.push(self.parse_value(depth)?);
            match self.take() {
                Some(b',') => {}
                Some(b']') => return Ok(CanonicalJsonValueV1::Array(values)),
                _ => return Err(()),
            }
        }
    }

    fn parse_object(&mut self, depth: usize) -> Result<CanonicalJsonValueV1, ()> {
        self.take();
        let mut values = Vec::new();
        if self.peek() == Some(b'}') {
            self.take();
            return Ok(CanonicalJsonValueV1::Object(values));
        }
        loop {
            let key = self.parse_string()?;
            if self.take() != Some(b':') {
                return Err(());
            }
            values.push((key, self.parse_value(depth)?));
            match self.take() {
                Some(b',') => {}
                Some(b'}') => break,
                _ => return Err(()),
            }
        }
        values.sort_unstable_by(|left, right| left.0.cmp(&right.0));
        if values.windows(2).any(|pair| pair[0].0 == pair[1].0) {
            return Err(());
        }
        Ok(CanonicalJsonValueV1::Object(values))
    }

    fn parse_integer(&mut self) -> Result<CanonicalJsonValueV1, ()> {
        let start = self.position;
        if self.peek() == Some(b'-') {
            self.take();
            if !matches!(self.peek(), Some(b'1'..=b'9')) {
                return Err(());
            }
        } else if self.peek() == Some(b'0') {
            self.take();
            if matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(());
            }
        } else if matches!(self.peek(), Some(b'1'..=b'9')) {
            self.take();
        } else {
            return Err(());
        }
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.take();
        }
        let token = std::str::from_utf8(&self.input[start..self.position]).map_err(|_| ())?;
        if token.starts_with('-') {
            token
                .parse::<i64>()
                .map(CanonicalJsonValueV1::I64)
                .map_err(|_| ())
        } else {
            token
                .parse::<u64>()
                .map(CanonicalJsonValueV1::U64)
                .map_err(|_| ())
        }
    }

    fn peek(&self) -> Option<u8> {
        self.input.get(self.position).copied()
    }
    fn take(&mut self) -> Option<u8> {
        let byte = self.peek()?;
        self.position += 1;
        Some(byte)
    }
}

fn encode_canonical_json_v1(value: &CanonicalJsonValueV1, output: &mut Vec<u8>) {
    match value {
        CanonicalJsonValueV1::Null => output.extend_from_slice(b"null"),
        CanonicalJsonValueV1::Bool(value) => {
            output.extend_from_slice(if *value { b"true" } else { b"false" });
        }
        CanonicalJsonValueV1::String(value) => encode_json_string_v1(value, output),
        CanonicalJsonValueV1::I64(value) => output.extend_from_slice(value.to_string().as_bytes()),
        CanonicalJsonValueV1::U64(value) => output.extend_from_slice(value.to_string().as_bytes()),
        CanonicalJsonValueV1::Array(values) => {
            output.push(b'[');
            for (index, value) in values.iter().enumerate() {
                if index != 0 {
                    output.push(b',');
                }
                encode_canonical_json_v1(value, output);
            }
            output.push(b']');
        }
        CanonicalJsonValueV1::Object(values) => {
            output.push(b'{');
            for (index, (key, value)) in values.iter().enumerate() {
                if index != 0 {
                    output.push(b',');
                }
                encode_json_string_v1(key, output);
                output.push(b':');
                encode_canonical_json_v1(value, output);
            }
            output.push(b'}');
        }
    }
}

fn encode_json_string_v1(value: &[u8], output: &mut Vec<u8>) {
    output.push(b'\"');
    for byte in value {
        match byte {
            b'\"' => output.extend_from_slice(b"\\\""),
            b'\\' => output.extend_from_slice(b"\\\\"),
            0x08 => output.extend_from_slice(b"\\b"),
            0x0c => output.extend_from_slice(b"\\f"),
            b'\n' => output.extend_from_slice(b"\\n"),
            b'\r' => output.extend_from_slice(b"\\r"),
            b'\t' => output.extend_from_slice(b"\\t"),
            _ => output.push(*byte),
        }
    }
    output.push(b'\"');
}

/// Sanitized structural rejection from the public value transport.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PluginValueTransportErrorV1 {
    ReservedOrOversized,
    MalformedScalar,
    MalformedFloat,
    MalformedBytes,
    MalformedText,
    MalformedStructured,
    MalformedOpaque,
    UnknownKind,
}

/// One generation-tagged incremental result entry.
#[repr(C)]
#[derive(Clone, Debug, StableAbi)]
pub struct IncrementalResultEntryV1 {
    pub item: ItemHandleV1,
    pub item_generation: u64,
    pub source_generation: u64,
    pub value: PluginValueV1,
}

/// Owned all-or-nothing sink batch. Sequence numbers are per job and monotonic.
#[repr(C)]
#[derive(Clone, Debug, StableAbi)]
pub struct IncrementalResultBatchV1 {
    pub job: JobHandleV1,
    pub sink_capability: SinkCapabilityV1,
    pub job_generation: u64,
    pub location: LocationHandleV1,
    pub location_generation: u64,
    pub source_generation: u64,
    pub sequence: u64,
    pub entries: RVec<IncrementalResultEntryV1>,
}

/// Non-exhaustive nonblocking sink outcome.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct SinkSubmitStatusV1(u32);

impl SinkSubmitStatusV1 {
    pub const ACCEPTED: Self = Self(1);
    pub const WOULD_BLOCK: Self = Self(2);
    pub const STALE: Self = Self(3);
    pub const CLOSED: Self = Self(4);
    pub const WRONG_THREAD: Self = Self(5);
    pub const INVALID: Self = Self(6);

    #[must_use]
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    #[must_use]
    pub const fn into_raw(self) -> u32 {
        self.0
    }
}

/// Sink result; rejected batches are returned unchanged and consume no credits.
#[repr(C)]
#[derive(Clone, Debug, StableAbi)]
pub struct SinkSubmitOutcomeV1 {
    pub status: SinkSubmitStatusV1,
    pub remaining_batch_credits: u32,
    pub remaining_item_credits: u32,
    pub remaining_byte_credits: u64,
    pub rejected_batch: ROption<IncrementalResultBatchV1>,
}

/// FFI callback that consumes a batch only on [`SinkSubmitStatusV1::ACCEPTED`].
#[repr(transparent)]
#[derive(Clone, Copy, StableAbi)]
pub struct IncrementalResultSubmitV1(
    extern "C" fn(IncrementalResultBatchV1) -> SinkSubmitOutcomeV1,
);

impl IncrementalResultSubmitV1 {
    #[must_use]
    pub const fn from_host(
        callback: extern "C" fn(IncrementalResultBatchV1) -> SinkSubmitOutcomeV1,
    ) -> Self {
        Self(callback)
    }

    #[must_use]
    pub fn try_submit(self, batch: IncrementalResultBatchV1) -> SinkSubmitOutcomeV1 {
        (self.0)(batch)
    }
}

/// Capability-bound result sink valid only for the enclosing synchronous call.
#[repr(C)]
#[derive(Clone, Copy, StableAbi)]
pub struct IncrementalResultSinkV1 {
    pub job: JobHandleV1,
    pub capability: SinkCapabilityV1,
    pub submit: IncrementalResultSubmitV1,
}

/// Fixed progress update for one synchronous job invocation.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct JobProgressUpdateV1 {
    pub job: JobHandleV1,
    pub sink_capability: SinkCapabilityV1,
    pub job_generation: u64,
    pub item_generation: u64,
    pub location_generation: u64,
    pub source_generation: u64,
    pub sequence: u64,
    pub completed_units: u64,
    pub total_units: u64,
    pub reserved: u32,
}

/// Non-exhaustive result of a nonblocking progress submission.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct JobProgressStatusV1(u32);

impl JobProgressStatusV1 {
    pub const ACCEPTED: Self = Self(1);
    pub const STALE: Self = Self(2);
    pub const CLOSED: Self = Self(3);
    pub const WRONG_THREAD: Self = Self(4);
    pub const INVALID: Self = Self(5);

    #[must_use]
    pub const fn into_raw(self) -> u32 {
        self.0
    }

    #[must_use]
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }
}

/// FFI callback for bounded, latest-wins progress updates.
#[repr(transparent)]
#[derive(Clone, Copy, StableAbi)]
pub struct JobProgressSubmitV1(extern "C" fn(JobProgressUpdateV1) -> JobProgressStatusV1);

impl JobProgressSubmitV1 {
    #[must_use]
    pub const fn from_host(
        callback: extern "C" fn(JobProgressUpdateV1) -> JobProgressStatusV1,
    ) -> Self {
        Self(callback)
    }

    #[must_use]
    pub fn try_submit(self, update: JobProgressUpdateV1) -> JobProgressStatusV1 {
        (self.0)(update)
    }
}

/// Capability-bound, latest-wins progress sink for the enclosing provider call.
#[repr(C)]
#[derive(Clone, Copy, StableAbi)]
pub struct JobProgressSinkV1 {
    pub job: JobHandleV1,
    pub capability: SinkCapabilityV1,
    pub submit: JobProgressSubmitV1,
}

impl JobProgressSinkV1 {
    #[must_use]
    pub fn try_submit(self, update: JobProgressUpdateV1) -> JobProgressStatusV1 {
        self.submit.try_submit(update)
    }
}

impl IncrementalResultSinkV1 {
    #[must_use]
    pub fn try_submit(self, batch: IncrementalResultBatchV1) -> SinkSubmitOutcomeV1 {
        self.submit.try_submit(batch)
    }
}

/// Immutable ABI context for one synchronous provider callback.
#[repr(C)]
#[derive(Clone, Copy, StableAbi)]
pub struct JobContextV1 {
    pub job: JobHandleV1,
    pub item: ROption<ItemHandleV1>,
    pub location: LocationHandleV1,
    pub feature_epoch: u64,
    pub job_generation: u64,
    pub item_generation: u64,
    pub location_generation: u64,
    pub source_generation: u64,
    pub control_poll: JobControlPollV1,
    pub sink: IncrementalResultSinkV1,
    pub progress: JobProgressSinkV1,
}

/// Non-exhaustive typed terminal returned by a synchronous provider callback.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct JobTerminalV1(u32);

impl JobTerminalV1 {
    pub const COMPLETED: Self = Self(1);
    pub const UNSUPPORTED: Self = Self(2);
    pub const UNAVAILABLE: Self = Self(3);
    pub const CANCELLED: Self = Self(4);
    pub const DEADLINE_ELAPSED: Self = Self(5);
    /// Provider-selected terminal after it elects to stop; a sink `WOULD_BLOCK`
    /// response itself never terminates a job and consumes no credits.
    pub const BACKPRESSURED: Self = Self(6);
    pub const PLUGIN_ERROR: Self = Self(7);
    pub const INCOMPATIBLE: Self = Self(8);
    pub const PANICKED: Self = Self(9);

    #[must_use]
    pub const fn into_raw(self) -> u32 {
        self.0
    }

    #[must_use]
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    /// Reports whether this is a v1 terminal code the host can safely publish.
    #[must_use]
    pub const fn is_known(self) -> bool {
        matches!(self.0, 1..=9)
    }
}

/// Static implementation trait; no Rust trait object crosses the ABI.
pub trait JobProviderImplementationV1 {
    fn run(context: JobContextV1) -> JobTerminalV1;
}

/// SDK-owned panic-translating synchronous provider callback.
#[repr(transparent)]
#[derive(Clone, Copy, StableAbi)]
pub struct JobProviderCallbackV1(extern "C" fn(JobContextV1) -> JobTerminalV1);

impl JobProviderCallbackV1 {
    #[must_use]
    pub fn new<T: JobProviderImplementationV1>() -> Self {
        Self(job_provider_trampoline::<T>)
    }

    #[must_use]
    pub fn invoke(self, context: JobContextV1) -> JobTerminalV1 {
        (self.0)(context)
    }
}

extern "C" fn job_provider_trampoline<T: JobProviderImplementationV1>(
    context: JobContextV1,
) -> JobTerminalV1 {
    match catch_unwind(AssertUnwindSafe(|| T::run(context))) {
        Ok(terminal) => terminal,
        Err(_) => JobTerminalV1::PANICKED,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::IdNamespaceV1;

    fn empty_value(kind: PluginValueKindV1) -> PluginValueV1 {
        PluginValueV1 {
            kind,
            reserved: 0,
            integer: 0,
            float: 0.0,
            text: RString::new(),
            payload: RVec::new(),
            opaque_schema: StableIdV1::new(IdNamespaceV1::new(0, 0), 0),
            opaque_schema_version: 0,
            reserved_tail: 0,
        }
    }

    #[test]
    fn structured_values_require_canonical_json_and_floats_are_finite() {
        let mut structured = empty_value(PluginValueKindV1::STRUCTURED);
        structured.payload = RVec::from(b"{\"a\":1}".as_slice());
        assert!(structured.validate_transport().is_ok());
        structured.payload = RVec::from(b"{ \"a\": 1 }".as_slice());
        assert_eq!(
            structured.validate_transport(),
            Err(PluginValueTransportErrorV1::MalformedStructured)
        );

        let mut float = empty_value(PluginValueKindV1::F64);
        float.float = f64::NAN;
        assert_eq!(
            float.validate_transport(),
            Err(PluginValueTransportErrorV1::MalformedFloat)
        );
    }

    #[test]
    fn canonical_json_v1_rejects_ambiguous_syntax_and_is_implementation_owned() {
        assert!(is_canonical_structured_json(
            br#"{"a":[true,null,-1,18446744073709551615],"b":"line\n"}"#
        ));
        for malformed in [
            br#"{"b":1,"a":2}"#.as_slice(),
            br#"{"a":1,"a":2}"#.as_slice(),
            br#"{"a":"\u0061"}"#.as_slice(),
            br#"{"a":1.0}"#.as_slice(),
            br#"{"a":1e0}"#.as_slice(),
            br#"{"a":-0}"#.as_slice(),
            br#"{ "a":1}"#.as_slice(),
        ] {
            assert!(!is_canonical_structured_json(malformed));
        }
        let too_deep = format!("{}0{}", "[".repeat(33), "]".repeat(33));
        assert!(!is_canonical_structured_json(too_deep.as_bytes()));
        let too_many = format!("[{}]", vec!["0"; 1_025].join(","));
        assert!(!is_canonical_structured_json(too_many.as_bytes()));
    }

    #[test]
    fn unused_wire_bits_are_canonical_zeroes() {
        for kind in [
            PluginValueKindV1::BOOL,
            PluginValueKindV1::I64,
            PluginValueKindV1::BYTES,
            PluginValueKindV1::TEXT,
            PluginValueKindV1::STRUCTURED,
        ] {
            let mut value = empty_value(kind);
            if kind == PluginValueKindV1::STRUCTURED {
                value.payload = RVec::from(br#"{"a":1}"#.as_slice());
            }
            value.float = -0.0;
            assert!(value.validate_transport().is_err());
        }
        let mut text = empty_value(PluginValueKindV1::TEXT);
        text.opaque_schema = StableIdV1::new(IdNamespaceV1::new(1, 1), 0);
        assert!(text.validate_transport().is_err());
    }

    #[test]
    fn terminal_codes_are_closed_over_v1() {
        assert!(JobTerminalV1::COMPLETED.is_known());
        assert!(!JobTerminalV1::from_raw(99).is_known());
    }

    #[test]
    fn progress_status_codes_and_fixed_update_shape_are_stable() {
        assert_eq!(JobProgressStatusV1::ACCEPTED.into_raw(), 1);
        assert_eq!(JobProgressStatusV1::INVALID.into_raw(), 5);
        let update = JobProgressUpdateV1 {
            job: JobHandleV1::from_host([1; 16], 1),
            sink_capability: SinkCapabilityV1::from_host([2; 16]),
            job_generation: 1,
            item_generation: 1,
            location_generation: 1,
            source_generation: 1,
            sequence: 0,
            completed_units: 0,
            total_units: 1,
            reserved: 0,
        };
        assert_eq!(update.total_units, 1);
    }

    #[test]
    fn text_values_have_the_same_per_value_byte_cap_as_payloads() {
        let mut text = empty_value(PluginValueKindV1::TEXT);
        text.text = RString::from("x".repeat(MAX_PLUGIN_VALUE_BYTES_V1 + 1));
        assert_eq!(
            text.validate_transport(),
            Err(PluginValueTransportErrorV1::ReservedOrOversized)
        );
    }
}
