use std::sync::atomic::{AtomicUsize, Ordering};
use unicode_width::UnicodeWidthChar;

use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments, Text};

use crate::event::{Action, Event, EventCtx};
use crate::message::Message;

use super::helpers::{adjust_line_length_no_bg, empty_classes, fixed_height_from_constraints};
use super::{ScrollView, Widget, WidgetId, WidgetStyles};

#[derive(Debug)]
pub struct RichLog {
    id: WidgetId,
    lines: Vec<LogLine>,
    max_lines: Option<usize>,
    auto_scroll: bool,
    wrap: bool,
    scroll_step: usize,
    offset_y: usize,
    focused: bool,
    classes: Vec<String>,
    focused_classes: Vec<String>,
    content_height: AtomicUsize,
    viewport_height: AtomicUsize,
    widget_width: AtomicUsize,
    widget_height: AtomicUsize,
    drag_v: Option<usize>,
    styles: WidgetStyles,
}

#[derive(Debug, Clone)]
enum LogLine {
    Plain(String),
    Styled(Vec<Segment>),
}

impl RichLog {
    pub fn new() -> Self {
        Self {
            id: WidgetId::new(),
            lines: Vec::new(),
            max_lines: None,
            auto_scroll: true,
            wrap: false,
            scroll_step: 1,
            offset_y: 0,
            focused: false,
            classes: Vec::new(),
            focused_classes: vec!["-focus".to_string()],
            content_height: AtomicUsize::new(1),
            viewport_height: AtomicUsize::new(1),
            widget_width: AtomicUsize::new(1),
            widget_height: AtomicUsize::new(1),
            drag_v: None,
            styles: WidgetStyles::default(),
        }
    }

    pub fn max_lines(mut self, max_lines: usize) -> Self {
        self.max_lines = Some(max_lines.max(1));
        self.trim_to_max_lines();
        self
    }

    pub fn auto_scroll(mut self, auto_scroll: bool) -> Self {
        self.auto_scroll = auto_scroll;
        self
    }

    pub fn wrap(mut self, wrap: bool) -> Self {
        self.wrap = wrap;
        self
    }

    pub fn scroll_step(mut self, step: usize) -> Self {
        self.scroll_step = step.max(1);
        self
    }

    pub fn write(&mut self, content: impl Into<String>) -> &mut Self {
        let content = content.into();
        if content.is_empty() {
            self.lines.push(LogLine::Plain(String::new()));
        } else {
            self.lines.extend(
                content
                    .split('\n')
                    .map(std::string::ToString::to_string)
                    .map(LogLine::Plain),
            );
        }
        self.trim_to_max_lines();
        if self.auto_scroll {
            self.scroll_end();
        } else {
            self.clamp_offset();
        }
        self
    }

    pub fn write_segments(&mut self, segments: Vec<Segment>) -> &mut Self {
        self.lines.push(LogLine::Styled(segments));
        self.trim_to_max_lines();
        if self.auto_scroll {
            self.scroll_end();
        } else {
            self.clamp_offset();
        }
        self
    }

    pub fn write_debug<T: std::fmt::Debug>(&mut self, value: T) -> &mut Self {
        self.write(format!("{value:?}"))
    }

    pub fn clear(&mut self) -> &mut Self {
        self.lines.clear();
        self.offset_y = 0;
        self.content_height.store(1, Ordering::Relaxed);
        self
    }

    fn trim_to_max_lines(&mut self) {
        if let Some(max_lines) = self.max_lines {
            if self.lines.len() > max_lines {
                let excess = self.lines.len() - max_lines;
                self.lines.drain(0..excess);
                self.offset_y = self.offset_y.saturating_sub(excess);
            }
        }
    }

    fn max_offset(&self) -> usize {
        ScrollView::line_max_offset(
            self.content_height.load(Ordering::Relaxed).max(1),
            self.viewport_height.load(Ordering::Relaxed).max(1),
        )
    }

    fn clamp_offset(&mut self) {
        self.offset_y = ScrollView::line_clamp_offset(
            self.offset_y,
            self.content_height.load(Ordering::Relaxed).max(1),
            self.viewport_height.load(Ordering::Relaxed).max(1),
        );
    }

    fn scroll_end(&mut self) {
        self.offset_y = ScrollView::line_scroll_end(
            self.lines
                .len()
                .max(self.content_height.load(Ordering::Relaxed)),
            self.viewport_height.load(Ordering::Relaxed).max(1),
        );
    }

    fn scroll_by(&mut self, delta: i32) {
        self.offset_y = ScrollView::line_scroll_by(
            self.offset_y,
            delta,
            self.content_height.load(Ordering::Relaxed).max(1),
            self.viewport_height.load(Ordering::Relaxed).max(1),
        );
    }

    fn emit_scroll_changed_message(&self, ctx: &mut EventCtx) {
        ctx.post_message(
            self.id,
            Message::RichLogScrolled {
                offset: self.offset_y,
                max_offset: self.max_offset(),
            },
        );
    }

    fn physical_lines(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        width: usize,
    ) -> Vec<Vec<Segment>> {
        if self.lines.is_empty() {
            return vec![vec![Segment::new(String::new())]];
        }

        let mut out: Vec<Vec<Segment>> = Vec::new();
        for line in &self.lines {
            match line {
                LogLine::Plain(content) => {
                    if self.wrap {
                        for wrapped in wrap_line(content, width.max(1)) {
                            let rendered = Text::plain(wrapped).render(console, options);
                            let split =
                                Segment::split_and_crop_lines(rendered, width, None, true, false);
                            if let Some(first) = split.first() {
                                out.push(first.clone());
                            } else {
                                out.push(vec![Segment::new(String::new())]);
                            }
                        }
                    } else {
                        let rendered = Text::plain(content).render(console, options);
                        let split =
                            Segment::split_and_crop_lines(rendered, width, None, true, false);
                        if let Some(first) = split.first() {
                            out.push(first.clone());
                        } else {
                            out.push(vec![Segment::new(String::new())]);
                        }
                    }
                }
                LogLine::Styled(segments) => {
                    let split =
                        Segment::split_and_crop_lines(segments.clone(), width, None, true, false);
                    if split.is_empty() {
                        out.push(vec![Segment::new(String::new())]);
                    } else {
                        out.extend(split);
                    }
                }
            }
        }

        if out.is_empty() {
            out.push(vec![Segment::new(String::new())]);
        }
        out
    }
}

impl Widget for RichLog {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);
        self.widget_width.store(width, Ordering::Relaxed);
        self.widget_height.store(height, Ordering::Relaxed);
        const V_SCROLLBAR_SIZE: usize = 2;

        let mut viewport_width = width;
        let mut physical = self.physical_lines(console, options, viewport_width);
        let mut content_height = physical.len().max(1);
        let mut show_scrollbar = content_height > height;
        let mut scrollbar_size = 0usize;
        if show_scrollbar && width > 1 {
            scrollbar_size = V_SCROLLBAR_SIZE.min(width.saturating_sub(1)).max(1);
            viewport_width = width.saturating_sub(scrollbar_size);
            physical = self.physical_lines(console, options, viewport_width);
            content_height = physical.len().max(1);
            show_scrollbar = content_height > height;
            if !show_scrollbar {
                scrollbar_size = 0;
            }
        }

        self.viewport_height.store(height, Ordering::Relaxed);
        self.content_height.store(content_height, Ordering::Relaxed);

        let max_offset = content_height.saturating_sub(height);
        let offset = self.offset_y.min(max_offset);
        let start = offset.min(physical.len());
        let end = (start + height).min(physical.len());

        let mut rows: Vec<Vec<Segment>> = Vec::with_capacity(height);
        for line in &physical[start..end] {
            rows.push(adjust_line_length_no_bg(line, viewport_width));
        }
        while rows.len() < height {
            rows.push(vec![Segment::new(" ".repeat(viewport_width))]);
        }

        if show_scrollbar {
            let (track_style, thumb_style, thumb_active_style) =
                ScrollView::line_scrollbar_styles();
            let track_len = height.max(1);
            let (thumb_start, thumb_len) =
                ScrollView::line_scrollbar_thumb(track_len, content_height, height, offset);
            let mut thumb_drawn = false;
            for (row, line) in rows.iter_mut().enumerate() {
                let in_track = row < track_len;
                let active = in_track && row >= thumb_start && row < thumb_start + thumb_len;
                let style = if active {
                    if self.drag_v.is_some() {
                        thumb_active_style
                    } else {
                        thumb_style
                    }
                } else {
                    track_style
                };
                for _ in 0..scrollbar_size.max(1) {
                    line.push(Segment::styled(" ".to_string(), style));
                }
                thumb_drawn |= active;
            }
            if !thumb_drawn && !rows.is_empty() {
                let row = track_len.saturating_sub(1).min(rows.len() - 1);
                let line = &mut rows[row];
                for _ in 0..scrollbar_size.max(1) {
                    if !line.is_empty() {
                        line.pop();
                    }
                }
                let active_style = if self.drag_v.is_some() {
                    thumb_active_style
                } else {
                    thumb_style
                };
                for _ in 0..scrollbar_size.max(1) {
                    line.push(Segment::styled(" ".to_string(), active_style));
                }
            }
        }

        let mut out = Segments::new();
        for (index, row) in rows.into_iter().enumerate() {
            out.extend(row);
            if index + 1 < height {
                out.push(Segment::line());
            }
        }
        out
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

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if let Event::MouseDown(mouse) = event {
            if mouse.target == self.id {
                let width = self.widget_width.load(Ordering::Relaxed).max(1);
                let height = self.widget_height.load(Ordering::Relaxed).max(1);
                let content_height = self.content_height.load(Ordering::Relaxed).max(1);
                let show_scrollbar = content_height > height;
                let scrollbar_size = 2usize.min(width.saturating_sub(1)).max(1);
                let local_x = mouse.x as usize;
                let local_y = mouse.y as usize;
                if show_scrollbar
                    && local_x >= width.saturating_sub(scrollbar_size)
                    && local_y < height
                {
                    let (thumb_start, thumb_len) = ScrollView::line_scrollbar_thumb(
                        height,
                        content_height,
                        height,
                        self.offset_y,
                    );
                    if local_y >= thumb_start && local_y < thumb_start.saturating_add(thumb_len) {
                        self.drag_v = Some(local_y.saturating_sub(thumb_start));
                        ctx.set_handled();
                        return;
                    }
                    let before = self.offset_y;
                    if local_y < thumb_start {
                        self.scroll_by(-(height as i32));
                    } else if local_y >= thumb_start.saturating_add(thumb_len) {
                        self.scroll_by(height as i32);
                    }
                    if self.offset_y != before {
                        ctx.request_repaint();
                        self.emit_scroll_changed_message(ctx);
                    }
                    ctx.set_handled();
                    return;
                }
            }
        }

        if matches!(event, Event::MouseUp(_) | Event::AppFocus(false)) {
            let was_dragging = self.drag_v.take().is_some();
            if was_dragging {
                ctx.set_handled();
            }
        }

        if let Event::Action(action) = event {
            let before = self.offset_y;
            match action {
                Action::ScrollUp => self.scroll_by(-(self.scroll_step as i32)),
                Action::ScrollDown => self.scroll_by(self.scroll_step as i32),
                Action::ScrollPageUp => {
                    let page = self.viewport_height.load(Ordering::Relaxed).max(1);
                    self.scroll_by(-(page as i32));
                }
                Action::ScrollPageDown => {
                    let page = self.viewport_height.load(Ordering::Relaxed).max(1);
                    self.scroll_by(page as i32);
                }
                _ => return,
            }
            if self.offset_y != before {
                ctx.request_repaint();
                self.emit_scroll_changed_message(ctx);
            }
            ctx.set_handled();
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
            self.emit_scroll_changed_message(ctx);
        }
        ctx.set_handled();
    }

    fn on_mouse_move(&mut self, _x: u16, y: u16) -> bool {
        let Some(grab_offset) = self.drag_v else {
            return false;
        };
        let viewport_h = self.viewport_height.load(Ordering::Relaxed).max(1);
        let content_h = self.content_height.load(Ordering::Relaxed).max(1);
        if content_h <= viewport_h {
            return false;
        }

        let new_offset = ScrollView::line_drag_offset(
            y as usize,
            grab_offset,
            viewport_h,
            content_h,
            viewport_h,
            self.offset_y,
        );
        if new_offset != self.offset_y {
            self.offset_y = new_offset;
            return true;
        }
        false
    }

    fn layout_height(&self) -> Option<usize> {
        fixed_height_from_constraints(self.layout_constraints())
    }

    fn style_classes(&self) -> &[String] {
        if self.focused {
            &self.focused_classes
        } else if self.classes.is_empty() {
            empty_classes()
        } else {
            &self.classes
        }
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

impl Renderable for RichLog {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

fn wrap_line(line: &str, width: usize) -> Vec<String> {
    if line.is_empty() {
        return vec![String::new()];
    }

    let mut out = Vec::new();
    let mut current = String::new();
    let mut current_width = 0usize;

    for ch in line.chars() {
        let char_width = UnicodeWidthChar::width(ch).unwrap_or(0).max(1);
        if current_width + char_width > width && !current.is_empty() {
            out.push(std::mem::take(&mut current));
            current_width = 0;
        }
        current.push(ch);
        current_width += char_width;
        if current_width >= width {
            out.push(std::mem::take(&mut current));
            current_width = 0;
        }
    }

    if !current.is_empty() {
        out.push(current);
    } else if out.is_empty() {
        out.push(String::new());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::RichLog;
    use crate::event::{Action, Event, EventCtx};
    use crate::message::Message;
    use crate::widgets::Widget;
    use rich_rs::Console;

    fn options_for(console: &Console, width: usize, height: usize) -> rich_rs::ConsoleOptions {
        let mut options = console.options().clone();
        options.size = (width, height);
        options.max_width = width;
        options.max_height = height;
        options
    }

    #[test]
    fn scroll_action_posts_scrolled_message() {
        let console = Console::new();
        let options = options_for(&console, 16, 2);
        let mut log = RichLog::new().auto_scroll(false);
        log.write("line 1");
        log.write("line 2");
        log.write("line 3");
        let _ = log.render(&console, &options);

        let mut ctx = EventCtx::default();
        log.on_event(&Event::Action(Action::ScrollDown), &mut ctx);
        let messages = ctx.take_messages();
        assert!(
            messages
                .iter()
                .any(|m| matches!(m.message, Message::RichLogScrolled { .. }))
        );
    }
}
