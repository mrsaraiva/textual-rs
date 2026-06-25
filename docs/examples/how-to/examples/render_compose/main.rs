/// Port of Python Textual `docs/examples/how-to/render_compose.py`.
///
/// Demonstrates the "render + compose" pattern where a custom Container
/// overrides `render()` to provide a `LinearGradient` background and
/// `compose()` to add a `Static` child widget on top.
///
/// The `Splash` container overrides `is_active()` to enable fast tick events
/// (~60 fps) and updates a time-driven gradient angle on each tick.
use std::time::{SystemTime, UNIX_EPOCH};

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

fn current_time_secs() -> f32 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f32()
}

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
    angle_deg: f32,
    stops: Vec<(f32, Color)>,
}

impl Splash {
    fn new() -> Self {
        let stops = build_stops();
        let angle_deg = current_time_secs() * 90.0;
        let container = Container::new().with_child(Static::new("Making a splash with Textual!"));
        Self {
            container,
            angle_deg,
            stops,
        }
    }
}

impl Widget for Splash {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let gradient = LinearGradient::new(self.angle_deg, self.stops.clone());
        gradient.render(console, options)
    }

    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        self.container.take_composed_children()
    }

    fn is_active(&self) -> bool {
        true
    }

    fn on_tick(&mut self, _tick: u64) {
        self.angle_deg = current_time_secs() * 90.0;
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

    /// LIVENESS probe (Pilot, headless): the `Splash` container is fast-tick
    /// active (`is_active() == true`); each `on_tick` advances the gradient angle
    /// (`angle_deg = current_time_secs() * 90.0`), so an animated demo should
    /// repaint a different gradient frame over time.
    ///
    /// UNCLEAR under the headless harness — `#[ignore]`d. Two compounding roots:
    /// (1) the per-frame `Widget::on_tick` hook is driven by the *live* event
    /// loop's wall-clock tick cadence (`runtime/event_loop.rs` `last_tick`), and
    /// is NOT invoked by the headless pump — `Pilot::advance_clock` fires
    /// `set_interval`/`set_timer` callbacks (the manual timer clock), but not
    /// `on_tick`; and (2) the angle is derived from wall-clock `SystemTime::now()`
    /// rather than the manual test clock, so even if ticked it would not advance
    /// deterministically. Confirmed: `advance_clock(2s)` leaves the frame
    /// unchanged. This is a harness gap for `on_tick`-animated widgets, not a
    /// demo defect. TODO: drive `on_tick` from the headless pump under
    /// `advance_clock` (and/or have the demo derive its angle from the manual
    /// clock); then drop `#[ignore]`.
    #[ignore = "UNCLEAR: Widget::on_tick is not pumped headless + angle uses wall-clock time"]
    #[test]
    fn render_compose_animation_is_live() {
        run_test(SplashApp, |pilot| {
            let before = pilot.app().frame_fingerprint();
            pilot.advance_clock(std::time::Duration::from_secs(2))?;
            assert_ne!(
                before,
                pilot.app().frame_fingerprint(),
                "the time-driven gradient must repaint a different frame"
            );
            Ok(())
        })
        .expect("render_compose animation harness should run");
    }
}
