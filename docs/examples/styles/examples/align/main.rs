/// Port of Python Textual `docs/examples/styles/align.py`.
///
/// Demonstrates the `align` CSS property: two Label boxes centered on the screen.
use textual::prelude::*;

const CSS: &str = r##"
Screen {
    align: center middle;
}

.box {
    width: 40;
    height: 5;
    margin: 1;
    padding: 1;
    background: green;
    color: white 90%;
    border: heavy white;
}
"##;

struct AlignApp;

impl TextualApp for AlignApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Label::new("Vertical alignment with [b]Textual[/]").class("box"))
            .with_child(Label::new("Take note, browsers.").class("box"))
    }
}

fn main() -> Result<()> {
    run_sync(AlignApp)
}
