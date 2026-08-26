//! Frozen data-only dynamic-column descriptor contract.

use abi_stable::{
    StableAbi,
    std_types::{ROption, RString},
};

use crate::{PluginValueKindV1, StableIdV1, StableSortValueKindV1};

macro_rules! wire_enum {
    ($name:ident { $($constant:ident = $value:expr),+ $(,)? }) => {
        #[repr(transparent)]
        #[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
        pub struct $name(u32);
        impl $name {
            $(pub const $constant: Self = Self($value);)+
            #[must_use] pub const fn from_raw(raw: u32) -> Self { Self(raw) }
            #[must_use] pub const fn into_raw(self) -> u32 { self.0 }
            #[allow(clippy::manual_range_patterns)]
            #[must_use] pub const fn is_known(self) -> bool {
                matches!(self.0, $($value)|+)
            }
        }
    };
}

wire_enum!(ColumnAlignmentV1 { START = 1, CENTER = 2, END = 3 });
wire_enum!(ColumnApplicabilityV1 { ALL_ENTRIES = 1, FILES = 2, CONTAINERS = 3 });
wire_enum!(ColumnProviderCostV1 {
    IMMEDIATE = 1,
    BACKGROUND_SINGLE = 2,
    BACKGROUND_BATCH = 3,
    BACKGROUND_AGGREGATE = 4,
});

/// Closed bit set describing the filesystems on which a column may run.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct ColumnFileSystemsV1(u32);

impl ColumnFileSystemsV1 {
    pub const NONE: Self = Self(0);
    pub const LOCAL: Self = Self(1 << 0);
    pub const ADB: Self = Self(1 << 1);
    pub const SFTP: Self = Self(1 << 2);
    pub const REMOTE: Self = Self(Self::ADB.0 | Self::SFTP.0);
    pub const ALL: Self = Self(Self::LOCAL.0 | Self::REMOTE.0);

    #[must_use]
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }
    #[must_use]
    pub const fn into_raw(self) -> u32 {
        self.0
    }
    #[must_use]
    pub const fn is_known(self) -> bool {
        self.0 & !Self::ALL.0 == 0
    }
}

/// Package-local canonical column ID. The authority envelope supplies the
/// package namespace, so two packages may safely use the same local ID.
#[repr(transparent)]
#[derive(Clone, Debug, Eq, PartialEq, StableAbi)]
pub struct ColumnLocalIdV1(pub RString);

impl ColumnLocalIdV1 {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        let value = self.0.as_str();
        !value.is_empty()
            && value.len() <= 64
            && value.bytes().enumerate().all(|(index, byte)| match byte {
                b'a'..=b'z' | b'0'..=b'9' => true,
                b'.' | b'_' | b'-' => index > 0,
                _ => false,
            })
    }
}

/// Optional aggregate dependency and bounded output declaration.
#[repr(C)]
#[derive(Clone, Debug, StableAbi)]
pub struct ColumnAggregateDescriptorV1 {
    pub aggregate_interface_id: StableIdV1,
    pub dependency_column: ColumnLocalIdV1,
    pub maximum_output_values: u32,
}

/// Optional data-only renderer binding. Rendering consumes only the immutable
/// public context and returns a host-painted plan.
#[repr(C)]
#[derive(Clone, Debug, StableAbi)]
pub struct ColumnRendererDescriptorV1 {
    pub renderer_interface_id: StableIdV1,
    pub renderer_contribution_id: RString,
    pub accepted_value_kind: PluginValueKindV1,
}

/// Complete public presentation/provider contract for one dynamic column.
///
/// Authors cannot attach a handwritten ABI callback table; provider objects
/// are created only through the SDK-owned ordinary-Rust adapters.
///
/// ```compile_fail
/// use explorer_extension_api::ColumnDescriptorV1;
/// extern "C" fn handwritten_callback() {}
/// let _ = ColumnDescriptorV1 {
///     raw_callback: handwritten_callback,
///     ..todo!()
/// };
/// ```
#[repr(C)]
#[derive(Clone, Debug, StableAbi)]
pub struct ColumnDescriptorV1 {
    pub id: ColumnLocalIdV1,
    pub display_name: RString,
    pub value_kind: PluginValueKindV1,
    pub default_width: u16,
    pub minimum_width: u16,
    pub maximum_width: u16,
    pub alignment: ColumnAlignmentV1,
    pub applicability: ColumnApplicabilityV1,
    pub file_systems: ColumnFileSystemsV1,
    pub cost: ColumnProviderCostV1,
    pub stable_sort_kind: ROption<StableSortValueKindV1>,
    pub provider_interface_id: StableIdV1,
    pub provider_contribution_id: RString,
    pub aggregate: ROption<ColumnAggregateDescriptorV1>,
    pub renderer: ROption<ColumnRendererDescriptorV1>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColumnDescriptorErrorV1 {
    InvalidId,
    InvalidDisplayName,
    InvalidWidth,
    UnknownSemantic,
    InvalidProvider,
    InvalidAggregate,
    InvalidRenderer,
}

impl ColumnDescriptorV1 {
    /// Host-side shape validation. Unknown numeric semantics are preserved by
    /// decoding but rejected until a newer host explicitly supports them.
    ///
    /// # Errors
    ///
    /// Returns [`ColumnDescriptorErrorV1`] when an identifier, width,
    /// semantic, provider, aggregate, or renderer binding is invalid.
    pub fn validate(&self) -> Result<(), ColumnDescriptorErrorV1> {
        if !self.id.is_valid() {
            return Err(ColumnDescriptorErrorV1::InvalidId);
        }
        if self.display_name.trim().is_empty()
            || self.display_name.len() > 256
            || self.display_name.chars().any(char::is_control)
        {
            return Err(ColumnDescriptorErrorV1::InvalidDisplayName);
        }
        if self.minimum_width < 48
            || self.minimum_width > self.default_width
            || self.default_width > self.maximum_width
            || self.maximum_width > 1_200
        {
            return Err(ColumnDescriptorErrorV1::InvalidWidth);
        }
        if !self.alignment.is_known()
            || !self.applicability.is_known()
            || !self.file_systems.is_known()
            || !self.cost.is_known()
            || self.value_kind.into_raw() == 0
        {
            return Err(ColumnDescriptorErrorV1::UnknownSemantic);
        }
        if !self.provider_interface_id.is_valid()
            || !valid_contribution_id(self.provider_contribution_id.as_str())
        {
            return Err(ColumnDescriptorErrorV1::InvalidProvider);
        }
        if let ROption::RSome(aggregate) = &self.aggregate
            && (!aggregate.aggregate_interface_id.is_valid()
                || !aggregate.dependency_column.is_valid()
                || aggregate.maximum_output_values == 0
                || aggregate.maximum_output_values > 4_096)
        {
            return Err(ColumnDescriptorErrorV1::InvalidAggregate);
        }
        if let ROption::RSome(renderer) = &self.renderer
            && (!renderer.renderer_interface_id.is_valid()
                || !valid_contribution_id(renderer.renderer_contribution_id.as_str())
                || renderer.accepted_value_kind != self.value_kind)
        {
            return Err(ColumnDescriptorErrorV1::InvalidRenderer);
        }
        Ok(())
    }
}

fn valid_contribution_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EXTENSION_ID_NAMESPACE_V1;

    fn descriptor() -> ColumnDescriptorV1 {
        ColumnDescriptorV1 {
            id: ColumnLocalIdV1(RString::from("folder-size")),
            display_name: RString::from("Folder size"),
            value_kind: PluginValueKindV1::BYTES,
            default_width: 144,
            minimum_width: 48,
            maximum_width: 600,
            alignment: ColumnAlignmentV1::END,
            applicability: ColumnApplicabilityV1::CONTAINERS,
            file_systems: ColumnFileSystemsV1::LOCAL,
            cost: ColumnProviderCostV1::BACKGROUND_AGGREGATE,
            stable_sort_kind: ROption::RSome(StableSortValueKindV1::BYTES),
            provider_interface_id: StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 10),
            provider_contribution_id: RString::from("folder-size.provider"),
            aggregate: ROption::RSome(ColumnAggregateDescriptorV1 {
                aggregate_interface_id: StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 11),
                dependency_column: ColumnLocalIdV1(RString::from("folder-size")),
                maximum_output_values: 1,
            }),
            renderer: ROption::RSome(ColumnRendererDescriptorV1 {
                renderer_interface_id: StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 12),
                renderer_contribution_id: RString::from("folder-size.renderer"),
                accepted_value_kind: PluginValueKindV1::BYTES,
            }),
        }
    }

    #[test]
    fn complete_descriptor_is_data_only_and_validated() {
        assert_eq!(descriptor().validate(), Ok(()));
        assert!(std::mem::size_of::<ColumnDescriptorV1>() > 0);
    }

    #[test]
    fn malformed_and_unknown_non_exhaustive_semantics_fail_closed() {
        let mut malformed = descriptor();
        malformed.alignment = ColumnAlignmentV1::from_raw(99);
        assert_eq!(
            malformed.validate(),
            Err(ColumnDescriptorErrorV1::UnknownSemantic)
        );
        let mut wrong_renderer = descriptor();
        if let ROption::RSome(renderer) = &mut wrong_renderer.renderer {
            renderer.accepted_value_kind = PluginValueKindV1::TEXT;
        }
        assert_eq!(
            wrong_renderer.validate(),
            Err(ColumnDescriptorErrorV1::InvalidRenderer)
        );
    }

    fn layout_hash<T: StableAbi>() -> u64 {
        format!("{}", T::LAYOUT)
            .bytes()
            .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
                (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
            })
    }

    #[test]
    fn frozen_column_abi_layout_hashes_are_exact() {
        let hashes = [
            layout_hash::<ColumnDescriptorV1>(),
            layout_hash::<ColumnAggregateDescriptorV1>(),
            layout_hash::<ColumnRendererDescriptorV1>(),
        ];
        assert_eq!(
            hashes,
            [
                0x7e31_2f55_b763_0cb0,
                0xaa96_1f12_e8dc_3838,
                0x65da_86e6_31fc_6fb8,
            ]
        );
    }

    #[test]
    fn public_column_surface_contains_no_private_or_async_types() {
        let source = include_str!("column.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        for forbidden in [
            "explorer_model",
            "explorer_ui",
            "gpui::",
            "std::future",
            "dyn Fn",
            "RawHandle",
            "PathBuf",
        ] {
            assert!(
                !source.contains(forbidden),
                "forbidden public type: {forbidden}"
            );
        }
    }
}
