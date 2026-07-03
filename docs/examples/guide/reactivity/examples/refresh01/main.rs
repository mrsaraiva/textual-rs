/// Port of Python Textual `docs/examples/guide/reactivity/refresh01.py`.
///
/// Demonstrates reactive text refresh driven by Input changes.
///
/// Python:
///   class Name(Widget):
///       who = reactive("name")
///       def render(self) -> str:
///           return f"Hello, {self.who}!"
///   on_input_changed: self.query_one(Name).who = event.value
///
/// Rust port (faithful): `Name` derives `Reactive` with `#[reactive] who`. Its
/// `render()` reads `who`. The generated `set_who(value, ctx)` records a repaint
/// change; the runtime reactive phase repaints the `Name` node — exactly Python's
/// default `reactive` (repaint on change). The app handler queries the `Name`
/// node, sets `who`, and enqueues the change for the runtime reactive phase.
use textual::reactive::{RuntimeReactiveEntry, enqueue_runtime_reactive_entry};
use textual::prelude::*;

const CSS: &str = r#"
Input {
    dock: top;
    margin-top: 1;
}

Name {
    height: 100%;
    content-align: center middle;
}
"#;

// ---------------------------------------------------------------------------
// Name widget — `who = reactive("name")`, render reads `who`
// ---------------------------------------------------------------------------

#[derive(Reactive)]
struct Name {
    #[reactive]
    who: String,
}

impl Name {
    fn new() -> Self {
        Self {
            who: "name".to_string(),
        }
    }
}

impl Widget for Name {
    fn style_type(&self) -> &'static str {
        "Name"
    }

    fn focusable(&self) -> bool {
        false
    }

    // Python `render` returns f"Hello, {self.who}!".
    fn render(
        &self,
        console: &rich_rs::Console,
        options: &rich_rs::ConsoleOptions,
    ) -> rich_rs::Segments {
        Static::new(format!("Hello, {}!", self.who)).render(console, options)
    }

    fn reactive_widget(&mut self) -> Option<&mut dyn ReactiveWidget> {
        Some(self)
    }
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

struct WatchApp;

impl TextualApp for WatchApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Input::new().with_placeholder("Enter your name"))
            .with_child(Name::new())
    }

    fn on_message_with_app(&mut self, app: &mut App, message: &MessageEvent, ctx: &mut textual::event::WidgetCtx) {
        if let Some(changed) = message.downcast_ref::<InputChanged>() {
            let value = changed.value.clone();
            if let Ok(name_id) = app.query_one("Name") {
                let mut rctx = ReactiveCtx::new(name_id);
                app.with_widget_mut_as::<Name, _>(name_id, |name| {
                    name.set_who(value, &mut rctx);
                });
                if rctx.has_changes() {
                    enqueue_runtime_reactive_entry(RuntimeReactiveEntry::new(name_id, rctx));
                }
            }
            ctx.request_repaint();
        }
    }
}

fn main() -> textual::Result<()> {
    run_sync(WatchApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watch_app_composes_without_panic() {
        let mut app = WatchApp;
        let _root = app.compose();
    }

    #[test]
    fn name_defaults_to_name() {
        let name = Name::new();
        assert_eq!(name.who().as_str(), "name");
    }

    #[test]
    fn set_who_records_repaint_change() {
        let mut name = Name::new();
        let mut ctx = ReactiveCtx::new(textual::node_id::NodeId::default());
        name.set_who("World".to_string(), &mut ctx);
        assert_eq!(name.who().as_str(), "World");
        assert!(ctx.has_changes());
        assert!(ctx.needs_repaint());
        assert!(!ctx.needs_layout(), "plain reactive does not request layout");
    }

    /// LIVENESS PROBE — typing into the Input must drive the `Name` widget's
    /// `who` reactive (via InputChanged). We assert the `Name` widget's own
    /// reactive value changed (not merely the frame, which the Input echoing the
    /// typed text would dirty on its own — an echo false positive we avoid). A
    /// dead demo (unwired InputChanged / reactive never enqueued) leaves `who`
    /// unchanged and fails this gate.
    #[test]
    fn liveness_typing_updates_name_render() {
        textual::run_test(WatchApp, |pilot| {
            pilot.click("Input")?;
            pilot.press(&["W", "o", "r", "l", "d"])?;
            let nid = pilot.app().query_one("Name").unwrap();
            let who = pilot
                .app_mut()
                .with_widget_mut_as::<Name, _>(nid, |n| n.who().clone())
                .unwrap_or_default();
            assert_eq!(
                who, "World",
                "typing must flow into the Name widget's `who` reactive"
            );
            Ok(())
        })
        .unwrap();
    }
}
