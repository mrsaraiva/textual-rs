/// Port of Python Textual `docs/examples/styles/visibility_containers.py`.
///
/// Demonstrates `visibility: hidden` on containers and `visibility: visible`
/// on individual children to override the parent's hidden state.
///
/// Note: `visibility` CSS property support in textual-rs may be a framework gap.
use textual::prelude::*;

const CSS: &str = r##"
Horizontal {
    padding: 1 2;
    background: white;
    height: 1fr;
}

#top {}

#middle {
    visibility: hidden;
}

#bot {
    visibility: hidden;
}

#bot > Placeholder {
    visibility: visible;
}

Placeholder {
    width: 1fr;
}
"##;

struct VisibilityContainersApp;

impl TextualApp for VisibilityContainersApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        // Put the id directly on each Horizontal (matching Python
        // `Horizontal(..., id="top")`) so the `#bot > Placeholder` child-combinator
        // rule targets the Placeholders that are DIRECT children of `#bot`. A
        // `Node` wrapper would split the id onto a wrapper level, leaving the
        // Placeholders as grandchildren that the `>` selector can't reach.
        AppRoot::new().with_child(
            VerticalScroll::new()
                .with_child(
                    Horizontal::new()
                        .id("top")
                        .with_child(Placeholder::new(""))
                        .with_child(Placeholder::new(""))
                        .with_child(Placeholder::new("")),
                )
                .with_child(
                    Horizontal::new()
                        .id("middle")
                        .with_child(Placeholder::new(""))
                        .with_child(Placeholder::new(""))
                        .with_child(Placeholder::new("")),
                )
                .with_child(
                    Horizontal::new()
                        .id("bot")
                        .with_child(Placeholder::new(""))
                        .with_child(Placeholder::new(""))
                        .with_child(Placeholder::new("")),
                ),
        )
    }
}

fn main() -> Result<()> {
    run_sync(VisibilityContainersApp)
}
