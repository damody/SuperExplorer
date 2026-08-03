//! Public, data-only Size Map view ABI contract.
//!
//! The host owns scanning, selection, navigation, and GPUI rendering.  A
//! plugin receives a copied node snapshot and returns only normalized treemap
//! rectangles.  No filesystem path, native handle, private Explorer object, or
//! GPUI entity crosses this boundary.

use std::{
    collections::HashSet,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::atomic::{AtomicU8, Ordering},
};

use abi_stable::{
    StableAbi, sabi_trait,
    std_types::{RBox, ROption, RResult, RString, RVec},
};

use crate::{
    AbiErrorCodeV1, AbiErrorV1, AbiResultV1, CellColorV1, CellThemeV1, ROOT_MODULE_CONTRACT_ID_V1,
    StableIdV1, dispose_caught_panic_payload_v1,
};

/// Maximum rectangles accepted from one Size Map renderer invocation.
pub const MAX_SIZE_MAP_RECTANGLES_V1: usize = 4_096;
/// Maximum UTF-8 bytes accepted for one rectangle label.
pub const MAX_SIZE_MAP_LABEL_BYTES_V1: usize = 256;
/// Maximum UTF-8 bytes accepted for one rectangle detail.
pub const MAX_SIZE_MAP_DETAIL_BYTES_V1: usize = 512;
/// Maximum UTF-8 bytes accepted for the plan status.
pub const MAX_SIZE_MAP_STATUS_BYTES_V1: usize = 1_024;
/// Maximum aggregate UTF-8 bytes accepted for a complete plan.
pub const MAX_SIZE_MAP_PLAN_TEXT_BYTES_V1: usize = 2 * 1024 * 1024;

/// The semantic kind of a Size Map node.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct SizeMapNodeKindV1(u32);

impl SizeMapNodeKindV1 {
    pub const DIRECTORY: Self = Self(1);
    pub const FILE: Self = Self(2);
    pub const OTHER: Self = Self(3);

    #[must_use]
    pub const fn into_raw(self) -> u32 {
        self.0
    }
}

/// The host-owned scan status attached to a node snapshot.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct SizeMapNodeStatusV1(u32);

impl SizeMapNodeStatusV1 {
    /// All descendants and exact logical bytes are known.
    pub const COMPLETE: Self = Self(1);
    /// A best-effort aggregate is available but must not be shown as exact.
    pub const PARTIAL: Self = Self(2);
    pub const UNAVAILABLE: Self = Self(3);
    pub const FAILED: Self = Self(4);

    #[must_use]
    pub const fn into_raw(self) -> u32 {
        self.0
    }
}

/// One opaque, copied directory-tree node supplied to a Size Map renderer.
#[repr(C)]
#[derive(Clone, Debug, StableAbi)]
pub struct SizeMapNodeV1 {
    pub node_id: StableIdV1,
    pub parent_id: ROption<StableIdV1>,
    pub name: RString,
    pub kind: SizeMapNodeKindV1,
    /// Present only when the host has an exact logical-byte total.
    pub exact_bytes: ROption<u64>,
    pub status: SizeMapNodeStatusV1,
}

/// A viewport in host logical pixels multiplied by 1,000.
///
/// Integer transport avoids cross-DLL NaN/infinity semantics.  Rectangles in
/// the returned plan use normalized millionths instead, so the host can map
/// them to the current GPUI layout without giving the plugin a GPUI type.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct SizeMapViewportV1 {
    pub width_milli: u32,
    pub height_milli: u32,
    pub dpi_milli: u32,
}

/// Immutable, data-only input for one worker-safe Size Map render/layout
/// callback. The host owns GPUI painting and invokes this synchronous ABI call
/// only on a bounded worker; no GPUI object or render context crosses the ABI.
#[repr(C)]
#[derive(Clone, Debug, StableAbi)]
pub struct SizeMapRenderContextV1 {
    /// Host location/refresh generation.  A renderer must echo this value in
    /// its plan; the host rejects plans for another generation.
    pub generation: u64,
    /// Host-minted revision for this complete copied snapshot (nodes, status,
    /// viewport, theme, selection, and settings). A plan must echo it exactly.
    pub render_revision: u64,
    pub nodes: RVec<SizeMapNodeV1>,
    pub viewport: SizeMapViewportV1,
    pub theme: CellThemeV1,
    /// Opaque selected node identities in this snapshot.
    pub selected_node_ids: RVec<StableIdV1>,
    /// Extension-owned UTF-8 display settings selected by the host.
    pub settings: RString,
}

/// A normalized treemap rectangle.  Coordinates and extents are fractions of
/// the host viewport in millionths; the host clamps them before GPUI drawing.
#[repr(C)]
#[derive(Clone, Debug, StableAbi)]
pub struct SizeMapRectangleV1 {
    pub node_id: StableIdV1,
    pub x_millionths: u32,
    pub y_millionths: u32,
    pub width_millionths: u32,
    pub height_millionths: u32,
    pub color: CellColorV1,
    pub label: RString,
    pub detail: RString,
}

impl SizeMapRectangleV1 {
    /// Normalizes all externally supplied geometry to the frozen [0, 1]
    /// millionths range.  The host still clips overlapping/overflowing output.
    pub fn clamp_geometry(&mut self) {
        self.x_millionths = self.x_millionths.min(1_000_000);
        self.y_millionths = self.y_millionths.min(1_000_000);
        self.width_millionths = self.width_millionths.min(1_000_000);
        self.height_millionths = self.height_millionths.min(1_000_000);
    }
}

/// Pure-data output consumed by a host-owned GPUI Size Map element.
#[repr(C)]
#[derive(Clone, Debug, StableAbi)]
pub struct SizeMapRenderPlanV1 {
    pub generation: u64,
    /// Echo of [`SizeMapRenderContextV1::render_revision`].
    pub render_revision: u64,
    pub rectangles: RVec<SizeMapRectangleV1>,
    pub status: RString,
}

impl SizeMapRenderPlanV1 {
    #[must_use]
    pub fn empty(generation: u64, render_revision: u64, status: impl Into<RString>) -> Self {
        Self {
            generation,
            render_revision,
            rectangles: RVec::new(),
            status: status.into(),
        }
    }

    /// Clamps rectangle coordinate units at the ABI boundary.  The host owns
    /// final clipping and hit testing.
    pub fn normalize_geometry(&mut self) {
        for rectangle in &mut self.rectangles {
            rectangle.clamp_geometry();
        }
    }

    fn validate_for_context(&self, context: &SizeMapRenderContextV1) -> bool {
        if self.generation != context.generation
            || self.render_revision != context.render_revision
            || self.rectangles.len() > context.nodes.len().min(MAX_SIZE_MAP_RECTANGLES_V1)
            || self.status.len() > MAX_SIZE_MAP_STATUS_BYTES_V1
        {
            return false;
        }
        let known = context
            .nodes
            .iter()
            .map(|node| node.node_id)
            .collect::<HashSet<_>>();
        let mut seen = HashSet::with_capacity(self.rectangles.len());
        let mut text_bytes = self.status.len();
        for rectangle in &self.rectangles {
            if !known.contains(&rectangle.node_id)
                || !seen.insert(rectangle.node_id)
                || rectangle.label.len() > MAX_SIZE_MAP_LABEL_BYTES_V1
                || rectangle.detail.len() > MAX_SIZE_MAP_DETAIL_BYTES_V1
            {
                return false;
            }
            text_bytes = text_bytes
                .saturating_add(rectangle.label.len())
                .saturating_add(rectangle.detail.len());
            if text_bytes > MAX_SIZE_MAP_PLAN_TEXT_BYTES_V1 {
                return false;
            }
        }
        true
    }
}

/// Private SDK-owned ABI vtable. Plugin authors never implement this trait.
#[sabi_trait]
#[doc(hidden)]
pub trait AbiSizeMapViewObjectV1: Send + Sync {
    #[sabi(last_prefix_field)]
    fn render_size_map(&self, context: SizeMapRenderContextV1) -> AbiResultV1<SizeMapRenderPlanV1>;
}

/// Opaque ABI-safe Size Map renderer retained by the host runtime.
#[repr(transparent)]
#[derive(StableAbi)]
pub struct SizeMapViewObjectV1(AbiSizeMapViewObjectV1_TO<'static, RBox<()>>);

/// Ordinary Rust author surface for a Size Map view renderer.
pub trait SizeMapViewImplementationV1: Send + Sync {
    fn render_size_map(&self, context: SizeMapRenderContextV1) -> SizeMapRenderPlanV1;
}

const VIEW_IDLE_V1: u8 = 0;
const VIEW_RUNNING_V1: u8 = 1;
const VIEW_FAULTED_V1: u8 = 2;

struct SizeMapViewAdapterV1<T> {
    implementation: Option<T>,
    invocation_state: AtomicU8,
}

impl<T: SizeMapViewImplementationV1> SizeMapViewAdapterV1<T> {
    fn enter(&self) -> bool {
        self.invocation_state
            .compare_exchange(
                VIEW_IDLE_V1,
                VIEW_RUNNING_V1,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    }

    fn leave(&self) {
        self.invocation_state.store(VIEW_IDLE_V1, Ordering::Release);
    }

    fn fault(&self) {
        self.invocation_state
            .store(VIEW_FAULTED_V1, Ordering::Release);
    }
}

impl<T: SizeMapViewImplementationV1> AbiSizeMapViewObjectV1 for SizeMapViewAdapterV1<T> {
    fn render_size_map(&self, context: SizeMapRenderContextV1) -> AbiResultV1<SizeMapRenderPlanV1> {
        if !self.enter() {
            return RResult::RErr(AbiErrorV1::new(
                AbiErrorCodeV1::CALLBACK_UNAVAILABLE,
                ROOT_MODULE_CONTRACT_ID_V1,
                1,
            ));
        }
        let Some(implementation) = self.implementation.as_ref() else {
            self.fault();
            return RResult::RErr(AbiErrorV1::new(
                AbiErrorCodeV1::CALLBACK_UNAVAILABLE,
                ROOT_MODULE_CONTRACT_ID_V1,
                2,
            ));
        };
        match catch_unwind(AssertUnwindSafe(|| {
            implementation.render_size_map(context.clone())
        })) {
            Ok(mut plan) if plan.validate_for_context(&context) => {
                plan.normalize_geometry();
                self.leave();
                RResult::ROk(plan)
            }
            Ok(_) => {
                self.fault();
                RResult::RErr(AbiErrorV1::new(
                    AbiErrorCodeV1::MALFORMED_CALLBACK_OUTPUT,
                    ROOT_MODULE_CONTRACT_ID_V1,
                    1,
                ))
            }
            Err(payload) => {
                self.fault();
                dispose_caught_panic_payload_v1(payload);
                RResult::RErr(AbiErrorV1::new(
                    AbiErrorCodeV1::CALLBACK_PANICKED,
                    ROOT_MODULE_CONTRACT_ID_V1,
                    1,
                ))
            }
        }
    }
}

impl<T> Drop for SizeMapViewAdapterV1<T> {
    fn drop(&mut self) {
        if let Some(implementation) = self.implementation.take()
            && let Err(payload) = catch_unwind(AssertUnwindSafe(|| drop(implementation)))
        {
            dispose_caught_panic_payload_v1(payload);
        }
    }
}

impl SizeMapViewObjectV1 {
    /// Wraps an ordinary Rust view implementation in the SDK-owned ABI object.
    #[must_use]
    pub fn new<T: SizeMapViewImplementationV1 + 'static>(implementation: T) -> Self {
        Self(AbiSizeMapViewObjectV1_TO::from_value(
            SizeMapViewAdapterV1 {
                implementation: Some(implementation),
                invocation_state: AtomicU8::new(VIEW_IDLE_V1),
            },
            sabi_trait::TD_Opaque,
        ))
    }

    #[doc(hidden)]
    #[must_use]
    pub fn render_size_map(
        &self,
        context: SizeMapRenderContextV1,
    ) -> AbiResultV1<SizeMapRenderPlanV1> {
        self.0.render_size_map(context)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EXTENSION_ID_NAMESPACE_V1;

    struct Example;

    impl SizeMapViewImplementationV1 for Example {
        fn render_size_map(&self, context: SizeMapRenderContextV1) -> SizeMapRenderPlanV1 {
            SizeMapRenderPlanV1 {
                generation: context.generation,
                render_revision: context.render_revision,
                rectangles: RVec::from(vec![SizeMapRectangleV1 {
                    node_id: StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 1),
                    x_millionths: 1_500_000,
                    y_millionths: 0,
                    width_millionths: 2_000_000,
                    height_millionths: 1,
                    color: context.theme.accent,
                    label: RString::from("fixture"),
                    detail: RString::new(),
                }]),
                status: RString::from("ready"),
            }
        }
    }

    #[test]
    fn sdk_owned_view_adapter_only_exposes_normalized_data_plan() {
        let color = CellColorV1::rgba(1, 2, 3, 255);
        let object = SizeMapViewObjectV1::new(Example);
        let plan = object
            .render_size_map(SizeMapRenderContextV1 {
                generation: 41,
                render_revision: 99,
                nodes: RVec::from(vec![SizeMapNodeV1 {
                    node_id: StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 1),
                    parent_id: ROption::RNone,
                    name: RString::from("fixture"),
                    kind: SizeMapNodeKindV1::FILE,
                    exact_bytes: ROption::RSome(1),
                    status: SizeMapNodeStatusV1::COMPLETE,
                }]),
                viewport: SizeMapViewportV1 {
                    width_milli: 1_000,
                    height_milli: 1_000,
                    dpi_milli: 1_000,
                },
                theme: CellThemeV1 {
                    foreground: color,
                    muted_foreground: color,
                    background: color,
                    selection_background: color,
                    accent: color,
                },
                selected_node_ids: RVec::new(),
                settings: RString::new(),
            })
            .into_result()
            .expect("valid bounded plan");
        assert_eq!(plan.generation, 41);
        assert_eq!(plan.render_revision, 99);
        assert_eq!(plan.rectangles[0].x_millionths, 1_000_000);
        assert_eq!(plan.rectangles[0].width_millionths, 1_000_000);
    }
}
