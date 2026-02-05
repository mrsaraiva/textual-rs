use textual::prelude::*;

/// Mirrors Python Textual's `docs/examples/widgets/text_area_extended.py`.
#[tokio::main]
async fn main() -> Result<()> {
    if cfg!(test) {
        return Ok(());
    }

    let editor = TextArea::code_editor("")
        .with_language("python")
        .on_key(|text_area, key, ctx| {
            if key.code == crossterm::event::KeyCode::Char('(') {
                text_area.insert("()");
                text_area.move_cursor_relative(-1, 0);
                ctx.request_repaint();
                ctx.set_handled();
            }
        });

    let mut root = AppRoot::new().with_child(editor);
    let mut app = App::new()?;
    app.run_widget_tree(&mut root).await
}
