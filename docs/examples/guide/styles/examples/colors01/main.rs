/// Port of Python Textual `docs/examples/guide/styles/colors01.py`.
///
/// Demonstrates setting background and foreground colors on widgets.
/// Python uses on_mount to set styles imperatively; here we use CSS id rules
/// to reproduce the same colors (equivalent visual result).
/// Colors: #9932CC (purple), hsl(150,42.9%,49.4%) green, rgb(191,78,96) rose.
use textual::prelude::*;

const CSS: &str = r##"
#widget1 {
    background: #9932CC;
}

#widget2 {
    background: hsl(150, 42.9%, 49.4%);
    color: blue;
}

#widget3 {
    background: rgb(191, 78, 96);
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
            .with_child(Static::new("Textual One").id("widget1"))
            .with_child(Static::new("Textual Two").id("widget2"))
            .with_child(Static::new("Textual Three").id("widget3"))
    }
}

fn main() -> Result<()> {
    run_sync(ColorApp)
}
