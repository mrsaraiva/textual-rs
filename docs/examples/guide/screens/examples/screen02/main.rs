/// Port of Python Textual `docs/examples/guide/screens/screen02.py`.
///
/// Demonstrates a BSOD (Blue Screen of Death) overlay screen. Press `b` to push
/// the BSOD screen. Press `escape` to pop it back to the main app.
///
/// Python used `install_screen(BSOD(), name="bsod")` in `on_mount`; Rust uses
/// `app.add_mode("bsod", ...)` in `configure`, which is equivalent.
///
/// The BSOD screen binds `escape` to `app.pop_screen` via the root widget's
/// `bindings()` method, mirroring Python's `BINDINGS` class variable on the
/// `BSOD(Screen)` class.
use rich_rs::Segments;
use textual::compose;
use textual::prelude::*;

const ERROR_TEXT: &str = "
An error has occurred. To continue:

Press Enter to return to Windows, or

Press CTRL+ALT+DEL to restart your computer. If you do this,
you will lose any unsaved information in all open applications.

Error: 0E : 016F : BFF9B3D4
";

const CSS: &str = r#"
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
"#;

// ---------------------------------------------------------------------------
// BSOD screen root widget
// ---------------------------------------------------------------------------

struct BSODRoot;

impl Widget for BSODRoot {
    fn style_type(&self) -> &'static str {
        "BSOD"
    }

    fn compose(&self) -> ComposeResult {
        compose![
            Static::new(" Windows ").id("title"),
            Static::new(ERROR_TEXT),
            Static::new("Press any key to continue _").id("any-key"),
        ]
    }

    fn render(&self, _console: &rich_rs::Console, _options: &rich_rs::ConsoleOptions) -> Segments {
        Segments::new()
    }

    fn bindings(&self) -> Vec<BindingDecl> {
        vec![BindingDecl::new("escape", "app.pop_screen", "Pop screen")]
    }
}

// ---------------------------------------------------------------------------
// BSOD Screen wrapper
// ---------------------------------------------------------------------------

struct BSODScreen;

impl Screen for BSODScreen {
    fn name(&self) -> &str {
        "bsod"
    }

    fn compose(&self) -> Box<dyn Widget> {
        Box::new(BSODRoot)
    }

    fn is_modal(&self) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// Main app
// ---------------------------------------------------------------------------

struct BSODApp;

impl TextualApp for BSODApp {
    fn bindings(&self) -> Vec<BindingDecl> {
        vec![BindingDecl::new("b", "app.push_screen('bsod')", "BSOD")]
    }

    fn configure(&mut self, app: &mut App) -> Result<()> {
        app.load_stylesheet(CSS);
        app.add_mode("bsod", || Box::new(BSODScreen));
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
    }
}

fn main() -> Result<()> {
    run_sync(BSODApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bsod_screen_registers_and_pushes() {
        let mut definition = BSODApp;
        let mut app = App::new().expect("app should initialize");
        definition
            .configure(&mut app)
            .expect("screen02 configure should succeed");

        assert_eq!(app.screen_count(), 0);
        assert!(app.action_push_screen("bsod"));
        assert_eq!(app.screen_count(), 1);
    }

    #[test]
    fn bsod_root_has_escape_binding() {
        let root = BSODRoot;
        let bindings = root.bindings();
        assert!(
            bindings.iter().any(|b| b.key == "escape"),
            "escape binding missing from BSODRoot"
        );
    }

    #[test]
    fn bsod_app_has_b_binding() {
        let app = BSODApp;
        let bindings = app.bindings();
        assert!(
            bindings.iter().any(|b| b.key == "b"),
            "b binding missing from BSODApp"
        );
    }

    #[test]
    fn bsod_screen_is_non_modal() {
        let screen = BSODScreen;
        assert!(!screen.is_modal());
    }
}
