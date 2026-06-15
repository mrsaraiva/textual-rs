/// Port of Python Textual `docs/examples/tutorial/stopwatch05.py`.
///
/// Adds actual time tracking to each Stopwatch. Python's `TimeDisplay` uses
/// `set_interval(1/60, self.update_time)` and reactive `start_time`/`total`.
/// `start()`/`stop()`/`reset()` control the timer.
///
/// In Rust, each `Stopwatch` widget maintains its own timer state and updates
/// its internal `Digits` widget on every `on_tick` call (using `std::time::Instant`).
/// Since the composed Digits are in the arena tree after mount, they are updated
/// via the `on_app_tick` hook and the app query API.
///
/// Architecture:
/// - `Stopwatch` widget tracks `started`, `start_instant`, `total_elapsed`.
/// - Buttons inside each `Stopwatch` post `ButtonPressed`; the `Stopwatch.on_message`
///   handles start/stop/reset and updates CSS classes via `ctx.add_class`/`remove_class`.
/// - Per-stopwatch Digits get unique ids ("digits-0", "digits-1", "digits-2").
/// - App-level `on_tick_with_app` refreshes running Digits each tick.
///
/// NON-PROMOTABLE (timer-driven): display changes every frame.
use std::time::Instant;
use textual::prelude::*;

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

// ---------------------------------------------------------------------------
// TimeDisplay widget
// ---------------------------------------------------------------------------

struct TimeDisplay {
    inner: Digits,
}

impl TimeDisplay {
    fn new(text: &str) -> Self {
        Self {
            inner: Digits::new(text),
        }
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
}

// ---------------------------------------------------------------------------
// Stopwatch widget
// ---------------------------------------------------------------------------

struct Stopwatch {
    inner: HorizontalGroup,
    #[allow(dead_code)]
    idx: usize,
    started: bool,
    start_instant: Option<Instant>,
    total_elapsed_secs: f64,
}

impl Stopwatch {
    fn new(idx: usize) -> Self {
        let digits_id = format!("digits-{idx}");
        let inner = HorizontalGroup::new().with_compose(vec![
            ChildDecl::from(Button::success("Start").id("start")),
            ChildDecl::from(Button::error("Stop").id("stop")),
            ChildDecl::from(Button::new("Reset").id("reset")),
            ChildDecl::from(TimeDisplay::new("00:00:00.00")).with_id(&digits_id),
        ]);
        Self {
            inner,
            idx,
            started: false,
            start_instant: None,
            total_elapsed_secs: 0.0,
        }
    }

    fn elapsed_secs(&self) -> f64 {
        let base = self.total_elapsed_secs;
        if let Some(inst) = &self.start_instant {
            base + inst.elapsed().as_secs_f64()
        } else {
            base
        }
    }
}

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
            match bp.button_id.as_deref() {
                Some("start") => {
                    self.started = true;
                    self.start_instant = Some(Instant::now());
                    ctx.add_class("started");
                    ctx.request_repaint();
                    ctx.set_handled();
                }
                Some("stop") => {
                    if let Some(inst) = self.start_instant.take() {
                        self.total_elapsed_secs += inst.elapsed().as_secs_f64();
                    }
                    self.started = false;
                    ctx.remove_class("started");
                    ctx.request_repaint();
                    ctx.set_handled();
                }
                Some("reset") => {
                    self.total_elapsed_secs = 0.0;
                    self.start_instant = None;
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
            .map(|i| ChildDecl::from(Stopwatch::new(i)).with_id(&format!("sw-{i}")))
            .collect();

        AppRoot::new()
            .with_child(Header::new())
            .with_child(Footer::new())
            .with_child(VerticalScroll::new().with_child(Vertical::new().with_compose(stopwatches)))
    }

    fn on_tick_with_app(&mut self, app: &mut App, _tick: u64, ctx: &mut EventCtx) {
        for i in 0..N {
            let sw_id = format!("#sw-{i}");
            let digits_id = format!("#digits-{i}");
            // Get elapsed time from the running stopwatch (if any)
            let maybe_time = app
                .with_query_one_mut_as::<Stopwatch, _>(&sw_id, |sw| {
                    if sw.started {
                        Some(format_time(sw.elapsed_secs()))
                    } else {
                        None
                    }
                })
                .ok()
                .flatten();
            if let Some(time_str) = maybe_time {
                let _ = app.with_query_one_mut_as::<TimeDisplay, _>(&digits_id, |d| {
                    d.inner.update(time_str);
                });
                ctx.request_repaint();
            }
        }
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
}
