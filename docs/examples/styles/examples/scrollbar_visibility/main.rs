/// Port of Python Textual `docs/examples/styles/scrollbar_visibility.py`.
///
/// Demonstrates `scrollbar-visibility` CSS property: left panel keeps the
/// scrollbar always visible, right panel hides it.
///
/// Framework gap: `scrollbar-visibility` property may not yet be fully
/// supported in textual-rs.
use textual::prelude::*;

const TEXT: &str = "I must not fear.\nFear is the mind-killer.\nFear is the little-death that brings total obliteration.\nI will face my fear.\nI will permit it to pass over me and through me.\nAnd when it has gone past, I will turn the inner eye to see its path.\nWhere the fear has gone there will be nothing. Only I will remain.\n";

const CSS: &str = r##"
VerticalScroll {
    width: 1fr;
}

.left {
    scrollbar-visibility: visible;
}

.right {
    scrollbar-visibility: hidden;
}
"##;

struct ScrollbarApp;

impl TextualApp for ScrollbarApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        let repeated_text = TEXT.repeat(10);
        AppRoot::new().with_child(
            Horizontal::new()
                .with_child(
                    VerticalScroll::new()
                        .class("left")
                        .with_child(Label::new(repeated_text.clone())),
                )
                .with_child(
                    VerticalScroll::new()
                        .class("right")
                        .with_child(Label::new(repeated_text)),
                ),
        )
    }
}

fn main() -> Result<()> {
    run_sync(ScrollbarApp)
}
