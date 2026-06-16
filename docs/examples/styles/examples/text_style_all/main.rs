/// Port of Python Textual `docs/examples/styles/text_style_all.py`.
///
/// Demonstrates all `text-style` variants (none, bold, italic, reverse, strike,
/// underline, bold italic, reverse strike) in a 4-column Grid of Labels.
use textual::prelude::*;

const TEXT: &str = "I must not fear.\nFear is the mind-killer.\nFear is the little-death that brings total obliteration.\nI will face my fear.\nI will permit it to pass over me and through me.\nAnd when it has gone past, I will turn the inner eye to see its path.\nWhere the fear has gone there will be nothing. Only I will remain.";

const CSS: &str = r##"
#lbl1 {
    text-style: none;
}

#lbl2 {
    text-style: bold;
}

#lbl3 {
    text-style: italic;
}

#lbl4 {
    text-style: reverse;
}

#lbl5 {
    text-style: strike;
}

#lbl6 {
    text-style: underline;
}

#lbl7 {
    text-style: bold italic;
}

#lbl8 {
    text-style: reverse strike;
}

Grid {
    grid-size: 4;
    grid-gutter: 1 2;
    margin: 1 2;
    height: 100%;
}

Label {
    height: 100%;
}
"##;

struct AllTextStyleApp;

impl TextualApp for AllTextStyleApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(
            Grid::new(4, 2)
                .with_child(Label::new(format!("none\n{TEXT}")).with_id("lbl1"))
                .with_child(Label::new(format!("bold\n{TEXT}")).with_id("lbl2"))
                .with_child(Label::new(format!("italic\n{TEXT}")).with_id("lbl3"))
                .with_child(Label::new(format!("reverse\n{TEXT}")).with_id("lbl4"))
                .with_child(Label::new(format!("strike\n{TEXT}")).with_id("lbl5"))
                .with_child(Label::new(format!("underline\n{TEXT}")).with_id("lbl6"))
                .with_child(Label::new(format!("bold italic\n{TEXT}")).with_id("lbl7"))
                .with_child(Label::new(format!("reverse strike\n{TEXT}")).with_id("lbl8")),
        )
    }
}

fn main() -> Result<()> {
    run_sync(AllTextStyleApp)
}
