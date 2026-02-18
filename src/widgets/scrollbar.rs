use rich_rs::{Segment, Segments};

use crate::event::{Event, EventCtx, MouseDownEvent, MouseUpEvent};
use crate::style::Color;
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

pub struct ScrollBar {
    vertical: bool,
    thickness: usize,
    window_virtual_size: usize,
    window_size: usize,
    position: f32,
    mouse_over: bool,
    grabbed: bool,
    grabbed_position: f32,
    styles: WidgetStyles,
}

impl ScrollBar {
    pub fn new(vertical: bool, thickness: usize) -> Self {
        Self {
            vertical,
            thickness: thickness.max(1),
            window_virtual_size: 100,
            window_size: 0,
            position: 0.0,
            mouse_over: false,
            grabbed: false,
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

        let bg = self
            .styles
            .style
            .scrollbar_background
            .unwrap_or_else(|| Color::rgb(40, 40, 40));
        let thumb = if self.grabbed {
            self.styles
                .style
                .scrollbar_color_active
                .unwrap_or_else(|| Color::rgb(1, 120, 212))
        } else if self.mouse_over {
            self.styles
                .style
                .scrollbar_color_hover
                .unwrap_or_else(|| Color::rgb(70, 150, 220))
        } else {
            self.styles
                .style
                .scrollbar_color
                .unwrap_or_else(|| Color::rgb(48, 156, 255))
        };
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
            Event::MouseDown(MouseDownEvent { target, .. }) if *target == self.node_id() => {
                self.grabbed = true;
                self.grabbed_position = self.position;
                ctx.set_handled();
            }
            Event::MouseUp(MouseUpEvent { target, .. })
                if target.is_some_and(|target| target == self.node_id()) =>
            {
                self.grabbed = false;
                ctx.set_handled();
            }
            Event::AppFocus(false) => {
                self.grabbed = false;
            }
            _ => {}
        }
    }

    fn on_mouse_move(&mut self, _x: u16, _y: u16) -> bool {
        let changed = !self.mouse_over;
        self.mouse_over = true;
        changed
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
        let color = self
            .styles
            .style
            .scrollbar_corner_color
            .unwrap_or_else(|| Color::rgb(40, 40, 40));
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
