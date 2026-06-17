/// Port of Python Textual `docs/examples/guide/dom1.py`.
///
/// Demonstrates the minimal app skeleton — an App with no widgets (empty DOM).
/// The Python source defines `ExampleApp(App): pass` with no CSS or compose body.
use textual::prelude::*;

struct ExampleApp;

impl TextualApp for ExampleApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
    }
}

fn main() -> Result<()> {
    run_sync(ExampleApp)
}
