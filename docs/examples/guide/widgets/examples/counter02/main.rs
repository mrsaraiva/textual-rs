/// Port of Python Textual `docs/examples/guide/widgets/counter02.py`.
///
/// Demonstrates a custom focusable widget (`Counter`) that:
/// - Holds reactive integer state (`count`).
/// - Declares key bindings (`up`/`k` → increment, `down`/`j` → decrement).
/// - Re-renders its content whenever the count changes.
/// - Applies distinct styles when focused (via CSS `:focus` pseudo-class).
///
/// Three Counter instances are stacked vertically with a Footer showing bindings.
///
/// Framework gaps:
/// - Python `reactive` auto-triggers re-render on mutation; here we manage
///   count as plain state and call `ctx.request_repaint()` from `execute_action`.
/// - CSS `&:focus` nested selectors are supported in textual-rs.
use textual::action::ParsedAction;
use textual::prelude::*;

const CSS: &str = r##"
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
"##;

// ---------------------------------------------------------------------------
// Counter widget
// ---------------------------------------------------------------------------

/// A counter that can be incremented and decremented via key bindings.
///
/// Mirrors Python `class Counter(Static, can_focus=True)`.
struct Counter {
    count: i64,
}

impl Counter {
    fn new() -> Self {
        Self { count: 0 }
    }
}

impl Widget for Counter {
    fn style_type(&self) -> &'static str {
        "Counter"
    }

    fn focusable(&self) -> bool {
        true
    }

    fn render(&self, _console: &rich_rs::Console, options: &rich_rs::ConsoleOptions) -> rich_rs::Segments {
        use rich_rs::{Segment, Segments};
        let text = format!("Count: {}", self.count);
        // Pad to widget width so the background fills the whole row.
        let width = options.size.0 as usize;
        let padded = format!("{:<width$}", text, width = width);
        Segments::from(vec![Segment::new(padded)])
    }

    fn bindings(&self) -> Vec<BindingDecl> {
        vec![
            BindingDecl::new("up,k", "change_count(1)", "Increment"),
            BindingDecl::new("down,j", "change_count(-1)", "Decrement"),
        ]
    }

    fn execute_action(&mut self, action: &ParsedAction, ctx: &mut textual::event::WidgetCtx) -> bool {
        if action.name != "change_count" {
            return false;
        }
        // Parse the first argument as an integer amount.
        let amount: i64 = match action.arguments.first().and_then(|s| s.parse().ok()) {
            Some(v) => v,
            None => return false,
        };
        self.count += amount;
        ctx.request_repaint();
        ctx.set_handled();
        true
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
    fn counter_app_composes_without_panic() {
        let mut app = CounterApp;
        let _root = app.compose();
    }

    #[test]
    fn counter_starts_at_zero() {
        let c = Counter::new();
        assert_eq!(c.count, 0);
    }

    #[test]
    fn counter_execute_action_increment() {
        let mut c = Counter::new();
        let action = textual::action::parse_action("change_count(1)").unwrap();
        let mut ctx = EventCtx::default();
        assert!(c.execute_action(&action, &mut ctx));
        assert_eq!(c.count, 1);
    }

    #[test]
    fn counter_execute_action_decrement() {
        let mut c = Counter::new();
        let action = textual::action::parse_action("change_count(-1)").unwrap();
        let mut ctx = EventCtx::default();
        assert!(c.execute_action(&action, &mut ctx));
        assert_eq!(c.count, -1);
    }

    #[test]
    fn counter_execute_action_unknown_returns_false() {
        let mut c = Counter::new();
        let action = textual::action::parse_action("unknown_action").unwrap();
        let mut ctx = EventCtx::default();
        assert!(!c.execute_action(&action, &mut ctx));
    }

    #[test]
    fn counter_bindings_declared() {
        let c = Counter::new();
        let bindings = c.bindings();
        assert_eq!(bindings.len(), 2);
        assert_eq!(bindings[0].key, "up,k");
        assert_eq!(bindings[1].key, "down,j");
        assert!(bindings[0].action.contains("change_count"));
        assert!(bindings[1].action.contains("change_count"));
    }

    #[test]
    fn counter_is_focusable() {
        let c = Counter::new();
        assert!(c.focusable());
    }

    // -- LIVENESS PROBE (Pilot run_test) --------------------------------------
    // Drives the real headless app: the first Counter is focused on mount, so
    // pressing `up` (the bound increment key) runs the bound `change_count(1)`
    // action and re-renders the focused Counter's "Count: N" text, changing the
    // frame.
    //
    // LIVE: a widget that declares BINDINGS whose action is served only by
    // `execute_action` (no `action_registry()` entry) now runs. The key path
    // first asks `action::resolve_action` for a registry owner; `Counter`
    // overrides `execute_action` but declares no `action_registry`, so that
    // returns `None`. Binding resolution then falls back to the binding's own
    // SOURCE node (the `Counter` that declared `up,k`) — the binding source IS
    // the target — and dispatches `change_count` there. See
    // `runtime/event_loop.rs` (CLUSTER 7 fallback) and `match_binding_chain`.
    //
    // The probe increments the mount-focused, visible first Counter (no Tab):
    // the demo's three counters stack but only the first is laid out visibly in
    // the headless frame today (a separate layout gap), so the assertion targets
    // the visible counter to keep this a pure binding-liveness check.
    #[test]
    fn liveness_up_key_increments_visible_count() {
        textual::run_test(CounterApp, |pilot| {
            // The first Counter is focused on mount; its key bindings are active.
            let before = pilot.app().frame_fingerprint();
            pilot.press(&["up"])?;
            let after = pilot.app().frame_fingerprint();
            assert_ne!(
                before, after,
                "pressing `up` on a focused Counter must change the rendered \
                 frame (count incremented)"
            );
            Ok(())
        })
        .unwrap();
    }
}
