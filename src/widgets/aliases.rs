use rich_rs::{Console, ConsoleOptions, Segments};

use crate::widgets::{Label, Node, Widget, WidgetStyles};

pub struct Static {
    label: Label,
}

impl Static {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            label: Label::new(text),
        }
    }

    pub fn class(self, value: impl Into<String>) -> Node {
        Node::new(self).class(value)
    }

    pub fn id(self, value: impl Into<String>) -> Node {
        Node::new(self).id(value)
    }
}

impl Widget for Static {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        self.label.render(console, options)
    }

    fn layout_height(&self) -> Option<usize> {
        self.label.layout_height()
    }

    fn content_width(&self) -> Option<usize> {
        self.label.content_width()
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        self.label.styles()
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        self.label.styles_mut()
    }
}
