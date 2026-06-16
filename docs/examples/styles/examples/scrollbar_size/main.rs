/// Port of Python Textual `docs/examples/styles/scrollbar_size.py`.
///
/// Demonstrates `scrollbar-size` CSS property on a ScrollableContainer.
/// Note: `scrollbar-size` and `color: blue 80%` (color with alpha) are CSS
/// properties that may not be fully implemented in textual-rs yet
/// (framework-gap flags).
use textual::prelude::*;

const TEXT: &str = "I must not fear.
Fear is the mind-killer.
Fear is the little-death that brings total obliteration.
I will face my fear.
I will permit it to pass over me and through me.
And when it has gone past, I will turn the inner eye to see its path.
Where the fear has gone there will be nothing. Only I will remain.
";

const CSS: &str = r##"
Screen {
    background: white;
    color: blue 80%;
    layout: horizontal;
}

Label {
    padding: 1 2;
    width: 200;
}

.panel {
    scrollbar-size: 10 4;
    padding: 1 2;
}
"##;

struct ScrollbarApp;

impl TextualApp for ScrollbarApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        let repeated_text = TEXT.repeat(5);
        AppRoot::new().with_child(
            ScrollableContainer::new()
                .class("panel")
                .with_child(Label::new(repeated_text)),
        )
    }
}

fn main() -> Result<()> {
    run_sync(ScrollbarApp)
}
