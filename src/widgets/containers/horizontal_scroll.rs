use crate::compose::ComposeResult;
use crate::widgets::{ScrollableContainer, Widget};
use textual_macros::widget;

// `style_type` keeps the trait default ("HorizontalScroll"); the alias mirrors
// Python's MRO — `HorizontalScroll(ScrollableContainer)` — so this node matches
// BOTH `HorizontalScroll` and `ScrollableContainer` type selectors (and thereby
// inherits the `ScrollableContainer { width: 1fr; height: 1fr; ... }` defaults,
// exactly as Python's DEFAULT_CSS inheritance does).
#[widget(base = ScrollableContainer, field = inner, override(style_type_aliases))]
pub struct HorizontalScroll {
    inner: ScrollableContainer,
}

impl HorizontalScroll {
    fn style_type_aliases(&self) -> &[&'static str] {
        &["ScrollableContainer"]
    }

    crate::delegate_ident_methods!(inner);
    crate::delegate_border_title_methods!(inner);

    pub fn new() -> Self {
        // Overflow is NOT set inline here: Python `containers.py::HorizontalScroll`
        // declares `overflow-x: auto; overflow-y: hidden` via DEFAULT_CSS (an
        // OVERRIDABLE default), mirrored in `css/defaults/containers.rs`. Setting it
        // inline would make it INLINE-specificity and override user CSS.
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
