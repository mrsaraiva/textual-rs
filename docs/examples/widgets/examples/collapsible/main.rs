/// Port of Python Textual `docs/examples/widgets/collapsible.py`.
///
/// Demonstrates the `Collapsible` widget:
/// - Three collapsible sections: "Leto" (expanded, Label child),
///   "Jessica" (expanded, Markdown child), "Paul" (collapsed, Markdown child).
/// - `c` collapses all sections.
/// - `e` expands all sections.
use textual::prelude::*;

const LETO: &str = "\
# Duke Leto I Atreides

Head of House Atreides.";

const JESSICA: &str = "
# Lady Jessica

Bene Gesserit and concubine of Leto, and mother of Paul and Alia.
";

const PAUL: &str = "
# Paul Atreides

Son of Leto and Jessica.
";

struct CollapsibleApp;

impl TextualApp for CollapsibleApp {
    fn title(&self) -> &'static str {
        "CollapsibleApp"
    }

    fn bindings(&self) -> Vec<BindingDecl> {
        vec![
            BindingDecl::new("c", "collapse_or_expand_true", "Collapse All"),
            BindingDecl::new("e", "collapse_or_expand_false", "Expand All"),
        ]
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Footer::new())
            .with_child(
                Collapsible::new("Leto")
                    .collapsed(false)
                    .with_child(Label::new(LETO)),
            )
            .with_child(
                Collapsible::new("Jessica")
                    .collapsed(false)
                    .with_child(Markdown::new(JESSICA)),
            )
            .with_child(
                Collapsible::new("Paul")
                    .collapsed(true)
                    .with_child(Markdown::new(PAUL)),
            )
    }

    fn on_app_action_str(&mut self, app: &mut App, action: &str, ctx: &mut EventCtx) {
        let collapse = match action {
            "collapse_or_expand_true" => true,
            "collapse_or_expand_false" => false,
            _ => return,
        };

        // Collect all Collapsible node IDs.
        let ids: Vec<NodeId> = app
            .query("Collapsible")
            .map(|q| q.into_ids())
            .unwrap_or_default();

        for id in ids {
            app.with_widget_mut_as::<Collapsible, _>(id, |c| {
                if c.is_collapsed() != collapse {
                    c.toggle();
                }
            });
        }

        ctx.request_layout_invalidation();
        ctx.request_repaint();
        ctx.set_handled();
    }
}

fn main() -> textual::Result<()> {
    run_sync(CollapsibleApp)
}

#[cfg(test)]
mod liveness {
    use super::*;
    use textual::run_test;

    /// LIVENESS (currently DEAD — see TODO): pressing `c` fires the
    /// `collapse_or_expand_true` action, which collapses every Collapsible.
    /// Two sections start expanded, so collapsing them should hide their bodies
    /// and change the rendered frame.
    ///
    /// The binding -> action -> toggle path IS wired and DOES mutate state: a
    /// diagnostic confirmed all three Collapsibles report `is_collapsed() ==
    /// true` after pressing `c`. But the rendered frame is byte-identical
    /// before and after — the collapsed bodies are not hidden.
    ///
    /// ROOT: a runtime collapse-state change does not relayout/repaint the
    /// Collapsible's body. The action handler requests layout invalidation +
    /// repaint, yet the cached body content/height is not recomputed from the
    /// new `collapsed` reactive. The fix is at the framework level (Collapsible
    /// must re-run layout/visibility of its contents when `collapsed` flips at
    /// runtime); after that this probe flips to LIVE — remove `#[ignore]`.
    #[test]
    #[ignore = "DEAD: runtime collapse mutates state but does not relayout/hide body; flip when fixed"]
    fn collapse_all_binding_changes_frame() {
        run_test(CollapsibleApp, |pilot| {
            let before = pilot.app().frame_fingerprint();
            pilot.press(&["c"])?;
            let after = pilot.app().frame_fingerprint();
            assert_ne!(
                before, after,
                "pressing 'c' must collapse the expanded sections and change the frame"
            );
            Ok(())
        })
        .unwrap();
    }
}
