/// Port of Python Textual `docs/examples/styles/background.py`.
///
/// Demonstrates background color with various CSS color formats.
use textual::prelude::*;

const CSS: &str = r##"
Label {
    width: 100%;
    height: 1fr;
    content-align: center middle;
    color: white;
}

#static1 {
    background: red;
}

#static2 {
    background: rgb(0, 255, 0);
}

#static3 {
    background: hsl(240, 100%, 50%);
}
"##;

struct BackgroundApp;

impl TextualApp for BackgroundApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Label::new("Widget 1").id("static1"))
            .with_child(Label::new("Widget 2").id("static2"))
            .with_child(Label::new("Widget 3").id("static3"))
    }
}

fn main() -> Result<()> {
    run_sync(BackgroundApp)
}
