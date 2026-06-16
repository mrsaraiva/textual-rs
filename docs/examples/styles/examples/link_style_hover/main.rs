/// Port of Python Textual `docs/examples/styles/link_style_hover.py`.
///
/// Demonstrates `link-style-hover` CSS property on Labels containing
/// Rich markup links and click actions.
///
/// Framework gap: `link-style-hover` CSS property may not be fully supported
/// in textual-rs yet.
use textual::prelude::*;

const CSS: &str = r##"
#lbl1, #lbl2 {
    link-style-hover: bold italic;
}

#lbl3 {
    link-style-hover: reverse strike;
}

#lbl4 {
    link-style-hover: bold;
}
"##;

struct LinkHoverStyleApp;

impl TextualApp for LinkHoverStyleApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(
                Label::new("Visit the [link='https://textualize.io']Textualize[/link] website.")
                    .with_markup(true)
                    .with_id("lbl1"),
            )
            .with_child(
                Label::new("Click [@click=app.bell]here[/] for the bell sound.")
                    .with_markup(true)
                    .with_id("lbl2"),
            )
            .with_child(
                Label::new(
                    "You can also click [@click=app.bell]here[/] for the bell sound.",
                )
                .with_markup(true)
                .with_id("lbl3"),
            )
            .with_child(
                Label::new("[@click=app.quit]Exit this application.[/]")
                    .with_markup(true)
                    .with_id("lbl4"),
            )
    }
}

fn main() -> Result<()> {
    run_sync(LinkHoverStyleApp)
}
