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
