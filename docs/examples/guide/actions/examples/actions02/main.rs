/// Port of Python Textual `docs/examples/guide/actions/actions02.py`.
///
/// Demonstrates `App::run_action(str)`: a key handler runs an action *by name*
/// (`"set_background('red')"`) rather than mutating state inline.  The action is
/// resolved against the app namespace and handled by the custom
/// `set_background` action (`on_app_action_str`).
///
/// Python:
/// ```python
/// class ActionsApp(App):
///     def action_set_background(self, color: str) -> None:
///         self.screen.styles.background = color
///     async def on_key(self, event: events.Key) -> None:
///         if event.key == "r":
///             await self.run_action("set_background('red')")
/// ```
use textual::prelude::*;

struct ActionsApp;

impl TextualApp for ActionsApp {
    fn compose(&mut self) -> AppRoot {
        // Python example composes nothing; just an empty screen.
        AppRoot::new()
    }

    fn on_key_with_app(&mut self, app: &mut App, key: &KeyEventData, ctx: &mut EventCtx) {
        if key.name() == "r" {
            // Python: await self.run_action("set_background('red')")
            app.run_action("set_background('red')");
            ctx.set_handled();
        }
    }

    /// Custom app action handler — mirrors Python `action_set_background`.
    fn on_app_action_str(&mut self, app: &mut App, action: &str, ctx: &mut EventCtx) {
        if let Some(parsed) = parse_action(action) {
            if parsed.name == "set_background" {
                if let Some(color_name) = parsed.arguments.first() {
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
}

fn main() -> textual::Result<()> {
    run_sync(ActionsApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_composes_without_panic() {
        let mut app = ActionsApp;
        let _root = app.compose();
    }
}
