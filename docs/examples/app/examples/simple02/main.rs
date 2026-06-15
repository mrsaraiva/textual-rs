/// Port of Python Textual `docs/examples/app/simple02.py`.
///
/// A minimal Textual app with no widgets, no title override, and no CSS.
/// Mirrors `MyApp(App): pass` — the bare default application.
use textual::prelude::*;

struct MyApp;

impl TextualApp for MyApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
    }
}

fn main() -> Result<()> {
    run_sync(MyApp)
}
