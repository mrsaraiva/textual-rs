/// Port of Python Textual `docs/examples/how-to/layout.py`.
///
/// Demonstrates a Twitter-like layout:
/// - Header (Placeholder, docked top, height 3)
/// - Footer (Placeholder, docked bottom, height 3)
/// - HorizontalScroll containing 4 Column widgets (VerticalScroll)
/// - Each Column contains 19 Tweet placeholders (height 5, width 1fr, tall border)
///
/// Python uses subclass type selectors (Header, Footer, Tweet, Column).
/// Rust uses CSS class selectors (.header, .footer, .tweet, .column) since
/// custom type selectors for user-defined structs are not supported.
use textual::prelude::*;

const CSS: &str = r#"
.header {
    height: 3;
    dock: top;
}

.footer {
    height: 3;
    dock: bottom;
}

.tweet {
    height: 5;
    width: 1fr;
    border: tall $background;
}

.column {
    height: 1fr;
    width: 32;
    margin: 0 2;
}
"#;

struct LayoutApp;

impl TextualApp for LayoutApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        let header = Node::new(Placeholder::new("")).id("Header").class("header");
        let footer = Node::new(Placeholder::new("")).id("Footer").class("footer");

        let mut h_scroll = HorizontalScroll::new();
        for _ in 0..4 {
            let mut column = VerticalScroll::new();
            for tweet_no in 1..=19 {
                column.push(
                    Node::new(Placeholder::new(""))
                        .id(format!("Tweet{}", tweet_no))
                        .class("tweet"),
                );
            }
            h_scroll.push(Node::new(column).class("column"));
        }

        AppRoot::new()
            .with_child(header)
            .with_child(footer)
            .with_child(h_scroll)
    }
}

fn main() -> textual::Result<()> {
    run_sync(LayoutApp)
}
