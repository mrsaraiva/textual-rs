use crossterm::event::KeyCode;
use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::event::{Action, Event, EventCtx};
use crate::message::Message;
use crate::style::{Color, parse_color_like};

use super::{
    Widget, WidgetId, WidgetStyles,
    helpers::{empty_classes, focused_classes},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorType {
    Cell,
    Row,
    Column,
    None,
}

#[derive(Debug, Clone)]
pub struct DataTable {
    id: WidgetId,
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    column_widths: Vec<usize>,
    selected: usize,
    offset: usize,
    cursor_column: usize,
    cursor_type: CursorType,
    content_width: u16,
    content_height: u16,
    focused: bool,
    hovered: bool,
    hover_coordinate: Option<(usize, usize)>,
    styles: WidgetStyles,
}

impl DataTable {
    pub fn new(headers: Vec<String>, rows: Vec<Vec<String>>) -> Self {
        let mut out = Self {
            id: WidgetId::new(),
            headers,
            rows,
            column_widths: Vec::new(),
            selected: 0,
            offset: 0,
            cursor_column: 0,
            cursor_type: CursorType::Cell,
            content_width: 0,
            content_height: 0,
            focused: false,
            hovered: false,
            hover_coordinate: None,
            styles: WidgetStyles::default(),
        };
        out.recompute_column_widths();
        out
    }

    /// Create an empty table (columns and rows added later).
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn add_columns<I, S>(&mut self, columns: I)
    where
        I: IntoIterator<Item = S>,
        S: ToString,
    {
        for col in columns {
            self.headers.push(col.to_string());
        }
        self.recompute_column_widths();
    }

    pub fn add_rows<I, R, S>(&mut self, rows: I)
    where
        I: IntoIterator<Item = R>,
        R: AsRef<[S]>,
        S: ToString,
    {
        for row in rows {
            self.rows
                .push(row.as_ref().iter().map(|s| s.to_string()).collect());
        }
        if self.selected >= self.rows.len() {
            self.selected = self.rows.len().saturating_sub(1);
        }
        self.recompute_column_widths();
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn set_selected(&mut self, index: usize) {
        if self.rows.is_empty() {
            self.selected = 0;
            self.offset = 0;
            return;
        }
        self.selected = index.min(self.rows.len() - 1);
    }

    pub fn cursor_type(mut self, ct: CursorType) -> Self {
        self.cursor_type = ct;
        self
    }

    fn recompute_column_widths(&mut self) {
        let mut widths: Vec<usize> = self
            .headers
            .iter()
            .map(|h| rich_rs::cell_len(h).max(3))
            .collect();
        for row in &self.rows {
            for (idx, value) in row.iter().enumerate() {
                if let Some(w) = widths.get_mut(idx) {
                    *w = (*w).max(rich_rs::cell_len(value).max(1));
                }
            }
        }
        self.column_widths = widths;
    }

    /// Return cached column widths (recomputed on mutation).
    fn column_widths(&self) -> &[usize] {
        &self.column_widths
    }

    /// Map an x coordinate to a column index, given column widths with 2-space gap.
    fn column_at_x(&self, x: usize, column_widths: &[usize]) -> usize {
        let mut pos = 0;
        for (i, &w) in column_widths.iter().enumerate() {
            let end = pos + w;
            if x < end {
                return i;
            }
            pos = end + 2; // 2-space gap between columns
        }
        column_widths.len().saturating_sub(1)
    }

    fn ensure_visible(&mut self, height: usize) {
        if self.rows.is_empty() || height == 0 {
            self.offset = 0;
            return;
        }
        if self.selected < self.offset {
            self.offset = self.selected;
        } else if self.selected >= self.offset + height {
            self.offset = self.selected + 1 - height;
        }
    }

    fn visible_rows(&self) -> usize {
        self.content_height.saturating_sub(1) as usize
    }

    fn effective_offset(&self, visible_rows: usize) -> usize {
        if self.rows.is_empty() || visible_rows == 0 {
            return 0;
        }
        let mut offset = self.offset.min(self.rows.len().saturating_sub(1));
        if self.selected < offset {
            offset = self.selected;
        } else if self.selected >= offset + visible_rows {
            offset = self.selected + 1 - visible_rows;
        }
        offset
    }
}

impl Default for DataTable {
    fn default() -> Self {
        let mut out = Self {
            id: WidgetId::new(),
            headers: Vec::new(),
            rows: Vec::new(),
            column_widths: Vec::new(),
            selected: 0,
            offset: 0,
            cursor_column: 0,
            cursor_type: CursorType::Cell,
            content_width: 0,
            content_height: 0,
            focused: false,
            hovered: false,
            hover_coordinate: None,
            styles: WidgetStyles::default(),
        };
        out.recompute_column_widths();
        out
    }
}

impl Widget for DataTable {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn focusable(&self) -> bool {
        true
    }

    fn mouse_interactive(&self) -> bool {
        true
    }

    fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
    }

    fn has_focus(&self) -> bool {
        self.focused
    }

    fn is_hovered(&self) -> bool {
        self.hovered
    }

    fn set_hovered(&mut self, hovered: bool) {
        self.hovered = hovered;
        if !hovered {
            self.hover_coordinate = None;
        }
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.content_width = width;
        self.content_height = height;
        self.ensure_visible(self.visible_rows());
    }

    fn on_mouse_move(&mut self, x: u16, y: u16) -> bool {
        let widths = self.column_widths();
        let col_idx = self.column_at_x(x as usize, widths);
        let next = if y == 0 {
            // Header row — use usize::MAX as sentinel (mirrors Textual's row_index=-1).
            Some((usize::MAX, col_idx))
        } else {
            let offset = self.effective_offset(self.visible_rows());
            let row_idx = (y as usize - 1) + offset;
            if row_idx < self.rows.len() {
                Some((row_idx, col_idx))
            } else {
                None
            }
        };
        let changed = next != self.hover_coordinate;
        self.hover_coordinate = next;
        changed
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        let visible_rows = self.visible_rows();
        let mut selection_changed = false;
        let mut cursor_changed = false;
        let mut header_clicked: Option<usize> = None;

        // Handle mouse events regardless of focus state.
        match event {
            Event::MouseDown(mouse) if mouse.target == self.id => {
                let widths = self.column_widths();
                let clicked_col = self.column_at_x(mouse.x as usize, widths);
                if matches!(self.cursor_type, CursorType::Cell | CursorType::Column) {
                    if self.cursor_column != clicked_col {
                        self.cursor_column = clicked_col;
                        cursor_changed = true;
                    }
                }

                if mouse.y > 0 {
                    let offset = self.effective_offset(visible_rows);
                    let row_y = (mouse.y as usize) - 1;
                    let clicked_row = row_y + offset;
                    if clicked_row < self.rows.len() {
                        if self.selected != clicked_row {
                            self.selected = clicked_row;
                            selection_changed = true;
                        }
                    }
                } else {
                    header_clicked = Some(clicked_col);
                }
                if selection_changed {
                    self.ensure_visible(visible_rows);
                }
                if let Some(col) = header_clicked {
                    ctx.post_message(self.id, Message::DataTableHeaderSelected { column: col });
                } else if selection_changed || cursor_changed {
                    ctx.post_message(
                        self.id,
                        Message::DataTableCursorMoved {
                            row: self.selected,
                            column: self.cursor_column,
                        },
                    );
                }
                ctx.set_handled();
                return;
            }
            _ => {}
        }

        if !self.focused {
            return;
        }
        let mut handled = false;
        match event {
            Event::Action(Action::ScrollUp) => {
                if matches!(self.cursor_type, CursorType::Cell | CursorType::Row) {
                    if self.selected > 0 {
                        self.selected -= 1;
                        selection_changed = true;
                    }
                    handled = true;
                }
            }
            Event::Action(Action::ScrollDown) => {
                if matches!(self.cursor_type, CursorType::Cell | CursorType::Row) {
                    if self.selected + 1 < self.rows.len() {
                        self.selected += 1;
                        selection_changed = true;
                    }
                    handled = true;
                }
            }
            Event::Action(Action::ScrollLeft) => {
                if matches!(self.cursor_type, CursorType::Cell | CursorType::Column) {
                    if self.cursor_column > 0 {
                        self.cursor_column -= 1;
                        cursor_changed = true;
                    }
                    handled = true;
                }
            }
            Event::Action(Action::ScrollRight) => {
                if matches!(self.cursor_type, CursorType::Cell | CursorType::Column) {
                    if self.cursor_column + 1 < self.headers.len() {
                        self.cursor_column += 1;
                        cursor_changed = true;
                    }
                    handled = true;
                }
            }
            Event::Action(Action::ScrollPageUp) => {
                if matches!(self.cursor_type, CursorType::Cell | CursorType::Row) {
                    if self.selected > 0 {
                        let step = 5.min(self.selected);
                        self.selected -= step;
                        selection_changed = true;
                    }
                    handled = true;
                }
            }
            Event::Action(Action::ScrollPageDown) => {
                if matches!(self.cursor_type, CursorType::Cell | CursorType::Row) {
                    if self.selected + 1 < self.rows.len() {
                        let step = 5.min(self.rows.len().saturating_sub(1) - self.selected);
                        self.selected += step;
                        selection_changed = true;
                    }
                    handled = true;
                }
            }
            Event::Key(key) => match key.code {
                KeyCode::Up => {
                    if matches!(self.cursor_type, CursorType::Cell | CursorType::Row) {
                        if self.selected > 0 {
                            self.selected -= 1;
                            selection_changed = true;
                        }
                        handled = true;
                    }
                }
                KeyCode::Down => {
                    if matches!(self.cursor_type, CursorType::Cell | CursorType::Row) {
                        if self.selected + 1 < self.rows.len() {
                            self.selected += 1;
                            selection_changed = true;
                        }
                        handled = true;
                    }
                }
                KeyCode::Left => {
                    if matches!(self.cursor_type, CursorType::Cell | CursorType::Column) {
                        if self.cursor_column > 0 {
                            self.cursor_column -= 1;
                            cursor_changed = true;
                        }
                        handled = true;
                    }
                }
                KeyCode::Right => {
                    if matches!(self.cursor_type, CursorType::Cell | CursorType::Column) {
                        if self.cursor_column + 1 < self.headers.len() {
                            self.cursor_column += 1;
                            cursor_changed = true;
                        }
                        handled = true;
                    }
                }
                KeyCode::PageUp => {
                    if matches!(self.cursor_type, CursorType::Cell | CursorType::Row) {
                        if self.selected > 0 {
                            let step = 5.min(self.selected);
                            self.selected -= step;
                            selection_changed = true;
                        }
                        handled = true;
                    }
                }
                KeyCode::PageDown => {
                    if matches!(self.cursor_type, CursorType::Cell | CursorType::Row) {
                        if self.selected + 1 < self.rows.len() {
                            let step = 5.min(self.rows.len().saturating_sub(1) - self.selected);
                            self.selected += step;
                            selection_changed = true;
                        }
                        handled = true;
                    }
                }
                KeyCode::Enter | KeyCode::Char(' ') => {
                    if !self.rows.is_empty() && !self.headers.is_empty() {
                        ctx.post_message(
                            self.id,
                            Message::DataTableCellActivated {
                                row: self.selected,
                                column: self.cursor_column,
                            },
                        );
                        handled = true;
                    }
                }
                _ => {}
            },
            _ => {}
        }
        if selection_changed {
            self.ensure_visible(visible_rows);
        }
        if (selection_changed || cursor_changed) && !self.rows.is_empty() && !self.headers.is_empty()
        {
            ctx.post_message(
                self.id,
                Message::DataTableCursorMoved {
                    row: self.selected,
                    column: self.cursor_column,
                },
            );
        }
        if handled {
            ctx.set_handled();
        }
    }

    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);
        let visible_rows = height.saturating_sub(1);
        let offset = self.effective_offset(visible_rows);

        let column_widths = self.column_widths();
        let cursor_type = self.cursor_type;
        let show_cursor = self.focused && cursor_type != CursorType::None;

        // Cursor and hover coordinates.
        let cursor_coord = (self.selected, self.cursor_column);
        let hover_coord = self.hover_coordinate;

        // Resolve theme colors.
        let header_bg = parse_color_like("$panel");
        let row_bg = parse_color_like("$surface");
        let cursor_bg = parse_color_like("$primary");
        let hover_bg = parse_color_like("$block-hover-background");
        let header_hover_bg = parse_color_like("$header-hover-background");

        let fallback_bg = parse_color_like("$background").unwrap_or(Color::rgb(0, 0, 0));
        let header_base = header_bg.unwrap_or(fallback_bg);
        let row_base = row_bg.unwrap_or(fallback_bg);
        let hover_bg = hover_bg.map(|c| c.flatten_over(row_base));
        let header_hover_bg = header_hover_bg.map(|c| c.flatten_over(header_base));

        let header_style = rich_rs::Style::new()
            .with_bold(true)
            .with_bgcolor(header_base.to_simple_opaque());
        let normal_style = rich_rs::Style::new().with_bgcolor(row_base.to_simple_opaque());
        let mut selected_style = rich_rs::Style::new().with_bold(true);
        if let Some(bg) = cursor_bg {
            selected_style = selected_style.with_bgcolor(bg.to_simple_opaque());
        }
        let mut hover_style = rich_rs::Style::new();
        if let Some(bg) = hover_bg {
            hover_style = hover_style.with_bgcolor(bg.to_simple_opaque());
        }
        let mut header_hover_style = rich_rs::Style::new().with_bold(true);
        if let Some(bg) = header_hover_bg {
            header_hover_style = header_hover_style.with_bgcolor(bg.to_simple_opaque());
        }

        let mut out = Segments::new();

        // Helper: join column values with 2-space padding, pad to full width.
        let format_row_uniform =
            |values: &[String], widths: &[usize], total_width: usize| -> String {
                let parts: Vec<String> = values
                    .iter()
                    .enumerate()
                    .map(|(i, val)| {
                        let col_w = *widths.get(i).unwrap_or(&3);
                        rich_rs::set_cell_size(val, col_w)
                    })
                    .collect();
                let joined = parts.join("  ");
                rich_rs::set_cell_size(&joined, total_width)
            };

        // Mirrors Textual's _should_highlight: does `target` match `cursor` given the type?
        let should_highlight =
            |cursor: (usize, usize), target: (usize, usize), ct: CursorType| -> bool {
                match ct {
                    CursorType::Cell => cursor == target,
                    CursorType::Row => cursor.0 == target.0,
                    CursorType::Column => cursor.1 == target.1,
                    CursorType::None => false,
                }
            };

        // Header line.
        // Headers use usize::MAX as their row sentinel (mirroring Textual's row=-1).
        // Per-cell rendering is needed when cursor or hover could highlight individual
        // header cells (Column cursor always, Cell/Column hover when on the header row).
        let header_needs_per_cell = (show_cursor && matches!(cursor_type, CursorType::Column))
            || (hover_coord.is_some() && cursor_type != CursorType::None);

        if header_needs_per_cell {
            emit_row_per_cell(
                &self.headers,
                column_widths,
                width,
                |col_idx| {
                    let target = (usize::MAX, col_idx);
                    if show_cursor && should_highlight(cursor_coord, target, cursor_type) {
                        return selected_style.with_bold(true);
                    }
                    if let Some(hc) = hover_coord {
                        if should_highlight(hc, target, cursor_type) {
                            return header_hover_style;
                        }
                    }
                    header_style
                },
                header_style,
                &mut out,
            );
        } else {
            let header_text = format_row_uniform(&self.headers, column_widths, width);
            out.push(Segment::styled(header_text, header_style));
        }
        out.push(Segment::line());
        let mut lines_used = 1usize;

        // Data rows.
        for (idx, row) in self.rows.iter().enumerate() {
            if idx < offset {
                continue;
            }
            if lines_used >= height {
                break;
            }

            // Check if any cell in this row needs per-cell styling.
            let row_has_cursor = show_cursor
                && match cursor_type {
                    CursorType::Row => idx == cursor_coord.0,
                    CursorType::Cell => idx == cursor_coord.0,
                    CursorType::Column => true, // every row has the highlighted column
                    CursorType::None => false,
                };
            let row_has_hover = hover_coord.is_some()
                && match cursor_type {
                    CursorType::Row => hover_coord.map(|h| h.0) == Some(idx),
                    CursorType::Cell => hover_coord.map(|h| h.0) == Some(idx),
                    CursorType::Column => true,
                    CursorType::None => false,
                };

            let needs_per_cell = (row_has_cursor
                && matches!(cursor_type, CursorType::Cell | CursorType::Column))
                || (row_has_hover && matches!(cursor_type, CursorType::Cell | CursorType::Column));

            if needs_per_cell {
                emit_row_per_cell(
                    row,
                    column_widths,
                    width,
                    |col_idx| {
                        let target = (idx, col_idx);
                        if show_cursor && should_highlight(cursor_coord, target, cursor_type) {
                            return selected_style;
                        }
                        if let Some(hc) = hover_coord {
                            if should_highlight(hc, target, cursor_type) {
                                return hover_style;
                            }
                        }
                        normal_style
                    },
                    normal_style,
                    &mut out,
                );
            } else {
                // Uniform style for the whole row.
                let style = if show_cursor && should_highlight(cursor_coord, (idx, 0), cursor_type)
                {
                    selected_style
                } else if let Some(hc) = hover_coord {
                    if should_highlight(hc, (idx, 0), cursor_type) {
                        hover_style
                    } else {
                        normal_style
                    }
                } else {
                    normal_style
                };
                let row_text = format_row_uniform(row, column_widths, width);
                out.push(Segment::styled(row_text, style));
            }

            out.push(Segment::line());
            lines_used += 1;
        }

        out
    }

    fn style_classes(&self) -> &[String] {
        if self.focused {
            focused_classes()
        } else {
            empty_classes()
        }
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

impl Renderable for DataTable {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

/// Emit a row where each cell can have a different style determined by `style_for_col`.
fn emit_row_per_cell(
    values: &[String],
    column_widths: &[usize],
    total_width: usize,
    style_for_col: impl Fn(usize) -> rich_rs::Style,
    gap_style: rich_rs::Style,
    out: &mut Segments,
) {
    let mut used = 0usize;
    for (i, val) in values.iter().enumerate() {
        if i > 0 {
            out.push(Segment::styled("  ", gap_style));
            used += 2;
        }
        let col_w = *column_widths.get(i).unwrap_or(&3);
        let cell_text = rich_rs::set_cell_size(val, col_w);
        out.push(Segment::styled(cell_text, style_for_col(i)));
        used += col_w;
    }
    // Pad remainder to full width.
    if used < total_width {
        out.push(Segment::styled(" ".repeat(total_width - used), gap_style));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::MouseDownEvent;

    #[test]
    fn header_click_does_not_change_selected_row() {
        let mut table = DataTable::new(
            vec!["A".into(), "B".into()],
            vec![
                vec!["r0".into(), "c0".into()],
                vec!["r1".into(), "c1".into()],
            ],
        );
        table.selected = 1;
        let id = table.id();
        let mut ctx = EventCtx::default();

        table.on_event(
            &Event::MouseDown(MouseDownEvent {
                target: id,
                screen_x: 0,
                screen_y: 0,
                x: 0,
                y: 0,
            }),
            &mut ctx,
        );

        assert!(ctx.handled());
        assert_eq!(table.selected, 1);
        assert_eq!(table.cursor_column, 0);
    }

    #[test]
    fn mouse_move_sets_hover_coordinate_for_header_and_cells() {
        let mut table = DataTable::new(
            vec!["ABC".into(), "D".into()],
            vec![
                vec!["row0".into(), "x".into()],
                vec!["row1".into(), "y".into()],
            ],
        );
        table.set_hovered(true);

        table.on_mouse_move(0, 0);
        assert_eq!(table.hover_coordinate, Some((usize::MAX, 0)));

        table.on_mouse_move(4, 0);
        assert_eq!(table.hover_coordinate, Some((usize::MAX, 1)));

        table.on_mouse_move(0, 1);
        assert_eq!(table.hover_coordinate, Some((0, 0)));

        table.on_mouse_move(4, 2);
        assert_eq!(table.hover_coordinate, Some((1, 1)));
    }

    #[test]
    fn clearing_hover_resets_hover_coordinate() {
        let mut table = DataTable::new(
            vec!["A".into()],
            vec![vec!["row0".into()], vec!["row1".into()]],
        );
        table.set_hovered(true);
        table.on_mouse_move(0, 1);
        assert_eq!(table.hover_coordinate, Some((0, 0)));

        table.set_hovered(false);
        assert_eq!(table.hover_coordinate, None);
    }
}
