use crate::compose::ComposeResult;
use crate::widgets::Widget;

use super::Vertical;
use crate::widgets::delegate::delegate_widget_to;

pub struct VerticalGroup {
    inner: Vertical,
}

impl VerticalGroup {
    pub fn new() -> Self {
        Self {
            inner: Vertical::new(),
        }
    }

    pub fn with_child(mut self, child: impl Widget + 'static) -> Self {
        self.inner = self.inner.with_child(child);
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

delegate_widget_to!(VerticalGroup, inner);
