/// Port of Python Textual `docs/examples/widgets/placeholder.py`.
///
/// Demonstrates the `Placeholder` widget with various variants and layout:
/// - Default variant shows a label or auto-generated identifier.
/// - Size variant shows the widget's WxH dimensions.
/// - Text variant shows Lorem Ipsum text.
/// - Clicking a placeholder cycles through variants.
///
/// Layout mirrors the Python original: a VerticalScroll containing two
/// Containers — `#bot` (8×8 grid) and `#top` (2×2 grid).
use textual::compose;
use textual::prelude::*;

const CSS: &str = r#"
Placeholder {
    height: 100%;
}

#top {
    height: 50%;
    width: 100%;
    layout: grid;
    grid-size: 2 2;
}

#left {
    row-span: 2;
}

#bot {
    height: 50%;
    width: 100%;
    layout: grid;
    grid-size: 8 8;
}

#c1 {
    row-span: 4;
    column-span: 8;
    height: 100%;
}

#col1, #col2, #col3 {
    width: 1fr;
}

#p1 {
    row-span: 4;
    column-span: 4;
}

#p2 {
    row-span: 2;
    column-span: 4;
}

#p3 {
    row-span: 2;
    column-span: 2;
}

#p4 {
    row-span: 1;
    column-span: 2;
}
"#;

struct PlaceholderApp;

impl TextualApp for PlaceholderApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        // Horizontal row inside #bot: three placeholders side by side.
        let horiz = Horizontal::new().with_compose(compose![
            Placeholder::new("").with_variant(PlaceholderVariant::Size).id("col1"),
            Placeholder::new("").with_variant(PlaceholderVariant::Text).id("col2"),
            Placeholder::new("").with_variant(PlaceholderVariant::Size).id("col3"),
        ]);
        // Give #c1 its CSS id via the seed of the delegate target (Container inner).
        // Since Horizontal delegates take_node_seed to its inner Container,
        // expose the seed by accessing the inner via seed access through the Node wrapper.
        // Use Node wrapper to carry the id for the Horizontal.
        let c1 = horiz.id("c1");

        // #bot container: 8×8 grid. Set id directly on the Container seed.
        // Python Placeholder(id="pN") shows "#pN" as its label when no custom label
        // is provided. Rust Placeholder shows "Placeholder" as fallback, so we
        // explicitly pass the "#id" strings to match Python's visual output.
        let mut bot = Container::new().with_compose(compose![
            Placeholder::new("This is a custom label for p1.").id("p1"),
            Placeholder::new("Placeholder p2 here!").id("p2"),
            Placeholder::new("#p3").id("p3"),
            Placeholder::new("#p4").id("p4"),
            Placeholder::new("#p5").id("p5"),
            Placeholder::new(""),
            c1,
        ]);
        bot.seed_mut().css_id = Some("bot".to_string());

        // #top container: 2×2 grid. Set id directly on the Container seed.
        let mut top = Container::new().with_compose(compose![
            Placeholder::new("").with_variant(PlaceholderVariant::Text).id("left"),
            Placeholder::new("").with_variant(PlaceholderVariant::Size).id("topright"),
            Placeholder::new("").with_variant(PlaceholderVariant::Text).id("botright"),
        ]);
        top.seed_mut().css_id = Some("top".to_string());

        AppRoot::new().with_child(VerticalScroll::new().with_compose(compose![bot, top]))
    }
}

fn main() -> textual::Result<()> {
    run_sync(PlaceholderApp)
}
