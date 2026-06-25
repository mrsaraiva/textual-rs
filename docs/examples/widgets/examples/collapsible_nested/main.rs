/// Port of Python Textual `docs/examples/widgets/collapsible_nested.py`.
///
/// Demonstrates nested `Collapsible` widgets:
/// - Outer collapsible is expanded (collapsed=False).
/// - Inner collapsible is collapsed (default).
/// - Inner collapsible contains a Label("Hello, world.").
use textual::prelude::*;

struct CollapsibleApp;

impl TextualApp for CollapsibleApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(
            Collapsible::new("Toggle")
                .collapsed(false)
                .with_child(Collapsible::new("Toggle").with_child(Label::new("Hello, world."))),
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

    /// LIVENESS: the outer Collapsible is expanded and contains an inner
    /// (collapsed) Collapsible; clicking the inner title toggles it, which
    /// should reveal its Label("Hello, world.") body and change the frame.
    ///
    /// NOTE: same root as the `collapsible` / `collapsible_custom_symbol` demos
    /// — a runtime collapse-state change mutates state but does not
    /// relayout/repaint the body, so the frame does not change. Kept as the
    /// guard that flips to LIVE once that framework gap is fixed.
    #[test]
    #[ignore = "DEAD: runtime collapse mutates state but does not relayout/repaint body; flip when fixed"]
    fn click_inner_title_reveals_body() {
        run_test(CollapsibleApp, |pilot| {
            let before = pilot.app().frame_fingerprint();
            // The inner Collapsible's title is indented one level, on the second row.
            pilot.click_at(6, 1)?;
            let after = pilot.app().frame_fingerprint();
            assert_ne!(
                before, after,
                "clicking the inner Collapsible title must reveal its body and change the frame"
            );
            Ok(())
        })
        .unwrap();
    }
}
