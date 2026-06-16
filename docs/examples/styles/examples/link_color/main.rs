/// Port of Python Textual `docs/examples/styles/link_color.py`.
///
/// Demonstrates `link-color` CSS property on Label widgets with inline markup
/// links and actions. Framework gap: `link-color` / `link_*` styles may not be
/// fully supported yet.
use textual::prelude::*;

const CSS: &str = r##"
#lbl1, #lbl2 {
    link-color: red;
}

#lbl3 {
    link-color: hsl(60,100%,50%) 50%;
}

#lbl4 {
    link-color: $accent;
}
"##;

struct LinkColorApp;

impl TextualApp for LinkColorApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(
                Label::new("Visit the [link='https://textualize.io']Textualize[/link] website.")
                    .with_id("lbl1"),
            )
            .with_child(
                Label::new("Click [@click=app.bell]here[/] for the bell sound.").with_id("lbl2"),
            )
            .with_child(
                Label::new("You can also click [@click=app.bell]here[/] for the bell sound.")
                    .with_id("lbl3"),
            )
            .with_child(Label::new("[@click=app.quit]Exit this application.[/]").with_id("lbl4"))
    }
}

fn main() -> Result<()> {
    run_sync(LinkColorApp)
}
