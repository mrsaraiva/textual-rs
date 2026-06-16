/// Port of Python Textual `docs/examples/styles/border_all.py`.
///
/// Demonstrates all available border styles (ascii, blank, dashed, double,
/// heavy, hidden, hkey, inner, outer, panel, round, solid, tall, thick, vkey, wide)
/// arranged in a 4x4 Grid.
use textual::prelude::*;

const CSS: &str = r##"
#ascii {
    border: ascii $accent;
}

#blank {
    border: blank $accent;
}

#dashed {
    border: dashed $accent;
}

#double {
    border: double $accent;
}

#heavy {
    border: heavy $accent;
}

#hidden {
    border: hidden $accent;
}

#hkey {
    border: hkey $accent;
}

#inner {
    border: inner $accent;
}

#outer {
    border: outer $accent;
}

#panel {
    border: panel $accent;
}

#round {
    border: round $accent;
}

#solid {
    border: solid $accent;
}

#tall {
    border: tall $accent;
}

#thick {
    border: thick $accent;
}

#vkey {
    border: vkey $accent;
}

#wide {
    border: wide $accent;
}

Grid {
    grid-size: 4 4;
    align: center middle;
    grid-gutter: 1 2;
}

Label {
    width: 20;
    height: 3;
    content-align: center middle;
}
"##;

struct AllBordersApp;

impl TextualApp for AllBordersApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(
            Grid::new(4, 4)
                .with_child(Label::new("ascii").with_id("ascii"))
                .with_child(Label::new("blank").with_id("blank"))
                .with_child(Label::new("dashed").with_id("dashed"))
                .with_child(Label::new("double").with_id("double"))
                .with_child(Label::new("heavy").with_id("heavy"))
                .with_child(Label::new("hidden/none").with_id("hidden"))
                .with_child(Label::new("hkey").with_id("hkey"))
                .with_child(Label::new("inner").with_id("inner"))
                .with_child(Label::new("outer").with_id("outer"))
                .with_child(Label::new("panel").with_id("panel"))
                .with_child(Label::new("round").with_id("round"))
                .with_child(Label::new("solid").with_id("solid"))
                .with_child(Label::new("tall").with_id("tall"))
                .with_child(Label::new("thick").with_id("thick"))
                .with_child(Label::new("vkey").with_id("vkey"))
                .with_child(Label::new("wide").with_id("wide")),
        )
    }
}

fn main() -> Result<()> {
    run_sync(AllBordersApp)
}
