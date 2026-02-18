use crate::compose::ComposeResult;
use crate::widgets::Widget;

use super::Horizontal;
use super::thin::delegate_widget_to;

pub struct HorizontalGroup {
    inner: Horizontal,
}

impl HorizontalGroup {
    pub fn new() -> Self {
        Self {
            inner: Horizontal::new(),
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

delegate_widget_to!(HorizontalGroup, inner);
