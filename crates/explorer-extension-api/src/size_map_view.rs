//! Public, data-only Size Map view ABI contract.
//!
//! The host owns scanning, selection, navigation, and GPUI rendering.  A
//! plugin receives a copied node snapshot and returns only normalized treemap
//! rectangles.  No filesystem path, native handle, private Explorer object, or
//! GPUI entity crosses this boundary.

use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::atomic::{AtomicU8, Ordering},
};

use abi_stable::{
    StableAbi, sabi_trait,
    std_types::{RBox, ROption, RString, RVec},
};

use crate::{CellColorV1, CellThemeV1, StableIdV1, dispose_caught_panic_payload_v1};

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

/// Immutable, data-only input for one Size Map render/layout callback.
#[repr(C)]
#[derive(Clone, Debug, StableAbi)]
pub struct SizeMapRenderContextV1 {
    /// Host location/refresh generation.  A renderer must echo this value in
    /// its plan; the host rejects plans for another generation.
    pub generation: u64,
    pub nodes: RVec<SizeMapNodeV1>,
    pub viewport: SizeMapViewportV1,
    pub theme: CellThemeV1,
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
    pub rectangles: RVec<SizeMapRectangleV1>,
    pub status: RString,
}

impl SizeMapRenderPlanV1 {
    #[must_use]
    pub fn empty(generation: u64, status: impl Into<RString>) -> Self {
        Self {
            generation,
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
}

/// Private SDK-owned ABI vtable. Plugin authors never implement this trait.
#[sabi_trait]
#[doc(hidden)]
pub trait AbiSizeMapViewObjectV1: Send + Sync {
    #[sabi(last_prefix_field)]
    fn render_size_map(&self, context: SizeMapRenderContextV1) -> SizeMapRenderPlanV1;
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
    fn render_size_map(&self, context: SizeMapRenderContextV1) -> SizeMapRenderPlanV1 {
        if !self.enter() {
            return SizeMapRenderPlanV1::empty(
                context.generation,
                "Size Map renderer is unavailable",
            );
        }
        let Some(implementation) = self.implementation.as_ref() else {
            self.fault();
            return SizeMapRenderPlanV1::empty(
                context.generation,
                "Size Map renderer is unavailable",
            );
        };
        match catch_unwind(AssertUnwindSafe(|| {
            implementation.render_size_map(context.clone())
        })) {
            Ok(mut plan) => {
                self.leave();
                plan.normalize_geometry();
                plan
            }
            Err(payload) => {
                self.fault();
                dispose_caught_panic_payload_v1(payload);
                SizeMapRenderPlanV1::empty(context.generation, "Size Map renderer panicked")
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
    pub fn render_size_map(&self, context: SizeMapRenderContextV1) -> SizeMapRenderPlanV1 {
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
        let plan = object.render_size_map(SizeMapRenderContextV1 {
            generation: 41,
            nodes: RVec::new(),
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
            settings: RString::new(),
        });
        assert_eq!(plan.generation, 41);
        assert_eq!(plan.rectangles[0].x_millionths, 1_000_000);
        assert_eq!(plan.rectangles[0].width_millionths, 1_000_000);
    }
}
