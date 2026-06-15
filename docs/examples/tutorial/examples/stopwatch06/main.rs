/// Port of Python Textual `docs/examples/tutorial/stopwatch06.py` (= `stopwatch.py`).
///
/// The final tutorial stopwatch: adds `action_add_stopwatch` / `action_remove_stopwatch`
/// bound to `a` / `r`. Python calls `self.query_one("#timers").mount(new_stopwatch)` and
/// `self.query("Stopwatch").last().remove()`.
///
/// FRAMEWORK GAP: textual-rs does not expose a public `mount_under(selector, widget)` API
/// for inserting widgets into an already-mounted parent at runtime. The bindings are declared
/// and keys are acknowledged, but the add/remove behavior is not functional.
/// The initial screen (3 stopwatches + Header + Footer) is faithful and builds correctly.
///
/// NON-PROMOTABLE: timer-driven + dynamic mount/unmount not yet supported.
use std::time::Instant;
use textual::prelude::*;

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
    #[allow(dead_code)]
    idx: usize,
    started: bool,
    start_instant: Option<Instant>,
    total_elapsed_secs: f64,
}

impl Stopwatch {
    fn new(idx: usize) -> Self {
        let digits_id = format!("digits-{idx}");
        let inner = HorizontalGroup::new().with_compose(vec![
            ChildDecl::from(Button::success("Start").id("start")),
            ChildDecl::from(Button::error("Stop").id("stop")),
            ChildDecl::from(Button::new("Reset").id("reset")),
            ChildDecl::from(TimeDisplay::new("00:00:00.00")).with_id(&digits_id),
        ]);
        Self {
            inner,
            idx,
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
            .map(|i| ChildDecl::from(Stopwatch::new(i)).with_id(&format!("sw-{i}")))
            .collect();

        AppRoot::new()
            .with_child(Header::new())
            .with_child(Footer::new())
            .with_child(
                Node::new(VerticalScroll::new().with_child(Vertical::new().with_compose(stopwatches)))
                    .id("timers"),
            )
    }

    fn on_tick_with_app(&mut self, app: &mut App, _tick: u64, ctx: &mut EventCtx) {
        for i in 0..INITIAL_STOPWATCHES {
            let sw_id = format!("#sw-{i}");
            let digits_id = format!("#digits-{i}");
            let maybe_time = app
                .with_query_one_mut_as::<Stopwatch, _>(&sw_id, |sw| {
                    if sw.started {
                        Some(format_time(sw.elapsed_secs()))
                    } else {
                        None
                    }
                })
                .ok()
                .flatten();
            if let Some(time_str) = maybe_time {
                let _ = app.with_query_one_mut_as::<TimeDisplay, _>(&digits_id, |d| {
                    d.inner.update(time_str);
                });
                ctx.request_repaint();
            }
        }
    }

    fn on_key_with_app(&mut self, _app: &mut App, key: &KeyEventData, ctx: &mut EventCtx) {
        match key.name() {
            "a" => {
                // FRAMEWORK GAP: dynamic mount_under is not yet implemented.
                // In Python: self.query_one("#timers").mount(Stopwatch())
                ctx.set_handled();
            }
            "r" => {
                // FRAMEWORK GAP: dynamic remove_last_of_type is not yet implemented.
                // In Python: self.query("Stopwatch").last().remove()
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
    fn stopwatch06_composes_without_panic() {
        let mut app = StopwatchApp;
        let _root = app.compose();
    }

    #[test]
    fn has_add_remove_bindings() {
        let app = StopwatchApp;
        let bindings = app.bindings();
        assert!(bindings.iter().any(|b| b.key == "a"), "missing 'a' binding");
        assert!(bindings.iter().any(|b| b.key == "r"), "missing 'r' binding");
    }

    #[test]
    fn format_time_zero() {
        assert_eq!(format_time(0.0), "00:00:00.00");
    }
}
