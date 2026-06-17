/// Port of Python Textual `docs/examples/guide/reactivity/dynamic_watch.py`.
///
/// Demonstrates `dynamic_watch`: a `Counter` widget with a reactive `counter`
/// field drives both an internal Label (via its own watcher) and an external
/// ProgressBar (via an app-level watcher registered in `on_mount`).
///
/// Python structure:
///   - Counter(Widget) — reactive `counter`, Label + Button("+10")
///     - watch_counter: updates Label text
///     - on_button_pressed: increments counter by 10
///   - WatchApp — Counter + ProgressBar(total=100, show_eta=False)
///     - on_mount: app.watch(counter, "counter", update_progress)
///
/// Rust differences:
///   - No `reactive(0)` macro; `counter` is a plain `i64` field.
///   - No framework-level `app.watch()` cross-widget watcher registration.
///   - `Counter` posts a custom `CounterChanged { value }` message when the
///     counter changes. The app's `on_message_with_app` intercepts it to update
///     both the internal Label and the external ProgressBar.
///   - This replaces Python's `app.watch(counter_widget, "counter", callback)`.
///
/// Framework gaps:
///   - No `app.watch(widget, field, callback)` cross-widget reactive watcher.
///   - No inline `reactive(0)` field declaration syntax.
use textual::impl_message;
use textual::message::MessageEvent;
use textual::prelude::*;

// ---------------------------------------------------------------------------
// CSS
// ---------------------------------------------------------------------------

const CSS: &str = r#"
Counter {
    height: auto;
}
"#;

// ---------------------------------------------------------------------------
// Custom message: emitted by Counter when its counter value changes
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct CounterChanged {
    value: i64,
}
impl_message!(CounterChanged);

// ---------------------------------------------------------------------------
// Counter widget
// ---------------------------------------------------------------------------

struct Counter {
    counter: i64,
    inner: Vertical,
}

impl Counter {
    fn new() -> Self {
        let inner = Vertical::new().with_compose(vec![
            ChildDecl::from(Label::new("0")).with_id("counter-label"),
            ChildDecl::from(Button::new("+10")).with_id("plus-btn"),
        ]);
        Self { counter: 0, inner }
    }
}

impl Widget for Counter {
    fn style_type(&self) -> &'static str {
        "Counter"
    }

    fn focusable(&self) -> bool {
        false
    }

    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        self.inner.take_composed_children()
    }

    fn render(
        &self,
        console: &rich_rs::Console,
        options: &rich_rs::ConsoleOptions,
    ) -> rich_rs::Segments {
        self.inner.render(console, options)
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

    /// Handle button press: increment counter and post CounterChanged.
    fn on_message(&mut self, message: &MessageEvent, ctx: &mut EventCtx) {
        self.inner.on_message(message, ctx);
        if let Some(bp) = message.downcast_ref::<ButtonPressed>() {
            if bp.button_id.as_deref() == Some("plus-btn") {
                self.counter += 10;
                ctx.post_message(CounterChanged {
                    value: self.counter,
                });
                ctx.set_handled();
            }
        }
    }
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

struct WatchApp;

impl TextualApp for WatchApp {
    fn title(&self) -> &'static str {
        "WatchApp"
    }

    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Counter::new())
            .with_compose(vec![
                ChildDecl::from(ProgressBar::new(Some(100.0))).with_id("progress"),
            ])
    }

    fn on_mount_with_app(&mut self, app: &mut App, ctx: &mut EventCtx) {
        // Python: ProgressBar(total=100, show_eta=False) — disable ETA display.
        if let Ok(node_id) = app.query_one("#progress") {
            let mut rctx = ReactiveCtx::new(node_id);
            let _ = app.with_query_one_mut_as::<ProgressBar, _>("#progress", |bar| {
                bar.set_show_eta(false, &mut rctx);
            });
        }
        ctx.request_repaint();
    }

    /// Python: on_mount registers a watcher via app.watch(counter, "counter", update_progress).
    /// Rust: intercept CounterChanged message to drive both Label and ProgressBar updates.
    fn on_message_with_app(
        &mut self,
        app: &mut App,
        message: &MessageEvent,
        ctx: &mut EventCtx,
    ) {
        if let Some(ev) = message.downcast_ref::<CounterChanged>() {
            let value = ev.value;

            // Update the Counter's internal Label (mirrors Python's watch_counter).
            let _ = app.with_query_one_mut_as::<Label, _>("#counter-label", |label| {
                label.set_text(value.to_string());
            });

            // Update the ProgressBar (mirrors Python's update_progress watcher).
            // Python: self.query_one(ProgressBar).update(progress=counter_value)
            // Rust: compute delta to advance from current position to counter_value.
            if let Ok(node_id) = app.query_one("#progress") {
                let mut rctx = ReactiveCtx::new(node_id);
                let _ = app.with_query_one_mut_as::<ProgressBar, _>("#progress", |bar| {
                    bar.set_progress(value as f64, &mut rctx);
                });
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
    fn compose_has_counter_and_progress_bar() {
        let mut app = WatchApp;
        let root = app.compose();
        assert!(!root.children().is_empty(), "AppRoot should have children");
    }

    #[test]
    fn counter_starts_at_zero() {
        let c = Counter::new();
        assert_eq!(c.counter, 0);
    }

    #[test]
    fn counter_widget_composes_without_panic() {
        let mut c = Counter::new();
        let children = c.take_composed_children();
        assert_eq!(children.len(), 2, "Counter should compose 2 children");
    }

    #[test]
    fn progress_bar_total_is_100() {
        let bar = ProgressBar::new(Some(100.0));
        assert_eq!(bar.total(), Some(100.0));
    }
}
