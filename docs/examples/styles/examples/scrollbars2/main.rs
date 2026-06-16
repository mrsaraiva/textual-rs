/// Port of Python Textual `docs/examples/styles/scrollbars2.py`.
///
/// Demonstrates custom scrollbar color CSS properties on the Screen.
/// The label content is repeated 10 times to make scrollbars visible.
///
/// Note: scrollbar color CSS properties (scrollbar-background, scrollbar-color, etc.)
/// are not yet visually implemented in textual-rs (framework gap).
use textual::prelude::*;

const CSS: &str = r##"
Screen {
    scrollbar-background: blue;
    scrollbar-background-active: red;
    scrollbar-background-hover: purple;
    scrollbar-color: cyan;
    scrollbar-color-active: yellow;
    scrollbar-color-hover: pink;
}
"##;

const TEXT: &str = "I must not fear.\nFear is the mind-killer.\nFear is the little-death that brings total obliteration.\nI will face my fear.\nI will permit it to pass over me and through me.\nAnd when it has gone past, I will turn the inner eye to see its path.\nWhere the fear has gone there will be nothing. Only I will remain.\n";

struct Scrollbar2App;

impl TextualApp for Scrollbar2App {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        let text = TEXT.repeat(10);
        AppRoot::new().with_child(Label::new(text))
    }
}

fn main() -> Result<()> {
    run_sync(Scrollbar2App)
}
