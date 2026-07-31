//! Bounded deduplicating thumbnail scheduler and decoded-byte-cost memory LRU.

use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::Arc,
};

use explorer_model::{ThumbnailConsumer, ThumbnailPixels, ThumbnailPriority, ThumbnailRequestKey};

const PRIORITY_COUNT: usize = 4;

const fn priority_index(priority: ThumbnailPriority) -> usize {
    priority as usize
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThumbnailScheduleOutcome {
    Queued,
    Deduplicated,
    Promoted,
    Overloaded,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ThumbnailSchedulerStats {
    pub queue_capacity: usize,
    pub concurrency_limit: usize,
    pub decoded_byte_limit: usize,
    pub queued_unique: usize,
    pub active_unique: usize,
    pub consumers: usize,
    pub deduplicated_submissions: u64,
    pub promotions: u64,
    pub overloads: u64,
    pub cancellations: u64,
    pub decoded_in_flight_bytes: usize,
}

struct ScheduledEntry {
    priority: ThumbnailPriority,
    consumers: HashSet<ThumbnailConsumer>,
    state: ScheduledState,
    reserved_bytes: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ScheduledState {
    Queued,
    Active,
}

/// Deduplicates cross-tab requests while bounding queue, concurrency, and decoded bytes.
pub struct ThumbnailScheduler {
    lanes: [VecDeque<ThumbnailRequestKey>; PRIORITY_COUNT],
    entries: HashMap<ThumbnailRequestKey, ScheduledEntry>,
    queue_capacity: usize,
    concurrency: usize,
    decoded_byte_limit: usize,
    stats: ThumbnailSchedulerStats,
}

impl ThumbnailScheduler {
    pub fn new(queue_capacity: usize, concurrency: usize, decoded_byte_limit: usize) -> Self {
        let queue_capacity = queue_capacity.max(1);
        let concurrency = concurrency.max(1);
        let decoded_byte_limit = decoded_byte_limit.max(1);
        Self {
            lanes: std::array::from_fn(|_| VecDeque::new()),
            entries: HashMap::new(),
            queue_capacity,
            concurrency,
            decoded_byte_limit,
            stats: ThumbnailSchedulerStats {
                queue_capacity,
                concurrency_limit: concurrency,
                decoded_byte_limit,
                ..ThumbnailSchedulerStats::default()
            },
        }
    }

    pub fn submit(
        &mut self,
        key: ThumbnailRequestKey,
        consumer: ThumbnailConsumer,
        priority: ThumbnailPriority,
    ) -> ThumbnailScheduleOutcome {
        if let Some(entry) = self.entries.get_mut(&key) {
            let inserted = entry.consumers.insert(consumer);
            if inserted {
                self.stats.consumers = self.stats.consumers.saturating_add(1);
            }
            self.stats.deduplicated_submissions =
                self.stats.deduplicated_submissions.saturating_add(1);
            if entry.state == ScheduledState::Queued && priority < entry.priority {
                entry.priority = priority;
                self.lanes[priority_index(priority)].push_back(key);
                self.stats.promotions = self.stats.promotions.saturating_add(1);
                return ThumbnailScheduleOutcome::Promoted;
            }
            return ThumbnailScheduleOutcome::Deduplicated;
        }
        if self.entries.len() >= self.queue_capacity.saturating_add(self.stats.active_unique) {
            self.stats.overloads = self.stats.overloads.saturating_add(1);
            return ThumbnailScheduleOutcome::Overloaded;
        }
        self.lanes[priority_index(priority)].push_back(key.clone());
        self.entries.insert(
            key,
            ScheduledEntry {
                priority,
                consumers: HashSet::from([consumer]),
                state: ScheduledState::Queued,
                reserved_bytes: 0,
            },
        );
        self.stats.queued_unique = self.stats.queued_unique.saturating_add(1);
        self.stats.consumers = self.stats.consumers.saturating_add(1);
        ThumbnailScheduleOutcome::Queued
    }

    /// Starts the highest-priority request if both worker and byte reservations allow it.
    pub fn try_start(&mut self, reserved_bytes: usize) -> Option<ThumbnailRequestKey> {
        if self.stats.active_unique >= self.concurrency
            || self
                .stats
                .decoded_in_flight_bytes
                .saturating_add(reserved_bytes)
                > self.decoded_byte_limit
        {
            return None;
        }
        for (lane_index, lane) in self.lanes.iter_mut().enumerate() {
            while let Some(key) = lane.pop_front() {
                let Some(entry) = self.entries.get_mut(&key) else {
                    continue;
                };
                if entry.state != ScheduledState::Queued
                    || priority_index(entry.priority) != lane_index
                {
                    continue;
                }
                entry.state = ScheduledState::Active;
                entry.reserved_bytes = reserved_bytes;
                self.stats.queued_unique = self.stats.queued_unique.saturating_sub(1);
                self.stats.active_unique = self.stats.active_unique.saturating_add(1);
                self.stats.decoded_in_flight_bytes = self
                    .stats
                    .decoded_in_flight_bytes
                    .saturating_add(reserved_bytes);
                return Some(key);
            }
        }
        None
    }

    /// Completes shared work and returns all still-current consumers for terminal fan-out.
    pub fn complete(&mut self, key: &ThumbnailRequestKey) -> Vec<ThumbnailConsumer> {
        let Some(entry) = self.entries.remove(key) else {
            return Vec::new();
        };
        match entry.state {
            ScheduledState::Queued => {
                self.stats.queued_unique = self.stats.queued_unique.saturating_sub(1);
            }
            ScheduledState::Active => {
                self.stats.active_unique = self.stats.active_unique.saturating_sub(1);
                self.stats.decoded_in_flight_bytes = self
                    .stats
                    .decoded_in_flight_bytes
                    .saturating_sub(entry.reserved_bytes);
            }
        }
        self.stats.consumers = self.stats.consumers.saturating_sub(entry.consumers.len());
        entry.consumers.into_iter().collect()
    }

    /// Cancels one consumer; shared work remains until its final consumer leaves.
    pub fn cancel_consumer(
        &mut self,
        key: &ThumbnailRequestKey,
        consumer: ThumbnailConsumer,
    ) -> bool {
        let Some(entry) = self.entries.get_mut(key) else {
            return false;
        };
        if !entry.consumers.remove(&consumer) {
            return false;
        }
        self.stats.cancellations = self.stats.cancellations.saturating_add(1);
        self.stats.consumers = self.stats.consumers.saturating_sub(1);
        if entry.consumers.is_empty() {
            let _ = self.complete(key);
        }
        true
    }

    pub const fn stats(&self) -> ThumbnailSchedulerStats {
        self.stats
    }

    /// Drops all queued/active bookkeeping; late service results remain generation-suppressed.
    pub fn clear(&mut self) {
        self.lanes.iter_mut().for_each(VecDeque::clear);
        self.entries.clear();
        self.stats.queued_unique = 0;
        self.stats.active_unique = 0;
        self.stats.consumers = 0;
        self.stats.decoded_in_flight_bytes = 0;
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CacheInsertOutcome {
    Inserted,
    Replaced,
    RejectedOversized,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ThumbnailCacheStats {
    pub byte_budget: usize,
    pub entry_budget: usize,
    pub entries: usize,
    pub current_bytes: usize,
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub oversized_rejections: u64,
}

/// Shared decoded thumbnail LRU bounded by both bytes and entry count.
pub struct ThumbnailMemoryCache {
    entries: HashMap<ThumbnailRequestKey, Arc<ThumbnailPixels>>,
    order: VecDeque<ThumbnailRequestKey>,
    byte_budget: usize,
    entry_budget: usize,
    stats: ThumbnailCacheStats,
}

impl ThumbnailMemoryCache {
    pub fn new(byte_budget: usize, entry_budget: usize) -> Self {
        let byte_budget = byte_budget.max(1);
        let entry_budget = entry_budget.max(1);
        Self {
            entries: HashMap::new(),
            order: VecDeque::new(),
            byte_budget,
            entry_budget,
            stats: ThumbnailCacheStats {
                byte_budget,
                entry_budget,
                ..ThumbnailCacheStats::default()
            },
        }
    }

    pub fn get(&mut self, key: &ThumbnailRequestKey) -> Option<Arc<ThumbnailPixels>> {
        let value = self.entries.get(key).cloned();
        if value.is_some() {
            self.stats.hits = self.stats.hits.saturating_add(1);
            self.touch(key);
        } else {
            self.stats.misses = self.stats.misses.saturating_add(1);
        }
        value
    }

    pub fn insert(
        &mut self,
        key: ThumbnailRequestKey,
        pixels: Arc<ThumbnailPixels>,
    ) -> CacheInsertOutcome {
        let cost = pixels.byte_cost();
        if cost > self.byte_budget {
            self.stats.oversized_rejections = self.stats.oversized_rejections.saturating_add(1);
            return CacheInsertOutcome::RejectedOversized;
        }
        self.order.retain(|candidate| candidate != &key);
        self.order.push_back(key.clone());
        let replaced = self.entries.insert(key, pixels);
        if let Some(previous) = &replaced {
            self.stats.current_bytes = self
                .stats
                .current_bytes
                .saturating_sub(previous.byte_cost());
        }
        self.stats.current_bytes = self.stats.current_bytes.saturating_add(cost);
        while self.entries.len() > self.entry_budget || self.stats.current_bytes > self.byte_budget
        {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            if let Some(evicted) = self.entries.remove(&oldest) {
                self.stats.current_bytes =
                    self.stats.current_bytes.saturating_sub(evicted.byte_cost());
                self.stats.evictions = self.stats.evictions.saturating_add(1);
            }
        }
        self.stats.entries = self.entries.len();
        if replaced.is_some() {
            CacheInsertOutcome::Replaced
        } else {
            CacheInsertOutcome::Inserted
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
        self.stats.entries = 0;
        self.stats.current_bytes = 0;
    }

    pub const fn stats(&self) -> ThumbnailCacheStats {
        self.stats
    }

    fn touch(&mut self, key: &ThumbnailRequestKey) {
        self.order.retain(|candidate| candidate != key);
        self.order.push_back(key.clone());
    }
}

#[cfg(test)]
mod tests {
    use explorer_model::{Generation, ShellIconTheme, ShellItemId, TabId, ThumbnailMode};

    use super::*;

    fn key(id: u8) -> ThumbnailRequestKey {
        ThumbnailRequestKey {
            item_id: ShellItemId::from_provider_bytes([id]).expect("id"),
            physical_size: 64,
            dpi: 96,
            mode: ThumbnailMode::Thumbnail,
            source_generation: 1,
            theme: ShellIconTheme::Light,
            association_generation: 1,
            overlay_generation: 1,
        }
    }

    fn consumer() -> ThumbnailConsumer {
        ThumbnailConsumer {
            tab_id: TabId::new(),
            generation: Generation::default(),
            size_generation: 1,
        }
    }

    fn numbered_key(id: u32, size_generation: u64) -> ThumbnailRequestKey {
        ThumbnailRequestKey {
            item_id: ShellItemId::from_provider_bytes(id.to_le_bytes()).expect("numbered id"),
            physical_size: match id % 5 {
                0 => 64,
                1 => 80,
                2 => 96,
                3 => 112,
                _ => 128,
            },
            dpi: match id % 5 {
                0 => 96,
                1 => 120,
                2 => 144,
                3 => 168,
                _ => 192,
            },
            mode: ThumbnailMode::Thumbnail,
            source_generation: size_generation,
            theme: if id % 2 == 0 {
                ShellIconTheme::Light
            } else {
                ShellIconTheme::Dark
            },
            association_generation: size_generation,
            overlay_generation: size_generation,
        }
    }

    #[test]
    fn thousand_scroll_zoom_resize_navigation_replacements_stay_bounded() {
        const OPERATIONS: u32 = 1_000;
        const QUEUE_CAPACITY: usize = 128;
        const CONCURRENCY: usize = 4;
        const DECODE_LIMIT: usize = 4 * 1024 * 1024;
        const CACHE_LIMIT: usize = 2 * 1024 * 1024;
        let mut scheduler = ThumbnailScheduler::new(QUEUE_CAPACITY, CONCURRENCY, DECODE_LIMIT);
        let mut cache = ThumbnailMemoryCache::new(CACHE_LIMIT, 96);
        let pixels = Arc::new(ThumbnailPixels {
            width: 32,
            height: 32,
            stride: 128,
            bytes: vec![0x80; 32 * 32 * 4],
        });
        pixels.validate(DECODE_LIMIT).expect("bounded pixels");
        let consumer = consumer();

        for operation in 0..OPERATIONS {
            // A 512-item ring models fast scroll replacement while the generation changes
            // model zoom/resize/navigation invalidation. The queue is continuously drained
            // like the production pump, so overload is controlled rather than accumulated.
            let key = numbered_key(operation % 512, u64::from(operation / 1_000 + 1));
            let _ = scheduler.submit(
                key.clone(),
                consumer,
                match operation % 4 {
                    0 => ThumbnailPriority::ActiveVisible,
                    1 => ThumbnailPriority::ActivePrefetch,
                    2 => ThumbnailPriority::BackgroundVisible,
                    _ => ThumbnailPriority::BackgroundPrefetch,
                },
            );
            while let Some(started) = scheduler.try_start(pixels.byte_cost()) {
                let _ = cache.insert(started.clone(), Arc::clone(&pixels));
                let delivered = scheduler.complete(&started);
                assert!(delivered.len() <= 1);
            }
            if operation % 7_919 == 0 {
                // Corrupt disk-cache input never enters this owned decoded cache; clearing is
                // the production recovery contract and must release every byte immediately.
                cache.clear();
            }
            let schedule = scheduler.stats();
            let cached = cache.stats();
            assert!(schedule.queued_unique <= QUEUE_CAPACITY);
            assert!(schedule.active_unique <= CONCURRENCY);
            assert!(schedule.decoded_in_flight_bytes <= DECODE_LIMIT);
            assert!(cached.entries <= 96);
            assert!(cached.current_bytes <= CACHE_LIMIT);
        }
        assert_eq!(scheduler.stats().queued_unique, 0);
        assert_eq!(scheduler.stats().active_unique, 0);
        assert_eq!(scheduler.stats().decoded_in_flight_bytes, 0);
    }

    #[test]
    fn dedupe_promotion_cancellation_and_limits_are_deterministic() {
        let mut scheduler = ThumbnailScheduler::new(2, 1, 100);
        let first = consumer();
        let second = consumer();
        assert_eq!(
            scheduler.submit(key(1), first, ThumbnailPriority::BackgroundVisible),
            ThumbnailScheduleOutcome::Queued
        );
        assert_eq!(
            scheduler.submit(key(1), second, ThumbnailPriority::ActiveVisible),
            ThumbnailScheduleOutcome::Promoted
        );
        assert_eq!(
            scheduler.submit(key(2), first, ThumbnailPriority::ActivePrefetch),
            ThumbnailScheduleOutcome::Queued
        );
        assert_eq!(
            scheduler.submit(key(3), first, ThumbnailPriority::ActiveVisible),
            ThumbnailScheduleOutcome::Overloaded
        );
        assert_eq!(scheduler.try_start(80), Some(key(1)));
        assert_eq!(scheduler.try_start(30), None);
        assert!(scheduler.cancel_consumer(&key(1), first));
        assert_eq!(scheduler.complete(&key(1)), vec![second]);
        assert_eq!(scheduler.try_start(30), Some(key(2)));
    }

    #[test]
    fn byte_cost_lru_promotes_replaces_evicts_rejects_and_clears() {
        let mut cache = ThumbnailMemoryCache::new(16, 2);
        let pixels = || {
            Arc::new(ThumbnailPixels {
                width: 1,
                height: 1,
                stride: 4,
                bytes: vec![0; 4],
            })
        };
        assert_eq!(cache.insert(key(1), pixels()), CacheInsertOutcome::Inserted);
        assert_eq!(cache.insert(key(2), pixels()), CacheInsertOutcome::Inserted);
        assert!(cache.get(&key(1)).is_some());
        assert_eq!(cache.insert(key(3), pixels()), CacheInsertOutcome::Inserted);
        assert!(cache.get(&key(2)).is_none());
        assert_eq!(cache.insert(key(1), pixels()), CacheInsertOutcome::Replaced);
        let huge = Arc::new(ThumbnailPixels {
            width: 1,
            height: 1,
            stride: 20,
            bytes: vec![0; 20],
        });
        assert_eq!(
            cache.insert(key(9), huge),
            CacheInsertOutcome::RejectedOversized
        );
        assert_eq!(cache.stats().entries, 2);
        cache.clear();
        assert_eq!(cache.stats().entries, 0);
        assert_eq!(cache.stats().current_bytes, 0);
    }
}
