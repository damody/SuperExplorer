//! Bounded, path-free cache telemetry shared by Host-owned reporters and UI consumers.

/// Prevents an untrusted or accidentally duplicated reporter set from growing a snapshot without
/// bound. The current product registers substantially fewer caches than this limit.
pub const MAX_CACHE_TELEMETRY_ENTRIES: usize = 32;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum CacheTelemetryIdV1 {
    VisibleIconsMemory,
    BaseIconsMemory,
    ThumbnailsMemory,
    IconsGpu,
    ThumbnailsGpu,
    ExtensionColumnsMemory,
    IconsDisk,
    ThumbnailsDisk,
    ExtensionColumnsDisk,
    MftPersistedIndex,
    MftVolumeIndexMemory,
    MftFileDataMemory,
    MftAggregateMemory,
    MftServiceLru,
    /// BC7 encoder pipeline. `bytes` is active staging, `entry_count` is active encoders, and
    /// counters are completed encodes/rejections. No path or content identity is retained.
    Bc7Pipeline,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum CacheTelemetryCategoryV1 {
    Memory,
    Disk,
    Gpu,
    MftService,
    Pipeline,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CacheTelemetryCountersV1 {
    pub hits: u64,
    pub misses: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CacheTelemetryValueV1 {
    pub bytes: u64,
    pub limit_bytes: Option<u64>,
    pub entry_count: u64,
    pub counters: Option<CacheTelemetryCountersV1>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CacheTelemetryAvailabilityV1 {
    Available(CacheTelemetryValueV1),
    /// The owner is expected to produce a sample, but one is not ready yet.
    Pending,
    /// The owner has authoritatively reported that telemetry cannot be produced.
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CacheTelemetryEntryV1 {
    pub id: CacheTelemetryIdV1,
    pub category: CacheTelemetryCategoryV1,
    pub availability: CacheTelemetryAvailabilityV1,
}

/// Bounded, identity-free detail for the BC7 conversion and upload path.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Bc7PipelineTelemetryV1 {
    pub queued_jobs: u64,
    pub queue_limit: u64,
    pub peak_queued_jobs: u64,
    pub active_jobs: u64,
    pub concurrency_limit: u64,
    pub reserved_staging_bytes: u64,
    pub staging_limit_bytes: u64,
    pub submitted_jobs: u64,
    pub completed_jobs: u64,
    pub duplicate_jobs: u64,
    pub overload_rejections: u64,
    pub oversized_rejections: u64,
    pub cancelled_jobs: u64,
    pub stale_jobs: u64,
    pub persist_errors: u64,
    pub fallbacks: u64,
    pub icon_gpu_uploads: u64,
    pub icon_gpu_evictions: u64,
    pub thumbnail_gpu_uploads: u64,
    pub thumbnail_gpu_evictions: u64,
    pub gpu_supported: Option<bool>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CacheTelemetrySubtotalV1 {
    pub bytes: u64,
    pub is_partial: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CacheTelemetrySnapshotV1 {
    entries: Vec<CacheTelemetryEntryV1>,
    bc7_pipeline: Option<Bc7PipelineTelemetryV1>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CacheTelemetrySnapshotErrorV1 {
    TooManyEntries,
    DuplicateId(CacheTelemetryIdV1),
}

impl CacheTelemetrySnapshotV1 {
    /// Builds a bounded snapshot with one entry per stable telemetry ID.
    ///
    /// # Errors
    ///
    /// Returns [`CacheTelemetrySnapshotErrorV1`] when the entry bound is
    /// exceeded or the input contains a duplicate ID.
    pub fn new(
        mut entries: Vec<CacheTelemetryEntryV1>,
    ) -> Result<Self, CacheTelemetrySnapshotErrorV1> {
        if entries.len() > MAX_CACHE_TELEMETRY_ENTRIES {
            return Err(CacheTelemetrySnapshotErrorV1::TooManyEntries);
        }
        entries.sort_unstable_by_key(|entry| (entry.category, entry.id));
        if let Some(pair) = entries.windows(2).find(|pair| pair[0].id == pair[1].id) {
            return Err(CacheTelemetrySnapshotErrorV1::DuplicateId(pair[0].id));
        }
        Ok(Self {
            entries,
            bc7_pipeline: None,
        })
    }

    pub fn entries(&self) -> &[CacheTelemetryEntryV1] {
        &self.entries
    }

    pub fn entry(&self, id: CacheTelemetryIdV1) -> Option<&CacheTelemetryEntryV1> {
        self.entries.iter().find(|entry| entry.id == id)
    }

    #[must_use]
    pub fn with_bc7_pipeline(mut self, telemetry: Bc7PipelineTelemetryV1) -> Self {
        self.bc7_pipeline = Some(telemetry);
        self
    }

    pub const fn bc7_pipeline(&self) -> Option<Bc7PipelineTelemetryV1> {
        self.bc7_pipeline
    }

    pub fn subtotal(&self, category: CacheTelemetryCategoryV1) -> CacheTelemetrySubtotalV1 {
        self.entries
            .iter()
            .filter(|entry| entry.category == category)
            .fold(
                CacheTelemetrySubtotalV1::default(),
                |mut subtotal, entry| {
                    match entry.availability {
                        CacheTelemetryAvailabilityV1::Available(value) => {
                            subtotal.bytes = subtotal.bytes.saturating_add(value.bytes);
                        }
                        CacheTelemetryAvailabilityV1::Pending
                        | CacheTelemetryAvailabilityV1::Unavailable => subtotal.is_partial = true,
                    }
                    subtotal
                },
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn available(
        id: CacheTelemetryIdV1,
        category: CacheTelemetryCategoryV1,
        bytes: u64,
    ) -> CacheTelemetryEntryV1 {
        CacheTelemetryEntryV1 {
            id,
            category,
            availability: CacheTelemetryAvailabilityV1::Available(CacheTelemetryValueV1 {
                bytes,
                limit_bytes: None,
                entry_count: 1,
                counters: None,
            }),
        }
    }

    #[test]
    fn construction_is_bounded_deterministic_and_rejects_duplicate_ids() {
        let snapshot = CacheTelemetrySnapshotV1::new(vec![
            available(
                CacheTelemetryIdV1::ThumbnailsDisk,
                CacheTelemetryCategoryV1::Disk,
                2,
            ),
            available(
                CacheTelemetryIdV1::VisibleIconsMemory,
                CacheTelemetryCategoryV1::Memory,
                1,
            ),
        ])
        .unwrap();
        assert_eq!(
            snapshot.entries()[0].id,
            CacheTelemetryIdV1::VisibleIconsMemory
        );

        let duplicate = CacheTelemetrySnapshotV1::new(vec![
            available(
                CacheTelemetryIdV1::IconsDisk,
                CacheTelemetryCategoryV1::Disk,
                1,
            ),
            available(
                CacheTelemetryIdV1::IconsDisk,
                CacheTelemetryCategoryV1::Disk,
                2,
            ),
        ]);
        assert_eq!(
            duplicate,
            Err(CacheTelemetrySnapshotErrorV1::DuplicateId(
                CacheTelemetryIdV1::IconsDisk
            ))
        );

        let oversized = vec![
            available(
                CacheTelemetryIdV1::IconsDisk,
                CacheTelemetryCategoryV1::Disk,
                0,
            );
            MAX_CACHE_TELEMETRY_ENTRIES + 1
        ];
        assert_eq!(
            CacheTelemetrySnapshotV1::new(oversized),
            Err(CacheTelemetrySnapshotErrorV1::TooManyEntries)
        );
    }

    #[test]
    fn subtotal_saturates_and_marks_unavailable_members_partial() {
        let snapshot = CacheTelemetrySnapshotV1::new(vec![
            available(
                CacheTelemetryIdV1::IconsDisk,
                CacheTelemetryCategoryV1::Disk,
                u64::MAX,
            ),
            available(
                CacheTelemetryIdV1::ThumbnailsDisk,
                CacheTelemetryCategoryV1::Disk,
                7,
            ),
            CacheTelemetryEntryV1 {
                id: CacheTelemetryIdV1::ExtensionColumnsDisk,
                category: CacheTelemetryCategoryV1::Disk,
                availability: CacheTelemetryAvailabilityV1::Unavailable,
            },
        ])
        .unwrap();
        assert_eq!(
            snapshot.subtotal(CacheTelemetryCategoryV1::Disk),
            CacheTelemetrySubtotalV1 {
                bytes: u64::MAX,
                is_partial: true,
            }
        );
    }

    #[test]
    fn contract_has_no_path_or_free_form_identity_field() {
        let snapshot = CacheTelemetrySnapshotV1::new(vec![available(
            CacheTelemetryIdV1::ExtensionColumnsMemory,
            CacheTelemetryCategoryV1::Memory,
            4,
        )])
        .unwrap()
        .with_bc7_pipeline(Bc7PipelineTelemetryV1 {
            queue_limit: 32,
            staging_limit_bytes: 64 * 1024 * 1024,
            gpu_supported: Some(true),
            ..Default::default()
        });
        let debug = format!("{snapshot:?}");
        assert!(!debug.to_ascii_lowercase().contains("path"));
        assert!(!debug.contains(r"C:\"));
        assert_eq!(snapshot.bc7_pipeline().unwrap().queue_limit, 32);
    }
}
