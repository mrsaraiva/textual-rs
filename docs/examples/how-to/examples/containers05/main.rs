/// Port of Python Textual `docs/examples/how-to/containers05.py`.
///
/// Demonstrates `HorizontalGroup` with a CSS class that applies a border.
/// Two `HorizontalGroup` rows, each containing three `Box` placeholders.
/// `Box` is a `Placeholder` subclass with fixed 16×8 dimensions.
use textual::prelude::*;

const CSS: &str = r#"
.with-border {
    border: heavy green;
    height: auto;
    layout: horizontal;
}

.with-border > Placeholder {
    width: 16;
    height: 8;
}
"#;

struct ContainerApp;

impl TextualApp for ContainerApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        let make_box = || Placeholder::new("");

        let mut row1 = Container::new()
            .with_child(make_box())
            .with_child(make_box())
            .with_child(make_box());
        row1.seed_mut().classes.push("with-border".to_string());

        let mut row2 = Container::new()
            .with_child(make_box())
            .with_child(make_box())
            .with_child(make_box());
        row2.seed_mut().classes.push("with-border".to_string());

        AppRoot::new().with_child(row1).with_child(row2)
    }
}

fn main() -> textual::Result<()> {
    run_sync(ContainerApp)
}
