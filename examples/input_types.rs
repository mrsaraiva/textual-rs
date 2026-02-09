use textual::prelude::*;

/// Mirrors Python Textual's `docs/examples/widgets/input_types.py`.
struct InputTypesApp;

impl TextualApp for InputTypesApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(
            Container::new()
                .with_child(
                    Input::new()
                        .with_placeholder("An integer")
                        .with_type(InputType::Integer),
                )
                .with_child(
                    Input::new()
                        .with_placeholder("A number")
                        .with_type(InputType::Number),
                ),
        )
    }
}

fn main() -> Result<()> {
    run_sync(InputTypesApp)
}
