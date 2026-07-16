use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use slotmap::SlotMap;
use textual::node_id::NodeId;
use textual::event::EventCtx;
use textual::prelude::*;
use textual::runtime::dispatch_ctx::set_dispatch_recipient;

fn key(code: KeyCode) -> Event {
    Event::Key(KeyEventData::from_crossterm(KeyEvent::new(
        code,
        KeyModifiers::NONE,
    )))
}

fn key_with_modifiers(code: KeyCode, modifiers: KeyModifiers) -> Event {
    Event::Key(KeyEventData::from_crossterm(KeyEvent::new(code, modifiers)))
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

#[test]
fn text_area_backspace_deletes_full_emoji_cluster() {
    let mut text_area = TextArea::new("a\u{0301}👩‍🚀z");
    let _guard = set_dispatch_recipient(make_node_id(), focused_state());
    let mut ctx = EventCtx::default();

    {
        let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut ctx);
        text_area.on_event(&key(KeyCode::End), &mut __w);
    }
    {
        let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut ctx);
        text_area.on_event(&key(KeyCode::Left), &mut __w);
    }
    {
        let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut ctx);
        text_area.on_event(&key(KeyCode::Backspace), &mut __w);
    }

    assert_eq!(text_area.text(), "a\u{0301}z");
}

#[test]
fn text_area_backspace_deletes_combining_cluster_as_unit() {
    let mut text_area = TextArea::new("a\u{0301}b");
    let _guard = set_dispatch_recipient(make_node_id(), focused_state());
    let mut ctx = EventCtx::default();

    {
        let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut ctx);
        text_area.on_event(&key(KeyCode::End), &mut __w);
    }
    {
        let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut ctx);
        text_area.on_event(&key(KeyCode::Left), &mut __w);
    }
    {
        let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut ctx);
        text_area.on_event(&key(KeyCode::Backspace), &mut __w);
    }

    assert_eq!(text_area.text(), "b");
}

#[test]
fn text_area_shift_selection_then_backspace_deletes_selected_text() {
    let mut text_area = TextArea::new("hello world");
    let _guard = set_dispatch_recipient(make_node_id(), focused_state());
    let mut ctx = EventCtx::default();

    {
        let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut ctx);
        text_area.on_event(&key(KeyCode::End), &mut __w);
    }
    {
        let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut ctx);
        text_area.on_event(
        &key_with_modifiers(KeyCode::Left, KeyModifiers::SHIFT),
        &mut __w);
    }
    {
        let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut ctx);
        text_area.on_event(&key(KeyCode::Backspace), &mut __w);
    }

    assert_eq!(text_area.text(), "hello worl");
}

#[test]
fn text_area_ctrl_backspace_deletes_previous_word() {
    let mut text_area = TextArea::new("alpha beta");
    let _guard = set_dispatch_recipient(make_node_id(), focused_state());
    let mut ctx = EventCtx::default();

    {
        let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut ctx);
        text_area.on_event(&key(KeyCode::End), &mut __w);
    }
    {
        let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut ctx);
        text_area.on_event(
        &key_with_modifiers(KeyCode::Backspace, KeyModifiers::CONTROL),
        &mut __w);
    }

    assert_eq!(text_area.text(), "alpha ");
}

#[test]
fn text_area_super_left_and_alt_backspace_shortcuts_work() {
    let mut text_area = TextArea::new("alpha beta");
    let _guard = set_dispatch_recipient(make_node_id(), focused_state());
    let mut ctx = EventCtx::default();

    {
        let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut ctx);
        text_area.on_event(&key(KeyCode::End), &mut __w);
    }
    {
        let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut ctx);
        text_area.on_event(
        &key_with_modifiers(KeyCode::Left, KeyModifiers::SUPER),
        &mut __w);
    }
    assert_eq!(text_area.text(), "alpha beta");

    {
        let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut ctx);
        text_area.on_event(&key(KeyCode::End), &mut __w);
    }
    {
        let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut ctx);
        text_area.on_event(
        &key_with_modifiers(KeyCode::Backspace, KeyModifiers::ALT),
        &mut __w);
    }
    assert_eq!(text_area.text(), "alpha ");
}

// ── document newline style: Python `Document` contract (_document.py) ───────

const NEWLINE_TEXT: &str = "I must not fear.\nFear is the mind-killer.";

#[test]
fn text_area_detects_and_round_trips_windows_newlines() {
    // Python test_newline_windows / test_text: the text we put in is the
    // text we get out, including the CRLF newline style.
    let windows = NEWLINE_TEXT.replace('\n', "\r\n");
    let text_area = TextArea::new(windows.clone());
    assert_eq!(text_area.newline(), "\r\n");
    assert_eq!(text_area.text(), windows);

    // Trailing newline round-trips as an empty final line.
    let with_trailing = format!("{windows}\r\n");
    let text_area = TextArea::new(with_trailing.clone());
    assert_eq!(text_area.text(), with_trailing);
}

#[test]
fn text_area_newline_detection_defaults() {
    // Python _detect_newline_style: \r\n wins, then \n, then \r, default \n.
    assert_eq!(TextArea::new("").newline(), "\n");
    assert_eq!(TextArea::new("no newlines here").newline(), "\n");
    assert_eq!(TextArea::new(NEWLINE_TEXT).newline(), "\n");
    assert_eq!(TextArea::new(NEWLINE_TEXT).text(), NEWLINE_TEXT);
    assert_eq!(TextArea::new("a\rb").newline(), "\r");
    assert_eq!(TextArea::new("a\rb").text(), "a\rb");
}

#[test]
fn text_area_selected_text_carries_document_newline() {
    // Python test_get_selected_text_multiple_lines_windows: a selection that
    // spans a line boundary joins lines with the document newline.
    let windows = NEWLINE_TEXT.replace('\n', "\r\n");
    let mut text_area = TextArea::new(windows);
    text_area.set_selection(TextAreaSelection {
        start: TextAreaCursor { row: 0, col: 2 },
        end: TextAreaCursor { row: 1, col: 2 },
    });
    assert_eq!(
        textual::widgets::Selectable::get_selection(&text_area).as_deref(),
        Some("must not fear.\r\nFe")
    );
}

#[test]
fn text_area_insert_normalizes_foreign_newlines_to_document_newline() {
    // Python test_insert_windows_newlines: inserting \r\n text into a \n
    // document reads back with the document's own newline.
    let mut text_area = TextArea::new(NEWLINE_TEXT);
    text_area.insert("\r\n\r\n\r\n");
    assert_eq!(text_area.text(), format!("\n\n\n{NEWLINE_TEXT}"));
}

#[test]
fn text_area_crlf_document_keeps_grapheme_cluster_editing() {
    // Grapheme guard: \r must never leak into line storage (where it could
    // pair into stray clusters); backspace over an emoji cluster on a CRLF
    // document still deletes the whole cluster.
    let mut text_area = TextArea::new("a\u{0301}👩‍🚀z\r\nsecond");
    assert_eq!(text_area.newline(), "\r\n");
    let _guard = set_dispatch_recipient(make_node_id(), focused_state());
    let mut ctx = EventCtx::default();

    {
        let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut ctx);
        text_area.on_event(&key(KeyCode::End), &mut __w);
    }
    {
        let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut ctx);
        text_area.on_event(&key(KeyCode::Left), &mut __w);
    }
    {
        let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut ctx);
        text_area.on_event(&key(KeyCode::Backspace), &mut __w);
    }

    assert_eq!(text_area.text(), "a\u{0301}z\r\nsecond");
}
