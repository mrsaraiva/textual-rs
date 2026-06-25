use textual::compose;
use textual::prelude::*;

/// Mirrors Python Textual's `docs/examples/widgets/input.py`.
struct InputApp;

impl TextualApp for InputApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Container::new().with_compose(compose![
            Input::new().with_placeholder("First Name"),
            Input::new().with_placeholder("Last Name"),
        ]))
    }
}

fn main() -> Result<()> {
    run_sync(InputApp)
}

#[cfg(test)]
mod liveness {
    use super::*;
    use textual::run_test;

    /// LIVENESS: the first Input auto-focuses; typing characters echoes them
    /// into the field (replacing the placeholder), changing the rendered frame.
    /// Proves the Input character-entry path is wired.
    #[test]
    fn typing_echoes_into_input() {
        run_test(InputApp, |pilot| {
            let before = pilot.app().frame_fingerprint();
            pilot.press(&["h", "e", "l", "l", "o"])?;
            let after = pilot.app().frame_fingerprint();
            assert_ne!(
                before, after,
                "typing into the focused Input must echo characters and change the frame"
            );
            Ok(())
        })
        .unwrap();
    }
}
