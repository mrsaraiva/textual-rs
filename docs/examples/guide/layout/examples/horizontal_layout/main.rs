/// Port of Python Textual `docs/examples/guide/layout/horizontal_layout.py`.
///
/// Demonstrates `layout: horizontal` on the Screen with three `.box` Static widgets
/// each taking equal width via `1fr`.
use textual::prelude::*;

const CSS: &str = r##"
Screen {
    layout: horizontal;
}

.box {
    height: 100%;
    width: 1fr;
    border: solid green;
}
"##;

struct HorizontalLayoutExample;

impl TextualApp for HorizontalLayoutExample {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Static::new("One").class("box"))
            .with_child(Static::new("Two").class("box"))
            .with_child(Static::new("Three").class("box"))
    }
}

fn main() -> Result<()> {
    run_sync(HorizontalLayoutExample)
}
