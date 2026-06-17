/// Port of Python Textual `docs/examples/tutorial/stopwatch.py`.
///
/// The final tutorial stopwatch app: three stopwatches with Start/Stop/Reset
/// buttons, a live time display, and bindings to add/remove stopwatches
/// dynamically (a = add, r = remove, d = toggle dark mode).
///
/// Python structure:
///   - TimeDisplay(Digits) — live clock display, updated via a 1/60s interval
///   - Stopwatch(HorizontalGroup) — one row: Start, Stop, Reset buttons + TimeDisplay
///   - StopwatchApp(App) — Header, Footer, VerticalScroll(#timers) with three Stopwatches
///
/// Rust differences:
///   - No reactive interval; clock is driven via `on_tick_with_app` (called every
///     frame) instead of Textual's `set_interval`.
///   - Dynamic add/remove uses `app.mount_under` / `app.remove_node`.
///   - `TimeDisplay` wraps `Digits` and delegates rendering; the tick loop calls
///     `Digits::update` to push the formatted time string.
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use textual::prelude::*;

// ---------------------------------------------------------------------------
// CSS (mirrors stopwatch.tcss exactly)
// ---------------------------------------------------------------------------

const CSS: &str = r#"
Stopwatch {
    layout: horizontal;
    background: $boost;
    height: 5;
    min-width: 50;
    margin: 1;
    padding: 1;
}

TimeDisplay {
    text-align: center;
    color: $foreground-muted;
    height: 3;
}

Button {
    width: 16;
}

#start {
    dock: left;
}

#stop {
    dock: left;
    display: none;
}

#reset {
    dock: right;
}

.started {
    background: $success-muted;
    color: $text;
}

.started TimeDisplay {
    color: $foreground;
}

.started #start {
    display: none;
}

.started #stop {
    display: block;
}

.started #reset {
    visibility: hidden;
}
"#;

// ---------------------------------------------------------------------------
// Monotonic id source — each Stopwatch (initial or dynamically added) gets a
// unique TimeDisplay id so the tick loop can pair them.
// ---------------------------------------------------------------------------

static NEXT_SW: AtomicU64 = AtomicU64::new(0);

// ---------------------------------------------------------------------------
// TimeDisplay widget
// ---------------------------------------------------------------------------

struct TimeDisplay {
    inner: Digits,
}

impl TimeDisplay {
    fn new(text: &str) -> Self {
        Self {
            inner: Digits::new(text),
        }
    }
}

impl Widget for TimeDisplay {
    fn style_type(&self) -> &'static str {
        "TimeDisplay"
    }

    fn focusable(&self) -> bool {
        false
    }

    fn render(
        &self,
        console: &rich_rs::Console,
        options: &rich_rs::ConsoleOptions,
    ) -> rich_rs::Segments {
        self.inner.render_styled(console, options)
    }

    fn content_width(&self) -> Option<usize> {
        self.inner.content_width()
    }

    fn layout_height(&self) -> Option<usize> {
        self.inner.layout_height()
    }
}

// ---------------------------------------------------------------------------
// Stopwatch widget
// ---------------------------------------------------------------------------

struct Stopwatch {
    inner: HorizontalGroup,
    /// CSS id of this stopwatch's TimeDisplay child (`digits-<n>`).
    digits_id: String,
    started: bool,
    start_instant: Option<Instant>,
    total_elapsed_secs: f64,
}

impl Stopwatch {
    fn new() -> Self {
        let n = NEXT_SW.fetch_add(1, Ordering::Relaxed);
        let digits_id = format!("digits-{n}");
        let inner = HorizontalGroup::new().with_compose(vec![
            ChildDecl::from(Button::success("Start").id("start")),
            ChildDecl::from(Button::error("Stop").id("stop")),
            ChildDecl::from(Button::new("Reset").id("reset")),
            ChildDecl::from(TimeDisplay::new("00:00:00.00")).with_id(&digits_id),
        ]);
        Self {
            inner,
            digits_id,
            started: false,
            start_instant: None,
            total_elapsed_secs: 0.0,
        }
    }

    fn elapsed_secs(&self) -> f64 {
        let base = self.total_elapsed_secs;
        if let Some(inst) = &self.start_instant {
            base + inst.elapsed().as_secs_f64()
        } else {
            base
        }
    }
}

fn format_time(secs: f64) -> String {
    let total_cs = (secs * 100.0) as u64;
    let cs = total_cs % 100;
    let total_s = total_cs / 100;
    let s = total_s % 60;
    let total_m = total_s / 60;
    let m = total_m % 60;
    let h = total_m / 60;
    format!("{h:02}:{m:02}:{s:02}.{cs:02}")
}

impl Widget for Stopwatch {
    fn style_type(&self) -> &'static str {
        "Stopwatch"
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

    fn focusable(&self) -> bool {
        false
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.inner.on_event(event, ctx);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.inner.on_event_capture(event, ctx);
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut EventCtx) {
        if let Some(bp) = message.downcast_ref::<ButtonPressed>() {
            match bp.button_id.as_deref() {
                Some("start") => {
                    self.started = true;
                    self.start_instant = Some(Instant::now());
                    ctx.add_class("started");
                    ctx.request_repaint();
                    ctx.set_handled();
                }
                Some("stop") => {
                    if let Some(inst) = self.start_instant.take() {
                        self.total_elapsed_secs += inst.elapsed().as_secs_f64();
                    }
                    self.started = false;
                    ctx.remove_class("started");
                    ctx.request_repaint();
                    ctx.set_handled();
                }
                Some("reset") => {
                    self.total_elapsed_secs = 0.0;
                    self.start_instant = None;
                    ctx.request_repaint();
                    ctx.set_handled();
                }
                _ => {}
            }
        }
    }

    fn on_tick(&mut self, tick: u64) {
        self.inner.on_tick(tick);
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.inner.on_layout(width, height);
    }
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

const INITIAL_STOPWATCHES: usize = 3;

struct StopwatchApp;

impl TextualApp for StopwatchApp {
    fn title(&self) -> &'static str {
        "StopwatchApp"
    }

    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn bindings(&self) -> Vec<BindingDecl> {
        vec![
            BindingDecl::new("d", "toggle_dark", "Toggle dark mode"),
            BindingDecl::new("a", "add_stopwatch", "Add"),
            BindingDecl::new("r", "remove_stopwatch", "Remove"),
        ]
    }

    fn compose(&mut self) -> AppRoot {
        let stopwatches: Vec<ChildDecl> = (0..INITIAL_STOPWATCHES)
            .map(|_| ChildDecl::from(Stopwatch::new()))
            .collect();

        AppRoot::new()
            .with_child(Header::new())
            .with_child(Footer::new())
            .with_child(
                Node::new(
                    VerticalScroll::new()
                        .with_child(Vertical::new().with_compose(stopwatches)),
                )
                .id("timers"),
            )
    }

    fn on_tick_with_app(&mut self, app: &mut App, _tick: u64, ctx: &mut EventCtx) {
        // Drive the live clock: for every running Stopwatch, push the formatted
        // elapsed time into its paired TimeDisplay child.
        let sw_nodes = app
            .query("Stopwatch")
            .map(|q| q.into_ids())
            .unwrap_or_default();
        let mut updates: Vec<(String, String)> = Vec::new();
        for node in sw_nodes {
            if let Some(Some(pair)) = app.with_widget_mut_as::<Stopwatch, _>(node, |sw| {
                sw.started
                    .then(|| (sw.digits_id.clone(), format_time(sw.elapsed_secs())))
            }) {
                updates.push(pair);
            }
        }
        let mut repainted = false;
        for (digits_id, time_str) in updates {
            let sel = format!("#{digits_id}");
            let _ = app.with_query_one_mut_as::<TimeDisplay, _>(&sel, |d| {
                d.inner.update(time_str);
            });
            repainted = true;
        }
        if repainted {
            ctx.request_repaint();
        }
    }

    fn on_key_with_app(&mut self, app: &mut App, key: &KeyEventData, ctx: &mut EventCtx) {
        match key.name() {
            "a" => {
                // Python: self.query_one("#timers").mount(Stopwatch())
                let _ = app.mount_under("#timers Vertical", Stopwatch::new());
                ctx.set_handled();
            }
            "r" => {
                // Python: self.query("Stopwatch").last().remove()
                if let Ok(last) = app.query("Stopwatch").and_then(|q| q.last()) {
                    let _ = app.remove_node(last);
                }
                ctx.set_handled();
            }
            _ => {}
        }
    }
}

fn main() -> textual::Result<()> {
    run_sync(StopwatchApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stopwatch_composes_without_panic() {
        let mut app = StopwatchApp;
        let _root = app.compose();
    }

    #[test]
    fn has_all_bindings() {
        let app = StopwatchApp;
        let bindings = app.bindings();
        assert!(bindings.iter().any(|b| b.key == "d"), "missing 'd' binding");
        assert!(bindings.iter().any(|b| b.key == "a"), "missing 'a' binding");
        assert!(bindings.iter().any(|b| b.key == "r"), "missing 'r' binding");
    }

    #[test]
    fn format_time_zero() {
        assert_eq!(format_time(0.0), "00:00:00.00");
    }

    #[test]
    fn format_time_one_minute() {
        assert_eq!(format_time(61.5), "00:01:01.50");
    }
}
