/// Port of Python Textual `docs/examples/guide/reactivity/recompose01.py`.
///
/// Displays a live clock (HH:MM:SS) using the `Digits` widget, centered on screen,
/// updating every second.
///
/// Python uses a `reactive` time field and a `watch_time` watcher that calls
/// `self.query_one(Digits).update(...)`.  The Rust equivalent uses `on_tick_with_app`
/// to poll each second and call `digits.update(...)` — same observable behavior.
///
/// NOTE: Non-deterministic output (live time) — cannot be verified by plain-text snapshot.
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

fn current_time() -> String {
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
struct Clock {
    last_second: u64,
}

impl TextualApp for Clock {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Digits::new(current_time()))
    }

    fn on_tick_with_app(&mut self, app: &mut App, _tick: u64, ctx: &mut EventCtx) {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if secs != self.last_second {
            self.last_second = secs;
            let time = current_time();
            let _ = app.with_query_one_mut_as::<Digits, _>("Digits", |digits| {
                digits.update(time);
            });
            ctx.request_repaint();
        }
    }
}

fn main() -> textual::Result<()> {
    run_sync(Clock::default())
}
