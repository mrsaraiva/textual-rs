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

#[cfg(test)]
mod liveness {
    use super::*;
    use textual::run_test;

    /// LIVENESS: clicking a Collapsible title toggles its collapsed state,
    /// revealing/hiding its body and changing the rendered frame.
    ///
    /// Geometry (Python parity): row 0 is the Collapsible's `border-top: hkey`
    /// rule; the clickable `CollapsibleTitle` sits on row 1. Clicking the border
    /// row is a no-op in Python too (only `CollapsibleTitle` handles clicks),
    /// so this probe clicks (3, 1) — the ">>> Toggle" title of the first
    /// (collapsed) section.
    #[test]
    fn click_title_toggles_body() {
        run_test(CollapsibleApp, |pilot| {
            let before = pilot.app().frame_fingerprint();
            pilot.click_at(3, 1)?;
            let after = pilot.app().frame_fingerprint();
            assert_ne!(
                before, after,
                "clicking a Collapsible title must toggle its body and change the frame"
            );
            // Expanding the first section swaps its symbol (">>>" -> "v") and
            // reveals its body, so BOTH sections now show "Hello, world.".
            let frame = pilot.app().frame_plain_text();
            assert_eq!(
                frame.matches("Hello, world.").count(),
                2,
                "expanding the first section must reveal its body:\n{frame}"
            );
            assert!(
                !frame.contains(">>>"),
                "the expanded section must show the expanded symbol:\n{frame}"
            );
            Ok(())
        })
        .unwrap();
    }
}
