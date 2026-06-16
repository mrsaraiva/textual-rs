/// Port of Python Textual `docs/examples/styles/align_all.py`.
///
/// Demonstrates all 9 align combinations in a 3x3 grid layout.
/// Each Container uses a different `align` value with a Label child.
use textual::prelude::*;

const CSS: &str = r##"
#left-top {
    /* align: left top; this is the default value and is implied. */
}

#center-top {
    align: center top;
}

#right-top {
    align: right top;
}

#left-middle {
    align: left middle;
}

#center-middle {
    align: center middle;
}

#right-middle {
    align: right middle;
}

#left-bottom {
    align: left bottom;
}

#center-bottom {
    align: center bottom;
}

#right-bottom {
    align: right bottom;
}

Screen {
    layout: grid;
    grid-size: 3 3;
    grid-gutter: 1;
}

Container {
    background: $boost;
    border: solid gray;
    height: 100%;
}

Label {
    width: auto;
    height: 1;
    background: $accent;
}
"##;

struct AlignAllApp;

impl TextualApp for AlignAllApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Container::new().id("left-top").with_child(Label::new("left top")))
            .with_child(Container::new().id("center-top").with_child(Label::new("center top")))
            .with_child(Container::new().id("right-top").with_child(Label::new("right top")))
            .with_child(Container::new().id("left-middle").with_child(Label::new("left middle")))
            .with_child(Container::new().id("center-middle").with_child(Label::new("center middle")))
            .with_child(Container::new().id("right-middle").with_child(Label::new("right middle")))
            .with_child(Container::new().id("left-bottom").with_child(Label::new("left bottom")))
            .with_child(Container::new().id("center-bottom").with_child(Label::new("center bottom")))
            .with_child(Container::new().id("right-bottom").with_child(Label::new("right bottom")))
    }
}

fn main() -> Result<()> {
    run_sync(AlignAllApp)
}
