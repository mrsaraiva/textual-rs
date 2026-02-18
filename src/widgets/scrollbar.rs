use rich_rs::{Segment, Segments};

use crate::event::{Event, EventCtx, MouseDownEvent, MouseMoveEvent};
use crate::message::{AppRootScrollbarAxis, AppRootScrollbarScrollTo, Message};
use crate::style::{Color, Overflow, ScrollbarGutter, ScrollbarVisibility, Style};
use crate::widgets::{Widget, WidgetStyles};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrollTo {
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub animate: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollDirectionMessage {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy)]
pub struct ScrollBarRender {
    pub virtual_size: usize,
    pub window_size: usize,
    pub position: f32,
    pub thickness: usize,
    pub vertical: bool,
}

impl ScrollBarRender {
    pub fn thumb_range(
        track_len: usize,
        virtual_size: usize,
        window_size: usize,
        position: f32,
    ) -> (usize, usize) {
        if track_len == 0 {
            return (0, 0);
        }
        if virtual_size <= window_size || window_size == 0 {
            return (0, track_len);
        }

        let bar_ratio = virtual_size as f32 / track_len as f32;
        let thumb_size = (window_size as f32 / bar_ratio).max(1.0);
        let thumb_len = thumb_size.ceil().clamp(1.0, track_len as f32) as usize;

        let max_position = (virtual_size.saturating_sub(window_size)) as f32;
        let clamped_position = position.clamp(0.0, max_position);
        let ratio = if max_position > 0.0 {
            clamped_position / max_position
        } else {
            0.0
        };
        let travel = (track_len as f32 - thumb_size).max(0.0);
        let thumb_start = (travel * ratio)
            .floor()
            .clamp(0.0, (track_len.saturating_sub(thumb_len)) as f32)
            as usize;
        (thumb_start, thumb_len)
    }

    pub fn render_bar(
        &self,
        track_len: usize,
        back_color: Color,
        thumb_color: Color,
    ) -> Vec<Vec<Segment>> {
        let (thumb_start, thumb_len) = Self::thumb_range(
            track_len,
            self.virtual_size,
            self.window_size,
            self.position,
        );
        let track_style = rich_rs::Style::new().with_bgcolor(back_color.to_simple_opaque());
        let thumb_style = rich_rs::Style::new().with_bgcolor(thumb_color.to_simple_opaque());

        if self.vertical {
            let mut lines = Vec::with_capacity(track_len.max(1));
            for row in 0..track_len.max(1) {
                let style = if row >= thumb_start && row < thumb_start.saturating_add(thumb_len) {
                    thumb_style
                } else {
                    track_style
                };
                lines.push(vec![
                    Segment::styled(" ".to_string(), style);
                    self.thickness.max(1)
                ]);
            }
            lines
        } else {
            let mut row = Vec::with_capacity(track_len.max(1));
            for col in 0..track_len.max(1) {
                let style = if col >= thumb_start && col < thumb_start.saturating_add(thumb_len) {
                    thumb_style
                } else {
                    track_style
                };
                row.push(Segment::styled(" ".to_string(), style));
            }
            vec![row]
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollbarAxis {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollbarPart {
    Track,
    Thumb,
    Corner,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrollbarHit {
    pub axis: ScrollbarAxis,
    pub part: ScrollbarPart,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrollbarPolicy {
    pub overflow_x: Overflow,
    pub overflow_y: Overflow,
    pub visibility: ScrollbarVisibility,
    pub gutter: ScrollbarGutter,
    pub vertical_size: usize,
    pub horizontal_size: usize,
}

impl ScrollbarPolicy {
    pub fn from_style(
        style: &Style,
        default_vertical_size: usize,
        default_horizontal_size: usize,
    ) -> Self {
        let fallback_overflow = style.overflow.unwrap_or(Overflow::Auto);
        Self {
            overflow_x: style.overflow_x.unwrap_or(fallback_overflow),
            overflow_y: style.overflow_y.unwrap_or(fallback_overflow),
            visibility: style
                .scrollbar_visibility
                .unwrap_or(ScrollbarVisibility::Auto),
            gutter: style.scrollbar_gutter.unwrap_or(ScrollbarGutter::Auto),
            vertical_size: style
                .scrollbar_size_vertical
                .or(style.scrollbar_size)
                .map(|size| size.max(1) as usize)
                .unwrap_or(default_vertical_size.max(1)),
            horizontal_size: style
                .scrollbar_size_horizontal
                .or(style.scrollbar_size)
                .map(|size| size.max(1) as usize)
                .unwrap_or(default_horizontal_size.max(1)),
        }
    }

    pub fn resolve(
        self,
        widget_width: usize,
        widget_height: usize,
        content_width: usize,
        content_height: usize,
    ) -> ScrollbarGeometry {
        let widget_width = widget_width.max(1);
        let widget_height = widget_height.max(1);
        let content_width = content_width.max(widget_width);
        let content_height = content_height.max(widget_height);

        let allow_h = !matches!(self.visibility, ScrollbarVisibility::Hidden)
            && !matches!(self.overflow_x, Overflow::Hidden);
        let allow_v = !matches!(self.visibility, ScrollbarVisibility::Hidden)
            && !matches!(self.overflow_y, Overflow::Hidden);
        let force_visible = matches!(self.visibility, ScrollbarVisibility::Visible);
        let force_gutter = matches!(self.gutter, ScrollbarGutter::Stable);

        let mut show_v = false;
        let mut show_h = false;
        let mut viewport_width = widget_width;
        let mut viewport_height = widget_height;
        for _ in 0..3 {
            let reserve_v = show_v || force_gutter;
            let reserve_h = show_h || (force_gutter && allow_h);
            let next_viewport_w = widget_width
                .saturating_sub(if reserve_v {
                    self.vertical_size.min(widget_width.saturating_sub(1))
                } else {
                    0
                })
                .max(1);
            let next_viewport_h = widget_height
                .saturating_sub(if reserve_h {
                    self.horizontal_size.min(widget_height.saturating_sub(1))
                } else {
                    0
                })
                .max(1);
            let next_show_v = allow_v && (content_height > next_viewport_h || force_visible);
            let next_show_h = allow_h && (content_width > next_viewport_w || force_visible);
            viewport_width = next_viewport_w;
            viewport_height = next_viewport_h;
            if next_show_v == show_v && next_show_h == show_h {
                break;
            }
            show_v = next_show_v;
            show_h = next_show_h;
        }

        let reserve_v = show_v || force_gutter;
        let reserve_h = show_h || (force_gutter && allow_h);
        let vertical_lane_width = if reserve_v {
            widget_width.saturating_sub(viewport_width)
        } else {
            0
        };
        let horizontal_lane_height = if reserve_h {
            widget_height.saturating_sub(viewport_height)
        } else {
            0
        };

        ScrollbarGeometry {
            widget_width,
            widget_height,
            content_width,
            content_height,
            viewport_width,
            viewport_height,
            vertical_lane_width,
            horizontal_lane_height,
            show_vertical: show_v,
            show_horizontal: show_h,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrollbarGeometry {
    pub widget_width: usize,
    pub widget_height: usize,
    pub content_width: usize,
    pub content_height: usize,
    pub viewport_width: usize,
    pub viewport_height: usize,
    pub vertical_lane_width: usize,
    pub horizontal_lane_height: usize,
    pub show_vertical: bool,
    pub show_horizontal: bool,
}

impl ScrollbarGeometry {
    pub fn from_runtime_state(
        widget_width: usize,
        widget_height: usize,
        content_width: usize,
        content_height: usize,
        viewport_width: usize,
        viewport_height: usize,
        vertical_lane_width: usize,
        horizontal_lane_height: usize,
    ) -> Self {
        Self {
            widget_width: widget_width.max(1),
            widget_height: widget_height.max(1),
            content_width: content_width.max(1),
            content_height: content_height.max(1),
            viewport_width: viewport_width.max(1),
            viewport_height: viewport_height.max(1),
            vertical_lane_width,
            horizontal_lane_height,
            show_vertical: vertical_lane_width > 0,
            show_horizontal: horizontal_lane_height > 0,
        }
    }

    pub fn max_offset_x(&self) -> usize {
        max_offset(self.content_width, self.viewport_width)
    }

    pub fn max_offset_y(&self) -> usize {
        max_offset(self.content_height, self.viewport_height)
    }

    pub fn clamp_offset_x(&self, offset: usize) -> usize {
        clamp_offset(offset, self.content_width, self.viewport_width)
    }

    pub fn clamp_offset_y(&self, offset: usize) -> usize {
        clamp_offset(offset, self.content_height, self.viewport_height)
    }

    pub fn vertical_lane_start(&self) -> Option<usize> {
        (self.vertical_lane_width > 0)
            .then_some(self.widget_width.saturating_sub(self.vertical_lane_width))
    }

    pub fn horizontal_lane_start(&self) -> Option<usize> {
        (self.horizontal_lane_height > 0).then_some(
            self.widget_height
                .saturating_sub(self.horizontal_lane_height),
        )
    }

    pub fn is_vertical_scrollable(&self) -> bool {
        self.vertical_lane_width > 0 && self.content_height > self.viewport_height
    }

    pub fn is_horizontal_scrollable(&self) -> bool {
        self.horizontal_lane_height > 0 && self.content_width > self.viewport_width
    }

    pub fn vertical_thumb(&self, offset_y: usize) -> (usize, usize) {
        thumb_range(
            self.viewport_height,
            self.content_height,
            self.viewport_height,
            offset_y,
        )
    }

    pub fn horizontal_thumb(&self, offset_x: usize) -> (usize, usize) {
        thumb_range(
            self.viewport_width,
            self.content_width,
            self.viewport_width,
            offset_x,
        )
    }

    pub fn hit_test(
        &self,
        x: usize,
        y: usize,
        offset_x: usize,
        offset_y: usize,
    ) -> Option<ScrollbarHit> {
        if let (Some(v_start), Some(h_start)) =
            (self.vertical_lane_start(), self.horizontal_lane_start())
        {
            if x >= v_start && y >= h_start {
                return Some(ScrollbarHit {
                    axis: ScrollbarAxis::Vertical,
                    part: ScrollbarPart::Corner,
                });
            }
        }

        if let Some(v_start) = self.vertical_lane_start() {
            if x >= v_start && y < self.viewport_height {
                if self.is_vertical_scrollable() {
                    let (thumb_start, thumb_len) = self.vertical_thumb(offset_y);
                    let part = if y >= thumb_start && y < thumb_start.saturating_add(thumb_len) {
                        ScrollbarPart::Thumb
                    } else {
                        ScrollbarPart::Track
                    };
                    return Some(ScrollbarHit {
                        axis: ScrollbarAxis::Vertical,
                        part,
                    });
                }
                return Some(ScrollbarHit {
                    axis: ScrollbarAxis::Vertical,
                    part: ScrollbarPart::Track,
                });
            }
        }

        if let Some(h_start) = self.horizontal_lane_start() {
            if y >= h_start && x < self.viewport_width {
                if self.is_horizontal_scrollable() {
                    let (thumb_start, thumb_len) = self.horizontal_thumb(offset_x);
                    let part = if x >= thumb_start && x < thumb_start.saturating_add(thumb_len) {
                        ScrollbarPart::Thumb
                    } else {
                        ScrollbarPart::Track
                    };
                    return Some(ScrollbarHit {
                        axis: ScrollbarAxis::Horizontal,
                        part,
                    });
                }
                return Some(ScrollbarHit {
                    axis: ScrollbarAxis::Horizontal,
                    part: ScrollbarPart::Track,
                });
            }
        }

        None
    }

    pub fn page_offset_for_track_click(
        &self,
        axis: ScrollbarAxis,
        pointer: usize,
        current_offset: usize,
    ) -> usize {
        let (viewport_len, content_len, thumb_start, thumb_len) = match axis {
            ScrollbarAxis::Vertical => {
                let (thumb_start, thumb_len) = self.vertical_thumb(current_offset);
                (
                    self.viewport_height,
                    self.content_height,
                    thumb_start,
                    thumb_len,
                )
            }
            ScrollbarAxis::Horizontal => {
                let (thumb_start, thumb_len) = self.horizontal_thumb(current_offset);
                (
                    self.viewport_width,
                    self.content_width,
                    thumb_start,
                    thumb_len,
                )
            }
        };
        let next = if pointer < thumb_start {
            current_offset.saturating_sub(viewport_len)
        } else if pointer >= thumb_start.saturating_add(thumb_len) {
            current_offset.saturating_add(viewport_len)
        } else {
            current_offset
        };
        clamp_offset(next, content_len, viewport_len)
    }

    pub fn drag_offset(
        &self,
        axis: ScrollbarAxis,
        pointer: usize,
        grab_offset: usize,
        current_offset: usize,
    ) -> usize {
        match axis {
            ScrollbarAxis::Vertical => drag_to_offset(
                pointer,
                grab_offset,
                self.viewport_height,
                self.content_height,
                self.viewport_height,
                current_offset,
            ),
            ScrollbarAxis::Horizontal => drag_to_offset(
                pointer,
                grab_offset,
                self.viewport_width,
                self.content_width,
                self.viewport_width,
                current_offset,
            ),
        }
    }
}

pub fn max_offset(content_len: usize, viewport_len: usize) -> usize {
    content_len.saturating_sub(viewport_len.max(1))
}

pub fn clamp_offset(offset: usize, content_len: usize, viewport_len: usize) -> usize {
    offset.min(max_offset(content_len, viewport_len))
}

pub fn scroll_by(offset: usize, delta: i32, content_len: usize, viewport_len: usize) -> usize {
    let next = if delta.is_negative() {
        offset.saturating_sub(delta.unsigned_abs() as usize)
    } else {
        offset.saturating_add(delta as usize)
    };
    clamp_offset(next, content_len, viewport_len)
}

pub fn scroll_end(content_len: usize, viewport_len: usize) -> usize {
    max_offset(content_len, viewport_len)
}

pub fn thumb_range(
    track_len: usize,
    content_len: usize,
    viewport_len: usize,
    offset: usize,
) -> (usize, usize) {
    ScrollBarRender::thumb_range(track_len, content_len, viewport_len, offset as f32)
}

pub fn drag_to_offset(
    pointer: usize,
    grab_offset: usize,
    track_len: usize,
    content_len: usize,
    viewport_len: usize,
    _current_offset: usize,
) -> usize {
    let max_offset = max_offset(content_len, viewport_len);
    if max_offset == 0 || viewport_len == 0 || track_len == 0 {
        return 0;
    }
    let (_thumb_start, thumb_len) = thumb_range(track_len, content_len, viewport_len, 0);
    let thumb_travel = track_len.saturating_sub(thumb_len);
    if thumb_travel == 0 {
        return 0;
    }
    let thumb_origin = pointer.saturating_sub(grab_offset).min(thumb_travel);
    let ratio = (thumb_origin as f64) / (thumb_travel as f64);
    (ratio * (max_offset as f64))
        .round()
        .clamp(0.0, max_offset as f64) as usize
}

pub struct ScrollBar {
    vertical: bool,
    thickness: usize,
    track_len: usize,
    window_virtual_size: usize,
    window_size: usize,
    position: f32,
    mouse_over: bool,
    grabbed: bool,
    grab_offset: usize,
    grab_anchor_screen: usize,
    grabbed_position: f32,
    styles: WidgetStyles,
}

impl ScrollBar {
    pub fn new(vertical: bool, thickness: usize) -> Self {
        Self {
            vertical,
            thickness: thickness.max(1),
            track_len: 1,
            window_virtual_size: 100,
            window_size: 0,
            position: 0.0,
            mouse_over: false,
            grabbed: false,
            grab_offset: 0,
            grab_anchor_screen: 0,
            grabbed_position: 0.0,
            styles: WidgetStyles::default(),
        }
    }

    pub fn set_window_virtual_size(&mut self, size: usize) {
        self.window_virtual_size = size.max(1);
    }

    pub fn set_window_size(&mut self, size: usize) {
        self.window_size = size.max(1);
    }

    pub fn set_position(&mut self, position: f32) {
        self.position = position.max(0.0);
    }

    pub fn position(&self) -> f32 {
        self.position
    }

    pub fn grabbed(&self) -> bool {
        self.grabbed
    }

    pub fn axis(&self) -> AppRootScrollbarAxis {
        if self.vertical {
            AppRootScrollbarAxis::Vertical
        } else {
            AppRootScrollbarAxis::Horizontal
        }
    }
}

impl Widget for ScrollBar {
    fn focusable(&self) -> bool {
        false
    }

    fn can_focus_children(&self) -> bool {
        false
    }

    fn render(&self, _console: &rich_rs::Console, options: &rich_rs::ConsoleOptions) -> Segments {
        let length = if self.vertical {
            options.size.1.max(1)
        } else {
            options.size.0.max(1)
        };

        let style = &self.styles.style;
        let base_bg = style
            .bg
            .or_else(|| crate::style::parse_color_like("$background"))
            .unwrap_or_else(|| Color::rgb(0, 0, 0));
        let bg_raw = if self.grabbed {
            style
                .scrollbar_background_active
                .or(style.scrollbar_background)
                .or_else(|| crate::style::parse_color_like("$scrollbar-background-active"))
                .or_else(|| crate::style::parse_color_like("$scrollbar-background"))
        } else if self.mouse_over {
            style
                .scrollbar_background_hover
                .or(style.scrollbar_background)
                .or_else(|| crate::style::parse_color_like("$scrollbar-background-hover"))
                .or_else(|| crate::style::parse_color_like("$scrollbar-background"))
        } else {
            style
                .scrollbar_background
                .or_else(|| crate::style::parse_color_like("$scrollbar-background"))
        }
        .unwrap_or_else(|| Color::rgb(40, 40, 40));
        let bg = bg_raw.flatten_over(base_bg);

        let thumb_raw = if self.grabbed {
            style
                .scrollbar_color_active
                .or_else(|| crate::style::parse_color_like("$scrollbar-active"))
        } else if self.mouse_over {
            style
                .scrollbar_color_hover
                .or(style.scrollbar_color)
                .or_else(|| crate::style::parse_color_like("$scrollbar-hover"))
                .or_else(|| crate::style::parse_color_like("$scrollbar"))
        } else {
            style
                .scrollbar_color
                .or_else(|| crate::style::parse_color_like("$scrollbar"))
        }
        .unwrap_or_else(|| Color::rgb(48, 156, 255));
        let thumb = thumb_raw.flatten_over(bg);
        let renderer = ScrollBarRender {
            virtual_size: self.window_virtual_size,
            window_size: self.window_size,
            position: self.position,
            thickness: self.thickness,
            vertical: self.vertical,
        };
        let lines = renderer.render_bar(length, bg, thumb);
        let mut out = Segments::new();
        for (idx, line) in lines.into_iter().enumerate() {
            out.extend(line);
            if idx + 1 < length {
                out.push(Segment::line());
            }
        }
        out
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        match event {
            Event::MouseDown(MouseDownEvent {
                target,
                x,
                y,
                screen_x,
                screen_y,
            }) if *target == self.node_id() => {
                let pointer = if self.vertical {
                    *y as usize
                } else {
                    *x as usize
                };
                let screen_pointer = if self.vertical {
                    *screen_y as usize
                } else {
                    *screen_x as usize
                };
                let track_len = self.track_len.max(1);
                let current_offset = self.position.max(0.0).round() as usize;
                let (thumb_start, thumb_len) = thumb_range(
                    track_len,
                    self.window_virtual_size,
                    self.window_size.max(1),
                    current_offset,
                );
                if pointer >= thumb_start && pointer < thumb_start.saturating_add(thumb_len.max(1))
                {
                    self.grabbed = true;
                    self.grab_offset = pointer.saturating_sub(thumb_start);
                    self.grab_anchor_screen = screen_pointer;
                    self.grabbed_position = self.position.max(0.0);
                } else {
                    let page = self.window_size.max(1);
                    let mut next = current_offset;
                    if pointer < thumb_start {
                        next = next.saturating_sub(page);
                    } else if pointer >= thumb_start.saturating_add(thumb_len) {
                        next = next.saturating_add(page);
                    }
                    let clamped =
                        clamp_offset(next, self.window_virtual_size, self.window_size.max(1));
                    self.position = clamped as f32;
                    ctx.post_message(Message::AppRootScrollbarScrollTo(
                        AppRootScrollbarScrollTo {
                            axis: self.axis(),
                            offset: clamped,
                        },
                    ));
                }
                ctx.set_handled();
            }
            Event::MouseMove(MouseMoveEvent {
                target,
                screen_x,
                screen_y,
                ..
            }) if *target == self.node_id() && self.grabbed => {
                let screen_pointer = if self.vertical {
                    *screen_y as usize
                } else {
                    *screen_x as usize
                };
                let max_pos = max_offset(self.window_virtual_size, self.window_size.max(1)) as f32;
                let scale = self.window_virtual_size as f32 / self.window_size.max(1) as f32;
                let delta = screen_pointer as f32 - self.grab_anchor_screen as f32;
                let next_pos = (self.grabbed_position + delta * scale).clamp(0.0, max_pos);
                if (next_pos - self.position).abs() > f32::EPSILON {
                    self.position = next_pos;
                    let next = next_pos.clamp(0.0, max_pos) as usize;
                    ctx.post_message(Message::AppRootScrollbarScrollTo(
                        AppRootScrollbarScrollTo {
                            axis: self.axis(),
                            offset: next,
                        },
                    ));
                }
                ctx.set_handled();
            }
            Event::MouseUp(_) if self.grabbed => {
                self.grabbed = false;
                self.grab_offset = 0;
                self.grab_anchor_screen = 0;
                self.grabbed_position = self.position.max(0.0);
                ctx.set_handled();
            }
            Event::AppFocus(false) => {
                self.grabbed = false;
                self.grab_offset = 0;
                self.grab_anchor_screen = 0;
                self.grabbed_position = self.position.max(0.0);
            }
            _ => {}
        }
    }

    fn on_mouse_move(&mut self, _x: u16, _y: u16) -> bool {
        let changed = !self.mouse_over;
        self.mouse_over = true;
        changed
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.track_len = if self.vertical {
            height.max(1) as usize
        } else {
            width.max(1) as usize
        };
    }

    fn set_hovered(&mut self, hovered: bool) {
        self.mouse_over = hovered;
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

pub struct ScrollBarCorner {
    styles: WidgetStyles,
}

impl ScrollBarCorner {
    pub fn new() -> Self {
        Self {
            styles: WidgetStyles::default(),
        }
    }
}

impl Default for ScrollBarCorner {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for ScrollBarCorner {
    fn focusable(&self) -> bool {
        false
    }

    fn can_focus_children(&self) -> bool {
        false
    }

    fn render(&self, _console: &rich_rs::Console, options: &rich_rs::ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);
        let style = &self.styles.style;
        let base_bg = style
            .bg
            .or_else(|| crate::style::parse_color_like("$background"))
            .unwrap_or_else(|| Color::rgb(0, 0, 0));
        let color = style
            .scrollbar_corner_color
            .or_else(|| crate::style::parse_color_like("$scrollbar-corner-color"))
            .unwrap_or_else(|| Color::rgb(40, 40, 40))
            .flatten_over(base_bg);
        let style = rich_rs::Style::new().with_bgcolor(color.to_simple_opaque());
        let mut out = Segments::new();
        for row in 0..height {
            out.extend(vec![Segment::styled(" ".to_string(), style); width]);
            if row + 1 < height {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::MouseUpEvent;

    #[test]
    fn policy_resolve_handles_vertical_overflow_with_lane() {
        let policy = ScrollbarPolicy {
            overflow_x: Overflow::Auto,
            overflow_y: Overflow::Auto,
            visibility: ScrollbarVisibility::Auto,
            gutter: ScrollbarGutter::Auto,
            vertical_size: 2,
            horizontal_size: 1,
        };
        let geometry = policy.resolve(80, 20, 80, 60);
        assert!(geometry.show_vertical);
        assert_eq!(geometry.vertical_lane_width, 2);
        assert_eq!(geometry.viewport_width, 78);
        assert!(geometry.is_vertical_scrollable());
    }

    #[test]
    fn geometry_hit_test_distinguishes_thumb_and_track() {
        let geometry = ScrollbarGeometry {
            widget_width: 80,
            widget_height: 20,
            content_width: 80,
            content_height: 60,
            viewport_width: 78,
            viewport_height: 20,
            vertical_lane_width: 2,
            horizontal_lane_height: 0,
            show_vertical: true,
            show_horizontal: false,
        };
        let (thumb_start, _thumb_len) = geometry.vertical_thumb(10);
        let thumb_hit = geometry.hit_test(79, thumb_start, 0, 10).unwrap();
        assert_eq!(thumb_hit.axis, ScrollbarAxis::Vertical);
        assert_eq!(thumb_hit.part, ScrollbarPart::Thumb);

        let track_y = if thumb_start > 0 {
            thumb_start - 1
        } else {
            geometry.vertical_thumb(10).1 + 1
        };
        let track_hit = geometry.hit_test(79, track_y, 0, 10).unwrap();
        assert_eq!(track_hit.axis, ScrollbarAxis::Vertical);
        assert_eq!(track_hit.part, ScrollbarPart::Track);
    }

    #[test]
    fn mouse_up_clears_grab_even_when_target_differs() {
        let mut bar = ScrollBar::new(true, 2);
        bar.grabbed = true;
        bar.grab_offset = 3;
        bar.grab_anchor_screen = 10;
        bar.grabbed_position = 4.0;

        let mut ctx = EventCtx::default();
        bar.on_event(
            &Event::MouseUp(MouseUpEvent {
                target: None,
                screen_x: 0,
                screen_y: 0,
                x: 0,
                y: 0,
            }),
            &mut ctx,
        );
        assert!(!bar.grabbed);
        assert_eq!(bar.grab_offset, 0);
        assert_eq!(bar.grab_anchor_screen, 0);
    }
}
