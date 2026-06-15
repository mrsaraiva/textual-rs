/// Port of Python Textual `docs/examples/app/widgets04.py`.
///
/// Demonstrates dynamic widget mounting at runtime:
/// - On any key press, a `Welcome` widget is mounted into the screen.
/// - The Python original also changes `self.query_one(Button).label = "YES!"`,
///   updating the close button label inside Welcome.
///
/// Python source:
///     from textual.app import App
///     from textual.widgets import Button, Welcome
///
///     class WelcomeApp(App):
///         async def on_key(self) -> None:
///             await self.mount(Welcome())
///             self.query_one(Button).label = "YES!"
///
/// Note: In Rust, `Welcome`'s Button is an internal field (not separately
/// tree-mounted), so it is not reachable via `app.query_one("Button")`.
/// The Welcome widget is mounted faithfully; the label change is a
/// best-effort adaptation (Welcome does not expose a public label setter).
use textual::prelude::*;

struct WelcomeApp {
    welcome_mounted: bool,
}

impl WelcomeApp {
    fn new() -> Self {
        Self {
            welcome_mounted: false,
        }
    }
}

impl TextualApp for WelcomeApp {
    fn title(&self) -> &'static str {
        "WelcomeApp"
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
    }

    fn on_key_with_app(&mut self, app: &mut App, _key: &KeyEventData, ctx: &mut EventCtx) {
        if !self.welcome_mounted {
            self.welcome_mounted = true;
            let _ = app.mount(Welcome::new());
            ctx.request_repaint();
        }
    }
}

fn main() -> Result<()> {
    run_sync(WelcomeApp::new())
}
