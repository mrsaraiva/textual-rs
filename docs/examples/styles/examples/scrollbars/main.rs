/// Port of Python Textual `docs/examples/styles/scrollbars.py`.
///
/// Demonstrates scrollbar styling: two ScrollableContainer columns inside a
/// Horizontal, one with custom scrollbar colors via the `.right` class.
/// Framework gap: `scrollbar-background`, `scrollbar-color`, `scrollbar-corner-color`
/// CSS properties may not be fully rendered by textual-rs.
use textual::prelude::*;

const TEXT: &str = "I must not fear.\nFear is the mind-killer.\nFear is the little-death that brings total obliteration.\nI will face my fear.\nI will permit it to pass over me and through me.\nAnd when it has gone past, I will turn the inner eye to see its path.\nWhere the fear has gone there will be nothing. Only I will remain.\n";

const CSS: &str = r##"
Label {
    width: 150%;
    height: 150%;
}

.right {
    scrollbar-background: red;
    scrollbar-color: green;
    scrollbar-corner-color: blue;
}

Horizontal > ScrollableContainer {
    width: 50%;
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
                    ScrollableContainer::new()
                        .with_child(Label::new(repeated_text.clone())),
                )
                .with_child(
                    // Python puts `classes="right"` on the ScrollableContainer
                    // itself, so the `.right` scrollbar-* tokens reach the bar.
                    ScrollableContainer::new()
                        .class("right")
                        .with_child(Label::new(repeated_text)),
                ),
        )
    }
}

fn main() -> Result<()> {
    run_sync(ScrollbarApp)
}
