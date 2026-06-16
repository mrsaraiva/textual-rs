/// Port of Python Textual `docs/examples/styles/links.py`.
///
/// Demonstrates `link-color`, `link-background`, and `link-style` CSS
/// properties. Two Static widgets both contain a clickable link markup
/// tag; the second (#custom) has custom link styling via CSS.
///
/// Framework gaps: `link-color`, `link-background`, `link-style` CSS
/// properties and `[@click='app.bell']` markup action links may not be
/// fully supported yet.
use textual::prelude::*;

const TEXT: &str = "Here is a [@click='app.bell']link[/] which you can click!\n";

const CSS: &str = r##"
#custom {
    link-color: black 90%;
    link-background: dodgerblue;
    link-style: bold italic underline;
}
"##;

struct LinksApp;

impl TextualApp for LinksApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Static::new(TEXT))
            .with_child(Static::new(TEXT).id("custom"))
    }
}

fn main() -> Result<()> {
    run_sync(LinksApp)
}
