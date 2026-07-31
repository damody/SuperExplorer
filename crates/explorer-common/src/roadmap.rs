//! Shared bounded-work primitives for the post-parity roadmap.

use std::{
    fmt,
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
    time::{Duration, Instant},
};

use thiserror::Error;

/// Current schema for centralized roadmap limits.
pub const ROADMAP_LIMITS_SCHEMA_VERSION: u16 = 2;

/// A monotonic request deadline that never crosses a serialization boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RequestDeadline(Option<Instant>);

impl RequestDeadline {
    /// Creates a request without a deadline.
    #[must_use]
    pub const fn none() -> Self {
        Self(None)
    }

    /// Creates a deadline relative to `now`, returning `None` on instant overflow.
    #[must_use]
    pub fn after(now: Instant, timeout: Duration) -> Option<Self> {
        now.checked_add(timeout)
            .map(|deadline| Self(Some(deadline)))
    }

    /// Returns whether this deadline has elapsed at the supplied monotonic instant.
    #[must_use]
    pub fn is_elapsed_at(self, now: Instant) -> bool {
        self.0.is_some_and(|deadline| now >= deadline)
    }

    /// Returns remaining time, or `None` when the request is unbounded.
    #[must_use]
    pub fn remaining_at(self, now: Instant) -> Option<Duration> {
        self.0
            .map(|deadline| deadline.saturating_duration_since(now))
    }
}

impl Default for RequestDeadline {
    fn default() -> Self {
        Self::none()
    }
}

/// Terminal result class claimed by one asynchronous request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum TerminalDisposition {
    Success = 1,
    Error = 2,
    Cancelled = 3,
    Timeout = 4,
    Disconnected = 5,
}

impl TerminalDisposition {
    fn from_atomic(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Success),
            2 => Some(Self::Error),
            3 => Some(Self::Cancelled),
            4 => Some(Self::Timeout),
            5 => Some(Self::Disconnected),
            _ => None,
        }
    }
}

/// Outcome of attempting the single terminal transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalClaim {
    Accepted(TerminalDisposition),
    AlreadyClaimed(TerminalDisposition),
}

/// Cloneable exactly-once terminal gate shared by competing completion paths.
#[derive(Clone, Default)]
pub struct TerminalGate(Arc<AtomicU8>);

impl TerminalGate {
    /// Creates an open terminal gate.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Claims terminal ownership or reports the disposition that already won.
    pub fn claim(&self, disposition: TerminalDisposition) -> TerminalClaim {
        match self
            .0
            .compare_exchange(0, disposition as u8, Ordering::AcqRel, Ordering::Acquire)
        {
            Ok(_) => TerminalClaim::Accepted(disposition),
            Err(value) => TerminalClaim::AlreadyClaimed(
                TerminalDisposition::from_atomic(value).unwrap_or(TerminalDisposition::Error),
            ),
        }
    }

    /// Returns the accepted terminal disposition, if any.
    #[must_use]
    pub fn disposition(&self) -> Option<TerminalDisposition> {
        TerminalDisposition::from_atomic(self.0.load(Ordering::Acquire))
    }
}

impl fmt::Debug for TerminalGate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TerminalGate")
            .field("disposition", &self.disposition())
            .finish()
    }
}

/// Centralized, versioned bounds shared by roadmap services.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RoadmapLimits {
    pub schema_version: u16,
    pub max_tabs: usize,
    pub max_history_entries_per_tab: usize,
    pub max_location_descriptor_bytes: usize,
    pub max_columns_per_tab: usize,
    pub max_column_width: u16,
    pub max_state_payload_bytes: usize,
    pub thumbnail_memory_bytes: usize,
    pub thumbnail_disk_bytes: u64,
    pub thumbnail_queue_items: usize,
    pub thumbnail_prefetch_items: usize,
    pub thumbnail_concurrency: usize,
    pub ipc_frame_bytes: usize,
    pub ipc_queue_frames: usize,
    pub broker_workers: usize,
    pub broker_worker_memory_bytes: usize,
    pub request_timeout_ms: u64,
    pub broker_cancel_grace_ms: u64,
    pub quarantine_failure_threshold: usize,
    pub quarantine_max_entries: usize,
    pub quarantine_duration_ms: u64,
    pub preview_debounce_ms: u64,
    pub preview_lookup_timeout_ms: u64,
    pub preview_initialize_timeout_ms: u64,
    pub preview_render_timeout_ms: u64,
    pub preview_resize_timeout_ms: u64,
    pub preview_input_timeout_ms: u64,
    pub preview_unload_timeout_ms: u64,
    pub lock_recovery_max_resources: usize,
    pub lock_recovery_max_owners: usize,
    pub lock_recovery_max_name_bytes: usize,
    pub lock_recovery_max_retries: usize,
    pub lock_discovery_timeout_ms: u64,
    pub lock_shutdown_timeout_ms: u64,
}

impl RoadmapLimits {
    /// Validates individual and cross-field bounds.
    ///
    /// # Errors
    ///
    /// Returns the first unsupported, zero, excessive, or inconsistent value.
    pub fn validate(self) -> Result<Self, RoadmapLimitsError> {
        if self.schema_version != ROADMAP_LIMITS_SCHEMA_VERSION {
            return Err(RoadmapLimitsError::UnsupportedSchema(self.schema_version));
        }

        let non_zero = [
            ("max_tabs", self.max_tabs),
            (
                "max_history_entries_per_tab",
                self.max_history_entries_per_tab,
            ),
            (
                "max_location_descriptor_bytes",
                self.max_location_descriptor_bytes,
            ),
            ("max_columns_per_tab", self.max_columns_per_tab),
            ("max_state_payload_bytes", self.max_state_payload_bytes),
            ("thumbnail_memory_bytes", self.thumbnail_memory_bytes),
            ("thumbnail_queue_items", self.thumbnail_queue_items),
            ("thumbnail_prefetch_items", self.thumbnail_prefetch_items),
            ("thumbnail_concurrency", self.thumbnail_concurrency),
            ("ipc_frame_bytes", self.ipc_frame_bytes),
            ("ipc_queue_frames", self.ipc_queue_frames),
            ("broker_workers", self.broker_workers),
            (
                "broker_worker_memory_bytes",
                self.broker_worker_memory_bytes,
            ),
            (
                "quarantine_failure_threshold",
                self.quarantine_failure_threshold,
            ),
            ("quarantine_max_entries", self.quarantine_max_entries),
            (
                "lock_recovery_max_resources",
                self.lock_recovery_max_resources,
            ),
            ("lock_recovery_max_owners", self.lock_recovery_max_owners),
            (
                "lock_recovery_max_name_bytes",
                self.lock_recovery_max_name_bytes,
            ),
            ("lock_recovery_max_retries", self.lock_recovery_max_retries),
        ];
        if let Some((field, _)) = non_zero.into_iter().find(|(_, value)| *value == 0) {
            return Err(RoadmapLimitsError::Zero(field));
        }
        let non_zero_u64 = [
            ("thumbnail_disk_bytes", self.thumbnail_disk_bytes),
            ("request_timeout_ms", self.request_timeout_ms),
            ("broker_cancel_grace_ms", self.broker_cancel_grace_ms),
            ("quarantine_duration_ms", self.quarantine_duration_ms),
            ("preview_debounce_ms", self.preview_debounce_ms),
            ("preview_lookup_timeout_ms", self.preview_lookup_timeout_ms),
            (
                "preview_initialize_timeout_ms",
                self.preview_initialize_timeout_ms,
            ),
            ("preview_render_timeout_ms", self.preview_render_timeout_ms),
            ("preview_resize_timeout_ms", self.preview_resize_timeout_ms),
            ("preview_input_timeout_ms", self.preview_input_timeout_ms),
            ("preview_unload_timeout_ms", self.preview_unload_timeout_ms),
            ("lock_discovery_timeout_ms", self.lock_discovery_timeout_ms),
            ("lock_shutdown_timeout_ms", self.lock_shutdown_timeout_ms),
        ];
        if let Some((field, _)) = non_zero_u64.into_iter().find(|(_, value)| *value == 0) {
            return Err(RoadmapLimitsError::Zero(field));
        }
        if self.max_column_width == 0 {
            return Err(RoadmapLimitsError::Zero("max_column_width"));
        }

        let maximums = [
            ("max_tabs", self.max_tabs, 1_024),
            (
                "max_history_entries_per_tab",
                self.max_history_entries_per_tab,
                16_384,
            ),
            (
                "max_location_descriptor_bytes",
                self.max_location_descriptor_bytes,
                1024 * 1024,
            ),
            ("max_columns_per_tab", self.max_columns_per_tab, 1_024),
            (
                "max_state_payload_bytes",
                self.max_state_payload_bytes,
                256 * 1024 * 1024,
            ),
            (
                "thumbnail_queue_items",
                self.thumbnail_queue_items,
                1_000_000,
            ),
            ("thumbnail_concurrency", self.thumbnail_concurrency, 256),
            ("ipc_frame_bytes", self.ipc_frame_bytes, 64 * 1024 * 1024),
            ("ipc_queue_frames", self.ipc_queue_frames, 65_536),
            ("broker_workers", self.broker_workers, 64),
            (
                "quarantine_max_entries",
                self.quarantine_max_entries,
                65_536,
            ),
            (
                "lock_recovery_max_resources",
                self.lock_recovery_max_resources,
                1_024,
            ),
            (
                "lock_recovery_max_owners",
                self.lock_recovery_max_owners,
                1_024,
            ),
            (
                "lock_recovery_max_name_bytes",
                self.lock_recovery_max_name_bytes,
                16_384,
            ),
            (
                "lock_recovery_max_retries",
                self.lock_recovery_max_retries,
                16,
            ),
        ];
        if let Some((field, value, maximum)) = maximums
            .into_iter()
            .find(|(_, value, maximum)| value > maximum)
        {
            return Err(RoadmapLimitsError::ExceedsMaximum {
                field,
                value,
                maximum,
            });
        }
        if self.thumbnail_prefetch_items > self.thumbnail_queue_items {
            return Err(RoadmapLimitsError::Inconsistent(
                "thumbnail_prefetch_items exceeds thumbnail_queue_items",
            ));
        }
        if self.thumbnail_concurrency > self.thumbnail_queue_items {
            return Err(RoadmapLimitsError::Inconsistent(
                "thumbnail_concurrency exceeds thumbnail_queue_items",
            ));
        }
        if self.broker_cancel_grace_ms >= self.request_timeout_ms {
            return Err(RoadmapLimitsError::Inconsistent(
                "broker_cancel_grace_ms must be shorter than request_timeout_ms",
            ));
        }
        Ok(self)
    }
}

impl Default for RoadmapLimits {
    fn default() -> Self {
        Self {
            schema_version: ROADMAP_LIMITS_SCHEMA_VERSION,
            max_tabs: 64,
            max_history_entries_per_tab: 256,
            max_location_descriptor_bytes: 64 * 1024,
            max_columns_per_tab: 64,
            max_column_width: 16_384,
            max_state_payload_bytes: 8 * 1024 * 1024,
            thumbnail_memory_bytes: 256 * 1024 * 1024,
            thumbnail_disk_bytes: 512 * 1024 * 1024,
            thumbnail_queue_items: 2_048,
            thumbnail_prefetch_items: 128,
            thumbnail_concurrency: 8,
            ipc_frame_bytes: 16 * 1024 * 1024,
            ipc_queue_frames: 256,
            broker_workers: 4,
            broker_worker_memory_bytes: 512 * 1024 * 1024,
            request_timeout_ms: 15_000,
            broker_cancel_grace_ms: 2_000,
            quarantine_failure_threshold: 3,
            quarantine_max_entries: 256,
            quarantine_duration_ms: 5 * 60 * 1_000,
            preview_debounce_ms: 150,
            preview_lookup_timeout_ms: 2_000,
            preview_initialize_timeout_ms: 5_000,
            preview_render_timeout_ms: 10_000,
            preview_resize_timeout_ms: 1_000,
            preview_input_timeout_ms: 500,
            preview_unload_timeout_ms: 2_000,
            lock_recovery_max_resources: 64,
            lock_recovery_max_owners: 64,
            lock_recovery_max_name_bytes: 1_024,
            lock_recovery_max_retries: 2,
            lock_discovery_timeout_ms: 5_000,
            lock_shutdown_timeout_ms: 10_000,
        }
    }
}

/// Failure to validate centralized roadmap bounds.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum RoadmapLimitsError {
    #[error("unsupported roadmap limits schema {0}")]
    UnsupportedSchema(u16),
    #[error("roadmap limit {0} must be non-zero")]
    Zero(&'static str),
    #[error("roadmap limit {field} value {value} exceeds maximum {maximum}")]
    ExceedsMaximum {
        field: &'static str,
        value: usize,
        maximum: usize,
    },
    #[error("inconsistent roadmap limits: {0}")]
    Inconsistent(&'static str),
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{Arc, Barrier},
        thread,
        time::{Duration, Instant},
    };

    use super::*;

    #[test]
    fn deadline_supports_unbounded_elapsed_and_remaining_contracts() {
        let now = Instant::now();
        assert!(!RequestDeadline::none().is_elapsed_at(now));
        assert_eq!(RequestDeadline::none().remaining_at(now), None);
        let deadline = RequestDeadline::after(now, Duration::from_millis(25))
            .expect("small duration must fit Instant");
        assert_eq!(deadline.remaining_at(now), Some(Duration::from_millis(25)));
        assert!(deadline.is_elapsed_at(now + Duration::from_millis(25)));
    }

    #[test]
    fn exactly_one_competing_terminal_path_wins() {
        let gate = TerminalGate::new();
        let barrier = Arc::new(Barrier::new(5));
        let mut workers = Vec::new();
        for disposition in [
            TerminalDisposition::Success,
            TerminalDisposition::Error,
            TerminalDisposition::Cancelled,
            TerminalDisposition::Timeout,
        ] {
            let gate = gate.clone();
            let barrier = Arc::clone(&barrier);
            workers.push(thread::spawn(move || {
                barrier.wait();
                gate.claim(disposition)
            }));
        }
        barrier.wait();
        let claims: Vec<_> = workers
            .into_iter()
            .map(|worker| worker.join().expect("terminal worker"))
            .collect();
        assert_eq!(
            claims
                .iter()
                .filter(|claim| matches!(claim, TerminalClaim::Accepted(_)))
                .count(),
            1
        );
        assert!(gate.disposition().is_some());
        assert!(matches!(
            gate.claim(TerminalDisposition::Disconnected),
            TerminalClaim::AlreadyClaimed(_)
        ));
    }

    #[test]
    fn default_limits_are_valid_and_bounded() {
        let limits = RoadmapLimits::default();
        assert_eq!(limits.validate(), Ok(limits));
    }

    #[test]
    fn limits_reject_zero_excessive_and_cross_field_values() {
        let zero = RoadmapLimits {
            max_tabs: 0,
            ..RoadmapLimits::default()
        };
        assert_eq!(zero.validate(), Err(RoadmapLimitsError::Zero("max_tabs")));
        let excessive = RoadmapLimits {
            broker_workers: 65,
            ..RoadmapLimits::default()
        };
        assert!(matches!(
            excessive.validate(),
            Err(RoadmapLimitsError::ExceedsMaximum {
                field: "broker_workers",
                ..
            })
        ));
        let inconsistent = RoadmapLimits {
            thumbnail_prefetch_items: 2_049,
            ..RoadmapLimits::default()
        };
        assert!(matches!(
            inconsistent.validate(),
            Err(RoadmapLimitsError::Inconsistent(_))
        ));
    }
}
