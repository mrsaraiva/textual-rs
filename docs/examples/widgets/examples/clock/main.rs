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
/// `update_clock`. No `on_tick` second-boundary faking.
///
/// NOTE: This example is non-deterministic — it displays the live current time, which
/// changes every second and cannot be parity-verified by plain-text snapshot comparison.
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

/// Compute current UTC time as "HH:MM:SS".
fn current_time_utc() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3600) % 24;
    format!("{h:02}:{m:02}:{s:02}")
}

/// Python `update_clock`: set the Digits display to the current time.
fn update_clock(app: &mut App, ctx: &mut EventCtx) {
    let time = current_time_utc();
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
        let time = current_time_utc();
        AppRoot::new().with_child(Digits::new(time))
    }

    fn on_mount_with_app(&mut self, app: &mut App, _ctx: &mut EventCtx) {
        // Python: self.set_interval(1, self.update_clock).
        app.set_interval(
            Duration::from_secs(1),
            None,
            false,
            Box::new(|app, ctx| update_clock(app, ctx)),
        );
    }
}

fn main() -> textual::Result<()> {
    run_sync(ClockApp::default())
}
