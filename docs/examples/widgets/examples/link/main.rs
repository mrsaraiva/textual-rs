/// Port of Python Textual `docs/examples/widgets/link.py`.
///
/// Demonstrates the `Link` widget centered on screen.
use textual::prelude::*;

const CSS: &str = r#"
Screen {
    align: center middle;
}
"#;

struct LinkApp;

impl TextualApp for LinkApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(
            Link::new("Go to textualize.io")
                .with_url("https://textualize.io")
                .with_tooltip("Click me"),
        )
    }
}

fn main() -> textual::Result<()> {
    run_sync(LinkApp)
}
