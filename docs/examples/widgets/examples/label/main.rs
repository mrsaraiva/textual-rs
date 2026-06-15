/// Port of Python Textual `docs/examples/widgets/label.py`.
///
/// Demonstrates the `Label` widget with a simple "Hello, world!" message.
use textual::prelude::*;

struct LabelApp;

impl TextualApp for LabelApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Label::new("Hello, world!"))
    }
}

fn main() -> Result<()> {
    run_sync(LabelApp)
}
