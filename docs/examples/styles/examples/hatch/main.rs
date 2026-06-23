/// Port of Python Textual `docs/examples/styles/hatch.py`.
///
/// Demonstrates the `hatch` CSS property with five variants:
/// cross, horizontal, custom ("T"), left, and right. Each panel sets its
/// `border_title` to the hatch name, mirroring Python's
/// `static.border_title = hatch`.
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
                .with_child(Vertical::new().with_child(
                    Static::new("").class("hatch").class("cross").with_border_title("cross"),
                ))
                .with_child(Vertical::new().with_child(
                    Static::new("").class("hatch").class("horizontal").with_border_title("horizontal"),
                ))
                .with_child(Vertical::new().with_child(
                    Static::new("").class("hatch").class("custom").with_border_title("custom"),
                ))
                .with_child(Vertical::new().with_child(
                    Static::new("").class("hatch").class("left").with_border_title("left"),
                ))
                .with_child(Vertical::new().with_child(
                    Static::new("").class("hatch").class("right").with_border_title("right"),
                )),
        )
    }
}

fn main() -> Result<()> {
    run_sync(HatchApp)
}
