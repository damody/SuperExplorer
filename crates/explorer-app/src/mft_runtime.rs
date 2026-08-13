//! Memory-first per-volume state used by the MFT service.

use std::{collections::HashMap, sync::Arc};

use crate::mft_journal::{MftChangeKindV2, MftChangeV2, PENDING_BYTE_LIMIT, PENDING_CHANGE_LIMIT};
use crate::mft_persistence::{JournalCursorV1, PendingBatchV1, capture_coalesced_batch};
use crate::mft_size_map::MftIndexV1;

#[derive(Debug)]
pub(crate) struct VolumeMemoryRuntimeV1 {
    pub(crate) index: Arc<MftIndexV1>,
    pub(crate) durable: JournalCursorV1,
    pub(crate) observed: JournalCursorV1,
    pending: HashMap<u64, MftChangeV2>,
    pending_bytes: usize,
    exact: bool,
}

impl VolumeMemoryRuntimeV1 {
    pub(crate) fn new(index: MftIndexV1, durable: JournalCursorV1) -> Self {
        Self {
            index: Arc::new(index),
            durable,
            observed: durable,
            pending: HashMap::new(),
            pending_bytes: 0,
            exact: true,
        }
    }

    pub(crate) fn rebuild_required(index: MftIndexV1, cursor: JournalCursorV1) -> Self {
        let mut runtime = Self::new(index, cursor);
        runtime.mark_inexact();
        runtime
    }

    pub(crate) fn replace_with_exact(&mut self, index: MftIndexV1, cursor: JournalCursorV1) {
        self.index = Arc::new(index);
        self.durable = cursor;
        self.observed = cursor;
        self.pending.clear();
        self.pending_bytes = 0;
        self.exact = true;
    }

    pub(crate) fn replace_with_partial(&mut self, index: MftIndexV1, cursor: JournalCursorV1) {
        self.index = Arc::new(index);
        self.durable = cursor;
        self.observed = cursor;
        self.pending.clear();
        self.pending_bytes = 0;
        self.exact = false;
    }

    pub(crate) fn replace_with_caught_up(
        &mut self,
        index: MftIndexV1,
        durable: JournalCursorV1,
        observed: JournalCursorV1,
        changes: impl IntoIterator<Item = MftChangeV2>,
    ) -> Result<(), String> {
        self.index = Arc::new(index);
        self.durable = durable;
        self.observed = observed;
        self.pending = changes
            .into_iter()
            .map(|change| (change.reference, change))
            .collect();
        self.recount_pending_bytes();
        if self.pending.len() > PENDING_CHANGE_LIMIT || self.pending_bytes > PENDING_BYTE_LIMIT {
            self.mark_inexact();
            return Err("MFT startup catch-up exceeds pending memory bounds".to_owned());
        }
        self.exact = true;
        Ok(())
    }

    pub(crate) fn observe(&mut self, change: MftChangeV2, next_usn: i64) -> Result<(), String> {
        if !self.exact {
            return Err("MFT memory state requires rebuild".to_owned());
        }
        if next_usn < self.observed.next_usn || change.kind == MftChangeKindV2::Invalidate {
            self.mark_inexact();
            return Err("MFT observation is non-contiguous or ambiguous".to_owned());
        }
        Arc::make_mut(&mut self.index).apply_change(&change)?;
        self.pending.insert(change.reference, change);
        self.observed.next_usn = next_usn;
        self.observed.generation = self.observed.generation.saturating_add(1);
        self.recount_pending_bytes();
        Ok(())
    }

    pub(crate) const fn is_exact(&self) -> bool {
        self.exact
    }

    pub(crate) fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    pub(crate) const fn pending_bytes(&self) -> usize {
        self.pending_bytes
    }

    pub(crate) fn pending_count(&self) -> usize {
        self.pending.len()
    }

    pub(crate) fn mark_inexact(&mut self) {
        self.exact = false;
        self.pending.clear();
        self.pending_bytes = 0;
    }

    /// Advances diagnostic/query freshness while a memory-budget-limited
    /// topology remains intentionally partial. No entry mutation is retained,
    /// so the state stays inexact and can never be persisted from this cursor.
    pub(crate) fn advance_inexact_observed(&mut self, next_usn: i64) -> Result<(), String> {
        if self.exact {
            return Err("exact MFT state must observe entry mutations".to_owned());
        }
        if next_usn < self.observed.next_usn {
            return Err("MFT observed cursor moved backwards".to_owned());
        }
        if next_usn > self.observed.next_usn {
            self.observed.next_usn = next_usn;
            self.observed.generation = self.observed.generation.saturating_add(1);
        }
        Ok(())
    }

    pub(crate) fn capture(&mut self) -> Result<PendingBatchV1<MftChangeV2>, String> {
        let batch = capture_coalesced_batch(
            &mut self.pending,
            self.durable,
            self.observed,
            self.pending_bytes,
        )
        .map_err(str::to_owned)?;
        self.pending_bytes = 0;
        Ok(batch)
    }

    pub(crate) fn commit_succeeded(&mut self, batch: &PendingBatchV1<MftChangeV2>) {
        self.durable = batch.observed;
    }

    pub(crate) fn commit_failed(&mut self, batch: PendingBatchV1<MftChangeV2>) {
        // Later observations win when they target the same file reference.
        for change in batch.changes {
            self.pending.entry(change.reference).or_insert(change);
        }
        self.recount_pending_bytes();
    }

    fn recount_pending_bytes(&mut self) {
        self.pending_bytes = self
            .pending
            .values()
            .map(|change| 49_usize.saturating_add(change.name.len()))
            .sum();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mft_size_map::{MftEntryV1, MftIndexV1};
    use std::collections::BTreeMap;

    fn cursor() -> JournalCursorV1 {
        JournalCursorV1 {
            journal_id: 9,
            next_usn: 10,
            generation: 0,
        }
    }

    fn base() -> MftIndexV1 {
        MftIndexV1::from_entries(BTreeMap::from([(
            5,
            MftEntryV1 {
                reference: 5,
                parent_reference: 5,
                name: "root".into(),
                logical_bytes: 0,
                allocated_bytes: 0,
                is_directory: true,
            },
        )]))
    }

    fn change(reference: u64, name: &str) -> MftChangeV2 {
        MftChangeV2 {
            kind: MftChangeKindV2::Upsert,
            reference,
            parent_reference: 5,
            name: name.into(),
            logical_bytes: 1,
            allocated_bytes: 4096,
            is_directory: false,
            reason: 1,
        }
    }

    #[test]
    fn live_index_advances_while_durable_cursor_waits() {
        let mut runtime = VolumeMemoryRuntimeV1::new(base(), cursor());
        runtime.observe(change(20, "live"), 11).unwrap();
        assert!(runtime.index.entries.contains_key(&20));
        assert_eq!(runtime.durable.next_usn, 10);
        assert_eq!(runtime.observed.next_usn, 11);
        assert!(runtime.has_pending());
    }

    #[test]
    fn failed_snapshot_merges_back_without_overwriting_later_event() {
        let mut runtime = VolumeMemoryRuntimeV1::new(base(), cursor());
        runtime.observe(change(20, "old"), 11).unwrap();
        let batch = runtime.capture().unwrap();
        runtime.observe(change(20, "new"), 12).unwrap();
        runtime.commit_failed(batch);
        let retry = runtime.capture().unwrap();
        assert_eq!(retry.changes.len(), 1);
        assert_eq!(retry.changes[0].name, "new");
        assert_eq!(retry.observed.next_usn, 12);
    }

    #[test]
    fn startup_catch_up_is_exact_but_retains_replayed_changes_for_persistence() {
        let durable = cursor();
        let mut caught_up = base();
        caught_up.apply_change(&change(20, "replayed")).unwrap();
        let observed = JournalCursorV1 {
            next_usn: 12,
            generation: 1,
            ..durable
        };
        let mut runtime = VolumeMemoryRuntimeV1::rebuild_required(base(), durable);

        runtime
            .replace_with_caught_up(caught_up, durable, observed, vec![change(20, "replayed")])
            .unwrap();

        assert!(runtime.is_exact());
        assert_eq!(runtime.durable, durable);
        assert_eq!(runtime.observed, observed);
        let batch = runtime.capture().expect("replay remains pending");
        assert_eq!(batch.start, durable);
        assert_eq!(batch.observed, observed);
        assert_eq!(batch.changes, vec![change(20, "replayed")]);
    }

    #[test]
    fn ambiguity_marks_state_inexact_and_discards_pending_detail() {
        let mut runtime = VolumeMemoryRuntimeV1::new(base(), cursor());
        runtime.observe(change(20, "valid"), 11).unwrap();
        let mut invalid = change(21, "invalid");
        invalid.kind = MftChangeKindV2::Invalidate;
        assert!(runtime.observe(invalid, 12).is_err());
        assert!(!runtime.is_exact());
        assert!(!runtime.has_pending());
    }

    #[test]
    fn budget_limited_state_advances_freshness_without_claiming_exactness() {
        let mut runtime = VolumeMemoryRuntimeV1::rebuild_required(base(), cursor());

        runtime.advance_inexact_observed(12).unwrap();

        assert!(!runtime.is_exact());
        assert_eq!(runtime.durable.next_usn, 10);
        assert_eq!(runtime.observed.next_usn, 12);
        assert_eq!(runtime.observed.generation, 1);
        assert!(!runtime.has_pending());
    }
}
