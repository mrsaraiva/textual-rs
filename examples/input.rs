use textual::compose;
use textual::prelude::*;

/// Mirrors Python Textual's `docs/examples/widgets/input.py`.
struct InputApp;

impl TextualApp for InputApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(
            Container::new().with_compose(compose![
                Input::new().with_placeholder("First Name"),
                Input::new().with_placeholder("Last Name"),
            ]),
        )
    }
}

fn main() -> Result<()> {
    run_sync(InputApp)
}
