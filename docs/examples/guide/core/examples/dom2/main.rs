/// Port of Python Textual `docs/examples/guide/dom2.py`.
///
/// Demonstrates a minimal app composed of Header and Footer widgets.
use textual::prelude::*;

struct ExampleApp;

impl TextualApp for ExampleApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Header::new())
            .with_child(Footer::new())
    }
}

fn main() -> Result<()> {
    run_sync(ExampleApp)
}
