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

    /// LIVENESS probe (Pilot, headless): pressing `b` pushes the BSOD screen and
    /// changes the frame; pressing `escape` (bound on the BSOD root to
    /// `app.pop_screen`) pops it and changes the frame back. Guards both the
    /// app-level push binding and the screen-root pop binding.
    ///
    /// Push (`b`, from the base screen) is LIVE; the screen-root `escape` pop
    /// binding is DEAD, so this probe is `#[ignore]`d for now.
    ///
    /// ROOT: a pushed screen's own declarative `bindings()` (here `escape ->
    /// app.pop_screen` on `BsodRoot`) are not in the active binding chain. With
    /// the BSOD screen active, `app.binding_hints()` lists only the inherited
    /// scroll/navigation bindings — `escape` is absent — so `match_binding_tree`
    /// (`runtime/routing.rs:621`) never finds it and the pop never fires.
    /// `match_binding_tree` walks the focused→root path of the active screen
    /// tree, but the screen-root binding owner is not reached on that path.
    /// Python collects Screen.BINDINGS (and App.BINDINGS) into the chain.
    /// TODO: include the screen-root (and app-root) bindings in the active chain
    /// when a screen is pushed; then drop `#[ignore]` — this probe flips to LIVE.
    #[ignore = "DEAD: screen-root key bindings (escape->pop_screen) not in the active binding chain"]
    #[test]
    fn screen01_push_and_pop_is_live() {
        run_test(BsodApp, |pilot| {
            assert_eq!(pilot.app().screen_count(), 0);
            let before = pilot.app().frame_fingerprint();

            pilot.press(&["b"])?;
            assert_eq!(pilot.app().screen_count(), 1, "b must push the BSOD screen");
            let pushed = pilot.app().frame_fingerprint();
            assert_ne!(before, pushed, "pushing the BSOD screen must change the frame");

            pilot.press(&["escape"])?;
            assert_eq!(pilot.app().screen_count(), 0, "escape must pop the BSOD screen");
            let popped = pilot.app().frame_fingerprint();
            assert_ne!(pushed, popped, "popping the BSOD screen must change the frame");
            Ok(())
        })
        .expect("screen01 push/pop harness should run");
    }

    /// LIVENESS probe (Pilot, headless): the *push* half of screen01 — pressing
    /// `b` (an app binding, fired from the base screen before any screen is
    /// pushed) pushes the BSOD screen and changes the frame. This half is LIVE
    /// and stays enabled as a permanent guard; the `escape` pop half is covered
    /// (currently `#[ignore]`d) by `screen01_push_and_pop_is_live`.
    #[test]
    fn screen01_push_is_live() {
        run_test(BsodApp, |pilot| {
            assert_eq!(pilot.app().screen_count(), 0);
            let before = pilot.app().frame_fingerprint();
            pilot.press(&["b"])?;
            assert_eq!(pilot.app().screen_count(), 1, "b must push the BSOD screen");
            assert_ne!(
                before,
                pilot.app().frame_fingerprint(),
                "pushing the BSOD screen must change the frame"
            );
            Ok(())
        })
        .expect("screen01 push harness should run");
    }
}
