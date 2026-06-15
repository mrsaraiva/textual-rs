use crossterm::event::{KeyCode, KeyModifiers};
use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::event::{Action, Event, EventCtx};
use crate::message::*;
use crate::style::{Color, parse_color_like};

use crate::action::ParsedAction;
use crate::reactive::{ReactiveChange, ReactiveCtx, ReactiveFlags, ReactiveWidget};

use super::{BindingDecl, NodeSeed, ScrollBar, ScrollView, Widget};

pub(crate) const DATA_TABLE_HSCROLLBAR_ID: &str = "__data_table_hscrollbar";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorType {
    Cell,
    Row,
    Column,
    None,
}

/// Horizontal justification of a cell's text within its column width.
///
/// Mirrors the `justify` attribute of a Rich `Text` cell in Python Textual
/// (for example `Text(str(cell), justify="right")`). Default is `Left`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CellJustify {
    #[default]
    Left,
    Right,
    Center,
}

impl CellJustify {
    /// Lay `text` out in `width` cells using this justification, truncating with
    /// `set_cell_size` when the content is wider than the column.
    fn render(self, text: &str, width: usize) -> String {
        let len = rich_rs::cell_len(text);
        if len >= width {
            // Truncation (and the trivial exact-fit case) is identical for every
            // justification: clamp from the left, matching `set_cell_size`.
            return rich_rs::set_cell_size(text, width);
        }
        let pad = width - len;
        match self {
            CellJustify::Left => rich_rs::set_cell_size(text, width),
            CellJustify::Right => format!("{}{}", " ".repeat(pad), text),
            CellJustify::Center => {
                let left = pad / 2;
                let right = pad - left;
                format!("{}{}{}", " ".repeat(left), text, " ".repeat(right))
            }
        }
    }
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
    column_keys: Vec<ColumnKey>,
    headers: Vec<String>,
    row_keys: Vec<RowKey>,
    rows: Vec<Vec<String>>,
    /// Optional per-row label (parallel to `rows`). When any row has a label and
    /// `show_row_labels` is set, a non-data label column is rendered as a prefix.
    row_labels: Vec<Option<String>>,
    /// Per-cell horizontal justification (parallel to `rows`/cells). Mirrors the
    /// `justify` attribute of a Rich `Text` cell in Python. Empty rows / missing
    /// entries default to `CellJustify::Left`. Headers are never justified here
    /// (they are added as plain strings via `add_column`).
    cell_justify: Vec<Vec<CellJustify>>,
    column_widths: Vec<usize>,
    selected: usize,
    offset: usize,
    cursor_column: usize,
    cursor_type: CursorType,
    fixed_rows: usize,
    fixed_columns: usize,
    horizontal_offset: usize,
    next_row_key: usize,
    next_column_key: usize,
    content_width: u16,
    content_height: u16,
    hover_coordinate: Option<(usize, usize)>,
    scrollbar_extracted: bool,
    show_header: bool,
    show_row_labels: bool,
    zebra_stripes: bool,
    seed: NodeSeed,
}

#[derive(Debug, Clone, Copy)]
struct HorizontalScrollbarState {
    content_width: usize,
    pixel_offset: usize,
    max_pixel_offset: usize,
}

impl DataTable {
    pub fn new(headers: Vec<String>, rows: Vec<Vec<String>>) -> Self {
        let mut out = Self {
            column_keys: Vec::new(),
            headers: Vec::new(),
            row_keys: Vec::new(),
            rows: Vec::new(),
            row_labels: Vec::new(),
            cell_justify: Vec::new(),
            column_widths: Vec::new(),
            selected: 0,
            offset: 0,
            cursor_column: 0,
            cursor_type: CursorType::Cell,
            fixed_rows: 0,
            fixed_columns: 0,
            horizontal_offset: 0,
            next_row_key: 0,
            next_column_key: 0,
            content_width: 0,
            content_height: 0,
            hover_coordinate: None,
            scrollbar_extracted: false,
            show_header: true,
            show_row_labels: true,
            zebra_stripes: false,
            seed: NodeSeed::default(),
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
        self.row_labels.push(None);
        self.cell_justify.push(Vec::new());
        self.clamp_indices();
        self.recompute_column_widths();
        key
    }

    /// Add a row with a row label (Python `add_row(..., label=…)`). When any row
    /// is labelled and `show_row_labels` is set, a non-data label column is
    /// rendered as a prefix to the left of the data cells.
    pub fn add_row_labeled<S, L>(&mut self, row: Vec<S>, label: L) -> RowKey
    where
        S: ToString,
        L: Into<String>,
    {
        let key = self.generate_row_key();
        self.row_keys.push(key.clone());
        self.rows
            .push(row.into_iter().map(|value| value.to_string()).collect());
        self.row_labels.push(Some(label.into()));
        self.cell_justify.push(Vec::new());
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
        self.row_labels.push(None);
        self.cell_justify.push(Vec::new());
        self.clamp_indices();
        self.recompute_column_widths();
        Some(key)
    }

    /// Set the horizontal justification of a single data cell. Mirrors building a
    /// cell from a Rich `Text(..., justify=…)` in Python. Out-of-range rows are
    /// ignored; the per-row justify vector is grown to fit the column index.
    pub fn set_cell_justify(&mut self, row: usize, col: usize, justify: CellJustify) {
        let Some(row_justify) = self.cell_justify.get_mut(row) else {
            return;
        };
        if row_justify.len() <= col {
            row_justify.resize(col + 1, CellJustify::Left);
        }
        row_justify[col] = justify;
    }

    /// Apply one justification to every cell in a data row.
    pub fn set_row_justify(&mut self, row: usize, justify: CellJustify) {
        let cols = self.headers.len().max(self.rows.get(row).map_or(0, Vec::len));
        for col in 0..cols {
            self.set_cell_justify(row, col, justify);
        }
    }

    /// Apply one justification to every data cell currently in the table.
    /// Headers are unaffected (they are plain strings). This mirrors the common
    /// Python pattern of wrapping every data cell in `Text(..., justify=…)`.
    pub fn set_all_data_cells_justify(&mut self, justify: CellJustify) {
        for row in 0..self.rows.len() {
            self.set_row_justify(row, justify);
        }
    }

    /// Justification for a specific data cell (defaults to `Left`).
    fn cell_justify_at(&self, row: usize, col: usize) -> CellJustify {
        self.cell_justify
            .get(row)
            .and_then(|r| r.get(col))
            .copied()
            .unwrap_or_default()
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

    // ── Reactive getters ─────────────────────────────────────────────────

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn selected_column(&self) -> usize {
        self.cursor_column
    }

    pub fn cursor(&self) -> (usize, usize) {
        (self.selected, self.cursor_column)
    }

    pub fn fixed_rows(&self) -> usize {
        self.fixed_rows
    }

    pub fn fixed_columns(&self) -> usize {
        self.fixed_columns
    }

    pub fn show_header(&self) -> bool {
        self.show_header
    }

    pub fn show_row_labels(&self) -> bool {
        self.show_row_labels
    }

    pub fn zebra_stripes(&self) -> bool {
        self.zebra_stripes
    }

    // Note: getter for `cursor_type` is not generated because
    // it conflicts with the existing builder method of the same name.

    // ── Reactive setters ─────────────────────────────────────────────────

    /// Reactive setter for `selected`.
    pub fn set_selected(&mut self, index: usize, ctx: &mut ReactiveCtx) {
        if self.rows.is_empty() {
            self.selected = 0;
            self.offset = 0;
            return;
        }
        let new_selected = index.min(self.rows.len() - 1);
        if self.selected != new_selected {
            let old = self.selected;
            self.selected = new_selected;
            self.ensure_visible(self.visible_rows());
            ctx.record_change(
                "selected",
                ReactiveFlags::reactive(),
                Box::new(old),
                Box::new(self.selected),
            );
        }
    }

    /// Reactive setter for cursor position (row + column).
    pub fn set_cursor(&mut self, row: usize, column: usize, ctx: &mut ReactiveCtx) {
        self.set_selected(row, ctx);
        if self.headers.is_empty() {
            self.cursor_column = 0;
        } else {
            let new_col = column.min(self.headers.len() - 1);
            if self.cursor_column != new_col {
                let old = self.cursor_column;
                self.cursor_column = new_col;
                ctx.record_change(
                    "cursor_column",
                    ReactiveFlags::reactive(),
                    Box::new(old),
                    Box::new(self.cursor_column),
                );
            }
        }
        self.ensure_cursor_column_visible(self.content_width as usize);
    }

    /// Reactive setter for `cursor_type`.
    pub fn set_cursor_type(&mut self, ct: CursorType, ctx: &mut ReactiveCtx) {
        if self.cursor_type != ct {
            let old = self.cursor_type;
            self.cursor_type = ct;
            ctx.record_change(
                "cursor_type",
                ReactiveFlags::reactive(),
                Box::new(old),
                Box::new(ct),
            );
        }
    }

    /// Reactive setter for `fixed_rows`.
    pub fn set_fixed_rows(&mut self, count: usize, ctx: &mut ReactiveCtx) {
        if self.fixed_rows != count {
            let old = self.fixed_rows;
            self.fixed_rows = count;
            self.ensure_visible(self.visible_rows());
            ctx.record_change(
                "fixed_rows",
                ReactiveFlags::reactive_layout(),
                Box::new(old),
                Box::new(count),
            );
        }
    }

    /// Reactive setter for `fixed_columns`.
    pub fn set_fixed_columns(&mut self, count: usize, ctx: &mut ReactiveCtx) {
        if self.fixed_columns != count {
            let old = self.fixed_columns;
            self.fixed_columns = count;
            self.clamp_horizontal_offset();
            self.ensure_cursor_column_visible(self.content_width as usize);
            ctx.record_change(
                "fixed_columns",
                ReactiveFlags::reactive_layout(),
                Box::new(old),
                Box::new(count),
            );
        }
    }

    /// Reactive setter for `show_header`.
    pub fn set_show_header(&mut self, show: bool, ctx: &mut ReactiveCtx) {
        if self.show_header != show {
            let old = self.show_header;
            self.show_header = show;
            ctx.record_change(
                "show_header",
                ReactiveFlags::reactive_layout(),
                Box::new(old),
                Box::new(show),
            );
        }
    }

    /// Reactive setter for `show_row_labels`.
    pub fn set_show_row_labels(&mut self, show: bool, ctx: &mut ReactiveCtx) {
        if self.show_row_labels != show {
            let old = self.show_row_labels;
            self.show_row_labels = show;
            ctx.record_change(
                "show_row_labels",
                ReactiveFlags::reactive(),
                Box::new(old),
                Box::new(show),
            );
        }
    }

    /// Reactive setter for `zebra_stripes`.
    pub fn set_zebra_stripes(&mut self, enabled: bool, ctx: &mut ReactiveCtx) {
        if self.zebra_stripes != enabled {
            let old = self.zebra_stripes;
            self.zebra_stripes = enabled;
            ctx.record_change(
                "zebra_stripes",
                ReactiveFlags::reactive(),
                Box::new(old),
                Box::new(enabled),
            );
        }
    }

    // ── Watchers ─────────────────────────────────────────────────────────

    fn watch_cursor_type(&mut self, _old: &CursorType, _new: &CursorType, _ctx: &mut ReactiveCtx) {
        // Visual change only — repaint is handled by ReactiveFlags.
    }

    fn watch_show_header(&mut self, _old: &bool, _new: &bool, _ctx: &mut ReactiveCtx) {
        // Visible row count changes — recompute scroll offsets.
        self.ensure_visible(self.visible_rows());
    }

    fn watch_zebra_stripes(&mut self, _old: &bool, _new: &bool, _ctx: &mut ReactiveCtx) {
        // Visual change only — repaint is handled by ReactiveFlags.
    }

    // ── Builder methods ─────────────────────────────────────────────────

    pub fn cursor_type(mut self, ct: CursorType) -> Self {
        self.cursor_type = ct;
        self
    }

    // --- API methods (QW-10) ---

    /// Remove a row by key. Returns the row data if found, or `None` if the
    /// key doesn't exist in the table.
    pub fn remove_row(&mut self, row_key: &RowKey) -> Option<Vec<String>> {
        let index = self.row_keys.iter().position(|k| k == row_key)?;
        self.row_keys.remove(index);
        let row_data = self.rows.remove(index);
        if index < self.row_labels.len() {
            self.row_labels.remove(index);
        }
        if index < self.cell_justify.len() {
            self.cell_justify.remove(index);
        }
        self.clamp_indices();
        self.recompute_column_widths();
        Some(row_data)
    }

    /// Remove a row by index. Returns the row data if the index is valid.
    pub fn remove_row_at(&mut self, index: usize) -> Option<Vec<String>> {
        if index >= self.rows.len() {
            return None;
        }
        self.row_keys.remove(index);
        let row_data = self.rows.remove(index);
        if index < self.row_labels.len() {
            self.row_labels.remove(index);
        }
        if index < self.cell_justify.len() {
            self.cell_justify.remove(index);
        }
        self.clamp_indices();
        self.recompute_column_widths();
        Some(row_data)
    }

    /// Remove all rows (and optionally all columns).
    pub fn clear(&mut self, clear_columns: bool) {
        self.rows.clear();
        self.row_keys.clear();
        self.row_labels.clear();
        self.cell_justify.clear();
        self.next_row_key = 0;
        if clear_columns {
            self.headers.clear();
            self.column_keys.clear();
            self.column_widths.clear();
            self.next_column_key = 0;
        }
        self.selected = 0;
        self.offset = 0;
        self.cursor_column = 0;
        self.horizontal_offset = 0;
        self.hover_coordinate = None;
        self.recompute_column_widths();
    }

    /// Sort rows by a given column index. If `reverse` is true, the order is
    /// descending. Columns are compared lexicographically by cell text.
    pub fn sort(&mut self, column: usize, reverse: bool) {
        if column >= self.headers.len() || self.rows.is_empty() {
            return;
        }
        // Build index-based permutation so we can reorder row_keys in sync.
        let mut indices: Vec<usize> = (0..self.rows.len()).collect();
        indices.sort_by(|&a, &b| {
            let va = self.rows[a].get(column).map(String::as_str).unwrap_or("");
            let vb = self.rows[b].get(column).map(String::as_str).unwrap_or("");
            if reverse { vb.cmp(va) } else { va.cmp(vb) }
        });
        let sorted_rows: Vec<Vec<String>> = indices.iter().map(|&i| self.rows[i].clone()).collect();
        let sorted_keys: Vec<RowKey> = indices.iter().map(|&i| self.row_keys[i].clone()).collect();
        self.rows = sorted_rows;
        self.row_keys = sorted_keys;
        if self.row_labels.len() == indices.len() {
            self.row_labels = indices.iter().map(|&i| self.row_labels[i].clone()).collect();
        }
        if self.cell_justify.len() == indices.len() {
            self.cell_justify = indices.iter().map(|&i| self.cell_justify[i].clone()).collect();
        }
        self.clamp_indices();
    }

    /// Update the value of a specific cell. Returns `true` if the cell existed
    /// and was updated, `false` if the coordinates are out of bounds.
    pub fn update_cell(&mut self, row: usize, col: usize, value: impl ToString) -> bool {
        if let Some(cell) = self.rows.get_mut(row).and_then(|r| r.get_mut(col)) {
            *cell = value.to_string();
            self.recompute_column_widths();
            true
        } else {
            false
        }
    }

    /// Get the value of a specific cell, or `None` if out of bounds.
    pub fn get_cell(&self, row: usize, col: usize) -> Option<&str> {
        self.rows
            .get(row)
            .and_then(|r| r.get(col))
            .map(String::as_str)
    }

    /// Get all values in a row, or `None` if the row index is out of bounds.
    pub fn get_row(&self, row: usize) -> Option<&[String]> {
        self.rows.get(row).map(Vec::as_slice)
    }

    /// Number of rows in the table.
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// Number of columns in the table.
    pub fn column_count(&self) -> usize {
        self.headers.len()
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
        self.clamp_horizontal_offset();
    }

    fn fixed_column_count(&self) -> usize {
        self.fixed_columns.min(self.headers.len())
    }

    fn scrollable_column_count(&self) -> usize {
        self.headers.len().saturating_sub(self.fixed_column_count())
    }

    fn clamp_horizontal_offset(&mut self) {
        let scrollable = self.scrollable_column_count();
        self.horizontal_offset = if scrollable == 0 {
            0
        } else {
            self.horizontal_offset.min(scrollable.saturating_sub(1))
        };
    }

    fn rendered_column_indices_with_offset(&self, offset: usize) -> Vec<usize> {
        let total = self.headers.len();
        if total == 0 {
            return Vec::new();
        }
        let fixed = self.fixed_column_count();
        let mut columns: Vec<usize> = (0..fixed).collect();
        if fixed < total {
            let clamped_offset = offset.min(total - fixed - 1);
            columns.extend((fixed + clamped_offset)..total);
        }
        columns
    }

    fn rendered_column_indices(&self) -> Vec<usize> {
        self.rendered_column_indices_with_offset(self.horizontal_offset)
    }

    fn column_is_visible_at_width(&self, column: usize, width: usize, offset: usize) -> bool {
        if width == 0 {
            return false;
        }
        // Columns start after the leading CELL_PADDING space.
        let usable = width.saturating_sub(CELL_PADDING);
        let columns = self.rendered_column_indices_with_offset(offset);
        let mut pos = 0usize;
        for (idx, col) in columns.iter().enumerate() {
            if idx > 0 {
                pos = pos.saturating_add(2);
            }
            let col_width = *self.column_widths.get(*col).unwrap_or(&0);
            let start = pos;
            let end = start.saturating_add(col_width);
            if *col == column {
                // Fully visible: column starts within usable space and ends within it.
                return start < usable && end <= usable;
            }
            pos = end;
        }
        false
    }

    fn ensure_cursor_column_visible(&mut self, width: usize) {
        if self.headers.is_empty() {
            self.horizontal_offset = 0;
            return;
        }
        if self.cursor_column < self.fixed_column_count() {
            self.horizontal_offset = 0;
            return;
        }
        let first_scrollable = self.fixed_column_count();
        if !self.column_is_visible_at_width(first_scrollable, width, 0) {
            // No horizontal viewport space remains after fixed columns; keep a stable offset.
            self.horizontal_offset = 0;
            return;
        }
        let max_offset = self.scrollable_column_count().saturating_sub(1);
        self.horizontal_offset = self.horizontal_offset.min(max_offset);
        while self.horizontal_offset < max_offset
            && !self.column_is_visible_at_width(self.cursor_column, width, self.horizontal_offset)
        {
            self.horizontal_offset += 1;
        }
        while self.horizontal_offset > 0
            && self.column_is_visible_at_width(
                self.cursor_column,
                width,
                self.horizontal_offset - 1,
            )
        {
            self.horizontal_offset -= 1;
        }
    }

    fn column_at_x_in_rendered_columns(&self, x: usize, rendered_columns: &[usize]) -> usize {
        if rendered_columns.is_empty() {
            return 0;
        }
        // Adjust for leading CELL_PADDING space.
        let x = x.saturating_sub(CELL_PADDING);
        let mut pos = 0usize;
        for (idx, col) in rendered_columns.iter().enumerate() {
            if idx > 0 {
                pos = pos.saturating_add(2);
            }
            let width = *self.column_widths.get(*col).unwrap_or(&0);
            let end = pos.saturating_add(width);
            if x < end {
                return *col;
            }
            pos = end;
        }
        *rendered_columns.last().unwrap_or(&0)
    }

    fn fixed_section_width(&self) -> usize {
        let fixed = self.fixed_column_count();
        if fixed == 0 {
            return 0;
        }
        let mut width = 0usize;
        for (idx, col) in (0..fixed).enumerate() {
            if idx > 0 {
                width = width.saturating_add(2);
            }
            width = width.saturating_add(*self.column_widths.get(col).unwrap_or(&0));
        }
        width
    }

    fn scrollable_content_width(&self) -> usize {
        let fixed = self.fixed_column_count();
        let scrollable = self.headers.len().saturating_sub(fixed);
        if scrollable == 0 {
            return 0;
        }
        let mut width = 0usize;
        for index in 0..scrollable {
            if index > 0 {
                width = width.saturating_add(2);
            }
            width = width.saturating_add(*self.column_widths.get(fixed + index).unwrap_or(&0));
        }
        width
    }

    fn scrollable_viewport_width(&self, width: usize) -> usize {
        let fixed = self.fixed_column_count();
        if fixed >= self.headers.len() {
            return 0;
        }
        let fixed_width = self.fixed_section_width();
        let inter_gap = if fixed > 0 { 2 } else { 0 };
        width.saturating_sub(fixed_width.saturating_add(inter_gap))
    }

    fn horizontal_offset_pixels(&self) -> usize {
        let fixed = self.fixed_column_count();
        let scrollable = self.scrollable_column_count();
        let offset = self.horizontal_offset.min(scrollable.saturating_sub(1));
        let mut pixels = 0usize;
        for idx in 0..offset {
            pixels = pixels
                .saturating_add(*self.column_widths.get(fixed + idx).unwrap_or(&0))
                .saturating_add(2);
        }
        pixels
    }

    fn horizontal_offset_from_pixels(&self, pixels: usize) -> usize {
        let fixed = self.fixed_column_count();
        let scrollable = self.scrollable_column_count();
        if scrollable == 0 {
            return 0;
        }
        let max_offset = scrollable.saturating_sub(1);
        let mut offset = 0usize;
        let mut consumed = 0usize;
        while offset < max_offset {
            let step = self
                .column_widths
                .get(fixed + offset)
                .copied()
                .unwrap_or(0)
                .saturating_add(2);
            if consumed.saturating_add(step) > pixels {
                break;
            }
            consumed = consumed.saturating_add(step);
            offset += 1;
        }
        offset
    }

    fn horizontal_scrollbar_state(&self, width: usize) -> Option<HorizontalScrollbarState> {
        if width == 0 {
            return None;
        }
        let viewport_width = self.scrollable_viewport_width(width);
        let content_width = self.scrollable_content_width();
        if viewport_width == 0 || content_width <= viewport_width {
            return None;
        }
        let max_pixel_offset = content_width.saturating_sub(viewport_width);
        let pixel_offset = self.horizontal_offset_pixels().min(max_pixel_offset);
        Some(HorizontalScrollbarState {
            content_width,
            pixel_offset,
            max_pixel_offset,
        })
    }

    fn page_horizontal_step(&self, width: usize) -> usize {
        let visible = self
            .rendered_column_indices_with_offset(self.horizontal_offset)
            .len()
            .saturating_sub(self.fixed_column_count());
        visible.saturating_sub(1).max((width > 0) as usize)
    }

    fn scroll_horizontal_by_columns(&mut self, delta: i32) -> bool {
        let scrollable = self.scrollable_column_count();
        if scrollable == 0 {
            self.horizontal_offset = 0;
            return false;
        }
        let max_offset = scrollable.saturating_sub(1);
        let next = if delta.is_negative() {
            self.horizontal_offset
                .saturating_sub(delta.unsigned_abs() as usize)
        } else {
            self.horizontal_offset.saturating_add(delta as usize)
        }
        .min(max_offset);
        if next == self.horizontal_offset {
            return false;
        }
        self.horizontal_offset = next;
        true
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
        // Column width = max(header label width, cell content widths), with a
        // minimum of 1 (Python `measure(console, renderable, minimum=1)`).
        // Do NOT impose a larger artificial minimum: Python sizes columns to
        // their content, so e.g. a single-letter header over 2-digit values
        // yields width 2, not 3.
        let mut widths: Vec<usize> = self
            .headers
            .iter()
            .map(|h| rich_rs::cell_len(h).max(1))
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

    /// Width of the non-data row-label column, or 0 when no row is labelled or
    /// labels are hidden. Mirrors Python: the label column appears only when at
    /// least one row has a label AND `show_row_labels` is set.
    fn label_col_width(&self) -> usize {
        if !self.show_row_labels {
            return 0;
        }
        self.row_labels
            .iter()
            .filter_map(|l| l.as_deref())
            .map(rich_rs::cell_len)
            .max()
            .unwrap_or(0)
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

    fn visible_rows_for_viewport(&self, height: usize) -> usize {
        let header_rows = if self.show_header { 1 } else { 0 };
        height.saturating_sub(header_rows)
    }

    fn visible_rows(&self) -> usize {
        self.visible_rows_for_viewport(self.content_height as usize)
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
        let header_rows = if self.show_header { 1 } else { 0 };
        if y < header_rows {
            return None;
        }
        let data_y = y - header_rows;
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
            column_keys: Vec::new(),
            headers: Vec::new(),
            row_keys: Vec::new(),
            rows: Vec::new(),
            row_labels: Vec::new(),
            cell_justify: Vec::new(),
            column_widths: Vec::new(),
            selected: 0,
            offset: 0,
            cursor_column: 0,
            cursor_type: CursorType::Cell,
            fixed_rows: 0,
            fixed_columns: 0,
            horizontal_offset: 0,
            next_row_key: 0,
            next_column_key: 0,
            content_width: 0,
            content_height: 0,
            hover_coordinate: None,
            scrollbar_extracted: false,
            show_header: true,
            show_row_labels: true,
            zebra_stripes: false,
            seed: NodeSeed::default(),
        };
        out.recompute_column_widths();
        out
    }
}

impl ReactiveWidget for DataTable {
    fn reactive_dispatch(&mut self, changes: &[ReactiveChange], ctx: &mut ReactiveCtx) {
        for change in changes {
            match change.field_name {
                "cursor_type" => {
                    if let (Some(old), Some(new)) = (
                        change.old_value.downcast_ref::<CursorType>(),
                        change.new_value.downcast_ref::<CursorType>(),
                    ) {
                        self.watch_cursor_type(old, new, ctx);
                    }
                }
                "show_header" => {
                    if let (Some(old), Some(new)) = (
                        change.old_value.downcast_ref::<bool>(),
                        change.new_value.downcast_ref::<bool>(),
                    ) {
                        self.watch_show_header(old, new, ctx);
                    }
                }
                "zebra_stripes" => {
                    if let (Some(old), Some(new)) = (
                        change.old_value.downcast_ref::<bool>(),
                        change.new_value.downcast_ref::<bool>(),
                    ) {
                        self.watch_zebra_stripes(old, new, ctx);
                    }
                }
                _ => {}
            }
        }
    }
}

impl Widget for DataTable {
    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        if self.scrollbar_extracted {
            return Vec::new();
        }
        self.scrollbar_extracted = true;
        let mut hbar = ScrollBar::new(false, 1);
        hbar.seed.css_id = Some(DATA_TABLE_HSCROLLBAR_ID.to_string());
        vec![Box::new(hbar)]
    }

    fn focusable(&self) -> bool {
        true
    }

    fn mouse_interactive(&self) -> bool {
        true
    }

    fn on_node_state_changed(
        &mut self,
        _old: crate::widgets::NodeState,
        new: crate::widgets::NodeState,
    ) {
        if !new.hovered {
            self.hover_coordinate = None;
        }
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.content_width = width;
        self.content_height = height;
        let visible_rows = self.visible_rows_for_viewport(height as usize);
        self.ensure_visible(visible_rows);
        self.ensure_cursor_column_visible(width as usize);
    }

    fn on_mouse_move(&mut self, x: u16, y: u16) -> bool {
        let rendered_columns = self.rendered_column_indices();
        let col_idx = self.column_at_x_in_rendered_columns(x as usize, &rendered_columns);
        let visible_rows = self.visible_rows();
        let next = if self.show_header && y == 0 {
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

    fn action_namespace(&self) -> &str {
        "data-table"
    }

    fn bindings(&self) -> Vec<BindingDecl> {
        // Python's DataTable declares all of these with show=False — they must
        // not leak into the Footer (which should show only app-level bindings).
        vec![
            BindingDecl::new("up", "cursor_up", "Move cursor up").hidden(),
            BindingDecl::new("down", "cursor_down", "Move cursor down").hidden(),
            BindingDecl::new("left", "cursor_left", "Move cursor left").hidden(),
            BindingDecl::new("right", "cursor_right", "Move cursor right").hidden(),
            BindingDecl::new("pageup", "scroll_up", "Page up").hidden(),
            BindingDecl::new("pagedown", "scroll_down", "Page down").hidden(),
            BindingDecl::new("home", "scroll_home", "Move to start").hidden(),
            BindingDecl::new("end", "scroll_end", "Move to end").hidden(),
            BindingDecl::new("ctrl+home", "scroll_top", "Move to first row").hidden(),
            BindingDecl::new("ctrl+end", "scroll_bottom", "Move to last row").hidden(),
            BindingDecl::new("enter,space", "select_cursor", "Activate cell").hidden(),
        ]
    }

    fn execute_action(&mut self, action: &ParsedAction, ctx: &mut EventCtx) -> bool {
        let width = self.content_width as usize;
        let height = self.content_height as usize;
        let visible_rows = self.visible_rows_for_viewport(height);
        let mut selection_changed = false;
        let mut cursor_changed = false;

        let handled = match action.name.as_str() {
            "cursor_up" => {
                if matches!(self.cursor_type, CursorType::Cell | CursorType::Row)
                    && self.selected > 0
                {
                    self.selected -= 1;
                    selection_changed = true;
                }
                true
            }
            "cursor_down" => {
                if matches!(self.cursor_type, CursorType::Cell | CursorType::Row)
                    && self.selected + 1 < self.rows.len()
                {
                    self.selected += 1;
                    selection_changed = true;
                }
                true
            }
            "cursor_left" => {
                if matches!(self.cursor_type, CursorType::Cell | CursorType::Column)
                    && self.cursor_column > 0
                {
                    self.cursor_column -= 1;
                    cursor_changed = true;
                }
                true
            }
            "cursor_right" => {
                if matches!(self.cursor_type, CursorType::Cell | CursorType::Column)
                    && self.cursor_column + 1 < self.headers.len()
                {
                    self.cursor_column += 1;
                    cursor_changed = true;
                }
                true
            }
            "scroll_up" => {
                if matches!(self.cursor_type, CursorType::Cell | CursorType::Row)
                    && self.selected > 0
                {
                    let step = visible_rows.max(1).min(self.selected);
                    self.selected -= step;
                    selection_changed = true;
                }
                true
            }
            "scroll_down" => {
                if matches!(self.cursor_type, CursorType::Cell | CursorType::Row)
                    && self.selected + 1 < self.rows.len()
                {
                    let step = visible_rows
                        .max(1)
                        .min(self.rows.len().saturating_sub(1) - self.selected);
                    self.selected += step;
                    selection_changed = true;
                }
                true
            }
            "scroll_home" => {
                if matches!(self.cursor_type, CursorType::Cell | CursorType::Column)
                    && self.cursor_column != 0
                {
                    self.cursor_column = 0;
                    cursor_changed = true;
                } else if self.horizontal_offset != 0 {
                    self.horizontal_offset = 0;
                    ctx.request_repaint();
                }
                true
            }
            "scroll_end" => {
                if matches!(self.cursor_type, CursorType::Cell | CursorType::Column)
                    && !self.headers.is_empty()
                {
                    let col = self.headers.len() - 1;
                    if self.cursor_column != col {
                        self.cursor_column = col;
                        cursor_changed = true;
                    }
                } else {
                    let max_offset = self.scrollable_column_count().saturating_sub(1);
                    if self.horizontal_offset != max_offset {
                        self.horizontal_offset = max_offset;
                        ctx.request_repaint();
                    }
                }
                true
            }
            "scroll_top" => {
                if matches!(self.cursor_type, CursorType::Cell | CursorType::Row)
                    && self.selected != 0
                {
                    self.selected = 0;
                    selection_changed = true;
                }
                true
            }
            "scroll_bottom" => {
                if matches!(self.cursor_type, CursorType::Cell | CursorType::Row)
                    && !self.rows.is_empty()
                {
                    let row = self.rows.len() - 1;
                    if self.selected != row {
                        self.selected = row;
                        selection_changed = true;
                    }
                }
                true
            }
            "select_cursor" => {
                if !self.rows.is_empty() && !self.headers.is_empty() {
                    ctx.post_message(DataTableCellActivated {
                        row: self.selected,
                        column: self.cursor_column,
                    });
                }
                true
            }
            _ => false,
        };

        if selection_changed {
            self.ensure_visible(visible_rows);
        }
        if cursor_changed {
            self.ensure_cursor_column_visible(width);
        }
        if (selection_changed || cursor_changed)
            && !self.rows.is_empty()
            && !self.headers.is_empty()
        {
            ctx.post_message(DataTableCursorMoved {
                row: self.selected,
                column: self.cursor_column,
            });
        }
        if handled {
            ctx.set_handled();
        }
        handled
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        let visible_rows = self.visible_rows();
        let mut selection_changed = false;
        let mut cursor_changed = false;
        let mut header_clicked: Option<usize> = None;

        // Handle mouse events regardless of focus state.
        match event {
            Event::MouseDown(mouse) if mouse.target == self.node_id() => {
                let rendered_columns = self.rendered_column_indices();
                let clicked_col =
                    self.column_at_x_in_rendered_columns(mouse.x as usize, &rendered_columns);
                if matches!(self.cursor_type, CursorType::Cell | CursorType::Column) {
                    if self.cursor_column != clicked_col {
                        self.cursor_column = clicked_col;
                        cursor_changed = true;
                    }
                }

                let header_rows = if self.show_header { 1 } else { 0 };
                if mouse.y >= header_rows {
                    if let Some(clicked_row) = self.row_index_from_y(mouse.y as usize, visible_rows)
                    {
                        if self.selected != clicked_row {
                            self.selected = clicked_row;
                            selection_changed = true;
                        }
                    }
                } else if self.show_header {
                    header_clicked = Some(clicked_col);
                }
                if selection_changed {
                    self.ensure_visible(visible_rows);
                }
                if cursor_changed {
                    self.ensure_cursor_column_visible(self.content_width as usize);
                }
                if let Some(col) = header_clicked {
                    ctx.post_message(DataTableHeaderSelected { column: col });
                } else if selection_changed || cursor_changed {
                    ctx.post_message(DataTableCursorMoved {
                        row: self.selected,
                        column: self.cursor_column,
                    });
                }
                ctx.set_handled();
                return;
            }
            _ => {}
        }

        if !self.node_state().focused {
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
            Event::Action(Action::ScrollHome) => {
                if matches!(self.cursor_type, CursorType::Cell | CursorType::Column)
                    && self.cursor_column != 0
                {
                    self.cursor_column = 0;
                    cursor_changed = true;
                } else if self.horizontal_offset != 0 {
                    self.horizontal_offset = 0;
                    ctx.request_repaint();
                }
                handled = true;
            }
            Event::Action(Action::ScrollEnd) => {
                if matches!(self.cursor_type, CursorType::Cell | CursorType::Column)
                    && !self.headers.is_empty()
                {
                    let col = self.headers.len() - 1;
                    if self.cursor_column != col {
                        self.cursor_column = col;
                        cursor_changed = true;
                    }
                } else {
                    let max_offset = self.scrollable_column_count().saturating_sub(1);
                    if self.horizontal_offset != max_offset {
                        self.horizontal_offset = max_offset;
                        ctx.request_repaint();
                    }
                }
                handled = true;
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
                } else if self.scroll_horizontal_by_columns(-1) {
                    handled = true;
                    ctx.request_repaint();
                }
            }
            Event::Action(Action::ScrollRight) => {
                if matches!(self.cursor_type, CursorType::Cell | CursorType::Column) {
                    if self.cursor_column + 1 < self.headers.len() {
                        self.cursor_column += 1;
                        cursor_changed = true;
                    }
                    handled = true;
                } else if self.scroll_horizontal_by_columns(1) {
                    handled = true;
                    ctx.request_repaint();
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
                } else {
                    let step = self.page_horizontal_step(self.content_width as usize) as i32;
                    if self.scroll_horizontal_by_columns(-step) {
                        handled = true;
                        ctx.request_repaint();
                    }
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
                } else {
                    let step = self.page_horizontal_step(self.content_width as usize) as i32;
                    if self.scroll_horizontal_by_columns(step) {
                        handled = true;
                        ctx.request_repaint();
                    }
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
                    } else if self.horizontal_offset != 0 {
                        self.horizontal_offset = 0;
                        ctx.request_repaint();
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
                    } else {
                        let max_offset = self.scrollable_column_count().saturating_sub(1);
                        if self.horizontal_offset != max_offset {
                            self.horizontal_offset = max_offset;
                            ctx.request_repaint();
                        }
                    }
                    handled = true;
                }
                KeyCode::Enter | KeyCode::Char(' ') => {
                    if !self.rows.is_empty() && !self.headers.is_empty() {
                        ctx.post_message(DataTableCellActivated {
                            row: self.selected,
                            column: self.cursor_column,
                        });
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
        if cursor_changed {
            self.ensure_cursor_column_visible(self.content_width as usize);
        }
        if (selection_changed || cursor_changed)
            && !self.rows.is_empty()
            && !self.headers.is_empty()
        {
            ctx.post_message(DataTableCursorMoved {
                row: self.selected,
                column: self.cursor_column,
            });
        }
        if handled {
            ctx.set_handled();
        }
    }

    fn on_mouse_scroll(&mut self, delta_x: i32, _delta_y: i32, ctx: &mut EventCtx) {
        if delta_x == 0 {
            return;
        }
        if self.scroll_horizontal_by_columns(delta_x) {
            ctx.request_repaint();
            ctx.set_handled();
        }
    }

    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);
        let visible_rows = self.visible_rows_for_viewport(height);
        let offset = self.effective_offset(visible_rows);

        let column_widths = self.column_widths();
        let rendered_columns = self.rendered_column_indices();
        let label_col_width = self.label_col_width();
        let cursor_type = self.cursor_type;
        let show_cursor = self.node_state().focused && cursor_type != CursorType::None;

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

        // Zebra stripes: resolve alternate row background (even rows).
        let zebra_stripes = self.zebra_stripes;
        let zebra_style = if zebra_stripes {
            let zebra_bg = parse_color_like("$surface-darken-1")
                .map(|c| c.flatten_over(row_base))
                .unwrap_or(row_base);
            rich_rs::Style::new().with_bgcolor(zebra_bg.to_simple_opaque())
        } else {
            normal_style
        };

        let fixed_data_rows = self.fixed_data_rows();
        let fixed_visible = self.visible_fixed_rows(visible_rows);

        // Header line (headers use usize::MAX as their row sentinel).
        if self.show_header {
            emit_row_per_cell(
                &self.headers,
                column_widths,
                &rendered_columns,
                width,
                (label_col_width > 0).then_some(("", label_col_width, header_style)),
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
                // Headers are plain strings (left-justified), matching Python where
                // `add_columns` stores the label without a per-cell `justify`.
                |_col_idx| CellJustify::Left,
                header_style,
                &mut out,
            );
            out.push(Segment::line());
        }
        let mut rendered_rows = 0usize;

        let mut emit_data_row = |row_idx: usize, out: &mut Segments| {
            if rendered_rows >= visible_rows as usize {
                return;
            }
            let Some(row) = self.rows.get(row_idx) else {
                return;
            };
            let is_even_row = row_idx % 2 == 0;
            let row_base_style = if zebra_stripes && is_even_row {
                zebra_style
            } else {
                normal_style
            };
            let row_label = self
                .row_labels
                .get(row_idx)
                .and_then(|l| l.as_deref())
                .unwrap_or("");
            emit_row_per_cell(
                row,
                column_widths,
                &rendered_columns,
                width,
                (label_col_width > 0).then_some((row_label, label_col_width, row_base_style)),
                |col_idx| {
                    let target = (row_idx, col_idx);
                    let is_fixed_target = row_idx < fixed_data_rows || col_idx < self.fixed_columns;
                    let base = if is_fixed_target {
                        fixed_style
                    } else {
                        row_base_style
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
                |col_idx| self.cell_justify_at(row_idx, col_idx),
                row_base_style,
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
        let header_rows = if self.show_header { 1 } else { 0 };
        let intrinsic = header_rows + self.rows.len().max(1);
        Some(intrinsic)
    }

    fn content_width(&self) -> Option<usize> {
        // Python's `DataTable` inherits `width: 1fr` from `ScrollableContainer`
        // (its DEFAULT_CSS only overrides height), so a `DataTable` with an UNSET
        // width fills its container horizontally — the column data sits at the left
        // and the remaining width is surface fill, with the vertical scrollbar in
        // the right gutter. Reporting `None` here lets layout flex-fill the unset
        // width (matching that default) instead of shrinking the box to the
        // intrinsic content width, which would let the scrollbar gutter clip the
        // rightmost column. An EXPLICIT `width: auto` still shrinks to content via
        // `auto_content_width()`.
        None
    }

    fn auto_content_width(&self) -> Option<usize> {
        let columns = self.headers.len().max(1);
        let widths = self.column_widths();
        let cells_width = widths.iter().copied().sum::<usize>();
        let gaps_width = columns.saturating_sub(1).saturating_mul(2);
        // Non-data row-label column (width + its 2-cell gap), when shown.
        let label_width = self.label_col_width();
        let label_extra = if label_width > 0 { label_width + 2 } else { 0 };
        // Add CELL_PADDING for the leading 1-space left pad (matches Python DataTable.cell_padding=1).
        let content_width = cells_width
            .saturating_add(gaps_width)
            .saturating_add(label_extra)
            .saturating_add(CELL_PADDING)
            .max(1);
        let meta = crate::css::selector_meta_generic(self);
        let resolved = crate::css::resolve_style(self, &meta);
        let padding = resolved.effective_padding();
        let (_, _, border_left, border_right) =
            super::helpers::border_spacing_from_style(&resolved);
        let chrome_lr =
            usize::from(padding.left.saturating_add(padding.right)) + border_left + border_right;
        Some(content_width.saturating_add(chrome_lr).max(1))
    }

    fn set_inline_style(&mut self, style: crate::style::Style) {
        self.seed.styles.style = style;
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }

    fn on_message(&mut self, event: &MessageEvent, ctx: &mut EventCtx) {
        let Some(payload) = event.downcast_ref::<ScrollbarScrollTo>() else {
            return;
        };
        if payload.axis != ScrollbarAxis::Horizontal {
            return;
        }
        let width = self.content_width as usize;
        let Some(state) = self.horizontal_scrollbar_state(width) else {
            return;
        };
        let target_pixels = payload.offset.max(0.0).round() as usize;
        let clamped_pixels = target_pixels.min(state.max_pixel_offset);
        let next = self.horizontal_offset_from_pixels(clamped_pixels);
        if next != self.horizontal_offset {
            self.horizontal_offset = next;
            ctx.request_repaint();
        }
        ctx.set_handled();
    }

    fn scroll_offset(&self) -> (usize, usize) {
        let width = self.content_width as usize;
        if let Some(state) = self.horizontal_scrollbar_state(width) {
            (state.pixel_offset, 0)
        } else {
            (0, 0)
        }
    }

    fn scroll_offset_f32(&self) -> (f32, f32) {
        let (x, y) = self.scroll_offset();
        (x as f32, y as f32)
    }

    fn scroll_virtual_content_size(&self) -> Option<(usize, usize)> {
        let width = self.content_width as usize;
        // Use layout_height() for virtual height so that the scrollbar policy
        // does not see an inflated height from the seed on_layout call (which uses
        // the full viewport height) and incorrectly reserve a vertical scrollbar lane.
        // DataTable manages its own row offset internally; the virtual content height
        // equals the number of rendered rows (header + data), not the seed viewport.
        let virtual_h = self.layout_height().unwrap_or(1).max(1);
        if let Some(state) = self.horizontal_scrollbar_state(width) {
            Some((state.content_width.max(1), virtual_h))
        } else {
            let content_w = self.auto_content_width().unwrap_or(width.max(1)).max(1);
            Some((content_w, virtual_h))
        }
    }
}

impl Renderable for DataTable {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

/// Cell padding: 1 space on each side of each cell, matching Python DataTable.cell_padding = 1.
/// Visual layout: [1 space][cell][2 spaces][cell][2 spaces]...[cell][fill to total_width]
const CELL_PADDING: usize = 1;

/// Emit a row where each cell can have a different style determined by
/// `style_for_col` and a different justification determined by `justify_for_col`.
#[allow(clippy::too_many_arguments)]
fn emit_row_per_cell(
    values: &[String],
    column_widths: &[usize],
    rendered_columns: &[usize],
    total_width: usize,
    // Non-data row-label column rendered as a prefix: (text, width, style).
    // `width == 0` means no label column (the common, unlabelled case).
    label: Option<(&str, usize, rich_rs::Style)>,
    style_for_col: impl Fn(usize) -> rich_rs::Style,
    justify_for_col: impl Fn(usize) -> CellJustify,
    gap_style: rich_rs::Style,
    out: &mut Segments,
) {
    if rendered_columns.is_empty() {
        if total_width > 0 {
            out.push(Segment::styled(" ".repeat(total_width), gap_style));
        }
        return;
    }
    let mut used = 0usize;
    // Leading space (left pad of first cell).
    out.push(Segment::styled(" ".repeat(CELL_PADDING), gap_style));
    used += CELL_PADDING;
    // Row-label column: padded to its width, then a 2-cell gap to the data
    // cells (Python renders the label column as a non-data leading column).
    if let Some((label_text, label_width, label_style)) = label
        && label_width > 0
    {
        out.push(Segment::styled(
            rich_rs::set_cell_size(label_text, label_width),
            label_style,
        ));
        out.push(Segment::styled("  ", gap_style));
        used += label_width + 2;
    }
    for (i, col_idx) in rendered_columns.iter().copied().enumerate() {
        let col_w = column_widths.get(col_idx).copied().unwrap_or(0);
        if i > 0 {
            // Inter-cell gap = right pad of previous + left pad of next = 2 spaces.
            out.push(Segment::styled("  ", gap_style));
            used += 2;
        }
        let val = values.get(col_idx).map(String::as_str).unwrap_or("");
        let cell_text = justify_for_col(col_idx).render(val, col_w);
        out.push(Segment::styled(cell_text, style_for_col(col_idx)));
        used += col_w;
    }
    // Pad remainder to full width (absorbs trailing right-pad of last cell).
    if used < total_width {
        out.push(Segment::styled(" ".repeat(total_width - used), gap_style));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::MouseDownEvent;
    use crate::keys::KeyEventData;
    use crate::node_id::NodeId;
    use crate::reactive::ReactiveCtx;
    use crate::runtime::dispatch_ctx::set_dispatch_recipient;
    use crate::widgets::NodeState;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use slotmap::SlotMap;

    fn make_node_id() -> NodeId {
        let mut sm: SlotMap<NodeId, ()> = SlotMap::new();
        sm.insert(())
    }

    fn focused_state() -> NodeState {
        NodeState {
            focused: true,
            ..Default::default()
        }
    }

    fn hovered_state() -> NodeState {
        NodeState {
            hovered: true,
            ..Default::default()
        }
    }

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
        let id = NodeId::default();
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

        table.on_mouse_move(0, 0);
        assert_eq!(table.hover_coordinate, Some((usize::MAX, 0)));

        // With cell_padding=1, col0 occupies render x=1..4. x=5 maps to x_adj=4 which is
        // past col0's end (4), landing in col1.
        table.on_mouse_move(5, 0);
        assert_eq!(table.hover_coordinate, Some((usize::MAX, 1)));

        table.on_mouse_move(0, 1);
        assert_eq!(table.hover_coordinate, Some((0, 0)));

        table.on_mouse_move(5, 2);
        assert_eq!(table.hover_coordinate, Some((1, 1)));
    }

    #[test]
    fn clearing_hover_resets_hover_coordinate() {
        let mut table = DataTable::new(
            vec!["A".into()],
            vec![vec!["row0".into()], vec!["row1".into()]],
        );
        table.on_layout(20, 4);
        table.on_mouse_move(0, 1);
        assert_eq!(table.hover_coordinate, Some((0, 0)));

        table.on_node_state_changed(hovered_state(), NodeState::default());
        assert_eq!(table.hover_coordinate, None);
    }

    #[test]
    fn mouse_move_column_mapping_uses_cell_width_for_wide_graphemes() {
        let mut table = DataTable::new(
            vec!["👩‍🚀".into(), "B".into()],
            vec![vec!["x".into(), "y".into()]],
        );

        // First header uses two display cells; x=0..2 should still map to col 0.
        table.on_mouse_move(0, 0);
        assert_eq!(table.hover_coordinate, Some((usize::MAX, 0)));
        table.on_mouse_move(2, 0);
        assert_eq!(table.hover_coordinate, Some((usize::MAX, 0)));
        // With cell_padding=1, col0 occupies render x=1..3 (width=3, x_adj=0..2).
        // x=4 maps to x_adj=3 which is past col0's end, entering col1.
        table.on_mouse_move(4, 0);
        assert_eq!(table.hover_coordinate, Some((usize::MAX, 1)));
    }

    #[test]
    fn column_widths_use_combining_cluster_cell_width() {
        let table = DataTable::new(
            vec!["e\u{0301}e\u{0301}e\u{0301}e\u{0301}".into()],
            vec![vec!["x".into()]],
        );
        assert_eq!(table.column_widths()[0], 4);
    }

    #[test]
    fn mouse_move_column_mapping_uses_cell_width_for_wide_cjk_headers() {
        let mut table = DataTable::new(
            vec!["中中".into(), "B".into()],
            vec![vec!["x".into(), "y".into()]],
        );
        table.on_mouse_move(0, 0);
        assert_eq!(table.hover_coordinate, Some((usize::MAX, 0)));
        table.on_mouse_move(4, 0);
        assert_eq!(table.hover_coordinate, Some((usize::MAX, 0)));
        // With cell_padding=1, x=5 maps to x_adj=4 which is past col0's end (4), landing in col1.
        table.on_mouse_move(5, 0);
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
        let mut rctx = ReactiveCtx::new(NodeId::default());
        table.set_fixed_rows(1, &mut rctx);
        table.content_height = 3; // header + 2 visible rows
        table.set_selected(4, &mut rctx);

        // y=1 is fixed row 0, y=2 is the first scrolled row.
        assert_eq!(table.row_index_from_y(1, table.visible_rows()), Some(0));
        assert_eq!(table.row_index_from_y(2, table.visible_rows()), Some(4));
    }

    #[test]
    fn fixed_column_stays_visible_when_cursor_moves_to_far_columns() {
        let console = Console::new();
        let mut options = console.options().clone();
        options.size = (12, 3);
        options.max_width = 12;
        options.max_height = 3;

        let mut table = DataTable::new(
            vec!["C0".into(), "C1".into(), "C2".into(), "C3".into()],
            vec![vec!["r0".into(), "r1".into(), "r2".into(), "r3".into()]],
        );
        let mut rctx = ReactiveCtx::new(NodeId::default());
        table.set_fixed_columns(1, &mut rctx);
        table.on_layout(12, 3);
        table.set_cursor(0, 3, &mut rctx);

        let buf = crate::render::FrameBuffer::from_renderable(&console, &options, &table, None);
        let header = &buf.as_plain_lines()[0];
        assert!(header.contains("C0"));
        assert!(header.contains("C3"));
        assert!(!header.contains("C1"));
    }

    #[test]
    fn header_click_uses_shifted_horizontal_column_mapping() {
        let mut table = DataTable::new(
            vec!["C0".into(), "C1".into(), "C2".into(), "C3".into()],
            vec![vec!["r0".into(), "r1".into(), "r2".into(), "r3".into()]],
        );
        let mut rctx = ReactiveCtx::new(NodeId::default());
        table.set_fixed_columns(1, &mut rctx);
        table.on_layout(12, 3);
        table.set_cursor(0, 3, &mut rctx);
        let mut ctx = EventCtx::default();

        table.on_event(
            &Event::MouseDown(MouseDownEvent {
                target: NodeId::default(),
                screen_x: 4,
                screen_y: 0,
                x: 4,
                y: 0,
            }),
            &mut ctx,
        );

        let messages = ctx.take_messages();
        assert_eq!(messages.len(), 1);
        assert!(messages[0].is::<DataTableHeaderSelected>());
        // Columns are sized to their content (Python parity): "C0".."C3" headers
        // over "r0".."r3" cells give width 2 each (not the old artificial min of 3).
        // With width-2 columns the scroll offset that makes C3 (cursor target)
        // visible is the minimal one, so the rendered columns are [C0, C2, C3]
        // (only C1 is scrolled past). Layout: [pad][C0:2][gap][C2:2][gap][C3:2].
        // screen_x=4 (content x 3) falls within the C2 cell, so the header click
        // maps to column 2.
        assert_eq!(
            messages[0]
                .downcast_ref::<DataTableHeaderSelected>()
                .unwrap()
                .column,
            2
        );
    }

    #[test]
    fn clicking_fixed_header_reanchors_horizontal_offset() {
        let mut table = DataTable::new(
            vec!["C0".into(), "C1".into(), "C2".into(), "C3".into()],
            vec![vec!["r0".into(), "r1".into(), "r2".into(), "r3".into()]],
        );
        let mut rctx = ReactiveCtx::new(NodeId::default());
        table.set_fixed_columns(1, &mut rctx);
        table.on_layout(12, 3);
        table.set_cursor(0, 3, &mut rctx);
        assert!(table.horizontal_offset > 0);

        let mut ctx = EventCtx::default();
        table.on_event(
            &Event::MouseDown(MouseDownEvent {
                target: NodeId::default(),
                screen_x: 0,
                screen_y: 0,
                x: 0,
                y: 0,
            }),
            &mut ctx,
        );

        assert_eq!(table.cursor_column, 0);
        assert_eq!(table.horizontal_offset, 0);
    }

    #[test]
    fn horizontal_offset_stays_stable_when_fixed_columns_fill_viewport() {
        let mut table = DataTable::new(
            vec!["WIDE_FIXED".into(), "C1".into(), "C2".into(), "C3".into()],
            vec![vec!["row".into(), "1".into(), "2".into(), "3".into()]],
        );
        let mut rctx = ReactiveCtx::new(NodeId::default());
        table.set_fixed_columns(1, &mut rctx);
        table.on_layout(4, 3);
        table.set_cursor(0, 3, &mut rctx);

        assert_eq!(table.horizontal_offset, 0);
    }

    #[test]
    fn home_end_navigation_matches_cursor_and_control_semantics() {
        let mut table = DataTable::new(
            vec!["A".into(), "B".into(), "C".into()],
            (0..5)
                .map(|n| vec![format!("row{n}"), "x".into(), "y".into()])
                .collect(),
        );
        let _guard = set_dispatch_recipient(make_node_id(), focused_state());
        table.content_height = 4;
        let mut rctx = ReactiveCtx::new(NodeId::default());
        table.set_cursor(3, 2, &mut rctx);
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
    fn scroll_home_end_actions_follow_column_cursor_navigation() {
        let mut table = DataTable::new(
            vec!["C0".into(), "C1".into(), "C2".into(), "C3".into()],
            vec![vec!["r0".into(), "r1".into(), "r2".into(), "r3".into()]],
        );
        let mut rctx = ReactiveCtx::new(NodeId::default());
        table.set_fixed_columns(1, &mut rctx);
        let _guard = set_dispatch_recipient(make_node_id(), focused_state());
        table.on_layout(12, 3);
        table.set_cursor(0, 3, &mut rctx);
        let offset_at_end = table.horizontal_offset;
        assert!(offset_at_end > 0);

        let mut ctx = EventCtx::default();
        table.on_event(&Event::Action(Action::ScrollHome), &mut ctx);
        assert!(ctx.handled());
        assert_eq!(table.cursor_column, 0);
        assert_eq!(table.horizontal_offset, 0);

        let mut ctx = EventCtx::default();
        table.on_event(&Event::Action(Action::ScrollEnd), &mut ctx);
        assert!(ctx.handled());
        assert_eq!(table.cursor_column, 3);
        assert_eq!(table.horizontal_offset, offset_at_end);
    }

    #[test]
    fn header_click_posts_header_selected_message() {
        let mut table = DataTable::new(
            vec!["A".into(), "B".into()],
            vec![vec!["r0".into(), "c0".into()]],
        );
        let id = NodeId::default();
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
        assert!(messages[0].is::<DataTableHeaderSelected>());
        assert_eq!(
            messages[0]
                .downcast_ref::<DataTableHeaderSelected>()
                .unwrap()
                .column,
            1
        );
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
        let _guard = set_dispatch_recipient(make_node_id(), focused_state());
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
        assert!(messages[0].is::<DataTableCursorMoved>());
        let m = messages[0].downcast_ref::<DataTableCursorMoved>().unwrap();
        assert_eq!((m.row, m.column), (1, 0));
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
        let _guard = set_dispatch_recipient(make_node_id(), focused_state());
        let mut rctx = ReactiveCtx::new(NodeId::default());
        table.set_cursor(1, 1, &mut rctx);
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
        assert!(messages[0].is::<DataTableCellActivated>());
        let m = messages[0]
            .downcast_ref::<DataTableCellActivated>()
            .unwrap();
        assert_eq!((m.row, m.column), (1, 1));
    }

    #[test]
    fn renders_horizontal_scrollbar_when_columns_overflow() {
        let console = Console::new();
        let mut options = console.options().clone();
        options.size = (12, 4);
        options.max_width = 12;
        options.max_height = 4;

        let table = DataTable::new(
            vec![
                "First".into(),
                "Second".into(),
                "Third".into(),
                "Fourth".into(),
            ],
            vec![vec!["a".into(), "b".into(), "c".into(), "d".into()]],
        );

        let buf = crate::render::FrameBuffer::from_renderable(&console, &options, &table, None);
        assert!(table.horizontal_scrollbar_state(12).is_some());
        assert_eq!(buf.as_plain_lines().len(), 4);
    }

    #[test]
    fn horizontal_scrollbar_track_click_pages_viewport() {
        let mut table = DataTable::new(
            vec![
                "First".into(),
                "Second".into(),
                "Third".into(),
                "Fourth".into(),
            ],
            vec![vec!["a".into(), "b".into(), "c".into(), "d".into()]],
        );
        table.on_layout(12, 4);
        let _ = table.take_composed_children();
        assert_eq!(table.horizontal_offset, 0);

        let mut ctx = EventCtx::default();
        table.on_message(
            &MessageEvent::new(
                NodeId::default(),
                ScrollbarScrollTo {
                    axis: ScrollbarAxis::Horizontal,
                    offset: 999.0,
                    animate: false,
                    scroll_duration: None,
                },
            ),
            &mut ctx,
        );

        assert!(ctx.handled());
        assert!(table.horizontal_offset > 0);
    }

    #[test]
    fn row_cursor_scroll_right_action_moves_horizontal_viewport() {
        let mut table = DataTable::new(
            vec![
                "First".into(),
                "Second".into(),
                "Third".into(),
                "Fourth".into(),
            ],
            vec![vec!["a".into(), "b".into(), "c".into(), "d".into()]],
        );
        let _guard = set_dispatch_recipient(make_node_id(), focused_state());
        let mut rctx = ReactiveCtx::new(NodeId::default());
        table.set_cursor_type(CursorType::Row, &mut rctx);
        table.on_layout(12, 4);
        assert_eq!(table.horizontal_offset, 0);

        let mut ctx = EventCtx::default();
        table.on_event(&Event::Action(Action::ScrollRight), &mut ctx);
        assert!(ctx.handled());
        assert!(table.horizontal_offset > 0);
    }

    #[test]
    fn dragging_horizontal_scrollbar_thumb_updates_offset() {
        let mut table = DataTable::new(
            vec![
                "First".into(),
                "Second".into(),
                "Third".into(),
                "Fourth".into(),
            ],
            vec![vec!["a".into(), "b".into(), "c".into(), "d".into()]],
        );
        table.on_layout(12, 4);
        let _ = table.take_composed_children();

        let mut ctx = EventCtx::default();
        table.on_message(
            &MessageEvent::new(
                NodeId::default(),
                ScrollbarScrollTo {
                    axis: ScrollbarAxis::Horizontal,
                    offset: 3.0,
                    animate: false,
                    scroll_duration: None,
                },
            ),
            &mut ctx,
        );
        assert!(ctx.handled());
        let after_first = table.horizontal_offset;

        let mut ctx2 = EventCtx::default();
        table.on_message(
            &MessageEvent::new(
                NodeId::default(),
                ScrollbarScrollTo {
                    axis: ScrollbarAxis::Horizontal,
                    offset: 10.0,
                    animate: false,
                    scroll_duration: None,
                },
            ),
            &mut ctx2,
        );
        assert!(ctx2.handled());
        assert!(ctx2.repaint_requested());
        assert!(table.horizontal_offset >= after_first);
        assert!(table.horizontal_offset > 0);
    }

    #[test]
    fn bindings_are_declared() {
        let table = DataTable::new(
            vec!["Name".into(), "Value".into()],
            vec![vec!["Alice".into(), "100".into()]],
        );
        let bindings = table.bindings();
        assert!(!bindings.is_empty());
        assert!(bindings.iter().any(|b| b.action == "cursor_up"));
        assert!(bindings.iter().any(|b| b.action == "cursor_down"));
        assert!(bindings.iter().any(|b| b.action == "select_cursor"));
    }

    #[test]
    fn execute_action_handles_cursor_down() {
        use crate::action::ParsedAction;
        let mut table = DataTable::new(
            vec!["Name".into(), "Value".into()],
            vec![
                vec!["Alice".into(), "100".into()],
                vec!["Bob".into(), "200".into()],
            ],
        );
        table.on_layout(40, 10);
        let mut ctx = EventCtx::default();
        let action = ParsedAction {
            namespace: None,
            name: "cursor_down".to_string(),
            arguments: vec![],
        };
        assert!(table.execute_action(&action, &mut ctx));
    }

    #[test]
    fn tree_mode_extracts_dedicated_scrollbar_child() {
        let mut table = DataTable::new(
            vec![
                "First".into(),
                "Second".into(),
                "Third".into(),
                "Fourth".into(),
            ],
            vec![vec!["a".into(), "b".into(), "c".into(), "d".into()]],
        );
        let mut children = table.take_composed_children();
        assert_eq!(children.len(), 1);
        assert_eq!(
            children[0].take_node_seed().css_id.as_deref(),
            Some(DATA_TABLE_HSCROLLBAR_ID)
        );
    }

    #[test]
    fn scrollbar_message_updates_horizontal_offset_in_tree_mode() {
        let mut table = DataTable::new(
            vec![
                "First".into(),
                "Second".into(),
                "Third".into(),
                "Fourth".into(),
            ],
            vec![vec!["a".into(), "b".into(), "c".into(), "d".into()]],
        );
        table.on_layout(12, 4);
        let _ = table.take_composed_children();

        let mut ctx = EventCtx::default();
        table.on_message(
            &MessageEvent::new(
                NodeId::default(),
                ScrollbarScrollTo {
                    axis: ScrollbarAxis::Horizontal,
                    offset: 999.0,
                    animate: false,
                    scroll_duration: None,
                },
            ),
            &mut ctx,
        );

        assert!(ctx.handled());
        assert!(table.horizontal_offset > 0);
    }
}
