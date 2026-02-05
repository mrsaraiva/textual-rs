use std::sync::Arc;
use std::time::{Duration, Instant};

use crossterm::event::KeyCode;
use rich_rs::{Console, ConsoleOptions, Segment, Segments};
use unicode_width::UnicodeWidthChar;

use crate::event::{Event, EventCtx};
use crate::style::{Color, parse_color_like};

use super::{
    Widget, WidgetId, WidgetStyles,
    helpers::{empty_classes, fixed_height_from_constraints},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Cursor {
    pub row: usize,
    pub col: usize, // byte index in line
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Selection {
    pub start: Cursor,
    pub end: Cursor,
}

impl Selection {
    pub fn cursor(pos: Cursor) -> Self {
        Self { start: pos, end: pos }
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
}

pub struct TextArea {
    id: WidgetId,
    lines: Vec<String>,
    cursor: Cursor,
    selection: Selection,
    focused: bool,
    language: Option<String>,
    code_editor: bool,
    scroll_row: usize,
    scroll_col: usize, // cell offset
    layout_w: u16,
    layout_h: u16,
    preferred_col_cells: Option<usize>,
    mouse_down: bool,
    app_active: bool,
    cursor_visible: bool,
    cursor_blink_next_at: Option<Instant>,
    classes: Vec<String>,
    focused_classes: Vec<String>,
    styles: WidgetStyles,
    on_change: Option<Arc<dyn Fn(&mut TextArea) + Send + Sync>>,
}

impl TextArea {
    const CURSOR_BLINK_PERIOD: Duration = Duration::from_millis(500);

    fn next_blink_deadline() -> Instant {
        let now = Instant::now();
        now.checked_add(Self::CURSOR_BLINK_PERIOD).unwrap_or(now)
    }

    pub fn new(text: impl Into<String>) -> Self {
        let mut out = Self {
            id: WidgetId::new(),
            lines: split_lines(text.into()),
            cursor: Cursor::default(),
            selection: Selection::cursor(Cursor::default()),
            focused: false,
            language: None,
            code_editor: false,
            scroll_row: 0,
            scroll_col: 0,
            layout_w: 1,
            layout_h: 1,
            preferred_col_cells: None,
            mouse_down: false,
            app_active: true,
            cursor_visible: false,
            cursor_blink_next_at: None,
            classes: Vec::new(),
            focused_classes: Vec::new(),
            styles: WidgetStyles::default(),
            on_change: None,
        };
        out.rebuild_classes();
        out.clamp_cursor();
        out
    }

    pub fn code_editor(text: impl Into<String>) -> Self {
        let mut out = Self::new(text);
        out.code_editor = true;
        out.rebuild_classes();
        out
    }

    pub fn with_language(mut self, language: impl Into<String>) -> Self {
        self.language = Some(language.into());
        self
    }

    pub fn selection(&self) -> Selection {
        self.selection
    }

    pub fn set_selection(&mut self, selection: Selection) {
        let start = self.clamp_cursor_pos(selection.start);
        let end = self.clamp_cursor_pos(selection.end);
        self.selection = Selection { start, end };
        self.cursor = end;
        self.preferred_col_cells = Some(self.cursor_cell_x());
        self.adjust_scroll_to_cursor();
        self.reset_blink();
    }

    pub fn with_selection(mut self, selection: Selection) -> Self {
        self.set_selection(selection);
        self
    }

    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    pub fn on_change(mut self, handler: impl Fn(&mut TextArea) + Send + Sync + 'static) -> Self {
        self.on_change = Some(Arc::new(handler));
        self
    }

    fn notify_changed(&mut self) {
        if let Some(handler) = self.on_change.clone() {
            handler(self);
        }
    }

    fn rebuild_classes(&mut self) {
        let mut classes = vec!["text-area".to_string()];
        if self.code_editor {
            classes.push("-code-editor".to_string());
        }
        self.classes = classes.clone();
        classes.push("focused".to_string());
        self.focused_classes = classes;
    }

    fn clamp_cursor(&mut self) {
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
        self.cursor.row = self.cursor.row.min(self.lines.len().saturating_sub(1));
        let line_len = self.lines[self.cursor.row].len();
        self.cursor.col = self.cursor.col.min(line_len);
        self.cursor.col = prev_char_boundary(&self.lines[self.cursor.row], self.cursor.col);
        self.selection = Selection::cursor(self.cursor);
    }

    fn clamp_cursor_pos(&self, cursor: Cursor) -> Cursor {
        if self.lines.is_empty() {
            return Cursor::default();
        }
        let row = cursor.row.min(self.lines.len().saturating_sub(1));
        let line = self.lines.get(row).map(String::as_str).unwrap_or("");
        let mut col = cursor.col.min(line.len());
        col = prev_char_boundary(line, col);
        Cursor { row, col }
    }

    fn line_number_gutter_width(&self) -> usize {
        if !self.code_editor {
            return 0;
        }
        let digits = self.lines.len().max(1).to_string().len();
        // Right aligned number + trailing space.
        digits + 1
    }

    fn cursor_cell_x(&self) -> usize {
        let line = self.lines.get(self.cursor.row).map(String::as_str).unwrap_or("");
        cell_len_prefix(line, self.cursor.col)
    }

    fn cursor_from_cell_x(&self, row: usize, cell_x: usize) -> usize {
        let line = self.lines.get(row).map(String::as_str).unwrap_or("");
        byte_index_from_cell_x(line, cell_x)
    }

    fn ensure_cursor_visible(&mut self, view_height: usize, view_width: usize) {
        self.scroll_row = self.scroll_row.min(self.cursor.row);
        if self.cursor.row >= self.scroll_row + view_height {
            self.scroll_row = self.cursor.row.saturating_sub(view_height.saturating_sub(1));
        }

        let cur_x = self.cursor_cell_x();
        self.scroll_col = self.scroll_col.min(cur_x);
        if cur_x >= self.scroll_col + view_width {
            self.scroll_col = cur_x.saturating_sub(view_width.saturating_sub(1));
        }
    }

    fn adjust_scroll_to_cursor(&mut self) {
        let gutter_w = self.line_number_gutter_width();
        let view_w = (self.layout_w.max(1) as usize)
            .saturating_sub(gutter_w)
            .max(1);
        let view_h = self.layout_h.max(1) as usize;
        self.ensure_cursor_visible(view_h, view_w);
    }

    fn reset_blink(&mut self) {
        if !self.focused || !self.app_active {
            return;
        }
        self.cursor_visible = true;
        self.cursor_blink_next_at = Some(Self::next_blink_deadline());
    }

    fn delete_selection_if_any(&mut self) -> bool {
        if self.selection.is_empty() {
            return false;
        }
        let (a, b) = normalized_selection(self.selection);
        if a.row == b.row {
            let line = &mut self.lines[a.row];
            line.drain(a.col..b.col);
            self.cursor = a;
        } else {
            let first = self.lines[a.row][..a.col].to_string();
            let last = self.lines[b.row][b.col..].to_string();
            self.lines[a.row] = format!("{first}{last}");
            // Remove middle lines inclusive of end row.
            self.lines.drain(a.row + 1..=b.row);
            self.cursor = a;
        }
        self.selection = Selection::cursor(self.cursor);
        true
    }

    fn insert_str(&mut self, text: &str) {
        if self.delete_selection_if_any() {
            // proceed with insertion at cursor
        }
        if text.is_empty() {
            return;
        }
        let row = self.cursor.row;
        let col = self.cursor.col;
        let line = &mut self.lines[row];
        line.insert_str(col, text);
        self.cursor.col += text.len();
        self.cursor.col = prev_char_boundary(line, self.cursor.col);
        self.selection = Selection::cursor(self.cursor);
    }

    fn insert_newline(&mut self) {
        if self.delete_selection_if_any() {
            // proceed with insertion at cursor
        }
        let row = self.cursor.row;
        let col = self.cursor.col;
        let current = self.lines[row].clone();
        let (left, right) = current.split_at(col);
        self.lines[row] = left.to_string();
        self.lines.insert(row + 1, right.to_string());
        self.cursor.row += 1;
        self.cursor.col = 0;
        self.selection = Selection::cursor(self.cursor);
    }

    fn backspace(&mut self) {
        if self.delete_selection_if_any() {
            return;
        }
        if self.cursor.col > 0 {
            let row = self.cursor.row;
            let line = &mut self.lines[row];
            let prev = prev_char_boundary(line, self.cursor.col);
            line.drain(prev..self.cursor.col);
            self.cursor.col = prev;
            self.selection = Selection::cursor(self.cursor);
        } else if self.cursor.row > 0 {
            let row = self.cursor.row;
            let prev_row = row - 1;
            let prev_len = self.lines[prev_row].len();
            let current = self.lines.remove(row);
            self.lines[prev_row].push_str(&current);
            self.cursor.row = prev_row;
            self.cursor.col = prev_len;
            self.selection = Selection::cursor(self.cursor);
        }
    }

    fn delete(&mut self) {
        if self.delete_selection_if_any() {
            return;
        }
        let row = self.cursor.row;
        let col = self.cursor.col;
        let line_len = self.lines[row].len();
        if col < line_len {
            let next = next_char_boundary(&self.lines[row], col);
            self.lines[row].drain(col..next);
        } else if row + 1 < self.lines.len() {
            let next_line = self.lines.remove(row + 1);
            self.lines[row].push_str(&next_line);
        }
        self.selection = Selection::cursor(self.cursor);
    }

    fn move_cursor_to(&mut self, row: usize, col: usize) {
        self.cursor.row = row.min(self.lines.len().saturating_sub(1));
        let line_len = self.lines[self.cursor.row].len();
        self.cursor.col = col.min(line_len);
        self.cursor.col = prev_char_boundary(&self.lines[self.cursor.row], self.cursor.col);
        self.selection = Selection::cursor(self.cursor);
    }

    fn move_left(&mut self) {
        if !self.selection.is_empty() {
            let (a, _b) = normalized_selection(self.selection);
            self.move_cursor_to(a.row, a.col);
            return;
        }
        if self.cursor.col > 0 {
            let line = &self.lines[self.cursor.row];
            self.cursor.col = prev_char_boundary(line, self.cursor.col);
        } else if self.cursor.row > 0 {
            self.cursor.row -= 1;
            self.cursor.col = self.lines[self.cursor.row].len();
        }
        self.selection = Selection::cursor(self.cursor);
    }

    fn move_right(&mut self) {
        if !self.selection.is_empty() {
            let (_a, b) = normalized_selection(self.selection);
            self.move_cursor_to(b.row, b.col);
            return;
        }
        let line_len = self.lines[self.cursor.row].len();
        if self.cursor.col < line_len {
            let next = next_char_boundary(&self.lines[self.cursor.row], self.cursor.col);
            self.cursor.col = next;
        } else if self.cursor.row + 1 < self.lines.len() {
            self.cursor.row += 1;
            self.cursor.col = 0;
        }
        self.selection = Selection::cursor(self.cursor);
    }

    fn move_up(&mut self) {
        if self.cursor.row == 0 {
            return;
        }
        let desired = self.preferred_col_cells.unwrap_or_else(|| self.cursor_cell_x());
        self.cursor.row -= 1;
        let new_col = self.cursor_from_cell_x(self.cursor.row, desired);
        self.cursor.col = new_col;
        self.selection = Selection::cursor(self.cursor);
    }

    fn move_down(&mut self) {
        if self.cursor.row + 1 >= self.lines.len() {
            return;
        }
        let desired = self.preferred_col_cells.unwrap_or_else(|| self.cursor_cell_x());
        self.cursor.row += 1;
        let new_col = self.cursor_from_cell_x(self.cursor.row, desired);
        self.cursor.col = new_col;
        self.selection = Selection::cursor(self.cursor);
    }
}

impl Widget for TextArea {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn focusable(&self) -> bool {
        true
    }

    fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
        if !focused {
            self.mouse_down = false;
            self.cursor_visible = false;
            self.cursor_blink_next_at = None;
            return;
        }
        self.reset_blink();
    }

    fn has_focus(&self) -> bool {
        self.focused
    }

    fn is_active(&self) -> bool {
        self.mouse_down
    }

    fn on_mouse_move(&mut self, x: u16, y: u16) -> bool {
        if !self.mouse_down {
            return false;
        }
        let gutter = self.line_number_gutter_width() as u16;
        let row = self.scroll_row.saturating_add(y as usize);
        let row = row.min(self.lines.len().saturating_sub(1));
        let local_x = x.saturating_sub(gutter) as usize;
        let cell_x = self.scroll_col.saturating_add(local_x);
        let col = self.cursor_from_cell_x(row, cell_x);
        let next = Cursor { row, col };
        if next == self.selection.end && next == self.cursor {
            return false;
        }
        self.selection.end = next;
        self.cursor = next;
        true
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        match event {
            Event::AppFocus(active) => {
                self.app_active = *active;
                if !*active {
                    self.cursor_visible = false;
                    self.cursor_blink_next_at = None;
                } else {
                    self.reset_blink();
                }
                ctx.request_repaint();
            }
            Event::Tick(_tick) => {
                if !self.focused || !self.app_active {
                    return;
                }
                let Some(next_at) = self.cursor_blink_next_at else {
                    return;
                };
                let now = Instant::now();
                if now >= next_at {
                    self.cursor_visible = !self.cursor_visible;
                    self.cursor_blink_next_at =
                        now.checked_add(Self::CURSOR_BLINK_PERIOD).or(Some(now));
                    ctx.request_repaint();
                }
            }
            Event::MouseDown(mouse) if mouse.target == self.id => {
                let gutter = self.line_number_gutter_width() as u16;
                let row = self.scroll_row.saturating_add(mouse.y as usize);
                let row = row.min(self.lines.len().saturating_sub(1));
                let local_x = mouse.x.saturating_sub(gutter) as usize;
                let cell_x = self.scroll_col.saturating_add(local_x);
                let col = self.cursor_from_cell_x(row, cell_x);
                self.cursor = Cursor { row, col };
                self.selection = Selection::cursor(self.cursor);
                self.mouse_down = true;
                self.preferred_col_cells = Some(self.cursor_cell_x());
                self.adjust_scroll_to_cursor();
                self.reset_blink();
                ctx.request_repaint();
                ctx.set_handled();
            }
            Event::MouseUp(_) => {
                if self.mouse_down {
                    self.mouse_down = false;
                    ctx.request_repaint();
                }
            }
            Event::Key(key) if self.focused => match key.code {
                KeyCode::Char(ch) => {
                    if ch != '\t' {
                        self.insert_str(&ch.to_string());
                        self.notify_changed();
                        self.preferred_col_cells = Some(self.cursor_cell_x());
                        self.adjust_scroll_to_cursor();
                        self.reset_blink();
                        ctx.request_repaint();
                    }
                    ctx.set_handled();
                }
                KeyCode::Enter => {
                    self.insert_newline();
                    self.notify_changed();
                    self.preferred_col_cells = Some(0);
                    self.adjust_scroll_to_cursor();
                    self.reset_blink();
                    ctx.request_repaint();
                    ctx.set_handled();
                }
                KeyCode::Backspace => {
                    self.backspace();
                    self.notify_changed();
                    self.preferred_col_cells = Some(self.cursor_cell_x());
                    self.adjust_scroll_to_cursor();
                    self.reset_blink();
                    ctx.request_repaint();
                    ctx.set_handled();
                }
                KeyCode::Delete => {
                    self.delete();
                    self.notify_changed();
                    self.preferred_col_cells = Some(self.cursor_cell_x());
                    self.adjust_scroll_to_cursor();
                    self.reset_blink();
                    ctx.request_repaint();
                    ctx.set_handled();
                }
                KeyCode::Left => {
                    self.move_left();
                    self.preferred_col_cells = Some(self.cursor_cell_x());
                    self.adjust_scroll_to_cursor();
                    self.reset_blink();
                    ctx.request_repaint();
                    ctx.set_handled();
                }
                KeyCode::Right => {
                    self.move_right();
                    self.preferred_col_cells = Some(self.cursor_cell_x());
                    self.adjust_scroll_to_cursor();
                    self.reset_blink();
                    ctx.request_repaint();
                    ctx.set_handled();
                }
                KeyCode::Up => {
                    self.move_up();
                    self.adjust_scroll_to_cursor();
                    self.reset_blink();
                    ctx.request_repaint();
                    ctx.set_handled();
                }
                KeyCode::Down => {
                    self.move_down();
                    self.adjust_scroll_to_cursor();
                    self.reset_blink();
                    ctx.request_repaint();
                    ctx.set_handled();
                }
                KeyCode::Home => {
                    self.move_cursor_to(self.cursor.row, 0);
                    self.preferred_col_cells = Some(0);
                    self.adjust_scroll_to_cursor();
                    self.reset_blink();
                    ctx.request_repaint();
                    ctx.set_handled();
                }
                KeyCode::End => {
                    let end = self.lines[self.cursor.row].len();
                    self.move_cursor_to(self.cursor.row, end);
                    self.preferred_col_cells = Some(self.cursor_cell_x());
                    self.adjust_scroll_to_cursor();
                    self.reset_blink();
                    ctx.request_repaint();
                    ctx.set_handled();
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        let width = width.max(1);
        let height = height.max(1);
        if self.layout_w != width || self.layout_h != height {
            self.layout_w = width;
            self.layout_h = height;
        }
        self.adjust_scroll_to_cursor();
    }

    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);
        let gutter_w = self.line_number_gutter_width();
        let text_w = width.saturating_sub(gutter_w).max(1);

        let base_meta = crate::css::selector_meta_generic(self);
        let base_style = crate::css::resolve_style(self, &base_meta);
        let fallback_bg = parse_color_like("$background").unwrap_or(Color::rgb(0, 0, 0));
        let base_bg = base_style.bg.unwrap_or(fallback_bg);

        let resolve_component_rich = |class: &str| -> rich_rs::Style {
            let meta = crate::css::selector_meta_component(self.style_type(), &[class]);
            let style = crate::css::resolve_style_for_meta(&meta);
            let mut rich = style.to_rich_without_colors().unwrap_or_else(rich_rs::Style::new);
            let mut under_bg = base_bg;

            if let Some(bg) = style.bg {
                let flat = bg.flatten_over(under_bg);
                under_bg = flat;
                rich = rich.with_bgcolor(flat.to_simple_opaque());
            }
            if let Some(fg) = style.fg {
                let flat = fg.flatten_over(under_bg);
                rich = rich.with_color(flat.to_simple_opaque());
            }
            rich
        };

        let cursor_style = resolve_component_rich("text-area--cursor");
        let selection_style = resolve_component_rich("text-area--selection");
        let gutter_style = resolve_component_rich("text-area--gutter");
        let gutter_active_style = resolve_component_rich("text-area--gutter-active");
        let cursor_line_style = resolve_component_rich("text-area--cursor-line");

        let (sel_a, sel_b) = normalized_selection(self.selection);

        let mut out = Segments::new();
        for y in 0..height {
            let row = self.scroll_row + y;
            let is_cursor_line = self.focused && self.app_active && row == self.cursor.row;
            let line_default_style = if is_cursor_line {
                Some(cursor_line_style)
            } else {
                None
            };
            if gutter_w > 0 {
                let line_no = row.saturating_add(1);
                let digits = gutter_w.saturating_sub(1).max(1);
                let gutter_text = format!("{line_no:>digits$} ");
                let style = if self.focused && row == self.cursor.row {
                    gutter_active_style
                } else {
                    gutter_style
                };
                out.push(Segment::styled(rich_rs::set_cell_size(&gutter_text, gutter_w), style));
            }

            if row >= self.lines.len() {
                out.push(Segment::new(" ".repeat(text_w)));
                if y + 1 < height {
                    out.push(Segment::line());
                }
                continue;
            }

            let line = &self.lines[row];
            let eol_in_sel = !self.selection.is_empty()
                && cursor_le(sel_a, Cursor { row, col: line.len() })
                && cursor_lt(Cursor { row, col: line.len() }, sel_b);
            let start_cell = self.scroll_col;
            let mut cell_x = 0usize;
            let mut pending_style: Option<rich_rs::Style> = None;
            let mut pending_text = String::new();

            let flush = |out: &mut Segments,
                         pending_style: &mut Option<rich_rs::Style>,
                         pending_text: &mut String| {
                if pending_text.is_empty() {
                    return;
                }
                let style = pending_style.take().unwrap_or_else(rich_rs::Style::new);
                out.push(Segment::styled(std::mem::take(pending_text), style));
            };

            let mut idx = 0usize;
            for (byte_idx, ch) in line.char_indices() {
                idx = byte_idx;
                let w = UnicodeWidthChar::width(ch).unwrap_or(0).max(1);
                let ch_cell_start = cell_len_prefix(line, byte_idx);
                let ch_cell_end = ch_cell_start + w;

                if ch_cell_end <= start_cell {
                    continue;
                }
                if cell_x >= text_w {
                    break;
                }

                let visible_ch = ch;
                let is_cursor = self.focused
                    && self.cursor_visible
                    && row == self.cursor.row
                    && byte_idx == self.cursor.col;
                let in_sel = !self.selection.is_empty()
                    && cursor_le(sel_a, Cursor { row, col: byte_idx })
                    && cursor_lt(Cursor { row, col: byte_idx }, sel_b);
                let style = if is_cursor {
                    Some(cursor_style)
                } else if in_sel {
                    Some(selection_style)
                } else {
                    line_default_style
                };

                let style_changed = match (&pending_style, &style) {
                    (None, None) => false,
                    (Some(a), Some(b)) => a != b,
                    _ => true,
                };
                if style_changed {
                    flush(&mut out, &mut pending_style, &mut pending_text);
                    pending_style = style;
                }

                // If we scrolled into the middle of a wide char, drop it for now.
                if ch_cell_start < start_cell {
                    continue;
                }

                pending_text.push(visible_ch);
                cell_x += w;
            }
            let _ = idx;
            flush(&mut out, &mut pending_style, &mut pending_text);

            // Cursor at end of line: paint a single cell with cursor style.
            if self.focused && self.cursor_visible && row == self.cursor.row {
                let end_cell = cell_len_prefix(line, line.len());
                if self.cursor.col == line.len() && end_cell >= start_cell && cell_x < text_w {
                    out.push(Segment::styled(" ".to_string(), cursor_style));
                    cell_x += 1;
                }
            }

            if cell_x < text_w {
                let pad = " ".repeat(text_w - cell_x);
                if eol_in_sel {
                    out.push(Segment::styled(pad, selection_style));
                } else if let Some(style) = line_default_style {
                    out.push(Segment::styled(pad, style));
                } else {
                    out.push(Segment::new(pad));
                }
            }

            if y + 1 < height {
                out.push(Segment::line());
            }
        }

        out
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

fn split_lines(text: String) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }
    let mut lines: Vec<String> = text.split('\n').map(|s| s.to_string()).collect();
    // Preserve trailing newline as an empty last line.
    if text.ends_with('\n') {
        if !lines.last().is_some_and(|s| s.is_empty()) {
            lines.push(String::new());
        }
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn prev_char_boundary(s: &str, mut idx: usize) -> usize {
    idx = idx.min(s.len());
    while idx > 0 && !s.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

fn next_char_boundary(s: &str, mut idx: usize) -> usize {
    idx = idx.min(s.len());
    if idx >= s.len() {
        return s.len();
    }
    idx += 1;
    while idx < s.len() && !s.is_char_boundary(idx) {
        idx += 1;
    }
    idx.min(s.len())
}

fn cell_len_prefix(s: &str, byte_end: usize) -> usize {
    let mut cells = 0usize;
    let end = byte_end.min(s.len());
    for (_idx, ch) in s[..end].char_indices() {
        let w = UnicodeWidthChar::width(ch).unwrap_or(0).max(1);
        cells = cells.saturating_add(w);
    }
    cells
}

fn byte_index_from_cell_x(s: &str, target_cell: usize) -> usize {
    let mut cells = 0usize;
    let mut last = 0usize;
    for (idx, ch) in s.char_indices() {
        let w = UnicodeWidthChar::width(ch).unwrap_or(0).max(1);
        if cells + w / 2 >= target_cell {
            return idx;
        }
        cells += w;
        last = idx + ch.len_utf8();
        if cells >= target_cell {
            return last;
        }
    }
    last
}

fn normalized_selection(sel: Selection) -> (Cursor, Cursor) {
    if cursor_le(sel.start, sel.end) {
        (sel.start, sel.end)
    } else {
        (sel.end, sel.start)
    }
}

fn cursor_le(a: Cursor, b: Cursor) -> bool {
    a.row < b.row || (a.row == b.row && a.col <= b.col)
}

fn cursor_lt(a: Cursor, b: Cursor) -> bool {
    a.row < b.row || (a.row == b.row && a.col < b.col)
}
