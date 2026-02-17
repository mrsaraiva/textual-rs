use std::sync::atomic::{AtomicUsize, Ordering};

use rich_rs::{Console, ConsoleOptions, Segment, Segments};

use crate::compose::ComposeResult;
use crate::event::{Action, Event, EventCtx, MouseDownEvent, MouseScrollEvent, MouseUpEvent};
use crate::node_id::NodeId;
use crate::widgets::helpers::{
    adjust_line_length_no_bg, clamp_with_constraints, crop_line_horizontal, pad_lines_to_width,
};
use crate::widgets::{Container, Widget, WidgetStyles};

use super::ScrollCore;

pub struct VerticalScroll {
    child: Container,
    children_extracted: bool,
    focused: bool,
    height: Option<usize>,
    offset_y: usize,
    scroll_step: usize,
    content_height: AtomicUsize,
    viewport_width: AtomicUsize,
    viewport_height: AtomicUsize,
    styles: WidgetStyles,
}

impl VerticalScroll {
    pub fn new() -> Self {
        Self {
            child: Container::new(),
            children_extracted: false,
            focused: false,
            height: None,
            offset_y: 0,
            scroll_step: 1,
            content_height: AtomicUsize::new(0),
            viewport_width: AtomicUsize::new(1),
            viewport_height: AtomicUsize::new(0),
            styles: WidgetStyles::default(),
        }
    }

    pub fn with_child(mut self, child: impl Widget + 'static) -> Self {
        self.child.push(child);
        self
    }

    pub fn with_compose(mut self, children: ComposeResult) -> Self {
        self.child = self.child.with_compose(children);
        self
    }

    pub fn push(&mut self, child: impl Widget + 'static) {
        self.child.push(child);
    }

    pub fn height(mut self, height: usize) -> Self {
        self.height = Some(height.max(1));
        self
    }

    pub fn scroll_by(&mut self, delta: i32) {
        if delta.is_negative() {
            self.offset_y = self.offset_y.saturating_sub(delta.unsigned_abs() as usize);
        } else {
            self.offset_y = self.offset_y.saturating_add(delta as usize);
        }
        self.clamp_offset();
    }

    pub fn scroll_step(mut self, step: usize) -> Self {
        self.scroll_step = step.max(1);
        self
    }

    pub fn set_virtual_content_size(&self, _width: usize, height: usize) {
        self.content_height
            .store(height.max(1), std::sync::atomic::Ordering::Relaxed);
    }

    fn max_offset(&self) -> usize {
        let content = self.content_height.load(Ordering::Relaxed);
        let viewport = self.viewport_height.load(Ordering::Relaxed).max(1);
        ScrollCore::max_offset(content, viewport)
    }

    fn clamp_offset(&mut self) {
        let max_y = self.max_offset();
        if self.offset_y > max_y {
            self.offset_y = max_y;
        }
    }

    fn child_coords(&self, x: u16, y: u16) -> (u16, u16) {
        (x, y.saturating_add(self.offset_y as u16))
    }

    fn sync_child_layout(&mut self) {
        if self.children_extracted {
            return;
        }
        let width = self.viewport_width.load(Ordering::Relaxed).max(1) as u16;
        let height = self.viewport_height.load(Ordering::Relaxed).max(1) as u16;
        self.child.on_layout(width, height);
    }
}

impl Default for VerticalScroll {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for VerticalScroll {
    fn compose(&self) -> ComposeResult {
        self.child.compose()
    }

    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        if self.children_extracted {
            return Vec::new();
        }
        self.children_extracted = true;
        self.child.take_composed_children()
    }

    fn focusable(&self) -> bool {
        true
    }

    fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
        if !self.children_extracted {
            self.child.set_focus(focused);
        }
    }

    fn has_focus(&self) -> bool {
        self.focused
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let viewport_height = self.height.unwrap_or_else(|| options.size.1.max(1)).max(1);
        self.viewport_width.store(width, Ordering::Relaxed);
        self.viewport_height
            .store(viewport_height, Ordering::Relaxed);

        if self.children_extracted {
            let content_h = self.content_height.load(Ordering::Relaxed);
            let show_v = content_h > viewport_height;
            const V_SCROLLBAR_SIZE: usize = 2;
            let content_viewport_w = width
                .saturating_sub(if show_v {
                    V_SCROLLBAR_SIZE.min(width.saturating_sub(1))
                } else {
                    0
                })
                .max(1);

            let mut slice: Vec<Vec<Segment>> = (0..viewport_height)
                .map(|_| vec![Segment::new(" ".repeat(content_viewport_w))])
                .collect();

            if show_v {
                let (track_style, thumb_style, _thumb_active_style) =
                    ScrollCore::scrollbar_styles();
                let track_len = viewport_height.max(1);
                let offset = self.offset_y.min(self.max_offset());
                let (thumb_start, thumb_len) =
                    ScrollCore::thumb(track_len, content_h, viewport_height, offset);
                let bar_width = width.saturating_sub(content_viewport_w).max(1);
                for (row, line) in slice.iter_mut().enumerate() {
                    let style =
                        if row < track_len && row >= thumb_start && row < thumb_start + thumb_len {
                            thumb_style
                        } else {
                            track_style
                        };
                    for _ in 0..bar_width {
                        line.push(Segment::styled(" ".to_string(), style));
                    }
                }
            }

            let slice = Segment::set_shape(&slice, width, Some(viewport_height), None, false);
            let line_count = slice.len();
            let mut out = Segments::new();
            for (idx, line) in slice.into_iter().enumerate() {
                out.extend(line);
                if idx + 1 < line_count {
                    out.push(Segment::line());
                }
            }
            return out;
        }

        let constraints = self.child.layout_constraints();
        const V_SCROLLBAR_SIZE: usize = 2;
        let child_layout_height = self.child.layout_height();
        let (track_style, thumb_style, _thumb_active_style) = ScrollCore::scrollbar_styles();

        let mut show_v = false;
        let mut content_viewport_w = width;
        let mut lines: Vec<Vec<Segment>> = Vec::new();
        let mut content_height = viewport_height;

        for _ in 0..2 {
            let viewport_w = width
                .saturating_sub(if show_v {
                    V_SCROLLBAR_SIZE.min(width.saturating_sub(1))
                } else {
                    0
                })
                .max(1);
            let target_height = child_layout_height
                .unwrap_or_else(|| viewport_height.saturating_add(viewport_height).max(1));
            let render_width = clamp_with_constraints(
                viewport_w,
                constraints.min_width,
                constraints.max_width,
                viewport_w,
            )
            .max(viewport_w);
            let render_height = clamp_with_constraints(
                target_height,
                constraints.min_height,
                constraints.max_height,
                target_height,
            );
            let mut child_options = options.clone();
            child_options.size = (render_width, render_height);
            child_options.max_width = render_width;
            child_options.max_height = render_height;

            let segments = self.child.render_styled(console, &child_options);
            let mut candidate =
                Segment::split_and_crop_lines(segments, render_width, None, true, false);
            let raw_lines_height = candidate.len();
            if let Some(height) = child_layout_height {
                let effective_height = height.max(raw_lines_height).max(1);
                candidate = Segment::set_shape(
                    &candidate,
                    render_width,
                    Some(effective_height),
                    None,
                    false,
                );
            }
            candidate = pad_lines_to_width(candidate, render_width);

            let candidate_height = candidate.len().max(viewport_height);
            let next_show_v = candidate_height > viewport_height;
            lines = candidate;
            content_height = candidate_height;
            content_viewport_w = viewport_w;
            if next_show_v == show_v {
                break;
            }
            show_v = next_show_v;
        }

        self.content_height.store(content_height, Ordering::Relaxed);

        let max_offset = content_height.saturating_sub(viewport_height);
        let offset = self.offset_y.min(max_offset);
        let start = offset.min(lines.len());
        let end = (start + viewport_height).min(lines.len());
        let mut slice = lines[start..end]
            .to_vec()
            .into_iter()
            .map(|line| {
                let cropped = crop_line_horizontal(&line, 0, content_viewport_w);
                adjust_line_length_no_bg(&cropped, content_viewport_w)
            })
            .collect::<Vec<_>>();
        slice = Segment::set_shape(
            &slice,
            content_viewport_w,
            Some(viewport_height),
            None,
            false,
        );

        if show_v {
            let track_len = viewport_height.max(1);
            let (thumb_start, thumb_len) =
                ScrollCore::thumb(track_len, content_height, viewport_height, offset);
            let bar_width = width.saturating_sub(content_viewport_w).max(1);
            for (row, line) in slice.iter_mut().enumerate() {
                let style =
                    if row < track_len && row >= thumb_start && row < thumb_start + thumb_len {
                        thumb_style
                    } else {
                        track_style
                    };
                for _ in 0..bar_width {
                    line.push(Segment::styled(" ".to_string(), style));
                }
            }
        }

        let slice = Segment::set_shape(&slice, width, Some(viewport_height), None, false);

        let line_count = slice.len();
        let mut out = Segments::new();
        for (idx, line) in slice.into_iter().enumerate() {
            out.extend(line);
            if idx + 1 < line_count {
                out.push(Segment::line());
            }
        }
        out
    }

    fn on_mount(&mut self) {
        if !self.children_extracted {
            self.child.on_mount();
        }
    }

    fn on_unmount(&mut self) {
        if !self.children_extracted {
            self.child.on_unmount();
        }
    }

    fn on_tick(&mut self, tick: u64) {
        if !self.children_extracted {
            self.child.on_tick(tick);
        }
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        self.viewport_width.store(width as usize, Ordering::Relaxed);
        self.viewport_height
            .store(height as usize, Ordering::Relaxed);
        if !self.children_extracted {
            self.child.on_resize(width, height);
        }
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.viewport_width.store(width as usize, Ordering::Relaxed);
        self.viewport_height
            .store(height as usize, Ordering::Relaxed);
        if !self.children_extracted {
            self.child.on_layout(width, height);
        }
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        if !self.children_extracted {
            self.child.on_event_capture(event, ctx);
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if !self.children_extracted {
            self.sync_child_layout();
        }

        if let Event::Action(action) = event {
            match action {
                Action::ScrollHome => {
                    self.offset_y = 0;
                    ctx.set_handled();
                    return;
                }
                Action::ScrollEnd => {
                    self.offset_y = self.max_offset();
                    ctx.set_handled();
                    return;
                }
                Action::ScrollUp => {
                    self.scroll_by(-(self.scroll_step as i32));
                    ctx.set_handled();
                    return;
                }
                Action::ScrollDown => {
                    self.scroll_by(self.scroll_step as i32);
                    ctx.set_handled();
                    return;
                }
                Action::ScrollPageUp => {
                    let page = self.height.unwrap_or(1).max(1);
                    self.scroll_by(-(page as i32));
                    ctx.set_handled();
                    return;
                }
                Action::ScrollPageDown => {
                    let page = self.height.unwrap_or(1).max(1);
                    self.scroll_by(page as i32);
                    ctx.set_handled();
                    return;
                }
                _ => {}
            }
        }

        if self.children_extracted {
            return;
        }

        let child_event = match event {
            Event::MouseDown(mouse) => {
                let (child_x, child_y) = self.child_coords(mouse.x, mouse.y);
                Some(Event::MouseDown(MouseDownEvent {
                    target: NodeId::default(),
                    screen_x: mouse.screen_x,
                    screen_y: mouse.screen_y,
                    x: child_x,
                    y: child_y,
                }))
            }
            Event::MouseUp(mouse) => {
                let (child_x, child_y) = self.child_coords(mouse.x, mouse.y);
                Some(Event::MouseUp(MouseUpEvent {
                    target: Some(NodeId::default()),
                    screen_x: mouse.screen_x,
                    screen_y: mouse.screen_y,
                    x: child_x,
                    y: child_y,
                }))
            }
            Event::MouseScroll(mouse) => {
                let (child_x, child_y) = self.child_coords(mouse.x, mouse.y);
                Some(Event::MouseScroll(MouseScrollEvent {
                    target: Some(NodeId::default()),
                    screen_x: mouse.screen_x,
                    screen_y: mouse.screen_y,
                    x: child_x,
                    y: child_y,
                    delta_x: mouse.delta_x,
                    delta_y: mouse.delta_y,
                    modifiers: mouse.modifiers,
                }))
            }
            _ => None,
        };
        if let Some(child_event) = child_event.as_ref() {
            self.child.on_event(child_event, ctx);
        } else {
            self.child.on_event(event, ctx);
        }
    }

    fn on_mouse_scroll(&mut self, _delta_x: i32, delta_y: i32, ctx: &mut EventCtx) {
        if delta_y == 0 {
            return;
        }
        let before = self.offset_y;
        self.scroll_by(delta_y.saturating_mul(self.scroll_step as i32));
        if self.offset_y != before {
            ctx.request_repaint();
            ctx.set_handled();
        }
    }

    fn on_mouse_move(&mut self, x: u16, y: u16) -> bool {
        if self.children_extracted {
            return false;
        }
        self.sync_child_layout();
        let (child_x, child_y) = self.child_coords(x, y);
        self.child.on_mouse_move(child_x, child_y)
    }

    fn scroll_offset(&self) -> (usize, usize) {
        (0, self.offset_y)
    }

    fn clips_descendants_to_content(&self) -> bool {
        true
    }

    fn layout_height(&self) -> Option<usize> {
        self.height.or_else(|| {
            if self.children_extracted {
                None
            } else {
                self.child.layout_height()
            }
        })
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EventCtx;
    use crate::prelude::Label;
    use rich_rs::{Console, ConsoleOptions, Segment};

    #[test]
    fn vscroll_tree_mode_render_produces_output() {
        let mut vs = VerticalScroll::new()
            .with_child(Label::new("a"))
            .with_child(Label::new("b"));
        let children = vs.take_composed_children();
        assert!(children.len() >= 2);
        assert!(vs.children_extracted);

        vs.content_height.store(100, Ordering::Relaxed);

        let console = Console::default();
        let mut opts = ConsoleOptions::default();
        opts.size = (20, 10);
        opts.max_width = 20;
        opts.max_height = 10;

        let segments = Widget::render(&vs, &console, &opts);
        let lines = Segment::split_and_crop_lines(segments, 20, None, true, false);
        assert_eq!(
            lines.len(),
            10,
            "tree-mode render must produce viewport-height lines"
        );
        let has_styled = lines.iter().any(|line| line.len() > 1);
        assert!(
            has_styled,
            "tree-mode render should include scrollbar chrome"
        );
    }

    #[test]
    fn vscroll_scroll_offset_and_clip() {
        let mut vs = VerticalScroll::new().with_child(Label::new("a"));
        vs.offset_y = 7;
        let _ = vs.take_composed_children();
        assert!(vs.children_extracted);

        assert_eq!(vs.scroll_offset(), (0, 7));
        assert!(vs.clips_descendants_to_content());
    }

    #[test]
    fn vscroll_tree_mode_scroll_actions() {
        let mut vs = VerticalScroll::new().with_child(Label::new("a"));
        let _ = vs.take_composed_children();
        vs.content_height.store(100, Ordering::Relaxed);
        vs.viewport_height.store(10, Ordering::Relaxed);

        let mut ctx = EventCtx::default();
        vs.on_event(&Event::Action(Action::ScrollDown), &mut ctx);
        assert!(ctx.handled());
        assert!(vs.offset_y > 0, "ScrollDown should increase offset_y");

        let mut ctx2 = EventCtx::default();
        vs.on_event(&Event::Action(Action::ScrollHome), &mut ctx2);
        assert!(ctx2.handled());
        assert_eq!(vs.offset_y, 0, "ScrollHome should reset offset_y");
    }

    #[test]
    fn vscroll_take_composed_children_idempotent() {
        let mut vs = VerticalScroll::new().with_child(Label::new("a"));
        let first = vs.take_composed_children();
        assert_eq!(first.len(), 1);
        let second = vs.take_composed_children();
        assert!(second.is_empty(), "second extraction must return empty");
    }
}
