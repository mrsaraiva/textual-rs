/// Port of Python Textual `docs/examples/styles/dock_all.py`.
///
/// Demonstrates docking containers to all four sides of the screen.
use textual::prelude::*;

const CSS: &str = r##"
#left {
    dock: left;
    height: 100%;
    width: auto;
    align-vertical: middle;
}
#top {
    dock: top;
    height: auto;
    width: 100%;
    align-horizontal: center;
}
#right {
    dock: right;
    height: 100%;
    width: auto;
    align-vertical: middle;
}
#bottom {
    dock: bottom;
    height: auto;
    width: 100%;
    align-horizontal: center;
}

Screen {
    align: center middle;
}

#big_container {
    width: 75%;
    height: 75%;
    border: round white;
}
"##;

struct DockAllApp;

impl TextualApp for DockAllApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(
            Container::new()
                .id("big_container")
                .with_child(Container::new().id("left").with_child(Label::new("left")))
                .with_child(Container::new().id("top").with_child(Label::new("top")))
                .with_child(Container::new().id("right").with_child(Label::new("right")))
                .with_child(Container::new().id("bottom").with_child(Label::new("bottom"))),
        )
    }
}

fn main() -> Result<()> {
    run_sync(DockAllApp)
}
