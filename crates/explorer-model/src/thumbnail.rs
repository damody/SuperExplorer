//! Generation-safe asynchronous thumbnail contracts with bounded owned pixels.

use explorer_common::{TerminalClaim, TerminalDisposition, TerminalGate};

use crate::{Generation, RequestContext, ShellIconTheme, ShellItemId, TabId, ViewMode};

/// Retrieval intent distinguishes content thumbnails from authentic Shell icons.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ThumbnailMode {
    Thumbnail,
    IconOnly,
}

/// Complete cache and invalidation identity for one rendered asset.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ThumbnailRequestKey {
    pub item_id: ShellItemId,
    pub physical_size: u16,
    pub dpi: u16,
    pub mode: ThumbnailMode,
    pub source_generation: u64,
    pub theme: ShellIconTheme,
    pub association_generation: u64,
    pub overlay_generation: u64,
}

/// Where the final image originated.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThumbnailSource {
    MemoryCache,
    DiskCache,
    WindowsCache,
    Provider,
    ShellIcon,
}

/// Non-terminal progress visible to diagnostics and fallback UI.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThumbnailStatus {
    Queued,
    CacheLookup,
    Extracting,
    Decoding,
}

/// Typed reason content extraction did not produce a thumbnail.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThumbnailFallbackReason {
    Offline,
    Unsupported,
    Timeout,
    Cancelled,
    Corrupt,
    ProviderFailure,
    ResourceLimit,
}

/// Scheduler ordering from active visible rows through background prefetch.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ThumbnailPriority {
    ActiveVisible,
    ActivePrefetch,
    BackgroundVisible,
    BackgroundPrefetch,
}

/// Owned premultiplied BGRA8 pixels safe to cross worker and render boundaries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThumbnailPixels {
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub bytes: Vec<u8>,
}

impl ThumbnailPixels {
    /// Validates dimensions, row stride, overflow, byte length, and decoded-byte budget.
    ///
    /// # Errors
    ///
    /// Returns the precise malformed or excessive pixel invariant.
    pub fn validate(&self, maximum_decoded_bytes: usize) -> Result<(), ThumbnailPixelError> {
        if self.width == 0 || self.height == 0 {
            return Err(ThumbnailPixelError::EmptyDimensions);
        }
        let minimum_stride = self
            .width
            .checked_mul(4)
            .ok_or(ThumbnailPixelError::Overflow)?;
        if self.stride < minimum_stride || !self.stride.is_multiple_of(4) {
            return Err(ThumbnailPixelError::InvalidStride);
        }
        let expected = usize::try_from(self.stride)
            .ok()
            .and_then(|stride| {
                usize::try_from(self.height)
                    .ok()
                    .and_then(|height| stride.checked_mul(height))
            })
            .ok_or(ThumbnailPixelError::Overflow)?;
        if expected > maximum_decoded_bytes {
            return Err(ThumbnailPixelError::TooLarge {
                actual: expected,
                maximum: maximum_decoded_bytes,
            });
        }
        if self.bytes.len() != expected {
            return Err(ThumbnailPixelError::LengthMismatch {
                expected,
                actual: self.bytes.len(),
            });
        }
        Ok(())
    }

    /// Returns decoded byte cost after validation.
    pub fn byte_cost(&self) -> usize {
        self.bytes.len()
    }
}

/// Invalid owned pixel payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ThumbnailPixelError {
    EmptyDimensions,
    InvalidStride,
    Overflow,
    TooLarge { actual: usize, maximum: usize },
    LengthMismatch { expected: usize, actual: usize },
}

impl std::fmt::Display for ThumbnailPixelError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid thumbnail pixels: {self:?}")
    }
}

impl std::error::Error for ThumbnailPixelError {}

/// One consumer-correlated request with a shared exactly-one-terminal gate.
#[derive(Clone, Debug)]
pub struct ThumbnailRequest {
    pub context: RequestContext,
    pub key: ThumbnailRequestKey,
    pub priority: ThumbnailPriority,
    terminal: TerminalGate,
}

impl ThumbnailRequest {
    pub fn new(
        context: RequestContext,
        key: ThumbnailRequestKey,
        priority: ThumbnailPriority,
    ) -> Self {
        Self {
            context,
            key,
            priority,
            terminal: TerminalGate::new(),
        }
    }

    /// Claims the only terminal outcome across success/error/cancel/timeout/disconnect races.
    pub fn claim_terminal(&self, terminal: &ThumbnailTerminal) -> TerminalClaim {
        self.terminal.claim(terminal.disposition())
    }
}

/// Owned terminal result fanned out to every still-current consumer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ThumbnailTerminal {
    Ready {
        source: ThumbnailSource,
        pixels: ThumbnailPixels,
    },
    Compressed {
        source: ThumbnailSource,
        raster: crate::Bc7RasterPayload,
    },
    Fallback(ThumbnailFallbackReason),
    Failed(String),
}

/// Provider-independent extraction outcome used by fake, Shell, and broker workers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ThumbnailProviderOutcome {
    Pixels(ThumbnailPixels),
    CacheMiss,
    Unsupported,
    Offline,
    Failure(String),
}

/// Normalizes provider output at the trust boundary, giving cancellation and
/// deadline precedence and rejecting malformed/oversized pixel buffers.
pub fn normalize_thumbnail_provider_outcome(
    request: &ThumbnailRequest,
    outcome: ThumbnailProviderOutcome,
    maximum_decoded_bytes: usize,
    now: std::time::Instant,
) -> ThumbnailTerminal {
    if request.context.cancellation.is_cancelled() {
        return ThumbnailTerminal::Fallback(ThumbnailFallbackReason::Cancelled);
    }
    if request.context.deadline.is_elapsed_at(now) {
        return ThumbnailTerminal::Fallback(ThumbnailFallbackReason::Timeout);
    }
    match outcome {
        ThumbnailProviderOutcome::Pixels(pixels) => {
            if pixels.validate(maximum_decoded_bytes).is_ok() {
                ThumbnailTerminal::Ready {
                    source: ThumbnailSource::Provider,
                    pixels,
                }
            } else {
                ThumbnailTerminal::Fallback(ThumbnailFallbackReason::Corrupt)
            }
        }
        ThumbnailProviderOutcome::CacheMiss | ThumbnailProviderOutcome::Unsupported => {
            ThumbnailTerminal::Fallback(ThumbnailFallbackReason::Unsupported)
        }
        ThumbnailProviderOutcome::Offline => {
            ThumbnailTerminal::Fallback(ThumbnailFallbackReason::Offline)
        }
        ThumbnailProviderOutcome::Failure(detail) => ThumbnailTerminal::Failed(detail),
    }
}

impl ThumbnailTerminal {
    fn disposition(&self) -> TerminalDisposition {
        match self {
            Self::Ready { .. } | Self::Compressed { .. } => TerminalDisposition::Success,
            Self::Fallback(ThumbnailFallbackReason::Cancelled) => TerminalDisposition::Cancelled,
            Self::Fallback(ThumbnailFallbackReason::Timeout) => TerminalDisposition::Timeout,
            Self::Fallback(_) | Self::Failed(_) => TerminalDisposition::Error,
        }
    }
}

/// Visible and bounded prefetch ranges derived without retaining UI entities.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ThumbnailViewport {
    pub visible_start: usize,
    pub visible_end: usize,
    pub prefetch_start: usize,
    pub prefetch_end: usize,
}

impl ThumbnailViewport {
    pub fn bounded(
        visible_start: usize,
        visible_end: usize,
        item_count: usize,
        prefetch_items: usize,
    ) -> Self {
        let start = visible_start.min(item_count);
        let end = visible_end.max(start).min(item_count);
        Self {
            visible_start: start,
            visible_end: end,
            prefetch_start: start.saturating_sub(prefetch_items),
            prefetch_end: end.saturating_add(prefetch_items).min(item_count),
        }
    }
}

/// Returns retrieval mode and logical edge length for every Explorer view mode.
pub const fn view_mode_thumbnail_policy(mode: ViewMode) -> (ThumbnailMode, u16) {
    match mode {
        ViewMode::ExtraLargeIcons => (ThumbnailMode::Thumbnail, 256),
        ViewMode::LargeIcons => (ThumbnailMode::Thumbnail, 96),
        ViewMode::MediumIcons => (ThumbnailMode::Thumbnail, 64),
        ViewMode::Content => (ThumbnailMode::Thumbnail, 48),
        ViewMode::SmallIcons | ViewMode::List | ViewMode::Details | ViewMode::Tiles => {
            (ThumbnailMode::IconOnly, 16)
        }
    }
}

/// Consumer identity prevents cross-tab fan-out after navigation or size changes.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ThumbnailConsumer {
    pub tab_id: TabId,
    pub generation: Generation,
    pub size_generation: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LocationDescriptor, RequestContext};

    fn key() -> ThumbnailRequestKey {
        ThumbnailRequestKey {
            item_id: ShellItemId::from_provider_bytes([1, 2, 3]).expect("identity"),
            physical_size: 96,
            dpi: 144,
            mode: ThumbnailMode::Thumbnail,
            source_generation: 7,
            theme: ShellIconTheme::Dark,
            association_generation: 2,
            overlay_generation: 3,
        }
    }

    #[test]
    fn key_changes_for_every_render_or_source_invalidation_dimension() {
        let baseline = key();
        let mut variants = Vec::new();
        let mut value = baseline.clone();
        value.physical_size += 1;
        variants.push(value);
        let mut value = baseline.clone();
        value.dpi += 1;
        variants.push(value);
        let mut value = baseline.clone();
        value.mode = ThumbnailMode::IconOnly;
        variants.push(value);
        let mut value = baseline.clone();
        value.source_generation += 1;
        variants.push(value);
        let mut value = baseline.clone();
        value.theme = ShellIconTheme::Light;
        variants.push(value);
        let mut value = baseline.clone();
        value.association_generation += 1;
        variants.push(value);
        let mut value = baseline.clone();
        value.overlay_generation += 1;
        variants.push(value);
        assert!(variants.into_iter().all(|candidate| candidate != baseline));
    }

    #[test]
    fn pixels_reject_empty_stride_overflow_length_and_budget() {
        let valid = ThumbnailPixels {
            width: 2,
            height: 2,
            stride: 8,
            bytes: vec![0; 16],
        };
        assert_eq!(valid.validate(16), Ok(()));
        let mut value = valid.clone();
        value.width = 0;
        assert!(value.validate(16).is_err());
        let mut value = valid.clone();
        value.stride = 7;
        assert!(value.validate(16).is_err());
        let mut value = valid.clone();
        value.bytes.pop();
        assert!(value.validate(16).is_err());
        assert!(matches!(
            valid.validate(15),
            Err(ThumbnailPixelError::TooLarge { .. })
        ));
        let overflow = ThumbnailPixels {
            width: u32::MAX,
            height: u32::MAX,
            stride: u32::MAX,
            bytes: vec![],
        };
        assert!(overflow.validate(usize::MAX).is_err());
    }

    #[test]
    fn terminal_race_accepts_exactly_one_and_viewport_is_bounded() {
        let context = RequestContext::new(TabId::new(), Generation::default());
        let request = ThumbnailRequest::new(context, key(), ThumbnailPriority::ActiveVisible);
        let success = ThumbnailTerminal::Ready {
            source: ThumbnailSource::Provider,
            pixels: ThumbnailPixels {
                width: 1,
                height: 1,
                stride: 4,
                bytes: vec![0; 4],
            },
        };
        assert!(matches!(
            request.claim_terminal(&success),
            TerminalClaim::Accepted(_)
        ));
        assert!(matches!(
            request.claim_terminal(&ThumbnailTerminal::Fallback(
                ThumbnailFallbackReason::Timeout
            )),
            TerminalClaim::AlreadyClaimed(TerminalDisposition::Success)
        ));
        assert_eq!(
            ThumbnailViewport::bounded(10, 20, 25, 8),
            ThumbnailViewport {
                visible_start: 10,
                visible_end: 20,
                prefetch_start: 2,
                prefetch_end: 25,
            }
        );
        let _ = LocationDescriptor::file_system(r"C:\fixture");
    }

    #[test]
    fn every_view_mode_has_an_explicit_policy() {
        for mode in ViewMode::ALL {
            let (_, logical) = view_mode_thumbnail_policy(mode);
            assert!(logical > 0);
        }
    }

    #[test]
    fn explorer_icon_views_use_the_expected_thumbnail_targets() {
        assert_eq!(
            view_mode_thumbnail_policy(ViewMode::ExtraLargeIcons),
            (ThumbnailMode::Thumbnail, 256)
        );
        assert_eq!(
            view_mode_thumbnail_policy(ViewMode::LargeIcons),
            (ThumbnailMode::Thumbnail, 96)
        );
        assert_eq!(
            view_mode_thumbnail_policy(ViewMode::MediumIcons),
            (ThumbnailMode::Thumbnail, 64)
        );
    }

    #[test]
    fn small_icon_and_tile_views_are_shell_icon_only() {
        assert_eq!(
            view_mode_thumbnail_policy(ViewMode::SmallIcons).0,
            ThumbnailMode::IconOnly
        );
        assert_eq!(
            view_mode_thumbnail_policy(ViewMode::Tiles).0,
            ThumbnailMode::IconOnly
        );
    }

    #[test]
    fn fake_provider_matrix_is_bounded_and_deterministic() {
        use explorer_common::RequestDeadline;
        use std::time::{Duration, Instant};

        let now = Instant::now();
        let context = RequestContext::new(TabId::new(), Generation::default())
            .with_deadline(RequestDeadline::after(now, Duration::from_secs(1)).expect("deadline"));
        let request = ThumbnailRequest::new(context, key(), ThumbnailPriority::ActiveVisible);
        let pixels = ThumbnailPixels {
            width: 2,
            height: 2,
            stride: 8,
            bytes: vec![0; 16],
        };
        assert!(matches!(
            normalize_thumbnail_provider_outcome(
                &request,
                ThumbnailProviderOutcome::Pixels(pixels.clone()),
                16,
                now
            ),
            ThumbnailTerminal::Ready { .. }
        ));
        for outcome in [
            ThumbnailProviderOutcome::CacheMiss,
            ThumbnailProviderOutcome::Unsupported,
            ThumbnailProviderOutcome::Offline,
        ] {
            assert!(matches!(
                normalize_thumbnail_provider_outcome(&request, outcome, 16, now),
                ThumbnailTerminal::Fallback(_)
            ));
        }
        let mut malformed = pixels.clone();
        malformed.bytes.pop();
        assert_eq!(
            normalize_thumbnail_provider_outcome(
                &request,
                ThumbnailProviderOutcome::Pixels(malformed),
                16,
                now
            ),
            ThumbnailTerminal::Fallback(ThumbnailFallbackReason::Corrupt)
        );
        assert_eq!(
            normalize_thumbnail_provider_outcome(
                &request,
                ThumbnailProviderOutcome::Pixels(pixels.clone()),
                15,
                now
            ),
            ThumbnailTerminal::Fallback(ThumbnailFallbackReason::Corrupt)
        );
        assert_eq!(
            normalize_thumbnail_provider_outcome(
                &request,
                ThumbnailProviderOutcome::Failure("HRESULT".to_owned()),
                16,
                now
            ),
            ThumbnailTerminal::Failed("HRESULT".to_owned())
        );
        assert_eq!(
            normalize_thumbnail_provider_outcome(
                &request,
                ThumbnailProviderOutcome::Pixels(pixels.clone()),
                16,
                now + Duration::from_secs(2)
            ),
            ThumbnailTerminal::Fallback(ThumbnailFallbackReason::Timeout)
        );
        request.context.cancellation.cancel();
        assert_eq!(
            normalize_thumbnail_provider_outcome(
                &request,
                ThumbnailProviderOutcome::Pixels(pixels),
                16,
                now
            ),
            ThumbnailTerminal::Fallback(ThumbnailFallbackReason::Cancelled)
        );
    }
}
