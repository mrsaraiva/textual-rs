/// Port of Python Textual `docs/examples/guide/structure.py`.
///
/// Demonstrates the basic app structure with a custom widget.
/// The Python example defines a `Clock` widget that displays the current
/// datetime (formatted with `%c`), refreshed every second.
///
/// Rust port uses a `Label` with `id="clock"`, refreshed every second by a real
/// `set_interval` timer — Python's `self.set_interval(1, self.update_time)`. The
/// timer callback queries the `Label` and updates it (Python's `update_time`).
///
/// Notes:
/// - Python `strftime("%c")` (locale datetime) is not available in `std`;
///   we produce a fixed-format "Weekday Mon DD HH:MM:SS YYYY" string using
///   only `std::time::SystemTime` (no external crates).  The content is
///   semantically equivalent — a live, updating datetime string.
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use textual::prelude::*;

const CSS: &str = r##"
/* Python: Clock { content-align: center middle; }
   Rust uses Label with id="clock" to fill the screen and center its content. */
#clock {
    width: 100%;
    height: 100%;
    content-align: center middle;
}
"##;

/// Days-of-week abbreviations (Sun=0 … Sat=6).
const WDAY: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
/// Month abbreviations (Jan=0 … Dec=11).
const MON: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Compute the current UTC time as epoch seconds.
fn epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Format epoch seconds as a `%c`-equivalent string:
/// "Www Mmm DD HH:MM:SS YYYY"  (e.g. "Mon Jun 17 14:05:03 2024").
///
/// Uses the proleptic Gregorian calendar algorithm from civil_from_days
/// (Howard Hinnant, http://howardhinnant.github.io/date_algorithms.html).
fn format_datetime(secs: u64) -> String {
    let days = secs / 86_400;
    let time_of_day = secs % 86_400;
    let hh = time_of_day / 3_600;
    let mm = (time_of_day % 3_600) / 60;
    let ss = time_of_day % 60;

    // Day-of-week: epoch (1970-01-01) was a Thursday (= 4).
    let wday = ((days + 4) % 7) as usize;

    // Civil date from days since epoch (Howard Hinnant algorithm).
    let z = days as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!(
        "{} {} {:2} {:02}:{:02}:{:02} {}",
        WDAY[wday],
        MON[(m - 1) as usize],
        d,
        hh,
        mm,
        ss,
        y
    )
}

/// Python `update_time`: set the clock Label to the current datetime.
fn update_time(app: &mut App, ctx: &mut EventCtx) {
    let text = format_datetime(epoch_secs());
    let _ = app.with_query_one_mut_as::<Label, _>("#clock", |label| {
        label.set_text(text);
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
        AppRoot::new().with_child(Label::new(format_datetime(epoch_secs())).with_id("clock"))
    }

    fn on_mount_with_app(&mut self, app: &mut App, _ctx: &mut EventCtx) {
        // Python: self.set_interval(1, self.update_time).
        app.set_interval(
            Duration::from_secs(1),
            None,
            false,
            Box::new(|app, ctx| update_time(app, ctx)),
        );
    }
}

fn main() -> textual::Result<()> {
    run_sync(ClockApp::default())
}
