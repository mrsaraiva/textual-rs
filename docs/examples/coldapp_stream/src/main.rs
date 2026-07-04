//! Live request-log viewer — a COLD-USER app pressure-testing the textual-rs 1.0
//! SCROLL + DYNAMIC-MOUNT surface: a `VerticalScroll` whose children are mounted
//! at runtime (one per streamed line), auto-scrolled to the bottom, with an
//! app-owned interval feeding new lines and a pause/clear control. Authored
//! against only `std` + `textual::prelude::*` (+ `rich-rs`).
//!
//! What it exercises that the other cold apps did not:
//! * runtime child mounting into a live container (`app.mount_under("#log", …)`),
//!   the canonical growing-list pattern (same idiom as the stopwatch tutorial),
//! * removing mounted nodes (`app.remove_node`) to clear the log,
//! * scroll control (`VerticalScroll::scroll_end`) to follow the tail,
//! * an app-owned `set_interval` producing structural mutations each fire,
//!   deterministic under `Pilot::advance_clock`.
//!
//! Design note: the feed timer MUST be app-owned, not widget-owned. `mount_under`
//! / `remove_node` live on `App`, but a widget-owned `WidgetCtx::set_interval`
//! callback only gets a `WidgetCtx` (no `App`), so it cannot mount siblings. A
//! growing, dynamically-mounted list is therefore driven from the app timer with
//! shared state in an `Arc<Mutex<…>>` (the same shape Kanban used for its modal
//! result queue).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use textual::prelude::*;

/// One feed tick.
const TICK: Duration = Duration::from_millis(250);

const CSS: &str = r#"
Screen { layout: vertical; }

#header {
    height: 1;
    background: $primary;
    color: $text;
    text-style: bold;
    padding: 0 1;
}

#log {
    height: 1fr;
    border: round $panel;
    padding: 0 1;
}
#log .line { width: 100%; }
#log .ok { color: $success; }
#log .warn { color: $warning; }
"#;

/// Shared feed state — mutated from the app interval closure (which only gets
/// `&mut App`, not `&mut StreamApp`), so it lives behind an `Arc<Mutex<…>>`.
struct Feed {
    count: u64,
    paused: bool,
    /// NodeIds of the mounted line widgets, so `clear` can remove them.
    lines: Vec<NodeId>,
}

impl Feed {
    fn new() -> Self {
        Self { count: 0, paused: false, lines: Vec::new() }
    }
}

struct StreamApp {
    feed: Arc<Mutex<Feed>>,
}

impl StreamApp {
    fn new() -> Self {
        Self { feed: Arc::new(Mutex::new(Feed::new())) }
    }
}

/// Build one synthetic request-log line from the counter (deterministic — no
/// clock, no randomness — so the demo and its tests are reproducible).
fn log_line(n: u64) -> (String, &'static str) {
    const PATHS: [&str; 4] = ["/api/users", "/api/orders", "/health", "/api/search"];
    let path = PATHS[(n as usize) % PATHS.len()];
    let ms = 3 + (n * 7) % 40;
    // Every 6th request is a slow 503 to exercise the warn color.
    if n % 6 == 5 {
        (format!("[req {n:04}] GET {path} -> 503 Unavailable ({ms}ms)"), "warn")
    } else {
        (format!("[req {n:04}] GET {path} -> 200 OK ({ms}ms)"), "ok")
    }
}

impl StreamApp {
    fn header_text(count: u64, paused: bool) -> String {
        format!(
            "Request stream — {count} lines {}   (space: pause · c: clear)",
            if paused { "· PAUSED" } else { "· live" }
        )
    }
}

impl TextualApp for StreamApp {
    fn configure(&mut self, app: &mut App) -> Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn bindings(&self) -> Vec<BindingDecl> {
        vec![
            BindingDecl::new("space", "pause", "Pause/resume"),
            BindingDecl::new("c", "clear", "Clear"),
        ]
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_compose(vec![
            ChildDecl::new(Box::new(Label::new(Self::header_text(0, false)))).with_id("header"),
            ChildDecl::new(Box::new(VerticalScroll::new())).with_id("log"),
            ChildDecl::from(Footer::new()),
        ])
    }

    /// Start the app-owned feed. Each fire (while not paused) mounts a new line
    /// under `#log`, follows the tail, and refreshes the header counter.
    fn on_mount_with_app(&mut self, app: &mut App, _ctx: &mut WidgetCtx) {
        let feed = self.feed.clone();
        app.set_interval(
            TICK,
            None,
            false,
            Box::new(move |app, _ctx| {
                let mut f = feed.lock().unwrap_or_else(|e| e.into_inner());
                if f.paused {
                    return;
                }
                f.count += 1;
                let (text, kind) = log_line(f.count);
                if let Ok(nid) =
                    app.mount_under("#log", Label::new(text).class("line").class(kind))
                {
                    f.lines.push(nid);
                }
                let count = f.count;
                drop(f);
                // Follow the tail: scrolling to `count` rows always lands at the
                // bottom (VerticalScroll clamps the offset; it has no scroll_end).
                let _ = app
                    .with_query_one_mut_as::<VerticalScroll, _>("#log", |s| s.scroll_to(count as usize));
                let _ = app.with_query_one_mut_as::<Label, _>("#header", |l| {
                    l.set_text(StreamApp::header_text(count, false));
                });
            }),
        );
    }

    fn on_app_action_str(&mut self, app: &mut App, action: &str, ctx: &mut WidgetCtx) {
        let mut f = self.feed.lock().unwrap_or_else(|e| e.into_inner());
        match action {
            "pause" => {
                f.paused = !f.paused;
                let (count, paused) = (f.count, f.paused);
                drop(f);
                let _ = app.with_query_one_mut_as::<Label, _>("#header", |l| {
                    l.set_text(StreamApp::header_text(count, paused));
                });
            }
            "clear" => {
                for nid in f.lines.drain(..) {
                    let _ = app.remove_node(nid);
                }
                f.count = 0;
                drop(f);
                let _ = app.with_query_one_mut_as::<Label, _>("#header", |l| {
                    l.set_text(StreamApp::header_text(0, false));
                });
            }
            _ => return,
        }
        ctx.set_handled();
    }
}

fn main() -> Result<()> {
    run_sync(StreamApp::new())
}

// ===========================================================================
// Tests — deterministic via Pilot's manual clock
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Number of line widgets currently mounted under `#log`.
    fn line_count(pilot: &mut Pilot) -> usize {
        pilot
            .app()
            .query("#log Label")
            .map(|q| q.into_ids().len())
            .unwrap_or(0)
    }

    #[test]
    fn app_composes_without_panic() {
        let mut app = StreamApp::new();
        let _root = app.compose();
    }

    #[test]
    fn log_line_is_deterministic() {
        let (a, ka) = log_line(1);
        let (b, kb) = log_line(1);
        assert_eq!(a, b);
        assert_eq!(ka, kb);
        assert_eq!(log_line(5).1, "warn", "every 6th request is a 503 warn");
        assert_eq!(log_line(1).1, "ok");
    }

    /// The app-owned feed mounts a new line per tick: advancing 1s at 250ms/tick
    /// mounts ~4 lines under `#log` (dynamic mount into a live scroll container).
    #[test]
    fn feed_mounts_lines_over_time() {
        let app = StreamApp::new();
        let feed = app.feed.clone();
        run_test(app, |pilot| {
            assert!(pilot.clock_is_manual());
            assert_eq!(line_count(pilot), 0, "log starts empty");
            pilot.advance_clock(Duration::from_secs(1))?;
            let n = line_count(pilot);
            assert!(n >= 3, "≈4 lines should mount over 1s at 250ms/tick, got {n}");
            assert_eq!(
                feed.lock().unwrap().lines.len(),
                n,
                "tracked line ids must match mounted widgets"
            );
            Ok(())
        })
        .unwrap();
    }

    /// Pausing halts the feed: no new lines mount while paused.
    #[test]
    fn pause_halts_the_feed() {
        run_test(StreamApp::new(), |pilot| {
            pilot.advance_clock(Duration::from_millis(750))?;
            let before = line_count(pilot);
            assert!(before >= 2, "some lines before pause, got {before}");
            pilot.press(&["space"])?; // pause
            pilot.advance_clock(Duration::from_secs(2))?;
            assert_eq!(line_count(pilot), before, "a paused feed must not mount new lines");
            Ok(())
        })
        .unwrap();
    }

    /// Clearing removes every mounted line and resets the counter.
    #[test]
    fn clear_removes_all_lines() {
        let app = StreamApp::new();
        let feed = app.feed.clone();
        run_test(app, |pilot| {
            pilot.advance_clock(Duration::from_secs(1))?;
            assert!(line_count(pilot) > 0);
            pilot.press(&["c"])?; // clear
            assert_eq!(line_count(pilot), 0, "clear must remove every mounted line");
            assert_eq!(feed.lock().unwrap().count, 0, "clear must reset the counter");
            assert!(feed.lock().unwrap().lines.is_empty(), "clear must drop tracked ids");
            Ok(())
        })
        .unwrap();
    }
}
