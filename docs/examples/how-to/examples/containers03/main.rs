/// Port of Python Textual `docs/examples/how-to/containers03.py`.
///
/// Demonstrates a `Horizontal` container with a CSS class applied for a border.
/// Three fixed-size `Placeholder` widgets are laid out side-by-side inside the
/// `Horizontal`, which is styled with `border: heavy green` via the `.with-border`
/// class.
///
/// Python original:
///   - `Box` is a `Placeholder` subclass with `width: 16; height: 8` as DEFAULT_CSS.
///   - App CSS: `.with-border { border: heavy green; }`
///   - compose: `Horizontal(classes="with-border")` containing 3 `Box()` instances.
use textual::prelude::*;

const CSS: &str = r#"
Placeholder {
    width: 16;
    height: 8;
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
        let horizontal = 
            Horizontal::new()
                .with_child(Placeholder::new(""))
                .with_child(Placeholder::new(""))
                .with_child(Placeholder::new(""))
        .class("with-border");

        AppRoot::new().with_child(horizontal)
    }
}

fn main() -> textual::Result<()> {
    run_sync(ContainerApp)
}
