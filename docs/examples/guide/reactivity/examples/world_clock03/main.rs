/// Port of Python Textual `docs/examples/guide/reactivity/world_clock03.py`.
///
/// Displays three world-clock panels — Europe/London, Europe/Paris, Asia/Tokyo —
/// each showing a live HH:MM:SS clock in the `Digits` widget with a `Label`
/// above it showing the timezone name. All three clocks update together every
/// second.
///
/// Python uses:
///   - A custom `WorldClock` widget that composes `Label` + `Digits`,
///     driven by a `reactive[datetime] clock_time` field with a `watch_clock_time`
///     watcher that converts the UTC datetime to local time and calls
///     `Digits.update(...)`.
///   - The app holds a `reactive[datetime] time` field and uses `data_bind` to
///     push the same timestamp to all three `WorldClock` instances simultaneously.
///   - A 1-second `set_interval` timer drives `time` updates.
///
/// Rust differences:
///   - No reactive fields or `data_bind` exist in textual-rs yet.  The
///     equivalent is an `on_tick_with_app` hook that fires every frame; it
///     detects 1-second boundaries and queries each of the three `Digits`
///     nodes by ID to push the formatted local time.
///   - Timezone conversion (pytz) is done with hardcoded UTC offsets because
///     no timezone-database crate is available in the example dependencies.
///     London = UTC+0, Paris = UTC+1, Tokyo = UTC+9.  DST is not applied.
///
/// Framework gaps:
///   1. No reactive fields / watchers / `data_bind` mechanism.
///   2. No `set_interval` timer primitive; polling via `on_tick_with_app`.
///   3. No pytz / IANA timezone database; UTC offsets are hardcoded.
use std::time::{SystemTime, UNIX_EPOCH};
use textual::prelude::*;

// ---------------------------------------------------------------------------
// CSS — faithful port of world_clock01.tcss (shared by world_clock0[123].py)
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
// WorldClock widget
// ---------------------------------------------------------------------------

/// One clock panel: a `Label` (timezone name) stacked above a `Digits` display.
///
/// Mirrors the Python `WorldClock(Widget)` class that composes `Label` + `Digits`.
/// The `timezone_label` is the display name shown in the Label.
/// The `digits_id` is the unique CSS id of the inner `Digits` node; the app
/// uses it to push updated time strings each second.
struct WorldClock {
    inner: Vertical,
    /// CSS id of this clock's `Digits` child, stored so the app can query it.
    digits_id: String,
}

impl WorldClock {
    fn new(timezone_label: &str, digits_id: &str) -> Self {
        let inner = Vertical::new().with_compose(vec![
            ChildDecl::from(Label::new(timezone_label)),
            ChildDecl::from(Digits::new("00:00:00")).with_id(digits_id),
        ]);
        Self {
            inner,
            digits_id: digits_id.to_string(),
        }
    }
}

impl Widget for WorldClock {
    fn style_type(&self) -> &'static str {
        "WorldClock"
    }

    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        self.inner.take_composed_children()
    }

    fn take_child_decl_meta(&mut self) -> Vec<(usize, Option<String>, Vec<String>)> {
        self.inner.take_child_decl_meta()
    }

    fn take_child_handle_sinks(&mut self) -> Vec<(usize, HandleSink)> {
        self.inner.take_child_handle_sinks()
    }

    fn render(
        &self,
        console: &rich_rs::Console,
        options: &rich_rs::ConsoleOptions,
    ) -> rich_rs::Segments {
        self.inner.render(console, options)
    }

    fn focusable(&self) -> bool {
        false
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.inner.on_event(event, ctx);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.inner.on_event_capture(event, ctx);
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        self.inner.take_node_seed()
    }

    fn content_width(&self) -> Option<usize> {
        self.inner.content_width()
    }

    fn layout_height(&self) -> Option<usize> {
        self.inner.layout_height()
    }
}

// ---------------------------------------------------------------------------
// Timezone helpers (hardcoded UTC offsets — no DST)
// ---------------------------------------------------------------------------

/// Format the current local time for a fixed UTC offset (in whole hours).
fn local_time_hms(utc_offset_hours: i64) -> String {
    let utc_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let local_secs = utc_secs + utc_offset_hours * 3600;
    let s = local_secs.rem_euclid(60);
    let m = (local_secs / 60).rem_euclid(60);
    let h = (local_secs / 3600).rem_euclid(24);
    format!("{h:02}:{m:02}:{s:02}")
}

/// Three timezone clocks mirroring the Python example.
const CLOCKS: &[(&str, &str, i64)] = &[
    ("Europe/London", "london-digits", 0),
    ("Europe/Paris", "paris-digits", 1),
    ("Asia/Tokyo", "tokyo-digits", 9),
];

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

#[derive(Default)]
struct WorldClockApp {
    last_second: i64,
}

impl TextualApp for WorldClockApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        let mut root = AppRoot::new();
        for (label, digits_id, _offset) in CLOCKS {
            root = root.with_child(WorldClock::new(label, digits_id));
        }
        root
    }

    fn on_tick_with_app(&mut self, app: &mut App, _tick: u64, ctx: &mut EventCtx) {
        let utc_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        if utc_secs == self.last_second {
            return;
        }
        self.last_second = utc_secs;

        for (_label, digits_id, offset) in CLOCKS {
            let time = local_time_hms(*offset);
            let selector = format!("#{digits_id}");
            let _ = app.with_query_one_mut_as::<Digits, _>(&selector, |digits| {
                digits.update(time);
            });
        }
        ctx.request_repaint();
    }
}

fn main() -> textual::Result<()> {
    run_sync(WorldClockApp::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_clock_app_composes_without_panic() {
        let mut app = WorldClockApp::default();
        let _root = app.compose();
    }

    #[test]
    fn world_clock_widget_composes_without_panic() {
        let _wc = WorldClock::new("Europe/London", "london-digits");
    }

    #[test]
    fn local_time_hms_formats_correctly() {
        // UTC offset 0 should match UTC time
        let t = local_time_hms(0);
        assert_eq!(t.len(), 8, "expected HH:MM:SS");
        assert_eq!(&t[2..3], ":");
        assert_eq!(&t[5..6], ":");
    }
}
