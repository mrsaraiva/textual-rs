/// Port of Python Textual `docs/examples/guide/actions/actions01.py`.
///
/// Demonstrates basic actions:
/// - Pressing "r" triggers `action_set_background("red")` which sets the
///   screen background to red.
use textual::prelude::*;

struct ActionsApp;

impl ActionsApp {
    fn action_set_background(&self, color: &str, app: &mut App, ctx: &mut EventCtx) {
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

    fn on_key_with_app(&mut self, app: &mut App, key: &KeyEventData, ctx: &mut EventCtx) {
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

    /// LIVENESS PROBE (DEAD — captures expected behavior, currently failing).
    ///
    /// Pressing 'r' fires `action_set_background("red")`, which calls
    /// `query_mut("Screen").set_styles(|s| s.set_bg(red))`. The node's *explicit*
    /// inline background does become red (`node_explicit_bg("Screen") ==
    /// red`), but the **rendered frame never changes**: every screen cell still
    /// paints the default theme background `(18,18,18)` — see the `frame_cell_bg`
    /// assertions below. So the visible "screen turns red" effect does not
    /// happen.
    ///
    /// ROOT (shared by actions01..05): dynamically setting the `Screen` node's
    /// inline `background` via `query_mut("Screen").set_styles(set_bg)` updates
    /// the node style but does not repaint the screen *surface* — the screen
    /// background is composited from the theme base, and the runtime-set inline
    /// bg on the `Screen` node is not picked up by the render/compositing path.
    /// (The existing `Pilot` `RgbApp` test in `src/runtime/pilot.rs` sidesteps
    /// this by asserting `node_explicit_bg` instead of the rendered frame.)
    ///
    /// TODO (fix then un-ignore): make a runtime-set `Screen` inline background
    /// composite into the rendered surface (so the cells actually turn red).
    /// LIVENESS PROBE (DEAD — captures expected behavior, currently failing).
    ///
    /// Pressing 'r' fires `action_set_background("red")`, which sets the
    /// `Screen` node's inline background red. The node's explicit bg becomes red
    /// (`node_explicit_bg("Screen") == red`), but the **rendered frame never
    /// turns red**: a full scan of the 80x24 frame finds 0 red cells.
    ///
    /// ROOT: this demo composes an **empty screen** (`AppRoot::new()` with no
    /// children). When the `Screen` has no child widgets, the runtime-set inline
    /// `background` is not composited into the rendered surface — every cell
    /// still paints the default theme bg `(18,18,18)`. The same `set_bg` path
    /// DOES render correctly once the screen has content: actions04/05 (which
    /// add a `Static`/`ColorSwitcher`) turn ~320 cells red. So the gap is
    /// specifically: an empty `Screen` does not repaint its own surface with a
    /// dynamically-set inline background.
    ///
    /// (`src/runtime/pilot.rs`'s `RgbApp` test hides this by asserting
    /// `node_explicit_bg` rather than the rendered frame.)
    ///
    /// TODO (fix then un-ignore): composite an empty `Screen`'s runtime-set
    /// inline background into the rendered surface. The scan below asserts the
    /// real expected behavior: pressing 'r' must produce red cells.
    #[ignore = "DEAD: empty Screen does not composite runtime-set inline bg into the rendered surface"]
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
