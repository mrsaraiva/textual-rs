/// Port of Python Textual `docs/examples/guide/compound/compound01.py`.
///
/// Demonstrates a compound widget (`InputWithLabel`) that combines a `Label`
/// and an `Input` in a horizontal layout — a reusable building block.
///
/// Python structure:
///   - `InputWithLabel(Widget)` — horizontal row: Label + Input
///   - `CompoundApp(App)` — three `InputWithLabel` rows centered on screen
use rich_rs::{Console, ConsoleOptions, Segments};
use textual::prelude::*;

// ---------------------------------------------------------------------------
// CSS (mirrors compound01.py exactly)
// ---------------------------------------------------------------------------

const CSS: &str = r#"
Screen {
    align: center middle;
}

InputWithLabel {
    layout: horizontal;
    height: auto;
    width: 80%;
    margin: 1;
}

InputWithLabel Label {
    padding: 1;
    width: 12;
    text-align: right;
}

InputWithLabel Input {
    width: 1fr;
}
"#;

// ---------------------------------------------------------------------------
// InputWithLabel — compound widget
// ---------------------------------------------------------------------------

/// A reusable compound widget: a right-aligned label beside an input field.
///
/// Mirrors Python's `InputWithLabel(Widget)` which composes a `Label` and
/// an `Input` in a horizontal layout.
struct InputWithLabel {
    inner: Horizontal,
}

impl InputWithLabel {
    fn new(label_text: &str) -> Self {
        let inner = Horizontal::new()
            .with_child(Label::new(label_text))
            .with_child(Input::new());
        Self { inner }
    }
}

impl Widget for InputWithLabel {
    fn style_type(&self) -> &'static str {
        "InputWithLabel"
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        self.inner.render(console, options)
    }

    fn compose(&mut self) -> textual::compose::ComposeResult {
        self.inner.compose()
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        self.inner.take_node_seed()
    }

    fn on_event(&mut self, event: &Event, ctx: &mut textual::event::WidgetCtx) {
        self.inner.on_event(event, ctx);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut textual::event::WidgetCtx) {
        self.inner.on_event_capture(event, ctx);
    }

    fn focusable(&self) -> bool {
        false
    }

    fn can_focus_children(&self) -> bool {
        true
    }
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

struct CompoundApp;

impl TextualApp for CompoundApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(InputWithLabel::new("First Name"))
            .with_child(InputWithLabel::new("Last Name"))
            .with_child(InputWithLabel::new("Email"))
    }
}

fn main() -> textual::Result<()> {
    run_sync(CompoundApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compound_app_composes_without_panic() {
        let mut app = CompoundApp;
        let _root = app.compose();
    }

    #[test]
    fn input_with_label_composes_label_and_input() {
        let mut w = InputWithLabel::new("First Name");
        let children = w.inner.compose();
        assert_eq!(children.len(), 2);
    }

    /// LIVENESS PROBE — the compound `InputWithLabel` must delegate focus/input
    /// to its inner Input: focusing it and typing echoes the text. We assert the
    /// Input's own text changed (state, not just frame). A dead compound (events
    /// not forwarded to the inner Input) leaves the text empty and fails.
    #[test]
    fn liveness_typing_into_compound_input() {
        textual::run_test(CompoundApp, |pilot| {
            pilot.click("Input")?;
            pilot.press(&["A", "d", "a"])?;
            let text = pilot
                .app_mut()
                .with_query_one_mut_as::<Input, _>("Input", |i| i.text().to_string())
                .unwrap_or_default();
            assert_eq!(
                text, "Ada",
                "typing into the compound widget must reach its inner Input"
            );
            Ok(())
        })
        .unwrap();
    }
}
