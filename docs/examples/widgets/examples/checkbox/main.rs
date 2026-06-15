/// Port of Python Textual `docs/examples/widgets/checkbox.py`.
///
/// Demonstrates the `Checkbox` widget:
/// - Eight checkboxes inside a VerticalScroll
/// - Two start checked ("Grumman" and "Novebruns")
/// - "Kaitain" receives initial focus on mount
use textual::prelude::*;

const CSS: &str = r#"
Screen {
    align: center middle;
}

VerticalScroll {
    width: auto;
    height: auto;
    background: $boost;
    padding: 2;
}
"#;

struct CheckboxApp;

impl TextualApp for CheckboxApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        // ChildDecl::from(widget).with_id(...) assigns a CSS id to a widget
        // that does not have its own with_id() builder method.
        let vs = VerticalScroll::new().with_compose(vec![
            ChildDecl::from(Checkbox::new("Arrakis :sweat:")),
            ChildDecl::from(Checkbox::new("Caladan")),
            ChildDecl::from(Checkbox::new("Chusuk")),
            ChildDecl::from(Checkbox::new("[b]Giedi Prime[/b]")),
            ChildDecl::from(Checkbox::new("[magenta]Ginaz[/]")),
            ChildDecl::from(Checkbox::new("Grumman")).with_id("grumman"),
            ChildDecl::from(Checkbox::new("Kaitain")).with_id("initial_focus"),
            ChildDecl::from(Checkbox::new("Novebruns")).with_id("novebruns"),
        ]);
        AppRoot::new().with_child(vs)
    }

    fn on_mount_with_app(&mut self, app: &mut App, ctx: &mut EventCtx) {
        // Set "Grumman" initially checked.
        if let Ok(nid) = app.query_one("#grumman") {
            let mut rctx = ReactiveCtx::new(nid);
            let _ = app.with_query_one_mut_as::<Checkbox, _>("#grumman", |cb| {
                cb.set_checked(true, &mut rctx);
            });
        }
        // Set "Novebruns" initially checked.
        if let Ok(nid) = app.query_one("#novebruns") {
            let mut rctx = ReactiveCtx::new(nid);
            let _ = app.with_query_one_mut_as::<Checkbox, _>("#novebruns", |cb| {
                cb.set_checked(true, &mut rctx);
            });
        }
        // Focus "Kaitain" on mount.
        let _ = app.query_mut("#initial_focus").map(|q| q.focus());
        ctx.request_repaint();
    }
}

fn main() -> textual::Result<()> {
    run_sync(CheckboxApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkbox_app_composes_without_panic() {
        let mut app = CheckboxApp;
        let _root = app.compose();
    }

    #[test]
    fn compose_produces_vertical_scroll_with_children() {
        let mut app = CheckboxApp;
        let root = app.compose();
        // AppRoot should have at least one child (the VerticalScroll).
        assert!(!root.children().is_empty());
    }
}
