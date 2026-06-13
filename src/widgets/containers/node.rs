use rich_rs::{Console, ConsoleOptions, Renderable, Segments};

use crate::debug::DebugLayout;
use crate::event::{Event, EventCtx};
use crate::style::Style;

use crate::widgets::{LayoutConstraints, NodeSeed, Spacer, Widget};

pub struct Node {
    child: Box<dyn Widget>,
    seed: NodeSeed,
    child_extracted: bool,
}

impl Node {
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            child: Box::new(child),
            seed: NodeSeed::default(),
            child_extracted: false,
        }
    }

    pub fn id(mut self, value: impl Into<String>) -> Self {
        self.seed.css_id = Some(value.into());
        self
    }

    pub fn class(mut self, value: impl Into<String>) -> Self {
        self.seed.classes.push(value.into());
        self
    }

    pub fn classes(mut self, values: impl IntoIterator<Item = impl Into<String>>) -> Self {
        for value in values {
            self.seed.classes.push(value.into());
        }
        self
    }

    fn seed_constraints(&self) -> LayoutConstraints {
        self.seed.styles.layout
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

    fn layout_height(&self) -> Option<usize> {
        let constraints = self.seed_constraints();
        if let (Some(min), Some(max)) = (constraints.min_height, constraints.max_height) {
            if min == max {
                return Some(min);
            }
        }
        self.child.layout_height()
    }

    fn style(&self) -> Option<Style> {
        let s = self.seed.styles.style.clone();
        if s == Default::default() {
            None
        } else {
            Some(s)
        }
    }

    fn style_type(&self) -> &'static str {
        "Node"
    }

    fn set_inline_style(&mut self, style: crate::style::Style) {
        self.seed.styles.style = style;
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }

    fn style_classes(&self) -> &[String] {
        &self.seed.classes
    }

    fn style_id(&self) -> Option<&str> {
        self.seed.css_id.as_deref()
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
