/// Port of Python Textual `docs/examples/guide/styles/margin01.py`.
///
/// Demonstrates setting margin, border, and background via inline CSS on Static widgets.
use textual::prelude::*;

const TEXT: &str = "I must not fear.
Fear is the mind-killer.
Fear is the little-death that brings total obliteration.
I will face my fear.
I will permit it to pass over me and through me.
And when it has gone past, I will turn the inner eye to see its path.
Where the fear has gone there will be nothing. Only I will remain.";

const CSS: &str = r##"
#widget1 {
    background: purple;
    border: heavy white;
    margin: 2;
}

#widget2 {
    background: darkgreen;
    border: heavy white;
    margin: 2;
}
"##;

struct MarginApp;

impl TextualApp for MarginApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Static::new(TEXT).id("widget1"))
            .with_child(Static::new(TEXT).id("widget2"))
    }
}

fn main() -> Result<()> {
    run_sync(MarginApp)
}
