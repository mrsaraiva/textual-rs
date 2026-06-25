/// Port of Python Textual `docs/examples/guide/widgets/counter01.py`.
///
/// Demonstrates a custom focusable widget (`Counter`) that extends `Static`
/// behavior: renders a count, styled differently when focused.
///
/// Python structure:
///   - Counter(Static, can_focus=True) — shows "Count: N"; reactive count field
///   - CounterApp(App) — three Counter widgets + Footer
///
/// Rust differences:
///   - No reactive macro for this example; `count` is a plain `i64` field.
///   - Widget renders the count string directly via `rich_rs::Text`.
///   - Focus styling is driven purely by CSS (`:focus` pseudo-class), matching
///     the Python example's `counter.tcss`.
use textual::prelude::*;

// ---------------------------------------------------------------------------
// CSS (mirrors counter.tcss)
// ---------------------------------------------------------------------------

const CSS: &str = r#"
Counter {
    background: $panel-darken-1;
    padding: 1 2;
    color: $text-muted;
}

Counter:focus {
    background: $primary;
    color: $text;
    text-style: bold;
    outline-left: thick $accent;
}
"#;

// ---------------------------------------------------------------------------
// Counter widget
// ---------------------------------------------------------------------------

struct Counter {
    count: i64,
    seed: NodeSeed,
}

impl Counter {
    fn new() -> Self {
        Self {
            count: 0,
            seed: NodeSeed::default(),
        }
    }
}

impl Widget for Counter {
    fn style_type(&self) -> &'static str {
        "Counter"
    }

    fn focusable(&self) -> bool {
        true
    }

    fn render(
        &self,
        console: &rich_rs::Console,
        options: &rich_rs::ConsoleOptions,
    ) -> rich_rs::Segments {
        let text = rich_rs::Text::plain(format!("Count: {}", self.count));
        rich_rs::Renderable::render(&text, console, options)
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

struct CounterApp;

impl TextualApp for CounterApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Counter::new())
            .with_child(Counter::new())
            .with_child(Counter::new())
            .with_child(Footer::new())
    }
}

fn main() -> textual::Result<()> {
    run_sync(CounterApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counter_starts_at_zero() {
        let c = Counter::new();
        assert_eq!(c.count, 0);
    }

    #[test]
    fn counter_is_focusable() {
        let c = Counter::new();
        assert!(c.focusable());
    }

    #[test]
    fn counter_style_type() {
        let c = Counter::new();
        assert_eq!(c.style_type(), "Counter");
    }

    #[test]
    fn app_composes_without_panic() {
        let mut app = CounterApp;
        let _root = app.compose();
    }

    // -- LIVENESS PROBE (Pilot run_test) --------------------------------------
    // counter01 has no key bindings; its only interaction is focus: pressing
    // Tab moves focus to a Counter, which the CSS `Counter:focus` rule styles
    // distinctly (background/color/outline). The rendered frame must change.
    #[test]
    fn liveness_tab_focuses_counter_changes_frame() {
        textual::run_test(CounterApp, |pilot| {
            let before = pilot.app().frame_fingerprint();
            pilot.press(&["tab"])?;
            let after = pilot.app().frame_fingerprint();
            assert_ne!(
                before, after,
                "pressing Tab must focus a Counter and change the rendered \
                 frame (the `:focus` style applies)"
            );
            Ok(())
        })
        .unwrap();
    }
}
