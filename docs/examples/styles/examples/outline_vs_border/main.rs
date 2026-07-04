/// Port of Python Textual `docs/examples/styles/outline_vs_border.py`.
///
/// Demonstrates the difference between `outline` and `border` by showing
/// three Label widgets: one with outline only, one with border only, and
/// one with both.
use textual::prelude::*;

const TEXT: &str = "I must not fear.\n\
Fear is the mind-killer.\n\
Fear is the little-death that brings total obliteration.\n\
I will face my fear.\n\
I will permit it to pass over me and through me.\n\
And when it has gone past, I will turn the inner eye to see its path.\n\
Where the fear has gone there will be nothing. Only I will remain.";

const CSS: &str = r##"
Label {
    height: 8;
}

.outline {
    outline: $error round;
}

.border {
    border: $success heavy;
}
"##;

struct OutlineBorderApp;

impl TextualApp for OutlineBorderApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Label::new(TEXT).class("outline"))
            .with_child(Label::new(TEXT).class("border"))
            .with_child(
                Label::new(TEXT)
                    .class("outline")
                    .class("border"),
            )
    }
}

fn main() -> Result<()> {
    run_sync(OutlineBorderApp)
}
