/// Port of Python Textual `docs/examples/tutorial/stopwatch04.py`.
///
/// Adds button-press handling: Start adds "started" class, Stop removes it.
/// CSS from `stopwatch04.tcss` hides/shows Start/Stop based on `.started`.
///
/// Python:
///   class Stopwatch(HorizontalGroup):
///       def on_button_pressed(self, event):
///           if event.button.id == "start": self.add_class("started")
///           elif event.button.id == "stop": self.remove_class("started")
///
/// In Rust, `add_class`/`remove_class` on a widget are done via `ReactiveCtx`
/// from within the widget's own event handler. Since `Stopwatch` here is a
/// wrapper, we handle button-pressed in the app and query the right node.
///
/// NON-PROMOTABLE (timer-driven + needs CSS class toggle).
use textual::compose;
use textual::prelude::*;

const CSS: &str = r#"
Stopwatch {
    background: $boost;
    height: 5;
    margin: 1;
    min-width: 50;
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

    /// Python's `class TimeDisplay(Digits)` inherits `Digits` DEFAULT_CSS via the
    /// MRO (notably `width: 1fr`). Declare `Digits` as a style-type alias so the
    /// framework applies the base widget's default CSS to this wrapper, matching
    /// Python's resolved styles.
    fn style_type_aliases(&self) -> &[&'static str] {
        &["Digits"]
    }

    fn focusable(&self) -> bool {
        false
    }

    fn render(
        &self,
        console: &rich_rs::Console,
        options: &rich_rs::ConsoleOptions,
    ) -> rich_rs::Segments {
        // Delegate to the inner Digits' plain render so the wrapper's forwarded
        // `text-align` (carried on `options.justify` by the runtime's generic
        // text-align propagation) reaches it. Using `render_styled` here would
        // make the inner re-resolve its own `"Digits"` type meta and clobber the
        // forwarded justify with the Digits default (`text-align: left`).
        self.inner.render(console, options)
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
    started: bool,
}

impl Stopwatch {
    fn new() -> Self {
        let inner = HorizontalGroup::new().with_compose(compose![
            Button::success("Start").id("start"),
            Button::error("Stop").id("stop"),
            Button::new("Reset").id("reset"),
            TimeDisplay::new("00:00:00.00"),
        ]);
        Self {
            inner,
            started: false,
        }
    }
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
        if let Event::Key(_) = event {
            // Pass through to inner
        }
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
                    ctx.add_class("started");
                    ctx.request_repaint();
                    ctx.set_handled();
                }
                Some("stop") => {
                    self.started = false;
                    ctx.remove_class("started");
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
        vec![BindingDecl::new("d", "toggle_dark", "Toggle dark mode")]
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Header::new())
            .with_child(Footer::new())
            .with_child(
                VerticalScroll::new().with_child(Vertical::new().with_compose(compose![
                    Stopwatch::new(),
                    Stopwatch::new(),
                    Stopwatch::new(),
                ])),
            )
    }
}

fn main() -> textual::Result<()> {
    run_sync(StopwatchApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stopwatch04_composes_without_panic() {
        let mut app = StopwatchApp;
        let _root = app.compose();
    }

    #[test]
    fn stopwatch_starts_not_started() {
        let sw = Stopwatch::new();
        assert!(!sw.started);
    }
}
