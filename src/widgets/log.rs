use std::sync::atomic::{AtomicUsize, Ordering};

use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::event::{Action, Event, EventCtx};
use crate::message::Message;

use super::helpers::{adjust_line_length_no_bg, empty_classes, fixed_height_from_constraints};
use super::{ScrollView, Widget, WidgetId, WidgetStyles};

#[derive(Debug)]
pub struct Log {
    id: WidgetId,
    lines: Vec<String>,
    max_lines: Option<usize>,
    auto_scroll: bool,
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
    max_line_width: usize,
    styles: WidgetStyles,
}

impl Log {
    pub fn new() -> Self {
        Self {
            id: WidgetId::new(),
            lines: Vec::new(),
            max_lines: None,
            auto_scroll: true,
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
            max_line_width: 0,
            styles: WidgetStyles::default(),
        }
    }

    pub fn max_lines(mut self, max_lines: usize) -> Self {
        self.max_lines = Some(max_lines.max(1));
        self.prune_max_lines();
        self
    }

    pub fn auto_scroll(mut self, auto_scroll: bool) -> Self {
        self.auto_scroll = auto_scroll;
        self
    }

    pub fn scroll_step(mut self, step: usize) -> Self {
        self.scroll_step = step.max(1);
        self
    }

    pub fn line_count(&self) -> usize {
        if self.lines.is_empty() {
            0
        } else {
            self.lines.len() - usize::from(self.lines.last().is_some_and(|line| line.is_empty()))
        }
    }

    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    pub fn write(&mut self, data: impl AsRef<str>) -> &mut Self {
        let data = data.as_ref();
        if data.is_empty() {
            return self;
        }
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }

        for chunk in data.split_inclusive('\n') {
            if let Some(stripped) = chunk.strip_suffix('\n') {
                if let Some(current) = self.lines.last_mut() {
                    current.push_str(stripped);
                    self.max_line_width = self.max_line_width.max(Self::processed_width(current));
                }
                self.lines.push(String::new());
            } else if let Some(current) = self.lines.last_mut() {
                current.push_str(chunk);
                self.max_line_width = self.max_line_width.max(Self::processed_width(current));
            }
        }

        self.prune_max_lines();
        if self.auto_scroll {
            self.scroll_end();
        } else {
            self.clamp_offset();
        }
        self
    }

    pub fn write_line(&mut self, line: impl Into<String>) -> &mut Self {
        self.write_lines([line.into()])
    }

    pub fn write_lines<I, S>(&mut self, lines: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut appended_any = false;
        for line in lines {
            for split in line.as_ref().lines() {
                self.lines.push(split.to_string());
                self.max_line_width = self.max_line_width.max(Self::processed_width(split));
                appended_any = true;
            }
        }
        if !appended_any {
            return self;
        }

        self.prune_max_lines();
        if self.auto_scroll {
            self.scroll_end();
        } else {
            self.clamp_offset();
        }
        self
    }

    pub fn clear(&mut self) -> &mut Self {
        self.lines.clear();
        self.max_line_width = 0;
        self.offset_y = 0;
        self.content_height.store(1, Ordering::Relaxed);
        self
    }

    fn prune_max_lines(&mut self) {
        if let Some(max_lines) = self.max_lines {
            if self.lines.len() > max_lines {
                let remove_lines = self.lines.len() - max_lines;
                self.lines.drain(0..remove_lines);
                self.offset_y = self.offset_y.saturating_sub(remove_lines);
                self.max_line_width = self
                    .lines
                    .iter()
                    .map(|line| Self::processed_width(line))
                    .max()
                    .unwrap_or(0);
            }
        }
    }

    fn display_line_count(&self) -> usize {
        self.line_count().max(1)
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
            self.display_line_count(),
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

    fn processed_width(line: &str) -> usize {
        rich_rs::cell_len(&Self::process_line(line))
    }

    fn process_line(line: &str) -> String {
        let expanded = Self::expand_tabs(line, 8);
        expanded
            .chars()
            .map(|ch| {
                if ('\u{0000}'..='\u{0014}').contains(&ch) {
                    '\u{FFFD}'
                } else {
                    ch
                }
            })
            .collect()
    }

    fn expand_tabs(line: &str, tab_size: usize) -> String {
        if !line.contains('\t') {
            return line.to_string();
        }
        let mut out = String::with_capacity(line.len());
        let mut col = 0usize;
        for ch in line.chars() {
            if ch == '\t' {
                let to_next_tab = tab_size - (col % tab_size);
                out.extend(std::iter::repeat(' ').take(to_next_tab));
                col += to_next_tab;
            } else {
                out.push(ch);
                col += unicode_width::UnicodeWidthChar::width(ch)
                    .unwrap_or(0)
                    .max(1);
            }
        }
        out
    }
}

impl Widget for Log {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);
        self.widget_width.store(width, Ordering::Relaxed);
        self.widget_height.store(height, Ordering::Relaxed);
        const V_SCROLLBAR_SIZE: usize = 2;

        let mut viewport_width = width;
        let mut content_height = self.display_line_count();
        let mut show_scrollbar = content_height > height;
        let mut scrollbar_size = 0usize;
        if show_scrollbar && width > 1 {
            scrollbar_size = V_SCROLLBAR_SIZE.min(width.saturating_sub(1)).max(1);
            viewport_width = width.saturating_sub(scrollbar_size);
            content_height = self.display_line_count();
            show_scrollbar = content_height > height;
            if !show_scrollbar {
                scrollbar_size = 0;
            }
        }

        self.viewport_height.store(height, Ordering::Relaxed);
        self.content_height.store(content_height, Ordering::Relaxed);

        let max_offset = content_height.saturating_sub(height);
        let offset = self.offset_y.min(max_offset);
        let start = offset.min(content_height);
        let end = (start + height).min(content_height);

        let display_count = self.line_count();
        let mut rows: Vec<Vec<Segment>> = Vec::with_capacity(height);
        for index in start..end {
            let line = if index < display_count {
                Self::process_line(&self.lines[index])
            } else {
                String::new()
            };
            rows.push(adjust_line_length_no_bg(
                &[Segment::new(line)],
                viewport_width.max(1),
            ));
        }
        while rows.len() < height {
            rows.push(vec![Segment::new(" ".repeat(viewport_width.max(1)))]);
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

    fn content_width(&self) -> Option<usize> {
        Some(self.max_line_width.max(1))
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

impl Renderable for Log {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[cfg(test)]
mod tests {
    use super::Log;
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
        let mut log = Log::new().auto_scroll(false);
        log.write_lines(["line 1", "line 2", "line 3"]);
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
