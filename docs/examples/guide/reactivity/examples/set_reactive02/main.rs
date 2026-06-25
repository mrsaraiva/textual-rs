/// Port of Python Textual `docs/examples/guide/reactivity/set_reactive02.py`.
///
/// Demonstrates `set_reactive` — initialising a reactive WITHOUT firing its
/// watcher, by setting the stored value directly (rather than via the setter).
///
/// Python:
///   def __init__(self, greeting="Hello", who="World!"):
///       super().__init__()
///       self.set_reactive(Greeter.greeting, greeting)   # (1) no watcher
///       self.set_reactive(Greeter.who, who)
///   greeting/who watchers update the child Labels.
///   The app cycles query_one(Greeter).greeting on Space.
///
/// Rust port (faithful): `set_reactive(field, value)` maps to TWO things in the
/// derive system: (a) assign the field DIRECTLY in `new()` (not via `set_<field>`,
/// so no change is recorded), and (b) declare the reactive `init = false` so the
/// init-phase watcher does NOT fire at mount. Together these reproduce Python's
/// `set_reactive`: the initial value is present but the watcher is skipped, while
/// later `set_greeting(...)` calls DO fire the watcher.
use textual::reactive::{RuntimeReactiveEntry, enqueue_runtime_reactive_entry};
use textual::prelude::*;

const GREETINGS: &[&str] = &[
    "Bonjour",
    "Hola",
    "こんにちは",
    "你好",
    "안녕하세요",
    "Hello",
];

const CSS: &str = r#"
Screen {
    align: center middle;
}

Greeter {
    width: auto;
    height: 1;
}

Greeter Label {
    margin: 0 1;
}
"#;

// ---------------------------------------------------------------------------
// Greeter: reactive greeting/who with `init = false` (set_reactive semantics)
// ---------------------------------------------------------------------------

#[derive(Reactive)]
struct Greeter {
    #[reactive(watch_with_app, init = false)]
    greeting: String,
    #[reactive(watch_with_app, init = false)]
    who: String,
}

impl Greeter {
    fn new(greeting: impl Into<String>, who: impl Into<String>) -> Self {
        // `set_reactive`: assign the stored value DIRECTLY (no setter call), so
        // no change is recorded and the watcher is not fired for the initial value.
        Self {
            greeting: greeting.into(),
            who: who.into(),
        }
    }

    fn watch_greeting(&mut self, app: &mut App, _old: &String, new: &String, _ctx: &mut ReactiveCtx) {
        let new = new.clone();
        let _ = app.with_query_one_mut_as::<Label, _>("#greeting", |label| {
            label.set_text(new);
        });
    }

    fn watch_who(&mut self, app: &mut App, _old: &String, new: &String, _ctx: &mut ReactiveCtx) {
        let new = new.clone();
        let _ = app.with_query_one_mut_as::<Label, _>("#who", |label| {
            label.set_text(new);
        });
    }
}

impl Widget for Greeter {
    fn style_type(&self) -> &'static str {
        "Greeter"
    }

    fn focusable(&self) -> bool {
        false
    }

    fn compose(&self) -> ComposeResult {
        // Labels carry the INITIAL values directly — no watcher needed at mount.
        vec![
            ChildDecl::from(Label::new(self.greeting.clone())).with_id("greeting"),
            ChildDecl::from(Label::new(self.who.clone())).with_id("name"),
        ]
    }

    fn render(
        &self,
        _console: &rich_rs::Console,
        _options: &rich_rs::ConsoleOptions,
    ) -> rich_rs::Segments {
        rich_rs::Segments::new()
    }

    fn reactive_widget(&mut self) -> Option<&mut dyn ReactiveWidget> {
        Some(self)
    }
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

struct NameApp {
    greeting_no: usize,
}

impl NameApp {
    fn new() -> Self {
        Self { greeting_no: 0 }
    }
}

impl TextualApp for NameApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn bindings(&self) -> Vec<BindingDecl> {
        vec![BindingDecl::new("space", "greeting", "Next greeting")]
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Greeter::new("Hello", "Textual"))
    }

    fn on_app_action_str(&mut self, app: &mut App, action: &str, ctx: &mut EventCtx) {
        if action != "greeting" {
            return;
        }
        self.greeting_no = (self.greeting_no + 1) % GREETINGS.len();
        let new_greeting = GREETINGS[self.greeting_no].to_string();
        if let Ok(greeter_id) = app.query_one("Greeter") {
            let mut rctx = ReactiveCtx::new(greeter_id);
            app.with_widget_mut_as::<Greeter, _>(greeter_id, |greeter| {
                greeter.set_greeting(new_greeting, &mut rctx);
            });
            if rctx.has_changes() {
                enqueue_runtime_reactive_entry(RuntimeReactiveEntry::new(greeter_id, rctx));
            }
        }
        ctx.request_repaint();
        ctx.set_handled();
    }
}

fn main() -> textual::Result<()> {
    run_sync(NameApp::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greeter_initial_values_set_without_watcher() {
        // `new` assigns directly; the reactives are `init = false`, so no
        // init-phase change is recorded for the initial value.
        let mut g = Greeter::new("Hello", "World");
        assert_eq!(g.greeting().as_str(), "Hello");
        let mut ctx = ReactiveCtx::new(textual::node_id::NodeId::default());
        g.reactive_record_init(&mut ctx);
        assert!(
            !ctx.has_changes(),
            "init = false reactives must not record an init-phase change"
        );
    }

    #[test]
    fn set_greeting_records_change() {
        let mut g = Greeter::new("Hello", "World");
        let mut ctx = ReactiveCtx::new(textual::node_id::NodeId::default());
        g.set_greeting("Hola".to_string(), &mut ctx);
        assert_eq!(g.greeting().as_str(), "Hola");
        assert!(ctx.has_changes());
    }

    #[test]
    fn greeter_composes_two_labels() {
        let g = Greeter::new("Hello", "World");
        assert_eq!(g.compose().len(), 2);
    }

    /// LIVENESS PROBE — pressing Space (the "greeting" binding) must cycle the
    /// Greeter's `greeting` reactive (the `set_reactive`/init=false field), whose
    /// watcher rewrites the #greeting Label on subsequent setter calls. A dead
    /// demo leaves the frame identical and fails this gate.
    #[test]
    fn liveness_space_cycles_greeting_label() {
        textual::run_test(NameApp::new(), |pilot| {
            let before = pilot.app().frame_fingerprint();
            pilot.press(&["space"])?;
            let after = pilot.app().frame_fingerprint();
            assert_ne!(
                before, after,
                "pressing space must update the greeting label"
            );
            Ok(())
        })
        .unwrap();
    }
}
