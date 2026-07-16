//! Port of Python `tests/text_area/test_history.py` (all 16 scenarios) plus
//! Rust-specific pins (CRLF round-trip, grapheme-cluster undo, emoji
//! batching).

use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use slotmap::SlotMap;
use textual::document::{Cursor, EditHistory, MockClock, Selection};
use textual::event::EventCtx;
use textual::node_id::NodeId;
use textual::prelude::*;
use textual::runtime::dispatch_ctx::set_dispatch_recipient;

const MAX_CHECKPOINTS: usize = 5;
const SIMPLE_TEXT: &str = "ABCDE\nFGHIJ\nKLMNO\nPQRST\nUVWXY\nZ\n";

fn key(code: KeyCode) -> Event {
    Event::Key(KeyEventData::from_crossterm(KeyEvent::new(
        code,
        KeyModifiers::NONE,
    )))
}

fn make_node_id() -> NodeId {
    let mut sm: SlotMap<NodeId, ()> = SlotMap::new();
    sm.insert(())
}

fn focused_state() -> NodeState {
    NodeState {
        focused: true,
        ..Default::default()
    }
}

/// The Python `TextAreaApp` fixture: a TextArea whose history is replaced
/// wholesale with a mock-clock instance (whole-history replacement is part
/// of the contract).
fn text_area_with_mock_history() -> (TextArea, MockClock) {
    let mut ta = TextArea::new("");
    let clock = MockClock::new();
    *ta.history_mut() =
        EditHistory::with_clock(MAX_CHECKPOINTS, Duration::from_secs(2), 100, clock.clone());
    (ta, clock)
}

fn press(ta: &mut TextArea, code: KeyCode) {
    let mut ctx = EventCtx::default();
    let mut w = textual::event::WidgetCtx::__from_dispatch(NodeId::default(), &mut ctx);
    ta.on_event(&key(code), &mut w);
}

fn press_chars(ta: &mut TextArea, chars: &str) {
    for ch in chars.chars() {
        press(ta, KeyCode::Char(ch));
    }
}

fn sel(start: (usize, usize), end: (usize, usize)) -> Selection {
    Selection {
        start: Cursor {
            row: start.0,
            col: start.1,
        },
        end: Cursor {
            row: end.0,
            col: end.1,
        },
    }
}

fn cursor_sel(row: usize, col: usize) -> Selection {
    Selection::cursor(Cursor { row, col })
}

#[test]
fn test_simple_undo_redo() {
    let (mut ta, _clock) = text_area_with_mock_history();
    let _guard = set_dispatch_recipient(make_node_id(), focused_state());
    ta.insert_at("123", (0, 0));

    assert_eq!(ta.text(), "123");
    ta.undo();
    assert_eq!(ta.text(), "");
    ta.redo();
    assert_eq!(ta.text(), "123");
}

#[test]
fn test_undo_selection_retained() {
    // Select a range of text and press backspace.
    let (mut ta, _clock) = text_area_with_mock_history();
    let _guard = set_dispatch_recipient(make_node_id(), focused_state());
    ta.set_text(SIMPLE_TEXT);
    ta.set_selection(sel((0, 0), (2, 3)));
    press(&mut ta, KeyCode::Backspace);
    assert_eq!(ta.text(), "NO\nPQRST\nUVWXY\nZ\n");
    assert_eq!(ta.selection(), cursor_sel(0, 0));

    // Undo the deletion: the text comes back, and the selection is restored.
    ta.undo();
    assert_eq!(ta.selection(), sel((0, 0), (2, 3)));
    assert_eq!(ta.text(), SIMPLE_TEXT);

    // Redo the deletion: the text is gone again. The selection goes to the
    // post-delete location.
    ta.redo();
    assert_eq!(ta.text(), "NO\nPQRST\nUVWXY\nZ\n");
    assert_eq!(ta.selection(), cursor_sel(0, 0));
}

#[test]
fn test_undo_checkpoint_created_on_cursor_move() {
    let (mut ta, _clock) = text_area_with_mock_history();
    let _guard = set_dispatch_recipient(make_node_id(), focused_state());
    ta.set_text(SIMPLE_TEXT);

    let checkpoint_one = ta.text();
    let checkpoint_one_selection = ta.selection();
    press(&mut ta, KeyCode::Char('1')); // Added to initial batch.

    // This cursor movement ensures a new checkpoint is created.
    let post_insert_one_location = ta.selection();
    press(&mut ta, KeyCode::Down);

    let checkpoint_two = ta.text();
    let checkpoint_two_selection = ta.selection();
    press(&mut ta, KeyCode::Char('2')); // Added to new batch.

    let checkpoint_three = ta.text();
    let checkpoint_three_selection = ta.selection();

    // Going back to checkpoint two.
    ta.undo();
    assert_eq!(ta.text(), checkpoint_two);
    assert_eq!(ta.selection(), checkpoint_two_selection);

    // Back again to checkpoint one (initial state).
    ta.undo();
    assert_eq!(ta.text(), checkpoint_one);
    assert_eq!(ta.selection(), checkpoint_one_selection);

    // Redo to move forward to checkpoint two.
    ta.redo();
    assert_eq!(ta.text(), checkpoint_two);
    assert_eq!(ta.selection(), post_insert_one_location);

    // Redo to move forward to checkpoint three.
    ta.redo();
    assert_eq!(ta.text(), checkpoint_three);
    assert_eq!(ta.selection(), checkpoint_three_selection);
}

#[test]
fn test_setting_text_property_resets_history() {
    let (mut ta, _clock) = text_area_with_mock_history();
    let _guard = set_dispatch_recipient(make_node_id(), focused_state());
    press(&mut ta, KeyCode::Char('1'));

    // Programmatically setting text invalidates the history.
    let text = "Hello, world!";
    ta.set_text(text);

    // The undo doesn't do anything, since we set the text.
    ta.undo();
    assert_eq!(ta.text(), text);
}

#[test]
fn test_edits_batched_by_time() {
    let (mut ta, clock) = text_area_with_mock_history();
    let _guard = set_dispatch_recipient(make_node_id(), focused_state());

    // The first "12" is batched since they happen within 2 seconds.
    clock.set_millis(0);
    press(&mut ta, KeyCode::Char('1'));

    clock.set_millis(1000);
    press(&mut ta, KeyCode::Char('2'));

    // Since "3" appears 10 seconds later, it's in a separate batch.
    clock.advance_millis(10_000);
    press(&mut ta, KeyCode::Char('3'));

    assert_eq!(ta.text(), "123");

    ta.undo();
    assert_eq!(ta.text(), "12");

    ta.undo();
    assert_eq!(ta.text(), "");
}

#[test]
fn test_undo_checkpoint_character_limit_reached() {
    let (mut ta, _clock) = text_area_with_mock_history();
    let _guard = set_dispatch_recipient(make_node_id(), focused_state());
    press(&mut ta, KeyCode::Char('1'));
    // Since the insertion below is > 100 characters it goes to a new batch.
    ta.insert(&"2".repeat(120));

    ta.undo();
    assert_eq!(ta.text(), "1");
    ta.undo();
    assert_eq!(ta.text(), "");
}

#[test]
fn test_redo_with_no_undo_is_noop() {
    let (mut ta, _clock) = text_area_with_mock_history();
    ta.set_text(SIMPLE_TEXT);
    ta.redo();
    assert_eq!(ta.text(), SIMPLE_TEXT);
}

#[test]
fn test_undo_with_empty_undo_stack_is_noop() {
    let (mut ta, _clock) = text_area_with_mock_history();
    ta.set_text(SIMPLE_TEXT);
    ta.undo();
    assert_eq!(ta.text(), SIMPLE_TEXT);
}

#[test]
fn test_redo_stack_cleared_on_edit() {
    let (mut ta, _clock) = text_area_with_mock_history();
    let _guard = set_dispatch_recipient(make_node_id(), focused_state());
    ta.set_text("");
    press(&mut ta, KeyCode::Char('1'));
    ta.history_mut().checkpoint();
    press(&mut ta, KeyCode::Char('2'));
    ta.history_mut().checkpoint();
    press(&mut ta, KeyCode::Char('3'));

    ta.undo();
    ta.undo();
    ta.undo();
    assert_eq!(ta.text(), "");
    assert_eq!(ta.selection(), cursor_sel(0, 0));

    // Redo stack has 3 edits in it now.
    press(&mut ta, KeyCode::Char('f'));
    assert_eq!(ta.text(), "f");
    assert_eq!(ta.selection(), cursor_sel(0, 1));

    // Redo stack is cleared because of the edit, so redo has no effect.
    ta.redo();
    assert_eq!(ta.text(), "f");
    assert_eq!(ta.selection(), cursor_sel(0, 1));
    ta.redo();
    assert_eq!(ta.text(), "f");
    assert_eq!(ta.selection(), cursor_sel(0, 1));
}

#[test]
fn test_inserts_not_batched_with_deletes() {
    // 3 batches here: __1___  ___________2____________  __3__
    let (mut ta, _clock) = text_area_with_mock_history();
    let _guard = set_dispatch_recipient(make_node_id(), focused_state());
    press_chars(&mut ta, "123");
    press(&mut ta, KeyCode::Backspace);
    press(&mut ta, KeyCode::Backspace);
    press_chars(&mut ta, "23");

    assert_eq!(ta.text(), "123");

    // Undo batch 3: the "23" insertion.
    ta.undo();
    assert_eq!(ta.text(), "1");

    // Undo batch 2: the double backspace.
    ta.undo();
    assert_eq!(ta.text(), "123");

    // Undo batch 1: the "123" insertion.
    ta.undo();
    assert_eq!(ta.text(), "");
}

#[test]
fn test_paste_is_an_isolated_batch() {
    let (mut ta, _clock) = text_area_with_mock_history();
    let id = make_node_id();
    let _guard = set_dispatch_recipient(id, focused_state());

    let paste = |ta: &mut TextArea, text: &str| {
        let mut ctx = EventCtx::default();
        let mut w = textual::event::WidgetCtx::__from_dispatch(NodeId::default(), &mut ctx);
        ta.on_message(
            &MessageEvent::new(
                NodeId::default(),
                TextEditClipboardPaste {
                    target: id,
                    text: text.to_string(),
                },
            ),
            &mut w,
        );
    };

    paste(&mut ta, "hello ");
    paste(&mut ta, "world");
    assert_eq!(ta.text(), "hello world");

    press(&mut ta, KeyCode::Char('!'));

    // The insertion of "!" does not get batched with the paste of "world".
    ta.undo();
    assert_eq!(ta.text(), "hello world");

    ta.undo();
    assert_eq!(ta.text(), "hello ");

    ta.undo();
    assert_eq!(ta.text(), "");
}

#[test]
fn test_focus_creates_checkpoint() {
    let (mut ta, _clock) = text_area_with_mock_history();
    let _guard = set_dispatch_recipient(make_node_id(), focused_state());
    press_chars(&mut ta, "123");

    // Blur then re-focus the widget.
    ta.on_node_state_changed(focused_state(), NodeState::default());
    ta.on_node_state_changed(NodeState::default(), focused_state());

    press_chars(&mut ta, "456");
    assert_eq!(ta.text(), "123456");

    // Since we re-focused, a checkpoint exists between 123 and 456, so when
    // we use undo, only the 456 is removed.
    ta.undo();
    assert_eq!(ta.text(), "123");
}

#[test]
fn test_undo_redo_deletions_batched() {
    let (mut ta, _clock) = text_area_with_mock_history();
    let _guard = set_dispatch_recipient(make_node_id(), focused_state());
    ta.set_text(SIMPLE_TEXT);
    ta.set_selection(sel((0, 2), (1, 2)));

    // A single delete of some selected text. It lives in its own batch
    // since it's a multi-line operation.
    press(&mut ta, KeyCode::Backspace);
    let checkpoint_one = "ABHIJ\nKLMNO\nPQRST\nUVWXY\nZ\n";
    assert_eq!(ta.text(), checkpoint_one);
    assert_eq!(ta.selection(), cursor_sel(0, 2));

    // Press backspace a few times to delete more characters.
    press(&mut ta, KeyCode::Backspace);
    press(&mut ta, KeyCode::Backspace);
    press(&mut ta, KeyCode::Backspace);
    let checkpoint_two = "HIJ\nKLMNO\nPQRST\nUVWXY\nZ\n";
    assert_eq!(ta.text(), checkpoint_two);
    assert_eq!(ta.selection(), cursor_sel(0, 0));

    // When we undo, the 3 deletions above should be batched, but not the
    // original deletion since it contains a newline character.
    ta.undo();
    assert_eq!(ta.text(), checkpoint_one);
    assert_eq!(ta.selection(), cursor_sel(0, 2));

    // Undoing again restores us back to our initial text and selection.
    ta.undo();
    assert_eq!(ta.text(), SIMPLE_TEXT);
    assert_eq!(ta.selection(), sel((0, 2), (1, 2)));

    // Redo to go back to checkpoint one.
    ta.redo();
    assert_eq!(ta.text(), checkpoint_one);
    assert_eq!(ta.selection(), cursor_sel(0, 2));

    // Redo again to go back to checkpoint two.
    ta.redo();
    assert_eq!(ta.text(), checkpoint_two);
    assert_eq!(ta.selection(), cursor_sel(0, 0));

    // Redo again does nothing.
    ta.redo();
    assert_eq!(ta.text(), checkpoint_two);
    assert_eq!(ta.selection(), cursor_sel(0, 0));
}

#[test]
fn test_max_checkpoints() {
    let (mut ta, _clock) = text_area_with_mock_history();
    let _guard = set_dispatch_recipient(make_node_id(), focused_state());
    assert_eq!(ta.history().undo_stack_len(), 0);
    for _ in 0..MAX_CHECKPOINTS {
        // Press enter since that will ensure a checkpoint is created.
        press(&mut ta, KeyCode::Enter);
    }

    assert_eq!(ta.history().undo_stack_len(), MAX_CHECKPOINTS);
    press(&mut ta, KeyCode::Enter);
    // Ensure we don't go over the limit.
    assert_eq!(ta.history().undo_stack_len(), MAX_CHECKPOINTS);
}

#[test]
fn test_redo_stack() {
    let (mut ta, _clock) = text_area_with_mock_history();
    let _guard = set_dispatch_recipient(make_node_id(), focused_state());
    assert_eq!(ta.history().redo_stack_len(), 0);
    press(&mut ta, KeyCode::Enter);
    press_chars(&mut ta, "123");
    assert_eq!(ta.history().undo_stack_len(), 2);
    assert_eq!(ta.history().redo_stack_len(), 0);
    ta.undo();
    assert_eq!(ta.history().undo_stack_len(), 1);
    assert_eq!(ta.history().redo_stack_len(), 1);
    ta.undo();
    assert_eq!(ta.history().undo_stack_len(), 0);
    assert_eq!(ta.history().redo_stack_len(), 2);
    ta.redo();
    assert_eq!(ta.history().undo_stack_len(), 1);
    assert_eq!(ta.history().redo_stack_len(), 1);
    ta.redo();
    assert_eq!(ta.history().undo_stack_len(), 2);
    assert_eq!(ta.history().redo_stack_len(), 0);
}

#[test]
fn test_backward_selection_undo_redo() {
    let (mut ta, _clock) = text_area_with_mock_history();
    let _guard = set_dispatch_recipient(make_node_id(), focused_state());
    // Failed prior to https://github.com/Textualize/textual/pull/4352
    ta.set_text(SIMPLE_TEXT);
    ta.set_selection(sel((3, 2), (0, 0)));

    press(&mut ta, KeyCode::Char('a'));

    ta.undo();
    press(&mut ta, KeyCode::Down);
    press(&mut ta, KeyCode::Down);
    press(&mut ta, KeyCode::Down);
    press(&mut ta, KeyCode::Down);

    assert_eq!(ta.text(), SIMPLE_TEXT);
}

// ── Rust-specific pins ─────────────────────────────────────────────────────

#[test]
fn crlf_document_round_trips_through_edit_and_undo() {
    // The document's newline style is preserved through the funnel and undo.
    let (mut ta, _clock) = text_area_with_mock_history();
    let _guard = set_dispatch_recipient(make_node_id(), focused_state());
    ta.set_text("one\r\ntwo");
    assert_eq!(ta.newline(), "\r\n");
    press(&mut ta, KeyCode::Char('X'));
    assert_eq!(ta.text(), "Xone\r\ntwo");
    ta.undo();
    assert_eq!(ta.text(), "one\r\ntwo");
    assert_eq!(ta.newline(), "\r\n");
    ta.redo();
    assert_eq!(ta.text(), "Xone\r\ntwo");
}

#[test]
fn undo_restores_grapheme_cluster_edits_exactly() {
    // Deleting a multi-codepoint cluster (emoji ZWJ sequence) restores the
    // exact text and selection on undo.
    let (mut ta, _clock) = text_area_with_mock_history();
    let _guard = set_dispatch_recipient(make_node_id(), focused_state());
    ta.set_text("a\u{0301}👩‍🚀z");
    press(&mut ta, KeyCode::End);
    press(&mut ta, KeyCode::Left);
    let before_selection = ta.selection();
    press(&mut ta, KeyCode::Backspace);
    assert_eq!(ta.text(), "a\u{0301}z");
    ta.undo();
    assert_eq!(ta.text(), "a\u{0301}👩‍🚀z");
    assert_eq!(ta.selection(), before_selection);
}

#[test]
fn multi_codepoint_cluster_insert_checkpoints_as_multi_character() {
    // A ZWJ emoji counts > 1 codepoint, so it forms an isolated batch in
    // both Python and Rust (codepoint-count batching parity).
    let (mut ta, _clock) = text_area_with_mock_history();
    let _guard = set_dispatch_recipient(make_node_id(), focused_state());
    press(&mut ta, KeyCode::Char('a'));
    ta.insert("👩‍🚀");
    press(&mut ta, KeyCode::Char('b'));
    assert_eq!(ta.history().undo_stack_len(), 3);
}

#[test]
fn undo_redo_work_in_read_only_mode() {
    // The read-only gate applies to mutations, not undo/redo (intentional;
    // Python does not gate action_undo/action_redo either).
    use textual::action::ParsedAction;
    let (mut ta, _clock) = text_area_with_mock_history();
    let _guard = set_dispatch_recipient(make_node_id(), focused_state());
    press(&mut ta, KeyCode::Char('x'));
    let mut ta = ta.with_read_only(true);

    let undo = ParsedAction {
        namespace: None,
        name: "undo".to_string(),
        arguments: vec![],
    };
    let mut ctx = EventCtx::default();
    {
        let mut w = textual::event::WidgetCtx::__from_dispatch(NodeId::default(), &mut ctx);
        assert!(ta.execute_action(&undo, &mut w));
    }
    assert_eq!(ta.text(), "");

    let redo = ParsedAction {
        namespace: None,
        name: "redo".to_string(),
        arguments: vec![],
    };
    let mut ctx = EventCtx::default();
    {
        let mut w = textual::event::WidgetCtx::__from_dispatch(NodeId::default(), &mut ctx);
        assert!(ta.execute_action(&redo, &mut w));
    }
    assert_eq!(ta.text(), "x");
}
