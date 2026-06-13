/// Port of Python Textual `docs/examples/widgets/select_widget.py`.
///
/// Demonstrates `Select<String>`:
/// - A `Select` widget populated with lines from a poem.
/// - When a selection changes, the app title is updated.
///
/// Python: `@on(Select.Changed)` sets `self.title = str(event.value)`.
/// Rust: `on_message_with_app` downcasts to `SelectChanged` and calls
/// `app.set_title(label)`.
use textual::prelude::*;

const LINES: &[&str] = &[
    "I must not fear.",
    "Fear is the mind-killer.",
    "Fear is the little-death that brings total obliteration.",
    "I will face my fear.",
    "I will permit it to pass over me and through me.",
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
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        let options: Vec<(String, String)> = LINES
            .iter()
            .map(|line| (line.to_string(), line.to_string()))
            .collect();
        let select = Select::new(options, "Select a line...");
        AppRoot::new().with_child(Header::new()).with_child(select)
    }

    fn on_message_with_app(
        &mut self,
        app: &mut App,
        message: &MessageEvent,
        _ctx: &mut EventCtx,
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
// Regression tests (DG-02)
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
    fn select_options_match_lines() {
        let options: Vec<(String, String)> = LINES
            .iter()
            .map(|line| (line.to_string(), line.to_string()))
            .collect();
        let sel = Select::new(options, "Select a line...");
        // Options are present.
        assert_eq!(sel.value(), Some(&"I must not fear.".to_string()));
    }

    #[test]
    fn select_changed_event_carries_label() {
        use textual::message::SelectChanged;
        let ev = SelectChanged {
            index: 0,
            label: "I must not fear.".to_string(),
        };
        assert_eq!(ev.label, "I must not fear.");
        assert_eq!(ev.index, 0);
    }
}
