use crossterm::event::{KeyCode, KeyModifiers};
use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::event::{Action, Event, EventCtx};
use crate::message::Message;
use crate::style::{Color, parse_color_like};

use super::{
    ScrollView, Widget, WidgetId, WidgetStyles,
    helpers::{empty_classes, fixed_height_from_constraints, focused_classes},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorType {
    Cell,
    Row,
    Column,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RowKey(String);

impl RowKey {
    pub fn new(key: impl Into<String>) -> Self {
        Self(key.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ColumnKey(String);

impl ColumnKey {
    pub fn new(key: impl Into<String>) -> Self {
        Self(key.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone)]
pub struct DataTable {
    id: WidgetId,
    column_keys: Vec<ColumnKey>,
    headers: Vec<String>,
    row_keys: Vec<RowKey>,
    rows: Vec<Vec<String>>,
    column_widths: Vec<usize>,
    selected: usize,
    offset: usize,
    cursor_column: usize,
    cursor_type: CursorType,
    fixed_rows: usize,
    fixed_columns: usize,
    next_row_key: usize,
    next_column_key: usize,
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
            column_keys: Vec::new(),
            headers: Vec::new(),
            row_keys: Vec::new(),
            rows: Vec::new(),
            column_widths: Vec::new(),
            selected: 0,
            offset: 0,
            cursor_column: 0,
            cursor_type: CursorType::Cell,
            fixed_rows: 0,
            fixed_columns: 0,
            next_row_key: 0,
            next_column_key: 0,
            content_width: 0,
            content_height: 0,
            focused: false,
            hovered: false,
            hover_coordinate: None,
            styles: WidgetStyles::default(),
        };
        for header in headers {
            let _ = out.add_column(header);
        }
        for row in rows {
            let _ = out.add_row(row);
        }
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
            let _ = self.add_column(col);
        }
    }

    pub fn add_rows<I, R, S>(&mut self, rows: I)
    where
        I: IntoIterator<Item = R>,
        R: AsRef<[S]>,
        S: ToString,
    {
        for row in rows {
            let row_values = row.as_ref().iter().map(|s| s.to_string()).collect();
            let _ = self.add_row(row_values);
        }
    }

    pub fn add_column<S>(&mut self, column: S) -> ColumnKey
    where
        S: ToString,
    {
        let key = self.generate_column_key();
        self.column_keys.push(key.clone());
        self.headers.push(column.to_string());
        self.clamp_indices();
        self.recompute_column_widths();
        key
    }

    pub fn add_column_with_key<K, S>(&mut self, key: K, column: S) -> Option<ColumnKey>
    where
        K: Into<String>,
        S: ToString,
    {
        let key = ColumnKey::new(key);
        if self.column_keys.iter().any(|existing| existing == &key) {
            return None;
        }
        self.column_keys.push(key.clone());
        self.headers.push(column.to_string());
        self.clamp_indices();
        self.recompute_column_widths();
        Some(key)
    }

    pub fn add_row<S>(&mut self, row: Vec<S>) -> RowKey
    where
        S: ToString,
    {
        let key = self.generate_row_key();
        self.row_keys.push(key.clone());
        self.rows
            .push(row.into_iter().map(|value| value.to_string()).collect());
        self.clamp_indices();
        self.recompute_column_widths();
        key
    }

    pub fn add_row_with_key<K, S>(&mut self, key: K, row: Vec<S>) -> Option<RowKey>
    where
        K: Into<String>,
        S: ToString,
    {
        let key = RowKey::new(key);
        if self.row_keys.iter().any(|existing| existing == &key) {
            return None;
        }
        self.row_keys.push(key.clone());
        self.rows
            .push(row.into_iter().map(|value| value.to_string()).collect());
        self.clamp_indices();
        self.recompute_column_widths();
        Some(key)
    }

    pub fn row_key_at(&self, row: usize) -> Option<&RowKey> {
        self.row_keys.get(row)
    }

    pub fn column_key_at(&self, column: usize) -> Option<&ColumnKey> {
        self.column_keys.get(column)
    }

    pub fn row_index_of(&self, key: &RowKey) -> Option<usize> {
        self.row_keys.iter().position(|existing| existing == key)
    }

    pub fn column_index_of(&self, key: &ColumnKey) -> Option<usize> {
        self.column_keys.iter().position(|existing| existing == key)
    }

    pub fn cell_key_at(&self, row: usize, column: usize) -> Option<(RowKey, ColumnKey)> {
        let row_key = self.row_key_at(row)?;
        let column_key = self.column_key_at(column)?;
        Some((row_key.clone(), column_key.clone()))
    }

    pub fn cursor_cell_key(&self) -> Option<(RowKey, ColumnKey)> {
        self.cell_key_at(self.selected, self.cursor_column)
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn selected_column(&self) -> usize {
        self.cursor_column
    }

    pub fn cursor(&self) -> (usize, usize) {
        (self.selected, self.cursor_column)
    }

    pub fn set_selected(&mut self, index: usize) {
        if self.rows.is_empty() {
            self.selected = 0;
            self.offset = 0;
            return;
        }
        self.selected = index.min(self.rows.len() - 1);
        self.ensure_visible(self.visible_rows());
    }

    pub fn set_cursor(&mut self, row: usize, column: usize) {
        self.set_selected(row);
        if self.headers.is_empty() {
            self.cursor_column = 0;
        } else {
            self.cursor_column = column.min(self.headers.len() - 1);
        }
    }

    pub fn set_cursor_type(&mut self, ct: CursorType) {
        self.cursor_type = ct;
    }

    pub fn set_fixed_rows(&mut self, count: usize) {
        self.fixed_rows = count;
        self.ensure_visible(self.visible_rows());
    }

    pub fn set_fixed_columns(&mut self, count: usize) {
        self.fixed_columns = count;
    }

    pub fn fixed_rows(&self) -> usize {
        self.fixed_rows
    }

    pub fn fixed_columns(&self) -> usize {
        self.fixed_columns
    }

    pub fn cursor_type(mut self, ct: CursorType) -> Self {
        self.cursor_type = ct;
        self
    }

    fn generate_row_key(&mut self) -> RowKey {
        loop {
            let candidate = RowKey::new(format!("row-{}", self.next_row_key));
            self.next_row_key = self.next_row_key.saturating_add(1);
            if !self.row_keys.iter().any(|key| key == &candidate) {
                return candidate;
            }
        }
    }

    fn generate_column_key(&mut self) -> ColumnKey {
        loop {
            let candidate = ColumnKey::new(format!("column-{}", self.next_column_key));
            self.next_column_key = self.next_column_key.saturating_add(1);
            if !self.column_keys.iter().any(|key| key == &candidate) {
                return candidate;
            }
        }
    }

    fn clamp_indices(&mut self) {
        if self.rows.is_empty() {
            self.selected = 0;
            self.offset = 0;
        } else if self.selected >= self.rows.len() {
            self.selected = self.rows.len().saturating_sub(1);
        }
        if self.headers.is_empty() {
            self.cursor_column = 0;
        } else if self.cursor_column >= self.headers.len() {
            self.cursor_column = self.headers.len().saturating_sub(1);
        }
    }

    fn fixed_data_rows(&self) -> usize {
        self.fixed_rows.min(self.rows.len())
    }

    fn visible_fixed_rows(&self, visible_rows: usize) -> usize {
        self.fixed_data_rows().min(visible_rows)
    }

    fn scrollable_visible_rows(&self, visible_rows: usize) -> usize {
        visible_rows.saturating_sub(self.visible_fixed_rows(visible_rows))
    }

    fn scrollable_row_count(&self) -> usize {
        self.rows.len().saturating_sub(self.fixed_data_rows())
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
        let fixed_rows = self.fixed_data_rows();
        if self.selected < fixed_rows {
            self.offset = 0;
            return;
        }
        let scrollable_visible = self.scrollable_visible_rows(height);
        if scrollable_visible == 0 {
            self.offset = 0;
            return;
        }
        let selected_scroll_index = self.selected.saturating_sub(fixed_rows);
        if selected_scroll_index < self.offset {
            self.offset = selected_scroll_index;
        } else if selected_scroll_index >= self.offset + scrollable_visible {
            self.offset = selected_scroll_index + 1 - scrollable_visible;
        }
        self.offset = ScrollView::line_clamp_offset(
            self.offset,
            self.scrollable_row_count(),
            scrollable_visible,
        );
    }

    fn visible_rows(&self) -> usize {
        self.content_height.saturating_sub(1) as usize
    }

    fn effective_offset(&self, visible_rows: usize) -> usize {
        if self.rows.is_empty() || visible_rows == 0 {
            return 0;
        }
        let fixed_rows = self.fixed_data_rows();
        let scrollable_visible = self.scrollable_visible_rows(visible_rows);
        if scrollable_visible == 0 {
            return 0;
        }
        let mut offset = ScrollView::line_clamp_offset(
            self.offset,
            self.scrollable_row_count(),
            scrollable_visible,
        );
        if self.selected < fixed_rows {
            return offset;
        }
        let selected_scroll_index = self.selected.saturating_sub(fixed_rows);
        if selected_scroll_index < offset {
            offset = selected_scroll_index;
        } else if selected_scroll_index >= offset + scrollable_visible {
            offset = selected_scroll_index + 1 - scrollable_visible;
        }
        ScrollView::line_clamp_offset(offset, self.scrollable_row_count(), scrollable_visible)
    }

    fn row_index_from_y(&self, y: usize, visible_rows: usize) -> Option<usize> {
        if y == 0 {
            return None;
        }
        let data_y = y - 1;
        let fixed_visible = self.visible_fixed_rows(visible_rows);
        if data_y < fixed_visible {
            return Some(data_y);
        }
        let scroll_slot = data_y.saturating_sub(fixed_visible);
        let scrollable_visible = self.scrollable_visible_rows(visible_rows);
        if scroll_slot >= scrollable_visible {
            return None;
        }
        let fixed_rows = self.fixed_data_rows();
        let row_index = fixed_rows + self.effective_offset(visible_rows) + scroll_slot;
        (row_index < self.rows.len()).then_some(row_index)
    }
}

impl Default for DataTable {
    fn default() -> Self {
        let mut out = Self {
            id: WidgetId::new(),
            column_keys: Vec::new(),
            headers: Vec::new(),
            row_keys: Vec::new(),
            rows: Vec::new(),
            column_widths: Vec::new(),
            selected: 0,
            offset: 0,
            cursor_column: 0,
            cursor_type: CursorType::Cell,
            fixed_rows: 0,
            fixed_columns: 0,
            next_row_key: 0,
            next_column_key: 0,
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
        let visible_rows = self.visible_rows();
        let next = if y == 0 {
            // Header row — use usize::MAX as sentinel (mirrors Textual's row_index=-1).
            Some((usize::MAX, col_idx))
        } else {
            self.row_index_from_y(y as usize, visible_rows)
                .map(|row_idx| (row_idx, col_idx))
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
                    if let Some(clicked_row) = self.row_index_from_y(mouse.y as usize, visible_rows)
                    {
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
                        let step = visible_rows.max(1).min(self.selected);
                        self.selected -= step;
                        selection_changed = true;
                    }
                    handled = true;
                }
            }
            Event::Action(Action::ScrollPageDown) => {
                if matches!(self.cursor_type, CursorType::Cell | CursorType::Row) {
                    if self.selected + 1 < self.rows.len() {
                        let step = visible_rows
                            .max(1)
                            .min(self.rows.len().saturating_sub(1) - self.selected);
                        self.selected += step;
                        selection_changed = true;
                    }
                    handled = true;
                }
            }
            Event::Action(Action::ScrollPageLeft) => {
                if matches!(self.cursor_type, CursorType::Cell | CursorType::Column) {
                    if self.cursor_column > 0 {
                        let step = 5.min(self.cursor_column);
                        self.cursor_column -= step;
                        cursor_changed = true;
                    }
                    handled = true;
                }
            }
            Event::Action(Action::ScrollPageRight) => {
                if matches!(self.cursor_type, CursorType::Cell | CursorType::Column) {
                    if self.cursor_column + 1 < self.headers.len() {
                        let step = 5.min(self.headers.len().saturating_sub(1) - self.cursor_column);
                        self.cursor_column += step;
                        cursor_changed = true;
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
                            let step = visible_rows.max(1).min(self.selected);
                            self.selected -= step;
                            selection_changed = true;
                        }
                        handled = true;
                    }
                }
                KeyCode::PageDown => {
                    if matches!(self.cursor_type, CursorType::Cell | CursorType::Row) {
                        if self.selected + 1 < self.rows.len() {
                            let step = visible_rows
                                .max(1)
                                .min(self.rows.len().saturating_sub(1) - self.selected);
                            self.selected += step;
                            selection_changed = true;
                        }
                        handled = true;
                    }
                }
                KeyCode::Home => {
                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                        if matches!(self.cursor_type, CursorType::Cell | CursorType::Row)
                            && self.selected != 0
                        {
                            self.selected = 0;
                            selection_changed = true;
                        }
                    } else if matches!(self.cursor_type, CursorType::Cell | CursorType::Column)
                        && self.cursor_column != 0
                    {
                        self.cursor_column = 0;
                        cursor_changed = true;
                    }
                    handled = true;
                }
                KeyCode::End => {
                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                        if matches!(self.cursor_type, CursorType::Cell | CursorType::Row)
                            && !self.rows.is_empty()
                        {
                            let row = self.rows.len() - 1;
                            if self.selected != row {
                                self.selected = row;
                                selection_changed = true;
                            }
                        }
                    } else if matches!(self.cursor_type, CursorType::Cell | CursorType::Column)
                        && !self.headers.is_empty()
                    {
                        let col = self.headers.len() - 1;
                        if self.cursor_column != col {
                            self.cursor_column = col;
                            cursor_changed = true;
                        }
                    }
                    handled = true;
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
        if (selection_changed || cursor_changed)
            && !self.rows.is_empty()
            && !self.headers.is_empty()
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
        let fixed_bg = parse_color_like("$secondary-muted");

        let fallback_bg = parse_color_like("$background").unwrap_or(Color::rgb(0, 0, 0));
        let header_base = header_bg.unwrap_or(fallback_bg);
        let row_base = row_bg.unwrap_or(fallback_bg);
        let hover_bg = hover_bg.map(|c| c.flatten_over(row_base));
        let header_hover_bg = header_hover_bg.map(|c| c.flatten_over(header_base));
        let fixed_base = fixed_bg
            .map(|c| c.flatten_over(row_base))
            .unwrap_or(row_base);

        let header_style = rich_rs::Style::new()
            .with_bold(true)
            .with_bgcolor(header_base.to_simple_opaque());
        let normal_style = rich_rs::Style::new().with_bgcolor(row_base.to_simple_opaque());
        let fixed_style = rich_rs::Style::new().with_bgcolor(fixed_base.to_simple_opaque());
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

        let fixed_data_rows = self.fixed_data_rows();
        let fixed_visible = self.visible_fixed_rows(visible_rows);

        // Header line (headers use usize::MAX as their row sentinel).
        emit_row_per_cell(
            &self.headers,
            column_widths,
            width,
            |col_idx| {
                let target = (usize::MAX, col_idx);
                if show_cursor && should_highlight(cursor_coord, target, cursor_type) {
                    return selected_style.with_bold(true);
                }
                if let Some(hc) = hover_coord
                    && should_highlight(hc, target, cursor_type)
                {
                    return header_hover_style;
                }
                if col_idx < self.fixed_columns {
                    return fixed_style.with_bold(true);
                }
                header_style
            },
            header_style,
            &mut out,
        );
        out.push(Segment::line());
        let mut rendered_rows = 0usize;

        let mut emit_data_row = |row_idx: usize, out: &mut Segments| {
            if rendered_rows >= visible_rows as usize {
                return;
            }
            let Some(row) = self.rows.get(row_idx) else {
                return;
            };
            emit_row_per_cell(
                row,
                column_widths,
                width,
                |col_idx| {
                    let target = (row_idx, col_idx);
                    let is_fixed_target = row_idx < fixed_data_rows || col_idx < self.fixed_columns;
                    let base = if is_fixed_target {
                        fixed_style
                    } else {
                        normal_style
                    };
                    if show_cursor && should_highlight(cursor_coord, target, cursor_type) {
                        return selected_style;
                    }
                    if let Some(hc) = hover_coord
                        && should_highlight(hc, target, cursor_type)
                    {
                        return hover_style;
                    }
                    base
                },
                normal_style,
                out,
            );
            out.push(Segment::line());
            rendered_rows += 1;
        };

        for fixed_row_idx in 0..fixed_visible {
            emit_data_row(fixed_row_idx, &mut out);
        }
        let scroll_start = fixed_data_rows + offset;
        let scrollable_slots = (visible_rows as usize).saturating_sub(fixed_visible);
        for row_offset in 0..scrollable_slots {
            emit_data_row(scroll_start + row_offset, &mut out);
        }

        out
    }

    fn layout_height(&self) -> Option<usize> {
        let intrinsic = 1usize.saturating_add(self.rows.len().max(1));
        fixed_height_from_constraints(self.layout_constraints()).or(Some(intrinsic))
    }

    fn content_width(&self) -> Option<usize> {
        let columns = self.headers.len().max(1);
        let widths = self.column_widths();
        let cells_width = widths.iter().copied().sum::<usize>();
        let gaps_width = columns.saturating_sub(1).saturating_mul(2);
        Some(cells_width.saturating_add(gaps_width).max(1))
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
    for (i, col_w) in column_widths.iter().copied().enumerate() {
        if i > 0 {
            out.push(Segment::styled("  ", gap_style));
            used += 2;
        }
        let val = values.get(i).map(String::as_str).unwrap_or("");
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
    use crate::keys::KeyEventData;
    use crate::message::Message;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

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
        table.on_layout(20, 4);
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
        table.on_layout(20, 4);
        table.set_hovered(true);
        table.on_mouse_move(0, 1);
        assert_eq!(table.hover_coordinate, Some((0, 0)));

        table.set_hovered(false);
        assert_eq!(table.hover_coordinate, None);
    }

    #[test]
    fn mouse_move_column_mapping_uses_cell_width_for_wide_graphemes() {
        let mut table = DataTable::new(
            vec!["👩‍🚀".into(), "B".into()],
            vec![vec!["x".into(), "y".into()]],
        );
        table.set_hovered(true);

        // First header uses two display cells; x=0..1 should still map to col 0.
        table.on_mouse_move(0, 0);
        assert_eq!(table.hover_coordinate, Some((usize::MAX, 0)));
        table.on_mouse_move(1, 0);
        assert_eq!(table.hover_coordinate, Some((usize::MAX, 0)));
        // x=2 enters the inter-column gap, x=3 reaches second column.
        table.on_mouse_move(3, 0);
        assert_eq!(table.hover_coordinate, Some((usize::MAX, 1)));
    }

    #[test]
    fn supports_keyed_rows_and_columns_lookup() {
        let mut table = DataTable::empty();
        let col = table
            .add_column_with_key("lane", "Lane")
            .expect("new column key");
        assert_eq!(table.add_column_with_key("lane", "Lane 2"), None);
        let row = table
            .add_row_with_key("heat-1", vec!["1"])
            .expect("new row key");
        assert_eq!(table.add_row_with_key("heat-1", vec!["2"]), None);

        assert_eq!(table.column_index_of(&col), Some(0));
        assert_eq!(table.row_index_of(&row), Some(0));
        assert_eq!(table.cell_key_at(0, 0), Some((row, col)));
    }

    #[test]
    fn fixed_rows_are_mapped_before_scrolled_rows() {
        let mut table = DataTable::new(
            vec!["A".into()],
            (0..5).map(|n| vec![format!("row{n}")]).collect(),
        );
        table.set_fixed_rows(1);
        table.content_height = 3; // header + 2 visible rows
        table.set_selected(4);

        // y=1 is fixed row 0, y=2 is the first scrolled row.
        assert_eq!(table.row_index_from_y(1, table.visible_rows()), Some(0));
        assert_eq!(table.row_index_from_y(2, table.visible_rows()), Some(4));
    }

    #[test]
    fn home_end_navigation_matches_cursor_and_control_semantics() {
        let mut table = DataTable::new(
            vec!["A".into(), "B".into(), "C".into()],
            (0..5)
                .map(|n| vec![format!("row{n}"), "x".into(), "y".into()])
                .collect(),
        );
        table.set_focus(true);
        table.content_height = 4;
        table.set_cursor(3, 2);
        let mut ctx = EventCtx::default();

        table.on_event(
            &Event::Key(KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Home,
                KeyModifiers::NONE,
            ))),
            &mut ctx,
        );
        assert_eq!(table.cursor(), (3, 0));

        table.on_event(
            &Event::Key(KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::End,
                KeyModifiers::NONE,
            ))),
            &mut ctx,
        );
        assert_eq!(table.cursor(), (3, 2));

        table.on_event(
            &Event::Key(KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Home,
                KeyModifiers::CONTROL,
            ))),
            &mut ctx,
        );
        assert_eq!(table.cursor(), (0, 2));

        table.on_event(
            &Event::Key(KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::End,
                KeyModifiers::CONTROL,
            ))),
            &mut ctx,
        );
        assert_eq!(table.cursor(), (4, 2));
    }

    #[test]
    fn header_click_posts_header_selected_message() {
        let mut table = DataTable::new(
            vec!["A".into(), "B".into()],
            vec![vec!["r0".into(), "c0".into()]],
        );
        let id = table.id();
        let mut ctx = EventCtx::default();

        table.on_event(
            &Event::MouseDown(MouseDownEvent {
                target: id,
                screen_x: 4,
                screen_y: 0,
                x: 4,
                y: 0,
            }),
            &mut ctx,
        );

        assert!(ctx.handled());
        let messages = ctx.take_messages();
        assert_eq!(messages.len(), 1);
        assert!(matches!(
            messages[0].message,
            Message::DataTableHeaderSelected { column: 1 }
        ));
    }

    #[test]
    fn keyboard_navigation_posts_cursor_moved_message() {
        let mut table = DataTable::new(
            vec!["A".into(), "B".into()],
            vec![
                vec!["r0".into(), "c0".into()],
                vec!["r1".into(), "c1".into()],
            ],
        );
        table.set_focus(true);
        let mut ctx = EventCtx::default();

        table.on_event(
            &Event::Key(KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Down,
                KeyModifiers::NONE,
            ))),
            &mut ctx,
        );

        assert!(ctx.handled());
        let messages = ctx.take_messages();
        assert_eq!(messages.len(), 1);
        assert!(matches!(
            messages[0].message,
            Message::DataTableCursorMoved { row: 1, column: 0 }
        ));
    }

    #[test]
    fn enter_posts_cell_activated_message() {
        let mut table = DataTable::new(
            vec!["A".into(), "B".into()],
            vec![
                vec!["r0".into(), "c0".into()],
                vec!["r1".into(), "c1".into()],
            ],
        );
        table.set_focus(true);
        table.set_cursor(1, 1);
        let mut ctx = EventCtx::default();

        table.on_event(
            &Event::Key(KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Enter,
                KeyModifiers::NONE,
            ))),
            &mut ctx,
        );

        assert!(ctx.handled());
        let messages = ctx.take_messages();
        assert_eq!(messages.len(), 1);
        assert!(matches!(
            messages[0].message,
            Message::DataTableCellActivated { row: 1, column: 1 }
        ));
    }
}
