/// Port of Python Textual `docs/examples/how-to/render_compose.py`.
///
/// Demonstrates the "render + compose" pattern where a custom Container
/// overrides `render()` to provide a `LinearGradient` background and
/// `compose()` to add a `Static` child widget on top.
///
/// The `Splash` container overrides `is_active()` to enable fast tick events
/// (~60 fps) and updates the gradient angle on each tick.
///
/// Python's `render_compose.py` enables `self.auto_refresh = 1 / 30` (a framework
/// refresh timer) and reads `time() * 90` in `render()` — i.e. the angle is driven
/// by the framework's periodic refresh, advancing ~90°/second. We mirror that with
/// the framework tick cadence (`on_tick`, the Rust analogue of `auto_refresh`):
/// the angle is a pure function of the runtime tick counter, exactly like
/// `LoadingIndicator` derives its phase. Driving the angle from the framework
/// clock — rather than wall-clock `SystemTime::now()` — keeps the animation
/// deterministic under the headless Pilot harness, where `advance_clock` /
/// `advance_ticks` step the runtime tick and rotate the gradient reproducibly.
use rich_rs::{Console, ConsoleOptions, Renderable, Segments};
use textual::prelude::*;
use textual::renderables::LinearGradient;
use textual::style::Color;

const COLORS: &[&str] = &[
    "#881177",
    "#aa3355",
    "#cc6666",
    "#ee9944",
    "#eedd00",
    "#99dd55",
    "#44dd88",
    "#22ccbb",
    "#00bbcc",
    "#0099cc",
    "#3366bb",
    "#663399",
];

fn build_stops() -> Vec<(f32, Color)> {
    let n = COLORS.len();
    COLORS
        .iter()
        .enumerate()
        .map(|(i, hex)| {
            let t = i as f32 / (n - 1) as f32;
            let color = parse_hex_color(hex);
            (t, color)
        })
        .collect()
}

fn parse_hex_color(s: &str) -> Color {
    let s = s.trim_start_matches('#');
    let r = u8::from_str_radix(&s[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&s[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&s[4..6], 16).unwrap_or(0);
    Color::rgb(r, g, b)
}

/// Degrees the gradient rotates per runtime tick.
///
/// Python rotates `time() * 90` = 90°/second. The runtime's active tick cadence
/// is ~16ms (~60 fps), so 90°/s ≈ 1.44°/tick. The exact constant is purely
/// visual; what matters is that the angle is a deterministic function of the
/// framework tick rather than wall-clock time.
const DEGREES_PER_TICK: f32 = 1.44;

const CSS: &str = r#"
Splash {
    align: center middle;
}
Static {
    width: 40;
    padding: 2 4;
}
"#;

struct Splash {
    container: Container,
    /// Runtime tick counter — the framework's refresh cadence drives the angle
    /// (Python's `auto_refresh`), so the gradient rotation is deterministic.
    tick: u64,
    stops: Vec<(f32, Color)>,
}

impl Splash {
    fn new() -> Self {
        let stops = build_stops();
        let container = Container::new().with_child(Static::new("Making a splash with Textual!"));
        Self {
            container,
            tick: 0,
            stops,
        }
    }

    /// Current gradient angle, derived purely from the framework tick counter
    /// (the Rust analogue of Python's `time() * 90` driven by `auto_refresh`).
    fn angle_deg(&self) -> f32 {
        self.tick as f32 * DEGREES_PER_TICK
    }
}

impl Widget for Splash {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let gradient = LinearGradient::new(self.angle_deg(), self.stops.clone());
        gradient.render(console, options)
    }

    fn compose(&mut self) -> textual::compose::ComposeResult {
        self.container.compose()
    }

    fn is_active(&self) -> bool {
        true
    }

    fn on_tick(&mut self, tick: u64) {
        // Advance the angle from the framework tick (Python: `auto_refresh`
        // triggers a refresh; `render` reads `time() * 90`). The runtime delivers
        // strictly-increasing ticks both in the live loop and under the headless
        // Pilot harness (`advance_clock` / `advance_ticks`), so the rotation is
        // deterministic in tests.
        self.tick = tick;
    }

    fn style_type(&self) -> &'static str {
        "Splash"
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        self.container.take_node_seed()
    }
}

struct SplashApp;

impl TextualApp for SplashApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Splash::new())
    }
}

fn main() -> textual::Result<()> {
    run_sync(SplashApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splash_app_composes_without_panic() {
        let mut app = SplashApp;
        let _root = app.compose();
    }

    #[test]
    fn build_stops_has_correct_length() {
        let stops = build_stops();
        assert_eq!(stops.len(), COLORS.len());
    }

    #[test]
    fn stops_first_is_zero_last_is_one() {
        let stops = build_stops();
        assert!((stops[0].0 - 0.0).abs() < f32::EPSILON);
        assert!((stops.last().unwrap().0 - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn parse_hex_color_roundtrip() {
        let c = parse_hex_color("#881177");
        assert_eq!(c.r, 0x88);
        assert_eq!(c.g, 0x11);
        assert_eq!(c.b, 0x77);
    }

    #[test]
    fn splash_is_active_returns_true() {
        let splash = Splash::new();
        assert!(splash.is_active());
    }

    #[test]
    fn splash_style_type_is_splash() {
        let splash = Splash::new();
        assert_eq!(splash.style_type(), "Splash");
    }

    #[test]
    fn angle_is_derived_from_framework_tick() {
        let mut splash = Splash::new();
        assert_eq!(splash.angle_deg(), 0.0);
        splash.on_tick(10);
        assert!((splash.angle_deg() - 10.0 * DEGREES_PER_TICK).abs() < f32::EPSILON);
        splash.on_tick(100);
        assert!((splash.angle_deg() - 100.0 * DEGREES_PER_TICK).abs() < f32::EPSILON);
        // Different ticks => different angle => a different rendered gradient.
        assert_ne!(Splash::new().angle_deg(), splash.angle_deg());
    }

    /// LIVENESS probe (Pilot, headless): the `Splash` container is fast-tick
    /// active (`is_active() == true`); each `on_tick` advances the gradient angle
    /// from the framework tick counter (the Rust analogue of Python's
    /// `auto_refresh`-driven `time() * 90`). Because the angle is now driven by the
    /// framework clock — not wall-clock `SystemTime::now()` — `advance_clock`
    /// steps the runtime tick deterministically and the gradient repaints a
    /// different frame. (The headless `advance_clock` pump delivers one runtime
    /// tick per wake, invoking `on_tick` on every active arena node including
    /// `Splash`.)
    #[test]
    fn render_compose_animation_is_live() {
        run_test(SplashApp, |pilot| {
            let before = pilot.app().frame_fingerprint();
            pilot.advance_clock(std::time::Duration::from_secs(2))?;
            assert_ne!(
                before,
                pilot.app().frame_fingerprint(),
                "the framework-tick-driven gradient must repaint a different frame"
            );
            Ok(())
        })
        .expect("render_compose animation harness should run");
    }
}
