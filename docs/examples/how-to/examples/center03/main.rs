/// Port of Python Textual `docs/examples/how-to/center03.py`.
///
/// Demonstrates centering a widget on the screen using `align: center middle`
/// on the Screen, with `width: auto` and a `wide white` border so the widget
/// shrinks to fit its content and is centered in both axes.
use textual::prelude::*;

const CSS: &str = r#"
Screen {
    align: center middle;
}

#hello {
    background: blue 50%;
    border: wide white;
    width: auto;
}
"#;

struct CenterApp;

impl TextualApp for CenterApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Static::new("Hello, World!").id("hello"))
    }
}

fn main() -> textual::Result<()> {
    run_sync(CenterApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn center_app_composes_without_panic() {
        let mut app = CenterApp;
        let _root = app.compose();
    }

    #[test]
    fn compose_produces_one_child() {
        let mut app = CenterApp;
        let root = app.compose();
        assert!(!root.children().is_empty());
    }
}
