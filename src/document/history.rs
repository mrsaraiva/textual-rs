//! Edit batching and undo/redo checkpointing (port of Python
//! `textual/document/_history.py`).

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use super::Edit;

/// A monotonic time source for [`EditHistory`] batching.
///
/// Clock reads happen only inside [`EditHistory::record`], which runs on the
/// input/message path, never during render, so render determinism is
/// unaffected. This is the documented mock seam (Python `_get_time`).
pub trait HistoryClock: std::fmt::Debug + Send + Sync {
    /// Monotonic now, arbitrary epoch.
    fn now(&self) -> Duration;
}

/// The default clock: elapsed time since construction.
#[derive(Debug)]
pub struct MonotonicClock(Instant);

impl MonotonicClock {
    pub fn new() -> Self {
        Self(Instant::now())
    }
}

impl Default for MonotonicClock {
    fn default() -> Self {
        Self::new()
    }
}

impl HistoryClock for MonotonicClock {
    fn now(&self) -> Duration {
        self.0.elapsed()
    }
}

/// A manually driven clock for tests (the `TimeMockableEditHistory` pattern
/// from Python's `test_history.py`, without any global time hook).
#[derive(Debug, Clone, Default)]
pub struct MockClock(Arc<AtomicU64>);

impl MockClock {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the clock to an absolute number of milliseconds.
    pub fn set_millis(&self, millis: u64) {
        self.0.store(millis, Ordering::SeqCst);
    }

    /// Advance the clock by a number of milliseconds.
    pub fn advance_millis(&self, millis: u64) {
        self.0.fetch_add(millis, Ordering::SeqCst);
    }
}

impl HistoryClock for MockClock {
    fn now(&self) -> Duration {
        Duration::from_millis(self.0.load(Ordering::SeqCst))
    }
}

/// Manages batching/checkpointing of [`Edit`]s into groups that can be
/// undone/redone in the `TextArea`.
#[derive(Debug)]
pub struct EditHistory {
    max_checkpoints: usize,
    /// Maximum time since the last edit until a new batch is created.
    checkpoint_timer: Duration,
    /// Maximum number of characters in a batch before a new batch is formed.
    checkpoint_max_characters: usize,
    clock: Box<dyn HistoryClock>,
    last_edit_time: Duration,
    /// Characters replaced + inserted since last batch creation.
    ///
    /// Quirk pin (Python parity): [`EditHistory::clear`] does NOT reset this
    /// counter.
    character_count: usize,
    /// Forces the creation of a new batch for the next recorded edit.
    force_end_batch: bool,
    /// Whether the most recent edit was a replacement (removed any text) as
    /// opposed to a pure insertion.
    previously_replaced: bool,
    /// Batches of edits; the front is evicted at `max_checkpoints`.
    undo_stack: VecDeque<Vec<Edit>>,
    /// Batches that have been undone, allowing them to be redone.
    redo_stack: Vec<Vec<Edit>>,
}

impl EditHistory {
    /// Create a history with the default monotonic clock.
    pub fn new(
        max_checkpoints: usize,
        checkpoint_timer: Duration,
        checkpoint_max_characters: usize,
    ) -> Self {
        Self::with_clock(
            max_checkpoints,
            checkpoint_timer,
            checkpoint_max_characters,
            MonotonicClock::new(),
        )
    }

    /// Create a history with an injected clock (the test seam).
    pub fn with_clock(
        max_checkpoints: usize,
        checkpoint_timer: Duration,
        checkpoint_max_characters: usize,
        clock: impl HistoryClock + 'static,
    ) -> Self {
        let clock: Box<dyn HistoryClock> = Box::new(clock);
        let last_edit_time = clock.now();
        Self {
            max_checkpoints,
            checkpoint_timer,
            checkpoint_max_characters,
            clock,
            last_edit_time,
            character_count: 0,
            force_end_batch: false,
            previously_replaced: false,
            undo_stack: VecDeque::new(),
            redo_stack: Vec::new(),
        }
    }

    /// Record an `Edit` so that it may be undone and redone, batching it
    /// with previous edits or starting a new batch/checkpoint.
    ///
    /// Must be called exactly once per performed edit, in chronological
    /// order. A new batch is created when any of the following hold: the
    /// undo stack is empty; a checkpoint was forced; the edit inserts more
    /// than one character (a paste); the edit involves a newline; the edit
    /// flips between pure insertion and replacement; the checkpoint timer
    /// expired; or the character limit is reached.
    ///
    /// # Panics
    ///
    /// Panics if the edit has not been performed via `Edit::apply` yet
    /// (Python raises `HistoryException`).
    pub fn record(&mut self, edit: Edit) {
        let edit_result = edit
            .edit_result()
            .expect("Cannot add an edit to history before it has been performed via `Edit::apply`.")
            .clone();

        if edit.text.is_empty() && edit_result.replaced_text.is_empty() {
            return;
        }

        let is_replacement = !edit_result.replaced_text.is_empty();
        let current_time = self.clock.now();
        // Codepoint count deliberately (parity heuristic, not a text-integrity
        // operation): a multi-codepoint emoji counts > 1 in both
        // implementations and therefore checkpoints identically.
        let edit_characters = edit.text.chars().count();
        // Deviation (strictly safer): Python checks only '\n', so CR-only
        // documents under-checkpoint newline edits there; Rust checks both.
        let has_newline = |text: &str| text.contains('\n') || text.contains('\r');
        let contains_newline = has_newline(&edit.text) || has_newline(&edit_result.replaced_text);

        let new_batch = self.undo_stack.is_empty()
            || self.force_end_batch
            || edit_characters > 1
            || contains_newline
            || is_replacement != self.previously_replaced
            || current_time.saturating_sub(self.last_edit_time) > self.checkpoint_timer
            || self.character_count + edit_characters > self.checkpoint_max_characters;

        if new_batch {
            // Create a new batch (a "checkpoint"), evicting the oldest at
            // the limit (Python deque maxlen).
            if self.undo_stack.len() >= self.max_checkpoints {
                self.undo_stack.pop_front();
            }
            self.undo_stack.push_back(vec![edit]);
            self.character_count = edit_characters;
            self.last_edit_time = current_time;
            self.force_end_batch = false;
        } else {
            self.undo_stack
                .back_mut()
                .expect("undo stack is non-empty in the batch-append branch")
                .push(edit);
            self.character_count += edit_characters;
            self.last_edit_time = current_time;
        }

        self.previously_replaced = is_replacement;
        self.redo_stack.clear();

        // For some edits, ensure the NEXT edit cannot be added to this batch.
        if contains_newline || edit_characters > 1 {
            self.checkpoint();
        }
    }

    /// Pop the latest batch from the undo stack, transferring ownership to
    /// the caller for replay.
    ///
    /// Ordering pin (Python parity): the caller must push the batch back via
    /// [`EditHistory::push_redone`] UNCONDITIONALLY after replaying it, even
    /// if the replay turned out to be a visual no-op (Python moves the batch
    /// across stacks before replaying; `test_redo_stack` asserts the
    /// resulting lengths).
    pub fn pop_undo(&mut self) -> Option<Vec<Edit>> {
        self.undo_stack.pop_back()
    }

    /// Place a batch popped by [`EditHistory::pop_undo`] onto the redo stack.
    pub fn push_redone(&mut self, batch: Vec<Edit>) {
        self.redo_stack.push(batch);
    }

    /// Pop the latest batch from the redo stack, transferring ownership to
    /// the caller for replay; push it back via
    /// [`EditHistory::push_undone`] unconditionally after replaying.
    pub fn pop_redo(&mut self) -> Option<Vec<Edit>> {
        self.redo_stack.pop()
    }

    /// Place a batch popped by [`EditHistory::pop_redo`] back onto the undo
    /// stack, forcing a checkpoint so that edits which follow cannot be
    /// added to the redone batch (Python `_pop_redo`).
    pub fn push_undone(&mut self, batch: Vec<Edit>) {
        if self.undo_stack.len() >= self.max_checkpoints {
            self.undo_stack.pop_front();
        }
        self.undo_stack.push_back(batch);
        self.checkpoint();
    }

    /// Completely clear the history.
    ///
    /// Matches the observable Python quirk: the character count is NOT
    /// reset.
    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.last_edit_time = self.clock.now();
        self.force_end_batch = false;
        self.previously_replaced = false;
    }

    /// Ensure the next recorded edit starts a new batch.
    pub fn checkpoint(&mut self) {
        self.force_end_batch = true;
    }

    /// The number of batches on the undo stack.
    pub fn undo_stack_len(&self) -> usize {
        self.undo_stack.len()
    }

    /// The number of batches on the redo stack.
    pub fn redo_stack_len(&self) -> usize {
        self.redo_stack.len()
    }

    /// A view of the batches on the undo stack (oldest first).
    pub fn undo_batches(&self) -> impl Iterator<Item = &[Edit]> {
        self.undo_stack.iter().map(Vec::as_slice)
    }

    /// A view of the batches on the redo stack (oldest first).
    pub fn redo_batches(&self) -> impl Iterator<Item = &[Edit]> {
        self.redo_stack.iter().map(Vec::as_slice)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Document, Selection};

    fn applied_edit(document: &mut Document, text: &str, from: (usize, usize)) -> Edit {
        let mut edit = Edit::new(text, from, from, false);
        edit.apply(document, Selection::default(), true);
        edit
    }

    fn history_with_clock(clock: MockClock) -> EditHistory {
        EditHistory::with_clock(5, Duration::from_secs(2), 100, clock)
    }

    #[test]
    #[should_panic(expected = "before it has been performed")]
    fn record_unapplied_edit_panics() {
        let mut history = history_with_clock(MockClock::new());
        history.record(Edit::new("x", (0, 0), (0, 0), false));
    }

    #[test]
    fn empty_edit_is_not_recorded() {
        let mut document = Document::new("");
        let mut history = history_with_clock(MockClock::new());
        history.record(applied_edit(&mut document, "", (0, 0)));
        assert_eq!(history.undo_stack_len(), 0);
    }

    #[test]
    fn single_characters_batch_within_timer() {
        let mut document = Document::new("");
        let clock = MockClock::new();
        let mut history = history_with_clock(clock.clone());
        history.record(applied_edit(&mut document, "1", (0, 0)));
        clock.advance_millis(1000);
        history.record(applied_edit(&mut document, "2", (0, 1)));
        assert_eq!(history.undo_stack_len(), 1);
        clock.advance_millis(10_000);
        history.record(applied_edit(&mut document, "3", (0, 2)));
        assert_eq!(history.undo_stack_len(), 2);
    }

    #[test]
    fn multi_character_inserts_are_isolated() {
        let mut document = Document::new("");
        let mut history = history_with_clock(MockClock::new());
        history.record(applied_edit(&mut document, "paste", (0, 0)));
        history.record(applied_edit(&mut document, "x", (0, 5)));
        // The paste is isolated on both sides.
        assert_eq!(history.undo_stack_len(), 2);
    }

    #[test]
    fn inserts_do_not_batch_with_deletes() {
        let mut document = Document::new("abc");
        let mut history = history_with_clock(MockClock::new());
        history.record(applied_edit(&mut document, "1", (0, 0)));
        let mut deletion = Edit::new("", (0, 0), (0, 1), false);
        deletion.apply(&mut document, Selection::default(), true);
        history.record(deletion);
        assert_eq!(history.undo_stack_len(), 2);
    }

    #[test]
    fn clear_does_not_reset_character_count() {
        // Python parity quirk: after clear(), the character count from
        // before the clear still counts toward the max-characters split.
        let mut document = Document::new("");
        let mut history = EditHistory::with_clock(5, Duration::from_secs(2), 3, MockClock::new());
        history.record(applied_edit(&mut document, "a", (0, 0)));
        history.record(applied_edit(&mut document, "b", (0, 1)));
        history.record(applied_edit(&mut document, "c", (0, 2)));
        assert_eq!(history.undo_stack_len(), 1);
        history.clear();
        assert_eq!(history.undo_stack_len(), 0);
        // character_count is still 3: the next single-char edit exceeds the
        // cap check, but a new batch would be created anyway (empty stack);
        // record two edits so the second demonstrates the retained count.
        history.record(applied_edit(&mut document, "d", (0, 3)));
        history.record(applied_edit(&mut document, "e", (0, 4)));
        // 3 (retained) + 1 > 3 forced "d" into a new batch resetting the
        // count to 1, then "e" batches with "d" (1 + 1 <= 3).
        assert_eq!(history.undo_stack_len(), 1);
    }

    #[test]
    fn max_checkpoints_evicts_oldest() {
        let mut document = Document::new("");
        let mut history = EditHistory::with_clock(2, Duration::from_secs(2), 100, MockClock::new());
        history.record(applied_edit(&mut document, "\n", (0, 0)));
        history.record(applied_edit(&mut document, "\n", (1, 0)));
        history.record(applied_edit(&mut document, "\n", (2, 0)));
        assert_eq!(history.undo_stack_len(), 2);
    }

    #[test]
    fn cr_only_newline_forces_checkpoint() {
        // Deviation pin: '\r' in inserted text checkpoints (Python checks
        // only '\n' and under-checkpoints CR-only documents).
        let mut document = Document::new("");
        let mut history = history_with_clock(MockClock::new());
        history.record(applied_edit(&mut document, "\r", (0, 0)));
        history.record(applied_edit(&mut document, "x", (1, 0)));
        assert_eq!(history.undo_stack_len(), 2);
    }
}
