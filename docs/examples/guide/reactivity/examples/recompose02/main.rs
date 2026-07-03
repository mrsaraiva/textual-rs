/// Port of Python Textual `docs/examples/guide/reactivity/recompose02.py`.
///
/// Displays a live clock (HH:MM:SS) using the `Digits` widget, centred on screen,
/// updating every second — via APP-LEVEL recompose.
///
/// Python:
///   time: reactive[datetime] = reactive(datetime.now, recompose=True)
///   def compose(self): yield Digits(f"{self.time:%X}")
///   def update_time(self): self.time = datetime.now()
///   on_mount: self.set_interval(1, self.update_time)
///
/// Rust port (faithful): the app derives `Reactive` with
/// `#[reactive(recompose)] time` (seconds-since-epoch stands in for `datetime`).
/// A real 1-second `set_interval` callback re-enters the app struct and calls
/// `set_time(...)` (Python's `update_time`), which records a recompose change; the
/// app reactive bridge then re-invokes `compose()` (rebuilding the `Digits`) —
/// exactly Python's `recompose=True`. No `watch_time` is needed: the fresh
/// `compose()` reads the current `time`.
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use textual::prelude::*;

const CSS: &str = r#"
Screen {
    align: center middle;
}
Digits {
    width: auto;
}
"#;

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn format_hms(secs: u64) -> String {
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3600) % 24;
    format!("{h:02}:{m:02}:{s:02}")
}

#[derive(Reactive)]
struct Clock {
    /// Mirrors Python `time = reactive(datetime.now, recompose=True)`.
    #[reactive(recompose)]
    time: u64,
}

impl Clock {
    fn new() -> Self {
        Self { time: now_secs() }
    }

    /// Python `update_time`: `self.time = datetime.now()`.
    fn update_time(&mut self, ctx: &mut ReactiveCtx) {
        self.set_time(now_secs(), ctx);
    }
}

impl TextualApp for Clock {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn reactive_widget_mut(&mut self) -> Option<&mut dyn ReactiveWidget> {
        Some(self)
    }

    fn compose(&mut self) -> AppRoot {
        // Python: yield Digits(f"{self.time:%X}"). Recompose re-runs this.
        AppRoot::new().with_child(Digits::new(format_hms(*self.time())))
    }

    fn on_mount_with_app(&mut self, app: &mut App, _ctx: &mut textual::event::WidgetCtx) {
        // Python: self.set_interval(1, self.update_time). The recompose reactive
        // re-invokes compose() to rebuild the Digits each second.
        app.set_interval(
            Duration::from_secs(1),
            None,
            false,
            Box::new(|app, ctx| {
                app.with_app_struct::<Clock, _>(
                    |clock, app, _ctx| {
                        clock.update_time(app.reactive_ctx());
                    },
                    ctx,
                );
            }),
        );
    }
}

fn main() -> textual::Result<()> {
    run_sync(Clock::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compose_does_not_panic() {
        let mut app = Clock::new();
        let _root = app.compose();
    }

    #[test]
    fn set_time_requests_recompose() {
        let mut app = Clock::new();
        let mut ctx = ReactiveCtx::new(textual::node_id::NodeId::default());
        app.set_time(*app.time() + 1, &mut ctx);
        assert!(ctx.has_changes());
        assert!(ctx.needs_recompose(), "recompose reactive must request recompose");
    }

    /// LIVENESS PROBE — the 1-second `set_interval` must fire under the manual
    /// clock and drive the recompose reactive, re-running `compose()` to rebuild
    /// the Digits from the new time. We seed a sentinel time, flush it (so the
    /// recompose renders "00:00:00"), then advance the clock so a live tick
    /// re-reads the real wall clock and recomposes. A dead demo (no timer /
    /// recompose not wired) leaves the sentinel and fails this gate.
    #[test]
    fn liveness_interval_tick_recomposes_digits() {
        textual::run_test(Clock::new(), |pilot| {
            assert!(pilot.clock_is_manual());
            pilot.app_mut().with_app_struct::<Clock, _>(
                |clock, app, _ctx| {
                    clock.set_time(0, app.reactive_ctx());
                },
                &mut textual::event::WidgetCtx::default(),
            );
            // Flush the seed via the app-reactive bridge (key press routes through
            // on_app_key -> dispatch_app_reactive -> recompose).
            pilot.press(&["space"])?;
            let before = pilot.app().frame_fingerprint();
            pilot.advance_clock(Duration::from_secs(1))?;
            let after = pilot.app().frame_fingerprint();
            assert_ne!(
                before, after,
                "a 1s interval tick must recompose the clock Digits"
            );
            Ok(())
        })
        .unwrap();
    }
}
