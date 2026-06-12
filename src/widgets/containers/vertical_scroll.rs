use crate::compose::ComposeResult;
use crate::style::Overflow;
use crate::widgets::{ScrollableContainer, Widget};

use crate::widgets::delegate::delegate_widget_to;

pub struct VerticalScroll {
    inner: ScrollableContainer,
}

impl VerticalScroll {
    pub fn new() -> Self {
        let mut inner = ScrollableContainer::new();
        if let Some(styles) = inner.styles_mut() {
            styles.style.overflow_x = Some(Overflow::Hidden);
            styles.style.overflow_y = Some(Overflow::Auto);
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

    pub fn scroll_by(&mut self, delta: i32) {
        self.inner.scroll_by(delta);
    }

    pub fn scroll_to(&mut self, offset_y: usize) {
        self.inner.scroll_to(offset_y);
    }

    pub fn scroll_step(mut self, step: usize) -> Self {
        self.inner = self.inner.scroll_step(step);
        self
    }

    pub fn set_scroll_step(&mut self, step: usize) {
        self.inner.set_scroll_step(step);
    }

    pub fn set_virtual_content_size(&self, width: usize, height: usize) {
        self.inner.set_virtual_content_size(width, height);
    }

    pub fn scroll_home(&mut self) {
        self.inner.scroll_home();
    }
}

impl Default for VerticalScroll {
    fn default() -> Self {
        Self::new()
    }
}

delegate_widget_to!(VerticalScroll, inner);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EventCtx;
    use crate::message::{MessageEvent, ScrollbarAxis, ScrollbarScrollTo};
    use crate::node_id::NodeId;
    use crate::prelude::Label;

    #[test]
    fn forwards_scrollbar_messages_to_inner_scrollable_container() {
        let mut vs = VerticalScroll::new().with_child(Label::new("line\n".repeat(40)));
        let _ = vs.take_composed_children();
        vs.set_virtual_content_size(40, 100);

        let mut ctx = EventCtx::default();
        vs.on_message(
            &MessageEvent::new(NodeId::default(), ScrollbarScrollTo {
                axis: ScrollbarAxis::Vertical,
                offset: 7.0,
                animate: false,
                scroll_duration: None,
            }),
            &mut ctx,
        );

        assert_eq!(vs.scroll_offset().1, 7);
        assert!(ctx.handled());
    }
}
