/// Port of Python Textual `docs/examples/guide/widgets/hello06.py`.
///
/// Extends hello05 by adding a border title ("Hello Widget") and border
/// subtitle ("Click for next hello") to the `Hello` Static widget.
///
/// Python structure:
/// - A `Hello(Static)` widget with `BORDER_TITLE = "Hello Widget"`.
/// - `on_mount` sets `self.border_subtitle = "Click for next hello"` and
///   calls `action_next_word()` to show the first greeting.
/// - `action_next_word` cycles through a list of greetings and calls
///   `self.update(f"[@click='next_word']{hello}[/], [b]World[/b]!")`.
/// - Layout from `hello05.tcss`: `Screen { align: center middle }` and
///   `Hello` styled with a tall `$secondary` border, 40×9, centred content.
///
/// Framework gaps:
/// - `[@click='next_word']` markup: Python Textual renders this as a
///   clickable link that fires `action_next_word` on the widget. The Rust
///   Label/Static pipeline does not yet route `@click` markup clicks as
///   widget-scoped actions.  The greeting text is composed with the markup
///   for visual fidelity; click interactions are absent.  Space bar cycling
///   is provided as a keyboard fallback.
use textual::prelude::*;

const CSS: &str = r##"
Screen {
    align: center middle;
}

#hello {
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

struct HelloApp {
    hello_idx: usize,
}

impl HelloApp {
    fn new() -> Self {
        Self { hello_idx: 0 }
    }

    fn current_markup(&self) -> String {
        let hello = HELLOS[self.hello_idx];
        format!("[@click='next_word']{hello}[/], [b]World[/b]!")
    }
}

impl TextualApp for HelloApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn bindings(&self) -> Vec<BindingDecl> {
        vec![BindingDecl::new("space", "next_word", "Next hello")]
    }

    fn compose(&mut self) -> AppRoot {
        // Python: `BORDER_TITLE = "Hello Widget"` — set at compose time via
        // `with_border_title`.  Content and border_subtitle are set in
        // `on_mount_with_app` (mirrors Python's `on_mount`).
        AppRoot::new().with_child(
            Static::new("")
                .with_border_title("Hello Widget")
                .id("hello"),
        )
    }

    fn on_mount_with_app(&mut self, app: &mut App, ctx: &mut EventCtx) {
        // Python: self.border_subtitle = "Click for next hello"
        //         self.action_next_word()
        let markup = self.current_markup();
        let _ = app.with_query_one_mut_as::<Static, _>("#hello", |s| {
            s.set_border_subtitle(Some("Click for next hello"));
            s.update(markup);
        });
        ctx.request_repaint();
    }

    fn on_app_action_str(&mut self, app: &mut App, action: &str, ctx: &mut EventCtx) {
        if action == "next_word" {
            self.hello_idx = (self.hello_idx + 1) % HELLOS.len();
            let markup = self.current_markup();
            let _ = app.with_query_one_mut_as::<Static, _>("#hello", |s| {
                s.update(markup);
            });
            ctx.request_repaint();
            ctx.set_handled();
        }
    }
}

fn main() -> Result<()> {
    run_sync(HelloApp::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_composes_without_panic() {
        let mut app = HelloApp::new();
        let _root = app.compose();
    }

    #[test]
    fn hello_idx_cycles() {
        let mut app = HelloApp::new();
        let n = HELLOS.len();
        for _ in 0..n {
            app.hello_idx = (app.hello_idx + 1) % n;
        }
        assert_eq!(app.hello_idx, 0);
    }

    #[test]
    fn markup_contains_hello_and_world() {
        let app = HelloApp::new();
        let markup = app.current_markup();
        assert!(markup.contains("Hola"));
        assert!(markup.contains("World"));
    }

    #[test]
    fn space_binding_declared() {
        let app = HelloApp::new();
        let bindings = app.bindings();
        assert!(bindings.iter().any(|b| b.key == "space"));
    }

    // -- LIVENESS PROBE (Pilot run_test) --------------------------------------
    // hello06 cycles the greeting via the app-level `space` → `next_word`
    // binding (the keyboard fallback for the @click link). Pressing space must
    // change the rendered frame.
    #[test]
    fn liveness_space_cycles_greeting() {
        textual::run_test(HelloApp::new(), |pilot| {
            let before = pilot.app().frame_fingerprint();
            pilot.press(&["space"])?;
            let after = pilot.app().frame_fingerprint();
            assert_ne!(
                before, after,
                "pressing space must cycle the greeting via the app `next_word` \
                 action and change the rendered frame"
            );
            Ok(())
        })
        .unwrap();
    }
}
