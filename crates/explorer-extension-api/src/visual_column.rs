//! Public, data-only visual-column ABI contract.
//!
//! No GPUI entity, private Explorer object, callback table, or native handle
//! crosses this boundary.  Plugin authors implement [`VisualColumnImplementationV1`];
//! the SDK supplies the `abi_stable` adapter stored by registration output.

use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::atomic::{AtomicU8, Ordering},
};

use abi_stable::{
    StableAbi, sabi_trait,
    std_types::{RBox, ROption, RString},
};

use crate::{PluginValueV1, dispose_caught_panic_payload_v1};

/// A data-only RGBA color selected by the host theme or a render plan.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct CellColorV1 {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

impl CellColorV1 {
    #[must_use]
    pub const fn rgba(red: u8, green: u8, blue: u8, alpha: u8) -> Self {
        Self {
            red,
            green,
            blue,
            alpha,
        }
    }
}

/// The palette snapshot available to a cell renderer.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct CellThemeV1 {
    pub foreground: CellColorV1,
    pub muted_foreground: CellColorV1,
    pub background: CellColorV1,
    pub selection_background: CellColorV1,
    pub accent: CellColorV1,
}

/// Largest-sibling aggregation for proportional visual cells.
#[repr(C)]
#[derive(Clone, Debug, StableAbi)]
pub struct CellAggregateV1 {
    /// The exact largest sibling value for the current column, when available.
    pub largest_sibling_value: ROption<PluginValueV1>,
    /// P0 folder-size aggregate used for proportional rendering without
    /// narrowing an exact byte count through a signed integer transport.
    pub largest_sibling_bytes: ROption<u64>,
}

/// Public, immutable input for a GPUI-thread visual-column render callback.
///
/// `settings` is extension-owned UTF-8 data selected by the host.  Renderers
/// must only transform this snapshot into a [`CellRenderPlanV1`]; they must not
/// enumerate files or perform parsing/I/O.
#[repr(C)]
#[derive(Clone, Debug, StableAbi)]
pub struct CellRenderContextV1 {
    pub value: ROption<PluginValueV1>,
    /// P0 folder-size value used for exact byte display and sorting.
    pub exact_bytes: ROption<u64>,
    pub aggregate: ROption<CellAggregateV1>,
    pub loading: bool,
    pub error: ROption<RString>,
    pub selected: bool,
    pub hovered: bool,
    /// Device-pixel scale multiplied by 1,000 (1.0x is `1000`).
    pub dpi_milli: u32,
    pub theme: CellThemeV1,
    pub settings: RString,
}

/// A pure-data render instruction for the host-owned GPUI cell element.
#[repr(C)]
#[derive(Clone, Debug, StableAbi)]
pub struct CellRenderPlanV1 {
    pub label: RString,
    pub detail: RString,
    /// Inclusive fraction of available cell width, in millionths.
    pub proportional_bar_millionths: u32,
    pub text_color: CellColorV1,
    pub bar_color: CellColorV1,
}

impl CellRenderPlanV1 {
    #[must_use]
    pub fn text_only(label: impl Into<RString>, text_color: CellColorV1) -> Self {
        Self {
            label: label.into(),
            detail: RString::new(),
            proportional_bar_millionths: 0,
            text_color,
            bar_color: CellColorV1::rgba(0, 0, 0, 0),
        }
    }

    /// Clamps a caller-supplied fraction to the frozen data-only range.
    pub fn set_proportional_bar_millionths(&mut self, value: u32) {
        self.proportional_bar_millionths = value.min(1_000_000);
    }
}

/// Explicit filesystem-path input for the P0 folder-size measure operation.
///
/// This narrow P0 API is the only visual-column call that may receive a path.
/// The host invokes it on its background worker; render callbacks never receive
/// this request or a path.
#[repr(C)]
#[derive(Clone, Debug, StableAbi)]
pub struct FolderSizeMeasureRequestV1 {
    pub filesystem_path: RString,
    pub max_entries: u32,
    pub max_depth: u16,
    pub deadline_millis: u32,
}

/// Data-only result of a folder-size measurement.
#[repr(C)]
#[derive(Clone, Debug, Eq, PartialEq, StableAbi)]
pub struct FolderSizeMeasureResultV1 {
    pub exact_bytes: u64,
    pub partial: bool,
    pub error: ROption<RString>,
}

impl FolderSizeMeasureResultV1 {
    #[must_use]
    pub const fn complete(exact_bytes: u64) -> Self {
        Self {
            exact_bytes,
            partial: false,
            error: ROption::RNone,
        }
    }

    #[must_use]
    pub fn partial(exact_bytes: u64, error: impl Into<RString>) -> Self {
        Self {
            exact_bytes,
            partial: true,
            error: ROption::RSome(error.into()),
        }
    }
}

/// Private ABI vtable.  Plugin authors implement
/// [`VisualColumnImplementationV1`], never this generated trait.
#[sabi_trait]
#[doc(hidden)]
pub trait AbiVisualColumnObjectV1: Send + Sync {
    fn measure_folder_size(&self, request: FolderSizeMeasureRequestV1)
    -> FolderSizeMeasureResultV1;
    #[sabi(last_prefix_field)]
    fn render(&self, context: CellRenderContextV1) -> CellRenderPlanV1;
}

/// Opaque ABI-safe visual-column object retained by the host runtime.
#[repr(transparent)]
#[derive(StableAbi)]
pub struct VisualColumnObjectV1(AbiVisualColumnObjectV1_TO<'static, RBox<()>>);

/// Ordinary Rust author surface for a visual column.
///
/// A P0 plugin registers separate instances for its background `COLUMN` and
/// GPUI-thread `GPUI_RENDERER` contributions.  The host keeps the instances in
/// separate single-owner runtimes and invokes only the corresponding method.
pub trait VisualColumnImplementationV1: Send + Sync {
    fn measure_folder_size(&self, request: FolderSizeMeasureRequestV1)
    -> FolderSizeMeasureResultV1;
    fn render(&self, context: CellRenderContextV1) -> CellRenderPlanV1;
}

const VISUAL_IDLE_V1: u8 = 0;
const VISUAL_RUNNING_V1: u8 = 1;
const VISUAL_FAULTED_V1: u8 = 2;

struct VisualColumnAdapterV1<T> {
    implementation: Option<T>,
    invocation_state: AtomicU8,
}

impl<T: VisualColumnImplementationV1> VisualColumnAdapterV1<T> {
    fn enter(&self) -> bool {
        self.invocation_state
            .compare_exchange(
                VISUAL_IDLE_V1,
                VISUAL_RUNNING_V1,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    }

    fn leave(&self) {
        self.invocation_state
            .store(VISUAL_IDLE_V1, Ordering::Release);
    }

    fn fault(&self) {
        self.invocation_state
            .store(VISUAL_FAULTED_V1, Ordering::Release);
    }
}

impl<T: VisualColumnImplementationV1> AbiVisualColumnObjectV1 for VisualColumnAdapterV1<T> {
    fn measure_folder_size(
        &self,
        request: FolderSizeMeasureRequestV1,
    ) -> FolderSizeMeasureResultV1 {
        if !self.enter() {
            return FolderSizeMeasureResultV1::partial(0, "visual column is unavailable");
        }
        let Some(implementation) = self.implementation.as_ref() else {
            self.fault();
            return FolderSizeMeasureResultV1::partial(0, "visual column is unavailable");
        };
        match catch_unwind(AssertUnwindSafe(|| {
            implementation.measure_folder_size(request)
        })) {
            Ok(result) => {
                self.leave();
                result
            }
            Err(payload) => {
                self.fault();
                dispose_caught_panic_payload_v1(payload);
                FolderSizeMeasureResultV1::partial(0, "visual column measure panicked")
            }
        }
    }

    fn render(&self, context: CellRenderContextV1) -> CellRenderPlanV1 {
        if !self.enter() {
            return CellRenderPlanV1::text_only(
                "visual column is unavailable",
                context.theme.muted_foreground,
            );
        }
        let Some(implementation) = self.implementation.as_ref() else {
            self.fault();
            return CellRenderPlanV1::text_only(
                "visual column is unavailable",
                context.theme.muted_foreground,
            );
        };
        match catch_unwind(AssertUnwindSafe(|| implementation.render(context.clone()))) {
            Ok(result) => {
                self.leave();
                result
            }
            Err(payload) => {
                self.fault();
                dispose_caught_panic_payload_v1(payload);
                CellRenderPlanV1::text_only(
                    "visual column renderer panicked",
                    context.theme.muted_foreground,
                )
            }
        }
    }
}

impl<T> Drop for VisualColumnAdapterV1<T> {
    fn drop(&mut self) {
        if let Some(implementation) = self.implementation.take()
            && let Err(payload) = catch_unwind(AssertUnwindSafe(|| drop(implementation)))
        {
            dispose_caught_panic_payload_v1(payload);
        }
    }
}

impl VisualColumnObjectV1 {
    /// Wraps an ordinary Rust visual-column implementation in the SDK-owned
    /// `abi_stable` adapter.
    #[must_use]
    pub fn new<T: VisualColumnImplementationV1 + 'static>(implementation: T) -> Self {
        Self(AbiVisualColumnObjectV1_TO::from_value(
            VisualColumnAdapterV1 {
                implementation: Some(implementation),
                invocation_state: AtomicU8::new(VISUAL_IDLE_V1),
            },
            sabi_trait::TD_Opaque,
        ))
    }

    #[doc(hidden)]
    #[must_use]
    pub fn measure_folder_size(
        &self,
        request: FolderSizeMeasureRequestV1,
    ) -> FolderSizeMeasureResultV1 {
        self.0.measure_folder_size(request)
    }

    #[doc(hidden)]
    #[must_use]
    pub fn render(&self, context: CellRenderContextV1) -> CellRenderPlanV1 {
        self.0.render(context)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Example;

    impl VisualColumnImplementationV1 for Example {
        fn measure_folder_size(&self, _: FolderSizeMeasureRequestV1) -> FolderSizeMeasureResultV1 {
            FolderSizeMeasureResultV1::complete(42)
        }

        fn render(&self, context: CellRenderContextV1) -> CellRenderPlanV1 {
            let mut plan = CellRenderPlanV1::text_only("42 B", context.theme.foreground);
            plan.set_proportional_bar_millionths(2_000_000);
            plan
        }
    }

    fn theme() -> CellThemeV1 {
        let color = CellColorV1::rgba(1, 2, 3, 255);
        CellThemeV1 {
            foreground: color,
            muted_foreground: color,
            background: color,
            selection_background: color,
            accent: color,
        }
    }

    #[test]
    fn sdk_owned_visual_adapter_exposes_data_only_measure_and_render_calls() {
        let object = VisualColumnObjectV1::new(Example);
        assert_eq!(
            object.measure_folder_size(FolderSizeMeasureRequestV1 {
                filesystem_path: RString::from("D:\\fixture"),
                max_entries: 10_000,
                max_depth: 64,
                deadline_millis: 1_000,
            }),
            FolderSizeMeasureResultV1::complete(42)
        );
        let plan = object.render(CellRenderContextV1 {
            value: ROption::RNone,
            exact_bytes: ROption::RNone,
            aggregate: ROption::RNone,
            loading: false,
            error: ROption::RNone,
            selected: false,
            hovered: false,
            dpi_milli: 1_000,
            theme: theme(),
            settings: RString::new(),
        });
        assert_eq!(plan.proportional_bar_millionths, 1_000_000);
    }
}
