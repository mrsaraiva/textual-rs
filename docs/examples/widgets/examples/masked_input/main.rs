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
