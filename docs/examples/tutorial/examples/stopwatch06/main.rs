/// Port of Python Textual `docs/examples/tutorial/stopwatch06.py`.
///
/// Builds on stopwatch05 by giving `TimeDisplay` real start/stop/reset behavior
/// using the framework's timer + reactive fundamentals (the same primitives
/// Python uses), NOT an app-level tick/id-query push.
///
/// Python (stopwatch06.py):
///   class TimeDisplay(Digits):
///       start_time = reactive(monotonic)
///       time = reactive(0.0)
///       total = reactive(0.0)
///       def on_mount(self):
///           self.update_timer = self.set_interval(1 / 60, self.update_time, pause=True)
///       def update_time(self): self.time = self.total + (monotonic() - self.start_time)
///       def watch_time(self, time): self.update(format(time))
///       def start(self): self.start_time = monotonic(); self.update_timer.resume()
///       def stop(self):
///           self.update_timer.pause()
///           self.total += monotonic() - self.start_time
///           self.time = self.total
///       def reset(self): self.total = 0; self.time = 0
///   class Stopwatch(HorizontalGroup):
///       def on_button_pressed(self, event):
///           time_display = self.query_one(TimeDisplay)
///           if button_id == "start": time_display.start(); self.add_class("started")
///           elif button_id == "stop": time_display.stop(); self.remove_class("started")
///           elif button_id == "reset": time_display.reset()
///
/// Rust faithful mapping:
///   - `TimeDisplay` is a `#[derive(Reactive)]` widget wrapping `Digits`, with
///     `time` and `total` reactives plus a `running` flag. `update_time` sets
///     `time = total + elapsed-since-start`; `watch_time` formats + pushes to the
///     Digits. `start`/`stop`/`reset` mirror Python exactly (resume/pause/reset).
///   - Widgets can't self-register timers here, so the app owns ONE
///     `set_interval(1/60)` that calls `update_time` on every TimeDisplay; each
///     display only advances while `running` (equivalent to Python pausing each
///     display's own timer).
///   - `Stopwatch::on_message` catches the `ButtonPressed`, toggles the `started`
///     class, and posts a `TimeDisplayCmd { display_id, kind }`. The app's
///     `on_message_with_app` applies that command to the addressed TimeDisplay
///     node (`start`/`stop`/`reset`), recording reactive changes that the runtime
///     dispatch turns into re-renders. The Reset button works while stopped.
///
/// NON-PROMOTABLE (timer-driven): the running clock digits change every frame.
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use textual::prelude::*;
use textual::reactive::{enqueue_runtime_reactive_entry, RuntimeReactiveEntry};

const CSS: &str = r#"
Stopwatch {
    background: $boost;
    height: 5;
    margin: 1;
    min-width: 50;
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

// Unique id per TimeDisplay so a Stopwatch can address its own display.
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
    /// CSS id of the target TimeDisplay (`#td-N`).
    display_id: String,
    kind: CmdKind,
}

textual::impl_message!(TimeDisplayCmd);

// ---------------------------------------------------------------------------
// TimeDisplay widget — Digits + `time`/`total` reactives + start/stop/reset
// ---------------------------------------------------------------------------

#[derive(Reactive)]
struct TimeDisplay {
    /// Displayed elapsed seconds; the timer sets this, `watch_time` renders it.
    #[reactive(watch, init = false)]
    time: f64,
    /// Accumulated elapsed seconds across previous run segments (Python `total`).
    #[reactive(init = false)]
    total: f64,
    /// Origin of the current run segment (set on `start`).
    start_instant: Instant,
    /// Whether this display is currently advancing (timer "resumed").
    running: bool,
    /// The wrapped `Digits` that renders the formatted time.
    inner: Digits,
    /// CSS id assigned to this display.
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
    /// Only advances while running (mirrors Python's per-display paused timer).
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

    /// Python `stop`: pause the timer, fold the segment into `total`, freeze time.
    fn stop(&mut self, ctx: &mut ReactiveCtx) {
        if self.running {
            self.total += self.start_instant.elapsed().as_secs_f64();
            self.running = false;
        }
        let total = self.total;
        self.set_total(total, ctx);
        self.set_time(total, ctx);
    }

    /// Python `reset`: total = 0, time = 0 (works while stopped).
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
// Stopwatch widget — buttons toggle `started` and command the TimeDisplay
// ---------------------------------------------------------------------------

struct Stopwatch {
    inner: HorizontalGroup,
    /// CSS id of this stopwatch's TimeDisplay child (`td-N`).
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

    fn focusable(&self) -> bool {
        false
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.inner.on_event(event, ctx);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.inner.on_event_capture(event, ctx);
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut EventCtx) {
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
                // Hand the start/stop/reset off to the app, which has the runtime
                // access needed to reach this stopwatch's TimeDisplay node.
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

const N: usize = 3;

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
        vec![BindingDecl::new("d", "toggle_dark", "Toggle dark mode")]
    }

    fn compose(&mut self) -> AppRoot {
        let stopwatches: Vec<ChildDecl> = (0..N)
            .map(|i| ChildDecl::from(Stopwatch::new()).with_id(&format!("sw-{i}")))
            .collect();

        AppRoot::new()
            .with_child(Header::new())
            .with_child(Footer::new())
            .with_child(VerticalScroll::new().with_child(Vertical::new().with_compose(stopwatches)))
    }

    fn on_mount_with_app(&mut self, app: &mut App, _ctx: &mut EventCtx) {
        // Python TimeDisplay.on_mount: set_interval(1/60, update_time, pause=True).
        // The app owns one 1/60s interval that drives `update_time` on every
        // TimeDisplay; each display only advances while it is `running` (the
        // per-display pause/resume lives in TimeDisplay state).
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

    fn on_message_with_app(&mut self, app: &mut App, message: &MessageEvent, ctx: &mut EventCtx) {
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
}

fn main() -> textual::Result<()> {
    run_sync(StopwatchApp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use textual::reactive::ReactiveWidget;

    /// Drive a TimeDisplay's recorded reactive changes through dispatch so the
    /// wrapped Digits re-renders (exactly what the runtime does each loop).
    fn flush(td: &mut TimeDisplay, mut ctx: ReactiveCtx) {
        let changes = ctx.take_changes();
        let mut dispatch_ctx = ReactiveCtx::new(textual::node_id::NodeId::default());
        td.reactive_dispatch(&changes, &mut dispatch_ctx);
    }

    #[test]
    fn stopwatch06_composes_without_panic() {
        let mut app = StopwatchApp;
        let _root = app.compose();
    }

    #[test]
    fn format_time_zero() {
        assert_eq!(format_time(0.0), "00:00:00.00");
    }

    #[test]
    fn format_time_minute() {
        assert_eq!(format_time(61.5), "00:01:01.50");
    }

    #[test]
    fn stopped_display_does_not_advance() {
        // A fresh (stopped) display ignores `update_time`.
        let mut td = TimeDisplay::new();
        td.start_instant = Instant::now() - Duration::from_secs(10);
        let mut ctx = ReactiveCtx::new(textual::node_id::NodeId::default());
        td.update_time(&mut ctx);
        assert!(!ctx.has_changes(), "stopped display must not advance");
        assert_eq!(td.inner.value(), "00:00:00.00");
    }

    /// start() + an advanced clock advances the displayed time via the reactive.
    #[test]
    fn start_then_update_advances_displayed_time() {
        let mut td = TimeDisplay::new();

        let mut ctx = ReactiveCtx::new(textual::node_id::NodeId::default());
        td.start(&mut ctx);
        assert!(td.running, "start() resumes the timer");

        // Simulate ~5s elapsed by backdating the segment origin (deterministic).
        td.start_instant = Instant::now() - Duration::from_secs(5);

        let mut ctx = ReactiveCtx::new(textual::node_id::NodeId::default());
        td.update_time(&mut ctx);
        assert!(ctx.has_changes(), "running display advances on update_time");
        assert!(*td.time() >= 5.0, "elapsed advanced: {}", td.time());
        flush(&mut td, ctx);
        assert!(
            td.inner.value().starts_with("00:00:05")
                || td.inner.value().starts_with("00:00:06"),
            "displayed ~5s: {}",
            td.inner.value()
        );
    }

    /// stop() folds the running segment into `total` and freezes the time.
    #[test]
    fn stop_accumulates_total_and_freezes() {
        let mut td = TimeDisplay::new();
        let mut ctx = ReactiveCtx::new(textual::node_id::NodeId::default());
        td.start(&mut ctx);
        td.start_instant = Instant::now() - Duration::from_secs(3);

        let mut ctx = ReactiveCtx::new(textual::node_id::NodeId::default());
        td.stop(&mut ctx);
        assert!(!td.running, "stop() pauses the timer");
        assert!(*td.total() >= 3.0, "total accumulated: {}", td.total());
        // time == total after stop.
        assert!((*td.time() - *td.total()).abs() < 1e-6);
        flush(&mut td, ctx);
        let frozen = td.inner.value().to_string();

        // After stop, further update_time calls do nothing (timer paused).
        td.start_instant = Instant::now() - Duration::from_secs(100);
        let mut ctx = ReactiveCtx::new(textual::node_id::NodeId::default());
        td.update_time(&mut ctx);
        assert!(!ctx.has_changes(), "paused display stays frozen");
        assert_eq!(td.inner.value(), frozen, "display stayed frozen after stop");
    }

    /// reset() zeroes the display even from a stopped, accumulated state.
    #[test]
    fn reset_zeroes_displayed_time() {
        let mut td = TimeDisplay::new();
        // Build up some accumulated time and a non-zero display.
        let mut ctx = ReactiveCtx::new(textual::node_id::NodeId::default());
        td.start(&mut ctx);
        td.start_instant = Instant::now() - Duration::from_secs(7);
        let mut ctx = ReactiveCtx::new(textual::node_id::NodeId::default());
        td.update_time(&mut ctx);
        flush(&mut td, ctx);
        assert_ne!(td.inner.value(), "00:00:00.00");

        // Reset works while stopped (Python: reset button has no `.started` gate).
        let mut ctx = ReactiveCtx::new(textual::node_id::NodeId::default());
        td.reset(&mut ctx);
        assert!(ctx.has_changes(), "reset records reactive changes");
        assert_eq!(*td.total(), 0.0);
        assert_eq!(*td.time(), 0.0);
        flush(&mut td, ctx);
        assert_eq!(td.inner.value(), "00:00:00.00", "reset zeroes the display");
    }

    #[test]
    fn time_display_ids_unique() {
        let a = TimeDisplay::new();
        let b = TimeDisplay::new();
        assert_ne!(a.display_id, b.display_id);
    }
}
