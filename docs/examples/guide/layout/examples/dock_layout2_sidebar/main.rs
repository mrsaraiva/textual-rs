/// Port of Python Textual `docs/examples/guide/layout/dock_layout2_sidebar.py`.
///
/// Demonstrates docking: two sidebars docked to the left edge with different
/// widths and background colors, plus a scrollable body of text.
use textual::prelude::*;

const CSS: &str = r##"
#another-sidebar {
    dock: left;
    width: 30;
    height: 100%;
    background: deeppink;
}

#sidebar {
    dock: left;
    width: 15;
    height: 100%;
    color: #0f2b41;
    background: dodgerblue;
}
"##;

const TEXT: &str = "Docking a widget removes it from the layout and fixes its position, aligned to either the top, right, bottom, or left edges of a container.\n\nDocked widgets will not scroll out of view, making them ideal for sticky headers, footers, and sidebars.\n\n";

struct DockLayoutExample;

impl TextualApp for DockLayoutExample {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        let body_text = TEXT.repeat(10);
        AppRoot::new()
            .with_child(Static::new("Sidebar2").id("another-sidebar"))
            .with_child(Static::new("Sidebar1").id("sidebar"))
            .with_child(Static::new(body_text).id("body"))
    }
}

fn main() -> Result<()> {
    run_sync(DockLayoutExample)
}
