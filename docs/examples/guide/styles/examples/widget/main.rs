/// Port of Python Textual `docs/examples/guide/styles/widget.py`.
///
/// Demonstrates setting background and border styles via on_mount.
/// Note: Python uses on_mount to set styles imperatively; here we approximate
/// with CSS since Rust does not yet expose on_mount style mutation API.
///
/// Framework gap: runtime style mutation (on_mount widget.styles.background /
/// widget.styles.border) not yet supported — CSS approximation used.
use textual::prelude::*;

const CSS: &str = r##"
Static {
    background: darkblue;
    border: heavy white;
}
"##;

struct WidgetApp;

impl TextualApp for WidgetApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Static::new("Textual"))
    }
}

fn main() -> Result<()> {
    run_sync(WidgetApp)
}
