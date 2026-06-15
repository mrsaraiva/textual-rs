/// Port of Python Textual `docs/examples/how-to/layout04.py`.
///
/// Demonstrates docked header/footer placeholders with a `HorizontalScroll`
/// body container. Python defines `Header(Placeholder)` and `Footer(Placeholder)`
/// as custom subclasses with `DEFAULT_CSS` that sets `height: 3` and `dock`.
/// Rust ports these as CSS rules targeting the placeholder ids.
use textual::prelude::*;

const CSS: &str = r#"
#Header {
    height: 3;
    dock: top;
    width: 1fr;
}

#Footer {
    height: 3;
    dock: bottom;
    width: 1fr;
}
"#;

struct LayoutApp;

impl TextualApp for LayoutApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        // Python uses `Placeholder(id="Header")` which renders the label as
        // "#Header" (the widget's CSS id). Mirror that by passing "#Header"
        // and "#Footer" as explicit labels.
        AppRoot::new()
            .with_child(Node::new(Placeholder::new("#Header")).id("Header"))
            .with_child(Node::new(Placeholder::new("#Footer")).id("Footer"))
            .with_child(HorizontalScroll::new())
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
