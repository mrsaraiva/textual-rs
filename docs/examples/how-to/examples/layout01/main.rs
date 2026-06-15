/// Port of Python Textual `docs/examples/how-to/layout01.py`.
///
/// Demonstrates a basic two-widget vertical layout using `Placeholder` widgets
/// as stand-ins for a header and footer.
///
/// Python original:
///   - `Header` and `Footer` are `Placeholder` subclasses (no extra CSS).
///   - `TweetScreen` yields `Header(id="Header")` and `Footer(id="Footer")`.
///   - Python Placeholder labels default to `#<id>` when no explicit label is set.
///   - In Python Textual, auto-height Placeholder children in a Screen each receive
///     the full viewport height, so only the first (Header) is visible in the
///     initial viewport — Footer is below the fold.
use textual::prelude::*;

struct LayoutApp;

impl TextualApp for LayoutApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Placeholder::new("#Header"))
            .with_child(Placeholder::new("#Footer"))
    }
}

fn main() -> textual::Result<()> {
    run_sync(LayoutApp)
}
