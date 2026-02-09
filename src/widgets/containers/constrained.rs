use rich_rs::{Console, ConsoleOptions, Renderable, Segments};

use crate::debug::DebugLayout;
use crate::event::{Event, EventCtx};
use crate::message::MessageEvent;

use crate::widgets::{
    LayoutConstraints, Widget, WidgetId, WidgetStyles,
    helpers::merge_constraints,
};

pub struct Constrained {
    id: WidgetId,
    child: Box<dyn Widget>,
    constraints: LayoutConstraints,
    styles: WidgetStyles,
}

impl Constrained {
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            id: WidgetId::new(),
            child: Box::new(child),
            constraints: LayoutConstraints::default(),
            styles: WidgetStyles::default(),
        }
    }

    pub fn min_width(mut self, value: usize) -> Self {
        self.constraints = self.constraints.min_width(value);
        self
    }

    pub fn max_width(mut self, value: usize) -> Self {
        self.constraints = self.constraints.max_width(value);
        self
    }

    pub fn min_height(mut self, value: usize) -> Self {
        self.constraints = self.constraints.min_height(value);
        self
    }

    pub fn max_height(mut self, value: usize) -> Self {
        self.constraints = self.constraints.max_height(value);
        self
    }
}

impl Widget for Constrained {
    fn id(&self) -> WidgetId {
        self.id
    }

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

    fn focusable(&self) -> bool {
        self.child.focusable()
    }

    fn set_focus(&mut self, focused: bool) {
        self.child.set_focus(focused);
    }

    fn layout_height(&self) -> Option<usize> {
        let constraints = self.layout_constraints();
        if let (Some(min), Some(max)) = (constraints.min_height, constraints.max_height) {
            if min == max {
                return Some(min);
            }
        }
        self.child.layout_height()
    }

    fn layout_constraints(&self) -> LayoutConstraints {
        merge_constraints(self.styles.layout, self.constraints)
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }

    fn visit_children_mut(&mut self, f: &mut dyn FnMut(&mut dyn Widget)) {
        f(self.child.as_mut());
    }
}

impl Renderable for Constrained {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}
