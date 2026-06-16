/// Port of Python Textual `docs/examples/guide/layout/dock_layout1_sidebar.py`.
///
/// Demonstrates docking a sidebar widget to the left edge of the screen.
use textual::prelude::*;

const TEXT: &str = "Docking a widget removes it from the layout and fixes its position, aligned to either the top, right, bottom, or left edges of a container.

Docked widgets will not scroll out of view, making them ideal for sticky headers, footers, and sidebars.

";

const CSS: &str = r##"
#sidebar {
    dock: left;
    width: 15;
    height: 100%;
    color: #0f2b41;
    background: dodgerblue;
}
"##;

struct DockLayoutExample;

impl TextualApp for DockLayoutExample {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        let text_repeated = TEXT.repeat(10);
        AppRoot::new()
            .with_child(Static::new("Sidebar").id("sidebar"))
            .with_child(Static::new(text_repeated).id("body"))
    }
}

fn main() -> Result<()> {
    run_sync(DockLayoutExample)
}
