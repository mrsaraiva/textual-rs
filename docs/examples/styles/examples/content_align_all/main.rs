/// Port of Python Textual `docs/examples/styles/content_align_all.py`.
///
/// Demonstrates all 9 combinations of content-align (horizontal x vertical)
/// arranged in a 3x3 grid layout.
use textual::prelude::*;

const CSS: &str = r##"
#left-top {
    /* content-align: left top; this is the default implied value. */
}
#center-top {
    content-align: center top;
}
#right-top {
    content-align: right top;
}
#left-middle {
    content-align: left middle;
}
#center-middle {
    content-align: center middle;
}
#right-middle {
    content-align: right middle;
}
#left-bottom {
    content-align: left bottom;
}
#center-bottom {
    content-align: center bottom;
}
#right-bottom {
    content-align: right bottom;
}

Screen {
    layout: grid;
    grid-size: 3 3;
    grid-gutter: 1;
}

Label {
    width: 100%;
    height: 100%;
    background: $primary;
}
"##;

struct AllContentAlignApp;

impl TextualApp for AllContentAlignApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Label::new("left top").id("left-top"))
            .with_child(Label::new("center top").id("center-top"))
            .with_child(Label::new("right top").id("right-top"))
            .with_child(Label::new("left middle").id("left-middle"))
            .with_child(Label::new("center middle").id("center-middle"))
            .with_child(Label::new("right middle").id("right-middle"))
            .with_child(Label::new("left bottom").id("left-bottom"))
            .with_child(Label::new("center bottom").id("center-bottom"))
            .with_child(Label::new("right bottom").id("right-bottom"))
    }
}

fn main() -> Result<()> {
    run_sync(AllContentAlignApp)
}
