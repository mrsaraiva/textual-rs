/// Port of Python Textual `docs/examples/styles/background_tint.py`.
///
/// Demonstrates the `background-tint` CSS property at 0%, 25%, 50%, 75%, and 100%.
/// Five Vertical containers (tint1–tint5) each contain a Label showing the tint level.
///
/// Framework gap: `background-tint` property may not be fully supported in textual-rs;
/// included verbatim per port rules.
use textual::prelude::*;

const CSS: &str = r##"
Vertical {
    background: $panel;
    color: auto 90%;
}
#tint1 { background-tint: $foreground 0%; }
#tint2 { background-tint: $foreground 25%; }
#tint3 { background-tint: $foreground 50%; }
#tint4 { background-tint: $foreground 75% }
#tint5 { background-tint: $foreground 100% }
"##;

struct BackgroundTintApp;

impl TextualApp for BackgroundTintApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Node::new(Vertical::new().with_child(Label::new("0%"))).id("tint1"))
            .with_child(Node::new(Vertical::new().with_child(Label::new("25%"))).id("tint2"))
            .with_child(Node::new(Vertical::new().with_child(Label::new("50%"))).id("tint3"))
            .with_child(Node::new(Vertical::new().with_child(Label::new("75%"))).id("tint4"))
            .with_child(Node::new(Vertical::new().with_child(Label::new("100%"))).id("tint5"))
    }
}

fn main() -> Result<()> {
    run_sync(BackgroundTintApp)
}
