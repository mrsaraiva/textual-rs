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
