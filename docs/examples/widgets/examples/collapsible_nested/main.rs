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
    fn click_inner_title_reveals_body() {
        run_test(CollapsibleApp, |pilot| {
            let ids = pilot.app().query("Collapsible").map(|q| q.into_ids()).unwrap_or_default();
            let inner = ids[1];
            let collapsed_before = pilot
                .app_mut()
                .with_widget_mut_as::<Collapsible, _>(inner, |c| c.is_collapsed())
                .unwrap_or(true);
            let before = pilot.app().frame_fingerprint();
            // The inner Collapsible's title sits inside the outer's expanded body:
            // row 0 = outer border-top, row 1 = outer title, row 2 = Contents
            // padding-top, row 3 = inner title (indented by the Contents padding).
            pilot.click_at(6, 3)?;
            let after = pilot.app().frame_fingerprint();
            let collapsed_after = pilot
                .app_mut()
                .with_widget_mut_as::<Collapsible, _>(inner, |c| c.is_collapsed())
                .unwrap_or(true);
            assert_ne!(
                collapsed_before, collapsed_after,
                "clicking the inner Collapsible title must toggle its collapsed state"
            );
            assert_ne!(
                before, after,
                "clicking the inner Collapsible title must reveal its body and change the frame"
            );
            Ok(())
        })
        .unwrap();
    }
}
