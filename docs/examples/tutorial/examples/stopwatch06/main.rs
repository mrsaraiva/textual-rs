//! Port of Python Textual `docs/examples/tutorial/stopwatch06.py`, rewritten on
//! the WidgetCtx surface. Imports are ONLY `std` + `textual::prelude::*` — ZERO
//! runtime internals (no `enqueue_runtime_reactive_entry` / `RuntimeReactiveEntry`
//! / `ReactiveCtx::new` / `with_widget_mut_as`). Each `TimeDisplay` owns its 1/60s
//! interval, so `Pilot::advance_clock` drives the clock deterministically.
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
#[textual::widget(base = Digits, reactive, override(on_mount_ctx))]
#[derive(textual::Reactive)]
struct TimeDisplay {
    base: Digits,
    #[reactive(watch, init = false)]
    time: f64,
    running: bool,
    timer: Option<TimerHandle>,
}

impl TimeDisplay {
    fn new() -> Self {
        Self { base: Digits::new("00:00:00.00"), time: 0.0, running: false, timer: None }
    }
    /// Python `TimeDisplay.on_mount`: `set_interval(1/60, update_time, pause=True)`.
    fn on_mount_ctx(&mut self, ctx: &mut WidgetCtx) {
        let dt = Duration::from_secs_f64(1.0 / 60.0);
        self.timer = Some(ctx.set_interval::<Self, _>(dt, true, |w, c| w.tick(c)));
    }
    /// Deterministic tick: `time` is the accumulator (advances 1/60s per fire).
    fn tick(&mut self, ctx: &mut WidgetCtx) {
        if self.running {
            let t = self.time + 1.0 / 60.0;
            self.set_time(t, ctx);
        }
    }
    fn watch_time(&mut self, _old: &f64, new: &f64, _ctx: &mut ReactiveCtx) {
        self.base.update(format_time(*new));
    }
    fn start(&mut self, _ctx: &mut WidgetCtx) {
        self.running = true;
        if let Some(t) = self.timer { t.resume(); }
    }
    fn stop(&mut self, _ctx: &mut WidgetCtx) {
        self.running = false;
        if let Some(t) = self.timer { t.pause(); }
    }
    fn reset(&mut self, ctx: &mut WidgetCtx) {
        self.set_time(0.0, ctx);
    }
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
    /// Python `Stopwatch.on_button_pressed`: query the `TimeDisplay`, drive it.
    #[textual::on(ButtonPressed)]
    fn on_button(&mut self, event: &ButtonPressed, ctx: &mut WidgetCtx) {
        let td = ctx.query_one::<TimeDisplay>();
        match event.button_id.as_deref() {
            Some("start") => { td.update_via(ctx, |d, c| d.start(c)); ctx.add_class("started"); }
            Some("stop") => { td.update_via(ctx, |d, c| d.stop(c)); ctx.remove_class("started"); }
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
    /// WidgetCtx path (on_mount_ctx set_interval + #[on] query_one/update_via +
    /// TimerHandle pause/resume + reactive watch → Digits).
    #[test]
    fn advance_clock_runs_and_pauses_the_stopwatch() {
        textual::run_test(StopwatchApp, |pilot| {
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
