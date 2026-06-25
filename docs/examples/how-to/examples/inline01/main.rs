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

#[cfg(test)]
mod tests {
    use super::*;

    /// LIVENESS probe (Pilot, headless): the live clock updates the `Digits`
    /// readout once per second via the tick hook, so the rendered frame should
    /// change as time advances.
    ///
    /// UNCLEAR under the headless harness — `#[ignore]`d. Two compounding roots:
    /// (1) `on_tick_with_app` is driven by the *live* event loop's wall-clock
    /// tick cadence and is NOT invoked by the headless pump —
    /// `Pilot::advance_clock` fires `set_interval`/`set_timer` callbacks (the
    /// manual timer clock), not the tick hook; and (2) the displayed time comes
    /// from wall-clock `SystemTime::now()` rather than the manual test clock, and
    /// the demo only repaints when the wall-clock second flips
    /// (`now != self.last_second`), so it cannot be advanced deterministically.
    /// This is a harness gap for `on_tick`-driven, wall-clock demos, not a demo
    /// defect. TODO: pump the tick hook under `advance_clock` (and/or derive the
    /// time from the manual clock); then drop `#[ignore]`.
    #[ignore = "UNCLEAR: on_tick hook not pumped headless + clock reads wall-clock time"]
    #[test]
    fn inline01_clock_ticks_is_live() {
        run_test(ClockApp::default(), |pilot| {
            let before = pilot.app().frame_fingerprint();
            pilot.advance_clock(std::time::Duration::from_secs(2))?;
            assert_ne!(
                before,
                pilot.app().frame_fingerprint(),
                "the clock readout must update as time advances"
            );
            Ok(())
        })
        .expect("inline01 clock harness should run");
    }
}
