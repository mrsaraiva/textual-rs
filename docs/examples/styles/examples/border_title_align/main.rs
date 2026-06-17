/// Port of Python Textual `docs/examples/styles/border_title_align.py`.
///
/// Demonstrates `border-title-align` CSS property with three Labels whose
/// border titles are left-, center-, and right-aligned.
use textual::prelude::*;

const CSS: &str = r##"
#label1 {
    border: solid $secondary;
    border-title-align: left;
}

#label2 {
    border: dashed $secondary;
    border-title-align: center;
}

#label3 {
    border: tall $secondary;
    border-title-align: right;
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

struct BorderTitleAlignApp;

impl TextualApp for BorderTitleAlignApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(
                Label::new("My title is on the left.")
                    .with_id("label1")
                    .with_border_title("< Left"),
            )
            .with_child(
                Label::new("My title is centered")
                    .with_id("label2")
                    .with_border_title("Centered!"),
            )
            .with_child(
                Label::new("My title is on the right")
                    .with_id("label3")
                    .with_border_title("Right >"),
            )
    }
}

fn main() -> Result<()> {
    run_sync(BorderTitleAlignApp)
}
