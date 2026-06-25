/// Port of Python Textual `docs/examples/app/suspend_process.py`.
///
/// Demonstrates the `suspend_process` built-in action:
/// - `Ctrl+Z` suspends the application process (SIGTSTP on Unix).
use textual::prelude::*;

struct SuspendKeysApp;

impl TextualApp for SuspendKeysApp {
    fn bindings(&self) -> Vec<BindingDecl> {
        vec![BindingDecl::new("ctrl+z", "suspend_process", "Suspend")]
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Label::new("Press Ctrl+Z to suspend!"))
    }
}

fn main() -> Result<()> {
    run_sync(SuspendKeysApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// SAFE liveness check: the app binds `ctrl+z` to the `suspend_process`
    /// built-in action. Verified statically so we never actually suspend the
    /// test process.
    #[test]
    fn suspend_process_binds_ctrl_z() {
        let app = SuspendKeysApp;
        let bindings = app.bindings();
        assert!(
            bindings
                .iter()
                .any(|b| b.key == "ctrl+z" && b.action == "suspend_process"),
            "ctrl+z must be bound to suspend_process"
        );
    }

    /// Now LIVE: under the headless `Pilot` harness `action_suspend_process`
    /// records the request (instead of sending a real `SIGTSTP`) and exposes it
    /// via `App::headless_suspend_count`. Pressing `ctrl+z` runs the
    /// `suspend_process` action, bumping the count — an observable, headless-safe
    /// signal that the binding fired without suspending the test runner.
    #[test]
    fn suspend_process_ctrl_z_is_live() {
        run_test(SuspendKeysApp, |pilot| {
            assert_eq!(pilot.app().headless_suspend_count(), 0, "no suspend before ctrl+z");
            pilot.press(&["ctrl+z"])?;
            assert_eq!(
                pilot.app().headless_suspend_count(),
                1,
                "pressing ctrl+z must request a process suspend"
            );
            Ok(())
        })
        .expect("suspend_process keypress harness should run");
    }
}
