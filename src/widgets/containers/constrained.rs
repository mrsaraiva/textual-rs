use rich_rs::{Console, ConsoleOptions, Renderable, Segments};

use crate::debug::DebugLayout;
use crate::event::{Event, EventCtx};
use crate::message::MessageEvent;

use crate::widgets::{LayoutConstraints, Spacer, Widget, WidgetStyles, helpers::merge_constraints};

pub struct Constrained {
    child: Box<dyn Widget>,
    constraints: LayoutConstraints,
    styles: WidgetStyles,
    child_extracted: bool,
}

impl Constrained {
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            child: Box::new(child),
            constraints: LayoutConstraints::default(),
            styles: WidgetStyles::default(),
            child_extracted: false,
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

    fn is_tree_mode(&self) -> bool {
        self.child_extracted
    }
}

impl Widget for Constrained {
    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        if self.child_extracted {
            return Vec::new();
        }
        self.child_extracted = true;
        let child = std::mem::replace(&mut self.child, Box::new(Spacer::new(1)));
        vec![child]
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        if self.is_tree_mode() {
            return Segments::new();
        }
        self.child.render_styled(console, options)
    }

    fn render_with_debug(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        debug: &DebugLayout,
    ) -> Segments {
        if self.is_tree_mode() {
            return Segments::new();
        }
        self.child.render_styled_with_debug(console, options, debug)
    }

    fn on_mount(&mut self) {
        if !self.is_tree_mode() {
            self.child.on_mount();
        }
    }

    fn on_unmount(&mut self) {
        if !self.is_tree_mode() {
            self.child.on_unmount();
        }
    }

    fn on_tick(&mut self, tick: u64) {
        if !self.is_tree_mode() {
            self.child.on_tick(tick);
        }
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        if !self.is_tree_mode() {
            self.child.on_resize(width, height);
        }
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        if !self.is_tree_mode() {
            self.child.on_layout(width, height);
        }
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        if !self.is_tree_mode() {
            self.child.on_event_capture(event, ctx);
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if !self.is_tree_mode() {
            self.child.on_event(event, ctx);
        }
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut EventCtx) {
        if !self.is_tree_mode() {
            self.child.on_message(message, ctx);
        }
    }

    fn focusable(&self) -> bool {
        if self.is_tree_mode() {
            return false;
        }
        self.child.focusable()
    }

    fn set_focus(&mut self, focused: bool) {
        if !self.is_tree_mode() {
            self.child.set_focus(focused);
        }
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
}

impl Renderable for Constrained {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constrained_extraction_returns_child() {
        let mut c = Constrained::new(Spacer::new(1));
        let children = c.take_composed_children();
        assert_eq!(children.len(), 1);
    }

    #[test]
    fn constrained_extraction_idempotent() {
        let mut c = Constrained::new(Spacer::new(1));
        let _ = c.take_composed_children();
        assert!(c.take_composed_children().is_empty());
    }

    #[test]
    fn constrained_render_after_extraction() {
        let mut c = Constrained::new(Spacer::new(1));
        let _ = c.take_composed_children();
        let console = Console::new();
        let options = ConsoleOptions {
            size: (20, 5),
            max_width: 20,
            ..Default::default()
        };
        let segments = Widget::render(&c, &console, &options);
        assert!(segments.is_empty());
    }

    #[test]
    fn constrained_is_tree_mode_after_extraction() {
        let mut c = Constrained::new(Spacer::new(1));
        assert!(!c.is_tree_mode());
        let _ = c.take_composed_children();
        assert!(c.is_tree_mode());
    }
}
