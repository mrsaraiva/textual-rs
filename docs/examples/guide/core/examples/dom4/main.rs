/// Port of Python Textual `docs/examples/guide/dom4.py`.
///
/// Demonstrates DOM structure with a dialog containing a question and two buttons.
/// Shows `Container`, `Horizontal` (via layout: horizontal), `Static`, `Button`,
/// `Header`, and `Footer`.
///
/// Note: `Horizontal::new().class(...)` is not available because `Horizontal` does
/// not expose `.class()` builder method. We use `Container` with `layout: horizontal`
/// in CSS to achieve the same visual result.
use textual::prelude::*;

const QUESTION: &str = "Do you want to learn about Textual CSS?";

const CSS: &str = r##"
/* The top level dialog (a Container) */
#dialog {
    height: 100%;
    margin: 4 8;
    background: $panel;
    color: $text;
    border: tall $background;
    padding: 1 2;
}

/* The button class */
Button {
    width: 1fr;
}

/* Matches the question text */
.question {
    text-style: bold;
    height: 100%;
    content-align: center middle;
}

/* Matches the button container */
.buttons {
    width: 100%;
    height: auto;
    dock: bottom;
    layout: horizontal;
}
"##;

struct Dom4App;

impl TextualApp for Dom4App {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Header::new())
            .with_child(Footer::new())
            .with_child(
                Container::new()
                    .id("dialog")
                    .with_child(Static::new(QUESTION).class("question"))
                    .with_child(
                        Container::new()
                            .class("buttons")
                            .with_child(Button::success("Yes"))
                            .with_child(Button::error("No")),
                    ),
            )
    }
}

fn main() -> Result<()> {
    run_sync(Dom4App)
}
