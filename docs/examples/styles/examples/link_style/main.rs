/// Port of Python Textual `docs/examples/styles/link_style.py`.
///
/// Demonstrates the link-style CSS property on label links.
///
/// Framework gap: `link-style` CSS property may not be fully supported in
/// textual-rs (link rendering/click handling). Included verbatim per port policy.
use textual::prelude::*;

const CSS: &str = r##"
#lbl1, #lbl2 {
    link-style: bold italic;
}

#lbl3 {
    link-style: reverse strike;
}

#lbl4 {
    link-style: bold;
}
"##;

struct LinkStyleApp;

impl TextualApp for LinkStyleApp {
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
            .with_child(
                Label::new("[@click=app.quit]Exit this application.[/]").id("lbl4"),
            )
    }
}

fn main() -> Result<()> {
    run_sync(LinkStyleApp)
}
