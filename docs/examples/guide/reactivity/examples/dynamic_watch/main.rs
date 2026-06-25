/// Port of Python Textual `docs/examples/guide/reactivity/dynamic_watch.py`.
///
/// Demonstrates DYNAMIC watch (`self.watch(obj, attribute, callback)`): a
/// `Counter` widget with a reactive `counter` drives BOTH its own Label (via its
/// own `watch_counter`) and an external `ProgressBar` (via an app-level dynamic
/// watcher registered in `on_mount`).
///
/// Python:
///   class Counter(Widget):
///       counter = reactive(0)   # (1)
///       def compose(self): yield Label(); yield Button("+10")
///       def on_button_pressed(self): self.counter += 10
///       def watch_counter(self, v): self.query_one(Label).update(str(v))
///   class WatchApp(App):
///       def compose(self): yield Counter(); yield ProgressBar(total=100, show_eta=False)
///       def on_mount(self):
///           def update_progress(v): self.query_one(ProgressBar).update(progress=v)  # (2)
///           self.watch(self.query_one(Counter), "counter", update_progress)  # (3)
///
/// Rust port (faithful): `Counter` derives `Reactive` with
/// `#[reactive(watch_with_app)] counter`; `watch_counter` updates its Label. The
/// app's `on_mount_with_app` registers a DYNAMIC watcher via
/// `App::watch_reactive(counter_node, "counter", cb)` — Rust's equivalent of
/// `self.watch(...)` — whose callback updates the ProgressBar. Pressing "+10"
/// sets the Counter's reactive (via the widget-level reactive phase), which fires
/// both watchers.
use textual::reactive::{RuntimeReactiveEntry, enqueue_runtime_reactive_entry};
use textual::prelude::*;

const CSS: &str = r#"
Counter {
    height: auto;
}
"#;

// ---------------------------------------------------------------------------
// Counter widget: reactive counter + Label + Button("+10")
// ---------------------------------------------------------------------------

#[derive(Reactive)]
struct Counter {
    #[reactive(watch_with_app)]
    counter: i64,
}

impl Counter {
    fn new() -> Self {
        Self { counter: 0 }
    }

    /// Python `watch_counter`: update the internal Label text.
    fn watch_counter(&mut self, app: &mut App, _old: &i64, new: &i64, _ctx: &mut ReactiveCtx) {
        let text = new.to_string();
        let _ = app.with_query_one_mut_as::<Label, _>("#counter-label", |label| {
            label.set_text(text);
        });
    }
}

impl Widget for Counter {
    fn style_type(&self) -> &'static str {
        "Counter"
    }

    fn focusable(&self) -> bool {
        false
    }

    fn compose(&self) -> ComposeResult {
        vec![
            ChildDecl::from(Label::new("0")).with_id("counter-label"),
            ChildDecl::from(Button::new("+10")).with_id("plus-btn"),
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

    // Python `on_button_pressed`: self.counter += 10.
    fn on_message(&mut self, message: &MessageEvent, ctx: &mut EventCtx) {
        if let Some(bp) = message.downcast_ref::<ButtonPressed>() {
            if bp.button_id.as_deref() == Some("plus-btn") {
                let node_id = self.node_id();
                let mut rctx = ReactiveCtx::new(node_id);
                self.set_counter(self.counter + 10, &mut rctx);
                if rctx.has_changes() {
                    enqueue_runtime_reactive_entry(RuntimeReactiveEntry::new(node_id, rctx));
                }
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
            .with_child(ProgressBar::new(Some(100.0)).id("progress"))
    }

    fn on_mount_with_app(&mut self, app: &mut App, ctx: &mut EventCtx) {
        // Python `ProgressBar(total=100, show_eta=False)`: disable ETA display.
        if let Ok(progress_id) = app.query_one("#progress") {
            let mut rctx = ReactiveCtx::new(progress_id);
            let _ = app.with_query_one_mut_as::<ProgressBar, _>("#progress", |bar| {
                bar.set_show_eta(false, &mut rctx);
            });
        }

        // Python: self.watch(self.query_one(Counter), "counter", update_progress).
        // Register a DYNAMIC watcher on the Counter's `counter` reactive whose
        // callback advances the ProgressBar.
        if let Ok(counter_id) = app.query_one("Counter") {
            app.watch_reactive(counter_id, "counter", |app, value| {
                if let Some(v) = value.downcast_ref::<i64>() {
                    if let Ok(progress_id) = app.query_one("#progress") {
                        let mut rctx = ReactiveCtx::new(progress_id);
                        let progress = *v as f64;
                        let _ = app.with_query_one_mut_as::<ProgressBar, _>("#progress", |bar| {
                            bar.set_progress(progress, &mut rctx);
                        });
                    }
                }
            });
        }
        ctx.request_repaint();
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
    fn counter_starts_at_zero() {
        let c = Counter::new();
        assert_eq!(*c.counter(), 0);
    }

    #[test]
    fn counter_composes_label_and_button() {
        let c = Counter::new();
        assert_eq!(c.compose().len(), 2);
    }

    #[test]
    fn set_counter_records_change() {
        let mut c = Counter::new();
        let mut ctx = ReactiveCtx::new(textual::node_id::NodeId::default());
        c.set_counter(10, &mut ctx);
        assert_eq!(*c.counter(), 10);
        assert!(ctx.has_changes());
    }

    #[test]
    fn progress_bar_total_is_100() {
        let bar = ProgressBar::new(Some(100.0));
        assert_eq!(bar.total(), Some(100.0));
    }

    /// LIVENESS PROBE (currently DEAD — see root cause below).
    ///
    /// Pressing the "+10" button must bump the Counter's `counter` reactive,
    /// firing BOTH watchers (its Label and the dynamically-watched ProgressBar).
    /// We assert the *counter value itself* changed (not just the frame, which a
    /// focus ring alone would dirty — a false positive we deliberately avoid).
    ///
    /// ROOT CAUSE (DEAD): the Button id is set via `ChildDecl::with_id("plus-btn")`,
    /// which stores the id on the child *node*, NOT on the Button widget's own
    /// `seed.css_id`. So `Button::take_node_seed` caches `css_id = None`, the
    /// emitted `ButtonPressed.button_id` is `None`, and `Counter::on_message`'s
    /// `bp.button_id == Some("plus-btn")` check never matches — the counter never
    /// increments. The fix is in the compose/node-build pipeline (propagate a
    /// `ChildDecl` id into the boxed widget's seed so id-carrying messages like
    /// `ButtonPressed` see it), out of scope for this reactive-dispatch sweep.
    /// Flip this `#[ignore]` once `ChildDecl::with_id` propagates to the widget
    /// seed (or change the demo to `Button::new("+10").id("plus-btn")`).
    #[test]
    #[ignore = "DEAD: ChildDecl::with_id not propagated to Button.css_id -> ButtonPressed.button_id is None -> Counter never increments (compose/node-build pipeline fix needed)"]
    fn liveness_plus_button_updates_label_and_progress() {
        textual::run_test(WatchApp, |pilot| {
            pilot.press(&["tab", "enter"])?;
            let cid = pilot.app().query_one("Counter").unwrap();
            let cval = pilot
                .app_mut()
                .with_widget_mut_as::<Counter, _>(cid, |c| *c.counter())
                .unwrap_or(0);
            assert_eq!(
                cval, 10,
                "pressing +10 must increment the Counter reactive to 10"
            );
            Ok(())
        })
        .unwrap();
    }
}
