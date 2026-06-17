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
/// Each second the tick calls `set_time(...)`, which records a recompose change;
/// the app reactive bridge then re-invokes `compose()` (rebuilding the `Digits`)
/// via `App::recompose_app` — exactly Python's `recompose=True`. No `watch_time`
/// is needed: the fresh `compose()` reads the current `time`.
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
    /// Mirrors Python `time = reactive(datetime.now, recompose=True)`.
    #[reactive(recompose)]
    time: u64,
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

    fn on_tick_with_app(&mut self, app: &mut App, _tick: u64, ctx: &mut EventCtx) {
        let secs = now_secs();
        if secs != self.last_second {
            self.last_second = secs;
            // Recompose reactive: the bridge re-invokes compose() to rebuild Digits.
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
    fn set_time_requests_recompose() {
        let mut app = Clock::new();
        let mut ctx = ReactiveCtx::new(textual::node_id::NodeId::default());
        app.set_time(*app.time() + 1, &mut ctx);
        assert!(ctx.has_changes());
        assert!(ctx.needs_recompose(), "recompose reactive must request recompose");
    }
}
