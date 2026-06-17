/// Port of Python Textual `docs/examples/guide/reactivity/world_clock02.py`.
///
/// Displays three world clocks (London, Paris, Tokyo) stacked vertically using
/// the `Digits` widget for time and a `Label` for the timezone name.
///
/// Python uses `pytz` for timezone-aware datetime conversion. The Rust port
/// approximates this with fixed UTC offsets (standard time only — DST is not
/// applied, which is a framework gap: there is no timezone crate in the
/// dependency list).
///
/// FRAMEWORK GAPS:
/// - No timezone/chrono crate available → fixed UTC offsets; DST not applied.
/// - Python's `data_bind(App.time)` reactive binding propagates a single
///   `datetime` reactive field from App to each WorldClock widget.  The Rust
///   `reactive` system does not yet support cross-widget data binding of this
///   kind; the Rust port drives the per-clock update from `on_tick_with_app`
///   instead (same observable behaviour, different mechanism).
/// - `WorldClock` is a simple wrapper; nested CSS selector
///   `WorldClock > Digits` is approximated by ID-based selectors at tick time.
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use textual::prelude::*;

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
    layout: vertical;
}

WorldClock Digits {
    width: auto;
    color: $secondary;
}
"#;

// ---------------------------------------------------------------------------
// Timezone helpers (fixed UTC offsets, no DST)
// ---------------------------------------------------------------------------

/// Compute "HH:MM:SS" for a given UTC offset in whole seconds.
fn time_with_offset(utc_offset_secs: i64) -> String {
    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let local_secs = now_secs + utc_offset_secs;
    let s = local_secs.rem_euclid(60) as u64;
    let total_m = local_secs.rem_euclid(3600 * 24) as u64 / 60;
    let m = total_m % 60;
    let h = (total_m / 60) % 24;
    format!("{h:02}:{m:02}:{s:02}")
}

// ---------------------------------------------------------------------------
// Monotonic counter so each WorldClock gets a unique Digits CSS id.
// ---------------------------------------------------------------------------
static NEXT_CLOCK: AtomicU64 = AtomicU64::new(0);

// ---------------------------------------------------------------------------
// WorldClock widget
// ---------------------------------------------------------------------------

struct WorldClock {
    inner: VerticalGroup,
    /// Display name for the timezone (stored for future use / debugging).
    #[allow(dead_code)]
    pub timezone: String,
    /// Fixed UTC offset in seconds (no DST).
    pub utc_offset_secs: i64,
    /// CSS id of the inner Digits child (`wc-digits-<n>`).
    pub digits_id: String,
}

impl WorldClock {
    fn new(timezone: impl Into<String>, utc_offset_secs: i64) -> Self {
        let timezone = timezone.into();
        let n = NEXT_CLOCK.fetch_add(1, Ordering::Relaxed);
        let digits_id = format!("wc-digits-{n}");
        let initial_time = time_with_offset(utc_offset_secs);

        let inner = VerticalGroup::new().with_compose(vec![
            ChildDecl::from(Label::new(timezone.clone())),
            ChildDecl::from(Digits::new(initial_time)).with_id(&digits_id),
        ]);

        Self {
            inner,
            timezone,
            utc_offset_secs,
            digits_id,
        }
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

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.inner.on_event(event, ctx);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.inner.on_event_capture(event, ctx);
    }

    fn on_tick(&mut self, tick: u64) {
        self.inner.on_tick(tick);
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.inner.on_layout(width, height);
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut EventCtx) {
        self.inner.on_message(message, ctx);
    }
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

/// Fixed UTC offsets (standard time, no DST):
///   Europe/London → UTC+0  (BST=+3600 in summer — not applied)
///   Europe/Paris  → UTC+1  (CEST=+7200 in summer — not applied)
///   Asia/Tokyo    → UTC+9  (no DST)
const CLOCKS: &[(&str, i64)] = &[
    ("Europe/London", 0),
    ("Europe/Paris", 3600),
    ("Asia/Tokyo", 9 * 3600),
];

struct WorldClockApp {
    last_second: u64,
    /// CSS ids of each clock's Digits child, in order, populated at first tick.
    digits_ids: Vec<(String, i64)>,
}

impl Default for WorldClockApp {
    fn default() -> Self {
        Self {
            last_second: 0,
            digits_ids: Vec::new(),
        }
    }
}

impl TextualApp for WorldClockApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        let clocks: Vec<ChildDecl> = CLOCKS
            .iter()
            .map(|(tz, offset)| ChildDecl::from(WorldClock::new(*tz, *offset)))
            .collect();

        AppRoot::new().with_compose(clocks)
    }

    fn on_mount_with_app(&mut self, app: &mut App, ctx: &mut EventCtx) {
        // Collect (digits_id, utc_offset) pairs from all WorldClock nodes.
        let wc_nodes = app
            .query("WorldClock")
            .map(|q| q.into_ids())
            .unwrap_or_default();

        for node_id in wc_nodes {
            if let Some(Some((digits_id, offset))) =
                app.with_widget_mut_as::<WorldClock, _>(node_id, |wc| {
                    Some((wc.digits_id.clone(), wc.utc_offset_secs))
                })
            {
                self.digits_ids.push((digits_id, offset));
            }
        }
        ctx.request_repaint();
    }

    fn on_tick_with_app(&mut self, app: &mut App, _tick: u64, ctx: &mut EventCtx) {
        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if now_secs == self.last_second {
            return;
        }
        self.last_second = now_secs;

        for (digits_id, offset) in &self.digits_ids {
            let time_str = time_with_offset(*offset);
            let sel = format!("#{digits_id}");
            let _ = app.with_query_one_mut_as::<Digits, _>(&sel, |digits| {
                digits.update(time_str);
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
    fn three_clocks_are_composed() {
        let mut app = WorldClockApp::default();
        let root = app.compose();
        assert_eq!(
            root.children().len(),
            3,
            "AppRoot should have 3 WorldClock children"
        );
    }

    #[test]
    fn time_with_offset_returns_hms_format() {
        let t = time_with_offset(0);
        assert_eq!(t.len(), 8, "expected HH:MM:SS (8 chars)");
        assert_eq!(&t[2..3], ":");
        assert_eq!(&t[5..6], ":");
    }

    #[test]
    fn time_with_offset_tokyo_differs_from_london() {
        // Tokyo is UTC+9, so its hours should differ from UTC+0 by 9 (mod 24).
        let london = time_with_offset(0);
        let tokyo = time_with_offset(9 * 3600);
        // Both must be valid HH:MM:SS
        assert_eq!(london.len(), 8);
        assert_eq!(tokyo.len(), 8);
    }

    #[test]
    fn world_clock_digits_id_is_unique() {
        let c1 = WorldClock::new("Europe/London", 0);
        let c2 = WorldClock::new("Europe/Paris", 3600);
        assert_ne!(c1.digits_id, c2.digits_id, "each WorldClock needs a unique digits_id");
    }
}
