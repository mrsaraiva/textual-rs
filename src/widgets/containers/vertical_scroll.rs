use crate::compose::ComposeResult;
use crate::widgets::{ScrollableContainer, Widget};

use crate::widgets::delegate::delegate_widget_to;

pub struct VerticalScroll {
    inner: ScrollableContainer,
}

impl VerticalScroll {
    crate::delegate_ident_methods!(inner);
    crate::delegate_border_title_methods!(inner);

    pub fn new() -> Self {
        // Overflow is NOT set inline here: Python `containers.py::VerticalScroll`
        // declares `overflow-x: hidden; overflow-y: auto` via DEFAULT_CSS (an
        // OVERRIDABLE default), mirrored in `css/defaults/containers.rs`. Setting it
        // inline would make it INLINE-specificity and override user CSS such as
        // `#right { overflow-y: hidden }`.
        let inner = ScrollableContainer::new();
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
        let _ = vs.compose();
        vs.set_virtual_content_size(40, 100);

        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            vs.on_message(
            &MessageEvent::new(
                NodeId::default(),
                ScrollbarScrollTo {
                    axis: ScrollbarAxis::Vertical,
                    offset: 7.0,
                    animate: false,
                    scroll_duration: None,
                },
            ),
            &mut __w);
        }

        assert_eq!(vs.scroll_offset().1, 7);
        assert!(ctx.handled());
    }
}
