/// Port of Python Textual `docs/examples/guide/reactivity/refresh03.py`.
///
/// Demonstrates WIDGET-LEVEL reactive recompose: a custom `Name` widget renders
/// `Label("Hello, {who}!")` inside a bordered box. Typing into an `Input` updates
/// the widget's `who`, which recomposes the `Name` subtree.
///
/// Python:
///   class Name(Widget):
///       who = reactive("name", recompose=True)   # (1)
///       def compose(self): yield Label(f"Hello, {self.who}!")   # (2)
///   on_input_changed: self.query_one(Name).who = event.value
///
/// Rust port (faithful): `Name` derives `Reactive` with `#[reactive(recompose)] who`
/// and composes a `Label`. The generated `set_who(value, ctx)` records a change
/// carrying the recompose flag; the runtime's reactive phase then recomposes the
/// `Name` node (re-running `compose()`), exactly like Python's `recompose=True`.
/// The app handler queries the `Name` node, sets `who`, and enqueues the change
/// for the runtime reactive phase.
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
// Name: custom widget with a recompose reactive `who`
// ---------------------------------------------------------------------------

#[derive(Reactive)]
struct Name {
    #[reactive(recompose)]
    who: String,
}

impl Name {
    fn new(who: impl Into<String>) -> Self {
        Self { who: who.into() }
    }
}

impl Widget for Name {
    fn style_type(&self) -> &'static str {
        "Name"
    }

    fn focusable(&self) -> bool {
        false
    }

    // Compose-only widget: the composed Label renders itself in the arena tree.
    fn render(
        &self,
        _console: &rich_rs::Console,
        _options: &rich_rs::ConsoleOptions,
    ) -> rich_rs::Segments {
        rich_rs::Segments::new()
    }

    // Python `compose`: yield Label(f"Hello, {self.who}!"). Recompose re-runs this.
    fn compose(&self) -> ComposeResult {
        vec![ChildDecl::from(Label::new(format!("Hello, {}!", self.who)))]
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
            .with_child(Name::new("name"))
    }

    fn on_message_with_app(&mut self, app: &mut App, message: &MessageEvent, ctx: &mut EventCtx) {
        if let Some(m) = message.downcast_ref::<InputChanged>() {
            let value = m.value.clone();
            // Python: self.query_one(Name).who = value. Setting the recompose
            // reactive records a change; enqueue it for the runtime reactive
            // phase, which recomposes the Name subtree.
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
    fn name_composes_greeting_label() {
        let name = Name::new("world");
        let decls = name.compose();
        assert_eq!(decls.len(), 1, "Name composes a single Label");
    }

    #[test]
    fn set_who_records_recompose_change() {
        let mut name = Name::new("name");
        let mut ctx = ReactiveCtx::new(textual::node_id::NodeId::default());
        name.set_who("Alice".to_string(), &mut ctx);
        assert_eq!(name.who().as_str(), "Alice");
        assert!(ctx.has_changes());
        assert!(ctx.needs_recompose(), "recompose reactive must request recompose");
    }

    /// LIVENESS PROBE — typing into the Input must drive the `Name` widget's
    /// `who` (recompose) reactive, re-running its `compose()`. We assert the
    /// widget's own reactive value changed (not merely the frame, which the
    /// Input echo would dirty on its own — an echo false positive we avoid). A
    /// dead demo (recompose not wired) leaves `who` unchanged and fails.
    #[test]
    fn liveness_typing_recomposes_name_label() {
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
                "typing must flow into the Name widget's `who` (recompose) reactive"
            );
            Ok(())
        })
        .unwrap();
    }
}
