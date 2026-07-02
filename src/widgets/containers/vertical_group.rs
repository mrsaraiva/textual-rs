use crate::compose::ComposeResult;
use crate::widgets::Widget;

use super::Vertical;

/// `VerticalGroup` is a thin, non-scrolling vertical container. Its full
/// `Widget` surface is delegated to the inner [`Vertical`] via the first-class
/// `#[widget(base = ..)]` derive (which replaces the deprecated
/// `delegate_widget_to!`). `style_type` intentionally keeps the trait default
/// so this widget matches `VerticalGroup { .. }` CSS, not `Vertical`.
#[textual::widget(base = Vertical, field = inner)]
pub struct VerticalGroup {
    inner: Vertical,
}

impl Default for VerticalGroup {
    fn default() -> Self {
        Self::new()
    }
}

impl VerticalGroup {
    crate::delegate_ident_methods!(inner);

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
