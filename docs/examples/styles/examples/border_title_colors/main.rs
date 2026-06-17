/// Port of Python Textual `docs/examples/styles/border_title_colors.py`.
///
/// Demonstrates border-title-color, border-title-background, border-title-style,
/// border-subtitle-color, border-subtitle-background, and border-subtitle-style CSS
/// properties on a Label.
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
        AppRoot::new().with_child(
            Label::new("Hello, World!")
                .with_border_title("Textual Rocks")
                .with_border_subtitle("Textual Rocks"),
        )
    }
}

fn main() -> Result<()> {
    run_sync(BorderTitleApp)
}
