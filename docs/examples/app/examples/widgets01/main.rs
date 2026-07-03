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

    fn on_button_pressed(&mut self, _description: &str, ctx: &mut textual::event::WidgetCtx) {
        ctx.request_stop();
    }
}

fn main() -> Result<()> {
    run_sync(WelcomeApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// LIVENESS probe (Pilot, headless): the `Welcome` widget docks an "OK"
    /// button (id `close`). Clicking it fires `on_button_pressed`, which requests
    /// app stop (the Python demo exits). Liveness via `headless_stop_requested()`
    /// since the exit demo's frame is unchanged.
    #[test]
    fn widgets01_ok_button_exits_is_live() {
        run_test(WelcomeApp, |pilot| {
            assert!(!pilot.app().headless_stop_requested(), "no stop before interaction");
            pilot.click("#close")?;
            assert!(
                pilot.app().headless_stop_requested(),
                "clicking the Welcome OK button must request app exit"
            );
            Ok(())
        })
        .expect("widgets01 OK-button harness should run");
    }
}
