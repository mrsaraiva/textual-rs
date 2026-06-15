/// Port of Python Textual `docs/examples/how-to/inline01.py`.
///
/// Displays a live clock (HH:MM:SS) using the `Digits` widget, centered on screen.
/// Updates every second via the tick hook.
///
/// Python original runs with `app.run(inline=True)` (inline/non-fullscreen terminal
/// mode), which Rust does not yet support. This port runs in normal full-screen mode
/// as the closest faithful equivalent.
///
/// NOTE: This example is non-deterministic — it displays the live current time, which
/// changes every second and cannot be parity-verified by plain-text snapshot comparison.
use std::time::{SystemTime, UNIX_EPOCH};
use textual::prelude::*;

const CSS: &str = r#"
Screen {
    align: center middle;
}
#clock {
    width: auto;
}
"#;

fn current_time_local() -> String {
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
        AppRoot::new().with_compose(vec![ChildDecl::from(Digits::new("")).with_id("clock")])
    }

    fn on_mount_with_app(&mut self, app: &mut App, ctx: &mut EventCtx) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.last_second = now;
        let time = current_time_local();
        let _ = app.with_query_one_mut_as::<Digits, _>("#clock", |digits| {
            digits.update(time);
        });
        ctx.request_repaint();
    }

    fn on_tick_with_app(&mut self, app: &mut App, _tick: u64, ctx: &mut EventCtx) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if now != self.last_second {
            self.last_second = now;
            let time = current_time_local();
            let _ = app.with_query_one_mut_as::<Digits, _>("#clock", |digits| {
                digits.update(time);
            });
            ctx.request_repaint();
        }
    }
}

fn main() -> textual::Result<()> {
    run_sync(ClockApp::default())
}
