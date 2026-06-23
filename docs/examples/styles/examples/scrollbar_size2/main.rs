/// Port of Python Textual `docs/examples/styles/scrollbar_size2.py`.
///
/// Demonstrates `scrollbar-size`, `scrollbar-size-vertical`, and `scrollbar-size-horizontal`
/// CSS properties. Three ScrollableContainers in a Horizontal layout each contain a
/// large Label (repeated text), styled with different scrollbar size settings.
///
/// Framework gap: `scrollbar-size`, `scrollbar-size-vertical`, `scrollbar-size-horizontal`
/// CSS properties may not be fully supported in textual-rs; included verbatim per port rules.
use textual::prelude::*;

const TEXT: &str = "I must not fear.\nFear is the mind-killer.\nFear is the little-death that brings total obliteration.\nI will face my fear.\nI will permit it to pass over me and through me.\nAnd when it has gone past, I will turn the inner eye to see its path.\nWhere the fear has gone there will be nothing. Only I will remain.\n";

const CSS: &str = r##"
ScrollableContainer {
    width: 1fr;
}

#v1 {
    scrollbar-size: 5 1;
    background: red 20%;
}

#v2 {
    scrollbar-size-vertical: 1;
    background: green 20%;
}

#v3 {
    scrollbar-size-horizontal: 5;
    background: blue 20%;
}
"##;

struct ScrollbarApp;

impl TextualApp for ScrollbarApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        let text5 = TEXT.repeat(5);
        // NOTE: id() lives directly on the ScrollableContainer (the scrollbar
        // host) — not on a Node wrapper. A wrapper level would make `#v1` match
        // the wrapper while the scrollbar host falls back to the `Widget`
        // default `scrollbar-size-vertical: 2`. Mirrors Python
        // `ScrollableContainer(Label(...), id="v1")`.
        AppRoot::new().with_child(
            Horizontal::new()
                .with_child(
                    ScrollableContainer::new()
                        .with_child(Label::new(text5.clone()))
                        .id("v1"),
                )
                .with_child(
                    ScrollableContainer::new()
                        .with_child(Label::new(text5.clone()))
                        .id("v2"),
                )
                .with_child(
                    ScrollableContainer::new()
                        .with_child(Label::new(text5.clone()))
                        .id("v3"),
                ),
        )
    }
}

fn main() -> Result<()> {
    run_sync(ScrollbarApp)
}
