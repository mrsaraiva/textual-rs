use rich_rs::{Console, ConsoleOptions, Renderable, Segments};

use crate::debug::DebugLayout;
use crate::event::{Event, EventCtx};
use crate::message::MessageEvent;
use crate::style::Style;

use crate::widgets::{
    LayoutConstraints, Widget, WidgetStyles,
    helpers::{fixed_height_from_constraints, merge_constraints},
};

pub struct Styled {
    child: Box<dyn Widget>,
    styles: WidgetStyles,
}

impl Styled {
    pub fn new(child: impl Widget + 'static, style: Style) -> Self {
        let mut styles = WidgetStyles::default();
        styles.style = style;
        Self {
            child: Box::new(child),
            styles,
        }
    }

    pub fn style(mut self, style: Style) -> Self {
        self.styles.style = style;
        self
    }
}

impl Widget for Styled {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        self.child.render_styled(console, options)
    }

    fn render_with_debug(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        debug: &DebugLayout,
    ) -> Segments {
        self.child.render_styled_with_debug(console, options, debug)
    }

    fn on_mount(&mut self) {
        self.child.on_mount();
    }

    fn on_unmount(&mut self) {
        self.child.on_unmount();
    }

    fn on_tick(&mut self, tick: u64) {
        self.child.on_tick(tick);
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        self.child.on_resize(width, height);
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.child.on_layout(width, height);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.child.on_event_capture(event, ctx);
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.child.on_event(event, ctx);
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut EventCtx) {
        self.child.on_message(message, ctx);
    }

    fn on_mouse_scroll(&mut self, delta_x: i32, delta_y: i32, ctx: &mut EventCtx) {
        self.child.on_mouse_scroll(delta_x, delta_y, ctx);
    }

    fn focusable(&self) -> bool {
        self.child.focusable()
    }

    fn set_focus(&mut self, focused: bool) {
        self.child.set_focus(focused);
    }

    fn layout_height(&self) -> Option<usize> {
        if let Some(fixed) = fixed_height_from_constraints(self.layout_constraints()) {
            return Some(fixed);
        }
        self.child.layout_height()
    }

    fn layout_constraints(&self) -> LayoutConstraints {
        merge_constraints(self.styles.layout, self.child.layout_constraints())
    }

    fn style(&self) -> Option<Style> {
        Some(self.styles.style.clone())
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }

    fn style_type(&self) -> &'static str {
        self.child.style_type()
    }
}

impl Renderable for Styled {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}
