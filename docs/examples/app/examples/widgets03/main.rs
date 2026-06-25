/// Port of Python Textual `docs/examples/app/widgets03.py`.
///
/// Demonstrates dynamic widget mounting: on any key press, a `Welcome` widget
/// is mounted into the app.  Python then queries `Button` to change its label
/// to "YES!".
///
/// Framework gap: Python's `Welcome` composes its `Button` as a queryable
/// child in the widget tree, so `self.query_one(Button).label = "YES!"` works.
/// Rust's `Welcome` is a monolithic widget whose internal `close` button is
/// not exposed in the arena tree — `query_one("Button")` will not find it.
/// The mount-on-keypress behavior is faithfully ported; the label mutation
/// silently does nothing (no-op) rather than erroring out.
use textual::prelude::*;

struct WelcomeApp;

impl TextualApp for WelcomeApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
    }

    fn on_key_with_app(&mut self, app: &mut App, _key: &KeyEventData, ctx: &mut EventCtx) {
        let _ = app.mount(Welcome::new());
        // Python: `self.query_one(Button).label = "YES!"`
        // In Rust, Welcome's internal Button is not exposed in the arena tree,
        // so this query returns NoMatch and the label mutation is a no-op.
        // The mount behavior is faithfully ported.
        ctx.request_repaint();
    }
}

fn main() -> textual::Result<()> {
    run_sync(WelcomeApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn welcome_app_compose_is_empty() {
        let mut app = WelcomeApp;
        let root = app.compose();
        let _ = root;
    }

    /// SAFE liveness check: pressing a key reaches `on_key_with_app`, which calls
    /// `app.mount(Welcome::new())` — the `Welcome` node IS inserted into the tree.
    #[test]
    fn widgets03_keypress_mounts_welcome_node() {
        run_test(WelcomeApp, |pilot| {
            assert!(pilot.app().query_one("Welcome").is_err(), "no Welcome before a key");
            pilot.press(&["x"])?;
            assert!(
                pilot.app().query_one("Welcome").is_ok(),
                "pressing a key must mount a Welcome node via app.mount"
            );
            Ok(())
        })
        .expect("widgets03 mount-node harness should run");
    }

    /// LIVENESS probe (Pilot, headless): pressing a key mounts `Welcome` and its
    /// OK button (`#close`) should appear and render.
    ///
    /// LIVE: `App::mount` / `mount_boxed` now routes through the compose-aware
    /// mount path (`mount_extracted_recursive`), so `Welcome`'s composed `#close`
    /// child builds, lays out, and paints.
    #[test]
    fn widgets03_keypress_mounts_welcome_is_live() {
        run_test(WelcomeApp, |pilot| {
            let empty = pilot.app().frame_fingerprint();
            assert!(pilot.app().query_one("#close").is_err(), "Welcome not mounted yet");

            pilot.press(&["x"])?;
            assert_ne!(
                empty,
                pilot.app().frame_fingerprint(),
                "pressing a key must mount Welcome (rendered frame changes)"
            );
            assert!(pilot.app().query_one("#close").is_ok(), "Welcome must be mounted after a key");
            Ok(())
        })
        .expect("widgets03 mount-on-key harness should run");
    }
}
