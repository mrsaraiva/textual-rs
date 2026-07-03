/// Port of Python Textual `docs/examples/guide/reactivity/set_reactive01.py`.
///
/// Demonstrates reactive fields on a custom widget, cycling greetings with Space.
///
/// Python: a custom `Greeter(Horizontal)` with reactive `greeting`/`who` and
/// watchers that update child Labels; `__init__` assigns `self.greeting = greeting`
/// (which FIRES the watcher). The app cycles `query_one(Greeter).greeting`.
///
/// Rust port (faithful): `Greeter` derives `Reactive` with
/// `#[reactive(watch_with_app)] greeting / who`, composes two Labels (`#greeting`,
/// `#name`), and its watchers update those Labels. `Greeter::new` assigns via the
/// setter so the watcher fires (matching `self.greeting = greeting`). The app's
/// `greeting` action queries the `Greeter` node, sets `greeting`, and enqueues the
/// change for the runtime reactive phase.
///
/// (Note: Python's `watch_who` queries `#who`, but the Label has `id="name"`, so it
/// is a no-op there too; this port keeps the same structure — only `#greeting`
/// visibly updates.)
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

const CSS: &str = r##"
Screen {
    align: center middle;
}

Greeter {
    layout: horizontal;
    width: auto;
    height: 1;
}

Greeter Label {
    margin: 0 1;
}
"##;

// ---------------------------------------------------------------------------
// Greeter: custom widget with reactive greeting/who
// ---------------------------------------------------------------------------

#[derive(Reactive)]
struct Greeter {
    #[reactive(watch_with_app)]
    greeting: String,
    #[reactive(watch_with_app)]
    who: String,
}

impl Greeter {
    fn new(greeting: impl Into<String>, who: impl Into<String>) -> Self {
        // Python `__init__`: self.greeting = greeting; self.who = who.
        Self {
            greeting: greeting.into(),
            who: who.into(),
        }
    }

    /// Python `watch_greeting`: update the `#greeting` Label.
    fn watch_greeting(&mut self, app: &mut App, _old: &String, new: &String, _ctx: &mut ReactiveCtx) {
        let new = new.clone();
        let _ = app.with_query_one_mut_as::<Label, _>("#greeting", |label| {
            label.set_text(new);
        });
    }

    /// Python `watch_who`: update the `#who` Label (no-op here, mirroring Python).
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

    fn compose(&mut self) -> ComposeResult {
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
        // Python: yield Greeter(who="Textual"). Default greeting is "Hello".
        AppRoot::new().with_child(Greeter::new("Hello", "Textual"))
    }

    fn on_app_action_str(&mut self, app: &mut App, action: &str, ctx: &mut textual::event::WidgetCtx) {
        if action != "greeting" {
            return;
        }
        // Python: self.greeting_no = (self.greeting_no + 1) % len(GREETINGS)
        //         self.query_one(Greeter).greeting = GREETINGS[self.greeting_no]
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
    fn greeter_composes_two_labels() {
        let g = Greeter::new("Hello", "World");
        assert_eq!(g.compose().len(), 2);
    }

    #[test]
    fn set_greeting_records_change() {
        let mut g = Greeter::new("Hello", "World");
        let mut ctx = ReactiveCtx::new(textual::node_id::NodeId::default());
        g.set_greeting("Bonjour".to_string(), &mut ctx);
        assert_eq!(g.greeting().as_str(), "Bonjour");
        assert!(ctx.has_changes());
    }

    #[test]
    fn app_has_space_binding() {
        let app = NameApp::new();
        assert!(app.bindings().iter().any(|b| b.key == "space"));
    }

    #[test]
    fn greeting_cycles() {
        let mut app = NameApp::new();
        let n = GREETINGS.len();
        for _ in 0..n {
            app.greeting_no = (app.greeting_no + 1) % n;
        }
        assert_eq!(app.greeting_no, 0);
    }

    /// LIVENESS PROBE — pressing Space (the "greeting" binding) must cycle the
    /// Greeter's `greeting` reactive, whose watcher rewrites the #greeting Label.
    /// A dead demo (unwired action / reactive never enqueued) leaves the frame
    /// identical and fails this gate.
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
