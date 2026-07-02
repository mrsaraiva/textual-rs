/// Port of Python Textual `docs/examples/guide/reactivity/world_clock03.py`.
///
/// Same three world clocks as world_clock02, but the child reactive is named
/// `clock_time` and the App binds to it with the **keyword** form of
/// `data_bind`, mapping the differently-named source field onto it:
///
/// ```python
/// def compose(self) -> ComposeResult:
///     yield WorldClock("Europe/London").data_bind(clock_time=WorldClockApp.time)  # (1)!
///     yield WorldClock("Europe/Paris").data_bind(clock_time=WorldClockApp.time)
///     yield WorldClock("Asia/Tokyo").data_bind(clock_time=WorldClockApp.time)
/// ```
///
/// So `App.time` (source field) propagates into each `WorldClock.clock_time`
/// (target field, a *different* name), firing `WorldClock::watch_clock_time`.
///
/// Rust port (faithful): the App and `WorldClock` derive `Reactive`. `on_mount`
/// registers `App::data_bind_reactive(App.time -> WorldClock.clock_time)`; the
/// runtime propagates each `App.time` change into every `WorldClock`'s
/// `clock_time` reactive and fires `WorldClock::watch_clock_time`, which formats
/// the local time and updates its `Digits`.
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
// WorldClock widget — own `clock_time` reactive driving its Digits
// ---------------------------------------------------------------------------

#[derive(Reactive)]
struct WorldClock {
    #[reactive(watch_with_app, init = false)]
    clock_time: u64,
    timezone: String,
    utc_offset_secs: i64,
    digits_id: String,
}

impl WorldClock {
    fn new(timezone: &'static str, utc_offset_secs: i64) -> Self {
        let n = NEXT_CLOCK.fetch_add(1, Ordering::Relaxed);
        Self {
            clock_time: now_secs(),
            timezone: timezone.to_string(),
            utc_offset_secs,
            digits_id: format!("wc-digits-{n}"),
        }
    }

    /// Python `watch_clock_time`: localize and update this clock's Digits.
    fn watch_clock_time(&mut self, app: &mut App, _old: &u64, new: &u64, _ctx: &mut ReactiveCtx) {
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
            ChildDecl::from(Digits::new(format_local(self.clock_time, self.utc_offset_secs)))
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
// App — `time` reactive data-bound to each WorldClock's `clock_time`
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

    fn on_mount_with_app(&mut self, app: &mut App, _ctx: &mut EventCtx) {
        // Python compose: `WorldClock(...).data_bind(clock_time=WorldClockApp.time)`.
        // The keyword form binds the source field `App.time` onto the differently
        // named target field `WorldClock.clock_time`: every `App.time` change
        // propagates into each child's `clock_time` and fires `watch_clock_time`.
        app.data_bind_reactive::<WorldClock, u64>(
            App::app_reactive_source(),
            "time",
            "WorldClock",
            |clock, value, ctx| {
                clock.set_clock_time(*value, ctx);
            },
        );

        // Python on_mount: self.update_time(); self.set_interval(1, self.update_time).
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
    fn local_time_hms_format() {
        let t = format_local(3661, 0); // 01:01:01 UTC
        assert_eq!(t, "01:01:01");
    }

    #[test]
    fn tokyo_offset_advances_hours() {
        assert_eq!(format_local(0, 9 * 3600), "09:00:00");
    }

    #[test]
    fn world_clock_digits_ids_unique() {
        let a = WorldClock::new("Europe/London", 0);
        let b = WorldClock::new("Europe/Paris", 3600);
        assert_ne!(a.digits_id, b.digits_id);
    }

    #[test]
    fn child_set_clock_time_records_change() {
        // Keyword data-bind contract: the binding sets each child's `clock_time`
        // reactive (differently named from the source `time`), recording a change
        // so `watch_clock_time` fires through the runtime reactive phase.
        let mut clock = WorldClock::new("Asia/Tokyo", 9 * 3600);
        let mut ctx = ReactiveCtx::new(textual::node_id::NodeId::default());
        clock.set_clock_time(*clock.clock_time() + 1, &mut ctx);
        assert!(ctx.has_changes(), "child clock_time set must record a change");
        assert_eq!(ctx.changes()[0].field_name, "clock_time");
    }

    /// LIVENESS PROBE — the 1s interval bumps `App.time`, which the keyword
    /// `data_bind` propagates into each WorldClock's differently-named
    /// `clock_time` reactive, firing `watch_clock_time` to update its Digits. We
    /// seed a sentinel app time, flush it (so the binding pushes the sentinel
    /// into each child), then advance the clock so a live tick re-reads the real
    /// wall clock. A dead demo (no timer / keyword binding not wired) leaves the
    /// frame identical and fails this gate.
    #[test]
    fn liveness_interval_tick_keyword_databinds_world_clocks() {
        textual::run_test(WorldClockApp::new(), |pilot| {
            assert!(pilot.clock_is_manual());
            pilot.app_mut().with_app_struct::<WorldClockApp, _>(
                |app_struct, app, _ctx| {
                    app_struct.set_time(0, app.reactive_ctx());
                },
                &mut EventCtx::default(),
            );
            pilot.press(&["space"])?;
            let before = pilot.app().frame_fingerprint();
            pilot.advance_clock(Duration::from_secs(1))?;
            let after = pilot.app().frame_fingerprint();
            assert_ne!(
                before, after,
                "a 1s interval tick must keyword-data-bind into the world clocks"
            );
            Ok(())
        })
        .unwrap();
    }
}
