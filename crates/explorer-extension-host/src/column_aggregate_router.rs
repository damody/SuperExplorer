//! Bounded, generation-scoped routing for dynamic-column aggregates.

use explorer_extension_api::PluginValueV1;
use explorer_model::ColumnId;

use crate::{ColumnAuthorityRegistryErrorV1, HostColumnAuthorityRegistryV1};

pub const MAX_AGGREGATE_INPUT_VALUES_V1: usize = 4_096;
pub const MAX_AGGREGATE_INPUT_BYTES_V1: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct ColumnAggregateRequestV1 {
    pub package_id: String,
    pub feature_id: String,
    pub interface_id: String,
    pub incarnation: u64,
    pub generation: u64,
    pub request_generation: u64,
    pub column_id: ColumnId,
    pub values: Vec<PluginValueV1>,
    pub maximum_output_values: usize,
}

#[derive(Clone, Debug)]
pub struct ColumnAggregateResultV1 {
    pub request_generation: u64,
    pub values: Vec<PluginValueV1>,
    pub complete: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ColumnAggregateRouteErrorV1 {
    Authority(ColumnAuthorityRegistryErrorV1),
    InvalidGeneration,
    InputLimitExceeded,
    InputByteLimitExceeded,
    OutputLimitExceeded,
    PartialResult,
}

/// Routes one aggregate callback only after checking live column authority and
/// accepts its result only when the same request generation returns complete.
pub fn route_column_aggregate_v1(
    authority: &HostColumnAuthorityRegistryV1,
    request: ColumnAggregateRequestV1,
    aggregate: impl FnOnce(&[PluginValueV1], u64) -> ColumnAggregateResultV1,
) -> Result<ColumnAggregateResultV1, ColumnAggregateRouteErrorV1> {
    if request.request_generation == 0 || request.maximum_output_values == 0 {
        return Err(ColumnAggregateRouteErrorV1::InvalidGeneration);
    }
    authority
        .authorize_dispatch(
            &request.package_id,
            &request.feature_id,
            &request.interface_id,
            request.incarnation,
            request.generation,
            &request.column_id,
        )
        .map_err(ColumnAggregateRouteErrorV1::Authority)?;
    if request.values.len() > MAX_AGGREGATE_INPUT_VALUES_V1 {
        return Err(ColumnAggregateRouteErrorV1::InputLimitExceeded);
    }
    let input_bytes = request.values.iter().try_fold(0_usize, |total, value| {
        total
            .checked_add(value.text.len())?
            .checked_add(value.payload.len())
    });
    if input_bytes.is_none_or(|bytes| bytes > MAX_AGGREGATE_INPUT_BYTES_V1) {
        return Err(ColumnAggregateRouteErrorV1::InputByteLimitExceeded);
    }

    let result = aggregate(&request.values, request.request_generation);
    if result.request_generation != request.request_generation {
        return Err(ColumnAggregateRouteErrorV1::InvalidGeneration);
    }
    if !result.complete {
        return Err(ColumnAggregateRouteErrorV1::PartialResult);
    }
    if result.values.len() > request.maximum_output_values {
        return Err(ColumnAggregateRouteErrorV1::OutputLimitExceeded);
    }
    authority
        .authorize_dispatch(
            &request.package_id,
            &request.feature_id,
            &request.interface_id,
            request.incarnation,
            request.generation,
            &request.column_id,
        )
        .map_err(ColumnAggregateRouteErrorV1::Authority)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ColumnFeatureRuntimeStateV1, SealedColumnRegistrationV1};
    use explorer_model::{
        ColumnAlignment, ColumnApplicability, ColumnCost, ColumnDescriptor, ColumnSortSemantics,
        ColumnValueType,
    };

    fn authority() -> (HostColumnAuthorityRegistryV1, ColumnId) {
        let id = ColumnId::extension("org.example.aggregate", "size").unwrap();
        let mut authority = HostColumnAuthorityRegistryV1::default();
        authority
            .replace_package(SealedColumnRegistrationV1 {
                package_id: "org.example.aggregate".into(),
                feature_id: "columns".into(),
                interface_id: "aggregate.v1".into(),
                incarnation: 7,
                generation: 9,
                state: ColumnFeatureRuntimeStateV1::Enabled,
                descriptors: vec![ColumnDescriptor {
                    id: id.clone(),
                    display_name: "Size".into(),
                    value_type: ColumnValueType::Bytes,
                    default_width: 120,
                    minimum_width: 48,
                    maximum_width: 600,
                    alignment: ColumnAlignment::End,
                    applicability: ColumnApplicability::AllEntries,
                    file_systems: explorer_model::ColumnFileSystems::LOCAL,
                    sort_semantics: ColumnSortSemantics::Bytes,
                    cost: ColumnCost::BackgroundAggregate,
                }],
            })
            .unwrap();
        (authority, id)
    }

    fn request(id: ColumnId) -> ColumnAggregateRequestV1 {
        ColumnAggregateRequestV1 {
            package_id: "org.example.aggregate".into(),
            feature_id: "columns".into(),
            interface_id: "aggregate.v1".into(),
            incarnation: 7,
            generation: 9,
            request_generation: 11,
            column_id: id,
            values: vec![PluginValueV1::integer(3), PluginValueV1::integer(8)],
            maximum_output_values: 1,
        }
    }

    #[test]
    fn accepts_one_bounded_complete_result_for_current_generation() {
        let (authority, id) = authority();
        let result = route_column_aggregate_v1(&authority, request(id), |values, generation| {
            ColumnAggregateResultV1 {
                request_generation: generation,
                values: vec![values[1].clone()],
                complete: true,
            }
        })
        .unwrap();
        assert_eq!(result.values[0].integer, 8);
    }

    #[test]
    fn rejects_stale_partial_and_oversized_results() {
        let (authority, id) = authority();
        let stale = route_column_aggregate_v1(&authority, request(id.clone()), |_, _| {
            ColumnAggregateResultV1 {
                request_generation: 10,
                values: Vec::new(),
                complete: true,
            }
        });
        assert!(matches!(
            stale,
            Err(ColumnAggregateRouteErrorV1::InvalidGeneration)
        ));
        let partial =
            route_column_aggregate_v1(&authority, request(id.clone()), |_, generation| {
                ColumnAggregateResultV1 {
                    request_generation: generation,
                    values: Vec::new(),
                    complete: false,
                }
            });
        assert!(matches!(
            partial,
            Err(ColumnAggregateRouteErrorV1::PartialResult)
        ));
        let oversized = route_column_aggregate_v1(&authority, request(id), |_, generation| {
            ColumnAggregateResultV1 {
                request_generation: generation,
                values: vec![PluginValueV1::integer(1), PluginValueV1::integer(2)],
                complete: true,
            }
        });
        assert!(matches!(
            oversized,
            Err(ColumnAggregateRouteErrorV1::OutputLimitExceeded)
        ));
    }
}
