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

    fn on_mount_with_app(&mut self, app: &mut App, ctx: &mut textual::event::WidgetCtx) {
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
    /// LIVE: the on-mount opacity animation progresses, changing the rendered
    /// `#box` over the 2s fade.
    ///
    /// Three fundamentals make this work under `run_test`:
    /// 1. The deterministic manual clock is installed BEFORE `headless_startup`,
    ///    so the on-mount `animate_style` is anchored to the manual timeline and
    ///    stepped only by `advance_clock` — not run to completion by startup's
    ///    settling pump on the wall clock.
    /// 2. `apply_style_value_to_property` plumbs the animator's per-tick
    ///    `opacity` (a `StyleValue::Float`) into the node's inline style, so the
    ///    resolved style reflects the fading value.
    /// 3. Render-time opacity compositing (`apply_widget_opacity_to_segments`)
    ///    blends the widget's cells toward the backdrop, so a changed opacity
    ///    visibly changes the rendered cells (and the frame fingerprint).
    #[test]
    fn liveness_opacity_animation_progresses() {
        textual::run_test(AnimationApp, |pilot| {
            let start = pilot.app().frame_fingerprint();

            // Advance to roughly the middle of the 2s fade.
            for _ in 0..3 {
                pilot.advance_clock(std::time::Duration::from_millis(300))?;
                pilot.pause()?;
            }
            let mid = pilot.app().frame_fingerprint();

            // Advance well past the end of the 2s animation.
            for _ in 0..6 {
                pilot.advance_clock(std::time::Duration::from_millis(300))?;
                pilot.pause()?;
            }
            let end = pilot.app().frame_fingerprint();

            // The fade must visibly change the rendering: the mid-animation frame
            // differs from the fully-opaque start, and the final (transparent)
            // frame differs from both.
            assert_ne!(
                start, mid,
                "opacity fade must change the #box rendering by mid-animation"
            );
            assert_ne!(
                mid, end,
                "opacity fade must keep changing the #box rendering through to the end"
            );
            assert_ne!(
                start, end,
                "the fully-faded #box must differ from the fully-opaque start"
            );
            Ok(())
        })
        .unwrap();
    }
}

fn main() -> textual::Result<()> {
    run_sync(AnimationApp)
}
