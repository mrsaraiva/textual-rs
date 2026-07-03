/// Port of Python Textual `docs/examples/guide/reactivity/refresh02.py`.
///
/// Demonstrates reactive LAYOUT invalidation: a custom `Name` widget renders
/// `"Hello, {who}!"` inside an auto-sized bordered box. Typing into the `Input`
/// updates `who`, and because the reactive declares `layout=True`, the box
/// re-lays-out (resizes) as the greeting's length changes.
///
/// Python:
///   class Name(Widget):
///       who = reactive("name", layout=True)   # (1)
///       def render(self) -> str: return f"Hello, {self.who}!"
///   on_input_changed: self.query_one(Name).who = event.value
///
/// Rust port (faithful): `Name` derives `Reactive` with `#[reactive(layout)] who`.
/// The generated `set_who(value, ctx)` records a layout+repaint change; the
/// runtime reactive phase invalidates layout for the `Name` node — exactly
/// Python's `reactive(layout=True)`. The app handler queries the `Name` node,
/// sets `who`, and enqueues the change for the runtime reactive phase.
use textual::reactive::{RuntimeReactiveEntry, enqueue_runtime_reactive_entry};
use textual::prelude::*;

const CSS: &str = r#"
Input {
    dock: top;
    margin-top: 1;
}

Name {
    width: auto;
    height: auto;
    border: heavy $secondary;
}
"#;

// ---------------------------------------------------------------------------
// Name widget — `who = reactive("name", layout=True)`
// ---------------------------------------------------------------------------

#[derive(Reactive)]
struct Name {
    #[reactive(layout)]
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
        if let Some(m) = message.downcast_ref::<InputChanged>() {
            let value = m.value.clone();
            if let Ok(name_id) = app.query_one("Name") {
                let mut rctx = ReactiveCtx::new(name_id);
                app.with_widget_mut_as::<Name, _>(name_id, |name| {
                    name.set_who(value, &mut rctx);
                });
                if rctx.has_changes() {
                    enqueue_runtime_reactive_entry(RuntimeReactiveEntry::new(name_id, rctx));
                }
            }
            // The reactive's layout flag drives relayout via the reactive phase.
            ctx.request_layout_invalidation();
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
        let w = Name::new();
        assert_eq!(w.who().as_str(), "name");
    }

    #[test]
    fn set_who_records_layout_change() {
        let mut w = Name::new();
        let mut ctx = ReactiveCtx::new(textual::node_id::NodeId::default());
        w.set_who("Alice".to_string(), &mut ctx);
        assert_eq!(w.who().as_str(), "Alice");
        assert!(ctx.needs_repaint());
        assert!(ctx.needs_layout(), "layout reactive must request layout");
    }

    /// LIVENESS PROBE — typing into the Input must drive the `Name` widget's
    /// `who` (layout=true) reactive. We assert the widget's own reactive value
    /// changed (not merely the frame, which the Input echo would dirty on its
    /// own — an echo false positive we avoid). A dead demo leaves `who`
    /// unchanged and fails this gate.
    #[test]
    fn liveness_typing_relayouts_name_box() {
        textual::run_test(WatchApp, |pilot| {
            pilot.click("Input")?;
            pilot.press(&["A", "l", "i", "c", "e"])?;
            let nid = pilot.app().query_one("Name").unwrap();
            let who = pilot
                .app_mut()
                .with_widget_mut_as::<Name, _>(nid, |n| n.who().clone())
                .unwrap_or_default();
            assert_eq!(
                who, "Alice",
                "typing must flow into the Name widget's `who` (layout) reactive"
            );
            Ok(())
        })
        .unwrap();
    }
}
