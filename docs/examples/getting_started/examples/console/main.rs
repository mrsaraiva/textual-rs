/// Port of Python Textual `docs/examples/getting_started/console.py`.
///
/// The Python source renders a `DevConsoleHeader` from `textual_dev` — the
/// Textual developer-tools CLI console header widget. That widget is part of
/// the separate `textual-dev` package and is not a standard Textual widget.
///
/// This port renders a plain `Static` label that reproduces the header text
/// that `DevConsoleHeader` displays: "Textual Development Console".
///
/// NON-PROMOTABLE: color-only differences expected (DevConsoleHeader uses
/// custom styling that textual-rs doesn't replicate identically).
use textual::prelude::*;

struct ConsoleApp;

impl TextualApp for ConsoleApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Static::new("Textual Development Console"))
    }
}

fn main() -> textual::Result<()> {
    run_sync(ConsoleApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn console_app_composes_without_panic() {
        let mut app = ConsoleApp;
        let _root = app.compose();
    }
}
