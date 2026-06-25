/// Port of Python Textual `docs/examples/guide/widgets/tooltip01.py`.
///
/// Demonstrates the system tooltip feature: hovering over a button shows a
/// multi-line tooltip using the widget's `tooltip()` hook.
///
/// Python structure:
///   - TooltipApp(App) — single `Button("Click me", variant="success")`
///   - Screen aligned center middle
///   - on_mount: sets `self.query_one(Button).tooltip = TEXT`
///
/// Rust differences:
///   - Python sets `.tooltip` as a property on an existing widget after composition.
///     Rust has no post-compose mutation hook equivalent, so we use a thin
///     `TooltipButton` wrapper that delegates everything to `Button` but overrides
///     `Widget::tooltip()` to return the text.
///   - CSS `Button { ... }` still resolves correctly because `style_type` is
///     delegated to the inner `Button`.
///
/// Framework gap: `Button` does not yet expose a `with_tooltip()` builder method.
/// Once that is added this wrapper becomes unnecessary.
use textual::prelude::*;

// ---------------------------------------------------------------------------
// Content
// ---------------------------------------------------------------------------

const TEXT: &str = "I must not fear.\n\
Fear is the mind-killer.\n\
Fear is the little-death that brings total obliteration.\n\
I will face my fear.";

// ---------------------------------------------------------------------------
// CSS
// ---------------------------------------------------------------------------

const CSS: &str = r#"
Screen {
    align: center middle;
}
"#;

// ---------------------------------------------------------------------------
// TooltipButton — thin wrapper that adds a tooltip to any Button
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
    // --- Core required ---

    fn render(
        &self,
        console: &rich_rs::Console,
        options: &rich_rs::ConsoleOptions,
    ) -> rich_rs::Segments {
        self.inner.render(console, options)
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        self.inner.take_node_seed()
    }

    // --- Type identity (keep Button CSS selectors working) ---

    fn style_type(&self) -> &'static str {
        self.inner.style_type()
    }

    fn style_type_aliases(&self) -> &[&'static str] {
        self.inner.style_type_aliases()
    }

    fn style_classes(&self) -> &[String] {
        self.inner.style_classes()
    }

    fn style_id(&self) -> Option<&str> {
        self.inner.style_id()
    }

    // --- Tooltip override (the whole point of this wrapper) ---

    fn tooltip(&self) -> Option<String> {
        Some(self.tooltip_text.clone())
    }

    fn tooltip_anchor(&self) -> Option<(u16, u16)> {
        self.inner.tooltip_anchor()
    }

    // --- Focus / interaction ---

    fn focusable(&self) -> bool {
        self.inner.focusable()
    }

    fn can_focus(&self) -> bool {
        self.inner.can_focus()
    }

    fn can_focus_children(&self) -> bool {
        self.inner.can_focus_children()
    }

    fn mouse_interactive(&self) -> bool {
        self.inner.mouse_interactive()
    }

    fn is_active(&self) -> bool {
        self.inner.is_active()
    }

    fn is_initially_disabled(&self) -> bool {
        self.inner.is_initially_disabled()
    }

    // --- Lifecycle ---

    fn on_mount(&mut self) {
        self.inner.on_mount();
    }

    fn on_unmount(&mut self) {
        self.inner.on_unmount();
    }

    fn on_tick(&mut self, tick: u64) {
        self.inner.on_tick(tick);
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        self.inner.on_resize(width, height);
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.inner.on_layout(width, height);
    }

    fn on_node_state_changed(&mut self, old: NodeState, new: NodeState) {
        self.inner.on_node_state_changed(old, new);
    }

    // --- Events ---

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.inner.on_event_capture(event, ctx);
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.inner.on_event(event, ctx);
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut EventCtx) {
        self.inner.on_message(message, ctx);
    }

    fn on_mouse_move(&mut self, x: u16, y: u16) -> bool {
        self.inner.on_mouse_move(x, y)
    }

    fn on_mouse_scroll(&mut self, delta_x: i32, delta_y: i32, ctx: &mut EventCtx) {
        self.inner.on_mouse_scroll(delta_x, delta_y, ctx);
    }

    // --- Actions / bindings ---

    fn bindings(&self) -> Vec<BindingDecl> {
        self.inner.bindings()
    }

    fn binding_hints(&self) -> Vec<BindingHint> {
        self.inner.binding_hints()
    }

    fn execute_action(&mut self, action: &ParsedAction, ctx: &mut EventCtx) -> bool {
        self.inner.execute_action(action, ctx)
    }

    fn action_namespace(&self) -> &str {
        self.inner.action_namespace()
    }

    // --- Layout / sizing ---

    fn content_width(&self) -> Option<usize> {
        self.inner.content_width()
    }

    fn layout_height(&self) -> Option<usize> {
        self.inner.layout_height()
    }

    // --- Style ---

    fn style(&self) -> Option<Style> {
        self.inner.style()
    }

    fn set_inline_style(&mut self, style: Style) {
        self.inner.set_inline_style(style);
    }

    // --- Reactive ---

    fn reactive_widget(&mut self) -> Option<&mut dyn ReactiveWidget> {
        self.inner.reactive_widget()
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
    fn tooltip_text_is_set() {
        let btn = TooltipButton::new(Button::success("Click me"), TEXT);
        assert_eq!(btn.tooltip(), Some(TEXT.to_string()));
    }

    #[test]
    fn style_type_delegates_to_button() {
        let btn = TooltipButton::new(Button::success("Click me"), TEXT);
        assert_eq!(btn.style_type(), "Button");
    }

    #[test]
    fn app_composes_without_panic() {
        let mut app = TooltipApp;
        let _root = app.compose();
    }

    // -- LIVENESS PROBE (Pilot run_test) — now LIVE ---------------------------
    // tooltip01's interaction is: hover the Button and a Tooltip overlay appears
    // with TEXT. `Pilot::hover(selector)` injects a mouse move through the same
    // headless dispatch as click injection, arming the shared system tooltip for
    // the hovered owner — so the overlay mounts and the rendered frame changes.
    #[test]
    fn liveness_hover_shows_tooltip() {
        textual::run_test(TooltipApp, |pilot| {
            let before = pilot.app().frame_fingerprint();
            pilot.hover("Button")?;
            assert_ne!(
                before,
                pilot.app().frame_fingerprint(),
                "hovering the Button must show the tooltip overlay (frame changes)"
            );
            Ok(())
        })
        .unwrap();
    }
}
