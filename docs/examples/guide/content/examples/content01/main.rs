/// Port of Python Textual `docs/examples/guide/content/content01.py`.
///
/// Demonstrates Static widgets with markup enabled (text1) and markup disabled
/// (text2 — `markup=False` in Python), split into two equal-height panels with
/// distinct background colours.
use textual::prelude::*;

const TEXT1: &str = "\
Hello, [bold $text on $primary]World[/]!

[@click=app.notify('Hello, World!')]Click me[/]
";

// markup=False in Python: tags are left as-is in the output, not interpreted.
const TEXT2: &str = "\
Markup will [bold]not[/bold] be displayed.

Tags will be left in the output.

";

const CSS: &str = r##"
Screen {
    Static {
        height: 1fr;
    }
    #text1 { background: $primary-muted; }
    #text2 { background: $error-muted; }
}
"##;

struct ContentApp;

impl TextualApp for ContentApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            // text1: markup enabled (Static default is markup=true)
            .with_child(Static::new(TEXT1).id("text1"))
            // text2: markup disabled — mirrors Python's Static(TEXT2, markup=False).
            // Using Static::without_markup() keeps the CSS type as `Static` so that
            // the `Static { height: 1fr }` rule applies to both panels equally.
            .with_child(Static::new(TEXT2).without_markup().id("text2"))
    }
}

fn main() -> Result<()> {
    run_sync(ContentApp)
}
