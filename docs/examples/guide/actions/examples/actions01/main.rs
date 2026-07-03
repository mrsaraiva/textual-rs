/// Port of Python Textual `docs/examples/guide/actions/actions01.py`.
///
/// Demonstrates basic actions:
/// - Pressing "r" triggers `action_set_background("red")` which sets the
///   screen background to red.
use textual::prelude::*;

struct ActionsApp;

impl ActionsApp {
    fn action_set_background(&self, color: &str, app: &mut App, ctx: &mut textual::event::WidgetCtx) {
        if let Some(c) = textual::style::parse_color_like(color) {
            if let Ok(q) = app.query_mut("Screen") {
                q.set_styles(|styles| styles.set_bg(c));
            }
            ctx.request_repaint();
        }
    }
}

impl TextualApp for ActionsApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
    }

    fn on_key_with_app(&mut self, app: &mut App, key: &KeyEventData, ctx: &mut textual::event::WidgetCtx) {
        if key.name() == "r" {
            self.action_set_background("red", app, ctx);
            ctx.set_handled();
        }
    }
}

fn main() -> textual::Result<()> {
    run_sync(ActionsApp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use textual::style::parse_color_like;

    fn screen_bg(app: &App) -> Option<textual::style::Color> {
        let node = app.query_one("Screen").ok()?;
        app.node_explicit_bg(node)
    }

    /// LIVENESS PROBE (LIVE).
    ///
    /// Pressing 'r' fires `action_set_background("red")`, which sets the
    /// `Screen` node's inline background red via
    /// `query_mut("Screen").set_styles(|s| s.set_bg(red))`. The node's explicit
    /// bg becomes red AND the rendered surface now turns red: the compositor
    /// re-fills the Screen surface node's rect with the resolved node background
    /// (which includes the runtime-set inline bg), so even an **empty screen**
    /// (`AppRoot::new()` with no children) repaints its surface red.
    ///
    /// Previously this was dead: the Screen surface widget (`AppRoot`) baked its
    /// blank surface from its own seed style, missing the runtime bg set on the
    /// node record — so a full 80x24 scan found 0 red cells. The compositor now
    /// owns this surface composite (see `node_is_screen_surface` in
    /// `src/runtime/render.rs`), mirroring Python's `Screen.styles.background`.
    #[test]
    fn liveness_press_r_sets_red_background() {
        textual::run_test(ActionsApp, |pilot| {
            pilot.press(&["r"])?;
            assert_eq!(
                screen_bg(pilot.app()),
                parse_color_like("red"),
                "pressing 'r' must set the Screen node's explicit background red"
            );
            let red = parse_color_like("red");
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
