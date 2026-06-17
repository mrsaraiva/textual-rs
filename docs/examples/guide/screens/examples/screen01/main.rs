use rich_rs::Segments;
use textual::compose;
use textual::prelude::*;

// CSS translated from Python's screen01.tcss
const CSS: &str = r##"
BSOD {
    align: center middle;
    background: blue;
    color: white;
}

BSOD > Static {
    width: 70;
}

#title {
    content-align-horizontal: center;
    text-style: reverse;
}

#any-key {
    content-align-horizontal: center;
}
"##;

const ERROR_TEXT: &str = "
An error has occurred. To continue:

Press Enter to return to Windows, or

Press CTRL+ALT+DEL to restart your computer. If you do this,
you will lose any unsaved information in all open applications.

Error: 0E : 016F : BFF9B3D4
";

// ---------------------------------------------------------------------------
// BSOD screen root widget
// ---------------------------------------------------------------------------

struct BsodRoot;

impl Widget for BsodRoot {
    fn style_type(&self) -> &'static str {
        "BSOD"
    }

    fn bindings(&self) -> Vec<BindingDecl> {
        vec![BindingDecl::new("escape", "app.pop_screen", "Pop screen")]
    }

    fn compose(&self) -> ComposeResult {
        compose![
            Static::new(" Windows ").id("title"),
            Static::new(ERROR_TEXT),
            Static::new("Press any key to continue _").id("any-key")
        ]
    }

    fn render(
        &self,
        _console: &rich_rs::Console,
        _options: &rich_rs::ConsoleOptions,
    ) -> Segments {
        Segments::new()
    }
}

// ---------------------------------------------------------------------------
// BSOD screen
// ---------------------------------------------------------------------------

struct BsodScreen;

impl Screen for BsodScreen {
    fn name(&self) -> &str {
        "BsodScreen"
    }

    fn compose(&self) -> Box<dyn Widget> {
        Box::new(BsodRoot)
    }

    fn is_modal(&self) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// BSODApp
// ---------------------------------------------------------------------------

struct BsodApp;

impl TextualApp for BsodApp {
    fn bindings(&self) -> Vec<BindingDecl> {
        vec![BindingDecl::new("b", "app.push_screen('bsod')", "BSOD")]
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
    }

    fn configure(&mut self, app: &mut App) -> Result<()> {
        app.load_stylesheet(CSS);
        app.add_mode("bsod", || Box::new(BsodScreen));
        Ok(())
    }
}

fn main() -> Result<()> {
    run_sync(BsodApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen01_registers_bsod_mode() {
        let mut definition = BsodApp;
        let mut app = App::new().expect("app should initialize");
        definition
            .configure(&mut app)
            .expect("screen01 configure should succeed");

        assert_eq!(app.screen_count(), 0);
        assert!(app.action_push_screen("bsod"));
        assert_eq!(app.screen_count(), 1);
    }

    #[test]
    fn screen01_bsod_screen_is_non_modal() {
        let screen = BsodScreen;
        assert!(!screen.is_modal());
    }
}
