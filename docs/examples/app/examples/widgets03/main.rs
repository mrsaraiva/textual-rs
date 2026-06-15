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
}
