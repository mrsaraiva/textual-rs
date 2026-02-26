use crate::compose::ComposeResult;
use crate::widgets::{Container, Widget};

use crate::widgets::delegate::delegate_widget_to;

pub struct Right {
    inner: Container,
}

impl Right {
    pub fn new() -> Self {
        Self {
            inner: Container::new(),
        }
    }

    pub fn with_child(mut self, child: impl Widget + 'static) -> Self {
        self.inner.push(child);
        self
    }

    pub fn with_compose(mut self, children: ComposeResult) -> Self {
        self.inner = self.inner.with_compose(children);
        self
    }

    pub fn push(&mut self, child: impl Widget + 'static) {
        self.inner.push(child);
    }
}

delegate_widget_to!(Right, inner);
