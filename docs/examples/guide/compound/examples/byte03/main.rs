/// Port of Python Textual `docs/examples/guide/compound/byte03.py`.
///
/// Demonstrates compound widgets with **bidirectional** reactive binding:
/// - `BitSwitch`: Switch with a numeric Label above; posts `BitChanged` when toggled.
/// - `ByteInput`: 8 `BitSwitch` widgets in a horizontal row (bits 7..=0).
/// - `ByteEditor`: an `Input` (shows byte decimal) stacked above a `ByteInput`.
///
/// Switch → Input: when a bit flips, the app reads all 8 switch states, computes
/// the byte value, and updates the Input via `Input::set_text`.
///
/// Input → Switches: when the Input text changes (`InputChanged`), the app parses
/// the decimal value (clamped 0..=255) and programmatically sets each Switch via
/// `Handle::update` (which fires the watcher so slider and CSS classes sync).
///
/// Framework notes:
/// - Python uses `with switch.prevent(BitSwitch.BitChanged)` when updating switches
///   from the Input watcher to avoid feedback loops. Rust's `prevent` scope is now
///   ambient and spans the reactive update→re-dispatch cycle (Python's
///   `ContextVar`-backed prevent stack + `Message._prevent` snapshot): the
///   `Handle::update` reactive mutation captures the active prevented set, the
///   Switch watcher's `SwitchChanged` rides it, and `BitChanged` posted while
///   handling that `SwitchChanged` is suppressed — so this port uses the real
///   `ctx.prevent::<BitChanged, _>(...)` scope, exactly like Python.
/// - Python `ByteEditor.validate_value` clamps 0..=255 via `clamp()`. In Rust we
///   clamp directly in the `InputChanged` handler.
use textual::prelude::*;

// ---------------------------------------------------------------------------
// Custom message
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct BitChanged {
    /// Message contract fields (Python `BitSwitch.BitChanged.bit/.value`); the
    /// app handler recomputes the byte by querying all switches instead of
    /// reading them, exactly like the Python example.
    #[allow(dead_code)]
    bit: u8,
    #[allow(dead_code)]
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

    fn on_event(&mut self, event: &Event, ctx: &mut textual::event::WidgetCtx) {
        self.inner.on_event(event, ctx);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut textual::event::WidgetCtx) {
        self.inner.on_event_capture(event, ctx);
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut textual::event::WidgetCtx) {
        if let Some(sc) = message.downcast_ref::<SwitchChanged>() {
            // Intercept raw SwitchChanged and re-emit as BitChanged.
            // Python: `on_switch_changed` stops propagation, sets self.value,
            //         posts BitChanged(self.bit, event.value).
            self.value = sc.value;
            ctx.post_message(BitChanged {
                bit: self.bit,
                value: sc.value,
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
// ByteInput widget: 8 BitSwitches (bits 7..=0 left to right)
// ---------------------------------------------------------------------------

struct ByteInput {
    inner: HorizontalGroup,
}

impl ByteInput {
    fn new() -> Self {
        // Bits 7..=0 left-to-right, matching Python's `reversed(range(8))`.
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

    fn on_event(&mut self, event: &Event, ctx: &mut textual::event::WidgetCtx) {
        self.inner.on_event(event, ctx);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut textual::event::WidgetCtx) {
        self.inner.on_event_capture(event, ctx);
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut textual::event::WidgetCtx) {
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
// ByteEditor widget: Container.top (Input) stacked above Container (ByteInput)
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

    fn on_event(&mut self, event: &Event, ctx: &mut textual::event::WidgetCtx) {
        self.inner.on_event(event, ctx);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut textual::event::WidgetCtx) {
        self.inner.on_event_capture(event, ctx);
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut textual::event::WidgetCtx) {
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

impl ByteInputApp {
    fn new() -> Self {
        Self
    }
}

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
        ctx: &mut textual::event::WidgetCtx,
    ) {
        if let Some(_bc) = message.downcast_ref::<BitChanged>() {
            // Switches changed → compute byte value → update Input.
            // Python: `on_bit_switch_bit_changed`: iterate all BitSwitches, OR bits.
            let mut byte_val: u32 = 0;
            for bit in 0..8u32 {
                let sel = format!("#switch-{bit}");
                let is_on = app
                    .with_query_one_mut_as::<Switch, _>(&sel, |sw| sw.value())
                    .unwrap_or(false);
                if is_on {
                    byte_val |= 1 << bit;
                }
            }
            let _ = app.with_query_one_mut_as::<Input, _>("#byte-input", |inp| {
                inp.set_text(byte_val.to_string());
            });
            ctx.request_repaint();
            ctx.set_handled();
        } else if let Some(ic) = message.downcast_ref::<InputChanged>() {
            // Input text changed → parse as 0..=255 → update each Switch.
            // Python: `on_input_changed` sets `self.value`; `watch_value` updates switches
            //         while suppressing `BitChanged` via `prevent`.
            let text = ic.value.clone();
            let byte_val: u32 = text
                .trim()
                .parse::<i64>()
                .map(|v| v.clamp(0, 255) as u32)
                .unwrap_or(0);

            // Suppress feedback while we set switches programmatically —
            // Python: `with switch.prevent(BitSwitch.BitChanged):`. The scope
            // spans the reactive update→re-dispatch cycle: each `Handle::update`
            // captures the prevented set, the Switch watcher's `SwitchChanged`
            // carries it, and the `BitChanged` that `BitSwitch` posts while
            // handling that `SwitchChanged` is dropped.
            ctx.prevent::<BitChanged, _>(|_ectx| {
                for bit in 0..8u8 {
                    let bit_on = (byte_val >> bit) & 1 == 1;
                    let switch_sel = format!("#switch-{bit}");
                    // Use `query_one_typed` + `Handle::update` so the reactive watcher
                    // fires, snapping the slider position and rebuilding CSS classes.
                    if let Ok(handle) = app.query_one_typed::<Switch>(&switch_sel) {
                        let _ = handle.update(app, |sw, rctx| {
                            sw.set_value(bit_on, rctx);
                        });
                    }
                }
            });
            ctx.request_repaint();
        }
    }
}

fn main() -> textual::Result<()> {
    run_sync(ByteInputApp::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte03_composes_without_panic() {
        let mut app = ByteInputApp::new();
        let _root = app.compose();
    }

    #[test]
    fn bit_changed_message_fields() {
        let m = BitChanged { bit: 3, value: true };
        assert_eq!(m.bit, 3);
        assert!(m.value);
    }

    #[test]
    fn bit_switch_initial_value_false() {
        let bs = BitSwitch::new(5);
        assert_eq!(bs.bit, 5);
        assert!(!bs.value);
    }

    #[test]
    fn byte_input_has_eight_switches() {
        let mut bi = ByteInput::new();
        let children = bi.inner.compose();
        assert_eq!(children.len(), 8);
    }

    /// LIVENESS PROBE (LIVE).
    ///
    /// Toggling a bit Switch posts `BitChanged`, recomputes the byte, and updates
    /// the Input (Switch -> Input wiring). We assert the Input's own text changed
    /// (state, not just frame — the Switch's own toggle visual would dirty the
    /// frame regardless, a false positive we avoid).
    ///
    /// ROOT (fixed): the app writes the recomputed byte via
    /// `app.with_query_one_mut_as::<Input, _>("#byte-input", ...)`, where
    /// `#byte-input` is `Input::new().id("byte-input")`. Previously the
    /// id landed on the `Node` *wrapper*, so the typed `#byte-input` query matched
    /// the `Node` and the `Input` downcast failed — the byte was never written.
    /// The node-build pipeline now collapses the structural `Node` out of the tree
    /// and forwards the id to the inner `Input`, so `#byte-input` resolves to the
    /// `Input` itself (Python parity: `Input(id="byte-input")`).
    #[test]
    fn liveness_toggling_switch_updates_input() {
        textual::run_test(ByteInputApp::new(), |pilot| {
            let initial = pilot
                .app_mut()
                .with_query_one_mut_as::<Input, _>("Input", |i| i.text().to_string())
                .unwrap_or_default();
            // NOTE: click only — a mouse press now FOCUSES the switch
            // (Python `Screen._forward_event` click-to-focus), so a trailing
            // Enter would toggle the freshly-focused switch straight back off.
            pilot.click("#switch-0")?;
            let text = pilot
                .app_mut()
                .with_query_one_mut_as::<Input, _>("Input", |i| i.text().to_string())
                .unwrap_or_default();
            assert_ne!(
                text, initial,
                "toggling a switch must update the byte Input (initial {initial:?}, got {text:?})"
            );
            assert!(!text.is_empty() && text != "0");
            Ok(())
        })
        .unwrap();
    }
}
