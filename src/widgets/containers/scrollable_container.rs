use rich_rs::{Console, ConsoleOptions, Segments};

use crate::compose::ComposeResult;
use crate::event::{Event, EventCtx};
use crate::widgets::{BindingDecl, Container, Widget, WidgetStyles};

use super::ScrollView;

pub struct ScrollableContainer {
    inner: ScrollView,
    can_focus: bool,
    can_focus_children: bool,
    can_maximize: Option<bool>,
}

impl ScrollableContainer {
    pub fn new() -> Self {
        Self {
            inner: ScrollView::new(Container::new()),
            can_focus: true,
            can_focus_children: true,
            // Python default for ScrollableContainer.
            can_maximize: Some(false),
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

    pub fn scroll_step_x(mut self, step: usize) -> Self {
        self.inner = self.inner.scroll_step_x(step);
        self
    }

    pub fn set_scroll_step(&mut self, step: usize) {
        self.inner.set_scroll_step(step);
    }

    pub fn set_scroll_step_x(&mut self, step: usize) {
        self.inner.set_scroll_step_x(step);
    }

    pub fn scroll_by(&mut self, delta: i32) {
        self.inner.scroll_by(delta);
    }

    pub fn scroll_by_x(&mut self, delta: i32) {
        self.inner.scroll_by_x(delta);
    }

    pub fn set_virtual_content_size(&self, width: usize, height: usize) {
        self.inner.set_virtual_content_size(width, height);
    }

    pub fn scroll_to(&mut self, offset_y: usize) {
        self.inner.scroll_to(offset_y);
    }

    pub fn scroll_home(&mut self) {
        self.inner.scroll_home();
    }

    pub fn with_can_focus(mut self, can_focus: bool) -> Self {
        self.can_focus = can_focus;
        self
    }

    pub fn with_can_focus_children(mut self, can_focus_children: bool) -> Self {
        self.can_focus_children = can_focus_children;
        self
    }

    pub fn with_can_maximize(mut self, can_maximize: Option<bool>) -> Self {
        self.can_maximize = can_maximize;
        self
    }

    pub fn can_maximize(&self) -> bool {
        self.can_maximize.unwrap_or(self.can_focus)
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
        let extracted = self.inner.take_composed_children();
        let mut out = Vec::new();
        let mut flattened_container = false;

        for mut child in extracted {
            let is_scrollbar_lane = matches!(
                child.style_id(),
                Some(
                    super::SCROLL_VIEW_VSCROLLBAR_ID
                        | super::SCROLL_VIEW_HSCROLLBAR_ID
                        | super::SCROLL_VIEW_SCROLLBAR_CORNER_ID
                )
            );
            if !flattened_container && !is_scrollbar_lane {
                let any = &mut *child as &mut dyn std::any::Any;
                if let Some(container) = any.downcast_mut::<Container>() {
                    out.extend(container.take_composed_children());
                    flattened_container = true;
                    continue;
                }
            }
            out.push(child);
        }

        out
    }

    fn focusable(&self) -> bool {
        self.can_focus
    }

    fn can_focus(&self) -> bool {
        self.can_focus
    }

    fn can_focus_children(&self) -> bool {
        self.can_focus_children
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

    fn render_with_debug(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        debug: &crate::debug::DebugLayout,
    ) -> Segments {
        self.inner.render_with_debug(console, options, debug)
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

    fn set_virtual_content_size(&mut self, width: usize, height: usize) {
        ScrollableContainer::set_virtual_content_size(self, width, height);
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

    fn scroll_viewport_size(&self) -> Option<(usize, usize)> {
        self.inner.scroll_viewport_size()
    }

    fn layout_height(&self) -> Option<usize> {
        self.inner.layout_height()
    }

    fn content_width(&self) -> Option<usize> {
        self.inner.content_width()
    }

    fn bindings(&self) -> Vec<crate::widgets::BindingDecl> {
        let mut bindings = self.inner.bindings();
        bindings.push(BindingDecl::new("ctrl+pageup", "page_left", "Page left").hidden());
        bindings.push(BindingDecl::new("ctrl+pagedown", "page_right", "Page right").hidden());
        bindings
    }

    fn execute_action(&mut self, action: &crate::action::ParsedAction, ctx: &mut EventCtx) -> bool {
        match action.name.as_str() {
            "page_left" => {
                let before = self.inner.offset_x();
                let page = self.inner.layout_height().unwrap_or(1).max(1);
                self.inner.scroll_by_x(-(page as i32));
                if self.inner.offset_x() != before {
                    ctx.request_repaint();
                }
                ctx.set_handled();
                true
            }
            "page_right" => {
                let before = self.inner.offset_x();
                let page = self.inner.layout_height().unwrap_or(1).max(1);
                self.inner.scroll_by_x(page as i32);
                if self.inner.offset_x() != before {
                    ctx.request_repaint();
                }
                ctx.set_handled();
                true
            }
            _ => self.inner.execute_action(action, ctx),
        }
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
    fn scrollable_container_defaults_match_python_policies() {
        let sc = ScrollableContainer::new();
        assert!(sc.focusable());
        assert!(sc.can_focus_children());
        assert!(!sc.can_maximize());
    }

    #[test]
    fn scrollable_container_forwards_scroll_offset() {
        let mut sc = ScrollableContainer::new().with_child(Label::new("a"));
        let _ = sc.take_composed_children();
        assert_eq!(sc.scroll_offset(), (0, 0));
        assert!(sc.clips_descendants_to_content());
    }
}
