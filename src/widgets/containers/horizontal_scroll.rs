use crate::compose::ComposeResult;
use crate::style::Overflow;
use crate::widgets::{ScrollableContainer, Widget};

use super::thin::delegate_widget_to;

pub struct HorizontalScroll {
    inner: ScrollableContainer,
}

impl HorizontalScroll {
    pub fn new() -> Self {
        let mut inner = ScrollableContainer::new();
        if let Some(styles) = inner.styles_mut() {
            styles.style.overflow_x = Some(Overflow::Auto);
            styles.style.overflow_y = Some(Overflow::Hidden);
        }
        Self { inner }
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

    pub fn height(mut self, height: usize) -> Self {
        self.inner = self.inner.height(height);
        self
    }

    pub fn scroll_by_x(&mut self, delta: i32) {
        self.inner.scroll_by_x(delta);
    }

    pub fn scroll_step_x(mut self, step: usize) -> Self {
        self.inner = self.inner.scroll_step_x(step);
        self
    }

    pub fn set_virtual_content_size(&self, width: usize, height: usize) {
        self.inner.set_virtual_content_size(width, height);
    }
}

impl Default for HorizontalScroll {
    fn default() -> Self {
        Self::new()
    }
}

delegate_widget_to!(HorizontalScroll, inner);
