use std::sync::atomic::{AtomicUsize, Ordering};

use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::compose::ComposeResult;
use crate::event::{Action, Event, EventCtx};
use crate::widgets::helpers::{
    adjust_line_length_no_bg, clamp_with_constraints, crop_line_horizontal, pad_lines_to_width,
};
use crate::widgets::{Container, Widget, WidgetStyles};

use super::ScrollCore;

pub struct HorizontalScroll {
    child: Container,
    children_extracted: bool,
    focused: bool,
    height: Option<usize>,
    offset_x: usize,
    scroll_step_x: usize,
    content_width: AtomicUsize,
    viewport_width: AtomicUsize,
    styles: WidgetStyles,
}

impl HorizontalScroll {
    pub fn new() -> Self {
        Self {
            child: Container::new(),
            children_extracted: false,
            focused: false,
            height: None,
            offset_x: 0,
            scroll_step_x: 1,
            content_width: AtomicUsize::new(0),
            viewport_width: AtomicUsize::new(0),
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

    pub fn scroll_by_x(&mut self, delta: i32) {
        if delta.is_negative() {
            self.offset_x = self.offset_x.saturating_sub(delta.unsigned_abs() as usize);
        } else {
            self.offset_x = self.offset_x.saturating_add(delta as usize);
        }
        self.clamp_offset();
    }

    pub fn scroll_step_x(mut self, step: usize) -> Self {
        self.scroll_step_x = step.max(1);
        self
    }

    pub fn set_virtual_content_size(&self, width: usize, _height: usize) {
        self.content_width
            .store(width.max(1), std::sync::atomic::Ordering::Relaxed);
    }

    fn is_tree_mode(&self) -> bool {
        self.children_extracted
    }

    fn max_offset(&self) -> usize {
        let content = self.content_width.load(Ordering::Relaxed);
        let viewport = self.viewport_width.load(Ordering::Relaxed).max(1);
        ScrollCore::max_offset(content, viewport)
    }

    fn clamp_offset(&mut self) {
        let max_x = self.max_offset();
        if self.offset_x > max_x {
            self.offset_x = max_x;
        }
    }
}

impl Default for HorizontalScroll {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for HorizontalScroll {
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
        if !self.is_tree_mode() {
            self.child.set_focus(focused);
        }
    }

    fn has_focus(&self) -> bool {
        self.focused
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let viewport_width = options.size.0.max(1);
        let viewport_height = self.height.unwrap_or_else(|| options.size.1.max(1)).max(1);
        self.viewport_width.store(viewport_width, Ordering::Relaxed);

        if self.is_tree_mode() {
            let content_w = self.content_width.load(Ordering::Relaxed);
            let show_h = content_w > viewport_width;
            const H_SCROLLBAR_SIZE: usize = 1;
            let content_viewport_h = viewport_height
                .saturating_sub(if show_h { H_SCROLLBAR_SIZE } else { 0 })
                .max(1);

            let mut slice: Vec<Vec<Segment>> = (0..content_viewport_h)
                .map(|_| vec![Segment::new(" ".repeat(viewport_width))])
                .collect();

            if show_h {
                let (track_style, thumb_style, _thumb_active_style) =
                    ScrollCore::scrollbar_styles();
                let offset = self.offset_x.min(self.max_offset());
                let (thumb_start, thumb_len) =
                    ScrollCore::thumb(viewport_width, content_w, viewport_width, offset);
                let mut row = Vec::new();
                for col in 0..viewport_width {
                    let style = if col >= thumb_start && col < thumb_start + thumb_len {
                        thumb_style
                    } else {
                        track_style
                    };
                    row.push(Segment::styled(" ".to_string(), style));
                }
                slice.push(row);
            }

            let slice =
                Segment::set_shape(&slice, viewport_width, Some(viewport_height), None, false);
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

        const H_SCROLLBAR_SIZE: usize = 1;
        let constraints = self.child.layout_constraints();
        let (track_style, thumb_style, _thumb_active_style) = ScrollCore::scrollbar_styles();

        let mut show_h = false;
        let mut content_viewport_h = viewport_height;
        let mut lines: Vec<Vec<Segment>> = Vec::new();
        let mut content_width = viewport_width;

        for _ in 0..2 {
            let viewport_h = viewport_height
                .saturating_sub(if show_h { H_SCROLLBAR_SIZE } else { 0 })
                .max(1);
            let target_width = self
                .child
                .content_width()
                .unwrap_or(viewport_width)
                .max(viewport_width);
            let render_width = clamp_with_constraints(
                target_width,
                constraints.min_width,
                constraints.max_width,
                target_width,
            )
            .max(viewport_width);
            let target_height = self.child.layout_height().unwrap_or(viewport_h).max(1);
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
            if let Some(height) = self.child.layout_height() {
                candidate =
                    Segment::set_shape(&candidate, render_width, Some(height.max(1)), None, false);
            }
            candidate = pad_lines_to_width(candidate, render_width);

            let candidate_width = candidate
                .iter()
                .map(|line| Segment::get_line_length(line))
                .max()
                .unwrap_or(viewport_width)
                .max(viewport_width);
            let next_show_h = candidate_width > viewport_width;
            lines = candidate;
            content_width = candidate_width;
            content_viewport_h = viewport_h;
            if next_show_h == show_h {
                break;
            }
            show_h = next_show_h;
        }

        self.content_width.store(content_width, Ordering::Relaxed);

        let max_offset = content_width.saturating_sub(viewport_width);
        let offset = self.offset_x.min(max_offset);
        let slice = lines
            .into_iter()
            .take(content_viewport_h)
            .map(|line| {
                let cropped = crop_line_horizontal(&line, offset, viewport_width);
                adjust_line_length_no_bg(&cropped, viewport_width)
            })
            .collect::<Vec<_>>();
        let mut slice = Segment::set_shape(
            &slice,
            viewport_width,
            Some(content_viewport_h),
            None,
            false,
        );

        if show_h {
            let (thumb_start, thumb_len) =
                ScrollCore::thumb(viewport_width, content_width, viewport_width, offset);
            let mut row = Vec::new();
            for col in 0..viewport_width {
                let style = if col >= thumb_start && col < thumb_start + thumb_len {
                    thumb_style
                } else {
                    track_style
                };
                row.push(Segment::styled(" ".to_string(), style));
            }
            slice.push(row);
        }

        let slice = Segment::set_shape(&slice, viewport_width, Some(viewport_height), None, false);

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
        if !self.is_tree_mode() {
            self.child.on_mount();
        }
    }

    fn on_unmount(&mut self) {
        if !self.is_tree_mode() {
            self.child.on_unmount();
        }
    }

    fn on_tick(&mut self, tick: u64) {
        if !self.is_tree_mode() {
            self.child.on_tick(tick);
        }
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        if !self.is_tree_mode() {
            self.child.on_resize(width, height);
        }
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        if !self.is_tree_mode() {
            self.child.on_layout(width, height);
        }
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        if !self.is_tree_mode() {
            self.child.on_event_capture(event, ctx);
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if let Event::Action(action) = event {
            match action {
                Action::ScrollHome => {
                    self.offset_x = 0;
                    ctx.set_handled();
                    return;
                }
                Action::ScrollEnd => {
                    self.offset_x = self.max_offset();
                    ctx.set_handled();
                    return;
                }
                Action::ScrollLeft => {
                    self.scroll_by_x(-(self.scroll_step_x as i32));
                    ctx.set_handled();
                    return;
                }
                Action::ScrollRight => {
                    self.scroll_by_x(self.scroll_step_x as i32);
                    ctx.set_handled();
                    return;
                }
                Action::ScrollPageLeft => {
                    let page = self.viewport_width.load(Ordering::Relaxed).max(1);
                    self.scroll_by_x(-(page as i32));
                    ctx.set_handled();
                    return;
                }
                Action::ScrollPageRight => {
                    let page = self.viewport_width.load(Ordering::Relaxed).max(1);
                    self.scroll_by_x(page as i32);
                    ctx.set_handled();
                    return;
                }
                _ => {}
            }
        }

        if self.is_tree_mode() {
            return;
        }
        self.child.on_event(event, ctx);
    }

    fn on_mouse_scroll(&mut self, delta_x: i32, delta_y: i32, ctx: &mut EventCtx) {
        let delta = if delta_x != 0 { delta_x } else { delta_y };
        if delta == 0 {
            return;
        }
        let before = self.offset_x;
        self.scroll_by_x(delta.saturating_mul(self.scroll_step_x as i32));
        if self.offset_x != before {
            ctx.request_repaint();
            ctx.set_handled();
        }
    }

    fn on_mouse_move(&mut self, x: u16, y: u16) -> bool {
        if self.is_tree_mode() {
            return false;
        }
        self.child.on_mouse_move(x, y)
    }

    fn scroll_offset(&self) -> (usize, usize) {
        (self.offset_x, 0)
    }

    fn clips_descendants_to_content(&self) -> bool {
        true
    }

    fn layout_height(&self) -> Option<usize> {
        self.height.or_else(|| {
            if self.is_tree_mode() {
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

impl Renderable for HorizontalScroll {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EventCtx;
    use crate::prelude::Label;
    use rich_rs::{Console, ConsoleOptions, Segment};

    #[test]
    fn hscroll_tree_mode_render_produces_output() {
        let mut hs = HorizontalScroll::new().with_child(Label::new("wide content here"));
        let children = hs.take_composed_children();
        assert!(!children.is_empty());
        assert!(hs.is_tree_mode());

        hs.content_width.store(100, Ordering::Relaxed);

        let console = Console::default();
        let mut opts = ConsoleOptions::default();
        opts.size = (20, 10);
        opts.max_width = 20;
        opts.max_height = 10;

        let segments = Widget::render(&hs, &console, &opts);
        let lines = Segment::split_and_crop_lines(segments, 20, None, true, false);
        assert_eq!(
            lines.len(),
            10,
            "tree-mode render must produce viewport-height lines"
        );
    }

    #[test]
    fn hscroll_scroll_offset_and_clip() {
        let mut hs = HorizontalScroll::new().with_child(Label::new("a"));
        hs.offset_x = 3;
        let _ = hs.take_composed_children();
        assert!(hs.is_tree_mode());

        assert_eq!(hs.scroll_offset(), (3, 0));
        assert!(hs.clips_descendants_to_content());
    }

    #[test]
    fn hscroll_tree_mode_scroll_actions() {
        let mut hs = HorizontalScroll::new().with_child(Label::new("a"));
        let _ = hs.take_composed_children();
        hs.content_width.store(100, Ordering::Relaxed);
        hs.viewport_width.store(20, Ordering::Relaxed);

        let mut ctx = EventCtx::default();
        hs.on_event(&Event::Action(Action::ScrollRight), &mut ctx);
        assert!(ctx.handled());
        assert!(hs.offset_x > 0, "ScrollRight should increase offset_x");

        let mut ctx2 = EventCtx::default();
        hs.on_event(&Event::Action(Action::ScrollHome), &mut ctx2);
        assert!(ctx2.handled());
        assert_eq!(hs.offset_x, 0, "ScrollHome should reset offset_x");
    }

    #[test]
    fn hscroll_take_composed_children_idempotent() {
        let mut hs = HorizontalScroll::new().with_child(Label::new("a"));
        let first = hs.take_composed_children();
        assert_eq!(first.len(), 1);
        let second = hs.take_composed_children();
        assert!(second.is_empty(), "second extraction must return empty");
    }
}
