/// Port of Python Textual `docs/examples/styles/height_comparison.py`.
///
/// Demonstrates various height value types: cells, percent, w, h, vw, vh, auto, fr.
/// A custom Ruler widget (Static subclass in Python) renders a vertical ruler on
/// the right side.
///
/// Note: `layers` and `layer` CSS properties are not yet implemented in
/// textual-rs (framework gap). The Ruler dock-right behavior is ported faithfully.
use rich_rs::Segments;
use textual::compose;
use textual::prelude::*;

const CSS: &str = r##"
#cells {
    height: 2;
}
#percent {
    height: 12.5%;
}
#w {
    height: 5w;
}
#h {
    height: 12.5h;
}
#vw {
    height: 6.25vw;
}
#vh {
    height: 12.5vh;
}
#auto {
    height: auto;
}
#fr1 {
    height: 1fr;
}
#fr2 {
    height: 2fr;
}

Screen {
    layers: ruler;
    overflow: hidden;
}

Ruler {
    layer: ruler;
    dock: right;
    width: 1;
    background: $accent;
}
"##;

struct Ruler;

impl Widget for Ruler {
    fn style_type(&self) -> &'static str {
        "Ruler"
    }

    fn compose(&mut self) -> ComposeResult {
        let ruler_text = "·\n·\n·\n·\n•\n".repeat(100);
        compose![Label::new(ruler_text)]
    }

    fn render(&self, _console: &rich_rs::Console, _options: &rich_rs::ConsoleOptions) -> Segments {
        Segments::new()
    }
}

struct HeightComparisonApp;

impl TextualApp for HeightComparisonApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(
                VerticalScroll::new().with_compose(compose![
                    Placeholder::new("").id("cells"),
                    Placeholder::new("").id("percent"),
                    Placeholder::new("").id("w"),
                    Placeholder::new("").id("h"),
                    Placeholder::new("").id("vw"),
                    Placeholder::new("").id("vh"),
                    Placeholder::new("").id("auto"),
                    Placeholder::new("").id("fr1"),
                    Placeholder::new("").id("fr2"),
                ]),
            )
            .with_child(Ruler)
    }
}

fn main() -> Result<()> {
    run_sync(HeightComparisonApp)
}
