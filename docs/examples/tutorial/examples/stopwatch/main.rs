/// Port of Python Textual `docs/examples/tutorial/stopwatch.py`.
///
/// The final tutorial stopwatch app: three stopwatches with Start/Stop/Reset
/// buttons, a live time display, and bindings to add/remove stopwatches
/// dynamically (a = add, r = remove, d = toggle dark mode).
///
/// This is `stopwatch06` plus dynamic add/remove. It uses the framework's timer +
/// reactive fundamentals (the same primitives Python uses) for the live clock,
/// NOT an app-level tick/id-query push.
///
/// Python structure:
///   - TimeDisplay(Digits) — live clock, `set_interval(1/60, update_time, pause=True)`,
///     reactive `time`/`total`, and `start()`/`stop()`/`reset()`.
///   - Stopwatch(HorizontalGroup) — Start/Stop/Reset buttons + a TimeDisplay;
///     `on_button_pressed` calls the matching TimeDisplay method + toggles `started`.
///   - StopwatchApp(App) — Header, Footer, VerticalScroll(#timers) with three
///     Stopwatches; `a`/`r` mount/remove a Stopwatch.
///
/// Rust faithful mapping (see stopwatch06 for the detailed rationale):
///   - `TimeDisplay` is a `#[derive(Reactive)]` widget wrapping `Digits` with
///     `time`/`total` reactives + a `running` flag and start/stop/reset methods.
///   - The app owns ONE `set_interval(1/60)` driving `update_time` on every
///     TimeDisplay (each only advances while `running`). Newly mounted stopwatches
///     are picked up automatically because the interval re-queries `TimeDisplay`.
///   - `Stopwatch::on_message` toggles `started` and posts a `TimeDisplayCmd`;
///     the app's `on_message_with_app` applies start/stop/reset to the addressed
///     TimeDisplay node.
///   - Dynamic add/remove uses `app.mount_under` / `app.remove_node`.
///
/// NON-PROMOTABLE as a full golden: the running clock digits are timer-driven and
/// nondeterministic. Structural add/remove is verified via tests and idle capture.
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use textual::prelude::*;
use textual::reactive::{enqueue_runtime_reactive_entry, RuntimeReactiveEntry};

// ---------------------------------------------------------------------------
// CSS (mirrors stopwatch.tcss exactly)
// ---------------------------------------------------------------------------

const CSS: &str = r#"
Stopwatch {
    layout: horizontal;
    background: $boost;
    height: 5;
    min-width: 50;
    margin: 1;
    padding: 1;
}

TimeDisplay {
    text-align: center;
    color: $foreground-muted;
    height: 3;
}

Button {
    width: 16;
}

#start {
    dock: left;
}

#stop {
    dock: left;
    display: none;
}

#reset {
    dock: right;
}

.started {
    background: $success-muted;
    color: $text;
}

.started TimeDisplay {
    color: $foreground;
}

.started #start {
    display: none;
}

.started #stop {
    display: block;
}

.started #reset {
    visibility: hidden;
}
"#;

/// Format elapsed seconds as `HH:MM:SS.cc` — mirrors Python's
/// `f"{hours:02,.0f}:{minutes:02.0f}:{seconds:05.2f}"`.
fn format_time(secs: f64) -> String {
    let total_cs = (secs * 100.0) as u64;
    let cs = total_cs % 100;
    let total_s = total_cs / 100;
    let s = total_s % 60;
    let total_m = total_s / 60;
    let m = total_m % 60;
    let h = total_m / 60;
    format!("{h:02}:{m:02}:{s:02}.{cs:02}")
}

// Unique id per TimeDisplay so a Stopwatch (initial or dynamically added) can
// address its own display.
static NEXT_DISPLAY: AtomicU64 = AtomicU64::new(0);

// ---------------------------------------------------------------------------
// Custom message: a Stopwatch tells the app to drive its TimeDisplay.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CmdKind {
    Start,
    Stop,
    Reset,
}

#[derive(Debug, Clone)]
struct TimeDisplayCmd {
    display_id: String,
    kind: CmdKind,
}

textual::impl_message!(TimeDisplayCmd);

// ---------------------------------------------------------------------------
// TimeDisplay widget — Digits + `time`/`total` reactives + start/stop/reset
// ---------------------------------------------------------------------------

#[derive(Reactive)]
struct TimeDisplay {
    #[reactive(watch, init = false)]
    time: f64,
    #[reactive(init = false)]
    total: f64,
    start_instant: Instant,
    running: bool,
    inner: Digits,
    display_id: String,
}

impl TimeDisplay {
    fn new() -> Self {
        let n = NEXT_DISPLAY.fetch_add(1, Ordering::Relaxed);
        Self {
            time: 0.0,
            total: 0.0,
            start_instant: Instant::now(),
            running: false,
            inner: Digits::new("00:00:00.00"),
            display_id: format!("td-{n}"),
        }
    }

    /// Python `update_time`: `self.time = self.total + (monotonic() - start_time)`.
    fn update_time(&mut self, ctx: &mut ReactiveCtx) {
        if !self.running {
            return;
        }
        let elapsed = self.start_instant.elapsed().as_secs_f64();
        self.set_time(self.total + elapsed, ctx);
    }

    /// Python `start`: reset the segment origin and resume the timer.
    fn start(&mut self, _ctx: &mut ReactiveCtx) {
        self.start_instant = Instant::now();
        self.running = true;
    }

    /// Python `stop`: pause, fold the segment into `total`, freeze time.
    fn stop(&mut self, ctx: &mut ReactiveCtx) {
        if self.running {
            self.total += self.start_instant.elapsed().as_secs_f64();
            self.running = false;
        }
        let total = self.total;
        self.set_total(total, ctx);
        self.set_time(total, ctx);
    }

    /// Python `reset`: total = 0, time = 0.
    fn reset(&mut self, ctx: &mut ReactiveCtx) {
        self.running = false;
        self.set_total(0.0, ctx);
        self.set_time(0.0, ctx);
    }

    /// Python `watch_time`: format the elapsed time and push it to the Digits.
    fn watch_time(&mut self, _old: &f64, new: &f64, _ctx: &mut ReactiveCtx) {
        self.inner.update(format_time(*new));
    }
}

impl Widget for TimeDisplay {
    fn style_type(&self) -> &'static str {
        "TimeDisplay"
    }

    fn focusable(&self) -> bool {
        false
    }

    fn render(
        &self,
        console: &rich_rs::Console,
        options: &rich_rs::ConsoleOptions,
    ) -> rich_rs::Segments {
        self.inner.render_styled(console, options)
    }

    fn content_width(&self) -> Option<usize> {
        self.inner.content_width()
    }

    fn layout_height(&self) -> Option<usize> {
        self.inner.layout_height()
    }

    fn reactive_widget(&mut self) -> Option<&mut dyn ReactiveWidget> {
        Some(self)
    }
}

// ---------------------------------------------------------------------------
// Stopwatch widget
// ---------------------------------------------------------------------------

struct Stopwatch {
    inner: HorizontalGroup,
    display_id: String,
}

impl Stopwatch {
    fn new() -> Self {
        let display = TimeDisplay::new();
        let display_id = display.display_id.clone();
        let inner = HorizontalGroup::new().with_compose(vec![
            ChildDecl::from(Button::success("Start").id("start")),
            ChildDecl::from(Button::error("Stop").id("stop")),
            ChildDecl::from(Button::new("Reset").id("reset")),
            ChildDecl::from(display).with_id(&display_id),
        ]);
        Self { inner, display_id }
    }
}

impl Widget for Stopwatch {
    fn style_type(&self) -> &'static str {
        "Stopwatch"
    }

    fn compose(&mut self) -> textual::compose::ComposeResult {
        self.inner.compose()
    }

    fn render(
        &self,
        console: &rich_rs::Console,
        options: &rich_rs::ConsoleOptions,
    ) -> rich_rs::Segments {
        self.inner.render(console, options)
    }

    fn focusable(&self) -> bool {
        false
    }

    fn on_event(&mut self, event: &Event, ctx: &mut textual::event::WidgetCtx) {
        self.inner.on_event(event, ctx);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut textual::event::WidgetCtx) {
        self.inner.on_event_capture(event, ctx);
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut textual::event::WidgetCtx) {
        if let Some(bp) = message.downcast_ref::<ButtonPressed>() {
            let kind = match bp.button_id.as_deref() {
                Some("start") => {
                    ctx.add_class("started");
                    Some(CmdKind::Start)
                }
                Some("stop") => {
                    ctx.remove_class("started");
                    Some(CmdKind::Stop)
                }
                Some("reset") => Some(CmdKind::Reset),
                _ => None,
            };
            if let Some(kind) = kind {
                ctx.post_message(TimeDisplayCmd {
                    display_id: self.display_id.clone(),
                    kind,
                });
                ctx.request_repaint();
                ctx.set_handled();
            }
        }
    }

    fn on_tick(&mut self, tick: u64) {
        self.inner.on_tick(tick);
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.inner.on_layout(width, height);
    }
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

const INITIAL_STOPWATCHES: usize = 3;

struct StopwatchApp;

impl TextualApp for StopwatchApp {
    fn title(&self) -> &'static str {
        "StopwatchApp"
    }

    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn bindings(&self) -> Vec<BindingDecl> {
        vec![
            BindingDecl::new("d", "toggle_dark", "Toggle dark mode"),
            BindingDecl::new("a", "add_stopwatch", "Add"),
            BindingDecl::new("r", "remove_stopwatch", "Remove"),
        ]
    }

    fn compose(&mut self) -> AppRoot {
        let stopwatches: Vec<ChildDecl> = (0..INITIAL_STOPWATCHES)
            .map(|_| ChildDecl::from(Stopwatch::new()))
            .collect();

        AppRoot::new()
            .with_child(Header::new())
            .with_child(Footer::new())
            .with_child(
                Node::new(
                    VerticalScroll::new()
                        .with_child(Vertical::new().with_compose(stopwatches)),
                )
                .id("timers"),
            )
    }

    fn on_mount_with_app(&mut self, app: &mut App, _ctx: &mut textual::event::WidgetCtx) {
        // Python TimeDisplay.on_mount: set_interval(1/60, update_time, pause=True).
        // One app-owned 1/60s interval drives `update_time` on every TimeDisplay
        // (including dynamically added ones, since it re-queries each fire); each
        // display only advances while `running`.
        app.set_interval(
            Duration::from_secs_f64(1.0 / 60.0),
            None,
            false,
            Box::new(|app, _ctx| {
                let display_ids = app
                    .query("TimeDisplay")
                    .map(|q| q.into_ids())
                    .unwrap_or_default();
                for node_id in display_ids {
                    let mut rctx = ReactiveCtx::new(node_id);
                    app.with_widget_mut_as::<TimeDisplay, _>(node_id, |td| {
                        td.update_time(&mut rctx);
                    });
                    if rctx.has_changes() {
                        enqueue_runtime_reactive_entry(RuntimeReactiveEntry::new(node_id, rctx));
                    }
                }
            }),
        );
    }

    fn on_message_with_app(&mut self, app: &mut App, message: &MessageEvent, ctx: &mut textual::event::WidgetCtx) {
        if let Some(cmd) = message.downcast_ref::<TimeDisplayCmd>() {
            let sel = format!("#{}", cmd.display_id);
            let node_id = match app.query(&sel).and_then(|q| q.first()) {
                Ok(id) => id,
                Err(_) => return,
            };
            let mut rctx = ReactiveCtx::new(node_id);
            app.with_widget_mut_as::<TimeDisplay, _>(node_id, |td| match cmd.kind {
                CmdKind::Start => td.start(&mut rctx),
                CmdKind::Stop => td.stop(&mut rctx),
                CmdKind::Reset => td.reset(&mut rctx),
            });
            if rctx.has_changes() {
                enqueue_runtime_reactive_entry(RuntimeReactiveEntry::new(node_id, rctx));
            }
            ctx.request_repaint();
            ctx.set_handled();
        }
    }

    fn on_key_with_app(&mut self, app: &mut App, key: &KeyEventData, ctx: &mut textual::event::WidgetCtx) {
        match key.name() {
            "a" => {
                // Python: self.query_one("#timers").mount(Stopwatch())
                let _ = app.mount_under("#timers Vertical", Stopwatch::new());
                ctx.set_handled();
            }
            "r" => {
                // Python: self.query("Stopwatch").last().remove()
                if let Ok(last) = app.query("Stopwatch").and_then(|q| q.last()) {
                    let _ = app.remove_node(last);
                }
                ctx.set_handled();
            }
            _ => {}
        }
    }
}

fn main() -> textual::Result<()> {
    run_sync(StopwatchApp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use textual::reactive::ReactiveWidget;

    fn flush(td: &mut TimeDisplay, mut ctx: ReactiveCtx) {
        let changes = ctx.take_changes();
        let mut dispatch_ctx = ReactiveCtx::new(textual::node_id::NodeId::default());
        td.reactive_dispatch(&changes, &mut dispatch_ctx);
    }

    #[test]
    fn stopwatch_composes_without_panic() {
        let mut app = StopwatchApp;
        let _root = app.compose();
    }

    #[test]
    fn has_all_bindings() {
        let app = StopwatchApp;
        let bindings = app.bindings();
        assert!(bindings.iter().any(|b| b.key == "d"), "missing 'd' binding");
        assert!(bindings.iter().any(|b| b.key == "a"), "missing 'a' binding");
        assert!(bindings.iter().any(|b| b.key == "r"), "missing 'r' binding");
    }

    #[test]
    fn format_time_zero() {
        assert_eq!(format_time(0.0), "00:00:00.00");
    }

    #[test]
    fn format_time_one_minute() {
        assert_eq!(format_time(61.5), "00:01:01.50");
    }

    /// start() + an advanced clock advances the displayed time; reset() zeroes it.
    #[test]
    fn start_advances_and_reset_zeroes() {
        let mut td = TimeDisplay::new();

        let mut ctx = ReactiveCtx::new(textual::node_id::NodeId::default());
        td.start(&mut ctx);
        td.start_instant = Instant::now() - Duration::from_secs(4);

        let mut ctx = ReactiveCtx::new(textual::node_id::NodeId::default());
        td.update_time(&mut ctx);
        assert!(ctx.has_changes(), "running display advances");
        flush(&mut td, ctx);
        assert_ne!(td.inner.value(), "00:00:00.00");

        // Reset works while stopped.
        let mut ctx = ReactiveCtx::new(textual::node_id::NodeId::default());
        td.reset(&mut ctx);
        flush(&mut td, ctx);
        assert_eq!(td.inner.value(), "00:00:00.00");
    }

    #[test]
    fn time_display_ids_unique() {
        let a = TimeDisplay::new();
        let b = TimeDisplay::new();
        assert_ne!(a.display_id, b.display_id);
    }

    // -- LIVENESS PROBES (Pilot run_test) -------------------------------------
    // The final stopwatch app's headline interactions are dynamic add/remove
    // (`a` mounts a Stopwatch under #timers, `r` removes the last one) and the
    // Start button (`started` class toggle). Both are deterministic structural/
    // style changes, so each must change the rendered frame.
    //
    // NOTE: the running-clock digit advance is NOT probed — the displayed time
    // derives from a wall-clock `Instant::elapsed()`, not the manual timer
    // clock, so `Pilot::advance_clock` cannot reproduce it deterministically
    // (the demo flags this as "NON-PROMOTABLE: timer-driven").

    #[test]
    fn liveness_press_a_adds_stopwatch() {
        textual::run_test(StopwatchApp, |pilot| {
            let before_count = pilot
                .app()
                .query("Stopwatch")
                .map(|q| q.into_ids().len())
                .unwrap_or(0);
            let before = pilot.app().frame_fingerprint();
            pilot.press(&["a"])?;
            let after = pilot.app().frame_fingerprint();
            let after_count = pilot
                .app()
                .query("Stopwatch")
                .map(|q| q.into_ids().len())
                .unwrap_or(0);
            assert_eq!(
                after_count,
                before_count + 1,
                "pressing `a` must mount one more Stopwatch"
            );
            assert_ne!(
                before, after,
                "pressing `a` must add a Stopwatch and change the rendered frame"
            );
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn liveness_press_r_removes_stopwatch() {
        textual::run_test(StopwatchApp, |pilot| {
            let before_count = pilot
                .app()
                .query("Stopwatch")
                .map(|q| q.into_ids().len())
                .unwrap_or(0);
            pilot.press(&["r"])?;
            let after_count = pilot
                .app()
                .query("Stopwatch")
                .map(|q| q.into_ids().len())
                .unwrap_or(0);
            assert_eq!(
                after_count,
                before_count.saturating_sub(1),
                "pressing `r` must remove one Stopwatch"
            );
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn liveness_click_start_toggles_started_class() {
        textual::run_test(StopwatchApp, |pilot| {
            let before = pilot.app().frame_fingerprint();
            pilot.click("#start")?;
            let after = pilot.app().frame_fingerprint();
            assert_ne!(
                before, after,
                "clicking Start must add the `started` class and change the \
                 rendered frame"
            );
            Ok(())
        })
        .unwrap();
    }
}
