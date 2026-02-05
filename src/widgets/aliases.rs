use std::sync::atomic::{AtomicUsize, Ordering};

use rich_rs::{Console, ConsoleOptions, Segment, Segments};

use crate::event::{Action, Event, EventCtx};

use super::helpers::adjust_line_length_no_bg;
use super::helpers::{clamp_with_constraints, crop_line_horizontal, pad_lines_to_width};
use super::{Container, Row, RowAlign, Widget, WidgetId, WidgetStyles};

pub struct Horizontal {
    row: Row,
}

impl Horizontal {
    pub fn new() -> Self {
        Self { row: Row::new() }
    }

    pub fn with_child(mut self, child: impl Widget + 'static) -> Self {
        self.row = self.row.with_child(child);
        self
    }

    pub fn push(&mut self, child: impl Widget + 'static) {
        self.row.push(child);
    }

    pub fn align(mut self, align: RowAlign) -> Self {
        self.row = self.row.align(align);
        self
    }
}

impl Widget for Horizontal {
    fn id(&self) -> WidgetId {
        self.row.id()
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        self.row.render(console, options)
    }

    fn render_with_debug(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        debug: &crate::debug::DebugLayout,
    ) -> Segments {
        self.row.render_with_debug(console, options, debug)
    }

    fn on_mount(&mut self) {
        self.row.on_mount();
    }

    fn on_unmount(&mut self) {
        self.row.on_unmount();
    }

    fn on_tick(&mut self, tick: u64) {
        self.row.on_tick(tick);
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        self.row.on_resize(width, height);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.row.on_event_capture(event, ctx);
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.row.on_event(event, ctx);
    }

    fn visit_children_mut(&mut self, f: &mut dyn FnMut(&mut dyn Widget)) {
        self.row.visit_children_mut(f);
    }

    fn layout_height(&self) -> Option<usize> {
        self.row.layout_height()
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        self.row.styles()
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        self.row.styles_mut()
    }
}

pub struct Static {
    label: super::Label,
}

impl Static {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            label: super::Label::new(text),
        }
    }
}

impl Widget for Static {
    fn id(&self) -> WidgetId {
        self.label.id()
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        self.label.render(console, options)
    }

    fn layout_height(&self) -> Option<usize> {
        self.label.layout_height()
    }

    fn content_width(&self) -> Option<usize> {
        self.label.content_width()
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        self.label.styles()
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        self.label.styles_mut()
    }
}

pub struct VerticalScroll {
    id: WidgetId,
    child: Container,
    height: Option<usize>,
    offset_y: usize,
    scroll_step: usize,
    content_height: AtomicUsize,
    viewport_height: AtomicUsize,
    styles: WidgetStyles,
}

impl VerticalScroll {
    pub fn new() -> Self {
        Self {
            id: WidgetId::new(),
            child: Container::new(),
            height: None,
            offset_y: 0,
            scroll_step: 1,
            content_height: AtomicUsize::new(0),
            viewport_height: AtomicUsize::new(0),
            styles: WidgetStyles::default(),
        }
    }

    pub fn with_child(mut self, child: impl Widget + 'static) -> Self {
        self.child.push(child);
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

    fn max_offset(&self) -> usize {
        let content = self.content_height.load(Ordering::Relaxed);
        let viewport = self.viewport_height.load(Ordering::Relaxed).max(1);
        content.saturating_sub(viewport)
    }

    fn clamp_offset(&mut self) {
        let max_y = self.max_offset();
        if self.offset_y > max_y {
            self.offset_y = max_y;
        }
    }
}

impl Widget for VerticalScroll {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let viewport_height = self.height.unwrap_or_else(|| options.size.1.max(1));
        self.viewport_height
            .store(viewport_height, Ordering::Relaxed);

        let constraints = self.child.layout_constraints();
        let target_height = self.child.layout_height().unwrap_or_else(|| {
            // For children without an intrinsic height, probe at least one extra viewport
            // so scrolling can start from offset 0, without letting probe height
            // grow with scroll offset.
            viewport_height.saturating_add(viewport_height).max(1)
        });
        let render_width =
            clamp_with_constraints(width, constraints.min_width, constraints.max_width, width);
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
        let mut lines = Segment::split_and_crop_lines(segments, render_width, None, true, false);
        if let Some(height) = self.child.layout_height() {
            lines = Segment::set_shape(&lines, render_width, Some(height.max(1)), None, false);
        }
        lines = pad_lines_to_width(lines, width);

        let content_height = lines.len().max(viewport_height);
        self.content_height.store(content_height, Ordering::Relaxed);

        let max_offset = content_height.saturating_sub(viewport_height);
        let offset = self.offset_y.min(max_offset);
        let start = offset.min(lines.len());
        let end = (start + viewport_height).min(lines.len());
        let slice = lines[start..end]
            .to_vec()
            .into_iter()
            .map(|line| adjust_line_length_no_bg(&line, width))
            .collect::<Vec<_>>();
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
        self.child.on_mount();
    }

    fn on_unmount(&mut self) {
        self.child.on_unmount();
    }

    fn on_tick(&mut self, tick: u64) {
        self.child.on_tick(tick);
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        self.child.on_resize(width, height);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.child.on_event_capture(event, ctx);
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if let Event::Action(action) = event {
            match action {
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
        self.child.on_event(event, ctx);
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

    fn visit_children_mut(&mut self, f: &mut dyn FnMut(&mut dyn Widget)) {
        f(&mut self.child);
    }

    fn layout_height(&self) -> Option<usize> {
        self.height
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

pub struct HorizontalScroll {
    id: WidgetId,
    child: Container,
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
            id: WidgetId::new(),
            child: Container::new(),
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

    fn max_offset(&self) -> usize {
        let content = self.content_width.load(Ordering::Relaxed);
        let viewport = self.viewport_width.load(Ordering::Relaxed).max(1);
        content.saturating_sub(viewport)
    }

    fn clamp_offset(&mut self) {
        let max_x = self.max_offset();
        if self.offset_x > max_x {
            self.offset_x = max_x;
        }
    }
}

impl Widget for HorizontalScroll {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let viewport_width = options.size.0.max(1);
        let viewport_height = self.height.unwrap_or_else(|| options.size.1.max(1));
        self.viewport_width.store(viewport_width, Ordering::Relaxed);

        let constraints = self.child.layout_constraints();
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
        let target_height = self.child.layout_height().unwrap_or(viewport_height).max(1);
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
        let mut lines = Segment::split_and_crop_lines(segments, render_width, None, true, false);
        if let Some(height) = self.child.layout_height() {
            lines = Segment::set_shape(&lines, render_width, Some(height.max(1)), None, false);
        }
        lines = pad_lines_to_width(lines, render_width);

        let content_width = lines
            .iter()
            .map(|line| Segment::get_line_length(line))
            .max()
            .unwrap_or(viewport_width)
            .max(viewport_width);
        self.content_width.store(content_width, Ordering::Relaxed);

        let max_offset = content_width.saturating_sub(viewport_width);
        let offset = self.offset_x.min(max_offset);
        let slice = lines
            .into_iter()
            .take(viewport_height)
            .map(|line| {
                let cropped = crop_line_horizontal(&line, offset, viewport_width);
                adjust_line_length_no_bg(&cropped, viewport_width)
            })
            .collect::<Vec<_>>();
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
        self.child.on_mount();
    }

    fn on_unmount(&mut self) {
        self.child.on_unmount();
    }

    fn on_tick(&mut self, tick: u64) {
        self.child.on_tick(tick);
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        self.child.on_resize(width, height);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.child.on_event_capture(event, ctx);
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if let Event::Action(action) = event {
            match action {
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

    fn visit_children_mut(&mut self, f: &mut dyn FnMut(&mut dyn Widget)) {
        f(&mut self.child);
    }

    fn layout_height(&self) -> Option<usize> {
        self.height.or_else(|| self.child.layout_height())
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

impl rich_rs::Renderable for HorizontalScroll {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}
