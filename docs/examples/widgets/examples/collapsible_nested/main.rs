/// Port of Python Textual `docs/examples/widgets/collapsible_nested.py`.
///
/// Demonstrates nested `Collapsible` widgets:
/// - Outer collapsible is expanded (collapsed=False).
/// - Inner collapsible is collapsed (default).
/// - Inner collapsible contains a Label("Hello, world.").
use textual::prelude::*;

struct CollapsibleApp;

impl TextualApp for CollapsibleApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(
            Collapsible::new("Collapsible")
                .collapsed(false)
                .with_child(
                    Collapsible::new("Collapsible").with_child(Label::new("Hello, world.")),
                ),
        )
    }
}

fn main() -> textual::Result<()> {
    run_sync(CollapsibleApp)
}
