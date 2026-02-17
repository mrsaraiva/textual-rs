use rich_rs::{Console, ConsoleOptions, Renderable, Segments};

use crate::debug::DebugLayout;
use crate::event::{Event, EventCtx};
use crate::style::Style;

use crate::widgets::{
    LayoutConstraints, Spacer, Widget, WidgetStyles,
    helpers::{fixed_height_from_constraints, merge_constraints},
};

pub struct Node {
    child: Box<dyn Widget>,
    style_id: Option<String>,
    classes: Vec<String>,
    styles: WidgetStyles,
    child_extracted: bool,
}

impl Node {
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            child: Box::new(child),
            style_id: None,
            classes: Vec::new(),
            styles: WidgetStyles::default(),
            child_extracted: false,
        }
    }

    pub fn id(mut self, value: impl Into<String>) -> Self {
        self.style_id = Some(value.into());
        self
    }

    pub fn class(mut self, value: impl Into<String>) -> Self {
        self.classes.push(value.into());
        self
    }

    pub fn classes(mut self, values: impl IntoIterator<Item = impl Into<String>>) -> Self {
        for value in values {
            self.classes.push(value.into());
        }
        self
    }
}

impl Widget for Node {
    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        if self.child_extracted {
            return Vec::new();
        }
        self.child_extracted = true;
        let child = std::mem::replace(&mut self.child, Box::new(Spacer::new(1)));
        vec![child]
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let _ = (console, options);
        Segments::new()
    }

    fn render_with_debug(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        debug: &DebugLayout,
    ) -> Segments {
        let _ = (console, options, debug);
        Segments::new()
    }

    fn on_mount(&mut self) {}

    fn on_unmount(&mut self) {}

    fn on_tick(&mut self, _tick: u64) {}

    fn on_resize(&mut self, _width: u16, _height: u16) {}

    fn on_event_capture(&mut self, _event: &Event, _ctx: &mut EventCtx) {}

    fn on_event(&mut self, _event: &Event, _ctx: &mut EventCtx) {}

    fn focusable(&self) -> bool {
        false
    }

    fn set_focus(&mut self, _focused: bool) {}

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
        "Node"
    }

    fn style_id(&self) -> Option<&str> {
        self.style_id.as_deref()
    }

    fn style_classes(&self) -> &[String] {
        &self.classes
    }
}

impl Renderable for Node {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_extraction_returns_child() {
        let mut n = Node::new(Spacer::new(1));
        let children = n.take_composed_children();
        assert_eq!(children.len(), 1);
    }

    #[test]
    fn node_extraction_idempotent() {
        let mut n = Node::new(Spacer::new(1));
        let _ = n.take_composed_children();
        assert!(n.take_composed_children().is_empty());
    }

    #[test]
    fn node_render_after_extraction() {
        let mut n = Node::new(Spacer::new(1));
        let _ = n.take_composed_children();
        let console = Console::new();
        let options = ConsoleOptions {
            size: (20, 5),
            max_width: 20,
            ..Default::default()
        };
        let segments = Widget::render(&n, &console, &options);
        assert!(segments.is_empty());
    }

    #[test]
    fn node_style_type_after_extraction() {
        let mut n = Node::new(Spacer::new(1));
        let _ = n.take_composed_children();
        assert_eq!(n.style_type(), "Node");
    }
}
