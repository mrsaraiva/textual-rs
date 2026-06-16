/// Port of Python Textual `docs/examples/guide/styles/dimensions04.py`.
///
/// Demonstrates fractional height sizing: two Static widgets where widget1
/// gets 2fr of height and widget2 gets 1fr, with different background colors.
///
/// Note: Python sets these styles via on_mount (runtime style mutation).
/// Rust uses inline CSS since runtime style mutation via on_mount is not yet
/// supported in textual-rs.
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
    height: 2fr;
}

#widget2 {
    background: darkgreen;
    height: 1fr;
}
"##;

struct DimensionsApp;

impl TextualApp for DimensionsApp {
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
    run_sync(DimensionsApp)
}
