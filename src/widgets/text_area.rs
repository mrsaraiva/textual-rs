use std::collections::HashMap;
use std::time::{Duration, Instant};

use crossterm::event::KeyCode;
use unicode_segmentation::UnicodeSegmentation;

use rich_rs::{Console, ConsoleOptions, Segment, Segments};
use textual_macros::widget;
use tree_sitter::{Parser, Query, QueryCursor};

use crate::event::Event;
use crate::message::*;
use crate::style::{Color, Style, parse_color_like};
use crate::{Error, Result};

use crate::action::ParsedAction;
use crate::reactive::{ReactiveChange, ReactiveCtx, ReactiveFlags, ReactiveWidget};

use super::{
    BindingDecl, NodeSeed, NodeState, Widget,
    text_edit::{
        EditCommand, MoveUnit, byte_index_from_cell_x as grapheme_byte_index_from_cell_x,
        cell_len_prefix as grapheme_cell_len_prefix, clamp_grapheme_boundary,
        edit_command_from_key, grapheme_cell_width as grapheme_width, next_grapheme_boundary,
        next_word_boundary, prev_grapheme_boundary, prev_word_boundary,
    },
};

use crate::document::{Document, DocumentNavigator, EditHistory, WrappedDocument};
// The document model types are defined in `crate::document` (a framework
// primitive); re-exported here for the existing `TextArea` API surface.
pub use crate::document::{Cursor, Edit, EditResult, Location, Selection};

#[derive(Debug, Clone)]
pub struct TextAreaTheme {
    pub name: String,
    pub cursor_style: Style,
    pub cursor_line_style: Style,
    pub selection_style: Style,
    pub gutter_style: Style,
    pub gutter_active_style: Style,
    pub syntax_styles: HashMap<String, Style>,
}

impl TextAreaTheme {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            cursor_style: Style::default(),
            cursor_line_style: Style::default(),
            selection_style: Style::default(),
            gutter_style: Style::default(),
            gutter_active_style: Style::default(),
            syntax_styles: HashMap::new(),
        }
    }
}

#[derive(Debug)]
struct LanguageDef {
    language: tree_sitter::Language,
    highlight_query: Query,
}

#[derive(Debug, Clone)]
struct SyntaxSpan {
    start: usize,
    end: usize,
    style: Style,
}

#[derive(Debug, Clone)]
struct SyntaxCache {
    revision: u64,
    line_offsets: Vec<usize>,
    spans: Vec<SyntaxSpan>,
}

impl Default for SyntaxCache {
    fn default() -> Self {
        // Use a sentinel revision so the first render always computes the cache.
        Self {
            revision: u64::MAX,
            line_offsets: Vec::new(),
            spans: Vec::new(),
        }
    }
}

#[widget(Focus, Interactive, Selectable)]
pub struct TextArea {
    /// The document model (lines + newline style + `replace_range`).
    document: Document,
    cursor: Cursor,
    selection: Selection,
    language: Option<String>,
    code_editor: bool,
    read_only: bool,
    show_line_numbers: bool,
    indent_width: usize,
    soft_wrap: bool,
    placeholder: String,
    /// Vertical scroll position in VISUAL rows (wrapped sections).
    scroll_row: usize,
    /// Horizontal scroll position in cells; only used when the wrap width
    /// is 0 (wrapped documents never scroll horizontally, matching Python).
    scroll_col: usize,
    layout_w: u16,
    layout_h: u16,
    layout_initialized: bool,
    /// The wrapped view over `document`; kept in sync by the edit funnel
    /// (`wrap_range`) and full re-wraps on layout/gutter changes.
    wrapped: WrappedDocument,
    /// Wrap-aware navigation state (`last_x_offset` is the remembered
    /// visual x for vertical movement, replacing `preferred_col_cells`).
    navigator: DocumentNavigator,
    mouse_down: bool,
    app_active: bool,
    cursor_visible: bool,
    cursor_blink_next_at: Option<Instant>,
    cursor_blink_enabled: bool,
    doc_revision: u64,
    history: EditHistory,
    syntax_cache: std::sync::Mutex<SyntaxCache>,
    languages: HashMap<String, LanguageDef>,
    themes: HashMap<String, TextAreaTheme>,
    theme: Option<String>,
    seed: NodeSeed,
}

impl TextArea {
    crate::seed_ident_methods!();

    const CURSOR_BLINK_PERIOD: Duration = Duration::from_millis(500);
    const PYTHON_HIGHLIGHTS: &str = r#"
(comment) @comment
(string) @string
((identifier) @constant (#match? @constant "^_*[A-Z][A-Z\\d_]+$"))
[
  "def"
  "class"
  "return"
  "if"
  "else"
  "elif"
  "for"
  "while"
  "try"
  "except"
  "finally"
  "import"
  "from"
  "as"
  "pass"
  "break"
  "continue"
  "with"
  "yield"
  "lambda"
] @keyword
"#;

    fn next_blink_deadline() -> Instant {
        let now = Instant::now();
        now.checked_add(Self::CURSOR_BLINK_PERIOD).unwrap_or(now)
    }

    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let document = Document::new(&text);
        // Wrap width starts at 0 (no wrapping); the first `on_layout`
        // computes the real wrap width.
        let wrapped = WrappedDocument::new(&document, 0, 4);
        let mut out = Self {
            document,
            cursor: Cursor::default(),
            selection: Selection::cursor(Cursor::default()),
            language: None,
            code_editor: false,
            read_only: false,
            show_line_numbers: false,
            indent_width: 4,
            soft_wrap: true,
            placeholder: String::new(),
            scroll_row: 0,
            scroll_col: 0,
            layout_w: 1,
            layout_h: 1,
            layout_initialized: false,
            wrapped,
            navigator: DocumentNavigator::new(),
            mouse_down: false,
            app_active: true,
            cursor_visible: false,
            cursor_blink_next_at: None,
            cursor_blink_enabled: true,
            doc_revision: 0,
            history: EditHistory::new(50, Duration::from_secs_f64(2.0), 100),
            syntax_cache: std::sync::Mutex::new(SyntaxCache::default()),
            languages: HashMap::new(),
            themes: HashMap::new(),
            theme: None,
            seed: NodeSeed::default(),
        };
        out.register_builtin_languages();
        out.rebuild_classes();
        out.clamp_cursor();
        out
    }

    pub fn code_editor(text: impl Into<String>) -> Self {
        let mut out = Self::new(text);
        out.code_editor = true;
        out.show_line_numbers = true;
        out.soft_wrap = false;
        out.rebuild_classes();
        out
    }

    pub fn with_read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self.rebuild_classes();
        self
    }

    pub fn read_only(&self) -> bool {
        self.read_only
    }

    /// Reactive setter for `read_only`. Records the change in the provided
    /// [`ReactiveCtx`] if the value actually changed.
    pub fn set_read_only(&mut self, value: bool, ctx: &mut ReactiveCtx) {
        if self.read_only != value {
            let old = self.read_only;
            self.read_only = value;
            ctx.record_change(
                "read_only",
                ReactiveFlags::reactive(),
                Box::new(old),
                Box::new(value),
            );
        }
    }

    pub fn with_show_line_numbers(mut self, show: bool) -> Self {
        self.show_line_numbers = show;
        self
    }

    pub fn show_line_numbers(&self) -> bool {
        self.show_line_numbers
    }

    /// Reactive setter for `show_line_numbers`.
    pub fn set_show_line_numbers(&mut self, value: bool, ctx: &mut ReactiveCtx) {
        if self.show_line_numbers != value {
            let old = self.show_line_numbers;
            self.show_line_numbers = value;
            ctx.record_change(
                "show_line_numbers",
                ReactiveFlags::reactive_layout(),
                Box::new(old),
                Box::new(value),
            );
        }
    }

    pub fn with_indent_width(mut self, width: usize) -> Self {
        self.indent_width = width;
        self
    }

    pub fn indent_width(&self) -> usize {
        self.indent_width
    }

    /// Reactive setter for `indent_width`.
    pub fn set_indent_width(&mut self, value: usize, ctx: &mut ReactiveCtx) {
        if self.indent_width != value {
            let old = self.indent_width;
            self.indent_width = value;
            ctx.record_change(
                "indent_width",
                ReactiveFlags::reactive(),
                Box::new(old),
                Box::new(value),
            );
        }
    }

    pub fn with_soft_wrap(mut self, wrap: bool) -> Self {
        self.soft_wrap = wrap;
        self
    }

    pub fn soft_wrap(&self) -> bool {
        self.soft_wrap
    }

    /// Reactive setter for `soft_wrap`. Triggers layout invalidation.
    pub fn set_soft_wrap(&mut self, value: bool, ctx: &mut ReactiveCtx) {
        if self.soft_wrap != value {
            let old = self.soft_wrap;
            self.soft_wrap = value;
            ctx.record_change(
                "soft_wrap",
                ReactiveFlags::reactive_layout(),
                Box::new(old),
                Box::new(value),
            );
        }
    }

    pub fn with_placeholder(mut self, text: impl Into<String>) -> Self {
        self.placeholder = text.into();
        self
    }

    pub fn placeholder(&self) -> &str {
        &self.placeholder
    }

    /// Reactive setter for `placeholder`.
    pub fn set_placeholder(&mut self, value: impl Into<String>, ctx: &mut ReactiveCtx) {
        let value = value.into();
        if self.placeholder != value {
            let old = self.placeholder.clone();
            self.placeholder = value;
            let new = self.placeholder.clone();
            ctx.record_change(
                "placeholder",
                ReactiveFlags::reactive(),
                Box::new(old),
                Box::new(new),
            );
        }
    }

    pub fn with_language(mut self, language: impl Into<String>) -> Self {
        self.language = Some(language.into());
        if let Ok(mut cache) = self.syntax_cache.lock() {
            cache.revision = u64::MAX;
        }
        self
    }

    pub fn language(&self) -> Option<&str> {
        self.language.as_deref()
    }

    /// Reactive setter for `language`. Triggers re-highlighting via watcher.
    pub fn set_language(&mut self, value: impl Into<String>, ctx: &mut ReactiveCtx) {
        let value = Some(value.into());
        if self.language != value {
            let old = self.language.clone();
            self.language = value;
            let new = self.language.clone();
            ctx.record_change(
                "language",
                ReactiveFlags::reactive(),
                Box::new(old),
                Box::new(new),
            );
        }
    }

    pub fn cursor_blink(&self) -> bool {
        self.cursor_blink_enabled
    }

    /// Reactive setter for `cursor_blink_enabled`.
    pub fn set_cursor_blink(&mut self, value: bool, ctx: &mut ReactiveCtx) {
        if self.cursor_blink_enabled != value {
            let old = self.cursor_blink_enabled;
            self.cursor_blink_enabled = value;
            ctx.record_change(
                "cursor_blink_enabled",
                ReactiveFlags::reactive(),
                Box::new(old),
                Box::new(value),
            );
        }
    }

    pub fn with_cursor_blink(mut self, enabled: bool) -> Self {
        self.cursor_blink_enabled = enabled;
        self
    }

    pub fn register_language(
        &mut self,
        name: impl Into<String>,
        language: tree_sitter::Language,
        highlight_query: &str,
    ) -> Result<()> {
        let name = name.into();
        let query = Query::new(&language, highlight_query)
            .map_err(|e| Error::TextAreaLanguage(e.to_string()))?;
        self.languages.insert(
            name,
            LanguageDef {
                language,
                highlight_query: query,
            },
        );
        if let Ok(mut cache) = self.syntax_cache.lock() {
            cache.revision = u64::MAX;
        }
        Ok(())
    }

    pub fn register_theme(&mut self, theme: TextAreaTheme) {
        self.themes.insert(theme.name.clone(), theme);
    }

    pub fn theme(&self) -> Option<&str> {
        self.theme.as_deref()
    }

    /// Reactive setter for `theme`. Triggers theme reload via watcher.
    pub fn set_theme(&mut self, value: impl Into<String>, ctx: &mut ReactiveCtx) {
        let value = Some(value.into());
        if self.theme != value {
            let old = self.theme.clone();
            self.theme = value;
            let new = self.theme.clone();
            ctx.record_change(
                "theme",
                ReactiveFlags::reactive(),
                Box::new(old),
                Box::new(new),
            );
        }
    }

    pub fn with_theme(mut self, name: impl Into<String>) -> Self {
        self.theme = Some(name.into());
        if let Ok(mut cache) = self.syntax_cache.lock() {
            cache.revision = u64::MAX;
        }
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
        self.record_cursor_width();
        self.adjust_scroll_to_cursor();
        self.reset_blink();
    }

    pub fn with_selection(mut self, selection: Selection) -> Self {
        self.set_selection(selection);
        self
    }

    /// Insert text at the cursor location (Python `TextArea.insert` with
    /// `location=None`). The edit is recorded in the undo history.
    pub fn insert(&mut self, text: &str) -> EditResult {
        let at = self.cursor.location();
        self.edit(Edit::new(text, at, at, true))
    }

    /// Insert text at `location` (Python `TextArea.insert`).
    pub fn insert_at(&mut self, text: &str, location: Location) -> EditResult {
        self.edit(Edit::new(text, location, location, true))
    }

    /// Replace the text between `start` and `end` with `insert` (Python
    /// `TextArea.replace`).
    pub fn replace(&mut self, insert: &str, start: Location, end: Location) -> EditResult {
        self.edit(Edit::new(insert, start, end, true))
    }

    /// Delete the text between `start` and `end` (Python `TextArea.delete`).
    pub fn delete(&mut self, start: Location, end: Location) -> EditResult {
        self.edit(Edit::new("", start, end, true))
    }

    /// Delete all text from the document (Python `TextArea.clear`).
    pub fn clear(&mut self) -> EditResult {
        self.edit(Edit::new("", (0, 0), self.document.end(), false))
    }

    pub fn move_cursor_relative(&mut self, columns: isize, rows: isize) {
        let row = (self.cursor.row as isize + rows)
            .clamp(0, self.document.line_count().saturating_sub(1) as isize)
            as usize;
        let cur_cells = self.cursor_cell_x() as isize;
        let target_cells = (cur_cells + columns).max(0) as usize;
        let col = self.cursor_from_cell_x(row, target_cells);
        self.cursor = Cursor { row, col };
        self.selection = Selection::cursor(self.cursor);
        self.record_cursor_width();
        self.adjust_scroll_to_cursor();
        self.reset_blink();
        // Cursor movement creates an undo checkpoint (Python `move_cursor`).
        self.history.checkpoint();
    }

    pub fn text(&self) -> String {
        self.document.text()
    }

    /// The newline style used by this document (Python `Document.newline`):
    /// `"\n"` by default, `"\r\n"` or `"\r"` if the initial text used it.
    pub fn newline(&self) -> &'static str {
        self.document.newline()
    }

    /// Read access to the document model.
    pub fn document(&self) -> &Document {
        &self.document
    }

    /// The edit history (undo/redo batches).
    pub fn history(&self) -> &EditHistory {
        &self.history
    }

    /// Mutable access to the edit history.
    ///
    /// Whole-history replacement is legal and is the test seam for
    /// injecting a mock clock:
    /// `*ta.history_mut() = EditHistory::with_clock(..., MockClock::new())`.
    pub fn history_mut(&mut self) -> &mut EditHistory {
        &mut self.history
    }

    /// Load new text into the text area, clearing the edit history
    /// (Python `TextArea.load_text`).
    pub fn load_text(&mut self, text: &str) {
        self.history.clear();
        self.document = Document::new(text);
        self.doc_revision = self.doc_revision.wrapping_add(1);
        self.cursor = Cursor::default();
        self.selection = Selection::cursor(self.cursor);
        self.navigator.last_x_offset = 0;
        self.scroll_row = 0;
        self.scroll_col = 0;
        self.rewrap_full();
    }

    /// Alias of [`TextArea::load_text`] (Python `TextArea.text` setter).
    pub fn set_text(&mut self, text: &str) {
        self.load_text(text);
    }

    /// Perform an [`Edit`]: the single mutation funnel (Python
    /// `TextArea.edit`). Applies the edit to the document, records it in
    /// the history, and applies the resulting selection.
    pub fn edit(&mut self, mut edit: Edit) -> EditResult {
        let old_gutter_width = self.line_number_gutter_width();
        let edit_top = edit.top();
        let edit_bottom = edit.bottom();
        let result = edit.apply(&mut self.document, self.selection, true);
        self.doc_revision = self.doc_revision.wrapping_add(1);
        let updated_selection = edit.updated_selection();
        self.history.record(edit);
        // Re-wrap BETWEEN the edit and the selection restore (Python
        // ordering: selection assignment scrolls using wrapped geometry).
        if old_gutter_width != self.line_number_gutter_width() {
            // The gutter width changed (line count digit transition), so
            // the wrap width changed: full re-wrap.
            self.rewrap_full();
        } else {
            self.wrapped
                .wrap_range(&self.document, edit_top, edit_bottom, result.end_location);
            self.clamp_scroll_to_wrapped_height();
        }
        if let Some(selection) = updated_selection {
            self.set_selection(selection);
        }
        result
    }

    /// Undo the edits since the last checkpoint (the most recent batch).
    /// Returns true when a batch was replayed.
    pub fn undo(&mut self) -> bool {
        if let Some(mut edits) = self.history.pop_undo() {
            self.undo_batch(&mut edits);
            // Unconditional: Python moves the batch across stacks before
            // replaying (`test_redo_stack` pins the resulting lengths).
            self.history.push_redone(edits);
            true
        } else {
            false
        }
    }

    /// Redo the most recently undone batch of edits.
    pub fn redo(&mut self) -> bool {
        if let Some(mut edits) = self.history.pop_redo() {
            self.redo_batch(&mut edits);
            self.history.push_undone(edits);
            true
        } else {
            false
        }
    }

    /// Undo a batch of edits in reverse chronological order (Python
    /// `_undo_batch`), accumulating the dirty region for one `wrap_range`.
    fn undo_batch(&mut self, edits: &mut [Edit]) {
        if edits.is_empty() {
            return;
        }
        let old_gutter_width = self.line_number_gutter_width();
        let mut minimum_top = edits.last().expect("non-empty").top();
        let mut maximum_old_bottom: Location = (0, 0);
        let mut maximum_new_bottom: Location = (0, 0);
        for edit in edits.iter_mut().rev() {
            edit.undo(&mut self.document);
            let end_location = edit
                .edit_result()
                .map(|result| result.end_location)
                .unwrap_or((0, 0));
            if edit.top() < minimum_top {
                minimum_top = edit.top();
            }
            if end_location > maximum_old_bottom {
                maximum_old_bottom = end_location;
            }
            if edit.bottom() > maximum_new_bottom {
                maximum_new_bottom = edit.bottom();
            }
        }
        self.doc_revision = self.doc_revision.wrapping_add(1);
        if old_gutter_width != self.line_number_gutter_width() {
            self.rewrap_full();
        } else {
            self.wrapped.wrap_range(
                &self.document,
                minimum_top,
                maximum_old_bottom,
                maximum_new_bottom,
            );
            self.clamp_scroll_to_wrapped_height();
        }
        for edit in edits.iter_mut().rev() {
            if let Some(selection) = edit.updated_selection() {
                self.set_selection(selection);
            }
        }
    }

    /// Redo a batch of edits in chronological order (Python `_redo_batch`).
    fn redo_batch(&mut self, edits: &mut [Edit]) {
        if edits.is_empty() {
            return;
        }
        let old_gutter_width = self.line_number_gutter_width();
        let mut minimum_top = edits.first().expect("non-empty").top();
        let mut maximum_old_bottom: Location = (0, 0);
        let mut maximum_new_bottom: Location = (0, 0);
        for edit in edits.iter_mut() {
            edit.apply(&mut self.document, self.selection, false);
            let end_location = edit
                .edit_result()
                .map(|result| result.end_location)
                .unwrap_or((0, 0));
            if edit.top() < minimum_top {
                minimum_top = edit.top();
            }
            if end_location > maximum_new_bottom {
                maximum_new_bottom = end_location;
            }
            if edit.bottom() > maximum_old_bottom {
                maximum_old_bottom = edit.bottom();
            }
        }
        self.doc_revision = self.doc_revision.wrapping_add(1);
        if old_gutter_width != self.line_number_gutter_width() {
            self.rewrap_full();
        } else {
            self.wrapped.wrap_range(
                &self.document,
                minimum_top,
                maximum_old_bottom,
                maximum_new_bottom,
            );
            self.clamp_scroll_to_wrapped_height();
        }
        for edit in edits.iter_mut() {
            if let Some(selection) = edit.updated_selection() {
                self.set_selection(selection);
            }
        }
    }

    /// The width the document wraps at: content width minus gutter and one
    /// cursor cell (Python `wrap_width`), or 0 when soft wrap is off.
    fn wrap_width(&self) -> usize {
        if !self.soft_wrap {
            return 0;
        }
        let cursor_width = 1;
        (self.layout_w.max(1) as usize)
            .saturating_sub(self.line_number_gutter_width() + cursor_width)
    }

    /// Fully re-wrap the document at the current wrap width and clamp the
    /// vertical scroll to the new wrapped height.
    fn rewrap_full(&mut self) {
        let wrap_width = self.wrap_width();
        self.wrapped
            .wrap(&self.document, wrap_width, Some(self.indent_width));
        self.clamp_scroll_to_wrapped_height();
    }

    /// Clamp `scroll_row` (a visual offset) so that deleting wrapped lines
    /// near the bottom cannot leave the viewport past the end (the maximum
    /// scroll is `wrapped height - viewport height`, ScrollView semantics).
    fn clamp_scroll_to_wrapped_height(&mut self) {
        let view_height = if self.layout_initialized {
            self.layout_h.max(1) as usize
        } else {
            1
        };
        self.scroll_row = self
            .scroll_row
            .min(self.wrapped.height().saturating_sub(view_height));
    }

    /// Record the current visual x of the cursor as the remembered offset
    /// for vertical movement (Python `record_cursor_width`).
    fn record_cursor_width(&mut self) {
        let (x_offset, _) = self
            .wrapped
            .location_to_offset(&self.document, self.cursor.location());
        self.navigator.last_x_offset = x_offset;
    }

    /// Map widget-local mouse coordinates to a document location through
    /// the wrapped view (clamps click-past-end automatically).
    fn hit_test_location(&self, x: u16, y: u16) -> Cursor {
        let gutter = self.line_number_gutter_width() as u16;
        let local_x = x.saturating_sub(gutter) as usize;
        let cell_x = if self.wrapped.width() == 0 {
            self.scroll_col.saturating_add(local_x)
        } else {
            local_x
        };
        let visual_y = self.scroll_row.saturating_add(y as usize);
        let (row, col) =
            self.wrapped
                .offset_to_location(&self.document, cell_x as isize, visual_y as isize);
        Cursor { row, col }
    }

    fn post_changed(&self, ctx: &mut crate::event::WidgetCtx) {
        ctx.post_message(TextAreaChanged { value: self.text() });
    }

    fn post_selection_changed(&self, ctx: &mut crate::event::WidgetCtx) {
        let (a, b) = normalized_selection(self.selection);
        ctx.post_message(TextAreaSelectionChanged {
            start: (a.row, a.col),
            end: (b.row, b.col),
        });
    }

    fn delete_to_start_of_line(&mut self) {
        if self.delete_selection_if_any() {
            return;
        }
        if self.cursor.col > 0 {
            let row = self.cursor.row;
            self.edit(Edit::new("", (row, 0), (row, self.cursor.col), false));
        }
    }

    fn delete_to_end_of_line(&mut self) {
        if self.delete_selection_if_any() {
            return;
        }
        let row = self.cursor.row;
        let line_len = self.document.line(row).len();
        if self.cursor.col < line_len {
            self.edit(Edit::new(
                "",
                (row, self.cursor.col),
                (row, line_len),
                false,
            ));
        }
    }

    fn delete_current_line(&mut self) {
        let line_count = self.document.line_count();
        let row = self.cursor.row.min(line_count.saturating_sub(1));
        if line_count > 1 {
            if row + 1 < line_count {
                // Remove the line together with its trailing newline.
                self.edit(Edit::new("", (row, 0), (row + 1, 0), false));
            } else {
                // Last line: remove the preceding newline and the line.
                let prev_len = self.document.line(row - 1).len();
                let row_len = self.document.line(row).len();
                self.edit(Edit::new("", (row - 1, prev_len), (row, row_len), false));
            }
        } else {
            let row_len = self.document.line(0).len();
            self.edit(Edit::new("", (0, 0), (0, row_len), false));
        }
    }

    fn select_all(&mut self) -> bool {
        let line_count = self.document.line_count();
        if line_count == 1 && self.document.line(0).is_empty() {
            return false;
        }
        let last_row = line_count.saturating_sub(1);
        let last_col = self.document.line(last_row).len();
        let start = Cursor { row: 0, col: 0 };
        let end = Cursor {
            row: last_row,
            col: last_col,
        };
        self.selection = Selection { start, end };
        self.cursor = end;
        true
    }

    fn select_line(&mut self) -> bool {
        let row = self.cursor.row;
        if row >= self.document.line_count() {
            return false;
        }
        let line_len = self.document.line(row).len();
        self.selection = Selection {
            start: Cursor { row, col: 0 },
            end: Cursor { row, col: line_len },
        };
        self.cursor = Cursor { row, col: line_len };
        true
    }

    fn register_builtin_languages(&mut self) {
        // Best effort: if query compilation fails, keep working without syntax highlighting.
        let _ = self.register_language(
            "python",
            tree_sitter_python::LANGUAGE.into(),
            Self::PYTHON_HIGHLIGHTS,
        );
    }

    fn active_theme(&self) -> Option<&TextAreaTheme> {
        let name = self.theme.as_deref()?;
        self.themes.get(name)
    }

    fn default_syntax_styles() -> &'static HashMap<String, Style> {
        use std::sync::OnceLock;
        static STYLES: OnceLock<HashMap<String, Style>> = OnceLock::new();
        STYLES.get_or_init(|| {
            let mut m = HashMap::new();
            let red = parse_color_like("red").unwrap_or(Color::rgb(255, 0, 0));
            let magenta = parse_color_like("magenta").unwrap_or(Color::rgb(255, 0, 255));
            let cyan = parse_color_like("cyan").unwrap_or(Color::rgb(0, 255, 255));
            let yellow = parse_color_like("yellow").unwrap_or(Color::rgb(255, 255, 0));
            let green = parse_color_like("#4EBF71").unwrap_or(Color::rgb(78, 191, 113));
            let blue = parse_color_like("$primary").unwrap_or(Color::rgb(1, 120, 212));
            m.insert("string".to_string(), Style::default().fg(red));
            m.insert("comment".to_string(), Style::default().fg(magenta));
            m.insert("keyword".to_string(), Style::default().fg(blue).bold(true));
            m.insert("number".to_string(), Style::default().fg(cyan));
            m.insert("type".to_string(), Style::default().fg(green));
            m.insert("function".to_string(), Style::default().fg(yellow));
            m.insert("operator".to_string(), Style::default().fg(yellow));
            m.insert("attribute".to_string(), Style::default().fg(yellow));
            m.insert("constant".to_string(), Style::default().fg(cyan).bold(true));
            m
        })
    }

    fn rebuild_line_offsets(&self) -> Vec<usize> {
        let lines = self.document.lines();
        let mut offsets = Vec::with_capacity(lines.len().max(1));
        let mut cur = 0usize;
        for (i, line) in lines.iter().enumerate() {
            offsets.push(cur);
            cur = cur.saturating_add(line.len());
            // Join adds '\n' between lines, but not after the last one.
            if i + 1 < lines.len() {
                cur = cur.saturating_add(1);
            }
        }
        offsets
    }

    fn lookup_syntax_style<'a>(
        capture: &str,
        map: &'a HashMap<String, Style>,
    ) -> Option<&'a Style> {
        if let Some(style) = map.get(capture) {
            return Some(style);
        }
        // Capture names often use dotted sub-categories (e.g. `type.builtin`).
        let prefix = capture.split('.').next().unwrap_or(capture);
        map.get(prefix)
    }

    fn recompute_syntax_cache(&self, cache: &mut SyntaxCache) {
        cache.revision = self.doc_revision;
        cache.spans.clear();
        cache.line_offsets = self.rebuild_line_offsets();

        let Some(lang) = self.language.as_deref() else {
            return;
        };
        let Some(def) = self.languages.get(lang) else {
            return;
        };

        let theme_styles = self
            .active_theme()
            .map(|t| &t.syntax_styles)
            .filter(|m| !m.is_empty());
        let style_map: &HashMap<String, Style> =
            theme_styles.unwrap_or(Self::default_syntax_styles());

        // The syntax source is always LF-joined regardless of the document's
        // newline style: `rebuild_line_offsets` assumes a 1-byte separator
        // when mapping tree-sitter byte ranges back to (row, col).
        let text = self.document.lines().join("\n");
        let mut parser = Parser::new();
        if parser.set_language(&def.language).is_err() {
            return;
        }
        let Some(tree) = parser.parse(&text, None) else {
            return;
        };
        let root = tree.root_node();
        let mut cursor = QueryCursor::new();
        let capture_names = def.highlight_query.capture_names();

        for m in cursor.matches(&def.highlight_query, root, text.as_bytes()) {
            for cap in m.captures {
                let name = capture_names.get(cap.index as usize).copied().unwrap_or("");
                let Some(style) = Self::lookup_syntax_style(name, style_map) else {
                    continue;
                };
                let range = cap.node.byte_range();
                if range.start >= range.end {
                    continue;
                }
                cache.spans.push(SyntaxSpan {
                    start: range.start,
                    end: range.end,
                    style: style.clone(),
                });
            }
        }
    }

    fn rebuild_classes(&mut self) {
        let mut classes = vec!["text-area".to_string()];
        if self.code_editor {
            classes.push("-code-editor".to_string());
        }
        if self.read_only {
            classes.push("-read-only".to_string());
        }
        self.seed.classes = classes;
    }

    fn clamp_cursor(&mut self) {
        self.cursor.row = self
            .cursor
            .row
            .min(self.document.line_count().saturating_sub(1));
        let line = self.document.line(self.cursor.row);
        self.cursor.col = clamp_grapheme_boundary(line, self.cursor.col.min(line.len()));
        self.selection = Selection::cursor(self.cursor);
    }

    fn clamp_cursor_pos(&self, cursor: Cursor) -> Cursor {
        let row = cursor.row.min(self.document.line_count().saturating_sub(1));
        let line = self.document.line(row);
        let mut col = cursor.col.min(line.len());
        col = clamp_grapheme_boundary(line, col);
        Cursor { row, col }
    }

    fn line_number_gutter_width(&self) -> usize {
        if !self.show_line_numbers {
            return 0;
        }
        let digits = self.document.line_count().max(1).to_string().len();
        // Right aligned number + 2-cell margin (matches Python Textual gutter_margin).
        digits + 2
    }

    fn cursor_cell_x(&self) -> usize {
        let line = self.document.line(self.cursor.row);
        grapheme_cell_len_prefix(line, self.cursor.col)
    }

    fn cursor_from_cell_x(&self, row: usize, cell_x: usize) -> usize {
        let line = self.document.line(row);
        grapheme_byte_index_from_cell_x(line, cell_x)
    }

    fn cursor_left_pos(&self, from: Cursor) -> Cursor {
        let row = from.row.min(self.document.line_count().saturating_sub(1));
        if from.col > 0 {
            let line = self.document.line(row);
            let col = prev_grapheme_boundary(line, from.col);
            Cursor { row, col }
        } else if row > 0 {
            let row = row - 1;
            let col = self.document.line(row).len();
            Cursor { row, col }
        } else {
            Cursor { row, col: 0 }
        }
    }

    fn cursor_right_pos(&self, from: Cursor) -> Cursor {
        let row = from.row.min(self.document.line_count().saturating_sub(1));
        let line_len = self.document.line(row).len();
        if from.col < line_len {
            let next = next_grapheme_boundary(self.document.line(row), from.col);
            Cursor { row, col: next }
        } else if row + 1 < self.document.line_count() {
            Cursor {
                row: row + 1,
                col: 0,
            }
        } else {
            Cursor { row, col: line_len }
        }
    }

    /// Scroll so the cursor is visible. Vertical scrolling moves in
    /// visual-offset space; horizontal scrolling applies only when the wrap
    /// width is 0 (wrapped documents never scroll horizontally).
    fn ensure_cursor_visible(&mut self, view_height: usize, view_width: usize) {
        let (cursor_x, cursor_y) = self
            .wrapped
            .location_to_offset(&self.document, self.cursor.location());
        self.scroll_row = self.scroll_row.min(cursor_y);
        if cursor_y >= self.scroll_row + view_height {
            self.scroll_row = cursor_y.saturating_sub(view_height.saturating_sub(1));
        }

        if self.wrapped.width() == 0 {
            self.scroll_col = self.scroll_col.min(cursor_x);
            if cursor_x >= self.scroll_col + view_width {
                self.scroll_col = cursor_x.saturating_sub(view_width.saturating_sub(1));
            }
        } else {
            self.scroll_col = 0;
        }
    }

    fn adjust_scroll_to_cursor(&mut self) {
        if !self.layout_initialized {
            return;
        }
        if !self.selection.is_empty() {
            let (a, _b) = normalized_selection(self.selection);
            let (_, top_y) = self
                .wrapped
                .location_to_offset(&self.document, a.location());
            self.scroll_row = self.scroll_row.min(top_y);
        }
        let gutter_w = self.line_number_gutter_width();
        let view_w = (self.layout_w.max(1) as usize)
            .saturating_sub(gutter_w)
            .max(1);
        let view_h = self.layout_h.max(1) as usize;
        self.ensure_cursor_visible(view_h, view_w);
    }

    fn reset_blink(&mut self) {
        if !self.node_state().focused || !self.app_active {
            return;
        }
        self.cursor_visible = true;
        if self.cursor_blink_enabled {
            self.cursor_blink_next_at = Some(Self::next_blink_deadline());
        } else {
            self.cursor_blink_next_at = None;
        }
    }

    /// Delete the selected text as a single history-recorded edit, if any.
    fn delete_selection_if_any(&mut self) -> bool {
        if self.selection.is_empty() {
            return false;
        }
        let (a, b) = normalized_selection(self.selection);
        self.edit(Edit::new("", a.location(), b.location(), false));
        true
    }

    /// Keyboard backspace (Python `action_delete_left`).
    fn backspace(&mut self) {
        if self.delete_selection_if_any() {
            return;
        }
        let left = self.cursor_left_pos(self.cursor);
        if left == self.cursor {
            return;
        }
        self.edit(Edit::new(
            "",
            self.cursor.location(),
            left.location(),
            false,
        ));
    }

    /// Keyboard forward delete (Python `action_delete_right`).
    fn delete_right(&mut self) {
        if self.delete_selection_if_any() {
            return;
        }
        let right = self.cursor_right_pos(self.cursor);
        if right == self.cursor {
            return;
        }
        self.edit(Edit::new(
            "",
            self.cursor.location(),
            right.location(),
            false,
        ));
    }

    fn cursor_word_left_pos(&self, from: Cursor) -> Cursor {
        let row = from.row.min(self.document.line_count().saturating_sub(1));
        if from.col > 0 {
            let line = self.document.line(row);
            let col = prev_word_boundary(line, from.col);
            Cursor { row, col }
        } else if row > 0 {
            let row = row - 1;
            let line = self.document.line(row);
            let col = prev_word_boundary(line, line.len());
            Cursor { row, col }
        } else {
            Cursor { row: 0, col: 0 }
        }
    }

    fn cursor_word_right_pos(&self, from: Cursor) -> Cursor {
        let row = from.row.min(self.document.line_count().saturating_sub(1));
        let line = self.document.line(row);
        if from.col < line.len() {
            let col = next_word_boundary(line, from.col);
            Cursor { row, col }
        } else if row + 1 < self.document.line_count() {
            let row = row + 1;
            let col = next_word_boundary(self.document.line(row), 0);
            Cursor { row, col }
        } else {
            Cursor {
                row,
                col: line.len(),
            }
        }
    }

    fn move_cursor_with_selection(&mut self, next: Cursor, select: bool) -> bool {
        let old_cursor = self.cursor;
        let old_selection = self.selection;
        let next = self.clamp_cursor_pos(next);
        if select {
            if self.selection.is_empty() {
                self.selection = Selection {
                    start: self.cursor,
                    end: next,
                };
            } else {
                self.selection.end = next;
            }
        } else {
            self.selection = Selection::cursor(next);
        }
        self.cursor = next;
        // Every non-edit cursor movement creates an undo checkpoint (Python
        // `move_cursor`).
        self.history.checkpoint();
        self.cursor != old_cursor || self.selection != old_selection
    }

    fn backspace_word(&mut self) {
        if self.delete_selection_if_any() {
            return;
        }
        let start = self.cursor_word_left_pos(self.cursor);
        if start == self.cursor {
            return;
        }
        self.edit(Edit::new(
            "",
            self.cursor.location(),
            start.location(),
            false,
        ));
    }

    fn delete_word(&mut self) {
        if self.delete_selection_if_any() {
            return;
        }
        let end = self.cursor_word_right_pos(self.cursor);
        if end == self.cursor {
            return;
        }
        self.edit(Edit::new("", self.cursor.location(), end.location(), false));
    }

    fn selected_text(&self) -> Option<String> {
        if self.selection.is_empty() {
            return None;
        }
        let (a, b) = normalized_selection(self.selection);
        Some(self.document.get_text_range(a.location(), b.location()))
    }

    fn cut_current_line(&mut self) -> Option<String> {
        let line_count = self.document.line_count();
        let row = self.cursor.row.min(line_count.saturating_sub(1));
        let mut copied = self.document.line(row).to_string();
        if line_count > 1 {
            if row + 1 < line_count {
                copied.push_str(self.document.newline());
                self.edit(Edit::new("", (row, 0), (row + 1, 0), false));
            } else {
                let prev_len = self.document.line(row - 1).len();
                let row_len = self.document.line(row).len();
                self.edit(Edit::new("", (row - 1, prev_len), (row, row_len), false));
            }
        } else {
            let row_len = self.document.line(0).len();
            self.edit(Edit::new("", (0, 0), (0, row_len), false));
        }
        Some(copied)
    }

    /// Replace the selection with text that may span multiple lines, as a
    /// single history-recorded edit (Python `_on_paste` ->
    /// `_replace_via_keyboard`). Newlines of any style are normalized into
    /// document lines by `Document::replace_range`; the document's own
    /// newline style governs `text()` read-back.
    fn insert_multiline(&mut self, text: &str) -> bool {
        if text.is_empty() {
            return false;
        }
        self.edit(Edit::new(
            text,
            self.selection.start.location(),
            self.selection.end.location(),
            false,
        ));
        true
    }

    // ── Watchers ─────────────────────────────────────────────────────────

    fn watch_read_only(&mut self, _old: &bool, _new: &bool, _ctx: &mut ReactiveCtx) {
        self.rebuild_classes();
    }

    fn watch_soft_wrap(&mut self, _old: &bool, _new: &bool, _ctx: &mut ReactiveCtx) {
        // Layout invalidation is handled by ReactiveFlags::reactive_layout();
        // re-wrap now so navigation stays in sync (Python
        // `_watch_soft_wrap` -> `_rewrap_and_refresh_virtual_size`).
        self.rewrap_full();
        self.adjust_scroll_to_cursor();
    }

    fn watch_language(
        &mut self,
        _old: &Option<String>,
        _new: &Option<String>,
        _ctx: &mut ReactiveCtx,
    ) {
        if let Ok(mut cache) = self.syntax_cache.lock() {
            cache.revision = u64::MAX;
        }
    }

    fn watch_cursor_blink(&mut self, _old: &bool, _new: &bool, _ctx: &mut ReactiveCtx) {
        if self.node_state().focused && self.app_active {
            self.reset_blink();
        } else {
            self.cursor_visible = false;
            self.cursor_blink_next_at = None;
        }
    }

    fn watch_theme(
        &mut self,
        _old: &Option<String>,
        _new: &Option<String>,
        _ctx: &mut ReactiveCtx,
    ) {
        if let Ok(mut cache) = self.syntax_cache.lock() {
            cache.revision = u64::MAX;
        }
    }
}

impl ReactiveWidget for TextArea {
    fn reactive_dispatch(&mut self, changes: &[ReactiveChange], ctx: &mut ReactiveCtx) {
        for change in changes {
            match change.field_name {
                "read_only" => {
                    if let (Some(old), Some(new)) = (
                        change.old_value.downcast_ref::<bool>(),
                        change.new_value.downcast_ref::<bool>(),
                    ) {
                        self.watch_read_only(old, new, ctx);
                    }
                }
                "soft_wrap" => {
                    if let (Some(old), Some(new)) = (
                        change.old_value.downcast_ref::<bool>(),
                        change.new_value.downcast_ref::<bool>(),
                    ) {
                        self.watch_soft_wrap(old, new, ctx);
                    }
                }
                "language" => {
                    if let (Some(old), Some(new)) = (
                        change.old_value.downcast_ref::<Option<String>>(),
                        change.new_value.downcast_ref::<Option<String>>(),
                    ) {
                        self.watch_language(old, new, ctx);
                    }
                }
                "cursor_blink_enabled" => {
                    if let (Some(old), Some(new)) = (
                        change.old_value.downcast_ref::<bool>(),
                        change.new_value.downcast_ref::<bool>(),
                    ) {
                        self.watch_cursor_blink(old, new, ctx);
                    }
                }
                "theme" => {
                    if let (Some(old), Some(new)) = (
                        change.old_value.downcast_ref::<Option<String>>(),
                        change.new_value.downcast_ref::<Option<String>>(),
                    ) {
                        self.watch_theme(old, new, ctx);
                    }
                }
                _ => {}
            }
        }
    }
}

impl crate::widgets::Focus for TextArea {
    fn focusable(&self) -> bool {
        true
    }

    fn is_active(&self) -> bool {
        self.mouse_down
    }

    fn action_namespace(&self) -> &str {
        "text-area"
    }

    fn bindings(&self) -> Vec<BindingDecl> {
        vec![
            BindingDecl::new("ctrl+z", "undo", "Undo").hidden(),
            BindingDecl::new("ctrl+y", "redo", "Redo").hidden(),
            BindingDecl::new("ctrl+shift+z", "redo", "Redo").hidden(),
        ]
    }

    fn execute_action(&mut self, action: &ParsedAction, ctx: &mut crate::event::WidgetCtx) -> bool {
        // Undo/redo are deliberately NOT gated on `read_only` (Python does
        // not gate `action_undo`/`action_redo` either).
        match action.name.as_str() {
            "undo" => {
                if self.undo() {
                    self.record_cursor_width();
                    self.adjust_scroll_to_cursor();
                    self.reset_blink();
                    self.post_changed(ctx);
                    ctx.request_repaint();
                }
                ctx.set_handled();
                true
            }
            "redo" => {
                if self.redo() {
                    self.record_cursor_width();
                    self.adjust_scroll_to_cursor();
                    self.reset_blink();
                    self.post_changed(ctx);
                    ctx.request_repaint();
                }
                ctx.set_handled();
                true
            }
            _ => false,
        }
    }
}

impl crate::widgets::Interactive for TextArea {
    fn on_node_state_changed(&mut self, old: NodeState, new: NodeState) {
        if old.focused != new.focused {
            if !new.focused {
                self.mouse_down = false;
                self.cursor_visible = false;
                self.cursor_blink_next_at = None;
            } else {
                self.reset_blink();
                // Gaining focus creates an undo checkpoint (Python
                // `_watch_has_focus`).
                self.history.checkpoint();
            }
        }
    }

    fn on_mouse_move(&mut self, x: u16, y: u16) -> bool {
        if !self.mouse_down {
            return false;
        }
        let next = self.hit_test_location(x, y);
        if next == self.selection.end && next == self.cursor {
            return false;
        }
        self.selection.end = next;
        self.cursor = next;
        true
    }

    fn on_event(&mut self, event: &Event, ctx: &mut crate::event::WidgetCtx) {
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
                if !self.node_state().focused || !self.app_active {
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
            Event::MouseDown(mouse) if mouse.target == self.node_id() => {
                self.cursor = self.hit_test_location(mouse.x, mouse.y);
                self.selection = Selection::cursor(self.cursor);
                self.mouse_down = true;
                self.record_cursor_width();
                self.adjust_scroll_to_cursor();
                self.reset_blink();
                // A mouse click creates an undo checkpoint (Python
                // `_on_mouse_down`).
                self.history.checkpoint();
                ctx.request_repaint();
                ctx.set_handled();
            }
            Event::MouseUp(_) if self.mouse_down => {
                self.mouse_down = false;
                ctx.request_repaint();
            }
            Event::Key(key) if self.node_state().focused => {
                if !self.read_only && matches!(key.code, KeyCode::Char('\u{7f}' | '\u{08}')) {
                    self.backspace();
                    self.post_changed(ctx);
                    self.record_cursor_width();
                    self.adjust_scroll_to_cursor();
                    self.reset_blink();
                    ctx.request_repaint();
                    ctx.set_handled();
                    return;
                }

                let Some(cmd) = edit_command_from_key(key, true) else {
                    return;
                };

                // Determine if this is a mutating command.
                let is_mutation = matches!(
                    cmd,
                    EditCommand::InsertChar(_)
                        | EditCommand::InsertNewline
                        | EditCommand::Backspace { .. }
                        | EditCommand::Delete { .. }
                        | EditCommand::DeleteToStart
                        | EditCommand::DeleteToEnd
                        | EditCommand::DeleteLine
                        | EditCommand::Cut
                        | EditCommand::Paste
                );

                // Block mutations in read-only mode.
                if self.read_only && is_mutation {
                    ctx.set_handled();
                    return;
                }

                let old_selection = self.selection;
                let mut changed = false;
                let mut value_changed = false;
                // Python `move_cursor(record_width=...)`: vertical moves keep
                // the remembered visual x; everything else records it.
                let mut record_width = true;

                match cmd {
                    EditCommand::InsertChar(ch) => {
                        if ch != '\t' {
                            // Replace the selection with the typed character
                            // as a single edit (Python `_replace_via_keyboard`).
                            self.edit(Edit::new(
                                ch.to_string(),
                                self.selection.start.location(),
                                self.selection.end.location(),
                                false,
                            ));
                            changed = true;
                            value_changed = true;
                        }
                    }
                    EditCommand::InsertNewline => {
                        self.edit(Edit::new(
                            "\n",
                            self.selection.start.location(),
                            self.selection.end.location(),
                            false,
                        ));
                        changed = true;
                        value_changed = true;
                    }
                    EditCommand::Backspace { unit } => {
                        let before = self.text();
                        match unit {
                            MoveUnit::Grapheme => self.backspace(),
                            MoveUnit::Word => self.backspace_word(),
                        }
                        changed = before != self.text();
                        value_changed = changed;
                    }
                    EditCommand::Delete { unit } => {
                        let before = self.text();
                        match unit {
                            MoveUnit::Grapheme => self.delete_right(),
                            MoveUnit::Word => self.delete_word(),
                        }
                        changed = before != self.text();
                        value_changed = changed;
                    }
                    EditCommand::DeleteToStart => {
                        let before = self.text();
                        self.delete_to_start_of_line();
                        changed = before != self.text();
                        value_changed = changed;
                    }
                    EditCommand::DeleteToEnd => {
                        let before = self.text();
                        self.delete_to_end_of_line();
                        changed = before != self.text();
                        value_changed = changed;
                    }
                    EditCommand::DeleteLine => {
                        self.delete_current_line();
                        changed = true;
                        value_changed = true;
                    }
                    EditCommand::SelectAll => {
                        changed = self.select_all();
                    }
                    EditCommand::SelectLine => {
                        changed = self.select_line();
                    }
                    EditCommand::MoveLeft { select, unit } => {
                        let next = match unit {
                            MoveUnit::Grapheme => self.cursor_left_pos(self.cursor),
                            MoveUnit::Word => self.cursor_word_left_pos(self.cursor),
                        };
                        changed = self.move_cursor_with_selection(next, select);
                    }
                    EditCommand::MoveRight { select, unit } => {
                        let next = match unit {
                            MoveUnit::Grapheme => self.cursor_right_pos(self.cursor),
                            MoveUnit::Word => self.cursor_word_right_pos(self.cursor),
                        };
                        changed = self.move_cursor_with_selection(next, select);
                    }
                    EditCommand::MoveUp { select } => {
                        // Wrap-aware movement (Python `get_location_above`):
                        // Up on the first wrapped line moves to (0, 0).
                        let next = self.navigator.get_location_above(
                            &self.document,
                            &self.wrapped,
                            self.cursor.location(),
                        );
                        changed = self.move_cursor_with_selection(next.into(), select);
                        record_width = false;
                    }
                    EditCommand::MoveDown { select } => {
                        // Wrap-aware movement (Python `get_location_below`):
                        // Down on the last wrapped line moves to the line end.
                        let next = self.navigator.get_location_below(
                            &self.document,
                            &self.wrapped,
                            self.cursor.location(),
                        );
                        changed = self.move_cursor_with_selection(next.into(), select);
                        record_width = false;
                    }
                    EditCommand::MoveHome { select } => {
                        // Home moves to the previous wrap offset when the
                        // line is wrapped, else column 0.
                        let next = self.navigator.get_location_home(
                            &self.document,
                            &self.wrapped,
                            self.cursor.location(),
                            false,
                        );
                        changed = self.move_cursor_with_selection(next.into(), select);
                    }
                    EditCommand::MoveEnd { select } => {
                        // End moves to the end of the current wrapped
                        // section, else the line end.
                        let next = self.navigator.get_location_end(
                            &self.document,
                            &self.wrapped,
                            self.cursor.location(),
                        );
                        changed = self.move_cursor_with_selection(next.into(), select);
                    }
                    EditCommand::Copy => {
                        if let Some(text) = self.selected_text() {
                            ctx.post_message(TextEditClipboardCopyRequested { text, cut: false });
                        }
                    }
                    EditCommand::Cut => {
                        if let Some(text) = self.selected_text() {
                            ctx.post_message(TextEditClipboardCopyRequested { text, cut: true });
                            if self.delete_selection_if_any() {
                                changed = true;
                                value_changed = true;
                            }
                        } else if let Some(text) = self.cut_current_line() {
                            ctx.post_message(TextEditClipboardCopyRequested { text, cut: true });
                            changed = true;
                            value_changed = true;
                        }
                    }
                    EditCommand::Paste => {
                        ctx.post_message(TextEditClipboardPasteRequested {
                            target: self.node_id(),
                        });
                    }
                    EditCommand::Submit => {}
                }

                if value_changed {
                    self.post_changed(ctx);
                }
                if self.selection != old_selection {
                    self.post_selection_changed(ctx);
                }
                if changed || value_changed {
                    if record_width {
                        self.record_cursor_width();
                    }
                    self.adjust_scroll_to_cursor();
                    self.reset_blink();
                    ctx.request_repaint();
                }
                ctx.set_handled();
            }
            _ => {}
        }
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut crate::event::WidgetCtx) {
        if let Some(m) = message.downcast_ref::<TextEditClipboardPaste>() {
            if m.target != self.node_id() {
                return;
            }
            if self.read_only {
                return;
            }
            if self.insert_multiline(&m.text) {
                self.post_changed(ctx);
                self.record_cursor_width();
                self.adjust_scroll_to_cursor();
                self.reset_blink();
                ctx.request_repaint();
                ctx.set_handled();
            }
        }
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        let width = width.max(1);
        let height = height.max(1);
        if self.layout_w != width || self.layout_h != height {
            self.layout_w = width;
            self.layout_h = height;
        }
        // Re-wrap when the effective wrap width changed (content width or
        // gutter width change; Python `_on_resize`).
        if self.wrapped.width() != self.wrap_width() {
            self.rewrap_full();
        }
        self.layout_initialized = true;
        self.adjust_scroll_to_cursor();
    }
}

impl crate::widgets::Selectable for TextArea {
    fn get_selection(&self) -> Option<String> {
        self.selected_text()
    }
}

impl crate::widgets::Render for TextArea {
    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);
        let gutter_w = self.line_number_gutter_width();
        let text_w = width.saturating_sub(gutter_w).max(1);

        let base_meta = crate::css::selector_meta_generic(self);
        let base_style = crate::css::resolve_style(self, &base_meta);
        let fallback_bg = parse_color_like("$background").unwrap_or(Color::rgb(0, 0, 0));
        let base_bg = base_style.bg.unwrap_or(fallback_bg);

        let theme = self.active_theme();
        let resolve_component_style = |class: &str| -> Style {
            if let Some(theme) = theme {
                let override_style = match class {
                    "text-area--cursor" => theme.cursor_style.clone(),
                    "text-area--cursor-line" => theme.cursor_line_style.clone(),
                    "text-area--selection" => theme.selection_style.clone(),
                    "text-area--gutter" => theme.gutter_style.clone(),
                    "text-area--gutter-active" => theme.gutter_active_style.clone(),
                    _ => Style::default(),
                };
                if !override_style.is_empty() {
                    return override_style;
                }
            }
            let meta = crate::css::selector_meta_component(
                crate::widgets::Widget::style_type(self),
                &[class],
            );
            crate::css::resolve_style_for_meta(&meta)
        };

        let cursor_style = resolve_component_style("text-area--cursor");
        let selection_style = resolve_component_style("text-area--selection");
        let gutter_style = resolve_component_style("text-area--gutter");
        let gutter_active_style = resolve_component_style("text-area--gutter-active");
        let cursor_line_style = resolve_component_style("text-area--cursor-line");

        let placeholder_style = resolve_component_style("text-area--placeholder");
        let cursor_rich = compose_rich(&cursor_style, base_bg);
        let selection_rich = compose_rich(&selection_style, base_bg);
        let gutter_rich = compose_rich(&gutter_style, base_bg);
        let gutter_active_rich = compose_rich(&gutter_active_style, base_bg);
        let placeholder_rich = compose_rich(&placeholder_style, base_bg);

        // Show placeholder when empty.
        let is_empty = self.document.line_count() == 1 && self.document.line(0).is_empty();
        if is_empty && !self.placeholder.is_empty() {
            let mut out = Segments::new();
            for y in 0..height {
                if gutter_w > 0 {
                    out.push(Segment::new(" ".repeat(gutter_w)));
                }
                if y == 0 {
                    let line = rich_rs::set_cell_size(&self.placeholder, text_w);
                    out.push(Segment::styled(line, placeholder_rich));
                } else {
                    out.push(Segment::new(" ".repeat(text_w)));
                }
                if y + 1 < height {
                    out.push(Segment::line());
                }
            }
            return out;
        }

        let syntax_cache = {
            let mut guard = self.syntax_cache.lock().unwrap_or_else(|e| e.into_inner());
            if guard.revision != self.doc_revision || guard.line_offsets.is_empty() {
                self.recompute_syntax_cache(&mut guard);
            }
            guard.clone()
        };

        // Render one wrapped SECTION per visual row. The widget-owned
        // wrapped view is kept in sync by the edit funnel and `on_layout`;
        // if this render sees a different width (first frame before layout,
        // or direct renders in tests), build a consistent local view.
        let render_wrap_width = if self.soft_wrap {
            text_w.saturating_sub(1)
        } else {
            0
        };
        let fallback_wrapped;
        let wrapped: &WrappedDocument = if self.wrapped.width() == render_wrap_width {
            &self.wrapped
        } else {
            fallback_wrapped =
                WrappedDocument::new(&self.document, render_wrap_width, self.indent_width);
            &fallback_wrapped
        };

        let (sel_a, sel_b) = normalized_selection(self.selection);

        let mut out = Segments::new();
        for y in 0..height {
            let visual_row = self.scroll_row + y;
            let line_info = wrapped.offset_line_info(visual_row);
            let row_for_style = line_info.map(|(row, _)| row);
            let is_cursor_line = self.node_state().focused
                && self.app_active
                && row_for_style == Some(self.cursor.row);
            let line_bg_style = if is_cursor_line {
                Some(cursor_line_style.clone())
            } else {
                None
            };
            if gutter_w > 0 {
                // Line numbers appear on the FIRST section of a line only;
                // continuation sections and padding rows get a blank gutter.
                let gutter_text = match line_info {
                    Some((row, 0)) => {
                        let line_no = row.saturating_add(1);
                        let digits = gutter_w.saturating_sub(2).max(1);
                        format!("{line_no:>digits$}  ")
                    }
                    _ => " ".repeat(gutter_w),
                };
                let style = if self.node_state().focused && row_for_style == Some(self.cursor.row) {
                    gutter_active_rich
                } else {
                    gutter_rich
                };
                out.push(Segment::styled(
                    rich_rs::set_cell_size(&gutter_text, gutter_w),
                    style,
                ));
            }

            let Some((row, section_index)) = line_info else {
                out.push(Segment::new(" ".repeat(text_w)));
                if y + 1 < height {
                    out.push(Segment::line());
                }
                continue;
            };

            let line = self.document.line(row);
            let offsets: &[usize] = wrapped.get_offsets(row).unwrap_or(&[]);
            let section_start = if section_index == 0 {
                0
            } else {
                offsets[section_index - 1]
            };
            let section_end = offsets.get(section_index).copied().unwrap_or(line.len());
            let is_last_section = section_index == offsets.len();
            let section = &line[section_start..section_end];

            let eol_in_sel = is_last_section
                && !self.selection.is_empty()
                && cursor_le(
                    sel_a,
                    Cursor {
                        row,
                        col: line.len(),
                    },
                )
                && cursor_lt(
                    Cursor {
                        row,
                        col: line.len(),
                    },
                    sel_b,
                );
            let line_abs_offset = syntax_cache.line_offsets.get(row).copied().unwrap_or(0);
            // Horizontal scrolling applies only when unwrapped.
            let start_cell = if render_wrap_width == 0 {
                self.scroll_col
            } else {
                0
            };
            let mut cell_x = 0usize;
            let mut pending_style: Option<rich_rs::Style> = None;
            let mut pending_text = String::new();

            let flush = |out: &mut Segments,
                         pending_style: &mut Option<rich_rs::Style>,
                         pending_text: &mut String| {
                if pending_text.is_empty() {
                    return;
                }
                let style = pending_style.take().unwrap_or_default();
                out.push(Segment::styled(std::mem::take(pending_text), style));
            };

            for (section_byte_idx, grapheme) in section.grapheme_indices(true) {
                // Document-space byte position (cursor/selection/syntax all
                // key off document space, agnostic to the visual break).
                let byte_idx = section_start + section_byte_idx;
                let w = grapheme_width(grapheme);
                let ch_cell_start = grapheme_cell_len_prefix(section, section_byte_idx);
                let ch_cell_end = ch_cell_start + w;

                if ch_cell_end <= start_cell {
                    continue;
                }
                if cell_x >= text_w {
                    break;
                }

                let is_cursor = self.node_state().focused
                    && self.cursor_visible
                    && row == self.cursor.row
                    && byte_idx == self.cursor.col;
                let in_sel = !self.selection.is_empty()
                    && cursor_le(sel_a, Cursor { row, col: byte_idx })
                    && cursor_lt(Cursor { row, col: byte_idx }, sel_b);
                let style = if is_cursor {
                    Some(cursor_rich)
                } else if in_sel {
                    Some(selection_rich)
                } else {
                    let abs = line_abs_offset.saturating_add(byte_idx);
                    let mut syntax: Option<Style> = None;
                    for span in &syntax_cache.spans {
                        if abs >= span.start && abs < span.end {
                            syntax = Some(span.style.clone());
                        }
                    }
                    let mut merged = syntax.unwrap_or_default();
                    if let Some(ref bg) = line_bg_style {
                        if merged.bg.is_none() {
                            merged.bg = bg.bg;
                        }
                    }
                    if merged.is_empty() {
                        None
                    } else {
                        Some(compose_rich(&merged, base_bg))
                    }
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

                pending_text.push_str(grapheme);
                cell_x += w;
            }
            flush(&mut out, &mut pending_style, &mut pending_text);

            // Cursor at end of line: paint a single cell with cursor style
            // (the cursor rests past the end only on the final section).
            if is_last_section
                && self.node_state().focused
                && self.cursor_visible
                && row == self.cursor.row
            {
                let end_cell = grapheme_cell_len_prefix(section, section.len());
                if self.cursor.col == line.len() && end_cell >= start_cell && cell_x < text_w {
                    out.push(Segment::styled(" ".to_string(), cursor_rich));
                    cell_x += 1;
                }
            }

            if cell_x < text_w {
                let pad = " ".repeat(text_w - cell_x);
                if eol_in_sel {
                    out.push(Segment::styled(pad, selection_rich));
                } else if let Some(bg) = line_bg_style {
                    if bg.is_empty() {
                        out.push(Segment::new(pad));
                    } else {
                        out.push(Segment::styled(pad, compose_rich(&bg, base_bg)));
                    }
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
}

fn compose_rich(style: &Style, base_bg: Color) -> rich_rs::Style {
    let mut rich = style.to_rich_without_colors().unwrap_or_default();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EventCtx;
    use crate::keys::KeyEventData;
    use crate::node_id::NodeId;
    use crate::runtime::dispatch_ctx::set_dispatch_recipient;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn make_node_id() -> NodeId {
        use slotmap::SlotMap;
        let mut sm: SlotMap<NodeId, ()> = SlotMap::new();
        sm.insert(())
    }

    fn focused_state() -> NodeState {
        NodeState {
            focused: true,
            ..Default::default()
        }
    }

    #[test]
    fn typing_emits_text_area_changed_message() {
        let mut text_area = TextArea::new("");
        let _guard = set_dispatch_recipient(make_node_id(), focused_state());
        let mut ctx = EventCtx::default();

        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(
                crate::node_id::NodeId::default(),
                &mut ctx,
            );
            text_area.on_event(
                &Event::Key(KeyEventData::from_crossterm(KeyEvent::new(
                    KeyCode::Char('x'),
                    KeyModifiers::NONE,
                ))),
                &mut __w,
            );
        }

        let messages = ctx.take_messages();
        assert!(messages.iter().any(|m| {
            m.downcast_ref::<TextAreaChanged>()
                .is_some_and(|c| c.value == "x")
        }));
    }

    #[test]
    fn clipboard_commands_emit_messages() {
        let mut text_area = TextArea::new("hello\nworld");
        let id = make_node_id();
        let _guard = set_dispatch_recipient(id, focused_state());
        text_area.set_selection(Selection {
            start: Cursor { row: 0, col: 0 },
            end: Cursor { row: 0, col: 5 },
        });

        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(
                crate::node_id::NodeId::default(),
                &mut ctx,
            );
            text_area.on_event(
                &Event::Key(KeyEventData::from_crossterm(KeyEvent::new(
                    KeyCode::Char('c'),
                    KeyModifiers::CONTROL,
                ))),
                &mut __w,
            );
        }
        let copy_messages = ctx.take_messages();
        assert!(copy_messages.iter().any(|m| {
            m.downcast_ref::<TextEditClipboardCopyRequested>()
                .is_some_and(|r| r.text == "hello" && !r.cut)
        }));

        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(
                crate::node_id::NodeId::default(),
                &mut ctx,
            );
            text_area.on_event(
                &Event::Key(KeyEventData::from_crossterm(KeyEvent::new(
                    KeyCode::Char('v'),
                    KeyModifiers::CONTROL,
                ))),
                &mut __w,
            );
        }
        let paste_messages = ctx.take_messages();
        assert!(paste_messages.iter().any(|m| {
            m.downcast_ref::<TextEditClipboardPasteRequested>()
                .is_some_and(|r| r.target == id)
        }));
    }

    #[test]
    fn paste_message_inserts_multiline_text() {
        let mut text_area = TextArea::new("abc");
        let id = make_node_id();
        let _guard = set_dispatch_recipient(id, focused_state());
        text_area.set_selection(Selection::cursor(Cursor { row: 0, col: 1 }));

        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(
                crate::node_id::NodeId::default(),
                &mut ctx,
            );
            text_area.on_message(
                &MessageEvent::new(
                    NodeId::default(),
                    TextEditClipboardPaste {
                        target: id,
                        text: "X\nY".to_string(),
                    },
                ),
                &mut __w,
            );
        }

        assert_eq!(text_area.text(), "aX\nYbc");
        assert!(ctx.handled());
    }

    #[test]
    fn bindings_are_declared() {
        let ta = TextArea::new("hello");
        let bindings = ta.bindings();
        assert!(!bindings.is_empty());
        assert!(bindings.iter().any(|b| b.action == "undo"));
        assert!(bindings.iter().any(|b| b.action == "redo"));
    }

    #[test]
    fn execute_action_handles_undo() {
        use crate::action::ParsedAction;
        let mut ta = TextArea::new("hello world");
        let _guard = set_dispatch_recipient(make_node_id(), focused_state());
        let mut ctx = EventCtx::default();
        let action = ParsedAction {
            namespace: None,
            name: "undo".to_string(),
            arguments: vec![],
        };
        assert!({
            let mut __w = crate::event::WidgetCtx::__from_dispatch(
                crate::node_id::NodeId::default(),
                &mut ctx,
            );
            ta.execute_action(&action, &mut __w)
        });
        assert!(ctx.handled());
    }

    // ── P1-14 dispatch-context regression tests ─────────────────────────

    #[test]
    fn mouse_click_with_dispatch_context_is_handled() {
        use crate::runtime::dispatch_ctx::set_dispatch_recipient;

        let mut ta = TextArea::new("hello");
        let id = make_node_id();
        let _guard = set_dispatch_recipient(id, crate::widgets::NodeState::default());

        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(
                crate::node_id::NodeId::default(),
                &mut ctx,
            );
            ta.on_event(
                &Event::MouseDown(crate::event::MouseDownEvent {
                    target: id,
                    screen_x: 0,
                    screen_y: 0,
                    x: 0,
                    y: 0,
                }),
                &mut __w,
            );
        }
        assert!(ctx.handled());
    }

    #[test]
    fn mouse_click_with_wrong_target_is_ignored() {
        use crate::runtime::dispatch_ctx::set_dispatch_recipient;
        use slotmap::SlotMap;

        let mut ta = TextArea::new("hello");
        let mut sm: SlotMap<NodeId, ()> = SlotMap::new();
        let my_id = sm.insert(());
        let other_id = sm.insert(());
        let _guard = set_dispatch_recipient(my_id, crate::widgets::NodeState::default());

        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(
                crate::node_id::NodeId::default(),
                &mut ctx,
            );
            ta.on_event(
                &Event::MouseDown(crate::event::MouseDownEvent {
                    target: other_id,
                    screen_x: 0,
                    screen_y: 0,
                    x: 0,
                    y: 0,
                }),
                &mut __w,
            );
        }
        assert!(!ctx.handled());
    }

    #[test]
    fn paste_message_with_wrong_target_is_ignored() {
        use crate::runtime::dispatch_ctx::set_dispatch_recipient;
        use slotmap::SlotMap;

        let mut ta = TextArea::new("abc");
        let mut sm: SlotMap<NodeId, ()> = SlotMap::new();
        let my_id = sm.insert(());
        let other_id = sm.insert(());
        let _guard = set_dispatch_recipient(my_id, crate::widgets::NodeState::default());

        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(
                crate::node_id::NodeId::default(),
                &mut ctx,
            );
            ta.on_message(
                &MessageEvent::new(
                    NodeId::default(),
                    TextEditClipboardPaste {
                        target: other_id,
                        text: "XYZ".to_string(),
                    },
                ),
                &mut __w,
            );
        }
        assert!(!ctx.handled());
        assert_eq!(ta.text(), "abc");
    }
}
