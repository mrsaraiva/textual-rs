use textual::compose;
use textual::prelude::*;

/// Mirrors Python Textual's `docs/examples/widgets/input_types.py`.
struct InputTypesApp;

impl TextualApp for InputTypesApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Container::new().with_compose(compose![
                Input::new()
                    .with_placeholder("An integer")
                    .with_type(InputType::Integer),
                Input::new()
                    .with_placeholder("A number")
                    .with_type(InputType::Number),
            ]))
    }
}

fn main() -> Result<()> {
    run_sync(InputTypesApp)
}

#[cfg(test)]
mod liveness {
    use super::*;
    use textual::run_test;

    /// LIVENESS: the first (Integer) Input auto-focuses; typing digits echoes
    /// them into the field (replacing the placeholder), changing the rendered
    /// frame. Proves the typed-Input character-entry path is wired.
    #[test]
    fn typing_echoes_into_typed_input() {
        run_test(InputTypesApp, |pilot| {
            let before = pilot.app().frame_fingerprint();
            pilot.press(&["4", "2"])?;
            let after = pilot.app().frame_fingerprint();
            assert_ne!(
                before, after,
                "typing digits into the focused Integer Input must echo and change the frame"
            );
            Ok(())
        })
        .unwrap();
    }
}
