/// Port of Python Textual `docs/examples/styles/tint.py`.
///
/// Demonstrates the `tint` CSS property with green tint at 0%–100%
/// in steps of 10%, each on a separate Label.
///
/// Python sets tint via `widget.styles.tint = color.with_alpha(...)` in a loop.
/// Here we use CSS id selectors for each label to achieve the same result.
///
/// Framework gap: `tint` CSS property rendering may differ from Python
/// until full compositing is implemented.
use textual::prelude::*;

const CSS: &str = r##"
Label {
    height: 3;
    width: 100%;
    text-style: bold;
    background: white;
    color: black;
    content-align: center middle;
}

#tint0  { tint: green 0%; }
#tint10 { tint: green 10%; }
#tint20 { tint: green 20%; }
#tint30 { tint: green 30%; }
#tint40 { tint: green 40%; }
#tint50 { tint: green 50%; }
#tint60 { tint: green 60%; }
#tint70 { tint: green 70%; }
#tint80 { tint: green 80%; }
#tint90 { tint: green 90%; }
#tint100 { tint: green 100%; }
"##;

struct TintApp;

impl TextualApp for TintApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Label::new("tint: green 0%;").with_id("tint0"))
            .with_child(Label::new("tint: green 10%;").with_id("tint10"))
            .with_child(Label::new("tint: green 20%;").with_id("tint20"))
            .with_child(Label::new("tint: green 30%;").with_id("tint30"))
            .with_child(Label::new("tint: green 40%;").with_id("tint40"))
            .with_child(Label::new("tint: green 50%;").with_id("tint50"))
            .with_child(Label::new("tint: green 60%;").with_id("tint60"))
            .with_child(Label::new("tint: green 70%;").with_id("tint70"))
            .with_child(Label::new("tint: green 80%;").with_id("tint80"))
            .with_child(Label::new("tint: green 90%;").with_id("tint90"))
            .with_child(Label::new("tint: green 100%;").with_id("tint100"))
    }
}

fn main() -> Result<()> {
    run_sync(TintApp)
}
