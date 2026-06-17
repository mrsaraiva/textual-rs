/// Port of Python Textual `docs/examples/guide/reactivity/watch01.py`.
///
/// Demonstrates reactive watchers: when `color` changes, `watch_color(old, new)`
/// is called with both the old and new value, updating the background colours of
/// two `Static` panels (`#old`, `#new`).
///
/// Python:
///   color = reactive(Color.parse("transparent"))
///   def watch_color(self, old_color, new_color):
///       self.query_one("#old").styles.background = old_color
///       self.query_one("#new").styles.background = new_color
///   def on_input_submitted(self, event): self.color = Color.parse(event.value)
///
/// Rust port (faithful): the app derives `Reactive` with
/// `#[reactive(watch_with_app)] color`. The generated `set_color(value, ctx)`
/// records the change; the app-level reactive bridge then invokes
/// `watch_color(&mut self, app, old, new, ctx)` — the both-values watcher —
/// which queries `#old`/`#new` and sets their inline backgrounds, exactly like
/// Python's two-argument `watch_color`.
use textual::prelude::*;
use textual::style::parse_color_like;

const CSS: &str = r#"
Input {
    dock: top;
    margin-top: 1;
}

#colors {
    grid-size: 2 1;
    grid-gutter: 2 4;
    grid-columns: 1fr;
    margin: 0 1;
}

#old {
    height: 100%;
    border: wide $secondary;
}

#new {
    height: 100%;
    border: wide $secondary;
}
"#;

#[derive(Reactive)]
struct WatchApp {
    /// Mirrors Python `color = reactive(Color.parse("transparent"))`.
    /// `watch_with_app` so the bridge calls `watch_color` (which queries widgets).
    /// `init = false`: Python's `transparent` default need not paint the panels at
    /// mount (the panels start with their CSS border only), matching the example.
    #[reactive(watch_with_app, init = false)]
    color: Color,
}

impl WatchApp {
    fn new() -> Self {
        // Python default: Color.parse("transparent").
        Self {
            color: parse_color_like("transparent").unwrap_or(Color::rgba(0, 0, 0, 0)),
        }
    }

    /// Python `watch_color(self, old_color, new_color)`: set both panels.
    fn watch_color(&mut self, app: &mut App, old: &Color, new: &Color, _ctx: &mut ReactiveCtx) {
        let old = *old;
        let new = *new;
        let _ = app.with_query_one_mut_as::<Static, _>("#old", |s| {
            s.set_inline_style(Style::new().bg(old));
        });
        let _ = app.with_query_one_mut_as::<Static, _>("#new", |s| {
            s.set_inline_style(Style::new().bg(new));
        });
    }
}

impl TextualApp for WatchApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn reactive_widget_mut(&mut self) -> Option<&mut dyn ReactiveWidget> {
        Some(self)
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Input::new().with_placeholder("Enter a color"))
            .with_child(
                Grid::new(1, 2)
                    .id("colors")
                    .with_child(Static::new("").id("old"))
                    .with_child(Static::new("").id("new")),
            )
    }

    fn on_message_with_app(&mut self, app: &mut App, message: &MessageEvent, ctx: &mut EventCtx) {
        if let Some(m) = message.downcast_ref::<InputSubmitted>() {
            let value = m.value.trim().to_string();
            if let Some(new_color) = parse_color_like(&value) {
                // Python: clear the input, then assign self.color (fires watch_color).
                let _ = app.with_query_one_mut_as::<Input, _>("Input", |input| {
                    input.set_text(String::new());
                });
                self.set_color(new_color, app.reactive_ctx());
                ctx.request_repaint();
            }
        }
    }
}

fn main() -> textual::Result<()> {
    run_sync(WatchApp::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watch_app_composes_without_panic() {
        let mut app = WatchApp::new();
        let _root = app.compose();
    }

    #[test]
    fn set_color_records_change() {
        let mut app = WatchApp::new();
        let mut ctx = ReactiveCtx::new(textual::node_id::NodeId::default());
        let red = parse_color_like("red").expect("red parses");
        app.set_color(red, &mut ctx);
        assert_eq!(*app.color(), red);
        assert!(ctx.has_changes(), "color change must be recorded");
    }
}
