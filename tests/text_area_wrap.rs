//! Phase 4 regression tests: wrap-aware navigation, the Python-parity
//! Up/Down boundary behavior change, soft-wrap rendering, visual-space
//! scrolling, and wrapped hit-testing.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use rich_rs::Console;
use slotmap::SlotMap;
use textual::document::{Cursor, Selection};
use textual::event::EventCtx;
use textual::node_id::NodeId;
use textual::prelude::*;
use textual::render::FrameBuffer;
use textual::runtime::dispatch_ctx::set_dispatch_recipient;

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

fn press(ta: &mut TextArea, code: KeyCode) {
    let mut ctx = EventCtx::default();
    let mut w = textual::event::WidgetCtx::__from_dispatch(NodeId::default(), &mut ctx);
    ta.on_event(&key(code), &mut w);
}

fn cursor_of(ta: &TextArea) -> (usize, usize) {
    let sel = ta.selection();
    (sel.end.row, sel.end.col)
}

fn options_for(console: &Console, width: usize, height: usize) -> rich_rs::ConsoleOptions {
    let mut options = console.options().clone();
    options.size = (width, height);
    options.max_width = width;
    options.max_height = height;
    options
}

fn plain_lines(ta: &TextArea, width: usize, height: usize) -> Vec<String> {
    let console = Console::new();
    let options = options_for(&console, width, height);
    let buf = FrameBuffer::from_renderable(&console, &options, ta, None);
    buf.as_plain_lines()
}

// ── Intended behavior change: Up/Down boundary navigation (CHANGELOG) ──────

#[test]
fn up_on_first_line_moves_to_document_start() {
    for soft_wrap in [false, true] {
        let mut ta = TextArea::new("hello\nworld").with_soft_wrap(soft_wrap);
        let _guard = set_dispatch_recipient(make_node_id(), focused_state());
        ta.set_selection(Selection::cursor(Cursor { row: 0, col: 3 }));
        press(&mut ta, KeyCode::Up);
        assert_eq!(cursor_of(&ta), (0, 0), "soft_wrap {soft_wrap}");
    }
}

#[test]
fn down_on_last_line_moves_to_line_end() {
    for soft_wrap in [false, true] {
        let mut ta = TextArea::new("hello\nworld").with_soft_wrap(soft_wrap);
        let _guard = set_dispatch_recipient(make_node_id(), focused_state());
        ta.set_selection(Selection::cursor(Cursor { row: 1, col: 2 }));
        press(&mut ta, KeyCode::Down);
        assert_eq!(cursor_of(&ta), (1, 5), "soft_wrap {soft_wrap}");
    }
}

// ── Wrapped vertical movement through sections ─────────────────────────────

#[test]
fn down_moves_through_wrapped_sections_of_the_same_line() {
    // The navigator fixture: "01 3456" wraps at width 4 into "01 " / "3456".
    let mut ta = TextArea::new("01 3456\n01234");
    let _guard = set_dispatch_recipient(make_node_id(), focused_state());
    // Layout 5 wide: wrap width = 5 - 0 (gutter) - 1 (cursor) = 4.
    ta.on_layout(5, 6);
    ta.set_selection(Selection::cursor(Cursor { row: 0, col: 1 }));
    press(&mut ta, KeyCode::Down);
    // Down from "01 " lands within the SAME document line, on "3456".
    assert_eq!(cursor_of(&ta), (0, 4));
    press(&mut ta, KeyCode::Down);
    assert_eq!(cursor_of(&ta), (1, 1));
}

#[test]
fn home_and_end_operate_on_wrapped_sections() {
    let mut ta = TextArea::new("01 3456\n01234");
    let _guard = set_dispatch_recipient(make_node_id(), focused_state());
    ta.on_layout(5, 6);
    // Cursor inside the second section of line 0.
    ta.set_selection(Selection::cursor(Cursor { row: 0, col: 5 }));
    press(&mut ta, KeyCode::Home);
    assert_eq!(cursor_of(&ta), (0, 3), "home goes to the wrap offset");
    press(&mut ta, KeyCode::End);
    assert_eq!(cursor_of(&ta), (0, 7), "end goes to the line end");
    // From the first section, End stops at the section end.
    ta.set_selection(Selection::cursor(Cursor { row: 0, col: 0 }));
    press(&mut ta, KeyCode::End);
    assert_eq!(cursor_of(&ta), (0, 2));
}

#[test]
fn vertical_move_restores_preferred_x_through_short_line() {
    let mut ta = TextArea::new("abcdef\nab\nabcdef").with_soft_wrap(false);
    let _guard = set_dispatch_recipient(make_node_id(), focused_state());
    ta.on_layout(10, 5);
    ta.set_selection(Selection::cursor(Cursor { row: 0, col: 5 }));
    press(&mut ta, KeyCode::Down);
    assert_eq!(cursor_of(&ta), (1, 2), "clamped to the short line");
    press(&mut ta, KeyCode::Down);
    // The remembered visual x (5) is restored on the longer line.
    assert_eq!(cursor_of(&ta), (2, 5));
}

// ── Soft-wrap rendering (default TextArea now wraps; CHANGELOG) ────────────

#[test]
fn soft_wrap_renders_sections_on_separate_rows() {
    let ta = TextArea::new("123 4567\n12345");
    // Render width 5 -> wrap width 4 (one cell reserved for the cursor).
    let lines = plain_lines(&ta, 5, 5);
    assert_eq!(lines[0].trim_end(), "123");
    assert_eq!(lines[1].trim_end(), "4567");
    assert_eq!(lines[2].trim_end(), "1234");
    assert_eq!(lines[3].trim_end(), "5");
}

#[test]
fn soft_wrap_off_overflows_horizontally() {
    let ta = TextArea::new("123 4567\n12345").with_soft_wrap(false);
    let lines = plain_lines(&ta, 5, 5);
    assert_eq!(lines[0].trim_end(), "123 4");
    assert_eq!(lines[1].trim_end(), "12345");
    assert_eq!(lines[2].trim_end(), "");
}

#[test]
fn gutter_numbers_appear_only_on_first_sections() {
    let ta = TextArea::new("123 4567\n12345").with_show_line_numbers(true);
    // Width 9: gutter 3 ("N  "), text 6, wrap width 5.
    let lines = plain_lines(&ta, 9, 5);
    assert!(lines[0].starts_with("1  123"), "line {:?}", lines[0]);
    assert!(
        lines[1].trim_start().starts_with("4567"),
        "continuation row has a blank gutter: {:?}",
        lines[1]
    );
    assert!(lines[1].starts_with("   "), "line {:?}", lines[1]);
    assert!(lines[2].starts_with("2  12345"), "line {:?}", lines[2]);
}

#[test]
fn wrapped_render_tracks_edits_incrementally() {
    let mut ta = TextArea::new("123 4567");
    let _guard = set_dispatch_recipient(make_node_id(), focused_state());
    ta.on_layout(5, 5);
    // Type at the end of the document; the funnel wrap_range keeps the
    // wrapped view in sync with the document.
    let end = ta.document().end();
    ta.set_selection(Selection::cursor(end.into()));
    press(&mut ta, KeyCode::Char('8'));
    press(&mut ta, KeyCode::Char('9'));
    let lines = plain_lines(&ta, 5, 5);
    assert_eq!(lines[0].trim_end(), "123");
    assert_eq!(lines[1].trim_end(), "4567");
    assert_eq!(lines[2].trim_end(), "89");
}

// ── Visual-space scrolling ─────────────────────────────────────────────────

#[test]
fn scroll_row_clamps_to_wrapped_height_after_shrinking_edit() {
    let text = (0..30)
        .map(|i| format!("line{i}"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut ta = TextArea::new(text).with_soft_wrap(false);
    let _guard = set_dispatch_recipient(make_node_id(), focused_state());
    ta.on_layout(12, 4);
    // Move the cursor (and scroll) to the bottom of the document.
    let end = ta.document().end();
    ta.set_selection(Selection::cursor(end.into()));
    // Delete almost everything; the viewport must not be left past the end.
    ta.delete((1, 0), end);
    let lines = plain_lines(&ta, 12, 4);
    assert_eq!(lines[0].trim_end(), "line0");
}

#[test]
fn wrapped_document_scrolls_vertically_to_cursor() {
    // One long line wrapping into many sections; the cursor at the line end
    // must scroll the view down in VISUAL rows.
    let mut ta = TextArea::new("aaaa ".repeat(10));
    let _guard = set_dispatch_recipient(make_node_id(), focused_state());
    ta.on_layout(6, 3);
    let end = ta.document().end();
    ta.set_selection(Selection::cursor(end.into()));
    let lines = plain_lines(&ta, 6, 3);
    // The final section (trailing content) is visible.
    assert!(
        lines.iter().any(|l| l.trim_end().ends_with("aaaa")),
        "lines {lines:?}"
    );
}

#[test]
fn selection_spanning_a_wrap_point_styles_both_rows() {
    // Selection styling keys off document space: a selection crossing the
    // wrap offset is painted on both visual rows (segments split at the
    // selection boundaries on each section).
    let mut ta = TextArea::new("01 3456");
    ta.set_selection(Selection {
        start: Cursor { row: 0, col: 1 },
        end: Cursor { row: 0, col: 5 },
    });
    let console = Console::new();
    let options = options_for(&console, 5, 3); // wrap width 4: "01 " / "3456"
    let segments = textual::widgets::Render::render(&ta, &console, &options);
    let debug = format!("{segments:?}");
    // Row 0: "0" unselected + "1 " selected; row 1: "34" selected + "56".
    assert!(debug.contains("\"1 \""), "selected tail of row 0: {debug}");
    assert!(debug.contains("\"34\""), "selected head of row 1: {debug}");
    assert!(
        debug.contains("\"56\""),
        "unselected tail of row 1: {debug}"
    );
}

// ── LF-canonical syntax invariant ──────────────────────────────────────────

#[test]
fn syntax_spans_stay_stable_across_crlf_and_wrapped_render() {
    // The syntax source is LF-joined regardless of the document newline
    // style; spans are document-space byte ranges and must survive both a
    // CRLF document and wrapped rendering.
    let ta = TextArea::new("def foo():\r\n    return 1\r\n").with_language("python");
    let console = Console::new();
    let options = options_for(&console, 8, 8); // wrap width 7 wraps "return 1"
    let segments = textual::widgets::Render::render(&ta, &console, &options);
    let debug = format!("{segments:?}");
    // The keywords are emitted as their own styled segments (span
    // boundaries respected through the wrap).
    assert!(debug.contains("\"def\""), "missing def segment: {debug}");
    assert!(
        debug.contains("\"return\""),
        "missing return segment: {debug}"
    );
    assert_eq!(ta.newline(), "\r\n");
}

// ── Wrapped hit-testing ────────────────────────────────────────────────────

#[test]
fn mouse_click_on_wrapped_section_maps_to_document_location() {
    let mut ta = TextArea::new("01 3456\n01234");
    let id = make_node_id();
    let _guard = set_dispatch_recipient(id, focused_state());
    ta.on_layout(5, 6);

    let mut ctx = EventCtx::default();
    {
        let mut w = textual::event::WidgetCtx::__from_dispatch(NodeId::default(), &mut ctx);
        // Click visual row 1 ("3456", the wrapped section of line 0), x=2.
        ta.on_event(
            &Event::MouseDown(textual::event::MouseDownEvent {
                target: id,
                screen_x: 2,
                screen_y: 1,
                x: 2,
                y: 1,
            }),
            &mut w,
        );
    }
    assert_eq!(cursor_of(&ta), (0, 5));

    // Clicking past the end of a section clamps (offset_to_location).
    let mut ctx = EventCtx::default();
    {
        let mut w = textual::event::WidgetCtx::__from_dispatch(NodeId::default(), &mut ctx);
        ta.on_event(
            &Event::MouseDown(textual::event::MouseDownEvent {
                target: id,
                screen_x: 4,
                screen_y: 3,
                x: 4,
                y: 3,
            }),
            &mut w,
        );
    }
    // Visual row 3 is "4" (last section of line 1): x=4 clamps to line end.
    assert_eq!(cursor_of(&ta), (1, 5));
}
