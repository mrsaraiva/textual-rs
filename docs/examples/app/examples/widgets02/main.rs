/// Port of Python Textual `docs/examples/app/widgets02.py`.
///
/// Demonstrates dynamic mounting: the `Welcome` widget is mounted when the
/// user presses any key. Pressing the OK button inside `Welcome` exits the app.
use textual::prelude::*;

struct WelcomeApp;

impl TextualApp for WelcomeApp {
    fn compose(&mut self) -> AppRoot {
        // Start with an empty screen — Welcome is mounted on first key press.
        AppRoot::new()
    }

    fn on_key_with_app(&mut self, app: &mut App, _key: &KeyEventData, _ctx: &mut EventCtx) {
        let _ = app.mount(Welcome::new());
    }

    fn on_button_pressed(&mut self, _description: &str, ctx: &mut EventCtx) {
        ctx.request_stop();
        ctx.set_handled();
    }
}

fn main() -> Result<()> {
    run_sync(WelcomeApp)
}
