use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use textual::prelude::*;

fn key(code: KeyCode) -> Event {
    Event::Key(KeyEventData::from_crossterm(KeyEvent::new(
        code,
        KeyModifiers::NONE,
    )))
}

#[test]
fn text_area_backspace_deletes_full_emoji_cluster() {
    let mut text_area = TextArea::new("a\u{0301}👩‍🚀z");
    text_area.set_focus(true);
    let mut ctx = EventCtx::default();

    text_area.on_event(&key(KeyCode::End), &mut ctx);
    text_area.on_event(&key(KeyCode::Left), &mut ctx);
    text_area.on_event(&key(KeyCode::Backspace), &mut ctx);

    assert_eq!(text_area.text(), "a\u{0301}z");
}

#[test]
fn text_area_backspace_deletes_combining_cluster_as_unit() {
    let mut text_area = TextArea::new("a\u{0301}b");
    text_area.set_focus(true);
    let mut ctx = EventCtx::default();

    text_area.on_event(&key(KeyCode::End), &mut ctx);
    text_area.on_event(&key(KeyCode::Left), &mut ctx);
    text_area.on_event(&key(KeyCode::Backspace), &mut ctx);

    assert_eq!(text_area.text(), "b");
}

#[test]
fn text_area_edit_emits_text_area_changed_message() {
    let mut text_area = TextArea::new("");
    text_area.set_focus(true);
    let mut ctx = EventCtx::default();

    text_area.on_event(&key(KeyCode::Char('x')), &mut ctx);
    let messages = ctx.take_messages();

    assert!(
        messages
            .iter()
            .any(|m| matches!(m.message, Message::TextAreaChanged { ref value } if value == "x"))
    );
}
