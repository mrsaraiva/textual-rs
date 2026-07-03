/// Port of Python Textual `docs/examples/guide/animator/animation01_static.py`.
///
/// Demonstrates the easing function used by the animator by showing three
/// static snapshots of what an opacity animation would look like at
/// t=0.25, t=0.50, and t=0.75 (using the default in-out-cubic easing).
///
/// Press keys 1, 2, 3 to apply each opacity level to the box.
///
/// Framework notes:
/// - Python Textual's `EASING[DEFAULT_EASING]` is `in_out_cubic`.
/// - Opacity values are computed as `1 - ease(t)` for t in {0.25, 0.5, 0.75}
///   and stored as percentages (0-100).
/// - Inline style changes at runtime are done via `app.query_mut(selector).set_styles(...)`.
use textual::prelude::*;

const CSS: &str = r##"
#box {
    background: red;
    color: black;
    padding: 1 2;
}
"##;

/// Apply the in-out-cubic easing function (Python Textual's DEFAULT_EASING).
fn in_out_cubic(x: f32) -> f32 {
    if x < 0.5 {
        4.0 * x * x * x
    } else {
        let t = -2.0 * x + 2.0;
        1.0 - t * t * t / 2.0
    }
}

/// Compute `1 - ease(t)` as an opacity percentage (0-100).
fn eased_opacity_pct(t: f32) -> u8 {
    let opacity = 1.0 - in_out_cubic(t);
    (opacity * 100.0).round() as u8
}

struct AnimationApp;

impl TextualApp for AnimationApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Static::new("Hello, World!").id("box"))
    }

    fn on_key_with_app(&mut self, app: &mut App, key: &KeyEventData, ctx: &mut textual::event::WidgetCtx) {
        let opacity_pct: Option<u8> = match key.name() {
            "1" => Some(eased_opacity_pct(0.25)),
            "2" => Some(eased_opacity_pct(0.50)),
            "3" => Some(eased_opacity_pct(0.75)),
            _ => None,
        };

        if let Some(pct) = opacity_pct {
            if let Ok(q) = app.query_mut("#box") {
                q.set_styles(|styles| {
                    styles.style = std::mem::take(&mut styles.style).opacity(pct);
                });
            }
            ctx.set_handled();
            ctx.request_repaint();
        }
    }
}

fn main() -> Result<()> {
    run_sync(AnimationApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_out_cubic_boundary_conditions() {
        assert!((in_out_cubic(0.0) - 0.0).abs() < 1e-6);
        assert!((in_out_cubic(1.0) - 1.0).abs() < 1e-6);
        assert!((in_out_cubic(0.5) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn eased_opacity_snapshots() {
        // t=0.25: ease=0.0625, opacity=93.75% -> 94
        assert_eq!(eased_opacity_pct(0.25), 94);
        // t=0.50: ease=0.5, opacity=50%
        assert_eq!(eased_opacity_pct(0.50), 50);
        // t=0.75: ease=0.9375, opacity=6.25% -> 6
        assert_eq!(eased_opacity_pct(0.75), 6);
    }

    #[test]
    fn animation_app_composes_without_panic() {
        let mut app = AnimationApp;
        let _root = app.compose();
    }

    /// LIVENESS PROBE — pressing "2" must apply the eased 50% opacity to `#box`
    /// via the node-level `query_mut(...).set_styles(...)` path, changing the
    /// rendered frame. A dead demo (unwired key handler) leaves the frame
    /// identical and fails this gate.
    #[test]
    fn liveness_key_applies_opacity() {
        textual::run_test(AnimationApp, |pilot| {
            let before = pilot.app().frame_fingerprint();
            pilot.press(&["2"])?;
            let after = pilot.app().frame_fingerprint();
            assert_ne!(
                before, after,
                "pressing 2 must apply the eased opacity to #box"
            );
            Ok(())
        })
        .unwrap();
    }
}
