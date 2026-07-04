use crate::compose::ComposeResult;
use crate::widgets::Widget;

use super::Horizontal;
use crate::widgets::delegate::delegate_widget_to;

pub struct HorizontalGroup {
    inner: Horizontal,
}

impl Default for HorizontalGroup {
    fn default() -> Self {
        Self::new()
    }
}

impl HorizontalGroup {
    crate::delegate_ident_methods!(inner);
    crate::delegate_border_title_methods!(inner);

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
