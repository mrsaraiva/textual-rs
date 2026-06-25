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

    fn screen_bg(app: &App) -> Option<textual::style::Color> {
        let node = app.query_one("Screen").ok()?;
        app.node_explicit_bg(node)
    }

    /// LIVENESS PROBE (LIVE).
    ///
    /// Pressing 'r' runs the named action `set_background('red')` (via
    /// `run_action`), which the app handles by setting the `Screen` inline bg
    /// red. The node's explicit bg becomes red AND — exactly as in actions01 —
    /// the rendered surface now turns red even on this **empty screen**
    /// (`AppRoot::new()` with no children): the compositor re-fills the Screen
    /// surface node's rect with the resolved node background. See
    /// `node_is_screen_surface` in `src/runtime/render.rs`.
    #[test]
    fn liveness_press_r_runs_action_and_sets_red() {
        textual::run_test(ActionsApp, |pilot| {
            pilot.press(&["r"])?;
            assert_eq!(
                screen_bg(pilot.app()),
                textual::style::parse_color_like("red"),
                "pressing 'r' must run set_background('red') and set the Screen node bg red"
            );
            let red = textual::style::parse_color_like("red");
            let red_cells = (0..24)
                .flat_map(|y| (0..80).map(move |x| (x, y)))
                .filter(|(x, y)| pilot.app().frame_cell_bg(*x, *y) == red)
                .count();
            assert!(
                red_cells > 0,
                "the rendered screen surface must turn red (found {red_cells} red cells)"
            );
            Ok(())
        })
        .unwrap();
    }
}
