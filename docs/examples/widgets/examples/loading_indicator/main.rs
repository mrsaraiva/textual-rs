/// Port of Python Textual `docs/examples/widgets/loading_indicator.py`.
///
/// Demonstrates the `LoadingIndicator` widget:
/// - A single LoadingIndicator filling the entire screen
use textual::prelude::*;

struct LoadingApp;

impl TextualApp for LoadingApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(LoadingIndicator::new())
    }
}

fn main() -> textual::Result<()> {
    run_sync(LoadingApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loading_app_composes_without_panic() {
        let mut app = LoadingApp;
        let _root = app.compose();
    }

    #[test]
    fn compose_produces_loading_indicator() {
        let mut app = LoadingApp;
        let root = app.compose();
        assert!(!root.children().is_empty());
    }

    /// UNCLEAR (headless harness gap): the LoadingIndicator animates its pulsing
    /// dots from `on_tick(tick)` — the per-frame animation tick delivered by the
    /// LIVE event loop (`root.on_tick(tick)` in `event_loop.rs`). The widget
    /// itself responds to ticks (see `loading_indicator_renders_dots` /
    /// `li.on_tick(42)` in `src/widgets/loading_indicator.rs`), so it animates
    /// in a real terminal.
    ///
    /// It cannot be driven by the current Pilot: `advance_clock` advances
    /// *timers*, and `headless_pump` runs the *animator* frame, but NEITHER
    /// delivers `root.on_tick(tick)`. So advancing the clock leaves `tick == 0`
    /// and the frame is unchanged.
    ///
    /// ROOT (harness, not demo): the headless pump should deliver the animation
    /// tick (or `Pilot` should expose `advance_ticks(n)`) so tick-driven
    /// animations are testable headless. Once that exists, replace the smoke
    /// check below with a real before/after frame-change assertion and remove
    /// `#[ignore]`.
    #[test]
    #[ignore = "UNCLEAR: on_tick-driven animation is not delivered by headless pump; needs Pilot::advance_ticks"]
    fn animation_advances_frame() {
        use std::time::Duration;
        textual::run_test(LoadingApp, |pilot| {
            let before = pilot.app().frame_fingerprint();
            pilot.advance_clock(Duration::from_millis(500))?;
            let after = pilot.app().frame_fingerprint();
            assert_ne!(
                before, after,
                "advancing the clock must animate the LoadingIndicator and change the frame"
            );
            Ok(())
        })
        .unwrap();
    }
}
