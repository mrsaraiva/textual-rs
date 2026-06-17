/// Port of Python Textual `docs/examples/guide/actions/actions02.py`.
///
/// Demonstrates app-level action dispatch triggered from a key handler.
///
/// Python:
///   class ActionsApp(App):
///       def action_set_background(self, color: str) -> None:
///           self.screen.styles.background = color
///
///       async def on_key(self, event: events.Key) -> None:
///           if event.key == "r":
///               await self.run_action("set_background('red')")
///
/// In Rust there is no `run_action(str)` API and no `screen.styles.background`
/// setter. We replicate the observable behaviour — pressing "r" turns the
/// screen background red — using `on_key_with_app` + `DomQueryMut::set_styles`.
use textual::prelude::*;

struct ActionsApp;

impl TextualApp for ActionsApp {
    fn compose(&mut self) -> AppRoot {
        // Python example composes nothing; just an empty screen.
        AppRoot::new()
    }

    fn on_key_with_app(&mut self, app: &mut App, key: &KeyEventData, ctx: &mut EventCtx) {
        if key.name() == "r" {
            // Python: self.run_action("set_background('red')")
            // → calls action_set_background("red")
            // → self.screen.styles.background = "red"
            if let Some(red) = Color::parse("red") {
                let _ = app.query_mut("Screen").map(|q| {
                    q.set_styles(|styles| {
                        styles.set_bg(red);
                    })
                    .refresh()
                });
            }
            ctx.set_handled();
            ctx.request_repaint();
        }
    }
}

fn main() -> textual::Result<()> {
    run_sync(ActionsApp)
}
