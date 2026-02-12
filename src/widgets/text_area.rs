use std::collections::HashMap;
use std::time::{Duration, Instant};

use crossterm::event::KeyCode;
use unicode_segmentation::UnicodeSegmentation;

use rich_rs::{Console, ConsoleOptions, Segment, Segments};
use tree_sitter::{Parser, Query, QueryCursor};

use crate::event::{Event, EventCtx};
use crate::message::{Message, MessageEvent};
use crate::style::{Color, Style, parse_color_like};
use crate::{Error, Result};

use crate::node_id::NodeId;

use super::{
    Widget, WidgetStyles,
    helpers::{empty_classes, fixed_height_from_constraints},
    text_edit::{
        EditCommand, MoveUnit, byte_index_from_cell_x as grapheme_byte_index_from_cell_x,
        cell_len_prefix as grapheme_cell_len_prefix, clamp_grapheme_boundary,
        edit_command_from_key, grapheme_cell_width as grapheme_width, next_grapheme_boundary,
        next_word_boundary, prev_grapheme_boundary, prev_word_boundary,
    },
};

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

#[derive(Debug, Clone)]
struct UndoEntry {
    lines: Vec<String>,
    cursor: Cursor,
    selection: Selection,
}

#[derive(Debug, Clone, Default)]
struct UndoStack {
    entries: Vec<UndoEntry>,
    position: usize,
    max_entries: usize,
}

impl UndoStack {
    fn new(max: usize) -> Self {
        Self {
            entries: Vec::new(),
            position: 0,
            max_entries: max,
        }
    }

    fn push(&mut self, entry: UndoEntry) {
        // Truncate any redo entries after current position.
        self.entries.truncate(self.position);
        self.entries.push(entry);
        if self.entries.len() > self.max_entries {
            self.entries.remove(0);
        }
        self.position = self.entries.len();
    }

    fn undo(&mut self) -> Option<&UndoEntry> {
        if self.position > 0 {
            self.position -= 1;
            self.entries.get(self.position)
        } else {
            None
        }
    }

    fn redo(&mut self) -> Option<&UndoEntry> {
        if self.position < self.entries.len() {
            let entry = self.entries.get(self.position);
            self.position += 1;
            entry
        } else {
            None
        }
    }
}

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
        Self {
            start: pos,
            end: pos,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
}

pub struct TextArea {
    lines: Vec<String>,
    cursor: Cursor,
    selection: Selection,
    focused: bool,
    language: Option<String>,
    code_editor: bool,
    read_only: bool,
    show_line_numbers: bool,
    indent_width: usize,
    soft_wrap: bool,
    placeholder: String,
    scroll_row: usize,
    scroll_col: usize, // cell offset
    layout_w: u16,
    layout_h: u16,
    layout_initialized: bool,
    preferred_col_cells: Option<usize>,
    mouse_down: bool,
    app_active: bool,
    cursor_visible: bool,
    cursor_blink_next_at: Option<Instant>,
    cursor_blink_enabled: bool,
    doc_revision: u64,
    undo_stack: UndoStack,
    syntax_cache: std::sync::Mutex<SyntaxCache>,
    languages: HashMap<String, LanguageDef>,
    themes: HashMap<String, TextAreaTheme>,
    theme: Option<String>,
    classes: Vec<String>,
    focused_classes: Vec<String>,
    styles: WidgetStyles,
}

impl TextArea {
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
        let mut out = Self {
            lines: split_lines(text.into()),
            cursor: Cursor::default(),
            selection: Selection::cursor(Cursor::default()),
            focused: false,
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
            preferred_col_cells: None,
            mouse_down: false,
            app_active: true,
            cursor_visible: false,
            cursor_blink_next_at: None,
            cursor_blink_enabled: true,
            doc_revision: 0,
            undo_stack: UndoStack::new(100),
            syntax_cache: std::sync::Mutex::new(SyntaxCache::default()),
            languages: HashMap::new(),
            themes: HashMap::new(),
            theme: None,
            classes: Vec::new(),
            focused_classes: Vec::new(),
            styles: WidgetStyles::default(),
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

    pub fn set_read_only(&mut self, read_only: bool) {
        self.read_only = read_only;
        self.rebuild_classes();
    }

    pub fn with_show_line_numbers(mut self, show: bool) -> Self {
        self.show_line_numbers = show;
        self
    }

    pub fn show_line_numbers(&self) -> bool {
        self.show_line_numbers
    }

    pub fn set_show_line_numbers(&mut self, show: bool) {
        self.show_line_numbers = show;
    }

    pub fn with_indent_width(mut self, width: usize) -> Self {
        self.indent_width = width;
        self
    }

    pub fn indent_width(&self) -> usize {
        self.indent_width
    }

    pub fn set_indent_width(&mut self, width: usize) {
        self.indent_width = width;
    }

    pub fn with_soft_wrap(mut self, wrap: bool) -> Self {
        self.soft_wrap = wrap;
        self
    }

    pub fn soft_wrap(&self) -> bool {
        self.soft_wrap
    }

    pub fn set_soft_wrap(&mut self, wrap: bool) {
        self.soft_wrap = wrap;
    }

    pub fn with_placeholder(mut self, text: impl Into<String>) -> Self {
        self.placeholder = text.into();
        self
    }

    pub fn placeholder(&self) -> &str {
        &self.placeholder
    }

    pub fn set_placeholder(&mut self, text: impl Into<String>) {
        self.placeholder = text.into();
    }

    pub fn with_language(mut self, language: impl Into<String>) -> Self {
        self.set_language(language);
        self
    }

    pub fn language(&self) -> Option<&str> {
        self.language.as_deref()
    }

    pub fn set_language(&mut self, language: impl Into<String>) {
        self.language = Some(language.into());
        if let Ok(mut cache) = self.syntax_cache.lock() {
            cache.revision = u64::MAX;
        }
    }

    pub fn cursor_blink(&self) -> bool {
        self.cursor_blink_enabled
    }

    pub fn set_cursor_blink(&mut self, enabled: bool) {
        self.cursor_blink_enabled = enabled;
        if self.focused && self.app_active {
            self.reset_blink();
        } else {
            self.cursor_visible = false;
            self.cursor_blink_next_at = None;
        }
    }

    pub fn with_cursor_blink(mut self, enabled: bool) -> Self {
        self.set_cursor_blink(enabled);
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

    pub fn set_theme(&mut self, name: impl Into<String>) {
        self.theme = Some(name.into());
        if let Ok(mut cache) = self.syntax_cache.lock() {
            cache.revision = u64::MAX;
        }
    }

    pub fn with_theme(mut self, name: impl Into<String>) -> Self {
        self.set_theme(name);
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

    pub fn insert(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.insert_str(text);
        self.preferred_col_cells = Some(self.cursor_cell_x());
        self.adjust_scroll_to_cursor();
        self.reset_blink();
    }

    pub fn move_cursor_relative(&mut self, columns: isize, rows: isize) {
        if self.lines.is_empty() {
            return;
        }
        let row = (self.cursor.row as isize + rows)
            .clamp(0, self.lines.len().saturating_sub(1) as isize) as usize;
        let cur_cells = self.cursor_cell_x() as isize;
        let target_cells = (cur_cells + columns).max(0) as usize;
        let col = self.cursor_from_cell_x(row, target_cells);
        self.cursor = Cursor { row, col };
        self.selection = Selection::cursor(self.cursor);
        self.preferred_col_cells = Some(self.cursor_cell_x());
        self.adjust_scroll_to_cursor();
        self.reset_blink();
    }

    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    fn post_changed(&self, ctx: &mut EventCtx) {
        ctx.post_message(Message::TextAreaChanged { value: self.text() });
    }

    fn post_selection_changed(&self, ctx: &mut EventCtx) {
        let (a, b) = normalized_selection(self.selection);
        ctx.post_message(Message::TextAreaSelectionChanged {
            start: (a.row, a.col),
            end: (b.row, b.col),
        });
    }

    fn save_undo_checkpoint(&mut self) {
        self.undo_stack.push(UndoEntry {
            lines: self.lines.clone(),
            cursor: self.cursor,
            selection: self.selection,
        });
    }

    fn undo(&mut self) -> bool {
        if let Some(entry) = self.undo_stack.undo() {
            self.lines = entry.lines.clone();
            self.cursor = entry.cursor;
            self.selection = entry.selection;
            self.doc_revision = self.doc_revision.wrapping_add(1);
            true
        } else {
            false
        }
    }

    fn redo(&mut self) -> bool {
        if let Some(entry) = self.undo_stack.redo() {
            self.lines = entry.lines.clone();
            self.cursor = entry.cursor;
            self.selection = entry.selection;
            self.doc_revision = self.doc_revision.wrapping_add(1);
            true
        } else {
            false
        }
    }

    fn delete_to_start_of_line(&mut self) {
        if self.delete_selection_if_any() {
            return;
        }
        if self.cursor.col > 0 {
            let row = self.cursor.row;
            self.lines[row].drain(..self.cursor.col);
            self.cursor.col = 0;
            self.selection = Selection::cursor(self.cursor);
            self.doc_revision = self.doc_revision.wrapping_add(1);
        }
    }

    fn delete_to_end_of_line(&mut self) {
        if self.delete_selection_if_any() {
            return;
        }
        let row = self.cursor.row;
        let line_len = self.lines[row].len();
        if self.cursor.col < line_len {
            self.lines[row].truncate(self.cursor.col);
            self.selection = Selection::cursor(self.cursor);
            self.doc_revision = self.doc_revision.wrapping_add(1);
        }
    }

    fn delete_current_line(&mut self) {
        if self.lines.is_empty() {
            return;
        }
        let row = self.cursor.row.min(self.lines.len().saturating_sub(1));
        if self.lines.len() > 1 {
            self.lines.remove(row);
        } else {
            self.lines[0].clear();
        }
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
        self.cursor.row = self.cursor.row.min(self.lines.len().saturating_sub(1));
        self.cursor.col = self.cursor.col.min(self.lines[self.cursor.row].len());
        self.selection = Selection::cursor(self.cursor);
        self.doc_revision = self.doc_revision.wrapping_add(1);
    }

    fn select_all(&mut self) -> bool {
        if self.lines.is_empty() || (self.lines.len() == 1 && self.lines[0].is_empty()) {
            return false;
        }
        let last_row = self.lines.len().saturating_sub(1);
        let last_col = self.lines[last_row].len();
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
        if row >= self.lines.len() {
            return false;
        }
        let line_len = self.lines[row].len();
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
        let mut offsets = Vec::with_capacity(self.lines.len().max(1));
        let mut cur = 0usize;
        for (i, line) in self.lines.iter().enumerate() {
            offsets.push(cur);
            cur = cur.saturating_add(line.len());
            // Join adds '\n' between lines, but not after the last one.
            if i + 1 < self.lines.len() {
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

        let text = self.text();
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
        self.cursor.col = clamp_grapheme_boundary(&self.lines[self.cursor.row], self.cursor.col);
        self.selection = Selection::cursor(self.cursor);
    }

    fn clamp_cursor_pos(&self, cursor: Cursor) -> Cursor {
        if self.lines.is_empty() {
            return Cursor::default();
        }
        let row = cursor.row.min(self.lines.len().saturating_sub(1));
        let line = self.lines.get(row).map(String::as_str).unwrap_or("");
        let mut col = cursor.col.min(line.len());
        col = clamp_grapheme_boundary(line, col);
        Cursor { row, col }
    }

    fn line_number_gutter_width(&self) -> usize {
        if !self.show_line_numbers {
            return 0;
        }
        let digits = self.lines.len().max(1).to_string().len();
        // Right aligned number + trailing space.
        digits + 1
    }

    fn cursor_cell_x(&self) -> usize {
        let line = self
            .lines
            .get(self.cursor.row)
            .map(String::as_str)
            .unwrap_or("");
        grapheme_cell_len_prefix(line, self.cursor.col)
    }

    fn cursor_from_cell_x(&self, row: usize, cell_x: usize) -> usize {
        let line = self.lines.get(row).map(String::as_str).unwrap_or("");
        grapheme_byte_index_from_cell_x(line, cell_x)
    }

    fn cursor_left_pos(&self, from: Cursor) -> Cursor {
        if self.lines.is_empty() {
            return from;
        }
        let row = from.row.min(self.lines.len().saturating_sub(1));
        if from.col > 0 {
            let line = &self.lines[row];
            let col = prev_grapheme_boundary(line, from.col);
            Cursor { row, col }
        } else if row > 0 {
            let row = row - 1;
            let col = self.lines[row].len();
            Cursor { row, col }
        } else {
            Cursor { row, col: 0 }
        }
    }

    fn cursor_right_pos(&self, from: Cursor) -> Cursor {
        if self.lines.is_empty() {
            return from;
        }
        let row = from.row.min(self.lines.len().saturating_sub(1));
        let line_len = self.lines[row].len();
        if from.col < line_len {
            let next = next_grapheme_boundary(&self.lines[row], from.col);
            Cursor { row, col: next }
        } else if row + 1 < self.lines.len() {
            Cursor {
                row: row + 1,
                col: 0,
            }
        } else {
            Cursor { row, col: line_len }
        }
    }

    fn ensure_cursor_visible(&mut self, view_height: usize, view_width: usize) {
        self.scroll_row = self.scroll_row.min(self.cursor.row);
        if self.cursor.row >= self.scroll_row + view_height {
            self.scroll_row = self
                .cursor
                .row
                .saturating_sub(view_height.saturating_sub(1));
        }

        let cur_x = self.cursor_cell_x();
        self.scroll_col = self.scroll_col.min(cur_x);
        if cur_x >= self.scroll_col + view_width {
            self.scroll_col = cur_x.saturating_sub(view_width.saturating_sub(1));
        }
    }

    fn adjust_scroll_to_cursor(&mut self) {
        if !self.layout_initialized {
            return;
        }
        if !self.selection.is_empty() {
            let (a, _b) = normalized_selection(self.selection);
            self.scroll_row = self.scroll_row.min(a.row);
        }
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
        if self.cursor_blink_enabled {
            self.cursor_blink_next_at = Some(Self::next_blink_deadline());
        } else {
            self.cursor_blink_next_at = None;
        }
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
        self.doc_revision = self.doc_revision.wrapping_add(1);
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
        self.cursor.col = clamp_grapheme_boundary(line, self.cursor.col);
        self.selection = Selection::cursor(self.cursor);
        self.doc_revision = self.doc_revision.wrapping_add(1);
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
        self.doc_revision = self.doc_revision.wrapping_add(1);
    }

    fn backspace(&mut self) {
        if self.delete_selection_if_any() {
            return;
        }
        if self.cursor.col > 0 {
            let row = self.cursor.row;
            let line = &mut self.lines[row];
            let prev = prev_grapheme_boundary(line, self.cursor.col);
            line.drain(prev..self.cursor.col);
            self.cursor.col = prev;
            self.selection = Selection::cursor(self.cursor);
            self.doc_revision = self.doc_revision.wrapping_add(1);
        } else if self.cursor.row > 0 {
            let row = self.cursor.row;
            let prev_row = row - 1;
            let prev_len = self.lines[prev_row].len();
            let current = self.lines.remove(row);
            self.lines[prev_row].push_str(&current);
            self.cursor.row = prev_row;
            self.cursor.col = prev_len;
            self.selection = Selection::cursor(self.cursor);
            self.doc_revision = self.doc_revision.wrapping_add(1);
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
            let next = next_grapheme_boundary(&self.lines[row], col);
            self.lines[row].drain(col..next);
            self.doc_revision = self.doc_revision.wrapping_add(1);
        } else if row + 1 < self.lines.len() {
            let next_line = self.lines.remove(row + 1);
            self.lines[row].push_str(&next_line);
            self.doc_revision = self.doc_revision.wrapping_add(1);
        }
        self.selection = Selection::cursor(self.cursor);
    }

    fn cursor_word_left_pos(&self, from: Cursor) -> Cursor {
        if self.lines.is_empty() {
            return from;
        }
        let row = from.row.min(self.lines.len().saturating_sub(1));
        if from.col > 0 {
            let line = &self.lines[row];
            let col = prev_word_boundary(line, from.col);
            Cursor { row, col }
        } else if row > 0 {
            let row = row - 1;
            let col = prev_word_boundary(&self.lines[row], self.lines[row].len());
            Cursor { row, col }
        } else {
            Cursor { row: 0, col: 0 }
        }
    }

    fn cursor_word_right_pos(&self, from: Cursor) -> Cursor {
        if self.lines.is_empty() {
            return from;
        }
        let row = from.row.min(self.lines.len().saturating_sub(1));
        let line = &self.lines[row];
        if from.col < line.len() {
            let col = next_word_boundary(line, from.col);
            Cursor { row, col }
        } else if row + 1 < self.lines.len() {
            let row = row + 1;
            let col = next_word_boundary(&self.lines[row], 0);
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
        if start.row == self.cursor.row {
            self.lines[self.cursor.row].drain(start.col..self.cursor.col);
        } else {
            let right = self.lines[self.cursor.row][self.cursor.col..].to_string();
            self.lines[start.row].truncate(start.col);
            self.lines[start.row].push_str(&right);
            self.lines.drain(start.row + 1..=self.cursor.row);
        }
        self.cursor = start;
        self.selection = Selection::cursor(self.cursor);
        self.doc_revision = self.doc_revision.wrapping_add(1);
    }

    fn delete_word(&mut self) {
        if self.delete_selection_if_any() {
            return;
        }
        let end = self.cursor_word_right_pos(self.cursor);
        if end == self.cursor {
            return;
        }
        if end.row == self.cursor.row {
            self.lines[self.cursor.row].drain(self.cursor.col..end.col);
        } else {
            let suffix = self.lines[end.row][end.col..].to_string();
            self.lines[self.cursor.row].truncate(self.cursor.col);
            self.lines[self.cursor.row].push_str(&suffix);
            self.lines.drain(self.cursor.row + 1..=end.row);
        }
        self.selection = Selection::cursor(self.cursor);
        self.doc_revision = self.doc_revision.wrapping_add(1);
    }

    fn selected_text(&self) -> Option<String> {
        if self.selection.is_empty() {
            return None;
        }
        let (a, b) = normalized_selection(self.selection);
        if a.row == b.row {
            return Some(self.lines[a.row][a.col..b.col].to_string());
        }
        let mut out = String::new();
        out.push_str(&self.lines[a.row][a.col..]);
        out.push('\n');
        for row in a.row + 1..b.row {
            out.push_str(&self.lines[row]);
            out.push('\n');
        }
        out.push_str(&self.lines[b.row][..b.col]);
        Some(out)
    }

    fn cut_current_line(&mut self) -> Option<String> {
        if self.lines.is_empty() {
            return None;
        }
        let row = self.cursor.row.min(self.lines.len().saturating_sub(1));
        let mut copied = self.lines[row].clone();
        if self.lines.len() > 1 {
            self.lines.remove(row);
            if row < self.lines.len() {
                copied.push('\n');
            }
        } else {
            self.lines[0].clear();
        }
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
        self.cursor.row = self.cursor.row.min(self.lines.len().saturating_sub(1));
        self.cursor.col = self.cursor.col.min(self.lines[self.cursor.row].len());
        self.selection = Selection::cursor(self.cursor);
        self.doc_revision = self.doc_revision.wrapping_add(1);
        Some(copied)
    }

    fn insert_clipboard_text(&mut self, text: &str) -> bool {
        if text.is_empty() {
            return false;
        }
        let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
        if normalized.is_empty() {
            return false;
        }
        if self.delete_selection_if_any() {
            // Insert replacement text at cursor.
        }
        let mut parts = normalized.split('\n').peekable();
        while let Some(part) = parts.next() {
            if !part.is_empty() {
                self.insert_str(part);
            }
            if parts.peek().is_some() {
                self.insert_newline();
            }
        }
        true
    }
}

impl Widget for TextArea {
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
            // TODO(P1-14 integration): wire tree-based NodeId comparison
            Event::MouseDown(mouse) if mouse.target == NodeId::default() => {
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
            Event::Key(key) if self.focused => {
                if !self.read_only && matches!(key.code, KeyCode::Char('\u{7f}' | '\u{08}')) {
                    self.save_undo_checkpoint();
                    self.backspace();
                    self.post_changed(ctx);
                    self.preferred_col_cells = Some(self.cursor_cell_x());
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
                let mut next_preferred = self.preferred_col_cells;

                // Save undo checkpoint before mutations.
                if is_mutation {
                    self.save_undo_checkpoint();
                }

                match cmd {
                    EditCommand::InsertChar(ch) => {
                        if ch != '\t' {
                            self.insert_str(&ch.to_string());
                            changed = true;
                            value_changed = true;
                            next_preferred = Some(self.cursor_cell_x());
                        }
                    }
                    EditCommand::InsertNewline => {
                        self.insert_newline();
                        changed = true;
                        value_changed = true;
                        next_preferred = Some(0);
                    }
                    EditCommand::Backspace { unit } => {
                        let before = self.text();
                        match unit {
                            MoveUnit::Grapheme => self.backspace(),
                            MoveUnit::Word => self.backspace_word(),
                        }
                        changed = before != self.text();
                        value_changed = changed;
                        next_preferred = Some(self.cursor_cell_x());
                    }
                    EditCommand::Delete { unit } => {
                        let before = self.text();
                        match unit {
                            MoveUnit::Grapheme => self.delete(),
                            MoveUnit::Word => self.delete_word(),
                        }
                        changed = before != self.text();
                        value_changed = changed;
                        next_preferred = Some(self.cursor_cell_x());
                    }
                    EditCommand::DeleteToStart => {
                        let before = self.text();
                        self.delete_to_start_of_line();
                        changed = before != self.text();
                        value_changed = changed;
                        next_preferred = Some(self.cursor_cell_x());
                    }
                    EditCommand::DeleteToEnd => {
                        let before = self.text();
                        self.delete_to_end_of_line();
                        changed = before != self.text();
                        value_changed = changed;
                        next_preferred = Some(self.cursor_cell_x());
                    }
                    EditCommand::DeleteLine => {
                        self.delete_current_line();
                        changed = true;
                        value_changed = true;
                        next_preferred = Some(self.cursor_cell_x());
                    }
                    EditCommand::SelectAll => {
                        changed = self.select_all();
                        next_preferred = Some(self.cursor_cell_x());
                    }
                    EditCommand::SelectLine => {
                        changed = self.select_line();
                        next_preferred = Some(self.cursor_cell_x());
                    }
                    EditCommand::Undo => {
                        if self.undo() {
                            changed = true;
                            value_changed = true;
                            next_preferred = Some(self.cursor_cell_x());
                        }
                    }
                    EditCommand::Redo => {
                        if self.redo() {
                            changed = true;
                            value_changed = true;
                            next_preferred = Some(self.cursor_cell_x());
                        }
                    }
                    EditCommand::MoveLeft { select, unit } => {
                        let next = match unit {
                            MoveUnit::Grapheme => self.cursor_left_pos(self.cursor),
                            MoveUnit::Word => self.cursor_word_left_pos(self.cursor),
                        };
                        changed = self.move_cursor_with_selection(next, select);
                        next_preferred = Some(self.cursor_cell_x());
                    }
                    EditCommand::MoveRight { select, unit } => {
                        let next = match unit {
                            MoveUnit::Grapheme => self.cursor_right_pos(self.cursor),
                            MoveUnit::Word => self.cursor_word_right_pos(self.cursor),
                        };
                        changed = self.move_cursor_with_selection(next, select);
                        next_preferred = Some(self.cursor_cell_x());
                    }
                    EditCommand::MoveUp { select } => {
                        if self.cursor.row > 0 {
                            let desired = self
                                .preferred_col_cells
                                .unwrap_or_else(|| self.cursor_cell_x());
                            let row = self.cursor.row - 1;
                            let col = self.cursor_from_cell_x(row, desired);
                            changed = self.move_cursor_with_selection(Cursor { row, col }, select);
                            next_preferred = Some(desired);
                        }
                    }
                    EditCommand::MoveDown { select } => {
                        if self.cursor.row + 1 < self.lines.len() {
                            let desired = self
                                .preferred_col_cells
                                .unwrap_or_else(|| self.cursor_cell_x());
                            let row = self.cursor.row + 1;
                            let col = self.cursor_from_cell_x(row, desired);
                            changed = self.move_cursor_with_selection(Cursor { row, col }, select);
                            next_preferred = Some(desired);
                        }
                    }
                    EditCommand::MoveHome { select } => {
                        changed = self.move_cursor_with_selection(
                            Cursor {
                                row: self.cursor.row,
                                col: 0,
                            },
                            select,
                        );
                        next_preferred = Some(0);
                    }
                    EditCommand::MoveEnd { select } => {
                        let end = self.lines[self.cursor.row].len();
                        changed = self.move_cursor_with_selection(
                            Cursor {
                                row: self.cursor.row,
                                col: end,
                            },
                            select,
                        );
                        next_preferred = Some(self.cursor_cell_x());
                    }
                    EditCommand::Copy => {
                        if let Some(text) = self.selected_text() {
                            ctx.post_message(Message::TextEditClipboardCopyRequested {
                                text,
                                cut: false,
                            });
                        }
                    }
                    EditCommand::Cut => {
                        if let Some(text) = self.selected_text() {
                            ctx.post_message(Message::TextEditClipboardCopyRequested {
                                text,
                                cut: true,
                            });
                            if self.delete_selection_if_any() {
                                changed = true;
                                value_changed = true;
                            }
                        } else if let Some(text) = self.cut_current_line() {
                            ctx.post_message(Message::TextEditClipboardCopyRequested {
                                text,
                                cut: true,
                            });
                            changed = true;
                            value_changed = true;
                            next_preferred = Some(self.cursor_cell_x());
                        }
                    }
                    EditCommand::Paste => {
                        // TODO(P1-14 integration): wire tree-based NodeId comparison
                        ctx.post_message(Message::TextEditClipboardPasteRequested {
                            target: NodeId::default(),
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
                    self.preferred_col_cells = next_preferred;
                    self.adjust_scroll_to_cursor();
                    self.reset_blink();
                    ctx.request_repaint();
                }
                ctx.set_handled();
            }
            _ => {}
        }
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut EventCtx) {
        if let Message::TextEditClipboardPaste { target, text } = &message.message {
            // TODO(P1-14 integration): wire tree-based NodeId comparison
            if *target != NodeId::default() {
                return;
            }
            if self.read_only {
                return;
            }
            self.save_undo_checkpoint();
            if self.insert_clipboard_text(text) {
                self.post_changed(ctx);
                self.preferred_col_cells = Some(self.cursor_cell_x());
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
        self.layout_initialized = true;
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
            let meta = crate::css::selector_meta_component(self.style_type(), &[class]);
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
        let is_empty = self.lines.len() == 1 && self.lines[0].is_empty();
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

        let (sel_a, sel_b) = normalized_selection(self.selection);

        let mut out = Segments::new();
        for y in 0..height {
            let row = self.scroll_row + y;
            let is_cursor_line = self.focused && self.app_active && row == self.cursor.row;
            let line_bg_style = if is_cursor_line {
                Some(cursor_line_style.clone())
            } else {
                None
            };
            if gutter_w > 0 {
                let gutter_text = if row < self.lines.len() {
                    let line_no = row.saturating_add(1);
                    let digits = gutter_w.saturating_sub(1).max(1);
                    format!("{line_no:>digits$} ")
                } else {
                    " ".repeat(gutter_w)
                };
                let style = if self.focused && row == self.cursor.row {
                    gutter_active_rich
                } else {
                    gutter_rich
                };
                out.push(Segment::styled(
                    rich_rs::set_cell_size(&gutter_text, gutter_w),
                    style,
                ));
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
            for (byte_idx, grapheme) in line.grapheme_indices(true) {
                idx = byte_idx;
                let w = grapheme_width(grapheme);
                let ch_cell_start = grapheme_cell_len_prefix(line, byte_idx);
                let ch_cell_end = ch_cell_start + w;

                if ch_cell_end <= start_cell {
                    continue;
                }
                if cell_x >= text_w {
                    break;
                }

                let is_cursor = self.focused
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
            let _ = idx;
            flush(&mut out, &mut pending_style, &mut pending_text);

            // Cursor at end of line: paint a single cell with cursor style.
            if self.focused && self.cursor_visible && row == self.cursor.row {
                let end_cell = grapheme_cell_len_prefix(line, line.len());
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

fn compose_rich(style: &Style, base_bg: Color) -> rich_rs::Style {
    let mut rich = style
        .to_rich_without_colors()
        .unwrap_or_else(rich_rs::Style::new);
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
    use crate::keys::KeyEventData;
    use crate::message::{Message, MessageEvent};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn typing_emits_text_area_changed_message() {
        let mut text_area = TextArea::new("");
        text_area.set_focus(true);
        let mut ctx = EventCtx::default();

        text_area.on_event(
            &Event::Key(KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Char('x'),
                KeyModifiers::NONE,
            ))),
            &mut ctx,
        );

        let messages = ctx.take_messages();
        assert!(
            messages.iter().any(
                |m| matches!(m.message, Message::TextAreaChanged { ref value } if value == "x")
            )
        );
    }

    #[test]
    fn clipboard_commands_emit_messages() {
        let mut text_area = TextArea::new("hello\nworld");
        text_area.set_focus(true);
        text_area.set_selection(Selection {
            start: Cursor { row: 0, col: 0 },
            end: Cursor { row: 0, col: 5 },
        });

        let mut ctx = EventCtx::default();
        text_area.on_event(
            &Event::Key(KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Char('c'),
                KeyModifiers::CONTROL,
            ))),
            &mut ctx,
        );
        let copy_messages = ctx.take_messages();
        assert!(copy_messages.iter().any(|m| {
            matches!(
                m.message,
                Message::TextEditClipboardCopyRequested { ref text, cut: false } if text == "hello"
            )
        }));

        let mut ctx = EventCtx::default();
        text_area.on_event(
            &Event::Key(KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Char('v'),
                KeyModifiers::CONTROL,
            ))),
            &mut ctx,
        );
        let paste_messages = ctx.take_messages();
        assert!(paste_messages.iter().any(|m| {
            matches!(
                m.message,
                Message::TextEditClipboardPasteRequested { target } if target == NodeId::default()
            )
        }));
    }

    #[test]
    fn paste_message_inserts_multiline_text() {
        let mut text_area = TextArea::new("abc");
        text_area.set_focus(true);
        text_area.set_selection(Selection::cursor(Cursor { row: 0, col: 1 }));

        let mut ctx = EventCtx::default();
        text_area.on_message(
            &MessageEvent {
                sender: NodeId::default(),
                message: Message::TextEditClipboardPaste {
                    target: NodeId::default(),
                    text: "X\nY".to_string(),
                },
            },
            &mut ctx,
        );

        assert_eq!(text_area.text(), "aX\nYbc");
        assert!(ctx.handled());
    }
}
