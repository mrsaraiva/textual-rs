/// Port of Python Textual `docs/examples/styles/background_transparency.py`.
///
/// Demonstrates different background transparency (alpha) settings using
/// `background: red N%` across ten Static widgets.
use textual::prelude::*;

const CSS: &str = r##"
#t10 {
    background: red 10%;
}

#t20 {
    background: red 20%;
}

#t30 {
    background: red 30%;
}

#t40 {
    background: red 40%;
}

#t50 {
    background: red 50%;
}

#t60 {
    background: red 60%;
}

#t70 {
    background: red 70%;
}

#t80 {
    background: red 80%;
}

#t90 {
    background: red 90%;
}

#t100 {
    background: red 100%;
}

Screen {
    layout: horizontal;
}

Static {
    height: 100%;
    width: 1fr;
    content-align: center middle;
}
"##;

struct BackgroundTransparencyApp;

impl TextualApp for BackgroundTransparencyApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Static::new("10%").id("t10"))
            .with_child(Static::new("20%").id("t20"))
            .with_child(Static::new("30%").id("t30"))
            .with_child(Static::new("40%").id("t40"))
            .with_child(Static::new("50%").id("t50"))
            .with_child(Static::new("60%").id("t60"))
            .with_child(Static::new("70%").id("t70"))
            .with_child(Static::new("80%").id("t80"))
            .with_child(Static::new("90%").id("t90"))
            .with_child(Static::new("100%").id("t100"))
    }
}

fn main() -> Result<()> {
    run_sync(BackgroundTransparencyApp)
}
