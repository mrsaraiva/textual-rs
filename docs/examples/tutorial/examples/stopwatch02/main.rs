/// Port of Python Textual `docs/examples/tutorial/stopwatch02.py`.
///
/// Adds the `Stopwatch` (HorizontalGroup) and `TimeDisplay` (Digits) widgets.
/// Three stopwatches in a `VerticalScroll`. No custom CSS yet.
///
/// Python defines:
///   class TimeDisplay(Digits): ...
///   class Stopwatch(HorizontalGroup):
///       def compose(self): yield Button(...), Button(...), Button(...), TimeDisplay("00:00:00.00")
///
/// In Rust, HorizontalGroup is used directly for Stopwatch since there is no
/// behavior yet (no button handlers, no timer). Digits is used for TimeDisplay.
///
/// NON-PROMOTABLE (timer-driven): Python auto-starts a timer on mount for
/// TimeDisplay; Rust shows a static "00:00:00.00".
use textual::compose;
use textual::prelude::*;

fn make_stopwatch() -> HorizontalGroup {
    HorizontalGroup::new().with_compose(compose![
        Button::success("Start").id("start"),
        Button::error("Stop").id("stop"),
        Button::new("Reset").id("reset"),
        Digits::new("00:00:00.00"),
    ])
}

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
            .with_child(
                VerticalScroll::new().with_child(Vertical::new().with_compose(compose![
                    make_stopwatch(),
                    make_stopwatch(),
                    make_stopwatch(),
                ])),
            )
    }
}

fn main() -> textual::Result<()> {
    run_sync(StopwatchApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stopwatch02_composes_without_panic() {
        let mut app = StopwatchApp;
        let _root = app.compose();
    }

    // -- LIVENESS PROBE (Pilot run_test) --------------------------------------
    // At this tutorial step the buttons have no handlers (Python's stopwatch02
    // is structure-only), but the widgets are still interactive: clicking a
    // Button focuses it and applies the Button focus/active styling, changing
    // the rendered frame. This proves the composed Stopwatch UI responds to
    // input. (The `d` toggle_dark binding is separately DEAD — see
    // stopwatch01's probe for the theme-token root cause.)
    #[test]
    fn liveness_tab_focuses_button_changes_frame() {
        textual::run_test(StopwatchApp, |pilot| {
            let before = pilot.app().frame_fingerprint();
            pilot.press(&["tab"])?;
            let after = pilot.app().frame_fingerprint();
            assert_ne!(
                before, after,
                "pressing Tab must focus the first Button and apply its focus \
                 styling, changing the rendered frame"
            );
            Ok(())
        })
        .unwrap();
    }
}
