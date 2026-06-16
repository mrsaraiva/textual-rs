/// Port of Python Textual `docs/examples/styles/text_align.py`.
///
/// Demonstrates `text-align` CSS property (left, center, right, justify)
/// on four Label widgets in a 2x2 grid.
use textual::prelude::*;

const TEXT: &str = "I must not fear. Fear is the mind-killer. Fear is the little-death that \
brings total obliteration. I will face my fear. I will permit it to pass over \
me and through me.";

const CSS: &str = r##"
#one {
    text-align: left;
    background: lightblue;
}

#two {
    text-align: center;
    background: indianred;
}

#three {
    text-align: right;
    background: palegreen;
}

#four {
    text-align: justify;
    background: palevioletred;
}

Label {
    padding: 1 2;
    height: 100%;
    color: auto;
}

Grid {
    grid-size: 2 2;
}
"##;

struct TextAlign;

impl TextualApp for TextAlign {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(
            Grid::new(2, 2)
                .with_child(Label::new(format!("[b]Left aligned[/]\n{TEXT}")).with_id("one"))
                .with_child(Label::new(format!("[b]Center aligned[/]\n{TEXT}")).with_id("two"))
                .with_child(Label::new(format!("[b]Right aligned[/]\n{TEXT}")).with_id("three"))
                .with_child(Label::new(format!("[b]Justified[/]\n{TEXT}")).with_id("four")),
        )
    }
}

fn main() -> Result<()> {
    run_sync(TextAlign)
}
