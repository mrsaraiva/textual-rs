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

    /// LIVENESS: clicking a Collapsible title toggles its collapsed state, which
    /// should reveal/hide its body and change the rendered frame.
    ///
    /// NOTE: see the sibling `collapsible` demo. Clicking the first Collapsible
    /// title DOES toggle its state — a diagnostic confirmed `is_collapsed()`
    /// flips from `true` to `false` after `click_at(3, 0)`. But the rendered
    /// frame is byte-identical before and after: the runtime collapse-state
    /// change does not relayout/repaint the body.
    ///
    /// ROOT: same as `collapsible` — Collapsible does not re-run its body
    /// layout/visibility when `collapsed` flips at runtime. Fix at the framework
    /// level, then remove `#[ignore]` to flip this guard to LIVE.
    #[test]
    #[ignore = "DEAD: runtime collapse mutates state but does not relayout/repaint body; flip when fixed"]
    fn click_title_toggles_body() {
        run_test(CollapsibleApp, |pilot| {
            let before = pilot.app().frame_fingerprint();
            // First Collapsible header sits at the top-left (rect 0,0..39,2).
            pilot.click_at(3, 0)?;
            let after = pilot.app().frame_fingerprint();
            assert_ne!(
                before, after,
                "clicking a Collapsible title must toggle its body and change the frame"
            );
            Ok(())
        })
        .unwrap();
    }
}
