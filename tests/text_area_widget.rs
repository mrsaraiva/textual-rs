use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use textual::prelude::*;

fn key(code: KeyCode) -> Event {
    Event::Key(KeyEventData::from_crossterm(KeyEvent::new(
        code,
        KeyModifiers::NONE,
    )))
}

fn key_with_modifiers(code: KeyCode, modifiers: KeyModifiers) -> Event {
    Event::Key(KeyEventData::from_crossterm(KeyEvent::new(code, modifiers)))
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
fn text_area_shift_selection_then_backspace_deletes_selected_text() {
    let mut text_area = TextArea::new("hello world");
    text_area.set_focus(true);
    let mut ctx = EventCtx::default();

    text_area.on_event(&key(KeyCode::End), &mut ctx);
    text_area.on_event(
        &key_with_modifiers(KeyCode::Left, KeyModifiers::SHIFT),
        &mut ctx,
    );
    text_area.on_event(&key(KeyCode::Backspace), &mut ctx);

    assert_eq!(text_area.text(), "hello worl");
}

#[test]
fn text_area_ctrl_backspace_deletes_previous_word() {
    let mut text_area = TextArea::new("alpha beta");
    text_area.set_focus(true);
    let mut ctx = EventCtx::default();

    text_area.on_event(&key(KeyCode::End), &mut ctx);
    text_area.on_event(
        &key_with_modifiers(KeyCode::Backspace, KeyModifiers::CONTROL),
        &mut ctx,
    );

    assert_eq!(text_area.text(), "alpha ");
}

#[test]
fn text_area_super_left_and_alt_backspace_shortcuts_work() {
    let mut text_area = TextArea::new("alpha beta");
    text_area.set_focus(true);
    let mut ctx = EventCtx::default();

    text_area.on_event(&key(KeyCode::End), &mut ctx);
    text_area.on_event(
        &key_with_modifiers(KeyCode::Left, KeyModifiers::SUPER),
        &mut ctx,
    );
    assert_eq!(text_area.text(), "alpha beta");

    text_area.on_event(&key(KeyCode::End), &mut ctx);
    text_area.on_event(
        &key_with_modifiers(KeyCode::Backspace, KeyModifiers::ALT),
        &mut ctx,
    );
    assert_eq!(text_area.text(), "alpha ");
}
