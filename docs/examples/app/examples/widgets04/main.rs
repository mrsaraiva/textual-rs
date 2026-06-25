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

#[cfg(test)]
mod tests {
    use super::*;

    /// SAFE liveness check: pressing a key reaches `on_key_with_app`, which calls
    /// `app.mount(Welcome::new())` — the `Welcome` node IS inserted into the tree.
    #[test]
    fn widgets04_keypress_mounts_welcome_node() {
        run_test(WelcomeApp::new(), |pilot| {
            assert!(pilot.app().query_one("Welcome").is_err(), "no Welcome before a key");
            pilot.press(&["k"])?;
            assert!(
                pilot.app().query_one("Welcome").is_ok(),
                "pressing a key must mount a Welcome node via app.mount"
            );
            Ok(())
        })
        .expect("widgets04 mount-node harness should run");
    }

    /// LIVENESS probe (Pilot, headless): pressing any key mounts a `Welcome`
    /// widget AND relabels its (arena-tree) button to "YES!"
    /// (`app.with_query_one_mut_as::<Button>("#close", ...)`), the full Python
    /// behavior (`self.query_one(Button).label = "YES!"`).
    ///
    /// LIVE: `App::mount` / `mount_boxed` now routes through the compose-aware
    /// mount path (`mount_extracted_recursive`), so `Welcome`'s composed `#close`
    /// button enters the arena tree. The relabel's `query_one("#close")` finds it
    /// and the widget composes + paints.
    #[test]
    fn widgets04_keypress_mounts_and_relabels_is_live() {
        run_test(WelcomeApp::new(), |pilot| {
            let empty = pilot.app().frame_fingerprint();
            assert!(pilot.app().query_one("#close").is_err(), "Welcome not mounted yet");

            pilot.press(&["k"])?;
            assert_ne!(
                empty,
                pilot.app().frame_fingerprint(),
                "pressing a key must mount Welcome (rendered frame changes)"
            );

            let label = pilot
                .app()
                .query_one_typed::<Button>("#close")
                .ok()
                .and_then(|h| h.read(pilot.app(), |b| b.label().to_string()).ok());
            assert_eq!(
                label.as_deref(),
                Some("YES!"),
                "the mounted Welcome button must be relabeled to YES!"
            );
            Ok(())
        })
        .expect("widgets04 mount-and-relabel harness should run");
    }
}
