use rich_rs::{Console, ConsoleOptions, Renderable, Segments};

use crate::debug::DebugLayout;
use crate::event::{Event, EventCtx};
use crate::message::MessageEvent;
use crate::style::Style;

use crate::widgets::{
    LayoutConstraints, Spacer, Widget, WidgetStyles,
    helpers::{fixed_height_from_constraints, merge_constraints},
};

pub struct Styled {
    child: Box<dyn Widget>,
    styles: WidgetStyles,
    child_extracted: bool,
}

impl Styled {
    pub fn new(child: impl Widget + 'static, style: Style) -> Self {
        let mut styles = WidgetStyles::default();
        styles.style = style;
        Self {
            child: Box::new(child),
            styles,
            child_extracted: false,
        }
    }

    pub fn style(mut self, style: Style) -> Self {
        self.styles.style = style;
        self
    }

    fn is_tree_mode(&self) -> bool {
        self.child_extracted
    }
}

impl Widget for Styled {
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

    fn on_mouse_scroll(&mut self, delta_x: i32, delta_y: i32, ctx: &mut EventCtx) {
        if !self.is_tree_mode() {
            self.child.on_mouse_scroll(delta_x, delta_y, ctx);
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
        if self.is_tree_mode() {
            "Styled"
        } else {
            self.child.style_type()
        }
    }
}

impl Renderable for Styled {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn styled_extraction_returns_child() {
        let mut s = Styled::new(Spacer::new(1), Style::default());
        let children = s.take_composed_children();
        assert_eq!(children.len(), 1);
    }

    #[test]
    fn styled_extraction_idempotent() {
        let mut s = Styled::new(Spacer::new(1), Style::default());
        let _ = s.take_composed_children();
        assert!(s.take_composed_children().is_empty());
    }

    #[test]
    fn styled_render_after_extraction() {
        let mut s = Styled::new(Spacer::new(1), Style::default());
        let _ = s.take_composed_children();
        let console = Console::new();
        let options = ConsoleOptions {
            size: (20, 5),
            max_width: 20,
            ..Default::default()
        };
        let segments = Widget::render(&s, &console, &options);
        assert!(segments.is_empty());
    }

    #[test]
    fn styled_style_type_after_extraction() {
        let mut s = Styled::new(Spacer::new(1), Style::default());
        let _ = s.take_composed_children();
        assert_eq!(s.style_type(), "Styled");
    }

    #[test]
    fn styled_is_tree_mode_after_extraction() {
        let mut s = Styled::new(Spacer::new(1), Style::default());
        assert!(!s.is_tree_mode());
        let _ = s.take_composed_children();
        assert!(s.is_tree_mode());
    }
}
