/// Port of Python Textual `docs/examples/app/widgets04.py`.
///
/// Demonstrates dynamic widget mounting at runtime:
/// - On any key press, a `Welcome` widget is mounted into the screen.
/// - The Button inside Welcome is in the arena tree (accessible via `query_one`),
///   so its label can be changed via `app.with_query_one_mut_as::<Button, _>`.
///
/// Python source:
///     class WelcomeApp(App):
///         async def on_key(self) -> None:
///             await self.mount(Welcome())
///             self.query_one(Button).label = "YES!"
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
            // Python: self.query_one(Button).label = "YES!"
            // Welcome's Button is in the arena tree — update it by querying "#close".
            let mut rctx = ReactiveCtx::new(NodeId::default());
            let _ = app.with_query_one_mut_as::<Button, _>("#close", |btn| {
                btn.set_label("YES!".to_string(), &mut rctx);
            });
            ctx.request_repaint();
        }
    }
}

fn main() -> Result<()> {
    run_sync(WelcomeApp::new())
}
