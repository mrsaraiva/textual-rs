/// Port of Python Textual `docs/examples/widgets/collapsible_custom_symbol.py`.
///
/// Demonstrates `Collapsible` with custom collapsed/expanded symbols:
/// - Two collapsible sections inside a `Horizontal`.
/// - Both use `collapsed_symbol=">>>"` and `expanded_symbol="v"`.
/// - First is collapsed (default), second is expanded.
use textual::prelude::*;

struct CollapsibleApp;

impl TextualApp for CollapsibleApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(
            Horizontal::new()
                .with_child(
                    Collapsible::new("Toggle")
                        .collapsed_symbol(">>>")
                        .expanded_symbol("v")
                        .with_child(Label::new("Hello, world.")),
                )
                .with_child(
                    Collapsible::new("Toggle")
                        .collapsed_symbol(">>>")
                        .expanded_symbol("v")
                        .collapsed(false)
                        .with_child(Label::new("Hello, world.")),
                ),
        )
    }
}

fn main() -> textual::Result<()> {
    run_sync(CollapsibleApp)
}
