/// Port of Python Textual `docs/examples/styles/scrollbar_corner_color.py`.
///
/// Demonstrates the `scrollbar-corner-color` CSS property on Screen with
/// overflow auto to show scrollbars in both directions.
/// Framework gap: `scrollbar-corner-color` may not be fully rendered yet.
use textual::prelude::*;

const TEXT: &str = "I must not fear.\n\
Fear is the mind-killer.\n\
Fear is the little-death that brings total obliteration.\n\
I will face my fear.\n\
I will permit it to pass over me and through me.\n\
And when it has gone past, I will turn the inner eye to see its path.\n\
Where the fear has gone there will be nothing. Only I will remain.\n";

const CSS: &str = r##"
Screen {
    overflow: auto auto;
    scrollbar-corner-color: white;
}
"##;

struct ScrollbarCornerColorApp;

impl TextualApp for ScrollbarCornerColorApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        let long_text = format!(
            "{}\n{}",
            TEXT.replace('\n', " "),
            TEXT.repeat(10)
        );
        AppRoot::new().with_child(Label::new(long_text))
    }
}

fn main() -> Result<()> {
    run_sync(ScrollbarCornerColorApp)
}
