/// Port of Python Textual `docs/examples/guide/animator/animation01.py`.
///
/// Demonstrates opacity animation: a Static widget with red background fades
/// from fully opaque to transparent over 2 seconds on mount.
use std::time::Duration;
use textual::event::{AnimationEase, StyleValue};
use textual::prelude::*;

const CSS: &str = r##"
#box {
    background: red;
    color: black;
    padding: 1 2;
}
"##;

struct AnimationApp;

impl TextualApp for AnimationApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Static::new("Hello, World!").id("box"))
    }

    fn on_mount_with_app(&mut self, app: &mut App, ctx: &mut EventCtx) {
        if let Ok(node_id) = app.query_one("#box") {
            ctx.animate_style(
                node_id,
                "opacity",
                StyleValue::Float(100.0),
                StyleValue::Float(0.0),
                Duration::from_secs_f64(2.0),
                AnimationEase::InOutCubic,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn animation_app_composes_without_panic() {
        let mut app = AnimationApp;
        let _root = app.compose();
    }

    /// LIVENESS PROBE (UNCLEAR under the headless harness — see note).
    ///
    /// The on-mount opacity animation should progress, changing the rendered
    /// `#box` over time.
    ///
    /// UNCLEAR ROOT: the animator advances on the wall clock — `Animator::step`
    /// and `enqueue` use `Instant::now()` (see `src/animation.rs`), independent
    /// of the deterministic manual timer clock that `Pilot::advance_clock`
    /// drives. In an instant headless test no real wall time elapses, so the
    /// animation fraction stays ~0 and the frame never changes. This is NOT a
    /// dead demo — it cannot be probed deterministically until the animator is
    /// put on the manual clock (the timer subsystem already is). Flip this
    /// `#[ignore]` once `advance_clock` also advances animations.
    #[test]
    #[ignore = "UNCLEAR: animator runs on Instant::now() (wall clock), not the manual timer clock, so advance_clock cannot deterministically step it headless"]
    fn liveness_opacity_animation_progresses() {
        textual::run_test(AnimationApp, |pilot| {
            let before = pilot.app().frame_fingerprint();
            // Pump several animation frames across a span of the 2s animation.
            for _ in 0..8 {
                pilot.advance_clock(std::time::Duration::from_millis(300))?;
                pilot.pause()?;
            }
            let after = pilot.app().frame_fingerprint();
            assert_ne!(
                before, after,
                "the on-mount opacity animation must change the #box rendering"
            );
            Ok(())
        })
        .unwrap();
    }
}

fn main() -> textual::Result<()> {
    run_sync(AnimationApp)
}
