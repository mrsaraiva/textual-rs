/// Port of Python Textual `docs/examples/guide/dom3.py`.
///
/// Demonstrates a dialog layout with a question and Yes/No buttons,
/// using Header, Footer, Container, Horizontal, Static, and Button.
use textual::prelude::*;

const QUESTION: &str = "Do you want to learn about Textual CSS?";

const CSS: &str = r##""##;

struct ExampleApp;

impl TextualApp for ExampleApp {
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
                        .with_child(Static::new(QUESTION).class("question"))
                        .with_child(
                            
                                Horizontal::new()
                                    .with_child(Button::success("Yes"))
                                    .with_child(Button::error("No"))
                            .class("buttons"),
                        )
                .id("dialog"),
            )
    }
}

fn main() -> textual::Result<()> {
    run_sync(ExampleApp)
}
