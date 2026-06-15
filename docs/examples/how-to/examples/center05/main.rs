use textual::prelude::*;

const QUOTE: &str = "Could not find you in Seattle and no terminal is in operation at your classified address.";

const CSS: &str = r#"
Screen {
    align: center middle;
}

#hello {
    background: blue 50%;
    border: wide white;
    width: 40;
    text-align: center;
}
"#;

struct CenterApp;

impl TextualApp for CenterApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        let child = ChildDecl::from(Static::new(QUOTE)).with_id("hello");
        AppRoot::new().with_compose(vec![child])
    }
}

fn main() -> Result<()> {
    if cfg!(test) {
        return Ok(());
    }
    run_sync(CenterApp)
}
