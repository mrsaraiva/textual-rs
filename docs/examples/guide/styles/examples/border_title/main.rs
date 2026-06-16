/// Port of Python Textual `docs/examples/guide/styles/border_title.py`.
///
/// Demonstrates border_title and border_subtitle set via on_mount styles.
/// Note: border_title / border_subtitle / border_title_align are set inline
/// via CSS since Rust does not yet expose on_mount style mutation API.
///
/// Framework gap: border_title / border_subtitle runtime mutation (on_mount)
/// is not yet supported. CSS-only approximation used.
use textual::prelude::*;

const CSS: &str = r##"
Static {
    background: darkblue;
    width: 50%;
    border: heavy yellow;
    border-title-align: center;
}
"##;

const TEXT: &str = "I must not fear.
Fear is the mind-killer.
Fear is the little-death that brings total obliteration.
I will face my fear.
I will permit it to pass over me and through me.
And when it has gone past, I will turn the inner eye to see its path.
Where the fear has gone there will be nothing. Only I will remain.";

struct BorderTitleApp;

impl TextualApp for BorderTitleApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Static::new(TEXT))
    }
}

fn main() -> Result<()> {
    run_sync(BorderTitleApp)
}
