/// Port of Python Textual `docs/examples/widgets/select_from_values_widget.py`.
///
/// Demonstrates `Select<String>` populated via `from_values`-style construction:
/// - A `Select` widget populated with lines from a poem (label == value).
/// - When a selection changes, the app title is updated.
///
/// Python: `Select.from_values(LINES)` creates options where label == value.
/// Rust: `Select::new` with identical label/value pairs achieves the same effect.
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
    fn title(&self) -> &'static str {
        "SelectApp"
    }

    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        let options: Vec<(String, String)> = LINES
            .iter()
            .map(|line| (line.to_string(), line.to_string()))
            .collect();
        let select = Select::new(options, "Select").with_allow_blank(true);
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
