/// Port of Python Textual `docs/examples/guide/reactivity/recompose01.py`.
///
/// Displays a live clock (HH:MM:SS) using the `Digits` widget, centred on screen,
/// updating every second.
///
/// Python:
///   time: reactive[datetime] = reactive(datetime.now, init=False)
///   def compose(self): yield Digits(f"{self.time:%X}")
///   def watch_time(self): self.query_one(Digits).update(f"{self.time:%X}")   # (1)
///   def update_time(self): self.time = datetime.now()
///   on_mount: self.set_interval(1, self.update_time)   # (2)
///
/// Rust port (faithful): the app derives `Reactive` with
/// `#[reactive(watch_with_app, init = false)] time` (seconds-since-epoch stands in
/// for `datetime`). `watch_time` formats and updates the `Digits` widget — exactly
/// Python's `watch_time`. The 1-second `set_interval` is mirrored by
/// `on_tick_with_app` detecting second boundaries and calling `set_time(...)`,
/// which fires the watcher through the app reactive bridge.
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
    /// Mirrors Python `time = reactive(datetime.now, init=False)`.
    #[reactive(watch_with_app, init = false)]
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

    /// Python `watch_time`: update the Digits display from the current time.
    fn watch_time(&mut self, app: &mut App, _old: &u64, new: &u64, _ctx: &mut ReactiveCtx) {
        let text = format_hms(*new);
        let _ = app.with_query_one_mut_as::<Digits, _>("Digits", |digits| {
            digits.update(text);
        });
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
        // Python: yield Digits(f"{self.time:%X}").
        AppRoot::new().with_child(Digits::new(format_hms(*self.time())))
    }

    fn on_mount_with_app(&mut self, app: &mut App, _ctx: &mut EventCtx) {
        // Python: self.set_interval(1, self.update_time). The timer callback
        // re-enters the app struct and bumps the `time` reactive, which fires
        // `watch_time` through the app reactive bridge.
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
    fn format_hms_is_well_formed() {
        let t = format_hms(3661); // 01:01:01
        assert_eq!(t, "01:01:01");
    }

    #[test]
    fn set_time_records_change() {
        let mut app = Clock::new();
        let mut ctx = ReactiveCtx::new(textual::node_id::NodeId::default());
        app.set_time(*app.time() + 1, &mut ctx);
        assert!(ctx.has_changes(), "time change must be recorded");
    }

    /// LIVENESS PROBE — the 1-second `set_interval` must fire under the manual
    /// clock and drive `watch_time`, which updates the Digits display. We seed a
    /// known stale time then advance the clock so a tick re-reads `now_secs()`
    /// and repaints. A dead demo (no timer / unwired watch) leaves the Digits
    /// frame identical and fails this gate.
    #[test]
    fn liveness_interval_tick_updates_digits() {
        textual::run_test(Clock::new(), |pilot| {
            assert!(pilot.clock_is_manual());
            // The demo seeds Digits from the real wall clock at compose time; an
            // instant test means a tick re-reads the *same* wall second, so the
            // value wouldn't visibly change. Stamp a sentinel into the Digits
            // first, so a live interval tick (which overwrites it via watch_time)
            // is guaranteed to repaint. A dead demo (no timer / unwired watch)
            // leaves the sentinel in place and fails the gate.
            // Seed the app's `time` reactive to a stale sentinel (midnight) so the
            // next interval tick — which sets time to the real `now_secs()` —
            // produces a genuine value change. An instant test never lets wall
            // time advance, so without this the tick's set_time(now) would equal
            // the composed-at-startup value and record no change (masking liveness).
            pilot.app_mut().with_app_struct::<Clock, _>(
                |clock, app, _ctx| {
                    clock.set_time(0, app.reactive_ctx());
                },
                &mut EventCtx::default(),
            );
            // Flush the seed through the app-reactive bridge (a key press routes
            // via on_app_key -> dispatch_app_reactive), firing watch_time so the
            // Digits now read the sentinel "00:00:00".
            pilot.press(&["space"])?;
            let before = pilot.app().frame_fingerprint();
            // A live 1s interval tick re-reads the real wall clock, changing the
            // displayed time away from the sentinel. A dead demo (no timer /
            // unwired watch) leaves "00:00:00" in place and fails this gate.
            pilot.advance_clock(std::time::Duration::from_secs(1))?;
            let after = pilot.app().frame_fingerprint();
            assert_ne!(
                before, after,
                "a 1s interval tick must update the clock Digits"
            );
            Ok(())
        })
        .unwrap();
    }
}
