//! Host-native value projection, deterministic sorting, and opaque routing.

use std::{
    cmp::Ordering,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering as AtomicOrdering},
    },
};

use explorer_extension_api::{
    IncrementalResultEntryV1, ItemHandleV1, JobTerminalV1, PluginItemOutcomeV1, PluginItemResultV1,
    PluginValueKindV1, PluginValueV1, StableSortValueKindV1, StableSortValueV1,
};

use crate::{ContributionKindV1, ExtensionJobAuthorityV1, ExtensionJobProducerV1};

/// Host-only row. Its sort key is copied and normalized once before a UI sort;
/// comparison makes no allocation, callback, lock, or plugin invocation.
#[derive(Clone, Debug)]
pub struct ExtensionValueRowV1 {
    outcome: PluginItemOutcomeV1,
    value: Option<HostExtensionValueV1>,
    sort: Option<HostSortKeyV1>,
    producer: Option<ExtensionJobProducerV1>,
    display_name: String,
    stable_item_identity: u128,
    generation: ExtensionValueGenerationV1,
}

/// Shared host-only tombstone carried by every clone published from one job
/// generation. Revocation remains observable after the runtime job/registry is
/// retired because readers retain this `Arc` rather than consulting a live job.
#[derive(Debug)]
pub(crate) struct ExtensionValueGenerationStateV1 {
    current: AtomicBool,
    parents: Vec<ExtensionValueGenerationV1>,
}

#[derive(Clone, Debug)]
pub(crate) struct ExtensionValueGenerationV1(Arc<ExtensionValueGenerationStateV1>);

impl ExtensionValueGenerationV1 {
    pub(crate) fn current() -> Self {
        Self(Arc::new(ExtensionValueGenerationStateV1 {
            current: AtomicBool::new(true),
            parents: Vec::new(),
        }))
    }

    pub(crate) fn combine(parents: impl IntoIterator<Item = Self>) -> Self {
        Self(Arc::new(ExtensionValueGenerationStateV1 {
            current: AtomicBool::new(true),
            parents: parents.into_iter().collect(),
        }))
    }

    pub(crate) fn revoke(&self) {
        self.0.current.store(false, AtomicOrdering::Release);
    }

    pub(crate) fn is_current(&self) -> bool {
        self.0.current.load(AtomicOrdering::Acquire) && self.parents_are_current()
    }

    pub(crate) fn downgrade(&self) -> std::sync::Weak<ExtensionValueGenerationStateV1> {
        Arc::downgrade(&self.0)
    }

    pub(crate) fn weak_is_current(weak: &std::sync::Weak<ExtensionValueGenerationStateV1>) -> bool {
        weak.upgrade().is_some_and(|state| {
            let generation = Self(state);
            generation.is_current()
        })
    }

    pub(crate) fn revoke_weak(weak: &std::sync::Weak<ExtensionValueGenerationStateV1>) -> bool {
        let Some(state) = weak.upgrade() else {
            return false;
        };
        Self(state).revoke();
        true
    }

    fn parents_are_current(&self) -> bool {
        self.0.parents.iter().all(Self::is_current)
    }
}

/// Fully owned host representation of a plugin result value. No accepted queue
/// retains `RString`, `RVec`, or another ABI-owned value after ingestion.
#[derive(Clone, Debug)]
#[allow(
    dead_code,
    reason = "the host-owned value is retained for downstream model/render consumers"
)]
enum HostExtensionValueV1 {
    Bool(bool),
    I64(i64),
    F64(f64),
    Bytes(Vec<u8>),
    TimeUnixNanos(i64),
    DurationNanos(u64),
    Text(String),
    LocalizedText(String),
    StructuredCanonicalJson(Vec<u8>),
    Opaque(RoutedOpaquePayloadV1),
}

#[derive(Clone, Debug)]
#[allow(
    dead_code,
    reason = "the complete host-native result is retained after ABI ingestion"
)]
pub(crate) struct HostPluginItemResultV1 {
    outcome: PluginItemOutcomeV1,
    value: Option<HostExtensionValueV1>,
    sort: Option<HostSortKeyV1>,
    producer: Option<ExtensionJobProducerV1>,
}

/// Borrowed, host-owned value exposed to model consumers. Opaque bytes are not
/// exposed here; they can only be obtained through a renderer binding minted
/// from this exact accepted row.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ExtensionValueViewV1<'a> {
    Bool(bool),
    I64(i64),
    F64(f64),
    Bytes(&'a [u8]),
    TimeUnixNanos(i64),
    DurationNanos(u64),
    Text(&'a str),
    LocalizedText(&'a str),
    StructuredCanonicalJson(&'a [u8]),
    Opaque {
        schema: explorer_extension_api::StableIdV1,
        schema_version: u32,
    },
}

#[derive(Clone, Debug)]
#[allow(
    dead_code,
    reason = "generation metadata travels with the host-owned queued result"
)]
pub(crate) struct HostIncrementalResultEntryV1 {
    pub item: ItemHandleV1,
    pub item_generation: u64,
    pub source_generation: u64,
    pub(crate) result: HostPluginItemResultV1,
}

impl HostIncrementalResultEntryV1 {
    pub(crate) fn rebind_row(
        &self,
        display_name: String,
        stable_item_identity: u128,
        generation: ExtensionValueGenerationV1,
    ) -> ExtensionValueRowV1 {
        ExtensionValueRowV1::from_host_with_generation(
            self.result.clone(),
            display_name,
            stable_item_identity,
            generation,
        )
    }
}

#[derive(Clone, Debug)]
enum HostSortKeyV1 {
    Bool(bool),
    I64(i64),
    U64(u64),
    F64(f64),
    Text(String),
    Bytes(Vec<u8>),
}

impl ExtensionValueRowV1 {
    /// Validated host construction; display name and stable identity originate in
    /// the model, never a plugin handle or queue arrival order.
    pub fn try_new(
        result: &PluginItemResultV1,
        expected_sort: abi_stable::std_types::ROption<StableSortValueKindV1>,
        display_name: String,
        stable_item_identity: u128,
    ) -> Option<Self> {
        let result = HostPluginItemResultV1::from_abi(result, expected_sort, None)?;
        Some(Self::from_host(result, display_name, stable_item_identity))
    }

    pub(crate) fn from_host(
        result: HostPluginItemResultV1,
        display_name: String,
        stable_item_identity: u128,
    ) -> Self {
        Self::from_host_with_generation(
            result,
            display_name,
            stable_item_identity,
            ExtensionValueGenerationV1::current(),
        )
    }

    pub(crate) fn from_host_with_generation(
        result: HostPluginItemResultV1,
        display_name: String,
        stable_item_identity: u128,
        generation: ExtensionValueGenerationV1,
    ) -> Self {
        let HostPluginItemResultV1 {
            outcome,
            value,
            sort,
            producer,
        } = result;
        Self {
            outcome,
            value,
            sort,
            producer,
            display_name,
            stable_item_identity,
            generation,
        }
    }

    #[allow(dead_code)]
    #[must_use]
    pub fn outcome(&self) -> PluginItemOutcomeV1 {
        if self.generation.is_current() {
            self.outcome
        } else {
            PluginItemOutcomeV1::INCOMPATIBLE
        }
    }

    #[allow(dead_code)]
    #[must_use]
    pub(crate) fn host_display_name(&self) -> &str {
        &self.display_name
    }

    #[allow(dead_code)]
    #[must_use]
    pub(crate) const fn host_stable_item_identity(&self) -> u128 {
        self.stable_item_identity
    }

    #[must_use]
    pub fn value(&self) -> Option<ExtensionValueViewV1<'_>> {
        if !self.generation.is_current() {
            return None;
        }
        Some(match self.value.as_ref()? {
            HostExtensionValueV1::Bool(value) => ExtensionValueViewV1::Bool(*value),
            HostExtensionValueV1::I64(value) => ExtensionValueViewV1::I64(*value),
            HostExtensionValueV1::F64(value) => ExtensionValueViewV1::F64(*value),
            HostExtensionValueV1::Bytes(value) => ExtensionValueViewV1::Bytes(value),
            HostExtensionValueV1::TimeUnixNanos(value) => {
                ExtensionValueViewV1::TimeUnixNanos(*value)
            }
            HostExtensionValueV1::DurationNanos(value) => {
                ExtensionValueViewV1::DurationNanos(*value)
            }
            HostExtensionValueV1::Text(value) => ExtensionValueViewV1::Text(value),
            HostExtensionValueV1::LocalizedText(value) => {
                ExtensionValueViewV1::LocalizedText(value)
            }
            HostExtensionValueV1::StructuredCanonicalJson(value) => {
                ExtensionValueViewV1::StructuredCanonicalJson(value)
            }
            HostExtensionValueV1::Opaque(payload) => ExtensionValueViewV1::Opaque {
                schema: payload.schema,
                schema_version: payload.schema_version,
            },
        })
    }
}

impl HostPluginItemResultV1 {
    pub(crate) fn from_abi(
        result: &PluginItemResultV1,
        expected_sort: abi_stable::std_types::ROption<StableSortValueKindV1>,
        producer: Option<&ExtensionJobProducerV1>,
    ) -> Option<Self> {
        result.validate_transport(expected_sort).ok()?;
        let value = match result.value.as_ref() {
            abi_stable::std_types::ROption::RNone => None,
            abi_stable::std_types::ROption::RSome(value) => Some(copy_value(value, producer)?),
        };
        let sort = match result.stable_sort.as_ref() {
            abi_stable::std_types::ROption::RNone => None,
            abi_stable::std_types::ROption::RSome(value) => Some(copy_sort(value)?),
        };
        Some(Self {
            outcome: result.outcome,
            value,
            sort,
            producer: producer.cloned(),
        })
    }
}

pub(crate) fn ingest_entry_v1(
    entry: &IncrementalResultEntryV1,
    expected_sort: abi_stable::std_types::ROption<StableSortValueKindV1>,
    producer: &ExtensionJobProducerV1,
) -> Option<HostIncrementalResultEntryV1> {
    Some(HostIncrementalResultEntryV1 {
        item: entry.item,
        item_generation: entry.item_generation,
        source_generation: entry.source_generation,
        result: HostPluginItemResultV1::from_abi(&entry.result, expected_sort, Some(producer))?,
    })
}

fn copy_value(
    value: &PluginValueV1,
    producer: Option<&ExtensionJobProducerV1>,
) -> Option<HostExtensionValueV1> {
    value.validate_transport().ok()?;
    Some(match value.kind.into_raw() {
        raw if raw == PluginValueKindV1::BOOL.into_raw() => {
            HostExtensionValueV1::Bool(value.integer != 0)
        }
        raw if raw == PluginValueKindV1::I64.into_raw() => HostExtensionValueV1::I64(value.integer),
        raw if raw == PluginValueKindV1::F64.into_raw() => HostExtensionValueV1::F64(value.float),
        raw if raw == PluginValueKindV1::BYTES.into_raw() => {
            HostExtensionValueV1::Bytes(value.payload.iter().copied().collect())
        }
        raw if raw == PluginValueKindV1::TIME_UNIX_NANOS.into_raw() => {
            HostExtensionValueV1::TimeUnixNanos(value.integer)
        }
        raw if raw == PluginValueKindV1::DURATION_NANOS.into_raw() => {
            HostExtensionValueV1::DurationNanos(u64::try_from(value.integer).ok()?)
        }
        raw if raw == PluginValueKindV1::TEXT.into_raw() => {
            HostExtensionValueV1::Text(value.text.to_string())
        }
        raw if raw == PluginValueKindV1::LOCALIZED_TEXT.into_raw() => {
            HostExtensionValueV1::LocalizedText(value.text.to_string())
        }
        raw if raw == PluginValueKindV1::STRUCTURED.into_raw() => {
            HostExtensionValueV1::StructuredCanonicalJson(value.payload.iter().copied().collect())
        }
        raw if raw == PluginValueKindV1::OPAQUE.into_raw() => {
            let producer = producer?;
            if producer.opaque_schema != Some(value.opaque_schema)
                || producer.opaque_schema_version != Some(value.opaque_schema_version)
            {
                return None;
            }
            HostExtensionValueV1::Opaque(RoutedOpaquePayloadV1 {
                schema: value.opaque_schema,
                schema_version: value.opaque_schema_version,
                bytes: value.payload.iter().copied().collect(),
            })
        }
        _ => return None,
    })
}

fn copy_sort(value: &StableSortValueV1) -> Option<HostSortKeyV1> {
    value.validate_transport().ok()?;
    Some(match value.kind.into_raw() {
        raw if raw == StableSortValueKindV1::BOOL.into_raw() => {
            HostSortKeyV1::Bool(value.unsigned != 0)
        }
        raw if raw == StableSortValueKindV1::I64.into_raw()
            || raw == StableSortValueKindV1::TIME_UNIX_NANOS.into_raw() =>
        {
            HostSortKeyV1::I64(value.signed)
        }
        raw if raw == StableSortValueKindV1::U64.into_raw()
            || raw == StableSortValueKindV1::DURATION_NANOS.into_raw() =>
        {
            HostSortKeyV1::U64(value.unsigned)
        }
        raw if raw == StableSortValueKindV1::F64.into_raw() => HostSortKeyV1::F64(value.float),
        raw if raw == StableSortValueKindV1::TEXT.into_raw() => {
            HostSortKeyV1::Text(value.text.to_string())
        }
        raw if raw == StableSortValueKindV1::BYTES.into_raw() => {
            HostSortKeyV1::Bytes(value.bytes.iter().copied().collect())
        }
        _ => return None,
    })
}

/// Value direction never reverses the fixed absent-outcome tail.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExtensionSortDirectionV1 {
    Ascending,
    Descending,
}

/// Infallible comparator over prevalidated host rows.
#[must_use]
pub fn compare_extension_rows_v1(
    left: &ExtensionValueRowV1,
    right: &ExtensionValueRowV1,
    direction: ExtensionSortDirectionV1,
) -> Ordering {
    let left_current = left.generation.is_current();
    let right_current = right.generation.is_current();
    let left_sort = left_current.then_some(left.sort.as_ref()).flatten();
    let right_sort = right_current.then_some(right.sort.as_ref()).flatten();
    let primary = match (left_sort, right_sort) {
        (Some(left), Some(right)) => compare_key(left, right),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => {
            let left_outcome = if left_current {
                left.outcome
            } else {
                PluginItemOutcomeV1::INCOMPATIBLE
            };
            let right_outcome = if right_current {
                right.outcome
            } else {
                PluginItemOutcomeV1::INCOMPATIBLE
            };
            outcome_rank(left_outcome).cmp(&outcome_rank(right_outcome))
        }
    };
    let primary = if left_current
        && right_current
        && left.sort.is_some()
        && right.sort.is_some()
        && direction == ExtensionSortDirectionV1::Descending
    {
        primary.reverse()
    } else {
        primary
    };
    primary
        .then_with(|| {
            left.display_name
                .as_bytes()
                .cmp(right.display_name.as_bytes())
        })
        .then_with(|| left.stable_item_identity.cmp(&right.stable_item_identity))
}

fn compare_key(left: &HostSortKeyV1, right: &HostSortKeyV1) -> Ordering {
    match (left, right) {
        (HostSortKeyV1::Bool(a), HostSortKeyV1::Bool(b)) => a.cmp(b),
        (HostSortKeyV1::I64(a), HostSortKeyV1::I64(b)) => a.cmp(b),
        (HostSortKeyV1::U64(a), HostSortKeyV1::U64(b)) => a.cmp(b),
        (HostSortKeyV1::F64(a), HostSortKeyV1::F64(b)) => a.total_cmp(b),
        (HostSortKeyV1::Text(a), HostSortKeyV1::Text(b)) => a.as_bytes().cmp(b.as_bytes()),
        (HostSortKeyV1::Bytes(a), HostSortKeyV1::Bytes(b)) => a.cmp(b),
        (a, b) => key_rank(a).cmp(&key_rank(b)),
    }
}
fn key_rank(key: &HostSortKeyV1) -> u8 {
    match key {
        HostSortKeyV1::Bool(_) => 1,
        HostSortKeyV1::I64(_) => 2,
        HostSortKeyV1::U64(_) => 3,
        HostSortKeyV1::F64(_) => 4,
        HostSortKeyV1::Text(_) => 5,
        HostSortKeyV1::Bytes(_) => 6,
    }
}
fn outcome_rank(outcome: PluginItemOutcomeV1) -> u8 {
    match outcome.into_raw() {
        // A valid display-only value can intentionally omit a stable sort key.
        // It still precedes the fixed absent/error tail in both directions.
        1 => 0,
        2 => 1,
        3 => 2,
        4 => 3,
        5 => 4,
        6 => 5,
        _ => 6,
    }
}

/// Per-item policy for a job that did not return a row for that item.
#[must_use]
pub const fn project_terminal_outcome_v1(
    terminal: JobTerminalV1,
    prior_value: bool,
) -> PluginItemOutcomeV1 {
    if prior_value && terminal.into_raw() != JobTerminalV1::INCOMPATIBLE.into_raw() {
        return PluginItemOutcomeV1::VALUE;
    }
    match terminal.into_raw() {
        1 | 8 => PluginItemOutcomeV1::INCOMPATIBLE,
        2 => PluginItemOutcomeV1::UNSUPPORTED,
        3 | 6 => PluginItemOutcomeV1::UNAVAILABLE,
        4 | 5 => PluginItemOutcomeV1::CANCELLED,
        7 | 9 => PluginItemOutcomeV1::PLUGIN_ERROR,
        _ if prior_value => PluginItemOutcomeV1::VALUE,
        _ => PluginItemOutcomeV1::INCOMPATIBLE,
    }
}

/// Explicit, host-created binding between accepted opaque source data and a
/// currently leased renderer contribution.
#[derive(Debug)]
pub struct OpaquePayloadBindingV1 {
    source: ExtensionJobProducerV1,
    renderer: ExtensionJobAuthorityV1,
    payload: RoutedOpaquePayloadV1,
    generation: ExtensionValueGenerationV1,
}
impl OpaquePayloadBindingV1 {
    #[allow(clippy::double_must_use, clippy::missing_errors_doc)]
    pub fn bind(
        row: &ExtensionValueRowV1,
        renderer: ExtensionJobAuthorityV1,
    ) -> Result<Self, OpaquePayloadRouteErrorV1> {
        if !row.generation.is_current() {
            return Err(OpaquePayloadRouteErrorV1::BindingDenied);
        }
        let source = row
            .producer
            .as_ref()
            .ok_or(OpaquePayloadRouteErrorV1::NotOpaque)?;
        let payload = match row.value.as_ref() {
            Some(HostExtensionValueV1::Opaque(payload)) => payload.clone(),
            _ => return Err(OpaquePayloadRouteErrorV1::NotOpaque),
        };
        if renderer.contribution_kind != ContributionKindV1::GpuiRenderer
            || source.package_id != renderer.producer().package_id
            || source.sealed_manifest_digest != renderer.producer().sealed_manifest_digest
            || source.feature_epoch != renderer.producer().feature_epoch
            || source.feature_id != renderer.producer().feature_id
            || source.renderer_contribution_id.as_deref()
                != Some(renderer.producer().contribution_id.as_str())
            || source.opaque_schema != renderer.producer().opaque_schema
            || source.opaque_schema_version != renderer.producer().opaque_schema_version
            || source.opaque_schema != renderer.opaque_schema
            || source.opaque_schema_version != renderer.opaque_schema_version
        {
            return Err(OpaquePayloadRouteErrorV1::BindingDenied);
        }
        Ok(Self {
            source: source.clone(),
            renderer,
            payload,
            generation: row.generation.clone(),
        })
    }
    #[allow(clippy::missing_errors_doc)]
    pub fn route(&self) -> Result<RoutedOpaquePayloadV1, OpaquePayloadRouteErrorV1> {
        if !self.generation.is_current()
            || self.source.package_id != self.renderer.producer().package_id
            || self.source.sealed_manifest_digest != self.renderer.producer().sealed_manifest_digest
            || self.source.feature_epoch != self.renderer.producer().feature_epoch
            || self.source.feature_id != self.renderer.producer().feature_id
        {
            return Err(OpaquePayloadRouteErrorV1::BindingDenied);
        }
        if Some(self.payload.schema) != self.source.opaque_schema
            || Some(self.payload.schema_version) != self.source.opaque_schema_version
        {
            return Err(OpaquePayloadRouteErrorV1::BindingDenied);
        }
        Ok(self.payload.clone())
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoutedOpaquePayloadV1 {
    pub schema: explorer_extension_api::StableIdV1,
    pub schema_version: u32,
    pub bytes: Vec<u8>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OpaquePayloadRouteErrorV1 {
    BindingDenied,
    NotOpaque,
}

#[cfg(test)]
mod tests {
    use super::*;
    use abi_stable::std_types::ROption;
    use explorer_extension_api::{PluginItemResultV1, PluginValueV1, StableSortValueV1};

    fn row(value: u64, name: &str, id: u128) -> ExtensionValueRowV1 {
        ExtensionValueRowV1::try_new(
            &PluginItemResultV1::value(
                PluginValueV1::integer(0),
                ROption::RSome(StableSortValueV1::unsigned(value)),
            ),
            ROption::RSome(StableSortValueKindV1::U64),
            name.to_owned(),
            id,
        )
        .unwrap()
    }
    #[test]
    fn comparator_is_total_and_absent_tail_is_direction_invariant() {
        let rows = [
            row((1_u64 << 53) + 1, "é", 2),
            row(2, "z", 1),
            row(2, "a", 3),
        ];
        for left in &rows {
            for right in &rows {
                assert_eq!(
                    compare_extension_rows_v1(left, right, ExtensionSortDirectionV1::Ascending),
                    compare_extension_rows_v1(right, left, ExtensionSortDirectionV1::Ascending)
                        .reverse()
                );
            }
        }
        let absent = ExtensionValueRowV1::try_new(
            &PluginItemResultV1::absent(PluginItemOutcomeV1::UNSUPPORTED),
            ROption::RNone,
            "x".into(),
            9,
        )
        .unwrap();
        let display_only = ExtensionValueRowV1::try_new(
            &PluginItemResultV1::value(
                PluginValueV1::text("display only").unwrap(),
                ROption::RNone,
            ),
            ROption::RNone,
            "display only".into(),
            10,
        )
        .unwrap();
        assert_eq!(
            display_only.value(),
            Some(ExtensionValueViewV1::Text("display only"))
        );
        assert_eq!(
            compare_extension_rows_v1(&display_only, &absent, ExtensionSortDirectionV1::Ascending,),
            Ordering::Less
        );
        assert_eq!(
            compare_extension_rows_v1(&display_only, &absent, ExtensionSortDirectionV1::Descending,),
            Ordering::Less
        );
        assert_eq!(
            compare_extension_rows_v1(&rows[0], &absent, ExtensionSortDirectionV1::Ascending),
            compare_extension_rows_v1(&rows[0], &absent, ExtensionSortDirectionV1::Descending)
        );
        for first in &rows {
            for second in &rows {
                for third in &rows {
                    let direction = ExtensionSortDirectionV1::Ascending;
                    if compare_extension_rows_v1(first, second, direction) != Ordering::Greater
                        && compare_extension_rows_v1(second, third, direction) != Ordering::Greater
                    {
                        assert_ne!(
                            compare_extension_rows_v1(first, third, direction),
                            Ordering::Greater
                        );
                    }
                }
            }
        }
        assert_eq!(
            compare_extension_rows_v1(&rows[1], &rows[2], ExtensionSortDirectionV1::Ascending),
            Ordering::Greater
        );
    }

    #[test]
    fn revoked_generation_invalidates_every_row_clone() {
        let result =
            PluginItemResultV1::value(PluginValueV1::text("retained").unwrap(), ROption::RNone);
        let row =
            ExtensionValueRowV1::try_new(&result, ROption::RNone, "retained".into(), 1).unwrap();
        let clone = row.clone();
        row.generation.revoke();
        assert_eq!(row.value(), None);
        assert_eq!(clone.value(), None);
        assert_eq!(clone.outcome(), PluginItemOutcomeV1::INCOMPATIBLE);
    }

    #[test]
    fn terminal_truth_table_and_mixed_sort_domains_fail_closed() {
        assert_eq!(
            project_terminal_outcome_v1(JobTerminalV1::COMPLETED, false),
            PluginItemOutcomeV1::INCOMPATIBLE
        );
        assert_eq!(
            project_terminal_outcome_v1(JobTerminalV1::UNSUPPORTED, false),
            PluginItemOutcomeV1::UNSUPPORTED
        );
        assert_eq!(
            project_terminal_outcome_v1(JobTerminalV1::UNAVAILABLE, false),
            PluginItemOutcomeV1::UNAVAILABLE
        );
        assert_eq!(
            project_terminal_outcome_v1(JobTerminalV1::CANCELLED, false),
            PluginItemOutcomeV1::CANCELLED
        );
        assert_eq!(
            project_terminal_outcome_v1(JobTerminalV1::PLUGIN_ERROR, false),
            PluginItemOutcomeV1::PLUGIN_ERROR
        );
        assert_eq!(
            project_terminal_outcome_v1(JobTerminalV1::INCOMPATIBLE, true),
            PluginItemOutcomeV1::INCOMPATIBLE
        );
        let result = PluginItemResultV1::value(
            PluginValueV1::integer(1),
            ROption::RSome(StableSortValueV1::text("x").unwrap()),
        );
        assert!(
            ExtensionValueRowV1::try_new(
                &result,
                ROption::RSome(StableSortValueKindV1::U64),
                "x".into(),
                1
            )
            .is_none()
        );
    }

    #[test]
    fn opaque_binding_denies_kind_package_digest_epoch_and_schema_drift() {
        let schema = explorer_extension_api::StableIdV1::new(
            explorer_extension_api::IdNamespaceV1::new(1, 2),
            9,
        );
        let source_authority = ExtensionJobAuthorityV1::for_test_opaque(
            "pkg",
            "source",
            ContributionKindV1::Column,
            schema,
            3,
            Some("renderer"),
        );
        let source = source_authority.producer().clone();
        let opaque_result = PluginItemResultV1::value(
            PluginValueV1::opaque(schema, 3, vec![1]).unwrap(),
            ROption::RNone,
        );
        let accepted_row = ExtensionValueRowV1::from_host(
            HostPluginItemResultV1::from_abi(&opaque_result, ROption::RNone, Some(&source))
                .unwrap(),
            "opaque".into(),
            1,
        );
        assert_eq!(
            accepted_row.value(),
            Some(ExtensionValueViewV1::Opaque {
                schema,
                schema_version: 3,
            })
        );
        let renderer = ExtensionJobAuthorityV1::for_test("pkg");
        assert_eq!(
            OpaquePayloadBindingV1::bind(&accepted_row, renderer).unwrap_err(),
            OpaquePayloadRouteErrorV1::BindingDenied
        );
        let renderer = ExtensionJobAuthorityV1::for_test_opaque(
            "pkg",
            "renderer",
            ContributionKindV1::GpuiRenderer,
            schema,
            3,
            None,
        );
        let binding = OpaquePayloadBindingV1::bind(&accepted_row, renderer).unwrap();
        assert_eq!(binding.route().unwrap().bytes, vec![1]);
        let mismatched_result = PluginItemResultV1::value(
            PluginValueV1::opaque(schema, 2, vec![1]).unwrap(),
            ROption::RNone,
        );
        assert!(
            HostPluginItemResultV1::from_abi(&mismatched_result, ROption::RNone, Some(&source),)
                .is_none()
        );

        assert_eq!(binding.route().unwrap().bytes, vec![1]);
    }
}
