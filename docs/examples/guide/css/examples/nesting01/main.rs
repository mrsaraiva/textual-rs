/// Port of Python Textual `docs/examples/guide/css/nesting01.py`.
///
/// Demonstrates CSS descendant selectors: a Horizontal container (#questions)
/// with two Static "button" widgets, one affirmative and one negative.
use textual::prelude::*;

const CSS: &str = r##"
/* Style the container */
#questions {
    border: heavy $primary;
    align: center middle;
}

/* Style all buttons */
#questions .button {
    width: 1fr;
    padding: 1 2;
    margin: 1 2;
    text-align: center;
    border: heavy $panel;
}

/* Style the Yes button */
#questions .button.affirmative {
    border: heavy $success;
}

/* Style the No button */
#questions .button.negative {
    border: heavy $error;
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
