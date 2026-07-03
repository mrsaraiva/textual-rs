/// Port of Python Textual `docs/examples/tutorial/stopwatch03.py`.
///
/// Same as stopwatch02 but with CSS from `stopwatch03.tcss` applied.
/// The CSS targets `Stopwatch` and `TimeDisplay` type selectors, so we need
/// custom widget wrappers to expose those type names.
///
/// Python:
///   class TimeDisplay(Digits): ...
///   class Stopwatch(HorizontalGroup):
///       def compose(self): yield ...
///   CSS_PATH = "stopwatch03.tcss"
///
/// NON-PROMOTABLE (timer-driven).
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
"#;

// ---------------------------------------------------------------------------
// TimeDisplay: thin wrapper around Digits with its own style_type
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
// Stopwatch: thin wrapper around HorizontalGroup with its own style_type
// ---------------------------------------------------------------------------

struct Stopwatch {
    inner: HorizontalGroup,
}

impl Stopwatch {
    fn new() -> Self {
        let inner = HorizontalGroup::new().with_compose(compose![
            Button::success("Start").id("start"),
            Button::error("Stop").id("stop"),
            Button::new("Reset").id("reset"),
            TimeDisplay::new("00:00:00.00"),
        ]);
        Self { inner }
    }
}

impl Widget for Stopwatch {
    fn style_type(&self) -> &'static str {
        "Stopwatch"
    }

    fn compose(&mut self) -> textual::compose::ComposeResult {
        self.inner.compose()
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

    fn on_event(&mut self, event: &Event, ctx: &mut textual::event::WidgetCtx) {
        self.inner.on_event(event, ctx);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut textual::event::WidgetCtx) {
        self.inner.on_event_capture(event, ctx);
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
    fn stopwatch03_composes_without_panic() {
        let mut app = StopwatchApp;
        let _root = app.compose();
    }

    // -- LIVENESS PROBE (Pilot run_test) --------------------------------------
    // Like stopwatch02, this step adds CSS styling but no button handlers /
    // timer (matches Python's structure-only step). The composed UI is still
    // interactive: pressing Tab focuses the first Button and applies its focus
    // styling, changing the rendered frame. (The `d` toggle_dark binding is
    // separately DEAD — see stopwatch01's probe for the theme-token root.)
    #[test]
    fn liveness_tab_focuses_button_changes_frame() {
        textual::run_test(StopwatchApp, |pilot| {
            let before = pilot.app().frame_fingerprint();
            pilot.press(&["tab"])?;
            let after = pilot.app().frame_fingerprint();
            assert_ne!(
                before, after,
                "pressing Tab must focus the first Button and change the \
                 rendered frame"
            );
            Ok(())
        })
        .unwrap();
    }
}
