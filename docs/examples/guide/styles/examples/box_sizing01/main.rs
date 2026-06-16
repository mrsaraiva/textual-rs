/// Port of Python Textual `docs/examples/guide/styles/box_sizing01.py`.
///
/// Demonstrates box-sizing: two Static widgets with identical CSS except that
/// widget2 uses `box-sizing: content-box`. Styles are applied via inline CSS.
/// Framework gap: on_mount style mutation is not supported; styles baked into CSS.
use textual::prelude::*;

const TEXT: &str = "I must not fear.\nFear is the mind-killer.\nFear is the little-death that brings total obliteration.\nI will face my fear.\nI will permit it to pass over me and through me.\nAnd when it has gone past, I will turn the inner eye to see its path.\nWhere the fear has gone there will be nothing. Only I will remain.";

const CSS: &str = r##"
#widget1 {
    background: purple;
    width: 30;
    height: 6;
    border: heavy white;
    padding: 1;
}

#widget2 {
    background: darkgreen;
    width: 30;
    height: 6;
    border: heavy white;
    padding: 1;
    box-sizing: content-box;
}
"##;

struct BoxSizing;

impl TextualApp for BoxSizing {
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
    run_sync(BoxSizing)
}
