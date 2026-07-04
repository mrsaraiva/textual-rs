/// Port of Python Textual `docs/examples/guide/css/nesting02.py`.
///
/// Demonstrates nested CSS with `&.class` selectors inside a parent rule.
/// A Horizontal container with id `questions` holds two Static widgets
/// with classes `button affirmative` and `button negative`.
use textual::prelude::*;

const CSS: &str = r##"
/* Style the container */
#questions {
    border: heavy $primary;
    align: center middle;

    /* Style all buttons */
    .button {
        width: 1fr;
        padding: 1 2;
        margin: 1 2;
        text-align: center;
        border: heavy $panel;

        /* Style the Yes button */
        &.affirmative {
            border: heavy $success;
        }

        /* Style the No button */
        &.negative {
            border: heavy $error;
        }
    }
}
"##;

struct NestingDemo;

impl TextualApp for NestingDemo {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(
            
                Horizontal::new()
                    .with_child(Static::new("Yes").class("button").class("affirmative"))
                    .with_child(Static::new("No").class("button").class("negative"))
            .id("questions"),
        )
    }
}

fn main() -> Result<()> {
    run_sync(NestingDemo)
}
