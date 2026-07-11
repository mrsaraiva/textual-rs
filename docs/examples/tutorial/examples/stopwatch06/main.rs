//! Port of Python Textual `docs/examples/tutorial/stopwatch06.py`, on the
//! WidgetCtx surface. Imports are ONLY `std` + `textual::prelude::*` — ZERO
//! runtime internals. Each `TimeDisplay` owns its 1/60s interval, so
//! `Pilot::advance_clock` drives the clock deterministically.
use std::time::Duration;
use textual::prelude::*;

const CSS: &str = r#"
Stopwatch { background: $boost; height: 5; margin: 1; min-width: 50; padding: 1; }
TimeDisplay { text-align: center; color: $foreground-muted; height: 3; }
Button { width: 16; }
#start { dock: left; }
#stop { dock: left; display: none; }
#reset { dock: right; }
.started { background: $success-muted; color: $text; }
.started TimeDisplay { color: $foreground; }
.started #start { display: none; }
.started #stop { display: block; }
.started #reset { visibility: hidden; }
"#;

/// `HH:MM:SS.cc` — mirrors Python `f"{hours:02,.0f}:{minutes:02.0f}:{seconds:05.2f}"`.
fn format_time(secs: f64) -> String {
    let cs = (secs * 100.0) as u64;
    format!("{:02}:{:02}:{:02}.{:02}", cs / 360_000, cs / 6_000 % 60, cs / 100 % 60, cs % 100)
}

/// A `Digits` showing elapsed time, advanced by its own paused 1/60s interval.
/// The timer's paused state IS the running state (start/stop resume/pause it).
#[textual::widget(base = Digits, reactive, override(on_mount))]
#[derive(textual::Reactive)]
struct TimeDisplay {
    base: Digits,
    #[reactive(watch, init = false)]
    time: f64,
    timer: Option<TimerHandle>,
}

impl TimeDisplay {
    fn new() -> Self {
        Self { base: Digits::new("00:00:00.00"), time: 0.0, timer: None }
    }
    /// Python `on_mount`: `set_interval(1/60, update_time, pause=True)`.
    fn on_mount(&mut self, ctx: &mut WidgetCtx) {
        let sixtieth = Duration::from_secs_f64(1.0 / 60.0);
        self.timer = Some(ctx.set_interval(sixtieth, true, |w: &mut Self, c, tick| w.tick(c, tick)));
    }
    /// `time` accumulates the REAL clock time elapsed since the previous fire
    /// (`tick.elapsed`) — drift-free vs Python's `monotonic() - start`, and
    /// deterministic under `Pilot::advance_clock` (one coalesced fire still
    /// advances by the true elapsed time, not a fixed nominal 1/60s).
    fn tick(&mut self, ctx: &mut WidgetCtx, tick: TimerTick) {
        self.set_time(self.time + tick.elapsed.as_secs_f64(), ctx);
    }
    fn watch_time(&mut self, _old: &f64, new: &f64, _ctx: &mut ReactiveCtx) {
        self.base.update(format_time(*new));
    }
    fn start(&mut self) { if let Some(t) = self.timer { t.resume(); } }
    fn stop(&mut self) { if let Some(t) = self.timer { t.pause(); } }
    fn reset(&mut self, ctx: &mut WidgetCtx) { self.set_time(0.0, ctx); }
}

/// A stopwatch: three buttons + a `TimeDisplay`, wired via `#[on]` + `query_one`.
#[textual::widget(base = HorizontalGroup, on(on_button))]
struct Stopwatch {
    base: HorizontalGroup,
}

impl Stopwatch {
    fn new() -> Self {
        Self {
            base: HorizontalGroup::new().with_compose(vec![
                ChildDecl::from(Button::success("Start").id("start")),
                ChildDecl::from(Button::error("Stop").id("stop")),
                ChildDecl::from(Button::new("Reset").id("reset")),
                ChildDecl::from(TimeDisplay::new()),
            ]),
        }
    }
    /// Python `on_button_pressed`: query the `TimeDisplay`, drive it, toggle class.
    #[textual::on(ButtonPressed)]
    fn on_button(&mut self, event: &ButtonPressed, ctx: &mut WidgetCtx) {
        let td = ctx.query_one::<TimeDisplay>();
        match event.button_id.as_deref() {
            Some("start") => { td.update_via(ctx, |d, _| d.start()); ctx.add_class("started"); }
            Some("stop") => { td.update_via(ctx, |d, _| d.stop()); ctx.remove_class("started"); }
            Some("reset") => td.update_via(ctx, |d, c| d.reset(c)),
            _ => {}
        }
    }
}

struct StopwatchApp;

impl TextualApp for StopwatchApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }
    fn bindings(&self) -> Vec<BindingDecl> {
        vec![BindingDecl::new("d", "toggle_dark", "Toggle dark mode")]
    }
    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Header::new())
            .with_child(Footer::new())
            .with_child(VerticalScroll::new().with_compose(vec![
                ChildDecl::from(Stopwatch::new()),
                ChildDecl::from(Stopwatch::new()),
                ChildDecl::from(Stopwatch::new()),
            ]))
    }
}

fn main() -> textual::Result<()> {
    run_sync(StopwatchApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_time_values() {
        assert_eq!(format_time(0.0), "00:00:00.00");
        assert_eq!(format_time(61.5), "00:01:01.50");
    }

    /// Deterministic drive via the widget-owned timer: Start advances the clock
    /// under `advance_clock`; Stop freezes it; Reset zeroes it. Exercises the full
    /// WidgetCtx path (on_mount set_interval + #[on] query_one/update_via +
    /// TimerHandle pause/resume + reactive watch → Digits).
    #[test]
    fn advance_clock_runs_and_pauses_the_stopwatch() {
        textual::run_test(StopwatchApp, |pilot| {
            // Clicking now moves focus to the pressed Button (Python
            // `Screen._forward_event` click-to-focus), so the `:focus` styling
            // is part of every post-click frame. Park focus on #reset FIRST so
            // the idle fingerprint and the final post-reset fingerprint carry
            // the same focus state and compare equal on the display alone.
            pilot.click("#reset")?;
            let idle = pilot.app().frame_fingerprint();
            pilot.click("#start")?;
            pilot.advance_clock(Duration::from_secs(1))?;
            assert_ne!(idle, pilot.app().frame_fingerprint(), "Start + 1s must advance the clock");

            pilot.click("#stop")?;
            let stopped = pilot.app().frame_fingerprint();
            pilot.advance_clock(Duration::from_secs(2))?;
            assert_eq!(stopped, pilot.app().frame_fingerprint(), "Stop must freeze the clock");

            pilot.click("#reset")?;
            assert_eq!(idle, pilot.app().frame_fingerprint(), "Reset must zero the display");
            Ok(())
        })
        .unwrap();
    }
}
