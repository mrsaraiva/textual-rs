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
}
