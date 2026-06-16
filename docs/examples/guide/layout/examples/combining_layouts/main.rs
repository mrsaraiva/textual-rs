/// Port of Python Textual `docs/examples/guide/layout/combining_layouts.py`.
///
/// Demonstrates combining grid, vertical-scroll, horizontal, and container
/// layouts in a single app.
///
/// Framework gaps: `row-span` and `column-span` CSS properties may not be
/// fully rendered yet.
use textual::prelude::*;

const CSS: &str = r##"
#app-grid {
    layout: grid;
    grid-size: 2;  /* two columns */
    grid-columns: 1fr;
    grid-rows: 1fr;
}

#left-pane > Static {
    background: $boost;
    color: auto;
    margin-bottom: 1;
    padding: 1;
}

#left-pane {
    width: 100%;
    height: 100%;
    row-span: 2;
    background: $panel;
    border: dodgerblue;
}

#top-right {
    height: 100%;
    background: $panel;
    border: mediumvioletred;
}

#top-right > Static {
    width: auto;
    height: 100%;
    margin-right: 1;
    background: $boost;
}

#bottom-right {
    height: 100%;
    layout: grid;
    grid-size: 3;
    grid-columns: 1fr;
    grid-rows: 1fr;
    grid-gutter: 1;
    background: $panel;
    border: greenyellow;
}

#bottom-right-final {
    column-span: 2;
}

#bottom-right > Static {
    height: 100%;
    background: $boost;
}
"##;

struct CombiningLayoutsExample;

impl TextualApp for CombiningLayoutsExample {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Header::new())
            .with_child(
                Node::new(Container::new()
                    .with_child(
                        Node::new(VerticalScroll::new()
                            .with_child(Static::new("Vertical layout, child 0"))
                            .with_child(Static::new("Vertical layout, child 1"))
                            .with_child(Static::new("Vertical layout, child 2"))
                            .with_child(Static::new("Vertical layout, child 3"))
                            .with_child(Static::new("Vertical layout, child 4"))
                            .with_child(Static::new("Vertical layout, child 5"))
                            .with_child(Static::new("Vertical layout, child 6"))
                            .with_child(Static::new("Vertical layout, child 7"))
                            .with_child(Static::new("Vertical layout, child 8"))
                            .with_child(Static::new("Vertical layout, child 9"))
                            .with_child(Static::new("Vertical layout, child 10"))
                            .with_child(Static::new("Vertical layout, child 11"))
                            .with_child(Static::new("Vertical layout, child 12"))
                            .with_child(Static::new("Vertical layout, child 13"))
                            .with_child(Static::new("Vertical layout, child 14")))
                        .id("left-pane"),
                    )
                    .with_child(
                        Node::new(Horizontal::new()
                            .with_child(Static::new("Horizontally"))
                            .with_child(Static::new("Positioned"))
                            .with_child(Static::new("Children"))
                            .with_child(Static::new("Here")))
                        .id("top-right"),
                    )
                    .with_child(
                        Node::new(Container::new()
                            .with_child(Static::new("This"))
                            .with_child(Static::new("panel"))
                            .with_child(Static::new("is"))
                            .with_child(Static::new("using"))
                            .with_child(Static::new("grid layout!").id("bottom-right-final")))
                        .id("bottom-right"),
                    ))
                .id("app-grid"),
            )
    }
}

fn main() -> Result<()> {
    run_sync(CombiningLayoutsExample)
}
