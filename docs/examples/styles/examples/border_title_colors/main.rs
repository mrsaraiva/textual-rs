/// Port of Python Textual `docs/examples/styles/border_title_colors.py`.
///
/// Demonstrates border-title-color, border-title-background, border-title-style,
/// border-subtitle-color, border-subtitle-background, and border-subtitle-style CSS
/// properties on a Label.
///
/// NOTE: Python sets `label.border_title = "Textual Rocks"` and
/// `label.border_subtitle = "Textual Rocks"` in on_mount. Label does not expose
/// `border_title`/`border_subtitle` text setters in Rust yet (framework gap).
/// The CSS styling rules are ported faithfully.
use textual::prelude::*;

const CSS: &str = r##"
Screen {
    align: center middle;
}

Label {
    padding: 4 8;
    border: heavy red;

    border-title-color: green;
    border-title-background: white;
    border-title-style: bold;

    border-subtitle-color: magenta;
    border-subtitle-background: yellow;
    border-subtitle-style: italic;
}
"##;

struct BorderTitleApp;

impl TextualApp for BorderTitleApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Label::new("Hello, World!"))
    }
}

fn main() -> Result<()> {
    run_sync(BorderTitleApp)
}
