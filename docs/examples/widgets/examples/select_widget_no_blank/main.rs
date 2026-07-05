/// Port of Python Textual `docs/examples/widgets/select_widget_no_blank.py`.
///
/// Demonstrates `Select<String>` with `allow_blank=false`:
/// - A `Select` widget populated with lines from a poem (allow_blank=false).
/// - When a selection changes, the app title is updated.
/// - Pressing `s` swaps the options with alternate lines.
///
/// Python: `@on(Select.Changed)` sets `self.title = str(event.value)`.
/// Python: `action_swap` calls `self.query_one(Select).set_options(...)`.
/// Rust: `on_message_with_app` downcasts to `SelectChanged` and calls
/// `app.set_title(label)`.
/// Rust: `on_key_with_app` handles `s` key via `with_query_one_mut_as`.
use textual::prelude::*;

const LINES: &[&str] = &[
    "I must not fear.",
    "Fear is the mind-killer.",
    "Fear is the little-death that brings total obliteration.",
    "I will face my fear.",
    "I will permit it to pass over me and through me.",
];

const ALTERNATE_LINES: &[&str] = &[
    "Twinkle, twinkle, little star,",
    "How I wonder what you are!",
    "Up above the world so high,",
    "Like a diamond in the sky.",
    "Twinkle, twinkle, little star,",
    "How I wonder what you are!",
];

const CSS: &str = r#"
Screen {
    align: center top;
}

Select {
    width: 60;
    margin: 2;
}
"#;

struct SelectApp;

impl TextualApp for SelectApp {
    fn title(&self) -> &'static str {
        "SelectApp"
    }

    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn bindings(&self) -> Vec<BindingDecl> {
        vec![BindingDecl::new("s", "swap", "Swap Select options")]
    }

    fn compose(&mut self) -> AppRoot {
        let options: Vec<(String, String)> = LINES
            .iter()
            .map(|line| (line.to_string(), line.to_string()))
            .collect();
        let select = Select::new(options, "Select").with_allow_blank(false);
        AppRoot::new().with_child(Header::new()).with_child(select)
    }

    fn on_key_with_app(&mut self, app: &mut App, key: &KeyEventData, ctx: &mut textual::event::WidgetCtx) {
        if key.name() == "s" {
            let new_options: Vec<(String, String)> = ALTERNATE_LINES
                .iter()
                .map(|line| (line.to_string(), line.to_string()))
                .collect();
            if let Ok(handle) = app.query_one_typed::<Select<String>>("Select") {
                let _ = handle.update(app, |select, rctx| {
                    select.set_options(new_options, rctx);
                });
            }
            ctx.set_handled();
        }
    }

    fn on_message_with_app(
        &mut self,
        app: &mut App,
        message: &MessageEvent,
        _ctx: &mut textual::event::WidgetCtx,
    ) {
        if let Some(ev) = message.downcast_ref::<SelectChanged>() {
            app.set_title(ev.label.clone());
        }
    }
}

fn main() -> textual::Result<()> {
    run_sync(SelectApp)
}

// ---------------------------------------------------------------------------
// Regression tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_app_composes_without_panic() {
        let mut app = SelectApp;
        let _root = app.compose();
    }

    #[test]
    fn lines_list_has_expected_count() {
        assert_eq!(LINES.len(), 5);
        assert_eq!(LINES[0], "I must not fear.");
    }

    #[test]
    fn alternate_lines_list_has_expected_count() {
        assert_eq!(ALTERNATE_LINES.len(), 6);
        assert_eq!(ALTERNATE_LINES[0], "Twinkle, twinkle, little star,");
    }

    #[test]
    fn select_options_allow_blank_false_auto_selects_first() {
        let options: Vec<(String, String)> = LINES
            .iter()
            .map(|line| (line.to_string(), line.to_string()))
            .collect();
        let sel = Select::new(options, "Select").with_allow_blank(false);
        assert!(!sel.allow_blank());
        assert_eq!(sel.value(), Some(&"I must not fear.".to_string()));
    }

    #[test]
    fn bindings_declare_swap() {
        let app = SelectApp;
        let bindings = app.bindings();
        let keys: Vec<&str> = bindings.iter().map(|b| b.key.as_str()).collect();
        assert!(keys.contains(&"s"), "expected 's' binding for swap");
    }

    /// LIVENESS: pressing `s` invokes `action_swap`, replacing the Select's
    /// options (Dune lines -> Twinkle lines). With `allow_blank=false` the value
    /// auto-snaps to the new first option, so the closed Select's displayed
    /// label changes — the frame must change. A dead `s` binding leaves it
    /// identical.
    #[test]
    fn liveness_swap_changes_options() {
        SelectApp
            .run_test(|pilot| {
                let before = pilot.app().frame_fingerprint();
                pilot.press(&["s"])?;
                let after = pilot.app().frame_fingerprint();
                assert_ne!(before, after, "pressing `s` must swap options (frame changes)");
                // Confirm the underlying value snapped to a new first line.
                let app = pilot.app();
                let value = app
                    .query_one_typed::<Select<String>>("Select")
                    .ok()
                    .and_then(|h| h.read(app, |s| s.value().cloned()).ok())
                    .flatten();
                assert_eq!(
                    value.as_deref(),
                    Some("Twinkle, twinkle, little star,"),
                    "after swap, value snaps to the new first option"
                );
                Ok(())
            })
            .expect("run_test");
    }
}
