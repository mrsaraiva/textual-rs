/// Port of Python Textual `docs/examples/styles/padding_all.py`.
///
/// Demonstrates all padding variants: `padding`, `padding: 1`, `padding: 1 5`,
/// `padding: 1 1 2 6`, `padding-top`, `padding-right`, `padding-bottom`,
/// `padding-left` using a 4-column Grid of Placeholder widgets.
use textual::prelude::*;

const CSS: &str = r##"
Screen {
    background: $background;
}

Grid {
    grid-size: 4;
    grid-gutter: 1 2;
}

Placeholder {
    width: auto;
    height: auto;
}

#p1 {
    /* default is no padding */
}

#p2 {
    padding: 1;
}

#p3 {
    padding: 1 5;
}

#p4 {
    padding: 1 1 2 6;
}

#p5 {
    padding-top: 4;
}

#p6 {
    padding-right: 3;
}

#p7 {
    padding-bottom: 4;
}

#p8 {
    padding-left: 3;
}
"##;

struct PaddingAllApp;

impl TextualApp for PaddingAllApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(
            Grid::new(4, 2)
                .with_child(Node::new(Placeholder::new("no padding")).id("p1"))
                .with_child(Node::new(Placeholder::new("padding: 1")).id("p2"))
                .with_child(Node::new(Placeholder::new("padding: 1 5")).id("p3"))
                .with_child(Node::new(Placeholder::new("padding: 1 1 2 6")).id("p4"))
                .with_child(Node::new(Placeholder::new("padding-top: 4")).id("p5"))
                .with_child(Node::new(Placeholder::new("padding-right: 3")).id("p6"))
                .with_child(Node::new(Placeholder::new("padding-bottom: 4")).id("p7"))
                .with_child(Node::new(Placeholder::new("padding-left: 3")).id("p8")),
        )
    }
}

fn main() -> Result<()> {
    run_sync(PaddingAllApp)
}
