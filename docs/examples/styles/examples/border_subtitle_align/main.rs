/// Port of Python Textual `docs/examples/styles/border_subtitle_align.py`.
///
/// Demonstrates `border-subtitle-align` with left, center, and right
/// alignment variants. Three Label widgets each have a border and a subtitle
/// text aligned differently.
use textual::prelude::*;

const CSS: &str = r##"
#label1 {
    border: solid $secondary;
    border-subtitle-align: left;
}

#label2 {
    border: dashed $secondary;
    border-subtitle-align: center;
}

#label3 {
    border: tall $secondary;
    border-subtitle-align: right;
}

Screen > Label {
    width: 100%;
    height: 5;
    content-align: center middle;
    color: white;
    margin: 1;
    box-sizing: border-box;
}
"##;

struct BorderSubtitleAlignApp;

impl TextualApp for BorderSubtitleAlignApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(
                Label::new("My subtitle is on the left.")
                    .with_id("label1")
                    .with_border_subtitle("< Left"),
            )
            .with_child(
                Label::new("My subtitle is centered")
                    .with_id("label2")
                    .with_border_subtitle("Centered!"),
            )
            .with_child(
                Label::new("My subtitle is on the right")
                    .with_id("label3")
                    .with_border_subtitle("Right >"),
            )
    }
}

fn main() -> Result<()> {
    run_sync(BorderSubtitleAlignApp)
}
