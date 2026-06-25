/// Port of Python Textual `docs/examples/widgets/masked_input.py`.
///
/// Demonstrates the `MaskedInput` widget for credit card number entry.
use textual::prelude::*;

const CSS: &str = r#"
MaskedInput.-valid {
    border: tall $success 60%;
}
MaskedInput.-valid:focus {
    border: tall $success;
}
MaskedInput {
    margin: 1 1;
}
Label {
    margin: 1 2;
}
"#;

struct MaskedInputApp;

impl TextualApp for MaskedInputApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_compose(vec![
            ChildDecl::from(Label::new("Enter a valid credit card number.")),
            ChildDecl::from(MaskedInput::new("9999-9999-9999-9999;0")),
        ])
    }
}

fn main() -> textual::Result<()> {
    run_sync(MaskedInputApp)
}

#[cfg(test)]
mod liveness {
    use super::*;
    use textual::run_test;

    /// LIVENESS: the MaskedInput auto-focuses (it is the only focusable widget);
    /// typing digits echoes them through the mask ("9999-9999-..."), changing
    /// the rendered frame. Proves the MaskedInput character-entry path is wired.
    #[test]
    fn typing_digits_echoes_through_mask() {
        run_test(MaskedInputApp, |pilot| {
            // Ensure the MaskedInput is focused (first focusable after the Label).
            pilot.press(&["tab"])?;
            let before = pilot.app().frame_fingerprint();
            pilot.press(&["1", "2", "3", "4"])?;
            let after = pilot.app().frame_fingerprint();
            assert_ne!(
                before, after,
                "typing digits into the MaskedInput must echo through the mask and change the frame"
            );
            Ok(())
        })
        .unwrap();
    }
}
