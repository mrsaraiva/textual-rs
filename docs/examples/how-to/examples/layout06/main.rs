/// Port of Python Textual `docs/examples/how-to/layout06.py`.
///
/// Demonstrates a docked header/footer (Placeholder subclasses) combined with
/// a HorizontalScroll body holding four vertically-scrollable Column widgets.
/// Each Column contains 19 Tweet placeholders.
///
/// Python defines custom Placeholder subclasses (`Header`, `Footer`, `Tweet`,
/// `Column`) with `DEFAULT_CSS`. Rust targets them via CSS classes and IDs.
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
        let header = Node::new(Placeholder::new("#Header")).id("Header").class("header");
        let footer = Node::new(Placeholder::new("#Footer")).id("Footer").class("footer");

        let mut hs = HorizontalScroll::new();
        for _ in 0..4 {
            let mut col = VerticalScroll::new();
            for tweet_no in 1..=19 {
                let label = format!("#Tweet{}", tweet_no);
                let tweet = Node::new(Placeholder::new(label.clone()))
                    .id(format!("Tweet{}", tweet_no))
                    .class("tweet");
                col.push(tweet);
            }
            hs.push(Node::new(col).class("column"));
        }

        AppRoot::new()
            .with_child(header)
            .with_child(footer)
            .with_child(hs)
    }
}

fn main() -> textual::Result<()> {
    run_sync(LayoutApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_app_composes_without_panic() {
        let mut app = LayoutApp;
        let _root = app.compose();
    }
}
