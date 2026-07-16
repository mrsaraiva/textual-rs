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
        // Like Python `Select.from_values(...)`, allow_blank defaults to true.
        let select = Select::new(options, "Select");
        AppRoot::new().with_child(Header::new()).with_child(select)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_app_composes_without_panic() {
        let mut app = SelectApp;
        let _root = app.compose();
    }

    /// LIVENESS: focus the Select and press enter to expand its overlay. The
    /// option list pops over the screen, changing the rendered frame. A dead
    /// Select (toggle not routed) leaves the closed control identical.
    #[test]
    fn liveness_expand_overlay() {
        SelectApp
            .run_test(|pilot| {
                pilot.press(&["tab"])?;
                let before = pilot.app().frame_fingerprint();
                pilot.press(&["enter"])?;
                let after = pilot.app().frame_fingerprint();
                assert_ne!(
                    before, after,
                    "expanding the Select overlay must change the rendered frame"
                );
                Ok(())
            })
            .expect("run_test");
    }
}
