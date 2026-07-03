/// Port of Python Textual `docs/examples/app/suspend.py`.
///
/// Demonstrates the app suspend/resume capability:
/// - A single Button("Open the editor", id="edit") is rendered.
/// - When the button is pressed, the TUI suspends so an external editor
///   (vim) can run in the terminal, then resumes when the editor exits.
///
/// Python uses `with self.suspend(): system("vim")`.
/// Rust posts `AppSuspendProcess` which sends SIGTSTP to the process
/// (POSIX suspend — user brings it back with `fg`).  The visual layout
/// and button are identical; the exact suspend semantics differ slightly.
use textual::prelude::*;

struct SuspendingApp;

impl TextualApp for SuspendingApp {
    fn title(&self) -> &'static str {
        "SuspendingApp"
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Button::new("Open the editor").id("edit"))
    }

    fn on_button_pressed(&mut self, _description: &str, ctx: &mut textual::event::WidgetCtx) {
        ctx.post_message(AppSuspendProcess);
        ctx.set_handled();
    }
}

fn main() -> textual::Result<()> {
    run_sync(SuspendingApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// SAFE liveness check: the "Open the editor" button's handler runs and marks
    /// the press handled (it posts the `AppSuspendProcess` suspend trigger).
    /// Verified directly via `on_button_pressed` so we never actually suspend the
    /// test process.
    #[test]
    fn suspend_button_handler_runs() {
        let mut app = SuspendingApp;
        let mut ctx = EventCtx::default();
        app.on_button_pressed("Open the editor", &mut ctx);
        assert!(
            ctx.handled(),
            "the button handler must run and handle the press (posting AppSuspendProcess)"
        );
    }

    /// Now LIVE: under the headless `Pilot` harness `action_suspend_process`
    /// records the request (instead of sending a real `SIGTSTP` that would
    /// suspend the test runner) and exposes it via `App::headless_suspend_count`.
    /// Clicking the button posts `AppSuspendProcess`, which the runtime routes to
    /// `action_suspend_process`, bumping the count — an observable, headless-safe
    /// signal that the suspend trigger fired.
    #[test]
    fn suspend_button_click_is_live() {
        run_test(SuspendingApp, |pilot| {
            assert_eq!(pilot.app().headless_suspend_count(), 0, "no suspend before click");
            pilot.click("#edit")?;
            assert_eq!(
                pilot.app().headless_suspend_count(),
                1,
                "clicking the button must request a process suspend"
            );
            Ok(())
        })
        .expect("suspend click harness should run");
    }
}
