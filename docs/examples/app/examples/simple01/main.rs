/// Port of Python Textual `docs/examples/app/simple01.py`.
///
/// The Python source is a minimal empty App subclass:
///
///     from textual.app import App
///     class MyApp(App):
///         pass
///
/// This renders the default Textual UI (dark theme, empty content area)
/// with the app title defaulting to the class name "MyApp".
use textual::prelude::*;

struct MyApp;

impl TextualApp for MyApp {
    fn title(&self) -> &'static str {
        "MyApp"
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
    }
}

fn main() -> Result<()> {
    run_sync(MyApp)
}
