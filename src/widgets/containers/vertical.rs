use crate::compose::ComposeResult;
use crate::widgets::Widget;

use super::Container;
use super::thin::delegate_widget_to;

pub struct Vertical {
    container: Container,
}

impl Vertical {
    pub fn new() -> Self {
        Self {
            container: Container::new(),
        }
    }

    pub fn with_child(mut self, child: impl Widget + 'static) -> Self {
        self.container = self.container.with_child(child);
        self
    }

    pub fn with_compose(mut self, children: ComposeResult) -> Self {
        self.container = self.container.with_compose(children);
        self
    }

    pub fn push(&mut self, child: impl Widget + 'static) {
        self.container.push(child);
    }
}

delegate_widget_to!(Vertical, container);
