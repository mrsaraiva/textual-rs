/// Port of Python Textual `docs/examples/guide/widgets/tooltip02.py`.
///
/// Demonstrates setting a tooltip on a Button widget. In Python, `button.tooltip`
/// is set in `on_mount`; in Rust, `Button` does not yet carry a tooltip field, so
/// a thin `TooltipButton` wrapper overrides `Widget::tooltip()` — the same runtime
/// hook the framework queries when the user hovers over a widget.
///
/// Framework gap: `Button` does not expose a `with_tooltip()` / `set_tooltip()`
/// builder method. Once that is added, this wrapper can be replaced with a plain
/// `Button::success("Click me").with_tooltip(TEXT)`.
use textual::prelude::*;

const TEXT: &str = "I must not fear.
Fear is the mind-killer.
Fear is the little-death that brings total obliteration.
I will face my fear.";

const CSS: &str = r#"
Screen {
    align: center middle;
}
Tooltip {
    padding: 2 4;
    background: $primary;
    color: auto 90%;
}
"#;

// ---------------------------------------------------------------------------
// TooltipButton — thin wrapper that adds a tooltip to a Button
// ---------------------------------------------------------------------------

struct TooltipButton {
    inner: Button,
    tooltip_text: String,
}

impl TooltipButton {
    fn new(inner: Button, tooltip: impl Into<String>) -> Self {
        Self {
            inner,
            tooltip_text: tooltip.into(),
        }
    }
}

impl Widget for TooltipButton {
    fn render(
        &self,
        console: &rich_rs::Console,
        options: &rich_rs::ConsoleOptions,
    ) -> rich_rs::Segments {
        Widget::render(&self.inner, console, options)
    }

    fn style_type(&self) -> &'static str {
        self.inner.style_type()
    }

    fn focusable(&self) -> bool {
        self.inner.focusable()
    }

    fn mouse_interactive(&self) -> bool {
        self.inner.mouse_interactive()
    }

    fn on_mount(&mut self) {
        self.inner.on_mount();
    }

    fn on_unmount(&mut self) {
        self.inner.on_unmount();
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

    fn on_layout(&mut self, width: u16, height: u16) {
        self.inner.on_layout(width, height);
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        self.inner.on_resize(width, height);
    }

    fn on_tick(&mut self, tick: u64) {
        self.inner.on_tick(tick);
    }

    fn on_mouse_move(&mut self, x: u16, y: u16) -> bool {
        self.inner.on_mouse_move(x, y)
    }

    fn layout_height(&self) -> Option<usize> {
        self.inner.layout_height()
    }

    fn content_width(&self) -> Option<usize> {
        self.inner.content_width()
    }

    fn style(&self) -> Option<Style> {
        self.inner.style()
    }

    fn set_inline_style(&mut self, style: Style) {
        self.inner.set_inline_style(style);
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        self.inner.take_node_seed()
    }

    fn binding_hints(&self) -> Vec<BindingHint> {
        self.inner.binding_hints()
    }

    fn bindings(&self) -> Vec<BindingDecl> {
        self.inner.bindings()
    }

    /// Expose the tooltip text to the runtime hover system.
    fn tooltip(&self) -> Option<String> {
        Some(self.tooltip_text.clone())
    }
}

impl rich_rs::Renderable for TooltipButton {
    fn render(
        &self,
        console: &rich_rs::Console,
        options: &rich_rs::ConsoleOptions,
    ) -> rich_rs::Segments {
        Widget::render(self, console, options)
    }
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

struct TooltipApp;

impl TextualApp for TooltipApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(TooltipButton::new(Button::success("Click me"), TEXT))
    }
}

fn main() -> textual::Result<()> {
    run_sync(TooltipApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tooltip_button_returns_text() {
        let btn = TooltipButton::new(Button::success("Click me"), TEXT);
        assert_eq!(btn.tooltip(), Some(TEXT.to_string()));
    }

    #[test]
    fn app_composes_without_panic() {
        let mut app = TooltipApp;
        let _root = app.compose();
    }

    // -- LIVENESS PROBE (Pilot run_test) — UNCLEAR ----------------------------
    // Same as tooltip01: the interaction is hover → dwell → Tooltip overlay
    // (here with custom CSS). Pilot has no headless mouse-move/hover injection,
    // so the hover that arms the tooltip can't be delivered. The tooltip
    // content hook is unit-covered by `tooltip_button_returns_text`. Needs
    // `Pilot::hover` + dwell-timer `advance_clock` to become a real probe.
    // Tracking: pilot-mouse-move-injection / tooltip-hover-headless.
    #[ignore = "UNCLEAR: no headless hover injection to arm the tooltip; see comment"]
    #[test]
    fn liveness_hover_shows_tooltip_placeholder() {
        let btn = TooltipButton::new(Button::success("Click me"), TEXT);
        assert_eq!(btn.tooltip(), Some(TEXT.to_string()));
    }
}
