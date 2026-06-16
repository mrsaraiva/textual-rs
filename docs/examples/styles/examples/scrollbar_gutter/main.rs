/// Port of Python Textual `docs/examples/styles/scrollbar_gutter.py`.
///
/// Demonstrates `scrollbar-gutter: stable` which reserves space for the
/// scrollbar gutter even when the scrollbar is not visible.
///
/// Note: `scrollbar-gutter` CSS property may be a framework gap in textual-rs.
use textual::prelude::*;

const CSS: &str = r##"
Screen {
    scrollbar-gutter: stable;
}

#text-box {
    color: floralwhite;
    background: darkmagenta;
}
"##;

const TEXT: &str = "I must not fear.
Fear is the mind-killer.
Fear is the little-death that brings total obliteration.
I will face my fear.
I will permit it to pass over me and through me.
And when it has gone past, I will turn the inner eye to see its path.
Where the fear has gone there will be nothing. Only I will remain.";

struct ScrollbarGutterApp;

impl TextualApp for ScrollbarGutterApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Static::new(TEXT).id("text-box"))
    }
}

fn main() -> Result<()> {
    run_sync(ScrollbarGutterApp)
}
