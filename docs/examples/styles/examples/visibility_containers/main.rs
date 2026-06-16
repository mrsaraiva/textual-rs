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
        AppRoot::new().with_child(
            VerticalScroll::new()
                .with_child(
                    Node::new(
                        Horizontal::new()
                            .with_child(Placeholder::new(""))
                            .with_child(Placeholder::new(""))
                            .with_child(Placeholder::new("")),
                    )
                    .id("top"),
                )
                .with_child(
                    Node::new(
                        Horizontal::new()
                            .with_child(Placeholder::new(""))
                            .with_child(Placeholder::new(""))
                            .with_child(Placeholder::new("")),
                    )
                    .id("middle"),
                )
                .with_child(
                    Node::new(
                        Horizontal::new()
                            .with_child(Placeholder::new(""))
                            .with_child(Placeholder::new(""))
                            .with_child(Placeholder::new("")),
                    )
                    .id("bot"),
                ),
        )
    }
}

fn main() -> Result<()> {
    run_sync(VisibilityContainersApp)
}
