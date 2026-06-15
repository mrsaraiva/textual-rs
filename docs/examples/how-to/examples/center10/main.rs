/// Port of Python Textual `docs/examples/how-to/center10.py`.
///
/// Demonstrates centering widgets horizontally using `Center` containers.
/// Each `Center` contains a `Static` with a `words` class styled with a blue
/// background, wide white border, and auto width.
///
/// The `Screen` uses `align: center middle` so both centers stack in the
/// vertical middle of the screen.
use textual::prelude::*;

const CSS: &str = r#"
Screen {
    align: center middle;
}

.words {
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
        let center1 = Center::new()
            .with_child(Static::new("How about a nice game").class("words"));
        let center2 = Center::new()
            .with_child(Static::new("of chess?").class("words"));

        AppRoot::new()
            .with_child(center1)
            .with_child(center2)
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
}
