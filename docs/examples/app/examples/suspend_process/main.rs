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
