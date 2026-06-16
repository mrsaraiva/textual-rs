/// Port of Python Textual `docs/examples/styles/text_style.py`.
///
/// Demonstrates the text-style CSS property (bold, italic, reverse) on three
/// Label widgets arranged in a horizontal layout.
use textual::prelude::*;

const TEXT: &str = "I must not fear.
Fear is the mind-killer.
Fear is the little-death that brings total obliteration.
I will face my fear.
I will permit it to pass over me and through me.
And when it has gone past, I will turn the inner eye to see its path.
Where the fear has gone there will be nothing. Only I will remain.";

const CSS: &str = r##"
Screen {
    layout: horizontal;
}
Label {
    width: 1fr;
}
#lbl1 {
    background: red 30%;
    text-style: bold;
}
#lbl2 {
    background: green 30%;
    text-style: italic;
}
#lbl3 {
    background: blue 30%;
    text-style: reverse;
}
"##;

struct TextStyleApp;

impl TextualApp for TextStyleApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Label::new(TEXT).id("lbl1"))
            .with_child(Label::new(TEXT).id("lbl2"))
            .with_child(Label::new(TEXT).id("lbl3"))
    }
}

fn main() -> Result<()> {
    run_sync(TextStyleApp)
}
