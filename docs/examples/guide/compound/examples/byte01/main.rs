/// Port of Python Textual `docs/examples/guide/compound/byte01.py`.
///
/// Demonstrates compound widgets composed from simpler widgets:
/// - `BitSwitch`: a vertical container with a centered `Label` (bit number) and a `Switch`.
/// - `ByteInput`: a horizontal container with 8 `BitSwitch` widgets (bits 7..0).
/// - `ByteEditor`: a vertical layout with two `Container`s — top shows an `Input`,
///   bottom shows the `ByteInput`.
///
/// Python: `BitSwitch(Widget)`, `ByteInput(Widget)`, `ByteEditor(Widget)`.
/// Rust: thin wrappers around `Vertical`/`Horizontal`/`Container` with custom
/// `style_type()` so CSS type selectors work correctly.
use textual::prelude::*;

const CSS: &str = r#"
BitSwitch {
    layout: vertical;
    width: auto;
    height: auto;
}

BitSwitch > Label {
    text-align: center;
    width: 100%;
}

ByteInput {
    width: auto;
    height: auto;
    border: blank;
    layout: horizontal;
}

ByteInput:focus-within {
    border: heavy $secondary;
}

ByteEditor > Container {
    height: 1fr;
    align: center middle;
}

ByteEditor > Container.top {
    background: $boost;
}

ByteEditor Input {
    width: 16;
}
"#;

// ---------------------------------------------------------------------------
// BitSwitch: vertical layout — Label (bit number) above Switch
// ---------------------------------------------------------------------------

struct BitSwitch {
    inner: Vertical,
}

impl BitSwitch {
    fn new(bit: u8) -> Self {
        let inner = Vertical::new()
            .with_child(Label::new(bit.to_string()))
            .with_child(Switch::new(false));
        Self { inner }
    }
}

impl Widget for BitSwitch {
    fn style_type(&self) -> &'static str {
        "BitSwitch"
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

    fn on_tick(&mut self, tick: u64) {
        self.inner.on_tick(tick);
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.inner.on_layout(width, height);
    }

    fn layout_height(&self) -> Option<usize> {
        self.inner.layout_height()
    }

    fn content_width(&self) -> Option<usize> {
        self.inner.content_width()
    }
}

// ---------------------------------------------------------------------------
// ByteInput: horizontal layout — 8 BitSwitches (bits 7 down to 0)
// ---------------------------------------------------------------------------

struct ByteInput {
    inner: Horizontal,
}

impl ByteInput {
    fn new() -> Self {
        // Python: `for bit in reversed(range(8))` → bits 7,6,5,4,3,2,1,0
        let mut inner = Horizontal::new();
        for bit in (0u8..8).rev() {
            inner = inner.with_child(BitSwitch::new(bit));
        }
        Self { inner }
    }
}

impl Widget for ByteInput {
    fn style_type(&self) -> &'static str {
        "ByteInput"
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

    fn on_tick(&mut self, tick: u64) {
        self.inner.on_tick(tick);
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.inner.on_layout(width, height);
    }

    fn layout_height(&self) -> Option<usize> {
        self.inner.layout_height()
    }

    fn content_width(&self) -> Option<usize> {
        self.inner.content_width()
    }
}

// ---------------------------------------------------------------------------
// ByteEditor: vertical layout — top Container (Input) + bottom Container (ByteInput)
// ---------------------------------------------------------------------------

struct ByteEditor {
    inner: Vertical,
}

impl ByteEditor {
    fn new() -> Self {
        let inner = Vertical::new()
            .with_child(
                Container::new()
                    .class("top")
                    .with_child(Input::new().with_placeholder("byte")),
            )
            .with_child(Container::new().with_child(ByteInput::new()));
        Self { inner }
    }
}

impl Widget for ByteEditor {
    fn style_type(&self) -> &'static str {
        "ByteEditor"
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

    fn on_tick(&mut self, tick: u64) {
        self.inner.on_tick(tick);
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.inner.on_layout(width, height);
    }

    fn layout_height(&self) -> Option<usize> {
        self.inner.layout_height()
    }

    fn content_width(&self) -> Option<usize> {
        self.inner.content_width()
    }
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

struct ByteInputApp;

impl TextualApp for ByteInputApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(ByteEditor::new())
    }
}

fn main() -> textual::Result<()> {
    run_sync(ByteInputApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_input_app_composes_without_panic() {
        let mut app = ByteInputApp;
        let _root = app.compose();
    }

    /// LIVENESS PROBE — byte01 is the structural-only first step of the compound
    /// tutorial: the Switches are NOT wired to the Input (Python byte01.py has no
    /// message handlers either — wiring arrives in byte02/byte03). But the Switch
    /// widget itself is interactive. We focus the first Switch and toggle it,
    /// asserting the Switch's own value flipped — the honest "interaction did
    /// something" signal.
    ///
    /// NOTE: we assert switch *state* (value), not the rendered frame: the Switch
    /// renders an animated slider whose position advances on ticks, and the
    /// tick/animation clock does not progress in an instant headless test, so the
    /// slider visual would not move even though the toggle is fully live. State is
    /// the robust, animation-independent liveness signal here.
    #[test]
    fn liveness_switch_toggles() {
        textual::run_test(ByteInputApp, |pilot| {
            let before = pilot
                .app_mut()
                .with_query_one_mut_as::<Switch, _>("Switch", |s| s.value())
                .unwrap_or(false);
            // Clicking the Switch toggles it.
            pilot.click("Switch")?;
            let after = pilot
                .app_mut()
                .with_query_one_mut_as::<Switch, _>("Switch", |s| s.value())
                .unwrap_or(before);
            assert_ne!(
                before, after,
                "toggling a Switch must flip its value"
            );
            Ok(())
        })
        .unwrap();
    }
}
