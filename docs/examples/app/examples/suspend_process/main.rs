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

    /// UNCLEAR under the headless Pilot harness — `#[ignore]`d. ROOT: the
    /// `ctrl+z` binding runs the `suspend_process` action, whose runtime handler
    /// (`App::action_suspend_process`) sends a real `SIGTSTP` to the *current
    /// process*. Pressing `ctrl+z` through `run_test` would suspend the test
    /// runner itself, and the suspend-impl override seam is crate-private, so the
    /// effect cannot be safely or observably exercised headless from the demo
    /// crate. The static check above proves the binding is wired to the trigger.
    /// TODO: expose a public test seam to stub the suspend impl, then drive the
    /// keypress headless and assert the stub fired; drop `#[ignore]`.
    #[ignore = "UNCLEAR: real SIGTSTP suspend is not headless-safe / observable"]
    #[test]
    fn suspend_process_ctrl_z_is_live() {
        run_test(SuspendKeysApp, |pilot| {
            pilot.press(&["ctrl+z"])?;
            Ok(())
        })
        .expect("suspend_process keypress harness should run");
    }
}
