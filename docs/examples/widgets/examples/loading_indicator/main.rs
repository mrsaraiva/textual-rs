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
    /// Now LIVE: `Pilot::advance_ticks(n)` delivers `root.on_tick(tick)` n times
    /// with a strictly-increasing counter (mirroring the live loop's per-frame
    /// tick), so the LoadingIndicator's pulsing phase advances and the rendered
    /// frame changes deterministically.
    #[test]
    fn animation_advances_frame() {
        textual::run_test(LoadingApp, |pilot| {
            let before = pilot.app().frame_fingerprint();
            pilot.advance_ticks(8)?;
            let after = pilot.app().frame_fingerprint();
            assert_ne!(
                before, after,
                "advancing ticks must animate the LoadingIndicator and change the frame"
            );
            Ok(())
        })
        .unwrap();
    }
}
