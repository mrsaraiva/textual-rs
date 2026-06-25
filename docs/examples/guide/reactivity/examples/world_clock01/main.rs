/// Port of Python Textual `docs/examples/guide/reactivity/world_clock01.py`.
///
/// Three world clocks (London, Paris, Tokyo), each showing its local time via a
/// `Digits` widget. Demonstrates TWO-LEVEL reactivity:
///   - the App has a `time` reactive; its `watch_time` pushes the timestamp to
///     each `WorldClock` widget's own `time` reactive;
///   - each `WorldClock` has a `time` reactive whose `watch_time` formats the
///     local time and updates its `Digits`.
///
/// Python:
///   class WorldClock(Widget):
///       time: reactive[datetime] = reactive(datetime.now)
///       def watch_time(self, time): self.query_one(Digits).update(localized)
///   class WorldClockApp(App):
///       time: reactive[datetime] = reactive(datetime.now)
///       def watch_time(self, time):
///           for world_clock in self.query(WorldClock): world_clock.time = time  # (1)
///       on_mount: self.update_time(); self.set_interval(1, self.update_time)
///
/// Rust port (faithful): both the app and `WorldClock` derive `Reactive` with a
/// `#[reactive(watch_with_app)] time` (seconds-since-epoch stands in for
/// `datetime`). The app's `watch_time` iterates `WorldClock` nodes and sets each
/// one's reactive (enqueuing a widget-level reactive entry). Each `WorldClock`'s
/// `watch_time` updates its own `Digits` (addressed by a unique id).
///
/// NOTE: interactive live clock — not promotable to a static snapshot. Timezone
/// conversion uses fixed UTC offsets (no pytz/DST), as `std` has no tz database.
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use textual::reactive::{RuntimeReactiveEntry, enqueue_runtime_reactive_entry};
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

    fn compose(&self) -> ComposeResult {
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
// App — `time` reactive fanned out to each WorldClock
// ---------------------------------------------------------------------------

#[derive(Reactive)]
struct WorldClockApp {
    #[reactive(watch_with_app, init = false)]
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

    /// Python `watch_time`: push the timestamp to each WorldClock's reactive.
    fn watch_time(&mut self, app: &mut App, _old: &u64, new: &u64, _ctx: &mut ReactiveCtx) {
        let time = *new;
        let clock_ids = app.query("WorldClock").map(|q| q.into_ids()).unwrap_or_default();
        for node_id in clock_ids {
            let mut rctx = ReactiveCtx::new(node_id);
            app.with_widget_mut_as::<WorldClock, _>(node_id, |clock| {
                clock.set_time(time, &mut rctx);
            });
            if rctx.has_changes() {
                enqueue_runtime_reactive_entry(RuntimeReactiveEntry::new(node_id, rctx));
            }
        }
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

    fn on_mount_with_app(&mut self, app: &mut App, _ctx: &mut EventCtx) {
        // Python on_mount: self.update_time(); self.set_interval(1, self.update_time).
        // Seed the initial time, then register the repeating timer; each fire
        // re-enters the app struct and bumps the `time` reactive, whose watcher
        // fans the timestamp out to each WorldClock child.
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

    /// LIVENESS PROBE — the 1s interval must fire and the App's `watch_time`
    /// must fan the timestamp out to every WorldClock child, whose own
    /// `watch_time` updates its Digits. We seed a sentinel app time, flush it
    /// (so each child's Digits read the sentinel), then advance the clock so a
    /// live tick re-reads the real wall clock. A dead demo (no timer / fan-out
    /// or child watch unwired) leaves the frame identical and fails this gate.
    #[test]
    fn liveness_interval_tick_updates_world_clocks() {
        textual::run_test(WorldClockApp::new(), |pilot| {
            assert!(pilot.clock_is_manual());
            pilot.app_mut().with_app_struct::<WorldClockApp, _>(
                |app_struct, app, _ctx| {
                    app_struct.set_time(0, app.reactive_ctx());
                },
                &mut EventCtx::default(),
            );
            // Flush the seed (and its fan-out) through the app-reactive bridge.
            pilot.press(&["space"])?;
            let before = pilot.app().frame_fingerprint();
            pilot.advance_clock(Duration::from_secs(1))?;
            let after = pilot.app().frame_fingerprint();
            assert_ne!(
                before, after,
                "a 1s interval tick must update the world clocks"
            );
            Ok(())
        })
        .unwrap();
    }
}
