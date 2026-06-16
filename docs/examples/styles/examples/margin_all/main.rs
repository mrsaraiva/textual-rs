/// Port of Python Textual `docs/examples/styles/margin_all.py`.
///
/// Demonstrates all margin variants (margin, margin-top, margin-right,
/// margin-bottom, margin-left) using a 4-column Grid with 8 Placeholder
/// widgets inside bordered Containers.
///
/// Note: Python `Container(Placeholder(...), classes="bordered")` maps to
/// `Node::new(Container::new()).class("bordered")` in Rust since Container
/// does not have builder methods for id/class. The `#pN` ids are placed
/// on Node wrappers around each Placeholder.
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
    width: 100%;
    height: 100%;
}

Container {
    width: 100%;
    height: 100%;
}

.bordered {
    border: white round;
}

#p1 {
    /* default is no margin */
}

#p2 {
    margin: 1;
}

#p3 {
    margin: 1 5;
}

#p4 {
    margin: 1 1 2 6;
}

#p5 {
    margin-top: 4;
}

#p6 {
    margin-right: 3;
}

#p7 {
    margin-bottom: 4;
}

#p8 {
    margin-left: 3;
}
"##;

struct MarginAllApp;

impl TextualApp for MarginAllApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(
            Grid::new(4, 2)
                .with_child(Node::new(Container::new().with_child(Node::new(Placeholder::new("no margin")).id("p1"))).class("bordered"))
                .with_child(Node::new(Container::new().with_child(Node::new(Placeholder::new("margin: 1")).id("p2"))).class("bordered"))
                .with_child(Node::new(Container::new().with_child(Node::new(Placeholder::new("margin: 1 5")).id("p3"))).class("bordered"))
                .with_child(Node::new(Container::new().with_child(Node::new(Placeholder::new("margin: 1 1 2 6")).id("p4"))).class("bordered"))
                .with_child(Node::new(Container::new().with_child(Node::new(Placeholder::new("margin-top: 4")).id("p5"))).class("bordered"))
                .with_child(Node::new(Container::new().with_child(Node::new(Placeholder::new("margin-right: 3")).id("p6"))).class("bordered"))
                .with_child(Node::new(Container::new().with_child(Node::new(Placeholder::new("margin-bottom: 4")).id("p7"))).class("bordered"))
                .with_child(Node::new(Container::new().with_child(Node::new(Placeholder::new("margin-left: 3")).id("p8"))).class("bordered")),
        )
    }
}

fn main() -> Result<()> {
    run_sync(MarginAllApp)
}
