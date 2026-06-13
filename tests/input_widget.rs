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
    input.on_event(&Event::Key(key), &mut EventCtx::default());

    let buf = FrameBuffer::from_renderable(&console, &options, &input, None);
    insta::assert_snapshot!(buf.debug_dump());
}

#[test]
fn input_shift_selection_then_backspace_deletes_selected_text() {
    let mut input = Input::new();
    let _guard = set_dispatch_recipient(make_node_id(), focused_state());
    input.set_text("hello world");

    input.on_event(
        &key(KeyCode::End, KeyModifiers::NONE),
        &mut EventCtx::default(),
    );
    input.on_event(
        &key(KeyCode::Left, KeyModifiers::SHIFT),
        &mut EventCtx::default(),
    );
    input.on_event(
        &key(KeyCode::Backspace, KeyModifiers::NONE),
        &mut EventCtx::default(),
    );

    assert_eq!(input.text(), "hello worl");
}

#[test]
fn input_ctrl_backspace_deletes_previous_word() {
    let mut input = Input::new();
    let _guard = set_dispatch_recipient(make_node_id(), focused_state());
    input.set_text("alpha beta");
    input.on_event(
        &key(KeyCode::End, KeyModifiers::NONE),
        &mut EventCtx::default(),
    );
    input.on_event(
        &key(KeyCode::Backspace, KeyModifiers::CONTROL),
        &mut EventCtx::default(),
    );

    assert_eq!(input.text(), "alpha ");
}

#[test]
fn input_super_left_and_alt_backspace_shortcuts_work() {
    let mut input = Input::new();
    let _guard = set_dispatch_recipient(make_node_id(), focused_state());
    input.set_text("alpha beta");
    input.on_event(
        &key(KeyCode::End, KeyModifiers::NONE),
        &mut EventCtx::default(),
    );
    input.on_event(
        &key(KeyCode::Left, KeyModifiers::SUPER),
        &mut EventCtx::default(),
    );
    input.on_event(
        &key(KeyCode::Char('Z'), KeyModifiers::NONE),
        &mut EventCtx::default(),
    );
    assert_eq!(input.text(), "Zalpha beta");

    input.set_text("alpha beta");
    input.on_event(
        &key(KeyCode::End, KeyModifiers::NONE),
        &mut EventCtx::default(),
    );
    input.on_event(
        &key(KeyCode::Backspace, KeyModifiers::ALT),
        &mut EventCtx::default(),
    );
    assert_eq!(input.text(), "alpha ");
}
