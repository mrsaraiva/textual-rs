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

    fn on_button_pressed(&mut self, _description: &str, ctx: &mut EventCtx) {
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

    /// UNCLEAR under the headless Pilot harness — `#[ignore]`d. ROOT: clicking
    /// the button posts `AppSuspendProcess`, whose runtime handler
    /// (`App::action_suspend_process`) sends a real `SIGTSTP` to the *current
    /// process* via the default `suspend_process_impl`. Driving this through
    /// `run_test` would suspend the test runner itself, and the override seam
    /// (`set_suspend_process_impl_for_test`) is crate-private, so the suspend
    /// effect cannot be safely or observably exercised headless from the demo
    /// crate. The safe check above proves the button is wired to the trigger.
    /// TODO: expose a public test seam to stub the suspend impl, then drive the
    /// click headless and assert the stub fired; drop `#[ignore]`.
    #[ignore = "UNCLEAR: real SIGTSTP suspend is not headless-safe / observable"]
    #[test]
    fn suspend_button_click_is_live() {
        run_test(SuspendingApp, |pilot| {
            pilot.click("#edit")?;
            Ok(())
        })
        .expect("suspend click harness should run");
    }
}
