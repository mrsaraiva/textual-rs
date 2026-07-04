/// Port of Python Textual `docs/examples/guide/layout/utility_containers.py`.
///
/// Demonstrates using Horizontal and Vertical containers with column classes.
use textual::prelude::*;

const CSS: &str = r##"
Static {
    content-align: center middle;
    background: crimson;
    border: solid darkred;
    height: 1fr;
}

.column {
    width: 1fr;
}
"##;

struct UtilityContainersExample;

impl TextualApp for UtilityContainersExample {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(
            Horizontal::new()
                .with_child(
                    
                        Vertical::new()
                            .with_child(Static::new("One"))
                            .with_child(Static::new("Two"))
                    .class("column"),
                )
                .with_child(
                    
                        Vertical::new()
                            .with_child(Static::new("Three"))
                            .with_child(Static::new("Four"))
                    .class("column"),
                ),
        )
    }
}

fn main() -> Result<()> {
    run_sync(UtilityContainersExample)
}
