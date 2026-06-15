/// Port of Python Textual `docs/examples/widgets/static.py`.
///
/// Demonstrates the `Static` widget with simple text content.
use textual::prelude::*;

struct StaticApp;

impl TextualApp for StaticApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Static::new("Hello, world!"))
    }
}

fn main() -> Result<()> {
    run_sync(StaticApp)
}
