/// Port of Python Textual `docs/examples/app/widgets01.py`.
///
/// Demonstrates the `Welcome` widget as the only composed child.
/// Pressing the "OK" button exits the app.
use textual::prelude::*;

struct WelcomeApp;

impl TextualApp for WelcomeApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Welcome::new())
    }

    fn on_button_pressed(&mut self, _description: &str, ctx: &mut EventCtx) {
        ctx.request_stop();
    }
}

fn main() -> Result<()> {
    run_sync(WelcomeApp)
}
