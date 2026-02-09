use std::sync::atomic::{AtomicUsize, Ordering};

use rich_rs::{Console, ConsoleOptions, Segment, Segments};

use crate::event::{Action, Event, EventCtx};
use crate::style::parse_color_like;

use super::helpers::adjust_line_length_no_bg;
use super::helpers::{clamp_with_constraints, crop_line_horizontal, pad_lines_to_width};
use super::{Container, Node, Row, RowAlign, Widget, WidgetId, WidgetStyles};

fn scrollbar_thumb(
    track_len: usize,
    virtual_len: usize,
    window_len: usize,
    position: usize,
) -> (usize, usize) {
    if track_len == 0 {
        return (0, 0);
    }
    if virtual_len <= window_len || virtual_len == 0 || window_len == 0 {
        return (0, track_len);
    }

    let track_f = track_len as f64;
    let window_f = window_len as f64;
    let virtual_f = virtual_len as f64;

    let bar_ratio = virtual_f / track_f;
    let thumb_size_f = (window_f / bar_ratio).max(1.0);
    let thumb_len = thumb_size_f.ceil().clamp(1.0, track_f) as usize;

    let max_pos = (virtual_len - window_len) as f64;
    if max_pos <= 0.0 {
        return (0, thumb_len);
    }
    let position_ratio = (position as f64 / max_pos).clamp(0.0, 1.0);
    let travel_f = (track_f - thumb_size_f).max(0.0);
    let thumb_start = (travel_f * position_ratio)
        .floor()
        .clamp(0.0, (track_len.saturating_sub(thumb_len)) as f64)
        as usize;
    (thumb_start, thumb_len)
}

fn scrollbar_styles() -> (rich_rs::Style, rich_rs::Style) {
    let track_bg = parse_color_like("$scrollbar-background")
        .or_else(|| parse_color_like("$surface-darken-2"))
        .unwrap_or_else(|| crate::style::Color::rgb(0x1f, 0x26, 0x30));
    let thumb_bg = parse_color_like("$scrollbar")
        .or_else(|| parse_color_like("$primary"))
        .unwrap_or_else(|| crate::style::Color::rgb(0x2f, 0x9e, 0xff));

    let track_style = rich_rs::Style::new().with_bgcolor(track_bg.to_simple_opaque());
    let thumb_style = rich_rs::Style::new().with_bgcolor(thumb_bg.to_simple_opaque());
    (track_style, thumb_style)
}

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

    pub fn class(self, value: impl Into<String>) -> Node {
        Node::new(self).class(value)
    }

    pub fn id(self, value: impl Into<String>) -> Node {
        Node::new(self).id(value)
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
    focused: bool,
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
            focused: false,
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

    fn focusable(&self) -> bool {
        true
    }

    fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
    }

    fn has_focus(&self) -> bool {
        self.focused
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let viewport_height = self.height.unwrap_or_else(|| options.size.1.max(1)).max(1);
        self.viewport_height
            .store(viewport_height, Ordering::Relaxed);

        let constraints = self.child.layout_constraints();
        const V_SCROLLBAR_SIZE: usize = 2;
        let child_layout_height = self.child.layout_height();
        let (track_style, thumb_style) = scrollbar_styles();

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
            let target_height = child_layout_height.unwrap_or_else(|| {
                viewport_height.saturating_add(viewport_height).max(1)
            });
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
                candidate =
                    Segment::set_shape(&candidate, render_width, Some(effective_height), None, false);
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
        slice = Segment::set_shape(&slice, content_viewport_w, Some(viewport_height), None, false);

        if show_v {
            let track_len = viewport_height.max(1);
            let (thumb_start, thumb_len) =
                scrollbar_thumb(track_len, content_height, viewport_height, offset);
            let bar_width = width.saturating_sub(content_viewport_w).max(1);
            for (row, line) in slice.iter_mut().enumerate() {
                let style = if row < track_len && row >= thumb_start && row < thumb_start + thumb_len
                {
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
        self.height.or_else(|| self.child.layout_height())
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
        let viewport_height = self.height.unwrap_or_else(|| options.size.1.max(1)).max(1);
        const H_SCROLLBAR_SIZE: usize = 1;
        let constraints = self.child.layout_constraints();
        let (track_style, thumb_style) = scrollbar_styles();

        let mut show_h = false;
        let mut content_viewport_h = viewport_height;
        let mut lines: Vec<Vec<Segment>> = Vec::new();
        let mut content_width = viewport_width;

        for _ in 0..2 {
            let viewport_h = viewport_height.saturating_sub(if show_h { H_SCROLLBAR_SIZE } else { 0 }).max(1);
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

        self.viewport_width.store(viewport_width, Ordering::Relaxed);
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
        let mut slice =
            Segment::set_shape(&slice, viewport_width, Some(content_viewport_h), None, false);

        if show_h {
            let (thumb_start, thumb_len) =
                scrollbar_thumb(viewport_width, content_width, viewport_width, offset);
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
