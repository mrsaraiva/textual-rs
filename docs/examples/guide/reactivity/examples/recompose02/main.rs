/// Port of Python Textual `docs/examples/guide/reactivity/recompose02.py`.
///
/// Displays a live clock (HH:MM:SS) using the `Digits` widget, centered on screen,
/// updating every second.
///
/// Python uses a `reactive` field with `recompose=True` so that each second
/// `compose()` is re-invoked and a fresh `Digits` is mounted.  Rust textual-rs
/// has no reactive-recompose mechanism, so the equivalent is to hold a tick
/// counter, detect second boundaries, and call `Digits::update()` directly via
/// `App::with_query_one_mut_as`.  The visible output is identical: the clock
/// advances once per second, centered on screen.
///
/// Framework gap: no reactive-recompose API (`reactive(recompose=True)`) exists
/// in textual-rs.  A future `App::recompose()` primitive would close this gap.
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

fn current_time_hms() -> String {
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
        AppRoot::new().with_child(Digits::new(current_time_hms()))
    }

    fn on_tick_with_app(&mut self, app: &mut App, _tick: u64, ctx: &mut EventCtx) {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if secs != self.last_second {
            self.last_second = secs;
            let time = current_time_hms();
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
