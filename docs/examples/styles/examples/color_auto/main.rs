/// Port of Python Textual `docs/examples/styles/color_auto.py`.
///
/// Demonstrates `color: auto 80%` which automatically picks a contrasting
/// foreground color based on the background.
use textual::prelude::*;

const CSS: &str = r##"
Label {
    color: auto 80%;
    content-align: center middle;
    height: 1fr;
    width: 100%;
}

#lbl1 {
    background: red 80%;
}

#lbl2 {
    background: yellow 80%;
}

#lbl3 {
    background: blue 80%;
}

#lbl4 {
    background: pink 80%;
}

#lbl5 {
    background: green 80%;
}
"##;

struct ColorApp;

impl TextualApp for ColorApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Label::new("The quick brown fox jumps over the lazy dog!").with_id("lbl1"))
            .with_child(Label::new("The quick brown fox jumps over the lazy dog!").with_id("lbl2"))
            .with_child(Label::new("The quick brown fox jumps over the lazy dog!").with_id("lbl3"))
            .with_child(Label::new("The quick brown fox jumps over the lazy dog!").with_id("lbl4"))
            .with_child(Label::new("The quick brown fox jumps over the lazy dog!").with_id("lbl5"))
    }
}

fn main() -> Result<()> {
    run_sync(ColorApp)
}
