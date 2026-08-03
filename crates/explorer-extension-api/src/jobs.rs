//! FFI-safe synchronous extension-job transport.
//!
//! These types deliberately carry opaque host capabilities, generations, and
//! owned `abi_stable` data only. They never expose a path, native handle,
//! `Instant`, cancellation token, closure, future, or private model object.

use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::atomic::{AtomicU8, Ordering},
};

use abi_stable::{
    StableAbi, sabi_trait,
    std_types::{RArc, RBox, ROption, RString, RVec},
};

use crate::{StableIdV1, dispose_caught_panic_payload_v1};

/// Maximum entries accepted atomically by one sink call.
pub const MAX_INCREMENTAL_RESULT_ITEMS_V1: usize = 1_024;
/// Maximum aggregate owned payload bytes accepted atomically by one sink call.
pub const MAX_INCREMENTAL_RESULT_BYTES_V1: usize = 1024 * 1024;
/// Maximum encoded payload for one public value.
pub const MAX_PLUGIN_VALUE_BYTES_V1: usize = 64 * 1024;
/// Largest byte vector the host may allocate and return from one stream read.
pub const MAX_INPUT_STREAM_READ_BYTES_V1: u32 = 64 * 1024;
/// Maximum host-attested file items delivered to one code-column callback.
///
/// The limit is deliberately fixed at the ABI boundary so a plugin cannot turn
/// a Details refresh into an unbounded allocation or callback. Hosts may elect
/// to issue a smaller batch.
pub const MAX_BATCH_COLUMN_ITEMS_V1: usize = 128;
/// Largest UTF-8 basename supplied for one batch-column item.
pub const MAX_BATCH_COLUMN_FILE_NAME_BYTES_V1: usize = 255;

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

/// Fixed numeric value kind with validated constructors and host-defined sort semantics.
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

#[allow(clippy::double_must_use, clippy::missing_errors_doc)]
impl PluginValueV1 {
    const fn empty(kind: PluginValueKindV1) -> Self {
        Self {
            kind,
            reserved: 0,
            integer: 0,
            float: 0.0,
            text: RString::new(),
            payload: RVec::new(),
            opaque_schema: StableIdV1::new(crate::IdNamespaceV1::new(0, 0), 0),
            opaque_schema_version: 0,
            reserved_tail: 0,
        }
    }

    #[must_use]
    pub const fn boolean(value: bool) -> Self {
        let mut result = Self::empty(PluginValueKindV1::BOOL);
        result.integer = value as i64;
        result
    }
    #[must_use]
    pub const fn integer(value: i64) -> Self {
        let mut result = Self::empty(PluginValueKindV1::I64);
        result.integer = value;
        result
    }
    #[must_use]
    pub fn float(value: f64) -> Result<Self, PluginValueTransportErrorV1> {
        if !value.is_finite() {
            return Err(PluginValueTransportErrorV1::MalformedFloat);
        }
        let mut result = Self::empty(PluginValueKindV1::F64);
        result.float = if value == 0.0 { 0.0 } else { value };
        Ok(result)
    }
    #[must_use]
    pub fn bytes(value: impl Into<RVec<u8>>) -> Result<Self, PluginValueTransportErrorV1> {
        let mut result = Self::empty(PluginValueKindV1::BYTES);
        result.payload = value.into();
        result.validate_transport()?;
        Ok(result)
    }
    #[must_use]
    pub const fn time_unix_nanos(value: i64) -> Self {
        let mut result = Self::empty(PluginValueKindV1::TIME_UNIX_NANOS);
        result.integer = value;
        result
    }
    #[must_use]
    pub fn duration_nanos(value: u64) -> Result<Self, PluginValueTransportErrorV1> {
        let integer =
            i64::try_from(value).map_err(|_| PluginValueTransportErrorV1::MalformedScalar)?;
        let mut result = Self::empty(PluginValueKindV1::DURATION_NANOS);
        result.integer = integer;
        Ok(result)
    }
    #[must_use]
    pub fn text(value: impl Into<RString>) -> Result<Self, PluginValueTransportErrorV1> {
        let mut result = Self::empty(PluginValueKindV1::TEXT);
        result.text = value.into();
        result.validate_transport()?;
        Ok(result)
    }
    #[must_use]
    pub fn localized_text(value: impl Into<RString>) -> Result<Self, PluginValueTransportErrorV1> {
        let mut result = Self::empty(PluginValueKindV1::LOCALIZED_TEXT);
        result.text = value.into();
        result.validate_transport()?;
        Ok(result)
    }
    #[must_use]
    pub fn structured_canonical_json(
        value: impl Into<RVec<u8>>,
    ) -> Result<Self, PluginValueTransportErrorV1> {
        let mut result = Self::empty(PluginValueKindV1::STRUCTURED);
        result.payload = value.into();
        result.validate_transport()?;
        Ok(result)
    }
    #[must_use]
    pub fn opaque(
        schema: StableIdV1,
        schema_version: u32,
        value: impl Into<RVec<u8>>,
    ) -> Result<Self, PluginValueTransportErrorV1> {
        let mut result = Self::empty(PluginValueKindV1::OPAQUE);
        result.payload = value.into();
        result.opaque_schema = schema;
        result.opaque_schema_version = schema_version;
        result.validate_transport()?;
        Ok(result)
    }

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

/// Fixed semantic domain for a stable host sort key. It deliberately excludes
/// structured and opaque values: neither has a host-generic ordering.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct StableSortValueKindV1(u32);

impl StableSortValueKindV1 {
    pub const BOOL: Self = Self(1);
    pub const I64: Self = Self(2);
    pub const U64: Self = Self(3);
    pub const F64: Self = Self(4);
    pub const TIME_UNIX_NANOS: Self = Self(5);
    pub const DURATION_NANOS: Self = Self(6);
    pub const TEXT: Self = Self(7);
    pub const BYTES: Self = Self(8);
    #[must_use]
    pub const fn into_raw(self) -> u32 {
        self.0
    }
    #[must_use]
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }
    #[must_use]
    pub const fn is_known(self) -> bool {
        matches!(self.0, 1..=8)
    }
}

/// Canonical, display-independent stable sort key. Unused members are exactly
/// zero so the host can compare a validated copy without callbacks or parsing.
#[repr(C)]
#[derive(Clone, Debug, StableAbi)]
pub struct StableSortValueV1 {
    pub kind: StableSortValueKindV1,
    pub reserved: u32,
    pub signed: i64,
    pub unsigned: u64,
    pub float: f64,
    pub text: RString,
    pub bytes: RVec<u8>,
    pub reserved_tail: u32,
}

#[allow(clippy::double_must_use, clippy::missing_errors_doc)]
impl StableSortValueV1 {
    const fn empty(kind: StableSortValueKindV1) -> Self {
        Self {
            kind,
            reserved: 0,
            signed: 0,
            unsigned: 0,
            float: 0.0,
            text: RString::new(),
            bytes: RVec::new(),
            reserved_tail: 0,
        }
    }
    #[must_use]
    pub const fn boolean(value: bool) -> Self {
        let mut result = Self::empty(StableSortValueKindV1::BOOL);
        result.unsigned = value as u64;
        result
    }
    #[must_use]
    pub const fn integer(value: i64) -> Self {
        let mut result = Self::empty(StableSortValueKindV1::I64);
        result.signed = value;
        result
    }
    #[must_use]
    pub const fn unsigned(value: u64) -> Self {
        let mut result = Self::empty(StableSortValueKindV1::U64);
        result.unsigned = value;
        result
    }
    #[must_use]
    pub fn float(value: f64) -> Result<Self, StableSortValueTransportErrorV1> {
        if !value.is_finite() {
            return Err(StableSortValueTransportErrorV1::MalformedFloat);
        }
        let mut result = Self::empty(StableSortValueKindV1::F64);
        result.float = if value == 0.0 { 0.0 } else { value };
        Ok(result)
    }
    #[must_use]
    pub const fn time_unix_nanos(value: i64) -> Self {
        let mut result = Self::empty(StableSortValueKindV1::TIME_UNIX_NANOS);
        result.signed = value;
        result
    }
    #[must_use]
    pub const fn duration_nanos(value: u64) -> Self {
        let mut result = Self::empty(StableSortValueKindV1::DURATION_NANOS);
        result.unsigned = value;
        result
    }
    #[must_use]
    pub fn text(value: impl Into<RString>) -> Result<Self, StableSortValueTransportErrorV1> {
        let mut result = Self::empty(StableSortValueKindV1::TEXT);
        result.text = value.into();
        result.validate_transport()?;
        Ok(result)
    }
    #[must_use]
    pub fn bytes(value: impl Into<RVec<u8>>) -> Result<Self, StableSortValueTransportErrorV1> {
        let mut result = Self::empty(StableSortValueKindV1::BYTES);
        result.bytes = value.into();
        result.validate_transport()?;
        Ok(result)
    }
    pub fn validate_transport(&self) -> Result<(), StableSortValueTransportErrorV1> {
        if self.reserved != 0
            || self.reserved_tail != 0
            || self.text.len() > MAX_PLUGIN_VALUE_BYTES_V1
            || self.bytes.len() > MAX_PLUGIN_VALUE_BYTES_V1
        {
            return Err(StableSortValueTransportErrorV1::ReservedOrOversized);
        }
        let zero = self.float.to_bits() == 0;
        match self.kind.into_raw() {
            1 => (self.unsigned <= 1
                && self.signed == 0
                && zero
                && self.text.is_empty()
                && self.bytes.is_empty())
            .then_some(())
            .ok_or(StableSortValueTransportErrorV1::MalformedScalar),
            2 | 5 => (self.unsigned == 0 && zero && self.text.is_empty() && self.bytes.is_empty())
                .then_some(())
                .ok_or(StableSortValueTransportErrorV1::MalformedScalar),
            3 | 6 => (self.signed == 0 && zero && self.text.is_empty() && self.bytes.is_empty())
                .then_some(())
                .ok_or(StableSortValueTransportErrorV1::MalformedScalar),
            4 => (self.float.is_finite()
                && self.float.to_bits() != (-0.0f64).to_bits()
                && self.signed == 0
                && self.unsigned == 0
                && self.text.is_empty()
                && self.bytes.is_empty())
            .then_some(())
            .ok_or(StableSortValueTransportErrorV1::MalformedFloat),
            7 => (self.signed == 0 && self.unsigned == 0 && zero && self.bytes.is_empty())
                .then_some(())
                .ok_or(StableSortValueTransportErrorV1::MalformedText),
            8 => (self.signed == 0 && self.unsigned == 0 && zero && self.text.is_empty())
                .then_some(())
                .ok_or(StableSortValueTransportErrorV1::MalformedBytes),
            _ => Err(StableSortValueTransportErrorV1::UnknownKind),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StableSortValueTransportErrorV1 {
    ReservedOrOversized,
    MalformedScalar,
    MalformedFloat,
    MalformedText,
    MalformedBytes,
    UnknownKind,
}

/// Semantic item outcome. These are valid provider results, not sink failures.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct PluginItemOutcomeV1(u32);
impl PluginItemOutcomeV1 {
    pub const VALUE: Self = Self(1);
    pub const UNSUPPORTED: Self = Self(2);
    pub const UNAVAILABLE: Self = Self(3);
    pub const CANCELLED: Self = Self(4);
    pub const PLUGIN_ERROR: Self = Self(5);
    pub const INCOMPATIBLE: Self = Self(6);
    #[must_use]
    pub const fn into_raw(self) -> u32 {
        self.0
    }
    #[must_use]
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }
    #[must_use]
    pub const fn is_known(self) -> bool {
        matches!(self.0, 1..=6)
    }
}

/// One value outcome with optional display and sort data.
#[repr(C)]
#[derive(Clone, Debug, StableAbi)]
pub struct PluginItemResultV1 {
    pub outcome: PluginItemOutcomeV1,
    pub value: ROption<PluginValueV1>,
    pub stable_sort: ROption<StableSortValueV1>,
    pub reserved: u32,
}
#[allow(clippy::missing_errors_doc)]
impl PluginItemResultV1 {
    #[must_use]
    pub fn value(value: PluginValueV1, stable_sort: ROption<StableSortValueV1>) -> Self {
        Self {
            outcome: PluginItemOutcomeV1::VALUE,
            value: ROption::RSome(value),
            stable_sort,
            reserved: 0,
        }
    }
    #[must_use]
    pub const fn absent(outcome: PluginItemOutcomeV1) -> Self {
        Self {
            outcome,
            value: ROption::RNone,
            stable_sort: ROption::RNone,
            reserved: 0,
        }
    }
    pub fn validate_transport(
        &self,
        expected_sort: ROption<StableSortValueKindV1>,
    ) -> Result<usize, PluginItemResultTransportErrorV1> {
        if self.reserved != 0 || !self.outcome.is_known() {
            return Err(PluginItemResultTransportErrorV1::MalformedOutcome);
        }
        match (self.outcome.into_raw(), &self.value, &self.stable_sort) {
            (1, ROption::RSome(value), sort) => {
                value
                    .validate_transport()
                    .map_err(|_| PluginItemResultTransportErrorV1::Value)?;
                let mut bytes = value
                    .text
                    .len()
                    .checked_add(value.payload.len())
                    .ok_or(PluginItemResultTransportErrorV1::Oversized)?;
                match (expected_sort, sort) {
                    (ROption::RNone, ROption::RNone) => {}
                    (ROption::RSome(expected), ROption::RSome(actual))
                        if actual.kind == expected =>
                    {
                        actual
                            .validate_transport()
                            .map_err(|_| PluginItemResultTransportErrorV1::Sort)?;
                        bytes = bytes
                            .checked_add(actual.text.len())
                            .and_then(|x| x.checked_add(actual.bytes.len()))
                            .ok_or(PluginItemResultTransportErrorV1::Oversized)?;
                    }
                    _ => return Err(PluginItemResultTransportErrorV1::SortContract),
                }
                Ok(bytes)
            }
            (1, _, _) => Err(PluginItemResultTransportErrorV1::MalformedOutcome),
            (_, ROption::RNone, ROption::RNone) => Ok(0),
            _ => Err(PluginItemResultTransportErrorV1::MalformedOutcome),
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PluginItemResultTransportErrorV1 {
    MalformedOutcome,
    Value,
    Sort,
    SortContract,
    Oversized,
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
    pub result: PluginItemResultV1,
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

/// Stateful Rust-ABI host service object for one synchronous provider call.
/// It is an `abi_stable` trait object, not a handwritten C callback table.
#[sabi_trait]
#[doc(hidden)]
pub trait AbiJobHostServicesV1: Send + Sync + Clone {
    fn poll_control(&self) -> JobControlStateV1;
    fn submit_results(&self, batch: IncrementalResultBatchV1) -> SinkSubmitOutcomeV1;
    #[sabi(last_prefix_field)]
    fn submit_progress(&self, update: JobProgressUpdateV1) -> JobProgressStatusV1;
}

/// Opaque capability-bound service object retained by [`JobContextV1`].
#[repr(transparent)]
#[derive(Clone, StableAbi)]
pub struct JobHostServicesV1(AbiJobHostServicesV1_TO<'static, RArc<()>>);

impl JobHostServicesV1 {
    #[doc(hidden)]
    pub fn from_host<T>(services: T) -> Self
    where
        T: AbiJobHostServicesV1 + 'static,
    {
        Self(AbiJobHostServicesV1_TO::from_ptr(
            RArc::new(services),
            abi_stable::sabi_trait::TD_Opaque,
        ))
    }

    #[must_use]
    pub fn poll_control(&self) -> JobControlStateV1 {
        self.0.poll_control()
    }

    #[must_use]
    pub fn try_submit(&self, batch: IncrementalResultBatchV1) -> SinkSubmitOutcomeV1 {
        self.0.submit_results(batch)
    }

    #[must_use]
    pub fn try_submit_progress(&self, update: JobProgressUpdateV1) -> JobProgressStatusV1 {
        self.0.submit_progress(update)
    }

    #[doc(hidden)]
    #[must_use]
    pub fn result_sink(
        &self,
        job: JobHandleV1,
        capability: SinkCapabilityV1,
    ) -> IncrementalResultSinkV1 {
        IncrementalResultSinkV1 {
            job,
            capability,
            services: self.clone(),
        }
    }

    #[doc(hidden)]
    #[must_use]
    pub fn progress_sink(
        &self,
        job: JobHandleV1,
        capability: SinkCapabilityV1,
    ) -> JobProgressSinkV1 {
        JobProgressSinkV1 {
            job,
            capability,
            services: self.clone(),
        }
    }
}

/// Capability-bound result client backed by the stateful Rust-ABI services
/// object. It contains no raw function pointer.
#[repr(C)]
#[derive(Clone, StableAbi)]
pub struct IncrementalResultSinkV1 {
    pub job: JobHandleV1,
    pub capability: SinkCapabilityV1,
    services: JobHostServicesV1,
}

impl IncrementalResultSinkV1 {
    #[must_use]
    pub fn try_submit(&self, batch: IncrementalResultBatchV1) -> SinkSubmitOutcomeV1 {
        self.services.try_submit(batch)
    }
}

/// Capability-bound progress client backed by the same Rust-ABI services
/// object. It contains no raw function pointer.
#[repr(C)]
#[derive(Clone, StableAbi)]
pub struct JobProgressSinkV1 {
    pub job: JobHandleV1,
    pub capability: SinkCapabilityV1,
    services: JobHostServicesV1,
}

impl JobProgressSinkV1 {
    #[must_use]
    pub fn try_submit(&self, update: JobProgressUpdateV1) -> JobProgressStatusV1 {
        self.services.try_submit_progress(update)
    }
}

/// Opaque host capability for one generation-bound decoder input stream.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, StableAbi)]
pub struct InputStreamCapabilityV1 {
    nonce: [u8; 16],
}

impl InputStreamCapabilityV1 {
    #[doc(hidden)]
    #[must_use]
    pub const fn from_host(nonce: [u8; 16]) -> Self {
        Self { nonce }
    }
}

/// Typed stream operation terminal. Values never contain a path, OS error, or
/// native handle. Cancellation and deadline are host-owned job controls.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct InputStreamStatusV1(u32);

impl InputStreamStatusV1 {
    pub const OK: Self = Self(1);
    pub const EOF: Self = Self(2);
    pub const CANCELLED: Self = Self(3);
    pub const DEADLINE_ELAPSED: Self = Self(4);
    pub const STALE: Self = Self(5);
    pub const CLOSED: Self = Self(6);
    pub const WRONG_THREAD: Self = Self(7);
    pub const UNSUPPORTED: Self = Self(8);
    pub const INVALID: Self = Self(9);

    #[must_use]
    pub const fn into_raw(self) -> u32 {
        self.0
    }

    #[must_use]
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }
}

/// Seek reference point. The host validates all signed arithmetic and does not
/// permit a plugin to address a native file handle directly.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct InputStreamSeekOriginV1(u32);

impl InputStreamSeekOriginV1 {
    pub const START: Self = Self(1);
    pub const CURRENT: Self = Self(2);
    pub const END: Self = Self(3);

    #[must_use]
    pub const fn into_raw(self) -> u32 {
        self.0
    }
}

/// Bounded read request. `maximum_bytes` is an allocation upper bound, not a
/// caller-provided buffer or pointer.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct InputStreamReadRequestV1 {
    pub maximum_bytes: u32,
    pub reserved: u32,
}

/// Owned read response. `data` is present only for [`InputStreamStatusV1::OK`]
/// and may be empty for a successful zero-length read.
#[repr(C)]
#[derive(Clone, Debug, StableAbi)]
pub struct InputStreamReadOutcomeV1 {
    pub status: InputStreamStatusV1,
    pub reserved: u32,
    pub source_generation: u64,
    pub position: u64,
    pub data: RVec<u8>,
}

/// Checked signed seek request.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct InputStreamSeekRequestV1 {
    pub origin: InputStreamSeekOriginV1,
    pub reserved: u32,
    pub offset: i64,
}

/// Seek result with the current host-attested source generation.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct InputStreamSeekOutcomeV1 {
    pub status: InputStreamStatusV1,
    pub reserved: u32,
    pub source_generation: u64,
    pub position: u64,
}

/// Optional length response. V1 never truncates a host length to a 32-bit
/// value; unavailable length is represented by `UNSUPPORTED`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct InputStreamLengthOutcomeV1 {
    pub status: InputStreamStatusV1,
    pub reserved: u32,
    pub source_generation: u64,
    pub length: u64,
}

/// Stateful Rust-first service object for a host-minted stream. It uses
/// `abi_stable` instead of raw callbacks and exposes no `Read`, `File`, path,
/// native handle, clock, cancellation token, or future across the ABI.
#[sabi_trait]
#[doc(hidden)]
pub trait AbiInputStreamServicesV1: Send + Sync + Clone {
    fn read(&self, request: InputStreamReadRequestV1) -> InputStreamReadOutcomeV1;
    fn seek(&self, request: InputStreamSeekRequestV1) -> InputStreamSeekOutcomeV1;
    #[sabi(last_prefix_field)]
    fn length(&self) -> InputStreamLengthOutcomeV1;
}

#[repr(C)]
#[derive(Clone, StableAbi)]
pub struct InputStreamV1 {
    capability: InputStreamCapabilityV1,
    services: AbiInputStreamServicesV1_TO<'static, RArc<()>>,
}

impl InputStreamV1 {
    #[doc(hidden)]
    pub fn from_host<T>(capability: InputStreamCapabilityV1, services: T) -> Self
    where
        T: AbiInputStreamServicesV1 + 'static,
    {
        Self {
            capability,
            services: AbiInputStreamServicesV1_TO::from_ptr(
                RArc::new(services),
                sabi_trait::TD_Opaque,
            ),
        }
    }

    #[must_use]
    pub fn read(&self, request: InputStreamReadRequestV1) -> InputStreamReadOutcomeV1 {
        self.services.read(request)
    }

    #[must_use]
    pub fn seek(&self, request: InputStreamSeekRequestV1) -> InputStreamSeekOutcomeV1 {
        self.services.seek(request)
    }

    #[must_use]
    pub fn length(&self) -> InputStreamLengthOutcomeV1 {
        self.services.length()
    }
}

/// Immutable ABI context for one synchronous provider callback.
#[repr(C)]
#[derive(Clone, StableAbi)]
pub struct JobContextV1 {
    pub job: JobHandleV1,
    pub item: ROption<ItemHandleV1>,
    pub location: LocationHandleV1,
    pub feature_epoch: u64,
    pub job_generation: u64,
    pub item_generation: u64,
    pub location_generation: u64,
    pub source_generation: u64,
    /// Present only for a host-attested source whose sealed contribution has
    /// `filesystem.read`; it is absent for metadata-only jobs.
    pub input: ROption<InputStreamV1>,
    pub sink: IncrementalResultSinkV1,
    pub progress: JobProgressSinkV1,
}

impl JobContextV1 {
    #[must_use]
    pub fn poll_control(&self) -> JobControlStateV1 {
        self.sink.services.poll_control()
    }

    #[must_use]
    pub fn try_submit(&self, batch: IncrementalResultBatchV1) -> SinkSubmitOutcomeV1 {
        self.sink.try_submit(batch)
    }

    #[must_use]
    pub fn try_submit_progress(&self, update: JobProgressUpdateV1) -> JobProgressStatusV1 {
        self.progress.try_submit(update)
    }
}

/// One host-attested file input in a bounded column-provider callback.
///
/// The item capability and stream are minted by the host for the context's
/// generation. Authors receive neither a path nor a native file handle.
#[repr(C)]
#[derive(Clone, StableAbi)]
pub struct BatchColumnItemV1 {
    pub item: ItemHandleV1,
    pub item_generation: u64,
    /// Host-attested basename only. It never contains a directory path or
    /// native handle and lets a source parser select its language safely.
    pub file_name: RString,
    pub input: InputStreamV1,
}

/// Immutable context for one bounded batch-column callback.
///
/// Results are submitted through the existing [`IncrementalResultSinkV1`], so
/// columns reuse the V1 typed value, outcome, and stable-sort transports. The
/// host validates every submitted result against the item capabilities below.
#[repr(C)]
#[derive(Clone, StableAbi)]
pub struct BatchColumnContextV1 {
    pub job: JobHandleV1,
    pub location: LocationHandleV1,
    pub feature_epoch: u64,
    pub job_generation: u64,
    pub item_generation: u64,
    pub location_generation: u64,
    pub source_generation: u64,
    pub items: RVec<BatchColumnItemV1>,
    pub sink: IncrementalResultSinkV1,
    pub progress: JobProgressSinkV1,
}

impl BatchColumnContextV1 {
    /// Returns the current host control state for this one synchronous call.
    #[must_use]
    pub fn poll_control(&self) -> JobControlStateV1 {
        self.sink.services.poll_control()
    }

    /// Submits an existing typed incremental result batch.
    #[must_use]
    pub fn try_submit(&self, batch: IncrementalResultBatchV1) -> SinkSubmitOutcomeV1 {
        self.sink.try_submit(batch)
    }

    /// Submits an existing typed progress update.
    #[must_use]
    pub fn try_submit_progress(&self, update: JobProgressUpdateV1) -> JobProgressStatusV1 {
        self.progress.try_submit(update)
    }

    /// Validates the fixed V1 shape before an author spends work on it.
    #[must_use]
    pub fn is_well_formed(&self) -> bool {
        !self.items.is_empty()
            && self.items.len() <= MAX_BATCH_COLUMN_ITEMS_V1
            && self.items.iter().all(|item| {
                item.item_generation == self.item_generation
                    && item.item.generation() == self.item_generation
                    && !item.file_name.is_empty()
                    && item.file_name.len() <= MAX_BATCH_COLUMN_FILE_NAME_BYTES_V1
                    && !item.file_name.contains(['/', '\\'])
            })
    }
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

/// Private ABI vtable. Plugin authors implement [`JobProviderImplementationV1`]
/// instead; this trait and its generated `_TO` never form public API.
#[sabi_trait]
#[doc(hidden)]
pub trait AbiJobProviderObjectV1: Send + Sync {
    /// Runs one bounded synchronous job invocation.
    #[sabi(last_prefix_field)]
    fn run(&self, context: JobContextV1) -> JobTerminalV1;
}

/// ABI-owned provider object storage. `RArc` keeps the trait object resident
/// and prevents plugin authors from passing arbitrary Rust vtables.
#[repr(transparent)]
#[derive(StableAbi)]
pub struct JobProviderObjectV1(AbiJobProviderObjectV1_TO<'static, RBox<()>>);

/// Public plugin-facing provider implementation. The SDK wraps it in a private
/// `#[sabi_trait]` object, catches panics before the ABI boundary returns, and
/// permanently fault-latches that adapter after its first panic.
pub trait JobProviderImplementationV1: Send + Sync {
    fn run(&self, context: JobContextV1) -> JobTerminalV1;
}

const PROVIDER_IDLE_V1: u8 = 0;
const PROVIDER_RUNNING_V1: u8 = 1;
const PROVIDER_FAULTED_V1: u8 = 2;

struct ProviderAdapterV1<T> {
    provider: Option<T>,
    invocation_state: AtomicU8,
}

impl<T: JobProviderImplementationV1> AbiJobProviderObjectV1 for ProviderAdapterV1<T> {
    fn run(&self, context: JobContextV1) -> JobTerminalV1 {
        if self
            .invocation_state
            .compare_exchange(
                PROVIDER_IDLE_V1,
                PROVIDER_RUNNING_V1,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_err()
        {
            // A faulted adapter stays quarantined. Concurrent reentry is also
            // denied rather than allowing two callbacks to race across one
            // stateful plugin provider.
            return JobTerminalV1::PANICKED;
        }
        let Some(provider) = self.provider.as_ref() else {
            self.invocation_state
                .store(PROVIDER_FAULTED_V1, Ordering::Release);
            return JobTerminalV1::INCOMPATIBLE;
        };
        match catch_unwind(AssertUnwindSafe(|| provider.run(context))) {
            Ok(terminal) if terminal.is_known() => {
                self.invocation_state
                    .store(PROVIDER_IDLE_V1, Ordering::Release);
                terminal
            }
            Ok(_) => {
                self.invocation_state
                    .store(PROVIDER_IDLE_V1, Ordering::Release);
                JobTerminalV1::INCOMPATIBLE
            }
            Err(payload) => {
                self.invocation_state
                    .store(PROVIDER_FAULTED_V1, Ordering::Release);
                dispose_caught_panic_payload_v1(payload);
                JobTerminalV1::PANICKED
            }
        }
    }
}
impl<T> Drop for ProviderAdapterV1<T> {
    fn drop(&mut self) {
        if let Some(provider) = self.provider.take()
            && let Err(payload) = catch_unwind(AssertUnwindSafe(|| drop(provider)))
        {
            dispose_caught_panic_payload_v1(payload);
        }
    }
}

impl JobProviderObjectV1 {
    /// Wraps a stateful Rust provider in the SDK-owned ABI object.
    #[must_use]
    pub fn new<T: JobProviderImplementationV1 + 'static>(provider: T) -> Self {
        Self(AbiJobProviderObjectV1_TO::from_value(
            ProviderAdapterV1 {
                provider: Some(provider),
                invocation_state: AtomicU8::new(PROVIDER_IDLE_V1),
            },
            sabi_trait::TD_Opaque,
        ))
    }

    #[doc(hidden)]
    pub fn invoke(&self, context: JobContextV1) -> JobTerminalV1 {
        self.0.run(context)
    }
}

/// Private ABI vtable. Plugin authors implement
/// [`BatchColumnProviderImplementationV1`] instead; this trait and its
/// generated `_TO` never form public API.
#[sabi_trait]
#[doc(hidden)]
pub trait AbiBatchColumnProviderObjectV1: Send + Sync {
    /// Runs one bounded, synchronous code-column callback.
    #[sabi(last_prefix_field)]
    fn run(&self, context: BatchColumnContextV1) -> JobTerminalV1;
}

/// SDK-owned ABI storage for one bounded batch-column provider.
#[repr(transparent)]
#[derive(StableAbi)]
pub struct BatchColumnProviderObjectV1(AbiBatchColumnProviderObjectV1_TO<'static, RBox<()>>);

/// Public Rust-first implementation trait for a bounded batch column.
///
/// The SDK owns the `abi_stable` adapter and panic boundary. Implementors are
/// called synchronously with at most [`MAX_BATCH_COLUMN_ITEMS_V1`] items and
/// must use the existing typed result sink for all output.
pub trait BatchColumnProviderImplementationV1: Send + Sync {
    fn run(&self, context: BatchColumnContextV1) -> JobTerminalV1;
}

struct BatchColumnProviderAdapterV1<T> {
    provider: Option<T>,
    invocation_state: AtomicU8,
}

impl<T: BatchColumnProviderImplementationV1> AbiBatchColumnProviderObjectV1
    for BatchColumnProviderAdapterV1<T>
{
    fn run(&self, context: BatchColumnContextV1) -> JobTerminalV1 {
        if !context.is_well_formed()
            || self
                .invocation_state
                .compare_exchange(
                    PROVIDER_IDLE_V1,
                    PROVIDER_RUNNING_V1,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_err()
        {
            return JobTerminalV1::PANICKED;
        }
        let Some(provider) = self.provider.as_ref() else {
            self.invocation_state
                .store(PROVIDER_FAULTED_V1, Ordering::Release);
            return JobTerminalV1::INCOMPATIBLE;
        };
        match catch_unwind(AssertUnwindSafe(|| provider.run(context))) {
            Ok(terminal) if terminal.is_known() => {
                self.invocation_state
                    .store(PROVIDER_IDLE_V1, Ordering::Release);
                terminal
            }
            Ok(_) => {
                self.invocation_state
                    .store(PROVIDER_IDLE_V1, Ordering::Release);
                JobTerminalV1::INCOMPATIBLE
            }
            Err(payload) => {
                self.invocation_state
                    .store(PROVIDER_FAULTED_V1, Ordering::Release);
                dispose_caught_panic_payload_v1(payload);
                JobTerminalV1::PANICKED
            }
        }
    }
}

impl<T> Drop for BatchColumnProviderAdapterV1<T> {
    fn drop(&mut self) {
        if let Some(provider) = self.provider.take()
            && let Err(payload) = catch_unwind(AssertUnwindSafe(|| drop(provider)))
        {
            dispose_caught_panic_payload_v1(payload);
        }
    }
}

impl BatchColumnProviderObjectV1 {
    /// Wraps an ordinary Rust batch provider in the SDK-owned ABI adapter.
    #[must_use]
    pub fn new<T: BatchColumnProviderImplementationV1 + 'static>(provider: T) -> Self {
        Self(AbiBatchColumnProviderObjectV1_TO::from_value(
            BatchColumnProviderAdapterV1 {
                provider: Some(provider),
                invocation_state: AtomicU8::new(PROVIDER_IDLE_V1),
            },
            sabi_trait::TD_Opaque,
        ))
    }

    #[doc(hidden)]
    pub fn invoke(&self, context: BatchColumnContextV1) -> JobTerminalV1 {
        self.0.run(context)
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

    #[test]
    fn stable_sort_rejects_noncanonical_float_and_preserves_integer_precision() {
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(StableSortValueV1::float(value).is_err());
        }
        assert_eq!(StableSortValueV1::float(-0.0).unwrap().float.to_bits(), 0);
        assert!(StableSortValueV1::float(f64::from_bits(1)).is_ok());
        let exact = StableSortValueV1::unsigned((1_u64 << 53) + 1);
        assert_eq!(exact.unsigned, (1_u64 << 53) + 1);
        assert!(StableSortValueV1::bytes(vec![0xff]).is_ok());
        assert!(StableSortValueV1::text("é").is_ok());
    }
}
