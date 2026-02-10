use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::debug::{DebugLayout, debug_input, debug_layout};
use crate::event::{
    Action, AnimationEase, AnimationLevel, AnimationRequest, AnimationValueEvent, Event, EventCtx,
};
use crate::style::{TransitionTiming, parse_color_like};

use crate::widgets::{
    Widget, WidgetId, WidgetStyles,
    helpers::{
        adjust_line_length_no_bg, apply_debug_box, clamp_with_constraints, crop_line_horizontal,
        fixed_height_from_constraints, pad_lines_to_width,
    },
};

pub struct ScrollView {
    id: WidgetId,
    child: Box<dyn Widget>,
    height: Option<usize>,
    pub(crate) offset_y: usize,
    render_offset_y: f32,
    scroll_step: usize,
    pub(crate) content_height: AtomicUsize,
    pub(crate) viewport_height: AtomicUsize,
    offset_x: usize,
    render_offset_x: f32,
    scroll_step_x: usize,
    pub(crate) content_width: AtomicUsize,
    pub(crate) viewport_width: AtomicUsize,
    widget_width: AtomicUsize,
    widget_height: AtomicUsize,
    drag_v: Option<usize>,
    drag_h: Option<usize>,
    styles: WidgetStyles,
}

impl ScrollView {
    pub(crate) const OFFSET_Y_ATTR: &'static str = "scrollview.offset_y";
    const OFFSET_X_ATTR: &'static str = "scrollview.offset_x";

    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            id: WidgetId::new(),
            child: Box::new(child),
            height: None,
            offset_y: 0,
            render_offset_y: 0.0,
            scroll_step: 1,
            content_height: AtomicUsize::new(0),
            viewport_height: AtomicUsize::new(0),
            offset_x: 0,
            render_offset_x: 0.0,
            scroll_step_x: 2,
            content_width: AtomicUsize::new(0),
            viewport_width: AtomicUsize::new(0),
            widget_width: AtomicUsize::new(0),
            widget_height: AtomicUsize::new(0),
            drag_v: None,
            drag_h: None,
            styles: WidgetStyles::default(),
        }
    }

    pub fn height(mut self, height: usize) -> Self {
        self.height = Some(height.max(1));
        self
    }

    pub fn scroll_to(&mut self, offset_y: usize) {
        self.offset_y = offset_y;
        self.clamp_offset();
        self.render_offset_y = self.offset_y as f32;
    }

    pub fn scroll_to_x(&mut self, offset_x: usize) {
        self.offset_x = offset_x;
        self.clamp_offset();
        self.render_offset_x = self.offset_x as f32;
    }

    pub fn scroll_by(&mut self, delta: i32) {
        if delta.is_negative() {
            self.offset_y = self.offset_y.saturating_sub(delta.unsigned_abs() as usize);
        } else {
            self.offset_y = self.offset_y.saturating_add(delta as usize);
        }
        self.clamp_offset();
        self.render_offset_y = self.offset_y as f32;
    }

    pub fn scroll_by_x(&mut self, delta: i32) {
        if delta.is_negative() {
            self.offset_x = self.offset_x.saturating_sub(delta.unsigned_abs() as usize);
        } else {
            self.offset_x = self.offset_x.saturating_add(delta as usize);
        }
        self.clamp_offset();
        self.render_offset_x = self.offset_x as f32;
    }

    pub fn scroll_step(mut self, step: usize) -> Self {
        self.scroll_step = step.max(1);
        self
    }

    pub fn scroll_step_x(mut self, step: usize) -> Self {
        self.scroll_step_x = step.max(1);
        self
    }

    pub fn offset_y(&self) -> usize {
        self.offset_y
    }

    pub fn offset_x(&self) -> usize {
        self.offset_x
    }

    pub(crate) fn line_max_offset(content_len: usize, viewport_len: usize) -> usize {
        content_len.saturating_sub(viewport_len.max(1))
    }

    pub(crate) fn line_clamp_offset(
        offset: usize,
        content_len: usize,
        viewport_len: usize,
    ) -> usize {
        offset.min(Self::line_max_offset(content_len, viewport_len))
    }

    pub(crate) fn line_scroll_by(
        offset: usize,
        delta: i32,
        content_len: usize,
        viewport_len: usize,
    ) -> usize {
        let next = if delta.is_negative() {
            offset.saturating_sub(delta.unsigned_abs() as usize)
        } else {
            offset.saturating_add(delta as usize)
        };
        Self::line_clamp_offset(next, content_len, viewport_len)
    }

    pub(crate) fn line_scroll_end(content_len: usize, viewport_len: usize) -> usize {
        content_len.saturating_sub(viewport_len.max(1))
    }

    pub(crate) fn line_scrollbar_thumb(
        track_len: usize,
        content_len: usize,
        viewport_len: usize,
        offset: usize,
    ) -> (usize, usize) {
        if track_len == 0 {
            return (0, 0);
        }
        if content_len <= viewport_len {
            return (0, track_len);
        }
        // Match Textual's scrollbar sizing/positioning model:
        // thumb_size = max(1, window_size / (virtual_size / track_size))
        // thumb_start = floor((track_size - thumb_size) * position_ratio)
        let track_f = track_len as f64;
        let virtual_f = content_len as f64;
        let window_f = viewport_len as f64;
        let bar_ratio = virtual_f / track_f;
        let thumb_size_f = (window_f / bar_ratio).max(1.0);
        let thumb_len = thumb_size_f.ceil().clamp(1.0, track_f) as usize;

        let max_offset = content_len.saturating_sub(viewport_len);
        if max_offset == 0 {
            return (0, thumb_len);
        }
        let position_ratio = (offset.min(max_offset) as f64) / (max_offset as f64);
        let travel_f = (track_f - thumb_size_f).max(0.0);
        let thumb_start = (travel_f * position_ratio)
            .floor()
            .clamp(0.0, (track_len.saturating_sub(thumb_len)) as f64)
            as usize;
        (thumb_start, thumb_len)
    }

    pub(crate) fn line_drag_offset(
        pointer: usize,
        grab_offset: usize,
        track_len: usize,
        content_len: usize,
        viewport_len: usize,
        current_offset: usize,
    ) -> usize {
        let (_thumb_start, thumb_len) =
            Self::line_scrollbar_thumb(track_len, content_len, viewport_len, current_offset);
        let travel = track_len.saturating_sub(thumb_len);
        let pointer = (pointer as isize) - (grab_offset as isize);
        let new_thumb_start = pointer.clamp(0, travel as isize) as usize;
        let max_offset = Self::line_max_offset(content_len, viewport_len);
        if travel == 0 {
            0
        } else {
            (((new_thumb_start as u128) * (max_offset as u128) + (travel as u128 / 2))
                / (travel as u128)) as usize
        }
    }

    pub(crate) fn line_scrollbar_styles() -> (rich_rs::Style, rich_rs::Style, rich_rs::Style) {
        let track_bg = parse_color_like("$scrollbar-background")
            .or_else(|| parse_color_like("$background-darken-1"))
            .or_else(|| parse_color_like("$surface-darken-1"))
            .unwrap_or_else(|| crate::style::Color::rgb(30, 30, 30));
        let thumb_bg = parse_color_like("$scrollbar")
            .or_else(|| parse_color_like("$primary-muted"))
            .or_else(|| parse_color_like("$primary"))
            .unwrap_or_else(|| crate::style::Color::rgb(48, 156, 255));
        let thumb_active_bg = parse_color_like("$scrollbar-active")
            .or_else(|| parse_color_like("$primary"))
            .unwrap_or_else(|| crate::style::Color::rgb(1, 120, 212));

        let track_style = rich_rs::Style::new().with_bgcolor(track_bg.to_simple_opaque());
        let thumb_style = rich_rs::Style::new().with_bgcolor(thumb_bg.to_simple_opaque());
        let thumb_active_style =
            rich_rs::Style::new().with_bgcolor(thumb_active_bg.to_simple_opaque());
        (track_style, thumb_style, thumb_active_style)
    }

    pub(crate) fn scrollbar_corner_style() -> rich_rs::Style {
        let corner_bg = parse_color_like("$scrollbar-corner-color")
            .or_else(|| parse_color_like("$scrollbar-background"))
            .or_else(|| parse_color_like("$background-darken-1"))
            .or_else(|| parse_color_like("$surface-darken-1"))
            .unwrap_or_else(|| crate::style::Color::rgb(30, 30, 30));
        rich_rs::Style::new().with_bgcolor(corner_bg.to_simple_opaque())
    }

    fn max_offset(&self) -> usize {
        Self::line_max_offset(
            self.content_height.load(Ordering::Relaxed),
            self.viewport_height.load(Ordering::Relaxed),
        )
    }

    fn max_offset_x(&self) -> usize {
        Self::line_max_offset(
            self.content_width.load(Ordering::Relaxed),
            self.viewport_width.load(Ordering::Relaxed),
        )
    }

    fn clamp_offset(&mut self) {
        let max_y = self.max_offset();
        if self.offset_y > max_y {
            self.offset_y = max_y;
        }
        self.render_offset_y = self.render_offset_y.clamp(0.0, max_y as f32);
        let max_x = self.max_offset_x();
        if self.offset_x > max_x {
            self.offset_x = max_x;
        }
        self.render_offset_x = self.render_offset_x.clamp(0.0, max_x as f32);
    }

    fn scroll_animation_params(&self) -> Option<(Duration, Duration, AnimationEase)> {
        let style = crate::css::resolve_component_style(self, &["scrollview--content"]);
        let duration = style.transition_duration?;
        if duration.is_zero() {
            return None;
        }
        let delay = style.transition_delay.unwrap_or(Duration::ZERO);
        let ease = style
            .transition_timing
            .map(Self::transition_timing_to_animation_ease)
            .unwrap_or(AnimationEase::OutCubic);
        Some((duration, delay, ease))
    }

    fn transition_timing_to_animation_ease(timing: TransitionTiming) -> AnimationEase {
        match timing {
            TransitionTiming::Linear => AnimationEase::Linear,
            TransitionTiming::InOutCubic => AnimationEase::InOutCubic,
            TransitionTiming::OutCubic => AnimationEase::OutCubic,
            TransitionTiming::Round => AnimationEase::Round,
            TransitionTiming::None => AnimationEase::None,
        }
    }

    fn request_offset_y_animation(&mut self, from: usize, to: usize, ctx: &mut EventCtx) {
        if from == to {
            self.render_offset_y = to as f32;
            return;
        }
        if let Some((duration, delay, ease)) = self.scroll_animation_params() {
            self.render_offset_y = from as f32;
            ctx.request_animation(
                AnimationRequest::new(
                    self.id,
                    Self::OFFSET_Y_ATTR,
                    from as f32,
                    to as f32,
                    duration,
                )
                .with_delay(delay)
                .with_ease(ease)
                .with_level(AnimationLevel::Basic),
            );
        } else {
            self.render_offset_y = to as f32;
        }
        ctx.request_repaint();
    }

    fn request_offset_x_animation(&mut self, from: usize, to: usize, ctx: &mut EventCtx) {
        if from == to {
            self.render_offset_x = to as f32;
            return;
        }
        if let Some((duration, delay, ease)) = self.scroll_animation_params() {
            self.render_offset_x = from as f32;
            ctx.request_animation(
                AnimationRequest::new(
                    self.id,
                    Self::OFFSET_X_ATTR,
                    from as f32,
                    to as f32,
                    duration,
                )
                .with_delay(delay)
                .with_ease(ease)
                .with_level(AnimationLevel::Basic),
            );
        } else {
            self.render_offset_x = to as f32;
        }
        ctx.request_repaint();
    }
}

impl Widget for ScrollView {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let viewport_height = self.height.unwrap_or_else(|| options.size.1.max(1));
        self.widget_width.store(width, Ordering::Relaxed);
        self.widget_height.store(viewport_height, Ordering::Relaxed);
        if std::env::var("TEXTUAL_DEBUG_LAYOUT_FILE").is_ok() {
            debug_layout(&format!(
                "[scroll] id={} viewport=({}, {}) offset=({}, {})",
                self.id.as_u64(),
                width,
                viewport_height,
                self.offset_x,
                self.offset_y
            ));
        }
        let constraints = self.child.layout_constraints();
        const V_SCROLLBAR_SIZE: usize = 2;
        const H_SCROLLBAR_SIZE: usize = 1;
        let mut show_v = false;
        let mut show_h = false;
        let mut content_viewport_w = width;
        let mut content_viewport_h = viewport_height;
        let mut lines: Vec<Vec<Segment>> = Vec::new();
        let mut content_width = width;
        let mut content_height = viewport_height;

        for _ in 0..3 {
            let viewport_w = width
                .saturating_sub(if show_v {
                    V_SCROLLBAR_SIZE.min(width.saturating_sub(1))
                } else {
                    0
                })
                .max(1);
            let viewport_h = viewport_height
                .saturating_sub(if show_h { H_SCROLLBAR_SIZE } else { 0 })
                .max(1);

            let target_height = self
                .child
                .layout_height()
                .unwrap_or_else(|| viewport_h.saturating_add(viewport_h).max(1));
            let target_width = self
                .child
                .content_width()
                .unwrap_or(viewport_w)
                .max(viewport_w);
            let render_width = clamp_with_constraints(
                target_width,
                constraints.min_width,
                constraints.max_width,
                target_width,
            )
            .max(viewport_w);
            if std::env::var("TEXTUAL_DEBUG_LAYOUT_FILE").is_ok() {
                debug_layout(&format!(
                    "[scroll] id={} child render_width={} constraints=({:?},{:?})",
                    self.id.as_u64(),
                    render_width,
                    constraints.min_width,
                    constraints.max_width
                ));
            }
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
            let fixed_height = self.child.layout_height();
            if let Some(height) = fixed_height {
                candidate =
                    Segment::set_shape(&candidate, render_width, Some(height.max(1)), None, false);
            }
            candidate = pad_lines_to_width(candidate, render_width);

            let candidate_height = candidate.len().max(viewport_h);
            let candidate_width = candidate
                .iter()
                .map(|line| Segment::get_line_length(line))
                .max()
                .unwrap_or(viewport_w)
                .max(viewport_w);
            let next_show_v = candidate_height > viewport_h;
            let next_show_h = candidate_width > viewport_w;

            lines = candidate;
            content_width = candidate_width;
            content_height = candidate_height;
            content_viewport_w = viewport_w;
            content_viewport_h = viewport_h;

            if next_show_v == show_v && next_show_h == show_h {
                break;
            }
            show_v = next_show_v;
            show_h = next_show_h;
        }

        self.viewport_height
            .store(content_viewport_h, Ordering::Relaxed);
        self.viewport_width
            .store(content_viewport_w, Ordering::Relaxed);
        self.content_height.store(content_height, Ordering::Relaxed);
        self.content_width.store(content_width, Ordering::Relaxed);

        let max_offset = content_height.saturating_sub(content_viewport_h);
        let offset = self.render_offset_y.clamp(0.0, max_offset as f32).round() as usize;
        let max_offset_x = content_width.saturating_sub(content_viewport_w);
        let offset_x = self.render_offset_x.clamp(0.0, max_offset_x as f32).round() as usize;
        let start = offset.min(lines.len());
        let end = (start + content_viewport_h).min(lines.len());
        let mut slice = lines[start..end]
            .to_vec()
            .into_iter()
            .map(|line| {
                let cropped = crop_line_horizontal(&line, offset_x, content_viewport_w);
                adjust_line_length_no_bg(&cropped, content_viewport_w)
            })
            .collect::<Vec<_>>();
        slice = Segment::set_shape(
            &slice,
            content_viewport_w,
            Some(content_viewport_h),
            None,
            false,
        );

        let (track_style, thumb_style, thumb_active_style) = Self::line_scrollbar_styles();
        let corner_style = Self::scrollbar_corner_style();
        let v_scrollbar_size = if show_v {
            width.saturating_sub(content_viewport_w)
        } else {
            0
        };
        if show_v {
            let track_len = content_viewport_h.max(1);
            let (thumb_start, thumb_len) =
                Self::line_scrollbar_thumb(track_len, content_height, content_viewport_h, offset);
            let mut thumb_drawn = false;
            for (row, line) in slice.iter_mut().enumerate() {
                let in_track = row < track_len;
                let style = if in_track && row >= thumb_start && row < thumb_start + thumb_len {
                    if self.drag_v.is_some() {
                        thumb_active_style
                    } else {
                        thumb_style
                    }
                } else {
                    track_style
                };
                for _ in 0..v_scrollbar_size.max(1) {
                    line.push(Segment::styled(" ".to_string(), style));
                }
                thumb_drawn |= in_track && row >= thumb_start && row < thumb_start + thumb_len;
            }
            if !thumb_drawn && !slice.is_empty() {
                let row = track_len.saturating_sub(1).min(slice.len() - 1);
                let line = &mut slice[row];
                for _ in 0..v_scrollbar_size.max(1) {
                    if !line.is_empty() {
                        line.pop();
                    }
                }
                for _ in 0..v_scrollbar_size.max(1) {
                    let active_style = if self.drag_v.is_some() {
                        thumb_active_style
                    } else {
                        thumb_style
                    };
                    line.push(Segment::styled(" ".to_string(), active_style));
                }
            }
        }
        if show_h {
            let (thumb_start, thumb_len) = Self::line_scrollbar_thumb(
                content_viewport_w,
                content_width,
                content_viewport_w,
                offset_x,
            );
            let mut row = Vec::new();
            for col in 0..content_viewport_w {
                let style = if col >= thumb_start && col < thumb_start + thumb_len {
                    if self.drag_h.is_some() {
                        thumb_active_style
                    } else {
                        thumb_style
                    }
                } else {
                    track_style
                };
                row.push(Segment::styled(" ".to_string(), style));
            }
            if show_v {
                for _ in 0..v_scrollbar_size.max(1) {
                    row.push(Segment::styled(" ".to_string(), corner_style));
                }
            }
            slice.push(row);
        }

        slice = Segment::set_shape(&slice, width, Some(viewport_height), None, false);
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

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }

    fn render_with_debug(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        debug: &DebugLayout,
    ) -> Segments {
        let width = options.size.0.max(1);
        let height = self.height.unwrap_or_else(|| options.size.1.max(1));
        let segments = Widget::render(self, console, options);
        let mut lines = Segment::split_and_crop_lines(segments, width, None, true, false);
        let label = if debug.show_sizes {
            Some(format!("{width}x{height}"))
        } else {
            None
        };
        lines = apply_debug_box(
            lines,
            width,
            height.max(3),
            label.as_deref(),
            debug.style_for(0),
        );
        let line_count = lines.len();
        let mut out = Segments::new();
        for (idx, line) in lines.into_iter().enumerate() {
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
        if let Event::AnimationValue(AnimationValueEvent {
            target,
            attribute,
            value,
            done,
        }) = event
        {
            if *target == self.id {
                if attribute == Self::OFFSET_Y_ATTR {
                    if self.drag_v.is_none() {
                        self.render_offset_y = if *done { self.offset_y as f32 } else { *value };
                        ctx.request_repaint();
                    }
                    ctx.set_handled();
                    return;
                }
                if attribute == Self::OFFSET_X_ATTR {
                    if self.drag_h.is_none() {
                        self.render_offset_x = if *done { self.offset_x as f32 } else { *value };
                        ctx.request_repaint();
                    }
                    ctx.set_handled();
                    return;
                }
            }
        }
        if let Event::MouseDown(mouse) = event {
            if mouse.target == self.id {
                let widget_width = self.widget_width.load(Ordering::Relaxed).max(1);
                let widget_height = self.widget_height.load(Ordering::Relaxed).max(1);
                let viewport_w = self.viewport_width.load(Ordering::Relaxed).max(1);
                let viewport_h = self.viewport_height.load(Ordering::Relaxed).max(1);
                let content_w = self.content_width.load(Ordering::Relaxed);
                let content_h = self.content_height.load(Ordering::Relaxed);
                let show_v = content_h > viewport_h;
                let show_h = content_w > viewport_w;
                let v_scrollbar_size = widget_width.saturating_sub(viewport_w).max(1);
                let h_scrollbar_size = widget_height.saturating_sub(viewport_h).max(1);
                let local_x = mouse.x as usize;
                let local_y = mouse.y as usize;

                if show_v
                    && local_x >= widget_width.saturating_sub(v_scrollbar_size)
                    && local_y < viewport_h
                {
                    let (thumb_start, thumb_len) = Self::line_scrollbar_thumb(
                        viewport_h,
                        content_h,
                        viewport_h,
                        self.offset_y,
                    );
                    if local_y >= thumb_start && local_y < thumb_start.saturating_add(thumb_len) {
                        self.drag_v = Some(local_y.saturating_sub(thumb_start));
                        self.drag_h = None;
                        ctx.set_handled();
                        return;
                    }
                    let before = self.offset_y;
                    if local_y < thumb_start {
                        self.scroll_by(-(viewport_h as i32));
                    } else if local_y >= thumb_start.saturating_add(thumb_len) {
                        self.scroll_by(viewport_h as i32);
                    }
                    if self.offset_y != before {
                        self.request_offset_y_animation(before, self.offset_y, ctx);
                        ctx.request_repaint();
                    }
                    ctx.set_handled();
                    return;
                }

                if show_h
                    && local_y >= widget_height.saturating_sub(h_scrollbar_size)
                    && local_x < viewport_w
                {
                    let (thumb_start, thumb_len) = Self::line_scrollbar_thumb(
                        viewport_w,
                        content_w,
                        viewport_w,
                        self.offset_x,
                    );
                    if local_x >= thumb_start && local_x < thumb_start.saturating_add(thumb_len) {
                        self.drag_h = Some(local_x.saturating_sub(thumb_start));
                        self.drag_v = None;
                        ctx.set_handled();
                        return;
                    }
                    let before = self.offset_x;
                    if local_x < thumb_start {
                        self.scroll_by_x(-(viewport_w as i32));
                    } else if local_x >= thumb_start.saturating_add(thumb_len) {
                        self.scroll_by_x(viewport_w as i32);
                    }
                    if self.offset_x != before {
                        self.request_offset_x_animation(before, self.offset_x, ctx);
                        ctx.request_repaint();
                    }
                    ctx.set_handled();
                    return;
                }
            }
        }
        if matches!(event, Event::MouseUp(_) | Event::AppFocus(false)) {
            let was_dragging = self.drag_v.take().is_some() || self.drag_h.take().is_some();
            if was_dragging {
                ctx.set_handled();
            }
        }

        let mut child_ctx = EventCtx::default();
        self.child.on_event(event, &mut child_ctx);
        let child_handled = child_ctx.handled();
        ctx.merge_from(child_ctx);
        if child_handled {
            return;
        }
        if let Event::Action(action) = event {
            match action {
                Action::ScrollUp => {
                    let before = self.offset_y;
                    self.scroll_by(-(self.scroll_step as i32));
                    self.request_offset_y_animation(before, self.offset_y, ctx);
                    debug_input(&format!(
                        "[scrollview] action=ScrollUp before_y={} after_y={} max_y={}",
                        before,
                        self.offset_y,
                        self.max_offset()
                    ));
                    ctx.set_handled();
                    return;
                }
                Action::ScrollDown => {
                    let before = self.offset_y;
                    self.scroll_by(self.scroll_step as i32);
                    self.request_offset_y_animation(before, self.offset_y, ctx);
                    debug_input(&format!(
                        "[scrollview] action=ScrollDown before_y={} after_y={} max_y={}",
                        before,
                        self.offset_y,
                        self.max_offset()
                    ));
                    ctx.set_handled();
                    return;
                }
                Action::ScrollPageUp => {
                    let before = self.offset_y;
                    let page = self.height.unwrap_or(1).max(1);
                    self.scroll_by(-(page as i32));
                    self.request_offset_y_animation(before, self.offset_y, ctx);
                    debug_input(&format!(
                        "[scrollview] action=ScrollPageUp page={} before_y={} after_y={} max_y={}",
                        page,
                        before,
                        self.offset_y,
                        self.max_offset()
                    ));
                    ctx.set_handled();
                    return;
                }
                Action::ScrollPageDown => {
                    let before = self.offset_y;
                    let page = self.height.unwrap_or(1).max(1);
                    self.scroll_by(page as i32);
                    self.request_offset_y_animation(before, self.offset_y, ctx);
                    debug_input(&format!(
                        "[scrollview] action=ScrollPageDown page={} before_y={} after_y={} max_y={}",
                        page,
                        before,
                        self.offset_y,
                        self.max_offset()
                    ));
                    ctx.set_handled();
                    return;
                }
                Action::ScrollLeft => {
                    let before = self.offset_x;
                    self.scroll_by_x(-(self.scroll_step_x as i32));
                    self.request_offset_x_animation(before, self.offset_x, ctx);
                    debug_input(&format!(
                        "[scrollview] action=ScrollLeft before_x={} after_x={} max_x={}",
                        before,
                        self.offset_x,
                        self.max_offset_x()
                    ));
                    ctx.set_handled();
                    return;
                }
                Action::ScrollRight => {
                    let before = self.offset_x;
                    self.scroll_by_x(self.scroll_step_x as i32);
                    self.request_offset_x_animation(before, self.offset_x, ctx);
                    debug_input(&format!(
                        "[scrollview] action=ScrollRight before_x={} after_x={} max_x={}",
                        before,
                        self.offset_x,
                        self.max_offset_x()
                    ));
                    ctx.set_handled();
                    return;
                }
                Action::ScrollPageLeft => {
                    let before = self.offset_x;
                    let page = self.viewport_width.load(Ordering::Relaxed).max(1);
                    self.scroll_by_x(-(page as i32));
                    self.request_offset_x_animation(before, self.offset_x, ctx);
                    debug_input(&format!(
                        "[scrollview] action=ScrollPageLeft page={} before_x={} after_x={} max_x={}",
                        page,
                        before,
                        self.offset_x,
                        self.max_offset_x()
                    ));
                    ctx.set_handled();
                    return;
                }
                Action::ScrollPageRight => {
                    let before = self.offset_x;
                    let page = self.viewport_width.load(Ordering::Relaxed).max(1);
                    self.scroll_by_x(page as i32);
                    self.request_offset_x_animation(before, self.offset_x, ctx);
                    debug_input(&format!(
                        "[scrollview] action=ScrollPageRight page={} before_x={} after_x={} max_x={}",
                        page,
                        before,
                        self.offset_x,
                        self.max_offset_x()
                    ));
                    ctx.set_handled();
                    return;
                }
                _ => {}
            }
        }
    }

    fn on_mouse_scroll(&mut self, delta_x: i32, delta_y: i32, ctx: &mut EventCtx) {
        let before_x = self.offset_x;
        let before_y = self.offset_y;

        if delta_y != 0 {
            self.scroll_by(delta_y.saturating_mul(self.scroll_step as i32));
        }
        if delta_x != 0 {
            self.scroll_by_x(delta_x.saturating_mul(self.scroll_step_x as i32));
        }
        debug_input(&format!(
            "[scrollview] mouse dx={} dy={} before=({}, {}) after=({}, {}) max=({}, {})",
            delta_x,
            delta_y,
            before_x,
            before_y,
            self.offset_x,
            self.offset_y,
            self.max_offset_x(),
            self.max_offset()
        ));

        if self.offset_x != before_x || self.offset_y != before_y {
            self.request_offset_x_animation(before_x, self.offset_x, ctx);
            self.request_offset_y_animation(before_y, self.offset_y, ctx);
            ctx.request_repaint();
            ctx.set_handled();
        }
    }

    fn on_mouse_move(&mut self, x: u16, y: u16) -> bool {
        let mut changed = false;
        if let Some(grab_offset) = self.drag_v {
            let viewport_h = self.viewport_height.load(Ordering::Relaxed).max(1);
            let content_h = self.content_height.load(Ordering::Relaxed).max(1);
            if content_h > viewport_h {
                let new_offset = Self::line_drag_offset(
                    y as usize,
                    grab_offset,
                    viewport_h,
                    content_h,
                    viewport_h,
                    self.offset_y,
                );
                if new_offset != self.offset_y {
                    self.offset_y = new_offset;
                    self.render_offset_y = new_offset as f32;
                    changed = true;
                }
            }
        } else if let Some(grab_offset) = self.drag_h {
            let viewport_w = self.viewport_width.load(Ordering::Relaxed).max(1);
            let content_w = self.content_width.load(Ordering::Relaxed).max(1);
            if content_w > viewport_w {
                let new_offset = Self::line_drag_offset(
                    x as usize,
                    grab_offset,
                    viewport_w,
                    content_w,
                    viewport_w,
                    self.offset_x,
                );
                if new_offset != self.offset_x {
                    self.offset_x = new_offset;
                    self.render_offset_x = new_offset as f32;
                    changed = true;
                }
            }
        }
        changed
    }

    fn visit_children_mut(&mut self, f: &mut dyn FnMut(&mut dyn Widget)) {
        f(self.child.as_mut());
    }

    fn layout_height(&self) -> Option<usize> {
        if let Some(fixed) = fixed_height_from_constraints(self.layout_constraints()) {
            return Some(fixed);
        }
        self.height
    }
}

impl Renderable for ScrollView {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}
