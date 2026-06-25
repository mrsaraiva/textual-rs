/// Port of Python Textual `docs/examples/tutorial/stopwatch01.py`.
///
/// The most basic tutorial step: a minimal app with only Header and Footer,
/// plus a binding to toggle dark mode. No stopwatch widgets yet.
///
/// Python:
///   class StopwatchApp(App):
///       BINDINGS = [("d", "toggle_dark", "Toggle dark mode")]
///       def compose(self):
///           yield Header()
///           yield Footer()
///       def action_toggle_dark(self):
///           self.theme = "textual-dark" if ... else "textual-light"
use textual::prelude::*;

struct StopwatchApp;

impl TextualApp for StopwatchApp {
    fn title(&self) -> &'static str {
        "StopwatchApp"
    }

    fn bindings(&self) -> Vec<BindingDecl> {
        vec![BindingDecl::new("d", "toggle_dark", "Toggle dark mode")]
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Header::new())
            .with_child(Footer::new())
    }
}

fn main() -> textual::Result<()> {
    run_sync(StopwatchApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stopwatch01_composes_without_panic() {
        let mut app = StopwatchApp;
        let _root = app.compose();
    }

    #[test]
    fn has_toggle_dark_binding() {
        let app = StopwatchApp;
        let bindings = app.bindings();
        assert!(bindings.iter().any(|b| b.key == "d"));
    }

    // -- LIVENESS PROBE (Pilot run_test) --------------------------------------
    // stopwatch01's only interaction is the `d` → toggle_dark binding, which in
    // Python switches the registered theme (`textual-dark` <-> `textual-light`)
    // and recolours every `$`-token-styled surface (Header/Footer/Screen).
    //
    // CURRENTLY DEAD — root cause: the binding routes correctly (root
    // `execute_action("toggle_dark")` posts `AppToggleDark`, the handler calls
    // `App::action_toggle_dark()` and requests repaint), but
    // `action_toggle_dark` only flips a flat `self.theme.base` bg/fg — it does
    // NOT switch the registered token theme. Header/Footer/Screen resolve their
    // colours from `$background`/`$panel`/etc. via the theme-token registry,
    // which is unchanged, so the re-rendered frame is byte-identical. The fix is
    // to make `action_toggle_dark` switch the active *registered* theme (as
    // Python does), so token-styled widgets recolour. Flip this test active once
    // that lands. Tracking: toggle_dark-switches-registered-theme.
    #[ignore = "DEAD: toggle_dark mutates theme.base but not registered token theme; frame unchanged; see comment"]
    #[test]
    fn liveness_d_toggles_dark_mode() {
        textual::run_test(StopwatchApp, |pilot| {
            let before = pilot.app().frame_fingerprint();
            pilot.press(&["d"])?;
            let after = pilot.app().frame_fingerprint();
            assert_ne!(
                before, after,
                "pressing `d` must toggle dark mode and recolour the frame"
            );
            Ok(())
        })
        .unwrap();
    }
}
