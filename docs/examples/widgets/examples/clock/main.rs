/// Port of Python Textual `docs/examples/widgets/clock.py`.
///
/// Displays a live clock (HH:MM:SS) using the `Digits` widget, centered on screen.
/// Updates every second via the tick hook.
///
/// Python uses `Digits` with `id="clock"` and targets it via `#clock { width: auto; }`.
/// The Rust `Digits` widget does not expose a `with_id()` builder, so we target it
/// via the type selector `Digits { width: auto; }`, which produces identical visual
/// output for this single-widget app.
///
/// NOTE: This example is non-deterministic — it displays the live current time, which
/// changes every second and cannot be parity-verified by plain-text snapshot comparison.
use std::time::{SystemTime, UNIX_EPOCH};
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

#[derive(Default)]
struct ClockApp {
    last_second: u64,
}

impl TextualApp for ClockApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        let time = current_time_utc();
        AppRoot::new().with_child(Digits::new(time))
    }

    fn on_tick_with_app(&mut self, app: &mut App, _tick: u64, ctx: &mut EventCtx) {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if secs != self.last_second {
            self.last_second = secs;
            let time = current_time_utc();
            let _ = app.with_query_one_mut_as::<Digits, _>("Digits", |digits| {
                digits.update(time);
            });
            ctx.request_repaint();
        }
    }
}

fn main() -> textual::Result<()> {
    run_sync(ClockApp::default())
}
