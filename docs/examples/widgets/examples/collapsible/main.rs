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
