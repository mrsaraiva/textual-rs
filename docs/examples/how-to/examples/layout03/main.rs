/// Port of Python Textual `docs/examples/how-to/layout03.py`.
///
/// Demonstrates docked header/footer placeholders with a central columns
/// container.  Mirrors the Python layout:
///
///   - `Header(Placeholder)` — docked to top, height 3
///   - `Footer(Placeholder)` — docked to bottom, height 3
///   - `ColumnsContainer(Placeholder)` — fills remaining space with a solid
///     white border
///
/// Python source uses DEFAULT_CSS on each subclass.  Here those styles are
/// expressed as CSS id-selectors (matching the Python compose IDs "Header",
/// "Footer", "Columns") loaded via `app.load_stylesheet`.
use textual::prelude::*;

const CSS: &str = r#"
#Header {
    height: 3;
    dock: top;
}

#Footer {
    height: 3;
    dock: bottom;
}

#Columns {
    width: 1fr;
    height: 1fr;
    border: solid white;
}
"#;

struct LayoutApp;

impl TextualApp for LayoutApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        let header = Placeholder::new("#Header").id("Header");
        let footer = Placeholder::new("#Footer").id("Footer");
        let columns = Placeholder::new("#Columns").id("Columns");

        AppRoot::new()
            .with_child(header)
            .with_child(footer)
            .with_child(columns)
    }
}

fn main() -> textual::Result<()> {
    run_sync(LayoutApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_app_composes_without_panic() {
        let mut app = LayoutApp;
        let _root = app.compose();
    }
}
