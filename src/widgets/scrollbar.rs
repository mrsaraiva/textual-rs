use rich_rs::{Segment, Segments};
use textual_macros::widget;

use crate::event::{Event, MouseDownEvent, MouseMoveEvent};
use crate::message::ScrollbarScrollTo;
use crate::style::{Color, Overflow, ScrollbarGutter, ScrollbarVisibility, Style};
use crate::widgets::{NodeSeed, Widget};

pub use crate::message::ScrollbarAxis;

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

    /// Render the scrollbar track.
    ///
    /// `track_fg`: fg for track (whitespace) cells. Python `_Styled(render, rich_style)` applies
    /// the host widget `color` to ALL segments (including track whitespace). When `Some`, this fg
    /// is baked into the track_style so `apply_style_to_segments` sees `s.color.is_some()`.
    pub fn render_bar(
        &self,
        track_len: usize,
        back_color: Color,
        thumb_color: Color,
        track_fg: Option<Color>,
    ) -> Vec<Vec<Segment>> {
        const FRACTION_BARS: usize = 8;
        // Glyphs used for scrollbar ends, for sub-cell granularity. Mirrors
        // Python `ScrollBarRender.VERTICAL_BARS` / `HORIZONTAL_BARS`
        // (textual/src/textual/scrollbar.py lines 76-79). The trailing entry is
        // a space, which Python skips (`if bar_character != " "`).
        const VERTICAL_BARS: [&str; FRACTION_BARS] = ["▁", "▂", "▃", "▄", "▅", "▆", "▇", " "];
        const HORIZONTAL_BARS: [&str; FRACTION_BARS] = ["▉", "▊", "▋", "▌", "▍", "▎", "▏", " "];

        let track_len = track_len.max(1);
        let width_thickness = if self.vertical {
            self.thickness.max(1)
        } else {
            1
        };
        let bars = if self.vertical {
            &VERTICAL_BARS
        } else {
            &HORIZONTAL_BARS
        };

        let back = back_color.to_simple_opaque();
        let bar = thumb_color.to_simple_opaque();
        let blank = " ".repeat(width_thickness);

        // Track (background) segment style. Python: `_Style(bgcolor=back, ...)`.
        // Also bake in the fg so that `apply_style_to_segments` sees s.color.is_some()
        // and preserves it (matching Python `_Styled` applying host fg to all segments).
        let track_style = {
            let mut s = rich_rs::Style::new().with_bgcolor(back);
            if let Some(fg) = track_fg {
                // `track_fg` is already composited over the host's base surface by
                // the caller (Python applies the host `color` over
                // `background_colors[0]`, not over the scrollbar track). Use it as-is.
                s = s.with_color(fg.to_simple_opaque());
            }
            s
        };
        // Thumb body. Python uses `_Style(color=bar, reverse=True)` with NO
        // bgcolor (lines 150-152); after the reverse swap this paints bg=bar.
        let thumb_fill_style = rich_rs::Style::new().with_color(bar).with_reverse(true);

        let make_row = |style: rich_rs::Style| vec![Segment::styled(blank.clone(), style)];

        let scrollable = self.window_size > 0
            && track_len > 0
            && self.virtual_size > 0
            && self.virtual_size != track_len;

        let mut segments: Vec<Vec<Segment>> = if scrollable {
            // Python `render_bar` (lines 128-186), shared for both axes.
            let bar_ratio = self.virtual_size as f32 / track_len as f32;
            let thumb_size = (self.window_size as f32 / bar_ratio).max(1.0);
            let max_position = self.virtual_size.saturating_sub(self.window_size) as f32;
            let clamped_position = self.position.clamp(0.0, max_position);
            let position_ratio = if max_position > 0.0 {
                clamped_position / max_position
            } else {
                0.0
            };
            let position = (track_len as f32 - thumb_size).max(0.0) * position_ratio;

            // start = int(position * len_bars); end = start + ceil(thumb_size * len_bars)
            let start = (position * FRACTION_BARS as f32).max(0.0).floor() as usize;
            let end = start.saturating_add((thumb_size * FRACTION_BARS as f32).ceil() as usize);

            // start_index, start_bar = divmod(max(0, start), len_bars)
            let start_index = start / FRACTION_BARS;
            let start_bar = start % FRACTION_BARS;
            let end_index = end / FRACTION_BARS;
            let end_bar = end % FRACTION_BARS;

            // segments = [back] * size; segments[end_index:] = [back] * (size - end_index)
            let mut segments: Vec<Vec<Segment>> = vec![make_row(track_style); track_len];

            // segments[start_index:end_index] = [thumb fill] * (end_index - start_index)
            let fill_end = end_index.min(track_len);
            for row in segments.iter_mut().take(fill_end).skip(start_index) {
                *row = make_row(thumb_fill_style);
            }

            // Apply partial-block glyphs at head/tail for sub-cell granularity.
            if start_index < track_len {
                let glyph = bars[FRACTION_BARS - 1 - start_bar];
                if glyph != " " {
                    // Vertical: `bgcolor=back, color=bar` (no reverse).
                    // Horizontal: `bgcolor=back, color=bar, reverse=True`.
                    let style = rich_rs::Style::new()
                        .with_bgcolor(back)
                        .with_color(bar)
                        .with_reverse(!self.vertical);
                    segments[start_index] =
                        vec![Segment::styled(glyph.repeat(width_thickness), style)];
                }
            }
            if end_index < track_len {
                let glyph = bars[FRACTION_BARS - 1 - end_bar];
                if glyph != " " {
                    // Vertical: `bgcolor=back, color=bar, reverse=True`.
                    // Horizontal: `bgcolor=back, color=bar` (no reverse).
                    let style = rich_rs::Style::new()
                        .with_bgcolor(back)
                        .with_color(bar)
                        .with_reverse(self.vertical);
                    segments[end_index] =
                        vec![Segment::styled(glyph.repeat(width_thickness), style)];
                }
            }
            segments
        } else {
            // Python else-branch: a plain back-colored track.
            vec![make_row(track_style); track_len]
        };

        if self.vertical {
            segments
        } else {
            // Horizontal: each line is the full row, repeated `thickness` times.
            // `segments` currently holds one cell per track index; flatten into a
            // single row and duplicate per thickness line.
            let row: Vec<Segment> = segments.drain(..).flatten().collect();
            vec![row; self.thickness.max(1)]
        }
    }
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
            // Python accepts `scrollbar-size: 0` (`_styles_builder.py`
            // `process_scrollbar_size` validates `isdigit()` only): a 0-size
            // lane reserves no gutter and paints no bar while the host stays
            // scrollable — e.g. `guide/actions` `#page-container
            // { scrollbar-size: 0 0; }`. Do NOT clamp the CSS value up to 1.
            vertical_size: style
                .scrollbar_size_vertical
                .or(style.scrollbar_size)
                .map(|size| size as usize)
                .unwrap_or(default_vertical_size.max(1)),
            horizontal_size: style
                .scrollbar_size_horizontal
                .or(style.scrollbar_size)
                .map(|size| size as usize)
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
        // NOTE: do NOT clamp content up to the widget size. The content extent is
        // the ACTUAL virtual content; a lane must be reserved only on genuine
        // overflow. Clamping content up to the widget made any host that reserved
        // one lane (e.g. a vertical scrollbar) spuriously reserve the other,
        // because the clamped content then "overflowed" the reduced viewport.
        let content_width = content_width.max(1);
        let content_height = content_height.max(1);

        // `scrollbar-visibility` does NOT drop the lane or force it to show.
        // Python reserves the gutter from overflow alone (`_refresh_scrollbars`
        // keys `show_*` off `overflow_x/overflow_y` ONLY, never visibility) and
        // merely declines to PAINT the chrome (`_compositor` adds the chrome
        // widgets only when `scrollbar_visibility == "visible"`). So lane
        // RESERVATION (`allow_*`) and forced-show derive from overflow alone;
        // visibility is handled by the separate `paint_*` flags below.
        let allow_h = !matches!(self.overflow_x, Overflow::Hidden);
        let allow_v = !matches!(self.overflow_y, Overflow::Hidden);
        let force_visible_v = matches!(self.overflow_y, Overflow::Scroll);
        let force_visible_h = matches!(self.overflow_x, Overflow::Scroll);
        // `paint_*` is whether the scrollbar GLYPHS are drawn into the reserved
        // lane. Python's default `scrollbar-visibility` is `visible`, so only the
        // explicit `hidden` case suppresses the paint (lane stays reserved).
        let paint_v = !matches!(self.visibility, ScrollbarVisibility::Hidden);
        let paint_h = !matches!(self.visibility, ScrollbarVisibility::Hidden);
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
            let next_show_v = allow_v && (content_height > next_viewport_h || force_visible_v);
            let next_show_h = allow_h && (content_width > next_viewport_w || force_visible_h);
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
            // `paint_*` gates whether the bar glyphs are drawn into the reserved
            // lane. `scrollbar-visibility: hidden` reserves the gutter (lane
            // stays) but suppresses the paint so the gutter shows the host
            // background, matching Python's compositor (chrome added only when
            // `scrollbar_visibility == "visible"`).
            paint_vertical: paint_v,
            paint_horizontal: paint_h,
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
    /// Whether the vertical scrollbar GLYPHS should be painted. False under
    /// `scrollbar-visibility: hidden` (lane still reserved, just not painted).
    pub paint_vertical: bool,
    /// Whether the horizontal scrollbar GLYPHS should be painted. False under
    /// `scrollbar-visibility: hidden`.
    pub paint_horizontal: bool,
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
            // Runtime-state geometry has no visibility context; default to
            // painting (the hidden-visibility suppression is decided in the
            // policy-resolve path that knows `scrollbar-visibility`).
            paint_vertical: true,
            paint_horizontal: true,
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

#[widget(Focus, Interactive)]
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
    pub(crate) seed: NodeSeed,
}

const DRAG_POSITION_GRANULARITY_STEPS: f32 = 8.0;
// Fixed gain to match thumb-drag feel with wheel scrolling.
const THUMB_DRAG_GAIN_FIXED: f32 = 0.7;

fn quantize_drag_position(position: f32) -> f32 {
    // Python parity: scrollbar position granularity is 1/8th of a cell.
    (position * DRAG_POSITION_GRANULARITY_STEPS).trunc() / DRAG_POSITION_GRANULARITY_STEPS
}

impl ScrollBar {
    crate::seed_ident_methods!();

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
            seed: NodeSeed::default(),
        }
    }

    /// Set the scrollbar thickness (cells across the minor axis).
    ///
    /// For a vertical bar this is its width; for a horizontal bar its height.
    /// The runtime drives this from the CSS-resolved `scrollbar-size` lane so a
    /// `scrollbar-size: H V` host paints a V-wide vertical / H-tall horizontal
    /// bar instead of the hardcoded creation default.
    pub fn set_thickness(&mut self, thickness: usize) {
        self.thickness = thickness.max(1);
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

    pub fn axis(&self) -> ScrollbarAxis {
        if self.vertical {
            ScrollbarAxis::Vertical
        } else {
            ScrollbarAxis::Horizontal
        }
    }
}

impl crate::widgets::Focus for ScrollBar {
    fn focusable(&self) -> bool {
        false
    }

    fn can_focus_children(&self) -> bool {
        false
    }
}

impl crate::widgets::Interactive for ScrollBar {
    fn on_event(&mut self, event: &Event, ctx: &mut crate::event::WidgetCtx) {
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
                    ctx.post_message(ScrollbarScrollTo {
                        axis: self.axis(),
                        offset: clamped as f32,
                        animate: true,
                        scroll_duration: None,
                    });
                }
                ctx.set_handled();
            }
            Event::MouseMove(MouseMoveEvent {
                target,
                x,
                y,
                screen_x,
                screen_y,
                ..
            }) if *target == self.node_id() && self.grabbed => {
                let screen_pointer = if self.vertical {
                    *screen_y as usize
                } else {
                    *screen_x as usize
                };
                let local_pointer = if self.vertical {
                    *y as usize
                } else {
                    *x as usize
                };
                let max_pos = max_offset(self.window_virtual_size, self.window_size.max(1)) as f32;
                let scale = self.window_virtual_size as f32 / self.window_size.max(1) as f32;
                let delta = screen_pointer as f32 - self.grab_anchor_screen as f32;
                let gain = THUMB_DRAG_GAIN_FIXED;
                let mut next_pos =
                    quantize_drag_position(self.grabbed_position + delta * scale * gain)
                        .clamp(0.0, max_pos);
                let track_len = self.track_len.max(1);
                if local_pointer == 0 {
                    next_pos = 0.0;
                } else if local_pointer >= track_len.saturating_sub(1) {
                    next_pos = max_pos;
                }
                if (next_pos - self.position).abs() > f32::EPSILON {
                    self.position = next_pos;
                    ctx.post_message(ScrollbarScrollTo {
                        axis: self.axis(),
                        offset: next_pos,
                        animate: true,
                        scroll_duration: None,
                    });
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

    fn on_node_state_changed(
        &mut self,
        _old: crate::widgets::NodeState,
        new: crate::widgets::NodeState,
    ) {
        self.mouse_over = new.hovered;
    }
}

impl crate::widgets::Render for ScrollBar {
    fn render(&self, _console: &rich_rs::Console, options: &rich_rs::ConsoleOptions) -> Segments {
        let length = if self.vertical {
            options.size.1.max(1)
        } else {
            options.size.0.max(1)
        };

        // Scrollbar color/background tokens are read from the HOST widget's
        // resolved styles, not the scrollbar's own. Mirrors Python `ScrollBar`
        // (textual/src/textual/scrollbar.py:282-294): `styles = self.parent.styles`
        // and `base_background, _ = self.parent.background_colors`. These tokens
        // are NOT inherited, so a dedicated scrollbar must read its host's styles.
        // Fall back to the scrollbar's own resolved style only if there is no host
        // (e.g. off-tree rendering).
        let resolved = crate::css::current_host_style()
            .or_else(crate::css::current_self_style)
            .unwrap_or_default();
        let style = &resolved;
        let base_bg = style
            .bg
            .or_else(crate::css::current_composited_background)
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
        // Python `_Styled(renderable, rich_style)` applies the host widget's
        // `color` (fg) to ALL segments from the scrollbar render, including track
        // whitespace. Bake it into the track style so `apply_style_to_segments`
        // sees s.color.is_some() and does not overwrite it.
        //
        // The host `color` (e.g. `color: blue 80%`) is composited over the host's
        // BASE BACKGROUND surface (Python `background_colors[0]`), NOT over the
        // scrollbar's own track background. Flatten it here over `base_bg` so a
        // semi-transparent host color resolves against the real surface (white in
        // `scrollbar_size`'s `background: white`) rather than the dark scrollbar
        // track (`#0000cc` regression). Already-opaque colors are unchanged.
        let track_fg = resolved.fg.map(|fg| fg.flatten_over(base_bg));
        let lines = renderer.render_bar(length, bg, thumb, track_fg);
        // NOTE: line-break between ROWS, so the bound is the number of rendered
        // rows — NOT `length`. For a vertical bar rows == `length` (track_len),
        // but for a horizontal bar rows == `thickness` while `length` is the
        // track width, so using `length` here emitted a spurious trailing line
        // break (and would skip breaks if thickness > length).
        let row_count = lines.len();
        let mut out = Segments::new();
        for (idx, line) in lines.into_iter().enumerate() {
            out.extend(line);
            if idx + 1 < row_count {
                out.push(Segment::line());
            }
        }
        out
    }
}

#[widget(Focus)]
pub struct ScrollBarCorner {
    pub(crate) seed: NodeSeed,
}

impl ScrollBarCorner {
    crate::seed_ident_methods!();

    pub fn new() -> Self {
        Self {
            seed: NodeSeed::default(),
        }
    }
}

impl Default for ScrollBarCorner {
    fn default() -> Self {
        Self::new()
    }
}

impl crate::widgets::Focus for ScrollBarCorner {
    fn focusable(&self) -> bool {
        false
    }

    fn can_focus_children(&self) -> bool {
        false
    }
}

impl crate::widgets::Render for ScrollBarCorner {
    fn render(&self, _console: &rich_rs::Console, options: &rich_rs::ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);
        // Corner color is read from the HOST widget's resolved styles, not the
        // corner's own. Mirrors Python `ScrollBarCorner` (scrollbar.py:408-410):
        // `styles = self.parent.styles; color = styles.scrollbar_corner_color`.
        let resolved = crate::css::current_host_style()
            .or_else(crate::css::current_self_style)
            .unwrap_or_default();
        let style = &resolved;
        let base_bg = style
            .bg
            .or_else(crate::css::current_composited_background)
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EventCtx;
    use crate::event::{MouseDownEvent, MouseMoveEvent, MouseUpEvent};

    fn render_glyphs(
        vertical: bool,
        track_len: usize,
        virtual_size: usize,
        window_size: usize,
        position: f32,
    ) -> Vec<(String, bool, bool, bool)> {
        // Returns (glyph, has_color, has_bgcolor, reverse) per cell of the first line.
        let renderer = ScrollBarRender {
            virtual_size,
            window_size,
            position,
            thickness: 1,
            vertical,
        };
        let lines = renderer.render_bar(track_len, Color::rgb(85, 85, 85), Color::rgb(255, 0, 255), None);
        // Vertical: one cell per line (one cell per track index). Horizontal:
        // first line holds the whole row.
        let cells: Vec<&Segment> = if vertical {
            lines.iter().map(|line| &line[0]).collect()
        } else {
            lines[0].iter().collect()
        };
        cells
            .into_iter()
            .map(|seg| {
                let style = seg.style.unwrap_or_default();
                (
                    seg.text.to_string(),
                    style.color.is_some(),
                    style.bgcolor.is_some(),
                    style.reverse == Some(true),
                )
            })
            .collect()
    }

    #[test]
    fn render_bar_vertical_matches_python_fractional_edges() {
        // Verified against Python ScrollBarRender.render_bar(size=10, virtual_size=50,
        // window_size=20, position=7, vertical=True): start cell '▅' (no reverse),
        // reversed-fill body, tail cell '▅' (reverse).
        let cells = render_glyphs(true, 10, 50, 20, 7.0);
        let glyphs: Vec<&str> = cells.iter().map(|c| c.0.as_str()).collect();
        let expected = [" ", "▅", " ", " ", " ", "▅", " ", " ", " ", " "];
        assert_eq!(glyphs, expected);

        // Track cells: bg set, no color, no reverse.
        assert_eq!(cells[0], (" ".to_string(), false, true, false));
        // Start partial '▅': color + bg, NOT reversed (vertical head).
        assert_eq!(cells[1], ("▅".to_string(), true, true, false));
        // Body fill: color only, reversed (Python `color=bar, reverse=True`).
        assert_eq!(cells[2], (" ".to_string(), true, false, true));
        assert_eq!(cells[3], (" ".to_string(), true, false, true));
        assert_eq!(cells[4], (" ".to_string(), true, false, true));
        // Tail partial '▅': color + bg, reversed (vertical tail).
        assert_eq!(cells[5], ("▅".to_string(), true, true, true));
        // Trailing track.
        assert_eq!(cells[6], (" ".to_string(), false, true, false));
    }

    #[test]
    fn render_bar_horizontal_matches_python_fractional_edges() {
        // Verified against Python render_bar(size=10, virtual_size=50, window_size=20,
        // position=7, vertical=False): start cell '▍' (reverse), body reversed,
        // tail cell '▍' (no reverse).
        let cells = render_glyphs(false, 10, 50, 20, 7.0);
        let glyphs: Vec<&str> = cells.iter().map(|c| c.0.as_str()).collect();
        let expected = [" ", "▍", " ", " ", " ", "▍", " ", " ", " ", " "];
        assert_eq!(glyphs, expected);
        // Head '▍': color + bg, reversed (horizontal head).
        assert_eq!(cells[1], ("▍".to_string(), true, true, true));
        // Body: color only, reversed.
        assert_eq!(cells[2], (" ".to_string(), true, false, true));
        // Tail '▍': color + bg, NOT reversed (horizontal tail).
        assert_eq!(cells[5], ("▍".to_string(), true, true, false));
    }

    #[test]
    fn render_bar_top_skips_space_partials() {
        // Verified against Python render_bar(size=10, vs=50, ws=20, position=0):
        // start_bar=0 -> bars[7]=' ' (skipped), top 4 cells are pure reversed fill,
        // tail end_bar=0 -> ' ' (skipped) -> plain track.
        let cells = render_glyphs(true, 10, 50, 20, 0.0);
        let glyphs: Vec<&str> = cells.iter().map(|c| c.0.as_str()).collect();
        assert_eq!(glyphs, vec![" "; 10]);
        // First four cells: reversed fill (color only).
        for cell in cells.iter().take(4) {
            assert_eq!(*cell, (" ".to_string(), true, false, true));
        }
        // Remaining: plain track.
        for cell in cells.iter().skip(4) {
            assert_eq!(*cell, (" ".to_string(), false, true, false));
        }
    }

    fn glyph_string(vertical: bool, size: usize, vs: usize, ws: usize, pos: f32) -> String {
        render_glyphs(vertical, size, vs, ws, pos)
            .iter()
            .map(|c| {
                let g = c.0.as_str();
                if g.trim().is_empty() { " " } else { g }
            })
            .collect()
    }

    #[test]
    fn render_bar_glyph_strings_match_python_across_positions() {
        // Expected strings captured from Python ScrollBarRender.render_bar
        // (textual/src/textual/scrollbar.py) for size=12, vs=80, ws=30.
        let vertical = [
            (0.0, "    ▄       "),
            (3.0, "▅   ▁       "),
            (7.0, "     ▄      "),
            (13.0, " ▁    ▅     "),
            (21.0, "   ▇   ▃    "),
            (30.0, "    ▄       "),
        ];
        for (pos, expected) in vertical {
            assert_eq!(
                glyph_string(true, 12, 80, 30, pos),
                expected,
                "vertical pos={pos}"
            );
        }
        let horizontal = [
            (0.0, "    ▌       "),
            (3.0, "▍   ▉       "),
            (7.0, "     ▌      "),
            (13.0, " ▉    ▍     "),
            (21.0, "   ▏   ▋    "),
            (30.0, "    ▌       "),
        ];
        for (pos, expected) in horizontal {
            assert_eq!(
                glyph_string(false, 12, 80, 30, pos),
                expected,
                "horizontal pos={pos}"
            );
        }
    }

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
    fn hidden_visibility_reserves_lane_but_suppresses_paint() {
        // Python parity: `scrollbar-visibility: hidden` reserves the gutter
        // (`Widget._refresh_scrollbars` keys `show_*` off overflow only) and
        // merely declines to PAINT the chrome (`_compositor` adds chrome only
        // when `scrollbar_visibility == "visible"`). The lane must stay reserved
        // so content width / layout is unchanged; only the bar glyphs disappear.
        let visible = ScrollbarPolicy {
            overflow_x: Overflow::Auto,
            overflow_y: Overflow::Auto,
            visibility: ScrollbarVisibility::Visible,
            gutter: ScrollbarGutter::Auto,
            vertical_size: 2,
            horizontal_size: 1,
        };
        let hidden = ScrollbarPolicy {
            visibility: ScrollbarVisibility::Hidden,
            ..visible
        };

        // Overflowing content: lane geometry is IDENTICAL between visible and
        // hidden; only the paint flag differs.
        let g_vis = visible.resolve(80, 20, 80, 60);
        let g_hid = hidden.resolve(80, 20, 80, 60);
        assert_eq!(g_vis.vertical_lane_width, g_hid.vertical_lane_width);
        assert_eq!(g_vis.viewport_width, g_hid.viewport_width);
        assert!(g_hid.show_vertical, "hidden visibility still reserves the lane");
        assert_eq!(g_hid.vertical_lane_width, 2);
        assert_eq!(g_hid.viewport_width, 78);
        assert!(g_vis.paint_vertical, "visible scrollbar is painted");
        assert!(!g_hid.paint_vertical, "hidden scrollbar is NOT painted");

        // `scrollbar-visibility: visible` does NOT force the lane to show. With
        // NO overflow + auto overflow, the bar does not show (Python's
        // `_refresh_scrollbars` keys off overflow only).
        let no_overflow = visible.resolve(80, 20, 80, 10);
        assert!(
            !no_overflow.show_vertical,
            "visible visibility must not force-show a non-overflowing auto lane"
        );
        assert_eq!(no_overflow.vertical_lane_width, 0);

        // `scrollbar-gutter: stable` + no overflow still RESERVES the lane (width
        // 2) even though the bar is not shown — Python `scrollbar_size_vertical`
        // returns the full size whenever gutter==stable and overflow==auto.
        let stable = ScrollbarPolicy {
            gutter: ScrollbarGutter::Stable,
            ..visible
        };
        let g_stable = stable.resolve(80, 20, 80, 10);
        assert_eq!(
            g_stable.vertical_lane_width, 2,
            "stable gutter reserves the lane even with no overflow"
        );
        assert!(
            !g_stable.show_vertical,
            "stable gutter reserves the lane but the bar is not shown without overflow"
        );
        assert_eq!(g_stable.viewport_width, 78);

        // Auto (the genuinely-overflowing default) still paints.
        let auto = ScrollbarPolicy {
            visibility: ScrollbarVisibility::Auto,
            ..visible
        };
        let g_auto = auto.resolve(80, 20, 80, 60);
        assert!(g_auto.paint_vertical);
        assert_eq!(g_auto.vertical_lane_width, 2);
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
            paint_vertical: true,
            paint_horizontal: true,
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
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            bar.on_event(
            &Event::MouseUp(MouseUpEvent {
                target: None,
                screen_x: 0,
                screen_y: 0,
                x: 0,
                y: 0,
            }),
            &mut __w);
        }
        assert!(!bar.grabbed);
        assert_eq!(bar.grab_offset, 0);
        assert_eq!(bar.grab_anchor_screen, 0);
    }

    #[test]
    fn drag_position_quantizes_to_eighth_cells() {
        assert!((quantize_drag_position(1.24) - 1.125).abs() < f32::EPSILON);
        assert!((quantize_drag_position(1.25) - 1.25).abs() < f32::EPSILON);
        assert!((quantize_drag_position(1.37) - 1.25).abs() < f32::EPSILON);
    }

    #[test]
    fn fixed_gain_uses_constant() {
        assert!((THUMB_DRAG_GAIN_FIXED - 0.7).abs() < f32::EPSILON);
    }

    #[test]
    fn vertical_thumb_drag_one_row_matches_wheel_step_with_fixed_gain() {
        // Match the modal01 scenario from logs: content=50, viewport=34.
        let mut bar = ScrollBar::new(true, 2);
        let id = bar.node_id();
        bar.set_window_virtual_size(50);
        bar.set_window_size(34);
        bar.on_layout(2, 34);

        // Start drag on thumb at top.
        let mut down_ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut down_ctx);
            bar.on_event(
            &Event::MouseDown(MouseDownEvent {
                target: id,
                screen_x: 0,
                screen_y: 0,
                x: 0,
                y: 0,
            }),
            &mut __w);
        }
        assert!(down_ctx.handled());

        // Move pointer by one terminal row.
        let mut move_ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut move_ctx);
            bar.on_event(
            &Event::MouseMove(MouseMoveEvent {
                target: id,
                screen_x: 0,
                screen_y: 1,
                x: 0,
                y: 1,
            }),
            &mut __w);
        }
        assert!(move_ctx.handled());

        let messages = move_ctx.take_messages();
        let mut emitted_offset = None;
        let mut emitted_animate = None;
        for msg in messages {
            if let Some(payload) = msg.downcast_ref::<ScrollbarScrollTo>() {
                emitted_offset = Some(payload.offset);
                emitted_animate = Some(payload.animate);
            }
        }
        let offset = emitted_offset.expect("expected drag to emit app-root scroll message");
        assert_eq!(emitted_animate, Some(true));
        // 1 row drag at scale 50/34, with fixed gain 0.7, quantized to 1/8.
        // Effective target is ~1 line per row (wheel parity).
        assert!((offset - 1.0).abs() < f32::EPSILON);
    }

    /// The scrollbar reads its color tokens from the HOST widget's resolved
    /// style (Python `self.parent.styles.scrollbar_*`), not its own. With a host
    /// style carrying `scrollbar-color`, the rendered thumb must use that color.
    #[test]
    fn scrollbar_color_resolves_from_host_style() {
        use crate::css::SelectorMeta;
        let mut host = Style::default();
        host.scrollbar_color = Some(Color::rgb(0, 255, 255));
        host.scrollbar_background = Some(Color::rgb(0, 0, 255));
        let host_meta = SelectorMeta::new("Screen".to_string(), None, Vec::new());

        let mut bar = ScrollBar::new(true, 1);
        bar.set_window_virtual_size(100);
        bar.set_window_size(20);
        bar.on_layout(1, 20);
        let self_meta = SelectorMeta::new("ScrollBar".to_string(), None, Vec::new());

        let console = rich_rs::Console::new();
        let mut options = console.options().clone();
        options.size = (1, 20);
        options.max_width = 1;
        options.max_height = 20;

        // Push host then self, mirroring the render-time stack order.
        crate::css::push_style_context(host_meta, host);
        crate::css::push_style_context(self_meta, Style::default());
        let segments = Widget::render(&bar, &console, &options);
        crate::css::pop_style_context();
        crate::css::pop_style_context();

        // The thumb body is painted with `reverse=true` and `color = thumb`.
        let cyan = Color::rgb(0, 255, 255).to_simple_opaque();
        let blue = Color::rgb(0, 0, 255).to_simple_opaque();
        let thumb_seg = segments
            .iter()
            .find(|s| s.style.and_then(|st| st.color) == Some(cyan))
            .expect("thumb should use host scrollbar-color (cyan)");
        assert_eq!(thumb_seg.style.and_then(|s| s.reverse), Some(true));
        // Track cells use the host scrollbar-background (blue).
        assert!(
            segments
                .iter()
                .any(|s| s.style.and_then(|st| st.bgcolor) == Some(blue)),
            "track should use host scrollbar-background (blue)"
        );
    }

    /// Regression: the host `color` baked into the track foreground must be
    /// composited over the host's BASE BACKGROUND surface (Python
    /// `background_colors[0]`), NOT over the scrollbar's own (dark) track
    /// background. `scrollbar_size` sets `background: white; color: blue 80%`, so
    /// the track fg must be `blue 80%` over white (#3333ff) — previously it was
    /// flattened over the dark scrollbar track (#0000cc), a parity regression.
    #[test]
    fn track_fg_composites_over_host_base_background_not_track() {
        use crate::css::SelectorMeta;
        let mut host = Style::default();
        // background: white; color: blue 80%
        host.bg = Some(Color::rgb(255, 255, 255));
        host.fg = Some(Color::rgba_f(0, 0, 255, 0.8));
        // scrollbar-background: a dark surface (so the regression would show).
        host.scrollbar_background = Some(Color::rgb(0, 0, 0));
        let host_meta = SelectorMeta::new("Screen".to_string(), None, Vec::new());

        let mut bar = ScrollBar::new(true, 1);
        bar.set_window_virtual_size(100);
        bar.set_window_size(20);
        bar.on_layout(1, 20);
        let self_meta = SelectorMeta::new("ScrollBar".to_string(), None, Vec::new());

        let console = rich_rs::Console::new();
        let mut options = console.options().clone();
        options.size = (1, 20);
        options.max_width = 1;
        options.max_height = 20;

        crate::css::push_style_context(host_meta, host);
        crate::css::push_style_context(self_meta, Style::default());
        let segments = Widget::render(&bar, &console, &options);
        crate::css::pop_style_context();
        crate::css::pop_style_context();

        // blue(0,0,255) at 80% over white(255,255,255) = (51,51,255) = #3333ff.
        let expected = Color::rgb(51, 51, 255).to_simple_opaque();
        let wrong = Color::rgb(0, 0, 204).to_simple_opaque(); // blue 80% over black
        let track_seg = segments
            .iter()
            .find(|s| {
                let st = s.style.unwrap_or_default();
                // Track cells carry the host fg color AND a bgcolor (no reverse).
                st.color.is_some() && st.bgcolor.is_some() && st.reverse != Some(true)
            })
            .expect("expected a track cell carrying the host fg color");
        let fg = track_seg.style.and_then(|s| s.color);
        assert_ne!(fg, Some(wrong), "track fg must NOT flatten over the dark track");
        assert_eq!(
            fg,
            Some(expected),
            "track fg must be host color (blue 80%) over base background (white)"
        );
    }

    /// Regression: a vertical ScrollBar must paint glyphs `thickness` cells wide.
    /// The runtime drives `set_thickness` from the CSS-resolved `scrollbar-size`
    /// lane (e.g. `scrollbar-size: 10 4` → vertical lane width 4); previously the
    /// thickness stayed at the creation default (2) regardless of CSS, so a 4-wide
    /// lane was painted only 2 cells wide (styles/scrollbar_size parity gap).
    #[test]
    fn scrollbar_thickness_drives_vertical_glyph_width() {
        let mut bar = ScrollBar::new(true, 2);
        // Simulate the runtime applying the CSS-resolved vertical lane width.
        bar.set_thickness(4);
        bar.set_window_virtual_size(100);
        bar.set_window_size(20);
        bar.on_layout(4, 20);

        let console = rich_rs::Console::new();
        let mut options = console.options().clone();
        options.size = (4, 20);
        options.max_width = 4;
        options.max_height = 20;

        let segments = Widget::render(&bar, &console, &options);
        // Every painted glyph segment (track blank or thumb glyph) on a vertical
        // bar repeats `thickness` graphemes; with thickness 4 each cell-run is 4
        // columns wide. Assert no rendered segment is narrower than 4 columns.
        let widest = segments
            .iter()
            .filter(|s| !s.text.is_empty() && s.text != "\n")
            .map(|s| s.text.chars().count())
            .max()
            .expect("scrollbar should emit at least one glyph segment");
        assert_eq!(
            widest, 4,
            "vertical scrollbar glyphs must be `thickness` (4) cells wide"
        );
    }

    /// Mirror of the above for a horizontal bar: thickness governs the number of
    /// stacked rows (`scrollbar-size` horizontal lane height).
    #[test]
    fn scrollbar_thickness_drives_horizontal_row_count() {
        let mut bar = ScrollBar::new(false, 1);
        bar.set_thickness(3);
        bar.set_window_virtual_size(100);
        bar.set_window_size(20);
        bar.on_layout(20, 3);

        let console = rich_rs::Console::new();
        let mut options = console.options().clone();
        options.size = (20, 3);
        options.max_width = 20;
        options.max_height = 3;

        let segments = Widget::render(&bar, &console, &options);
        // A horizontal bar duplicates its row `thickness` times, separated by
        // `Segment::line()`. Count the line separators: thickness rows => 2 lines.
        let line_breaks = segments.iter().filter(|s| s.text == "\n").count();
        assert_eq!(
            line_breaks, 2,
            "horizontal scrollbar must stack `thickness` (3) rows (2 line breaks)"
        );
    }
}
