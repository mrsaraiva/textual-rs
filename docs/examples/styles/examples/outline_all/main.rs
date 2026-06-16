/// Port of Python Textual `docs/examples/styles/outline_all.py`.
///
/// Demonstrates all available `outline` style variants on a 3x5 grid of Labels.
use textual::prelude::*;

const CSS: &str = r##"
#ascii {
    outline: ascii $accent;
}

#blank {
    outline: blank $accent;
}

#dashed {
    outline: dashed $accent;
}

#double {
    outline: double $accent;
}

#heavy {
    outline: heavy $accent;
}

#hidden {
    outline: hidden $accent;
}

#hkey {
    outline: hkey $accent;
}

#inner {
    outline: inner $accent;
}

#none {
    outline: none $accent;
}

#outer {
    outline: outer $accent;
}

#round {
    outline: round $accent;
}

#solid {
    outline: solid $accent;
}

#tall {
    outline: tall $accent;
}

#vkey {
    outline: vkey $accent;
}

#wide {
    outline: wide $accent;
}

Grid {
    grid-size: 3 5;
    align: center middle;
    grid-gutter: 1 2;
}

Label {
    width: 20;
    height: 3;
    content-align: center middle;
}
"##;

struct AllOutlinesApp;

impl TextualApp for AllOutlinesApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(
            Grid::new(5, 3)
                .with_child(Label::new("ascii").with_id("ascii"))
                .with_child(Label::new("blank").with_id("blank"))
                .with_child(Label::new("dashed").with_id("dashed"))
                .with_child(Label::new("double").with_id("double"))
                .with_child(Label::new("heavy").with_id("heavy"))
                .with_child(Label::new("hidden/none").with_id("hidden"))
                .with_child(Label::new("hkey").with_id("hkey"))
                .with_child(Label::new("inner").with_id("inner"))
                .with_child(Label::new("none").with_id("none"))
                .with_child(Label::new("outer").with_id("outer"))
                .with_child(Label::new("round").with_id("round"))
                .with_child(Label::new("solid").with_id("solid"))
                .with_child(Label::new("tall").with_id("tall"))
                .with_child(Label::new("vkey").with_id("vkey"))
                .with_child(Label::new("wide").with_id("wide")),
        )
    }
}

fn main() -> Result<()> {
    run_sync(AllOutlinesApp)
}
