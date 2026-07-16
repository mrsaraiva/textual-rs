use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use rich_rs::Console;
use slotmap::SlotMap;
use textual::node_id::NodeId;
use textual::prelude::*;
use textual::render::FrameBuffer;
use textual::runtime::dispatch_ctx::set_dispatch_recipient;

fn key(code: KeyCode, modifiers: KeyModifiers) -> Event {
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
fn input_accepts_typing() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (12, 1);
    options.max_width = 12;
    options.max_height = 1;

    let mut input = Input::new().with_placeholder("name");
    let _guard = set_dispatch_recipient(make_node_id(), focused_state());

    let key =
        KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::empty()));
    { let mut __e = textual::event::EventCtx::default(); let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut __e); input.on_event(&Event::Key(key), &mut __w) };

    let buf = FrameBuffer::from_renderable(&console, &options, &input, None);
    insta::assert_snapshot!(buf.debug_dump());
}

#[test]
fn input_shift_selection_then_backspace_deletes_selected_text() {
    let mut input = Input::new();
    let _guard = set_dispatch_recipient(make_node_id(), focused_state());
    input.set_text("hello world");

    { let mut __e = textual::event::EventCtx::default(); let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut __e); input.on_event(
        &key(KeyCode::End, KeyModifiers::NONE),
        &mut __w) };
    { let mut __e = textual::event::EventCtx::default(); let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut __e); input.on_event(
        &key(KeyCode::Left, KeyModifiers::SHIFT),
        &mut __w) };
    { let mut __e = textual::event::EventCtx::default(); let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut __e); input.on_event(
        &key(KeyCode::Backspace, KeyModifiers::NONE),
        &mut __w) };

    assert_eq!(input.text(), "hello worl");
}

#[test]
fn input_ctrl_backspace_deletes_previous_word() {
    let mut input = Input::new();
    let _guard = set_dispatch_recipient(make_node_id(), focused_state());
    input.set_text("alpha beta");
    { let mut __e = textual::event::EventCtx::default(); let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut __e); input.on_event(
        &key(KeyCode::End, KeyModifiers::NONE),
        &mut __w) };
    { let mut __e = textual::event::EventCtx::default(); let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut __e); input.on_event(
        &key(KeyCode::Backspace, KeyModifiers::CONTROL),
        &mut __w) };

    assert_eq!(input.text(), "alpha ");
}

#[test]
fn input_super_left_and_alt_backspace_shortcuts_work() {
    let mut input = Input::new();
    let _guard = set_dispatch_recipient(make_node_id(), focused_state());
    input.set_text("alpha beta");
    { let mut __e = textual::event::EventCtx::default(); let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut __e); input.on_event(
        &key(KeyCode::End, KeyModifiers::NONE),
        &mut __w) };
    { let mut __e = textual::event::EventCtx::default(); let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut __e); input.on_event(
        &key(KeyCode::Left, KeyModifiers::SUPER),
        &mut __w) };
    { let mut __e = textual::event::EventCtx::default(); let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut __e); input.on_event(
        &key(KeyCode::Char('Z'), KeyModifiers::NONE),
        &mut __w) };
    assert_eq!(input.text(), "Zalpha beta");

    input.set_text("alpha beta");
    { let mut __e = textual::event::EventCtx::default(); let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut __e); input.on_event(
        &key(KeyCode::End, KeyModifiers::NONE),
        &mut __w) };
    { let mut __e = textual::event::EventCtx::default(); let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut __e); input.on_event(
        &key(KeyCode::Backspace, KeyModifiers::ALT),
        &mut __w) };
    assert_eq!(input.text(), "alpha ");
}

// ── restrict: Python `re.fullmatch` contract (_input.py) ────────────────────

fn type_chars(input: &mut Input, chars: &str) {
    for ch in chars.chars() {
        let mut __e = textual::event::EventCtx::default();
        let mut __w = textual::event::WidgetCtx::__from_dispatch(
            textual::node_id::NodeId::default(),
            &mut __e,
        );
        input.on_event(&key(KeyCode::Char(ch), KeyModifiers::NONE), &mut __w);
    }
}

#[test]
fn input_restrict_fullmatch_accepts_all_digit_value() {
    let mut input = Input::new().with_restrict(r"\d+");
    let _guard = set_dispatch_recipient(make_node_id(), focused_state());

    type_chars(&mut input, "123");
    assert_eq!(input.text(), "123");
}

#[test]
fn input_restrict_fullmatch_rejects_non_digit_chars() {
    // Python checks `re.fullmatch(restrict, value)`: the WHOLE value must
    // match `\d+`, so "abc" stays empty and "12a" keeps only "12".
    let mut input = Input::new().with_restrict(r"\d+");
    let _guard = set_dispatch_recipient(make_node_id(), focused_state());

    type_chars(&mut input, "abc");
    assert_eq!(input.text(), "");

    type_chars(&mut input, "12a");
    assert_eq!(input.text(), "12");
}

#[test]
fn input_restrict_fullmatch_rejects_suffix_after_matching_prefix() {
    // Regression: unanchored `is_match` accepted "123a" because it CONTAINS
    // a `\d+` match; fullmatch semantics must reject the trailing letter.
    let mut input = Input::new().with_restrict(r"\d+");
    let _guard = set_dispatch_recipient(make_node_id(), focused_state());

    input.insert_text_at_cursor("123");
    assert_eq!(input.text(), "123");

    type_chars(&mut input, "a");
    assert_eq!(input.text(), "123");

    input.insert_text_at_cursor("a");
    assert_eq!(input.text(), "123");
}
