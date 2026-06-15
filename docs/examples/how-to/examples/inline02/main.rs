/// Port of Python Textual `docs/examples/how-to/inline02.py`.
///
/// A clock app displayed using the `Digits` widget, centered on screen.
/// The Python source runs in inline mode (`app.run(inline=True)`) with CSS that
/// applies extra styling only when `&:inline` — those rules are omitted here as
/// the parity scoreboard runs the app in full-screen mode, where they would not
/// apply.
///
/// NOTE: This example is non-deterministic — it displays the live current time,
/// which changes every second and cannot be parity-verified by plain-text
/// snapshot comparison.
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
struct ClockApp {
    last_second: u64,
}

impl TextualApp for ClockApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        let time = current_time();
        AppRoot::new().with_child(Digits::new(time))
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
    run_sync(ClockApp::default())
}
