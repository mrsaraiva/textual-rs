/// Port of Python Textual `docs/examples/guide/reactivity/world_clock01.py`.
///
/// Demonstrates reactive time display across multiple world clocks, each
/// showing the current time for a different timezone.
///
/// Python structure:
///   - `WorldClock(Widget)` — custom widget with `Label(timezone)` + `Digits()`
///   - `WorldClockApp` — composes three `WorldClock` instances for London, Paris, Tokyo
///   - A 1-second interval (`set_interval`) drives `update_time` → `watch_time`
///     which pushes the current `datetime` to all `WorldClock` instances.
///
/// Rust differences:
///   - `on_tick_with_app` replaces `set_interval`; frame-rate driven but we
///     only update the display on elapsed-second boundaries to match 1s interval.
///   - No timezone crate in the examples workspace, so UTC offsets are hardcoded
///     for the three cities (London UTC+0, Paris UTC+1, Tokyo UTC+9).
///   - Post-mount Digits are reached via `app.with_widget_mut_as::<Digits>` on
///     the matched `"WorldClock Digits"` node ids, paired by tree order with the
///     computed time strings from each `WorldClock` widget.
///
/// FRAMEWORK GAP: Python uses `pytz`/`dateutil` for real IANA timezone
/// conversions.  No equivalent is available in `std` or the examples Cargo.toml.
/// This port uses fixed UTC offsets; DST transitions are not handled.
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use textual::prelude::*;

// ---------------------------------------------------------------------------
// CSS — mirrors world_clock01.tcss
// ---------------------------------------------------------------------------

const CSS: &str = r#"
Screen {
    align: center middle;
}

WorldClock {
    width: auto;
    height: auto;
    padding: 1 2;
    background: $panel;
    border: wide $background;
}

WorldClock Digits {
    width: auto;
    color: $secondary;
}
"#;

// ---------------------------------------------------------------------------
// WorldClock widget — mirrors Python's `class WorldClock(Widget)`
// ---------------------------------------------------------------------------

struct WorldClock {
    /// UTC offset in seconds (positive = east of UTC).
    utc_offset_secs: i64,
    /// Inner vertical layout container (Label + Digits).
    inner: Vertical,
}

impl WorldClock {
    fn new(timezone: &'static str, utc_offset_secs: i64) -> Self {
        let inner = Vertical::new()
            .with_child(Label::new(timezone))
            .with_child(Digits::new("00:00:00"));
        Self {
            utc_offset_secs,
            inner,
        }
    }

    /// Compute the current local time string for this clock's UTC offset.
    fn current_time_str(&self) -> String {
        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs() as i64;
        let local_secs = now_secs + self.utc_offset_secs;
        let hour = ((local_secs / 3600) % 24 + 24) as u64 % 24;
        let min = ((local_secs / 60) % 60 + 60) as u64 % 60;
        let sec = ((local_secs % 60) + 60) as u64 % 60;
        format!("{:02}:{:02}:{:02}", hour, min, sec)
    }
}

impl Widget for WorldClock {
    fn style_type(&self) -> &'static str {
        "WorldClock"
    }

    fn focusable(&self) -> bool {
        false
    }

    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        self.inner.take_composed_children()
    }

    fn render(
        &self,
        console: &rich_rs::Console,
        options: &rich_rs::ConsoleOptions,
    ) -> rich_rs::Segments {
        self.inner.render(console, options)
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.inner.on_layout(width, height);
    }

    fn on_tick(&mut self, tick: u64) {
        self.inner.on_tick(tick);
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.inner.on_event(event, ctx);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.inner.on_event_capture(event, ctx);
    }
}

// ---------------------------------------------------------------------------
// App — mirrors Python's `WorldClockApp`
// ---------------------------------------------------------------------------

struct WorldClockApp {
    /// Track last second so we only update once per second.
    last_second: u64,
}

impl WorldClockApp {
    fn new() -> Self {
        Self { last_second: 0 }
    }
}

impl TextualApp for WorldClockApp {
    fn title(&self) -> &'static str {
        "WorldClockApp"
    }

    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            // London: UTC+0 (approximation; no DST)
            .with_child(WorldClock::new("Europe/London", 0))
            // Paris: UTC+1 (approximation; no DST)
            .with_child(WorldClock::new("Europe/Paris", 3600))
            // Tokyo: UTC+9 (no DST)
            .with_child(WorldClock::new("Asia/Tokyo", 9 * 3600))
    }

    fn on_tick_with_app(&mut self, app: &mut App, _tick: u64, ctx: &mut EventCtx) {
        // Only update once per second (mirrors Python's `set_interval(1, ...)`)
        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs();
        if now_secs != self.last_second {
            self.last_second = now_secs;
            self.update_all_clocks(app, ctx);
        }
    }
}

impl WorldClockApp {
    /// Query all WorldClock widgets, compute their local times, then push the
    /// time strings into the corresponding Digits children.
    ///
    /// Python equivalent: `watch_time` sets `world_clock.time` on each clock,
    /// which triggers `watch_time(self, time)` on the widget → `Digits.update`.
    /// In Rust, post-mount children live in the arena tree and are reached via
    /// typed selector queries.
    fn update_all_clocks(&mut self, app: &mut App, ctx: &mut EventCtx) {
        // Step 1: collect computed time strings in tree order from WorldClock widgets.
        let clock_ids = app
            .query("WorldClock")
            .map(|q| q.into_ids())
            .unwrap_or_default();

        let mut time_strings: Vec<String> = Vec::new();
        for node_id in &clock_ids {
            if let Some(time_str) =
                app.with_widget_mut_as::<WorldClock, _>(*node_id, |clock| clock.current_time_str())
            {
                time_strings.push(time_str);
            }
        }

        // Step 2: collect Digits node ids in tree order (one per WorldClock).
        let digits_ids = app
            .query("WorldClock Digits")
            .map(|q| q.into_ids())
            .unwrap_or_default();

        // Step 3: pair and update.
        let mut repainted = false;
        for (digits_id, time_str) in digits_ids.into_iter().zip(time_strings.into_iter()) {
            let _ = app.with_widget_mut_as::<Digits, _>(digits_id, |digits| {
                digits.update(time_str);
            });
            repainted = true;
        }

        if repainted {
            ctx.request_repaint();
        }
    }
}

fn main() -> textual::Result<()> {
    run_sync(WorldClockApp::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_clock_app_composes_without_panic() {
        let mut app = WorldClockApp::new();
        let _root = app.compose();
    }

    #[test]
    fn world_clock_time_str_format() {
        let clock = WorldClock::new("Asia/Tokyo", 9 * 3600);
        let val = clock.current_time_str();
        // Must be HH:MM:SS
        assert_eq!(val.len(), 8);
        assert_eq!(&val[2..3], ":");
        assert_eq!(&val[5..6], ":");
    }

    #[test]
    fn world_clock_constructs_for_all_cities() {
        let _london = WorldClock::new("Europe/London", 0);
        let _paris = WorldClock::new("Europe/Paris", 3600);
        let _tokyo = WorldClock::new("Asia/Tokyo", 9 * 3600);
    }
}
