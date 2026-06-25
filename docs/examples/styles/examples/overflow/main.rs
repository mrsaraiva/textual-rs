/// Port of Python Textual `docs/examples/styles/overflow.py`.
///
/// Demonstrates `overflow-y` CSS property: two VerticalScroll panels side
/// by side, each containing three Static widgets with the Dune litany text.
/// The left panel (#left) uses the default auto scroll, while the right
/// (#right) has `overflow-y: hidden` set via CSS.
use textual::prelude::*;

const TEXT: &str = "I must not fear.
Fear is the mind-killer.
Fear is the little-death that brings total obliteration.
I will face my fear.
I will permit it to pass over me and through me.
And when it has gone past, I will turn the inner eye to see its path.
Where the fear has gone there will be nothing. Only I will remain.";

const CSS: &str = r##"
Screen {
    background: $background;
    color: black;
}

VerticalScroll {
    width: 1fr;
}

Static {
    margin: 1 2;
    background: green 80%;
    border: green wide;
    color: white 90%;
    height: auto;
}

#right {
    overflow-y: hidden;
}
"##;

struct OverflowApp;

impl TextualApp for OverflowApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(
            Horizontal::new()
                .with_child(
                    // id() lives directly on the VerticalScroll (the scroll host) —
                    // not on a Node wrapper. A wrapper would make `#right` match the
                    // wrapper while the scroll host keeps the default `overflow-y:
                    // auto`. Mirrors Python `VerticalScroll(..., id="left")`.
                    VerticalScroll::new()
                        .with_child(Static::new(TEXT))
                        .with_child(Static::new(TEXT))
                        .with_child(Static::new(TEXT))
                        .id("left"),
                )
                .with_child(
                    VerticalScroll::new()
                        .with_child(Static::new(TEXT))
                        .with_child(Static::new(TEXT))
                        .with_child(Static::new(TEXT))
                        .id("right"),
                ),
        )
    }
}

fn main() -> Result<()> {
    run_sync(OverflowApp)
}
