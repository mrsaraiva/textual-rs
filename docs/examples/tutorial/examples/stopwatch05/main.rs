/// Port of Python Textual `docs/examples/tutorial/stopwatch05.py`.
///
/// Adds real elapsed-time tracking to each Stopwatch using the framework's
/// timer + reactive fundamentals (the same primitives Python uses), NOT an
/// app-level tick/id-query push.
///
/// Python (stopwatch05.py):
///   class TimeDisplay(Digits):
///       start_time = reactive(monotonic)
///       time = reactive(0.0)
///       def on_mount(self): self.set_interval(1 / 60, self.update_time)
///       def update_time(self): self.time = monotonic() - self.start_time
///       def watch_time(self, time): self.update(format(time))
///   class Stopwatch(HorizontalGroup):
///       def on_button_pressed(self, event):
///           if event.button.id == "start": self.add_class("started")
///           elif event.button.id == "stop": self.remove_class("started")
///
/// Notes:
///   - The clock ticks from MOUNT, unconditionally (exactly like Python). The
///     Start/Stop buttons ONLY toggle the `started` CSS class — they do not
///     start/stop the clock in stopwatch05. (start/stop/reset arrive in 06.)
///   - Rust faithful mapping: `TimeDisplay` is a `#[derive(Reactive)]` widget
///     wrapping a `Digits`, with a `time` reactive. `update_time` sets `time` to
///     the elapsed seconds since `start_instant`; `watch_time` formats and pushes
///     into the wrapped `Digits`. Widgets can't self-register timers in this
///     runtime, so the app registers ONE `set_interval(1/60)` at mount that fans
///     the `update_time` call out to every `TimeDisplay` (the runtime's reactive
///     dispatch then re-renders each one). This preserves the Python contract:
///     the timer drives the reactive, the reactive re-renders the display.
///
/// NON-PROMOTABLE (timer-driven): display changes every frame.
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

// Unique id per TimeDisplay so the interval fan-out can address each one.
static NEXT_DISPLAY: AtomicU64 = AtomicU64::new(0);

// ---------------------------------------------------------------------------
// TimeDisplay widget — a Digits with a `time` reactive driven by a timer
// ---------------------------------------------------------------------------

#[derive(Reactive)]
struct TimeDisplay {
    /// Elapsed seconds; the timer sets this, `watch_time` renders it.
    #[reactive(watch, init = false)]
    time: f64,
    /// Mount-time origin for elapsed-time computation (Python `start_time`).
    start_instant: Instant,
    /// The wrapped `Digits` that actually renders the formatted time.
    inner: Digits,
    /// CSS id assigned to this display so the interval can target it.
    display_id: String,
}

impl TimeDisplay {
    fn new() -> Self {
        let n = NEXT_DISPLAY.fetch_add(1, Ordering::Relaxed);
        Self {
            time: 0.0,
            start_instant: Instant::now(),
            inner: Digits::new("00:00:00.00"),
            display_id: format!("td-{n}"),
        }
    }

    /// Python `update_time`: `self.time = monotonic() - self.start_time`.
    fn update_time(&mut self, ctx: &mut ReactiveCtx) {
        let elapsed = self.start_instant.elapsed().as_secs_f64();
        self.set_time(elapsed, ctx);
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
// Stopwatch widget — Start/Stop buttons only toggle the `started` class
// ---------------------------------------------------------------------------

struct Stopwatch {
    inner: HorizontalGroup,
}

impl Stopwatch {
    fn new() -> Self {
        let inner = HorizontalGroup::new().with_compose(vec![
            ChildDecl::from(Button::success("Start").id("start")),
            ChildDecl::from(Button::error("Stop").id("stop")),
            ChildDecl::from(Button::new("Reset").id("reset")),
            {
                let td = TimeDisplay::new();
                let id = td.display_id.clone();
                ChildDecl::from(td).with_id(&id)
            },
        ]);
        Self { inner }
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
            match bp.button_id.as_deref() {
                // Python stopwatch05: buttons ONLY toggle the `started` class.
                Some("start") => {
                    ctx.add_class("started");
                    ctx.request_repaint();
                    ctx.set_handled();
                }
                Some("stop") => {
                    ctx.remove_class("started");
                    ctx.request_repaint();
                    ctx.set_handled();
                }
                _ => {}
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

    fn on_mount_with_app(&mut self, app: &mut App, _ctx: &mut textual::event::WidgetCtx) {
        // Python TimeDisplay.on_mount: self.set_interval(1 / 60, self.update_time).
        // Widgets can't self-register timers in this runtime, so the app owns one
        // 1/60s interval that drives `update_time` on every TimeDisplay. Each call
        // bumps that display's `time` reactive, whose watcher re-renders it.
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
}

fn main() -> textual::Result<()> {
    run_sync(StopwatchApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stopwatch05_composes_without_panic() {
        let mut app = StopwatchApp;
        let _root = app.compose();
    }

    #[test]
    fn format_time_zero() {
        assert_eq!(format_time(0.0), "00:00:00.00");
    }

    #[test]
    fn format_time_one_hour() {
        assert_eq!(format_time(3600.0), "01:00:00.00");
    }

    #[test]
    fn format_time_minute_and_centis() {
        assert_eq!(format_time(61.5), "00:01:01.50");
    }

    /// Deterministic timer+reactive wiring: simulate a clock advance by moving
    /// the display's origin into the past, then run `update_time` and dispatch the
    /// recorded reactive change (exactly what the runtime does after the interval
    /// callback). The reactive must record a change and `watch_time` must advance
    /// the rendered Digits.
    #[test]
    fn update_time_advances_displayed_time() {
        use textual::reactive::ReactiveWidget;

        let mut td = TimeDisplay::new();
        // Display starts at zero.
        assert_eq!(td.inner.value(), "00:00:00.00");

        // Simulate ~5s elapsed by backdating the origin (deterministic, no sleep).
        td.start_instant = Instant::now() - Duration::from_secs(5);

        // Step 1: the timer drives `update_time`, which bumps the `time` reactive.
        let mut ctx = ReactiveCtx::new(textual::node_id::NodeId::default());
        td.update_time(&mut ctx);
        assert!(ctx.has_changes(), "update_time must change the time reactive");
        // The `time` reactive advanced past 5s.
        assert!(*td.time() >= 5.0, "elapsed time advanced: {}", td.time());

        // Step 2: the runtime dispatches the recorded change -> `watch_time` runs.
        let changes = ctx.take_changes();
        let mut dispatch_ctx = ReactiveCtx::new(textual::node_id::NodeId::default());
        td.reactive_dispatch(&changes, &mut dispatch_ctx);

        // `watch_time` re-rendered the Digits to the advanced value.
        assert_ne!(
            td.inner.value(),
            "00:00:00.00",
            "displayed time advanced from zero"
        );
        assert!(
            td.inner.value().starts_with("00:00:05")
                || td.inner.value().starts_with("00:00:06"),
            "displayed ~5s: {}",
            td.inner.value()
        );
    }

    #[test]
    fn time_display_ids_unique() {
        let a = TimeDisplay::new();
        let b = TimeDisplay::new();
        assert_ne!(a.display_id, b.display_id);
    }

    // -- LIVENESS PROBE (Pilot run_test) --------------------------------------
    // stopwatch05 wires the buttons to toggle the `started` class (the timer
    // itself is free-running from mount). Clicking Start adds `started`, and the
    // CSS `.started` rules flip Start/Stop `display` and recolour the row, so
    // the click must change the rendered frame.
    //
    // NOTE: the running-clock digit advance is intentionally NOT probed here —
    // the displayed time derives from a wall-clock `Instant::elapsed()`, not the
    // manual timer clock, so `Pilot::advance_clock` cannot reproduce it
    // deterministically (the demos flag this as "NON-PROMOTABLE: timer-driven").
    // The deterministic, observable interaction is the `started` class toggle.
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
