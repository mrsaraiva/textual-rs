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

    /// LIVENESS (currently DEAD — see TODO): "Kaitain" is focused on mount;
    /// pressing space should toggle its checked state. Python conveys the
    /// checked state via the `toggle--button` component color (the `X` glyph
    /// recolors via the `-on` class). The toggle fires at the widget level
    /// (see `checkbox_emits_message_on_toggle` in `src/widgets/checkbox.rs`),
    /// but in the running app the `-on` recolor is NOT reflected in the
    /// rendered frame: pressing `tab` changes the frame (focus moves) yet a
    /// subsequent `space` produces an identical fingerprint.
    ///
    /// ROOT: the runtime toggle does not re-resolve the focused Checkbox's
    /// component styles (`&.-on > .toggle--button`) into the frame — the `-on`
    /// class change does not invalidate/repaint the cell color. The fix is at
    /// the framework level (component-style re-resolution on reactive state
    /// change), after which this probe flips to LIVE: remove `#[ignore]`.
    #[test]
    #[ignore = "DEAD: focused-Checkbox toggle does not repaint -on component color; flip when fixed"]
    fn space_toggles_focused_checkbox() {
        textual::run_test(CheckboxApp, |pilot| {
            let before = pilot.app().frame_fingerprint();
            pilot.press(&["space"])?;
            let after = pilot.app().frame_fingerprint();
            assert_ne!(
                before, after,
                "space on the focused Checkbox must toggle it and change the frame"
            );
            Ok(())
        })
        .unwrap();
    }
}
