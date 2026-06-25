/// Port of Python Textual `docs/examples/events/custom01.py`.
///
/// Demonstrates custom messages in Textual:
/// - `ColorButton` is a custom widget that renders its color string and
///   posts a `ColorSelected` message when clicked.
/// - `ColorApp` handles `ColorSelected` and animates the screen background.
///
/// In Python this uses `self.post_message(self.Selected(self.color))` inside a
/// widget sub-class and an `App.on_color_button_selected` handler.
///
/// In Rust we define a custom message (`ColorSelected`), implement `Widget` for
/// `ColorButton`, and handle the message in `TextualApp::on_message_with_app`.
///
/// NOTE: The Python example animates the background on click. The Rust port
/// sets the background immediately (textual-rs animation API does not yet expose
/// a public `animate` call for arbitrary properties from user code).
use textual::prelude::*;
use textual::style::parse_color_like;

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

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
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
        Static::new(&self.label).render(console, options)
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
    height: 5;
    border: tall white;
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
        ctx: &mut EventCtx,
    ) {
        if let Some(m) = message.downcast_ref::<ColorSelected>() {
            if let Ok(q) = app.query_mut("Screen") {
                q.set_styles(|s| s.set_bg(m.color));
            }
            ctx.request_repaint();
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
    /// the custom `ColorSelected` message, which the app handles by setting the
    /// screen background to the button's color. Asserted via the explicit screen
    /// background (`node_explicit_bg`) — proving the custom message round-trips
    /// from the widget's `on_event` click handler to the app handler.
    #[test]
    fn custom01_color_button_click_sets_background_is_live() {
        fn screen_bg(app: &App) -> Option<Color> {
            app.query_one("Screen").ok().and_then(|n| app.node_explicit_bg(n))
        }
        run_test(ColorApp, |pilot| {
            let before = screen_bg(pilot.app());
            // First ColorButton is "#008080" (teal).
            pilot.click("ColorButton")?;
            let after = screen_bg(pilot.app());
            assert_ne!(before, after, "clicking a ColorButton must change the screen background");
            assert_eq!(
                after,
                parse_color_like("#008080"),
                "clicking the first ColorButton must set the bg to #008080"
            );
            Ok(())
        })
        .expect("custom01 color-button harness should run");
    }
}
