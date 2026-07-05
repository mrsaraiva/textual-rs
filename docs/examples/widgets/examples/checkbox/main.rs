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

    fn on_mount_with_app(&mut self, app: &mut App, ctx: &mut textual::event::WidgetCtx) {
        // Set "Grumman" initially checked.
        if let Ok(handle) = app.query_one_typed::<Checkbox>("#grumman") {
            let _ = handle.update(app, |cb, rctx| {
                cb.set_checked(true, rctx);
            });
        }
        // Set "Novebruns" initially checked.
        if let Ok(handle) = app.query_one_typed::<Checkbox>("#novebruns") {
            let _ = handle.update(app, |cb, rctx| {
                cb.set_checked(true, rctx);
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

    /// LIVENESS: focusing a Checkbox and pressing `space` toggles its `value`,
    /// applying the `-on` class. Python conveys the checked state via the
    /// `toggle--button` component color (`&.-on > .toggle--button`), so the
    /// rendered frame must change when the toggle fires.
    ///
    /// We focus the first Checkbox via the framework focus API rather than relying
    /// on the demo's mount-time `#initial_focus` focus: that id is set through
    /// `ChildDecl::with_id`, which is not yet propagated onto the mounted node
    /// (a separate decl-id/mount wiring gap, outside this relayout/repaint fix).
    /// The toggle-repaint behaviour this probe guards is now LIVE: pressing
    /// `space` flips the checked state AND changes the frame (the `-on` class now
    /// reaches the arena node and triggers a repaint of the recolored button).
    #[test]
    fn space_toggles_focused_checkbox() {
        textual::run_test(CheckboxApp, |pilot| {
            let ids = pilot
                .app()
                .query("Checkbox")
                .map(|q| q.into_ids())
                .unwrap_or_default();
            assert!(!ids.is_empty(), "expected Checkbox widgets in the tree");
            let _ = pilot.app_mut().query_mut("Checkbox").map(|q| q.focus());

            let before_checked = pilot
                .app_mut()
                .with_widget_mut_as::<Checkbox, _>(ids[0], |c| c.checked())
                .unwrap_or(false);
            let before = pilot.app().frame_fingerprint();
            pilot.press(&["space"])?;
            let after = pilot.app().frame_fingerprint();
            let after_checked = pilot
                .app_mut()
                .with_widget_mut_as::<Checkbox, _>(ids[0], |c| c.checked())
                .unwrap_or(false);

            assert_ne!(
                before_checked, after_checked,
                "space on the focused Checkbox must toggle its checked state"
            );
            assert_ne!(
                before, after,
                "toggling the focused Checkbox must change the frame (the -on \
                 component color repaints)"
            );
            Ok(())
        })
        .unwrap();
    }
}
