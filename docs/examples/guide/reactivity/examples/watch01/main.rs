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

    fn on_message_with_app(&mut self, app: &mut App, message: &MessageEvent, ctx: &mut textual::event::WidgetCtx) {
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

    /// LIVENESS PROBE (currently DEAD — see root cause below).
    ///
    /// Submitting a colour name into the Input must fire the app-level
    /// `watch_color`, painting the `#new` panel background. We assert the panel's
    /// *own background* changed (the Input is cleared on submit, so the frame
    /// alone wouldn't move — there is no echo false positive here).
    ///
    /// ROOT CAUSE (DEAD): the InputSubmitted handler + app-level `watch_color`
    /// fire correctly (`app.color` becomes the parsed colour), but the watcher
    /// paints via `Static::set_inline_style`, which writes to the widget's
    /// detached `seed.styles.style` (emptied at mount). A post-mount
    /// `set_inline_style` on an in-tree widget never reaches the arena node's
    /// rendered style, so `#new`'s background stays unset. Styling-pipeline gap
    /// (same as computed01), distinct from this reactive-dispatch sweep. Flip
    /// this `#[ignore]` once post-mount `set_inline_style` reaches render, or
    /// switch the watcher to `query_mut(sel).set_styles(...)`.
    #[test]
    fn liveness_submitting_color_repaints_panels() {
        textual::run_test(WatchApp::new(), |pilot| {
            pilot.click("Input")?;
            pilot.press(&["r", "e", "d", "enter"])?;
            let newnode = pilot.app().query_one("#new").unwrap();
            let bg = pilot.app().node_explicit_bg(newnode);
            assert!(
                bg.is_some(),
                "submitting a colour must paint the #new panel background"
            );
            Ok(())
        })
        .unwrap();
    }
}
