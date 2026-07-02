/// Port of Python Textual `docs/examples/styles/width_comparison.py`.
///
/// Demonstrates width value types: cells, percent, w, h, vw, vh, auto, fr.
/// A custom Ruler widget (Static subclass in Python) renders a ruler label.
///
/// Note: `layers` and `layer` CSS properties are not yet implemented in
/// textual-rs (framework gap). The Ruler dock-bottom behavior is ported faithfully.
use rich_rs::Segments;
use textual::compose;
use textual::prelude::*;

const CSS: &str = r##"
#cells {
    width: 9;
}
#percent {
    width: 12.5%;
}
#w {
    width: 10w;
}
#h {
    width: 25h;
}
#vw {
    width: 15vw;
}
#vh {
    width: 25vh;
}
#auto {
    width: auto;
}
#fr1 {
    width: 1fr;
}
#fr3 {
    width: 3fr;
}

Screen {
    layers: ruler;
}

Ruler {
    layer: ruler;
    dock: bottom;
    overflow: hidden;
    height: 1;
    background: $accent;
}
"##;

struct Ruler;

impl Widget for Ruler {
    fn style_type(&self) -> &'static str {
        "Ruler"
    }

    fn compose(&mut self) -> ComposeResult {
        let ruler_text = "····•".repeat(100);
        compose![Label::new(ruler_text)]
    }

    fn render(&self, _console: &rich_rs::Console, _options: &rich_rs::ConsoleOptions) -> Segments {
        Segments::new()
    }
}

struct WidthComparisonApp;

impl TextualApp for WidthComparisonApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(
                Horizontal::new()
                    .with_child(Placeholder::new("").id("cells"))
                    .with_child(Placeholder::new("").id("percent"))
                    .with_child(Placeholder::new("").id("w"))
                    .with_child(Placeholder::new("").id("h"))
                    .with_child(Placeholder::new("").id("vw"))
                    .with_child(Placeholder::new("").id("vh"))
                    .with_child(Placeholder::new("").id("auto"))
                    .with_child(Placeholder::new("").id("fr1"))
                    .with_child(Placeholder::new("").id("fr3")),
            )
            .with_child(Ruler)
    }
}

fn main() -> Result<()> {
    run_sync(WidthComparisonApp)
}
