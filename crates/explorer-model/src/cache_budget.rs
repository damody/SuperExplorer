//! Shared cache-budget identifiers, bounds, and logarithmic slider math.

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CacheBudgetIdV1 {
    IconMemory,
    BaseIconMemory,
    ThumbnailMemory,
    ExtensionMemory,
    IconGpu,
    ThumbnailGpu,
    IconDisk,
    ThumbnailDisk,
    ExtensionDisk,
    MftPersistedIndex,
    MftVolumeIndex,
    MftFileData,
    MftAggregates,
    MftLru,
    /// Folder-size host cache reuse window in seconds. `0` disables TTL reuse
    /// so a changed directory mtime immediately triggers a rescan. Unlike the
    /// other rows this descriptor's value is seconds, not MiB.
    FolderSizeCacheTtlSeconds,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CacheBudgetDescriptorV1 {
    pub id: CacheBudgetIdV1,
    pub default_mb: u32,
    pub minimum_mb: u32,
    pub maximum_mb: u32,
}

pub const CACHE_BUDGET_DESCRIPTORS_V1: [CacheBudgetDescriptorV1; 15] = [
    descriptor(CacheBudgetIdV1::IconMemory, 24, 8, 1_024),
    descriptor(CacheBudgetIdV1::BaseIconMemory, 8, 4, 256),
    descriptor(CacheBudgetIdV1::ThumbnailMemory, 128, 32, 2_048),
    descriptor(CacheBudgetIdV1::ExtensionMemory, 32, 8, 2_048),
    descriptor(CacheBudgetIdV1::IconGpu, 32, 8, 2_048),
    descriptor(CacheBudgetIdV1::ThumbnailGpu, 128, 32, 4_096),
    descriptor(CacheBudgetIdV1::IconDisk, 512, 64, 8_192),
    descriptor(CacheBudgetIdV1::ThumbnailDisk, 1_024, 128, 16_384),
    descriptor(CacheBudgetIdV1::ExtensionDisk, 256, 32, 8_192),
    descriptor(CacheBudgetIdV1::MftPersistedIndex, 1_024, 256, 16_384),
    descriptor(CacheBudgetIdV1::MftVolumeIndex, 512, 128, 16_384),
    descriptor(CacheBudgetIdV1::MftFileData, 256, 64, 16_384),
    descriptor(CacheBudgetIdV1::MftAggregates, 512, 128, 16_384),
    descriptor(CacheBudgetIdV1::MftLru, 512, 128, 16_384),
    // Unit is seconds, not MiB; the `_mb` field names are shared with the
    // MiB rows for descriptor reuse.
    descriptor(
        CacheBudgetIdV1::FolderSizeCacheTtlSeconds,
        DEFAULT_FOLDER_SIZE_CACHE_TTL_SECONDS as u32,
        0,
        FOLDER_SIZE_CACHE_TTL_MAX_SECONDS as u32,
    ),
];

/// Default folder-size host cache reuse window in seconds.
pub const DEFAULT_FOLDER_SIZE_CACHE_TTL_SECONDS: u64 = 60;
/// Upper bound for the folder-size host cache reuse window in seconds.
pub const FOLDER_SIZE_CACHE_TTL_MAX_SECONDS: u64 = 3_600;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CacheBudgetSettingsV1 {
    pub icon_memory_mb: u32,
    pub base_icon_memory_mb: u32,
    pub thumbnail_memory_mb: u32,
    pub extension_memory_mb: u32,
    pub icon_gpu_mb: u32,
    pub thumbnail_gpu_mb: u32,
    pub icon_disk_mb: u32,
    pub thumbnail_disk_mb: u32,
    pub extension_disk_mb: u32,
    pub mft_persisted_index_mb: u32,
    pub mft_volume_index_mb: u32,
    pub mft_file_data_mb: u32,
    pub mft_aggregates_mb: u32,
    pub mft_lru_mb: u32,
    /// Folder-size host cache reuse window in seconds (0 = disabled).
    pub folder_size_cache_ttl_seconds: u32,
}

impl Default for CacheBudgetSettingsV1 {
    fn default() -> Self {
        Self {
            icon_memory_mb: 24,
            base_icon_memory_mb: 8,
            thumbnail_memory_mb: 128,
            extension_memory_mb: 32,
            icon_gpu_mb: 32,
            thumbnail_gpu_mb: 128,
            icon_disk_mb: 512,
            thumbnail_disk_mb: 1_024,
            extension_disk_mb: 256,
            mft_persisted_index_mb: 1_024,
            mft_volume_index_mb: 1_024,
            mft_file_data_mb: 256,
            mft_aggregates_mb: 512,
            mft_lru_mb: 512,
            folder_size_cache_ttl_seconds: DEFAULT_FOLDER_SIZE_CACHE_TTL_SECONDS as u32,
        }
    }
}

impl CacheBudgetSettingsV1 {
    pub const fn get(self, id: CacheBudgetIdV1) -> u32 {
        match id {
            CacheBudgetIdV1::IconMemory => self.icon_memory_mb,
            CacheBudgetIdV1::BaseIconMemory => self.base_icon_memory_mb,
            CacheBudgetIdV1::ThumbnailMemory => self.thumbnail_memory_mb,
            CacheBudgetIdV1::ExtensionMemory => self.extension_memory_mb,
            CacheBudgetIdV1::IconGpu => self.icon_gpu_mb,
            CacheBudgetIdV1::ThumbnailGpu => self.thumbnail_gpu_mb,
            CacheBudgetIdV1::IconDisk => self.icon_disk_mb,
            CacheBudgetIdV1::ThumbnailDisk => self.thumbnail_disk_mb,
            CacheBudgetIdV1::ExtensionDisk => self.extension_disk_mb,
            CacheBudgetIdV1::MftPersistedIndex => self.mft_persisted_index_mb,
            CacheBudgetIdV1::MftVolumeIndex => self.mft_volume_index_mb,
            CacheBudgetIdV1::MftFileData => self.mft_file_data_mb,
            CacheBudgetIdV1::MftAggregates => self.mft_aggregates_mb,
            CacheBudgetIdV1::MftLru => self.mft_lru_mb,
            CacheBudgetIdV1::FolderSizeCacheTtlSeconds => self.folder_size_cache_ttl_seconds,
        }
    }

    pub fn set(&mut self, id: CacheBudgetIdV1, value_mb: u32) {
        let value_mb = cache_budget_descriptor(id).normalize(value_mb);
        match id {
            CacheBudgetIdV1::IconMemory => self.icon_memory_mb = value_mb,
            CacheBudgetIdV1::BaseIconMemory => self.base_icon_memory_mb = value_mb,
            CacheBudgetIdV1::ThumbnailMemory => self.thumbnail_memory_mb = value_mb,
            CacheBudgetIdV1::ExtensionMemory => self.extension_memory_mb = value_mb,
            CacheBudgetIdV1::IconGpu => self.icon_gpu_mb = value_mb,
            CacheBudgetIdV1::ThumbnailGpu => self.thumbnail_gpu_mb = value_mb,
            CacheBudgetIdV1::IconDisk => self.icon_disk_mb = value_mb,
            CacheBudgetIdV1::ThumbnailDisk => self.thumbnail_disk_mb = value_mb,
            CacheBudgetIdV1::ExtensionDisk => self.extension_disk_mb = value_mb,
            CacheBudgetIdV1::MftPersistedIndex => self.mft_persisted_index_mb = value_mb,
            CacheBudgetIdV1::MftVolumeIndex => self.mft_volume_index_mb = value_mb,
            CacheBudgetIdV1::MftFileData => self.mft_file_data_mb = value_mb,
            CacheBudgetIdV1::MftAggregates => self.mft_aggregates_mb = value_mb,
            CacheBudgetIdV1::MftLru => self.mft_lru_mb = value_mb,
            CacheBudgetIdV1::FolderSizeCacheTtlSeconds => {
                self.folder_size_cache_ttl_seconds = value_mb;
            }
        }
    }

    #[must_use]
    pub fn normalized(mut self) -> Self {
        macro_rules! normalize {
            ($field:ident, $id:ident) => {
                self.$field = cache_budget_descriptor(CacheBudgetIdV1::$id).normalize(self.$field);
            };
        }
        normalize!(icon_memory_mb, IconMemory);
        normalize!(base_icon_memory_mb, BaseIconMemory);
        normalize!(thumbnail_memory_mb, ThumbnailMemory);
        normalize!(extension_memory_mb, ExtensionMemory);
        normalize!(icon_gpu_mb, IconGpu);
        normalize!(thumbnail_gpu_mb, ThumbnailGpu);
        normalize!(icon_disk_mb, IconDisk);
        normalize!(thumbnail_disk_mb, ThumbnailDisk);
        normalize!(extension_disk_mb, ExtensionDisk);
        normalize!(mft_persisted_index_mb, MftPersistedIndex);
        normalize!(mft_volume_index_mb, MftVolumeIndex);
        normalize!(mft_file_data_mb, MftFileData);
        normalize!(mft_aggregates_mb, MftAggregates);
        normalize!(mft_lru_mb, MftLru);
        normalize!(folder_size_cache_ttl_seconds, FolderSizeCacheTtlSeconds);
        self
    }
}

const fn descriptor(
    id: CacheBudgetIdV1,
    default_mb: u32,
    minimum_mb: u32,
    maximum_mb: u32,
) -> CacheBudgetDescriptorV1 {
    CacheBudgetDescriptorV1 {
        id,
        default_mb,
        minimum_mb,
        maximum_mb,
    }
}

pub const CACHE_BUDGET_SLIDER_STOPS_MB_V1: [u32; 30] = [
    8, 16, 24, 32, 48, 64, 72, 84, 96, 128, 192, 256, 320, 384, 512, 640, 768, 1_024, 1_280, 1_536,
    2_048, 2_560, 3_072, 4_096, 5_120, 6_144, 8_192, 10_240, 12_288, 16_384,
];

/// Second-valued slider stops for the folder-size cache TTL row. `0` disables
/// TTL reuse entirely; the rest are practical refresh windows up to one hour.
pub const FOLDER_SIZE_CACHE_TTL_SLIDER_STOPS_SECONDS_V1: [u32; 11] =
    [0, 5, 10, 15, 30, 60, 120, 300, 600, 1_800, 3_600];

impl CacheBudgetDescriptorV1 {
    pub const fn normalize(self, value_mb: u32) -> u32 {
        if value_mb < self.minimum_mb {
            self.minimum_mb
        } else if value_mb > self.maximum_mb {
            self.maximum_mb
        } else {
            value_mb
        }
    }

    pub fn bytes(self, value_mb: u32) -> u64 {
        u64::from(self.normalize(value_mb)) * 1024 * 1024
    }

    pub fn slider_stops(self) -> Vec<u32> {
        if self.id == CacheBudgetIdV1::FolderSizeCacheTtlSeconds {
            return FOLDER_SIZE_CACHE_TTL_SLIDER_STOPS_SECONDS_V1
                .into_iter()
                .filter(|value| *value >= self.minimum_mb && *value <= self.maximum_mb)
                .collect();
        }
        let mut stops = Vec::with_capacity(CACHE_BUDGET_SLIDER_STOPS_MB_V1.len() + 2);
        stops.push(self.minimum_mb);
        stops.extend(
            CACHE_BUDGET_SLIDER_STOPS_MB_V1
                .into_iter()
                .filter(|value| *value > self.minimum_mb && *value < self.maximum_mb),
        );
        if self.maximum_mb != self.minimum_mb {
            stops.push(self.maximum_mb);
        }
        stops
    }

    #[allow(clippy::cast_possible_truncation)]
    pub fn logarithmic_fraction(self, value_mb: u32) -> f32 {
        let value = f64::from(self.normalize(value_mb));
        let minimum = f64::from(self.minimum_mb);
        let maximum = f64::from(self.maximum_mb);
        if maximum <= minimum || minimum == 0.0 {
            return 0.0;
        }
        ((value.ln() - minimum.ln()) / (maximum.ln() - minimum.ln())) as f32
    }
}

pub fn cache_budget_descriptor(id: CacheBudgetIdV1) -> CacheBudgetDescriptorV1 {
    let index = match id {
        CacheBudgetIdV1::IconMemory => 0,
        CacheBudgetIdV1::BaseIconMemory => 1,
        CacheBudgetIdV1::ThumbnailMemory => 2,
        CacheBudgetIdV1::ExtensionMemory => 3,
        CacheBudgetIdV1::IconGpu => 4,
        CacheBudgetIdV1::ThumbnailGpu => 5,
        CacheBudgetIdV1::IconDisk => 6,
        CacheBudgetIdV1::ThumbnailDisk => 7,
        CacheBudgetIdV1::ExtensionDisk => 8,
        CacheBudgetIdV1::MftPersistedIndex => 9,
        CacheBudgetIdV1::MftVolumeIndex => 10,
        CacheBudgetIdV1::MftFileData => 11,
        CacheBudgetIdV1::MftAggregates => 12,
        CacheBudgetIdV1::MftLru => 13,
        CacheBudgetIdV1::FolderSizeCacheTtlSeconds => 14,
    };
    CACHE_BUDGET_DESCRIPTORS_V1[index]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptors_are_unique_bounded_and_include_approved_mft_maxima() {
        let mut ids = std::collections::HashSet::new();
        for descriptor in CACHE_BUDGET_DESCRIPTORS_V1 {
            assert!(ids.insert(descriptor.id));
            assert!(descriptor.minimum_mb <= descriptor.default_mb);
            assert!(descriptor.default_mb <= descriptor.maximum_mb);
            assert_eq!(descriptor.normalize(0), descriptor.minimum_mb);
            assert_eq!(descriptor.normalize(u32::MAX), descriptor.maximum_mb);
        }
        let defaults = CacheBudgetSettingsV1::default().normalized();
        assert_eq!(defaults.icon_memory_mb, 24);
        assert_eq!(defaults.mft_lru_mb, 512);
        for id in [
            CacheBudgetIdV1::MftVolumeIndex,
            CacheBudgetIdV1::MftFileData,
            CacheBudgetIdV1::MftAggregates,
            CacheBudgetIdV1::MftLru,
        ] {
            assert_eq!(cache_budget_descriptor(id).maximum_mb, 16_384);
        }
    }

    #[test]
    fn slider_stops_include_24_and_exact_row_endpoints() {
        let descriptor = cache_budget_descriptor(CacheBudgetIdV1::IconMemory);
        let stops = descriptor.slider_stops();
        assert_eq!(stops.first(), Some(&8));
        assert!(stops.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(stops.contains(&24));
        assert_eq!(stops.last(), Some(&1_024));
        assert_eq!(descriptor.logarithmic_fraction(8), 0.0);
        assert_eq!(descriptor.logarithmic_fraction(1_024), 1.0);
    }

    #[test]
    fn folder_size_cache_ttl_is_second_valued_with_zero_disabling_reuse() {
        let descriptor = cache_budget_descriptor(CacheBudgetIdV1::FolderSizeCacheTtlSeconds);
        assert_eq!(descriptor.default_mb, 60);
        assert_eq!(descriptor.minimum_mb, 0);
        assert_eq!(descriptor.maximum_mb, 3_600);
        assert_eq!(descriptor.normalize(0), 0);
        assert_eq!(descriptor.normalize(30), 30);
        assert_eq!(descriptor.normalize(u32::MAX), 3_600);

        let stops = descriptor.slider_stops();
        assert_eq!(
            stops,
            FOLDER_SIZE_CACHE_TTL_SLIDER_STOPS_SECONDS_V1.to_vec()
        );
        assert_eq!(stops.first(), Some(&0));
        assert_eq!(stops.last(), Some(&3_600));

        let defaults = CacheBudgetSettingsV1::default().normalized();
        assert_eq!(
            defaults.folder_size_cache_ttl_seconds,
            DEFAULT_FOLDER_SIZE_CACHE_TTL_SECONDS as u32
        );
        let mut settings = CacheBudgetSettingsV1::default();
        settings.set(CacheBudgetIdV1::FolderSizeCacheTtlSeconds, u32::MAX);
        assert_eq!(
            settings.get(CacheBudgetIdV1::FolderSizeCacheTtlSeconds),
            3_600
        );
    }
}
