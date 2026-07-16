/// Port of Python Textual `docs/examples/guide/testing/test_rgb.py`.
///
/// The Python source is a *pytest* test file that exercises `RGBApp`
/// (defined in `rgb.py`) through Textual's `app.run_test()` / `pilot` API:
///   - `test_keys`:    press r/g/b (and unmapped x) and assert screen background changes.
///   - `test_buttons`: click #red / #green / #blue and assert screen background changes.
///
/// textual-rs now exposes an in-process `run_test()` / [`Pilot`] headless test
/// harness, so this file ports BOTH the `RGBApp` (a live interactive app) and
/// the Python pytest tests as real Pilot-driven behavioural tests: each test
/// runs the app headless, simulates key/mouse input via the Pilot, and asserts
/// the resulting app state — exactly mirroring `test_rgb.py`.
use textual::message::ButtonPressed;
use textual::prelude::*;

const CSS: &str = r#"
Screen {
    align: center middle;
}
Horizontal {
    width: auto;
    height: auto;
}
"#;

struct RGBApp;

impl TextualApp for RGBApp {
    fn bindings(&self) -> Vec<BindingDecl> {
        vec![
            BindingDecl::new("r", "switch_color('red')", "Go Red"),
            BindingDecl::new("g", "switch_color('green')", "Go Green"),
            BindingDecl::new("b", "switch_color('blue')", "Go Blue"),
        ]
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(
                Horizontal::new().with_compose(textual::compose![
                    Button::new("Red").id("red"),
                    Button::new("Green").id("green"),
                    Button::new("Blue").id("blue"),
                ]),
            )
            .with_child(Footer::new())
    }

    fn configure(&mut self, app: &mut App) -> Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    /// Handle the `switch_color` action dispatched by the r/g/b key bindings.
    ///
    /// Python: `def action_switch_color(self, color: str) -> None: self.screen.styles.background = color`
    fn on_app_action_str(&mut self, app: &mut App, action: &str, ctx: &mut textual::event::WidgetCtx) {
        if let Ok(parsed) = parse_action(action) {
            if parsed.name == "switch_color" {
                if let Some(color_name) = parsed.arguments.first().and_then(|a| a.as_str()) {
                    if let Some(color) = textual::style::parse_color_like(color_name) {
                        let _ = app.query_mut("Screen").map(|q| {
                            q.set_styles(|styles| styles.set_bg(color));
                        });
                        ctx.set_handled();
                        ctx.request_repaint();
                    }
                }
            }
        }
    }

    /// Handle button presses — mirror Python's `@on(Button.Pressed)` handler:
    ///   `self.action_switch_color(event.button.id)`
    ///
    /// `ButtonPressed.button_id` carries the CSS id set via `.id("red")` etc.,
    /// which we use as the color name to set the screen background.
    fn on_message_with_app(&mut self, app: &mut App, message: &MessageEvent, ctx: &mut textual::event::WidgetCtx) {
        if let Some(bp) = message.downcast_ref::<ButtonPressed>() {
            if let Some(color_name) = &bp.button_id {
                if let Some(color) = textual::style::parse_color_like(color_name) {
                    let _ = app.query_mut("Screen").map(|q| {
                        q.set_styles(|styles| styles.set_bg(color));
                    });
                    ctx.set_handled();
                    ctx.request_repaint();
                }
            }
        }
    }
}

fn main() -> Result<()> {
    if cfg!(test) {
        return Ok(());
    }
    run_sync(RGBApp)
}

// ---------------------------------------------------------------------------
// Behavioural tests — a faithful port of Python's pytest file `test_rgb.py`,
// driven through the real `run_test()` / `Pilot` headless harness.
//
// `app.screen.styles.background` in Python maps to the explicit background of
// the "Screen" node (`AppRoot::style_type() == "Screen"`), read via
// `App::node_explicit_bg`.
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use textual::style::parse_color_like;

    fn screen_bg(app: &App) -> Option<textual::style::Color> {
        let node = app.query_one("Screen").ok()?;
        app.node_explicit_bg(node)
    }

    /// Port of Python `test_keys`: press r/g/b (and unmapped x) and assert the
    /// screen background changes accordingly.
    #[test]
    fn test_keys() {
        textual::run_test(RGBApp, |pilot| {
            pilot.press(&["r"])?;
            assert_eq!(screen_bg(pilot.app()), parse_color_like("red"));

            pilot.press(&["g"])?;
            assert_eq!(screen_bg(pilot.app()), parse_color_like("green"));

            pilot.press(&["b"])?;
            assert_eq!(screen_bg(pilot.app()), parse_color_like("blue"));

            // No binding for x — color must stay blue.
            pilot.press(&["x"])?;
            assert_eq!(screen_bg(pilot.app()), parse_color_like("blue"));
            Ok(())
        })
        .unwrap();
    }

    /// Port of Python `test_buttons`: click #red / #green / #blue and assert the
    /// screen background changes accordingly.
    #[test]
    fn test_buttons() {
        textual::run_test(RGBApp, |pilot| {
            pilot.click("#red")?;
            assert_eq!(screen_bg(pilot.app()), parse_color_like("red"));

            pilot.click("#green")?;
            assert_eq!(screen_bg(pilot.app()), parse_color_like("green"));

            pilot.click("#blue")?;
            assert_eq!(screen_bg(pilot.app()), parse_color_like("blue"));
            Ok(())
        })
        .unwrap();
    }
}
