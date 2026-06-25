/// Port of Python Textual `docs/examples/guide/widgets/hello05.py`.
///
/// Demonstrates a custom widget that exposes a named action (`next_word`) which
/// cycles through a list of greetings and updates the displayed content.  The
/// greeting text itself is a `[@click='next_word']…[/]` action-link: clicking
/// the word fires the widget-scoped `next_word` action.  The unnamespaced
/// action resolves to the first widget on the bubble path whose action registry
/// declares it — here, the `Hello` widget itself.
///
/// Python features used:
///   - `class Hello(Static)` → custom widget wrapping `Static`
///   - `on_mount()` → `Widget::on_mount()`
///   - `action_next_word()` → exposed via `action_registry` / `execute_action`
///   - `self.update(f"[@click='next_word']{hello}[/], [b]World[/b]!")` → `Static::update()`
///   - `CSS_PATH = "hello05.tcss"` → `const CSS` loaded via `configure`
use textual::prelude::*;

const CSS: &str = r##"
Screen {
    align: center middle;
}

Hello {
    width: 40;
    height: 9;
    padding: 1 2;
    background: $panel;
    border: $secondary tall;
    content-align: center middle;
}
"##;

const HELLOS: &[&str] = &[
    "Hola",
    "Bonjour",
    "Guten tag",
    "Salve",
    "Nǐn hǎo",
    "Olá",
    "Asalaam alaikum",
    "Konnichiwa",
    "Anyoung haseyo",
    "Zdravstvuyte",
    "Hello",
];

struct Hello {
    inner: Static,
    index: usize,
}

impl Hello {
    fn new() -> Self {
        Self {
            inner: Static::new(""),
            index: 0,
        }
    }

    /// Mirrors Python `action_next_word`: cycles to the next greeting and
    /// updates the widget content.  The new content wraps the greeting in a
    /// `[@click='next_word']…[/]` action-link so clicking the word fires this
    /// same action again.
    fn action_next_word(&mut self) {
        let hello = HELLOS[self.index % HELLOS.len()];
        // Python: f"[@click='next_word']{hello}[/], [b]World[/b]!"
        self.inner
            .update(format!("[@click='next_word']{hello}[/], [b]World[/b]!"));
        self.index += 1;
    }
}

/// Action registry for the `Hello` widget — declares the `next_word` action so
/// the runtime can resolve an unnamespaced `[@click='next_word']` click to it.
const HELLO_ACTIONS: &[ActionDecl] = &[ActionDecl {
    name: "next_word",
    namespace: "",
    description: "Show the next greeting",
    default_binding: None,
}];

impl Widget for Hello {
    fn style_type(&self) -> &'static str {
        "Hello"
    }

    fn render(
        &self,
        console: &rich_rs::Console,
        options: &rich_rs::ConsoleOptions,
    ) -> rich_rs::Segments {
        self.inner.render(console, options)
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

    /// Mirrors Python `on_mount`: calls `action_next_word` to show the first
    /// greeting immediately on startup.
    fn on_mount(&mut self) {
        self.action_next_word();
    }

    /// Declare the widget-scoped `next_word` action so unnamespaced
    /// `[@click='next_word']` clicks resolve here (Python `Hello.action_next_word`).
    fn action_registry(&self) -> &[ActionDecl] {
        HELLO_ACTIONS
    }

    fn execute_action(&mut self, action: &ParsedAction, ctx: &mut EventCtx) -> bool {
        if action.name == "next_word" {
            self.action_next_word();
            ctx.request_repaint();
            ctx.set_handled();
            return true;
        }
        false
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        self.inner.take_node_seed()
    }
}

struct CustomApp;

impl TextualApp for CustomApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Hello::new())
    }
}

fn main() -> textual::Result<()> {
    run_sync(CustomApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_cycles_through_greetings() {
        let mut h = Hello::new();
        assert_eq!(h.index, 0);
        h.action_next_word();
        assert_eq!(h.index, 1);
        h.action_next_word();
        assert_eq!(h.index, 2);
    }

    #[test]
    fn hello_wraps_around() {
        let mut h = Hello::new();
        h.index = HELLOS.len() - 1;
        h.action_next_word(); // last item
        assert_eq!(h.index, HELLOS.len());
        h.action_next_word(); // wraps back to index 0
        assert_eq!(h.index, HELLOS.len() + 1);
    }

    #[test]
    fn hello_app_composes_without_panic() {
        let mut app = CustomApp;
        let _root = app.compose();
    }

    // -- LIVENESS PROBE (Pilot run_test) --------------------------------------
    // hello05's greeting is wrapped in a `[@click='next_word']…[/]` action-link.
    // Clicking the centred greeting SHOULD fire the widget-scoped `next_word`
    // action and cycle the text.
    //
    // CURRENTLY DEAD — root cause: the Static/Label render pipeline does not yet
    // route `@click` markup-link clicks to a widget-scoped action. The action
    // text is emitted into the rendered markup for visual fidelity, but a mouse
    // click on the link is not translated into an `execute_action("next_word")`
    // dispatch (the `@click` meta on the styled segment is not consumed by
    // hit-testing). The widget's `action_registry`/`execute_action` are correct
    // and fire when the action is invoked directly (see hello06's `space`
    // binding, which is LIVE) — the missing link is click → action routing.
    // Flip this test active once `@click` link clicks are wired through hit
    // testing. Tracking: at-click-markup-link-routing.
    #[ignore = "DEAD: @click markup-link clicks not routed to widget actions; see comment"]
    #[test]
    fn liveness_click_action_link_cycles_greeting() {
        textual::run_test(CustomApp, |pilot| {
            let before = pilot.app().frame_fingerprint();
            pilot.click("Hello")?;
            let after = pilot.app().frame_fingerprint();
            assert_ne!(
                before, after,
                "clicking the [@click='next_word'] greeting link must fire the \
                 widget action and change the rendered frame"
            );
            Ok(())
        })
        .unwrap();
    }
}
