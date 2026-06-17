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
    /// Tracks the last applied second so the tick only updates once per second.
    last_second: u64,
}

impl Clock {
    fn new() -> Self {
        let now = now_secs();
        Self {
            time: now,
            last_second: now,
        }
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

    fn on_tick_with_app(&mut self, app: &mut App, _tick: u64, ctx: &mut EventCtx) {
        // Python `set_interval(1, update_time)` → `self.time = datetime.now()`.
        let secs = now_secs();
        if secs != self.last_second {
            self.last_second = secs;
            self.set_time(secs, app.reactive_ctx());
            ctx.request_repaint();
        }
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
}
