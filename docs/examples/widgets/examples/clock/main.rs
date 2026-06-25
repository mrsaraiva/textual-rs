/// Port of Python Textual `docs/examples/widgets/clock.py`.
///
/// Displays a live clock (HH:MM:SS) using the `Digits` widget, centered on screen.
/// Updates every second via a real `set_interval` timer (Python parity).
///
/// Python uses `Digits` with `id="clock"` and targets it via `#clock { width: auto; }`.
/// The Rust `Digits` widget does not expose a `with_id()` builder, so we target it
/// via the type selector `Digits { width: auto; }`, which produces identical visual
/// output for this single-widget app.
///
/// Python:
///   on_ready: self.update_clock(); self.set_interval(1, self.update_clock)
///   update_clock: self.query_one(Digits).update(f"{clock:%T}")
///
/// Rust port (faithful): `on_mount_with_app` registers `app.set_interval(1s, ...)`.
/// The timer callback queries the `Digits` widget and updates it — exactly Python's
/// `update_clock`. Uses `chrono::Local::now()` to match Python `datetime.now()`
/// (local wall-clock time, not UTC).
///
/// NOTE: This example is non-deterministic — it displays the live current time, which
/// changes every second and cannot be parity-verified by plain-text snapshot comparison.
use chrono::Timelike;
use textual::prelude::*;

const CSS: &str = r#"
Screen {
    align: center middle;
}
Digits {
    width: auto;
}
"#;

/// Compute current LOCAL time as "HH:MM:SS".
///
/// Mirrors Python `datetime.now().time()` formatted as `%T` (HH:MM:SS).
fn current_time_local() -> String {
    let now = chrono::Local::now();
    format!("{:02}:{:02}:{:02}", now.hour(), now.minute(), now.second())
}

/// Python `update_clock`: set the Digits display to the current time.
fn update_clock(app: &mut App, ctx: &mut EventCtx) {
    let time = current_time_local();
    let _ = app.with_query_one_mut_as::<Digits, _>("Digits", |digits| {
        digits.update(time);
    });
    ctx.request_repaint();
}

#[derive(Default)]
struct ClockApp;

impl TextualApp for ClockApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        let time = current_time_local();
        AppRoot::new().with_child(Digits::new(time))
    }

    fn on_mount_with_app(&mut self, app: &mut App, _ctx: &mut EventCtx) {
        // Python: self.set_interval(1, self.update_clock).
        app.set_interval(
            std::time::Duration::from_secs(1),
            None,
            false,
            Box::new(|app, ctx| update_clock(app, ctx)),
        );
    }
}

fn main() -> textual::Result<()> {
    run_sync(ClockApp::default())
}

#[cfg(test)]
mod liveness {
    use super::*;
    use std::time::Duration;
    use textual::run_test;

    /// LIVENESS: the clock registers a 1s repeating timer in `on_mount`
    /// (Python `set_interval(1, self.update_clock)`). Advancing the manual test
    /// clock fires the timer callback, which re-renders the Digits widget.
    ///
    /// NOTE: the callback reads wall-clock `chrono::Local::now()` (not the manual
    /// test clock), so the displayed time only changes when a real second
    /// elapses between fires. We sleep ~1.1s of real time across the advance so
    /// the wall second rolls over, making the frame change observable and the
    /// timer wiring demonstrable.
    #[test]
    fn timer_tick_updates_clock_frame() {
        run_test(ClockApp::default(), |pilot| {
            assert!(
                pilot.clock_is_manual(),
                "run_test must install the deterministic manual clock"
            );
            let before = pilot.app().frame_fingerprint();
            // Fire the 1s interval a few times; the demo's callback reads
            // real wall-clock time, so straddle a real-second boundary.
            pilot.advance_clock(Duration::from_secs(1))?;
            std::thread::sleep(Duration::from_millis(1100));
            pilot.advance_clock(Duration::from_secs(1))?;
            let after = pilot.app().frame_fingerprint();
            assert_ne!(
                before, after,
                "the 1s clock timer must fire and update the displayed time"
            );
            Ok(())
        })
        .unwrap();
    }
}
