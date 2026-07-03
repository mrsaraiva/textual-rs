/// Port of Python Textual `docs/examples/guide/reactivity/world_clock02.py`.
///
/// Same three world clocks as world_clock01, but the App fans its `time`
/// reactive out to each child via **`data_bind`** instead of an explicit
/// `watch_time` loop:
///
/// ```python
/// def compose(self) -> ComposeResult:
///     yield WorldClock("Europe/London").data_bind(WorldClockApp.time)  # (1)!
///     yield WorldClock("Europe/Paris").data_bind(WorldClockApp.time)
///     yield WorldClock("Asia/Tokyo").data_bind(WorldClockApp.time)
/// ```
///
/// `data_bind(WorldClockApp.time)` binds the App's `time` reactive to each
/// `WorldClock`'s same-named `time` reactive: whenever `App.time` changes, the
/// value propagates into each child and the child's `watch_time` fires.
///
/// Rust port (faithful): both the App and `WorldClock` derive `Reactive` with a
/// `time` reactive (seconds-since-epoch stands in for `datetime`). The App no
/// longer needs a `watch_time` — instead `on_mount` registers a field-to-field
/// binding via `App::data_bind_reactive(App.time -> WorldClock.time)`. The
/// runtime propagates each `App.time` change into every `WorldClock`, firing
/// `WorldClock::watch_time`, which formats the local time and updates its
/// `Digits`.
///
/// NOTE: interactive live clock — not promotable to a static snapshot. Timezone
/// conversion uses fixed UTC offsets (no pytz/DST), as `std` has no tz database.
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
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

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Format the local time (UTC seconds + offset) as HH:MM:SS.
fn format_local(utc_secs: u64, offset_secs: i64) -> String {
    let local = (utc_secs as i64 + offset_secs).rem_euclid(24 * 3600) as u64;
    let s = local % 60;
    let m = (local / 60) % 60;
    let h = (local / 3600) % 24;
    format!("{h:02}:{m:02}:{s:02}")
}

// Unique Digits id per WorldClock so each watcher targets its own display.
static NEXT_CLOCK: AtomicU64 = AtomicU64::new(0);

// ---------------------------------------------------------------------------
// WorldClock widget — own `time` reactive driving its Digits
// ---------------------------------------------------------------------------

#[derive(Reactive)]
struct WorldClock {
    #[reactive(watch_with_app, init = false)]
    time: u64,
    timezone: String,
    utc_offset_secs: i64,
    digits_id: String,
}

impl WorldClock {
    fn new(timezone: &'static str, utc_offset_secs: i64) -> Self {
        let n = NEXT_CLOCK.fetch_add(1, Ordering::Relaxed);
        Self {
            time: now_secs(),
            timezone: timezone.to_string(),
            utc_offset_secs,
            digits_id: format!("wc-digits-{n}"),
        }
    }

    /// Python `watch_time`: localize and update this clock's Digits.
    fn watch_time(&mut self, app: &mut App, _old: &u64, new: &u64, _ctx: &mut ReactiveCtx) {
        let text = format_local(*new, self.utc_offset_secs);
        let sel = format!("#{}", self.digits_id);
        let _ = app.with_query_one_mut_as::<Digits, _>(&sel, |digits| {
            digits.update(text);
        });
    }
}

impl Widget for WorldClock {
    fn style_type(&self) -> &'static str {
        "WorldClock"
    }

    fn focusable(&self) -> bool {
        false
    }

    fn compose(&mut self) -> ComposeResult {
        vec![
            ChildDecl::from(Label::new(self.timezone.clone())),
            ChildDecl::from(Digits::new(format_local(self.time, self.utc_offset_secs)))
                .with_id(&self.digits_id),
        ]
    }

    fn render(
        &self,
        _console: &rich_rs::Console,
        _options: &rich_rs::ConsoleOptions,
    ) -> rich_rs::Segments {
        rich_rs::Segments::new()
    }

    fn reactive_widget(&mut self) -> Option<&mut dyn ReactiveWidget> {
        Some(self)
    }
}

// ---------------------------------------------------------------------------
// App — `time` reactive data-bound to each WorldClock
// ---------------------------------------------------------------------------

#[derive(Reactive)]
struct WorldClockApp {
    #[reactive(init = false)]
    time: u64,
}

impl WorldClockApp {
    fn new() -> Self {
        Self { time: now_secs() }
    }

    /// Python `update_time`: `self.time = datetime.now()`.
    fn update_time(&mut self, ctx: &mut ReactiveCtx) {
        self.set_time(now_secs(), ctx);
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

    fn reactive_widget_mut(&mut self) -> Option<&mut dyn ReactiveWidget> {
        Some(self)
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(WorldClock::new("Europe/London", 0))
            .with_child(WorldClock::new("Europe/Paris", 3600))
            .with_child(WorldClock::new("Asia/Tokyo", 9 * 3600))
    }

    fn on_mount_with_app(&mut self, app: &mut App, _ctx: &mut textual::event::WidgetCtx) {
        // Python compose: `WorldClock(...).data_bind(WorldClockApp.time)`.
        // Register the field-to-field binding `App.time -> WorldClock.time`:
        // every change to the app's `time` reactive propagates into each
        // WorldClock's `time` reactive and fires `WorldClock::watch_time`.
        app.data_bind_reactive::<WorldClock, u64>(
            App::app_reactive_source(),
            "time",
            "WorldClock",
            |clock, value, ctx| {
                clock.set_time(*value, ctx);
            },
        );

        // Python on_mount: self.update_time(); self.set_interval(1, self.update_time).
        // Seed the initial time (propagates through the binding), then register
        // the repeating timer; each fire bumps `App.time`, fanning out via the
        // data binding.
        self.update_time(app.reactive_ctx());
        app.set_interval(
            Duration::from_secs(1),
            None,
            false,
            Box::new(|app, ctx| {
                app.with_app_struct::<WorldClockApp, _>(
                    |clock_app, app, _ctx| {
                        clock_app.update_time(app.reactive_ctx());
                    },
                    ctx,
                );
            }),
        );
    }
}

fn main() -> textual::Result<()> {
    run_sync(WorldClockApp::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_composes_three_clocks() {
        let mut app = WorldClockApp::new();
        let root = app.compose();
        assert_eq!(root.children().len(), 3);
    }

    #[test]
    fn world_clock_composes_label_and_digits() {
        let wc = WorldClock::new("Europe/London", 0);
        assert_eq!(wc.compose().len(), 2);
    }

    #[test]
    fn format_local_is_hms() {
        let t = format_local(3661, 0); // 01:01:01 UTC
        assert_eq!(t, "01:01:01");
    }

    #[test]
    fn tokyo_offset_advances_hours() {
        // 00:00:00 UTC + 9h = 09:00:00.
        assert_eq!(format_local(0, 9 * 3600), "09:00:00");
    }

    #[test]
    fn world_clock_digits_ids_unique() {
        let a = WorldClock::new("Europe/London", 0);
        let b = WorldClock::new("Europe/Paris", 3600);
        assert_ne!(a.digits_id, b.digits_id);
    }

    #[test]
    fn set_time_records_change() {
        let mut app = WorldClockApp::new();
        let mut ctx = ReactiveCtx::new(textual::node_id::NodeId::default());
        app.set_time(*app.time() + 1, &mut ctx);
        assert!(ctx.has_changes());
    }

    #[test]
    fn child_set_time_records_change() {
        // Deterministic data-bind contract: setting a WorldClock's `time`
        // reactive (what the binding does for each child) records a change so
        // its `watch_time` fires through the runtime reactive phase.
        let mut clock = WorldClock::new("Asia/Tokyo", 9 * 3600);
        let mut ctx = ReactiveCtx::new(textual::node_id::NodeId::default());
        clock.set_time(*clock.time() + 1, &mut ctx);
        assert!(ctx.has_changes(), "child time set must record a change");
        assert_eq!(ctx.changes()[0].field_name, "time");
    }

    /// LIVENESS PROBE — the 1s interval bumps `App.time`, which propagates to
    /// each WorldClock's `time` via the `data_bind` registered in on_mount,
    /// firing each child's `watch_time` to update its Digits. We seed a sentinel
    /// app time, flush it (so the binding pushes the sentinel into each child),
    /// then advance the clock so a live tick re-reads the real wall clock. A
    /// dead demo (no timer / binding not wired) leaves the frame identical.
    #[test]
    fn liveness_interval_tick_databinds_world_clocks() {
        textual::run_test(WorldClockApp::new(), |pilot| {
            assert!(pilot.clock_is_manual());
            pilot.app_mut().with_app_struct::<WorldClockApp, _>(
                |app_struct, app, _ctx| {
                    app_struct.set_time(0, app.reactive_ctx());
                },
                &mut textual::event::WidgetCtx::default(),
            );
            pilot.press(&["space"])?;
            let before = pilot.app().frame_fingerprint();
            pilot.advance_clock(Duration::from_secs(1))?;
            let after = pilot.app().frame_fingerprint();
            assert_ne!(
                before, after,
                "a 1s interval tick must data-bind into the world clocks"
            );
            Ok(())
        })
        .unwrap();
    }
}
