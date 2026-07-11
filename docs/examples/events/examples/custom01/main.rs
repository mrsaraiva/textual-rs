/// Port of Python Textual `docs/examples/events/custom01.py`.
///
/// Demonstrates custom messages in Textual:
/// - `ColorButton` is a custom widget that renders its color string and
///   posts a `ColorSelected` message when clicked.
/// - `ColorApp` handles `ColorSelected` and animates the screen background
///   to the selected color over 0.5s (Python:
///   `self.screen.styles.animate("background", message.color, duration=0.5)`).
///
/// In Python this uses `self.post_message(self.Selected(self.color))` inside a
/// widget sub-class and an `App.on_color_button_selected` handler; the button's
/// per-instance styles (translucent white background + a `tall` border in the
/// button's own color) are set inline in `on_mount`.
///
/// In Rust we define a custom message (`ColorSelected`), implement `Widget` for
/// `ColorButton` (its per-instance border color comes from the inline
/// `Widget::style()` hook — the analogue of Python's `on_mount` inline styles),
/// and handle the message in `TextualApp::on_message_with_app` with
/// `ctx.animate_style(screen, "bg", ...)`.
use std::time::Duration;

use textual::event::{AnimationEase, StyleValue};
use textual::prelude::*;
use textual::style::{BorderEdge, BorderType, parse_color_like};

// ---------------------------------------------------------------------------
// Custom message
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct ColorSelected {
    color: Color,
}

textual::impl_message!(ColorSelected);

// ---------------------------------------------------------------------------
// ColorButton widget
// ---------------------------------------------------------------------------

struct ColorButton {
    color: Color,
    label: String,
}

impl ColorButton {
    fn new(hex: &str) -> Self {
        let color = parse_color_like(hex).unwrap_or(Color::rgb(0x80, 0x80, 0x80));
        Self {
            color,
            label: hex.to_string(),
        }
    }
}

impl Widget for ColorButton {
    fn style_type(&self) -> &'static str {
        "ColorButton"
    }

    /// Inline styles, mirroring Python's `on_mount`:
    /// `self.styles.border = ("tall", self.color)` — the per-instance border
    /// color that CSS (shared across all buttons) cannot express.
    fn style(&self) -> Option<Style> {
        let mut style = Style::new();
        let edge = BorderEdge::Edge {
            border_type: BorderType::Tall,
            color: self.color,
        };
        style.border_top = edge;
        style.border_right = edge;
        style.border_bottom = edge;
        style.border_left = edge;
        Some(style)
    }

    fn on_event(&mut self, event: &Event, ctx: &mut textual::event::WidgetCtx) {
        if matches!(event, Event::Click(_)) {
            ctx.post_message(ColorSelected { color: self.color });
            ctx.set_handled();
        }
    }

    fn render(
        &self,
        console: &rich_rs::Console,
        options: &rich_rs::ConsoleOptions,
    ) -> rich_rs::Segments {
        let text = format!(
            "Color({}, {}, {})",
            self.color.r, self.color.g, self.color.b
        );
        Static::new(text).render(console, options)
    }
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

const CSS: &str = r#"
Screen {
    layout: vertical;
}

ColorButton {
    margin: 1 2;
    content-align: center middle;
    height: auto;
    background: #ffffff33;
}
"#;

struct ColorApp;

impl TextualApp for ColorApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(ColorButton::new("#008080"))
            .with_child(ColorButton::new("#808000"))
            .with_child(ColorButton::new("#E9967A"))
            .with_child(ColorButton::new("#121212"))
    }

    fn on_message_with_app(
        &mut self,
        app: &mut App,
        message: &MessageEvent,
        ctx: &mut textual::event::WidgetCtx,
    ) {
        if let Some(m) = message.downcast_ref::<ColorSelected>() {
            if let Ok(screen) = app.query_one("Screen") {
                // Animate from the current background (a previous selection's
                // explicit bg, or the theme `$background`) to the new color —
                // Python: `styles.animate("background", color, duration=0.5)`.
                let from = app
                    .node_explicit_bg(screen)
                    .or_else(|| parse_color_like("$background"))
                    .unwrap_or(Color::rgb(0x12, 0x12, 0x12));
                ctx.animate_style(
                    screen,
                    "bg",
                    StyleValue::Color(from),
                    StyleValue::Color(m.color),
                    Duration::from_millis(500),
                    AnimationEase::InOutCubic,
                );
            }
            ctx.set_handled();
        }
    }
}

fn main() -> textual::Result<()> {
    run_sync(ColorApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_app_composes_without_panic() {
        let mut app = ColorApp;
        let _root = app.compose();
    }

    #[test]
    fn color_button_parses_hex() {
        let btn = ColorButton::new("#008080");
        assert_eq!(btn.label, "#008080");
    }

    /// LIVENESS probe (Pilot, headless): clicking a custom `ColorButton` posts
    /// the custom `ColorSelected` message, which the app handles by animating
    /// the screen background to the button's color over 0.5s. Advancing the
    /// test clock past the animation must settle the explicit screen bg on the
    /// target — proving the custom message round-trips from the widget's
    /// `on_event` click handler to the app handler and into the animator.
    #[test]
    fn custom01_color_button_click_sets_background_is_live() {
        fn screen_bg(app: &App) -> Option<Color> {
            app.query_one("Screen").ok().and_then(|n| app.node_explicit_bg(n))
        }
        run_test(ColorApp, |pilot| {
            let before = screen_bg(pilot.app());
            // First ColorButton is "#008080" (teal).
            pilot.click("ColorButton")?;
            // Let the 0.5s bg animation run to completion on the test clock.
            pilot.advance_clock(Duration::from_millis(700))?;
            let after = screen_bg(pilot.app());
            assert_ne!(before, after, "clicking a ColorButton must change the screen background");
            assert_eq!(
                after,
                parse_color_like("#008080"),
                "clicking the first ColorButton must animate the bg to #008080"
            );
            Ok(())
        })
        .expect("custom01 color-button harness should run");
    }
}
