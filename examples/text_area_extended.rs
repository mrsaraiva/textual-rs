use textual::prelude::*;

/// Mirrors Python Textual's `docs/examples/widgets/text_area_extended.py`.
struct TextAreaExtendedApp;

impl TextualApp for TextAreaExtendedApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(
            TextArea::code_editor("")
                .with_language("python")
                .on_key(|text_area, key, ctx| {
                    if key.code == crossterm::event::KeyCode::Char('(') {
                        text_area.insert("()");
                        text_area.move_cursor_relative(-1, 0);
                        ctx.request_repaint();
                        ctx.set_handled();
                    }
                }),
        )
    }
}

fn main() -> Result<()> {
    if cfg!(test) {
        return Ok(());
    }
    run_sync(TextAreaExtendedApp)
}
