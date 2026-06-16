/// Port of Python Textual `docs/examples/styles/content_align.py`.
///
/// Demonstrates `content-align`, `content-align-horizontal`, and
/// `content-align-vertical` CSS properties on Label widgets.
use textual::prelude::*;

const CSS: &str = r##"
#box1 {
    content-align: left top;
    background: red;
}

#box2 {
    content-align-horizontal: center;
    content-align-vertical: middle;
    background: green;
}

#box3 {
    content-align: right bottom;
    background: blue;
}

Label {
    width: 100%;
    height: 1fr;
    padding: 1;
    color: white;
}
"##;

struct ContentAlignApp;

impl TextualApp for ContentAlignApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Label::new("With [i]content-align[/] you can...").id("box1"))
            .with_child(Label::new("...[b]Easily align content[/]...").id("box2"))
            .with_child(Label::new("...Horizontally [i]and[/] vertically!").id("box3"))
    }
}

fn main() -> Result<()> {
    run_sync(ContentAlignApp)
}
