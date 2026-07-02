/// Port of Python Textual `docs/examples/guide/compound/byte02.py`.
///
/// Demonstrates compound widgets with custom messages:
/// - `BitSwitch`: a Switch with a numeric label, posts `BitChanged` when toggled.
/// - `ByteInput`: 8 `BitSwitch` widgets arranged horizontally (bits 7..=0).
/// - `ByteEditor`: Input (shows byte value as decimal) + ByteInput.
///
/// When a switch is toggled, `BitChanged` bubbles to the app. The app reads all
/// 8 switch values (by id `switch-0`..`switch-7`), computes the byte value, and
/// updates the Input widget.
use textual::prelude::*;

// ---------------------------------------------------------------------------
// Custom message: BitChanged
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct BitChanged {
    bit: u8,
    value: bool,
}

textual::impl_message!(BitChanged);

// ---------------------------------------------------------------------------
// CSS
// ---------------------------------------------------------------------------

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
// BitSwitch widget
// ---------------------------------------------------------------------------

struct BitSwitch {
    bit: u8,
    value: bool,
    inner: VerticalGroup,
}

impl BitSwitch {
    fn new(bit: u8) -> Self {
        let switch_id = format!("switch-{bit}");
        let inner = VerticalGroup::new()
            .with_child(Label::new(bit.to_string()))
            .with_child(Switch::new(false).id(switch_id));
        Self {
            bit,
            value: false,
            inner,
        }
    }
}

impl Widget for BitSwitch {
    fn style_type(&self) -> &'static str {
        "BitSwitch"
    }

    fn render(
        &self,
        console: &rich_rs::Console,
        options: &rich_rs::ConsoleOptions,
    ) -> rich_rs::Segments {
        self.inner.render(console, options)
    }

    fn compose(&mut self) -> textual::compose::ComposeResult {
        self.inner.compose()
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.inner.on_event(event, ctx);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.inner.on_event_capture(event, ctx);
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut EventCtx) {
        if let Some(sc) = message.downcast_ref::<SwitchChanged>() {
            self.value = sc.value;
            // Post a BitChanged message so the app can update the byte value.
            ctx.post_message(BitChanged {
                bit: self.bit,
                value: self.value,
            });
            ctx.set_handled();
            return;
        }
        self.inner.on_message(message, ctx);
    }

    fn on_tick(&mut self, tick: u64) {
        self.inner.on_tick(tick);
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.inner.on_layout(width, height);
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        self.inner.take_node_seed()
    }

    fn focusable(&self) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// ByteInput widget: 8 BitSwitches (bits 7 down to 0)
// ---------------------------------------------------------------------------

struct ByteInput {
    inner: HorizontalGroup,
}

impl ByteInput {
    fn new() -> Self {
        // Bits 7..=0 left to right, mirroring Python's `reversed(range(8))`.
        let mut inner = HorizontalGroup::new();
        for bit in (0..8u8).rev() {
            inner.push(BitSwitch::new(bit));
        }
        Self { inner }
    }
}

impl Widget for ByteInput {
    fn style_type(&self) -> &'static str {
        "ByteInput"
    }

    fn render(
        &self,
        console: &rich_rs::Console,
        options: &rich_rs::ConsoleOptions,
    ) -> rich_rs::Segments {
        self.inner.render(console, options)
    }

    fn compose(&mut self) -> textual::compose::ComposeResult {
        self.inner.compose()
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.inner.on_event(event, ctx);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.inner.on_event_capture(event, ctx);
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut EventCtx) {
        self.inner.on_message(message, ctx);
    }

    fn on_tick(&mut self, tick: u64) {
        self.inner.on_tick(tick);
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.inner.on_layout(width, height);
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        self.inner.take_node_seed()
    }

    fn focusable(&self) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// ByteEditor widget: Container.top (Input) + Container (ByteInput)
// ---------------------------------------------------------------------------

struct ByteEditor {
    inner: VerticalGroup,
}

impl ByteEditor {
    fn new() -> Self {
        let inner = VerticalGroup::new()
            .with_child(
                Container::new()
                    .class("top")
                    .with_child(Input::new().with_placeholder("byte").id("byte-input")),
            )
            .with_child(Container::new().with_child(ByteInput::new()));
        Self { inner }
    }
}

impl Widget for ByteEditor {
    fn style_type(&self) -> &'static str {
        "ByteEditor"
    }

    fn render(
        &self,
        console: &rich_rs::Console,
        options: &rich_rs::ConsoleOptions,
    ) -> rich_rs::Segments {
        self.inner.render(console, options)
    }

    fn compose(&mut self) -> textual::compose::ComposeResult {
        self.inner.compose()
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.inner.on_event(event, ctx);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.inner.on_event_capture(event, ctx);
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut EventCtx) {
        self.inner.on_message(message, ctx);
    }

    fn on_tick(&mut self, tick: u64) {
        self.inner.on_tick(tick);
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.inner.on_layout(width, height);
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        self.inner.take_node_seed()
    }

    fn focusable(&self) -> bool {
        false
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

    fn on_message_with_app(
        &mut self,
        app: &mut App,
        message: &MessageEvent,
        ctx: &mut EventCtx,
    ) {
        if message.downcast_ref::<BitChanged>().is_some() {
            // Compute byte value by reading all 8 switch states.
            let mut byte_val: u32 = 0;
            for bit in 0..8u32 {
                let selector = format!("#switch-{bit}");
                let is_on = app
                    .with_query_one_mut_as::<Switch, _>(&selector, |sw| sw.value())
                    .unwrap_or(false);
                if is_on {
                    byte_val |= 1 << bit;
                }
            }
            // Update the Input widget with the new decimal value.
            let _ = app.with_query_one_mut_as::<Input, _>("#byte-input", |inp| {
                inp.set_text(byte_val.to_string());
            });
            ctx.request_repaint();
            ctx.set_handled();
        }
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

    #[test]
    fn bit_changed_message_fields() {
        let m = BitChanged { bit: 3, value: true };
        assert_eq!(m.bit, 3);
        assert!(m.value);
    }

    #[test]
    fn bit_switch_new_has_correct_bit() {
        let bs = BitSwitch::new(5);
        assert_eq!(bs.bit, 5);
        assert!(!bs.value);
    }

    /// LIVENESS PROBE — toggling a bit Switch must recompute the byte and update
    /// the `#byte-input` Input (Switch -> Input wiring). We assert the Input text
    /// itself changed (state, not just frame). A dead demo (unwired SwitchChanged
    /// / byte not recomputed) leaves the Input empty and fails this gate.
    #[test]
    fn liveness_toggling_switch_updates_byte_input() {
        textual::run_test(ByteInputApp, |pilot| {
            // Clicking the first switch toggles bit 0 (value 1).
            pilot.click("#switch-0")?;
            let text = pilot
                .app_mut()
                .with_query_one_mut_as::<Input, _>("#byte-input", |i| i.text().to_string())
                .unwrap_or_default();
            assert_eq!(
                text, "1",
                "toggling bit 0 must set the byte Input to 1 (got {text:?})"
            );
            Ok(())
        })
        .unwrap();
    }
}
