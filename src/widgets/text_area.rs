use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyModifiers;
use rich_rs::{Console, ConsoleOptions, Segment, Segments};
use tree_sitter::{Parser, Query, QueryCursor};
use unicode_width::UnicodeWidthChar;

use crate::event::{Event, EventCtx};
use crate::style::{Color, Style, parse_color_like};
use crate::{Error, Result};

use super::{
    Widget, WidgetId, WidgetStyles,
    helpers::{empty_classes, fixed_height_from_constraints},
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
    layout_initialized: bool,
    preferred_col_cells: Option<usize>,
    mouse_down: bool,
    app_active: bool,
    cursor_visible: bool,
    cursor_blink_next_at: Option<Instant>,
    cursor_blink_enabled: bool,
    doc_revision: u64,
    syntax_cache: std::sync::Mutex<SyntaxCache>,
    languages: HashMap<String, LanguageDef>,
    themes: HashMap<String, TextAreaTheme>,
    theme: Option<String>,
    classes: Vec<String>,
    focused_classes: Vec<String>,
    styles: WidgetStyles,
    on_change: Option<Arc<dyn Fn(&mut TextArea) + Send + Sync>>,
    on_key: Option<Arc<dyn Fn(&mut TextArea, KeyEvent, &mut EventCtx) + Send + Sync>>,
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
            layout_initialized: false,
            preferred_col_cells: None,
            mouse_down: false,
            app_active: true,
            cursor_visible: false,
            cursor_blink_next_at: None,
            cursor_blink_enabled: true,
            doc_revision: 0,
            syntax_cache: std::sync::Mutex::new(SyntaxCache::default()),
            languages: HashMap::new(),
            themes: HashMap::new(),
            theme: None,
            classes: Vec::new(),
            focused_classes: Vec::new(),
            styles: WidgetStyles::default(),
            on_change: None,
            on_key: None,
        };
        out.register_builtin_languages();
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

    pub fn on_key(
        mut self,
        handler: impl Fn(&mut TextArea, KeyEvent, &mut EventCtx) + Send + Sync + 'static,
    ) -> Self {
        self.on_key = Some(Arc::new(handler));
        self
    }

    pub fn insert(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.insert_str(text);
        self.notify_changed();
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

    pub fn on_change(mut self, handler: impl Fn(&mut TextArea) + Send + Sync + 'static) -> Self {
        self.on_change = Some(Arc::new(handler));
        self
    }

    fn notify_changed(&mut self) {
        if let Some(handler) = self.on_change.clone() {
            handler(self);
        }
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
        let line = self
            .lines
            .get(self.cursor.row)
            .map(String::as_str)
            .unwrap_or("");
        cell_len_prefix(line, self.cursor.col)
    }

    fn cursor_from_cell_x(&self, row: usize, cell_x: usize) -> usize {
        let line = self.lines.get(row).map(String::as_str).unwrap_or("");
        byte_index_from_cell_x(line, cell_x)
    }

    fn cursor_left_pos(&self, from: Cursor) -> Cursor {
        if self.lines.is_empty() {
            return from;
        }
        let row = from.row.min(self.lines.len().saturating_sub(1));
        if from.col > 0 {
            let line = &self.lines[row];
            let col = prev_char_boundary(line, from.col);
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
            let next = next_char_boundary(&self.lines[row], from.col);
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
        self.cursor.col = prev_char_boundary(line, self.cursor.col);
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
            let prev = prev_char_boundary(line, self.cursor.col);
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
            let next = next_char_boundary(&self.lines[row], col);
            self.lines[row].drain(col..next);
            self.doc_revision = self.doc_revision.wrapping_add(1);
        } else if row + 1 < self.lines.len() {
            let next_line = self.lines.remove(row + 1);
            self.lines[row].push_str(&next_line);
            self.doc_revision = self.doc_revision.wrapping_add(1);
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
        let desired = self
            .preferred_col_cells
            .unwrap_or_else(|| self.cursor_cell_x());
        self.cursor.row -= 1;
        let new_col = self.cursor_from_cell_x(self.cursor.row, desired);
        self.cursor.col = new_col;
        self.selection = Selection::cursor(self.cursor);
    }

    fn move_down(&mut self) {
        if self.cursor.row + 1 >= self.lines.len() {
            return;
        }
        let desired = self
            .preferred_col_cells
            .unwrap_or_else(|| self.cursor_cell_x());
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
            Event::Key(key) if self.focused => {
                if let Some(handler) = self.on_key.clone() {
                    handler(self, *key, ctx);
                    if ctx.handled() {
                        return;
                    }
                }

                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    let old = self.cursor;
                    let mut next = old;
                    let mut desired_vertical: Option<usize> = None;
                    match key.code {
                        KeyCode::Left => {
                            next = self.cursor_left_pos(old);
                        }
                        KeyCode::Right => {
                            next = self.cursor_right_pos(old);
                        }
                        KeyCode::Up => {
                            if old.row > 0 {
                                let desired = self
                                    .preferred_col_cells
                                    .unwrap_or_else(|| self.cursor_cell_x());
                                let row = old.row - 1;
                                let col = self.cursor_from_cell_x(row, desired);
                                next = Cursor { row, col };
                                desired_vertical = Some(desired);
                            }
                        }
                        KeyCode::Down => {
                            if old.row + 1 < self.lines.len() {
                                let desired = self
                                    .preferred_col_cells
                                    .unwrap_or_else(|| self.cursor_cell_x());
                                let row = old.row + 1;
                                let col = self.cursor_from_cell_x(row, desired);
                                next = Cursor { row, col };
                                desired_vertical = Some(desired);
                            }
                        }
                        KeyCode::Home => {
                            next = Cursor {
                                row: old.row,
                                col: 0,
                            };
                            self.preferred_col_cells = Some(0);
                        }
                        KeyCode::End => {
                            let end = self.lines.get(old.row).map(|s| s.len()).unwrap_or(0);
                            next = Cursor {
                                row: old.row,
                                col: end,
                            };
                        }
                        _ => {}
                    }

                    if next != old {
                        if self.selection.is_empty() {
                            self.selection = Selection {
                                start: old,
                                end: next,
                            };
                        } else {
                            self.selection.end = next;
                        }
                        self.cursor = next;
                        if matches!(key.code, KeyCode::Up | KeyCode::Down) {
                            if let Some(desired) = desired_vertical {
                                self.preferred_col_cells = Some(desired);
                            }
                        } else if matches!(key.code, KeyCode::Home) {
                            self.preferred_col_cells = Some(0);
                        } else {
                            self.preferred_col_cells = Some(self.cursor_cell_x());
                        }
                        self.adjust_scroll_to_cursor();
                        self.reset_blink();
                        ctx.request_repaint();
                        ctx.set_handled();
                        return;
                    }
                }
                match key.code {
                    KeyCode::Char(ch) => {
                        // Some terminals encode Backspace as a control character rather than
                        // `KeyCode::Backspace` (notably DEL `\u{7f}` and BS `\u{08}`).
                        if ch == '\u{7f}' || ch == '\u{08}' {
                            self.backspace();
                            self.notify_changed();
                            self.preferred_col_cells = Some(self.cursor_cell_x());
                            self.adjust_scroll_to_cursor();
                            self.reset_blink();
                            ctx.request_repaint();
                            ctx.set_handled();
                            return;
                        }
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
                }
            }
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
                    "text-area--cursor" => theme.cursor_style,
                    "text-area--cursor-line" => theme.cursor_line_style,
                    "text-area--selection" => theme.selection_style,
                    "text-area--gutter" => theme.gutter_style,
                    "text-area--gutter-active" => theme.gutter_active_style,
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

        let cursor_rich = compose_rich(&cursor_style, base_bg);
        let selection_rich = compose_rich(&selection_style, base_bg);
        let gutter_rich = compose_rich(&gutter_style, base_bg);
        let gutter_active_rich = compose_rich(&gutter_active_style, base_bg);

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
                Some(cursor_line_style)
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
                    Some(cursor_rich)
                } else if in_sel {
                    Some(selection_rich)
                } else {
                    let abs = line_abs_offset.saturating_add(byte_idx);
                    let mut syntax: Option<Style> = None;
                    for span in &syntax_cache.spans {
                        if abs >= span.start && abs < span.end {
                            syntax = Some(span.style);
                        }
                    }
                    let mut merged = syntax.unwrap_or_default();
                    if let Some(bg) = line_bg_style {
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

                pending_text.push(visible_ch);
                cell_x += w;
            }
            let _ = idx;
            flush(&mut out, &mut pending_style, &mut pending_text);

            // Cursor at end of line: paint a single cell with cursor style.
            if self.focused && self.cursor_visible && row == self.cursor.row {
                let end_cell = cell_len_prefix(line, line.len());
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
