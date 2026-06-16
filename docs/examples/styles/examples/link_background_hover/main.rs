/// Port of Python Textual `docs/examples/styles/link_background_hover.py`.
///
/// Demonstrates the `link-background-hover` CSS property on Labels with
/// inline markup links and action links.
///
/// Framework gap: `link-background-hover` is a link-specific style that may
/// not yet be fully supported in textual-rs.
use textual::prelude::*;

const CSS: &str = r##"
#lbl1, #lbl2 {
    link-background-hover: red;
}

#lbl3 {
    link-background-hover: hsl(60,100%,50%) 50%;
}

#lbl4 {
    /* Empty to show the default hover background */
}
"##;

struct LinkHoverBackgroundApp;

impl TextualApp for LinkHoverBackgroundApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(
                Label::new("Visit the [link='https://textualize.io']Textualize[/link] website.")
                    .id("lbl1"),
            )
            .with_child(
                Label::new("Click [@click=app.bell]here[/] for the bell sound.").id("lbl2"),
            )
            .with_child(
                Label::new("You can also click [@click=app.bell]here[/] for the bell sound.")
                    .id("lbl3"),
            )
            .with_child(Label::new("[@click=app.quit]Exit this application.[/]").id("lbl4"))
    }
}

fn main() -> Result<()> {
    run_sync(LinkHoverBackgroundApp)
}
