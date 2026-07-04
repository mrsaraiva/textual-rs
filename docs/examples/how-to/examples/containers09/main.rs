/// Port of Python Textual `docs/examples/how-to/containers09.py`.
///
/// Demonstrates a `Middle` container holding three fixed-size placeholder
/// widgets (`Box` in Python, using `Placeholder` here) with a heavy green
/// border applied via the `.with-border` CSS class.
///
/// Python layout:
///   class Box(Placeholder):
///       DEFAULT_CSS = "Box { width: 16; height: 5; }"
///
///   class ContainerApp(App):
///       CSS = ".with-border { border: heavy green; }"
///
///       def compose(self):
///           with Middle(classes="with-border"):
///               yield Box("Box 1.")
///               yield Box("Box 2.")
///               yield Box("Box 3.")
use textual::prelude::*;

const CSS: &str = r#"
.box {
    width: 16;
    height: 5;
}

.with-border {
    border: heavy green;
}
"#;

struct ContainerApp;

impl TextualApp for ContainerApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        let middle = 
            Middle::new()
                .with_child(Placeholder::new("Box 1.").class("box"))
                .with_child(Placeholder::new("Box 2.").class("box"))
                .with_child(Placeholder::new("Box 3.").class("box"))
        .class("with-border");

        AppRoot::new().with_child(middle)
    }
}

fn main() -> textual::Result<()> {
    run_sync(ContainerApp)
}
