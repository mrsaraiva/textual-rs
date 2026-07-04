/// Port of Python Textual `docs/examples/how-to/layout02.py`.
///
/// Demonstrates docked Placeholder widgets acting as Header and Footer:
/// - `Header` is a Placeholder docked to the top with height 3.
/// - `Footer` is a Placeholder docked to the bottom with height 3.
///
/// Python source defines `Header(Placeholder)` and `Footer(Placeholder)` as
/// subclasses with `DEFAULT_CSS` that sets `dock: top` / `dock: bottom`.
/// In Rust we achieve the same layout using CSS id selectors.
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
"#;

struct LayoutApp;

impl TextualApp for LayoutApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        // Python: Header(id="Header") — Placeholder shows "#Header" (no label → falls
        // back to f"#{id}"). Pass "#Header" explicitly to match Python's display text.
        AppRoot::new()
            .with_child(Placeholder::new("#Header").id("Header"))
            .with_child(Placeholder::new("#Footer").id("Footer"))
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
