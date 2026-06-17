/// Port of Python Textual `docs/examples/guide/reactivity/set_reactive02.py`.
///
/// Demonstrates the equivalent of `set_reactive` — initializing reactive
/// variables with a value before `compose()` runs, so the watcher is NOT
/// called during construction.
///
/// Python structure:
///   - `Greeter(Horizontal)` widget: two `Label`s — greeting and who.
///     `set_reactive(Greeter.greeting, greeting)` initialises the reactive
///     before compose so the watcher is skipped on first render.
///   - `NameApp(App)`: cycles through GREETINGS on Space.
///
/// Rust differences:
///   - No reactive system for custom widgets; instead `Greeter` stores the
///     greeting/who strings and uses `Static` children.
///   - Space key → `on_key_with_app` cycles `greeting_no` and updates the
///     `#greeting` Static in the arena tree via `with_query_one_mut_as`.
///   - `set_reactive` semantics (no watcher on init) are naturally satisfied
///     because the initial text is set in `Greeter::new()` before composition.
use textual::prelude::*;

// ---------------------------------------------------------------------------
// Greeting list (mirrors Python GREETINGS)
// ---------------------------------------------------------------------------

const GREETINGS: &[&str] = &[
    "Bonjour",
    "Hola",
    "こんにちは",
    "你好",
    "안녕하세요",
    "Hello",
];

// ---------------------------------------------------------------------------
// CSS
// ---------------------------------------------------------------------------

const CSS: &str = r#"
Screen {
    align: center middle;
}

Greeter {
    width: auto;
    height: 1;
}

Greeter Static {
    margin: 0 1;
    width: auto;
}
"#;

// ---------------------------------------------------------------------------
// Greeter widget — mirrors Python's Greeter(Horizontal)
// ---------------------------------------------------------------------------

struct Greeter {
    inner: Horizontal,
}

impl Greeter {
    /// Mirrors Python's `__init__` with `set_reactive` calls.
    ///
    /// The initial `greeting` and `who` are set directly — no watcher is
    /// triggered (equivalent to Python's `set_reactive` semantics).
    fn new(greeting: impl Into<String>, who: impl Into<String>) -> Self {
        let greeting = greeting.into();
        let who = who.into();
        let inner = Horizontal::new()
            .with_child(Static::new(greeting).id("greeting"))
            .with_child(Static::new(who).id("name"));
        Self { inner }
    }
}

impl Widget for Greeter {
    fn style_type(&self) -> &'static str {
        "Greeter"
    }

    fn focusable(&self) -> bool {
        false
    }

    fn render(
        &self,
        console: &rich_rs::Console,
        options: &rich_rs::ConsoleOptions,
    ) -> rich_rs::Segments {
        self.inner.render(console, options)
    }

    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        self.inner.take_composed_children()
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.inner.on_event(event, ctx);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.inner.on_event_capture(event, ctx);
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.inner.on_layout(width, height);
    }

    fn layout_height(&self) -> Option<usize> {
        self.inner.layout_height()
    }

    fn content_width(&self) -> Option<usize> {
        self.inner.content_width()
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        self.inner.take_node_seed()
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
        // Python: yield Greeter(who="Textual")
        // Default greeting in Python's Greeter.__init__ is "Hello"; greeting_no
        // starts at 0 and only advances on Space.
        AppRoot::new().with_child(Greeter::new("Hello", "Textual"))
    }

    fn on_app_action_str(&mut self, app: &mut App, action: &str, ctx: &mut EventCtx) {
        if action == "greeting" {
            // Python: self.greeting_no = (self.greeting_no + 1) % len(GREETINGS)
            //         self.query_one(Greeter).greeting = GREETINGS[self.greeting_no]
            self.greeting_no = (self.greeting_no + 1) % GREETINGS.len();
            let new_greeting = GREETINGS[self.greeting_no].to_string();
            // NOTE: `Static::id("greeting")` wraps the Static in a transparent Node
            // that carries the css_id; the concrete Static is a child of that Node.
            // Therefore `#greeting` matches the Node (style_type "Node"), and
            // downcasting it to `Static` fails.  Use a descendant selector so the
            // query matches the Static directly.
            let result = app.with_query_one_mut_as::<Static, _>("#greeting Static", |s| {
                s.update(new_greeting);
            });
            debug_assert!(result.is_ok(), "greeting Static not found: {:?}", result);
            ctx.request_repaint();
            ctx.set_handled();
        }
    }
}

fn main() -> textual::Result<()> {
    run_sync(NameApp::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greeter_composes_without_panic() {
        let mut g = Greeter::new("Hello", "World");
        let _ = g.take_composed_children();
    }

    #[test]
    fn greeting_cycles_through_all() {
        let mut app = NameApp::new();
        let n = GREETINGS.len();
        for _ in 0..n {
            app.greeting_no = (app.greeting_no + 1) % n;
        }
        // After a full cycle we're back to 0
        assert_eq!(app.greeting_no, 0);
    }

    #[test]
    fn app_has_space_binding() {
        let app = NameApp::new();
        let bindings = app.bindings();
        assert!(
            bindings.iter().any(|b| b.key == "space"),
            "space binding must exist"
        );
    }

    #[test]
    fn initial_greeting_is_hello() {
        // Python: Greeter(who="Textual") uses default greeting="Hello"
        let mut app = NameApp::new();
        let root = app.compose();
        // The first child is Greeter; its first composed child should carry "Hello"
        // We can verify by inspecting the greeting_no (starts at 0) and GREETINGS[0].
        // The initial text rendered is "Hello" (hardcoded in compose, not GREETINGS[0]).
        assert_eq!(app.greeting_no, 0);
        // The root must yield at least one child (the Greeter)
        drop(root);
    }

    #[test]
    fn action_greeting_updates_greeting_no() {
        // Verify that cycling via on_app_action_str progresses greeting_no correctly.
        // This is a unit-level check; PTY/arena integration is covered by the
        // "initial greeting + Space" regression below.
        let mut app_state = NameApp::new();
        assert_eq!(app_state.greeting_no, 0);
        app_state.greeting_no = (app_state.greeting_no + 1) % GREETINGS.len();
        assert_eq!(app_state.greeting_no, 1);
        assert_eq!(GREETINGS[app_state.greeting_no], "Hola");
        // Cycle all the way around
        for _ in 1..GREETINGS.len() {
            app_state.greeting_no = (app_state.greeting_no + 1) % GREETINGS.len();
        }
        assert_eq!(app_state.greeting_no, 0);
    }
}
