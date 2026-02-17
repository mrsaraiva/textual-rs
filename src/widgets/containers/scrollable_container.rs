use rich_rs::{Console, ConsoleOptions, Segments};

use crate::compose::ComposeResult;
use crate::event::{Event, EventCtx};
use crate::widgets::{Widget, WidgetStyles};

use super::VerticalScroll;

pub struct ScrollableContainer {
    inner: VerticalScroll,
}

impl ScrollableContainer {
    pub fn new() -> Self {
        Self {
            inner: VerticalScroll::new(),
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

    pub fn height(mut self, height: usize) -> Self {
        self.inner = self.inner.height(height);
        self
    }

    pub fn scroll_step(mut self, step: usize) -> Self {
        self.inner = self.inner.scroll_step(step);
        self
    }
}

impl Default for ScrollableContainer {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for ScrollableContainer {
    fn compose(&self) -> ComposeResult {
        self.inner.compose()
    }

    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        self.inner.take_composed_children()
    }

    fn focusable(&self) -> bool {
        self.inner.focusable()
    }

    fn set_focus(&mut self, focused: bool) {
        self.inner.set_focus(focused);
    }

    fn has_focus(&self) -> bool {
        self.inner.has_focus()
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        self.inner.render(console, options)
    }

    fn on_mount(&mut self) {
        self.inner.on_mount();
    }

    fn on_unmount(&mut self) {
        self.inner.on_unmount();
    }

    fn on_tick(&mut self, tick: u64) {
        self.inner.on_tick(tick);
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        self.inner.on_resize(width, height);
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.inner.on_layout(width, height);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.inner.on_event_capture(event, ctx);
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.inner.on_event(event, ctx);
    }

    fn on_mouse_scroll(&mut self, delta_x: i32, delta_y: i32, ctx: &mut EventCtx) {
        self.inner.on_mouse_scroll(delta_x, delta_y, ctx);
    }

    fn on_mouse_move(&mut self, x: u16, y: u16) -> bool {
        self.inner.on_mouse_move(x, y)
    }

    fn scroll_offset(&self) -> (usize, usize) {
        self.inner.scroll_offset()
    }

    fn clips_descendants_to_content(&self) -> bool {
        self.inner.clips_descendants_to_content()
    }

    fn layout_height(&self) -> Option<usize> {
        self.inner.layout_height()
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        self.inner.styles()
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        self.inner.styles_mut()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::Label;

    #[test]
    fn scrollable_container_forwards_scroll_offset() {
        let mut sc = ScrollableContainer::new().with_child(Label::new("a"));
        let _ = sc.take_composed_children();
        assert_eq!(sc.scroll_offset(), (0, 0));
        assert!(sc.clips_descendants_to_content());
    }
}
