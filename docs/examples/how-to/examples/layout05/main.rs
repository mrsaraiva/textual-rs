/// Port of Python Textual `docs/examples/how-to/layout05.py`.
///
/// Demonstrates docked header/footer placeholders with a `HorizontalScroll`
/// body container. The body has 4 `Column` (VerticalScroll) widgets, each
/// containing 19 `Tweet` (Placeholder) widgets.
///
/// Python defines `Header`, `Footer`, `Tweet` as `Placeholder` subclasses,
/// and `Column` as a `VerticalScroll` subclass that composes Tweet instances.
/// Rust ports these using CSS id selectors for Header/Footer and CSS class
/// selectors for Tweet placeholders.
use textual::prelude::*;

const CSS: &str = r#"
#Header {
    height: 3;
    dock: top;
}

#Footer {
    height: 3;
    dock: bottom;
}
"#;

struct LayoutApp;

impl TextualApp for LayoutApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        let mut columns = HorizontalScroll::new();
        for _ in 0..4 {
            let mut column = VerticalScroll::new();
            for tweet_no in 1..=19 {
                column.push(
                    Placeholder::new(format!("#Tweet{}", tweet_no))
                        .id(format!("Tweet{}", tweet_no)),
                );
            }
            columns.push(column);
        }

        AppRoot::new()
            .with_child(Placeholder::new("#Header").id("Header"))
            .with_child(Placeholder::new("#Footer").id("Footer"))
            .with_child(columns)
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
