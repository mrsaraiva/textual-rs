/// Port of Python Textual `docs/examples/styles/hatch.py`.
///
/// Demonstrates the `hatch` CSS property with five variants:
/// cross, horizontal, custom ("T"), left, and right.
///
/// Framework gaps:
///   - `hatch` CSS property is not yet implemented in textual-rs.
///   - `border_title` cannot be set dynamically on arbitrary widgets;
///     the per-widget title will be absent until a generic builder is added.
use textual::prelude::*;

const CSS: &str = r##"
.hatch {
    height: 1fr;
    border: solid $secondary;

    &.cross {
        hatch: cross $success;
    }
    &.horizontal {
        hatch: horizontal $success 80%;
    }
    &.custom {
        hatch: "T" $success 60%;
    }
    &.left {
        hatch: left $success 40%;
    }
    &.right {
        hatch: right $success 20%;
    }
}
"##;

struct HatchApp;

impl TextualApp for HatchApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(
            Horizontal::new()
                .with_child(Vertical::new().with_child(Static::new("").class("hatch").class("cross")))
                .with_child(Vertical::new().with_child(Static::new("").class("hatch").class("horizontal")))
                .with_child(Vertical::new().with_child(Static::new("").class("hatch").class("custom")))
                .with_child(Vertical::new().with_child(Static::new("").class("hatch").class("left")))
                .with_child(Vertical::new().with_child(Static::new("").class("hatch").class("right"))),
        )
    }
}

fn main() -> Result<()> {
    run_sync(HatchApp)
}
