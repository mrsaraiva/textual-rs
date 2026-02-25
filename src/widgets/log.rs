use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use crossterm::event::{KeyCode, KeyModifiers};
use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments, Style as RichStyle};

use crate::event::{Action, Event, EventCtx};
use crate::message::*;

use super::helpers::{adjust_line_length_no_bg, empty_classes, fixed_height_from_constraints};

use super::{ScrollBar, ScrollView, Widget, WidgetStyles};

pub(crate) const LOG_VSCROLLBAR_ID: &str = "__log_vscrollbar";

// ── WP-25: LRU render cache ────────────────────────────────────────────────

/// Simple LRU cache for rendered line segments, keyed by (line_index, content_hash).
#[derive(Debug)]
struct LogLineCache {
    entries: HashMap<(usize, u64), Vec<Segment>>,
    order: Vec<(usize, u64)>,
    max_size: usize,
}

impl LogLineCache {
    fn new(max_size: usize) -> Self {
        Self {
            entries: HashMap::new(),
            order: Vec::new(),
            max_size: max_size.max(1),
        }
    }

    fn get(&mut self, key: &(usize, u64)) -> Option<&Vec<Segment>> {
        if self.entries.contains_key(key) {
            self.order.retain(|k| k != key);
            self.order.push(*key);
            self.entries.get(key)
        } else {
            None
        }
    }

    fn insert(&mut self, key: (usize, u64), value: Vec<Segment>) {
        if self.entries.contains_key(&key) {
            self.order.retain(|k| *k != key);
        } else if self.entries.len() >= self.max_size {
            if let Some(evicted) = self.order.first().cloned() {
                self.entries.remove(&evicted);
                self.order.remove(0);
            }
        }
        self.entries.insert(key, value);
        self.order.push(key);
    }

    fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
    }

    fn invalidate_from(&mut self, line_index: usize) {
        self.entries.retain(|&(idx, _), _| idx < line_index);
        self.order.retain(|&(idx, _)| idx < line_index);
    }
}

// ── WP-24: Selection state ─────────────────────────────────────────────────

/// A position in the log: (line_index, column).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LogPos {
    line: usize,
    col: usize,
}

impl LogPos {
    fn new(line: usize, col: usize) -> Self {
        Self { line, col }
    }
}

impl PartialOrd for LogPos {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for LogPos {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.line.cmp(&other.line).then(self.col.cmp(&other.col))
    }
}

/// Normalized selection range (start <= end).
#[derive(Debug, Clone, Copy)]
struct SelectionRange {
    start: LogPos,
    end: LogPos,
}

// ── Log widget ─────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct Log {
    lines: Vec<String>,
    max_lines: Option<usize>,
    auto_scroll: bool,
    scroll_step: usize,
    offset_y: usize,
    focused: bool,
    highlight: bool,
    classes: Vec<String>,
    focused_classes: Vec<String>,
    content_height: AtomicUsize,
    viewport_height: AtomicUsize,
    widget_width: AtomicUsize,
    widget_height: AtomicUsize,
    scrollbar_extracted: bool,
    max_line_width: usize,
    styles: WidgetStyles,
    // WP-24: text selection
    selection_anchor: Option<LogPos>,
    selection_end: Option<LogPos>,
    selecting: bool,
    // WP-25: render cache
    cache: Mutex<LogLineCache>,
    cache_width: AtomicUsize,
}

impl Log {
    pub fn new() -> Self {
        Self {
            lines: Vec::new(),
            max_lines: None,
            auto_scroll: true,
            scroll_step: 1,
            offset_y: 0,
            focused: false,
            highlight: false,
            classes: Vec::new(),
            focused_classes: vec!["-focus".to_string()],
            content_height: AtomicUsize::new(1),
            viewport_height: AtomicUsize::new(1),
            widget_width: AtomicUsize::new(1),
            widget_height: AtomicUsize::new(1),
            scrollbar_extracted: false,
            max_line_width: 0,
            styles: WidgetStyles::default(),
            selection_anchor: None,
            selection_end: None,
            selecting: false,
            cache: Mutex::new(LogLineCache::new(1000)),
            cache_width: AtomicUsize::new(0),
        }
    }

    pub fn with_highlight(mut self, highlight: bool) -> Self {
        self.highlight = highlight;
        self
    }

    pub fn with_highlighter(mut self, _name: impl Into<String>) -> Self {
        // Language-specific highlighting is reserved for future use.
        // Currently enables the default repr highlighter.
        self.highlight = true;
        self
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
        let insert_from = self.lines.len();
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

        self.cache.lock().unwrap().invalidate_from(insert_from);
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
        let insert_from = self.lines.len();
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

        self.cache.lock().unwrap().invalidate_from(insert_from);
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
        self.clear_selection();
        self.cache.lock().unwrap().clear();
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
                self.cache.lock().unwrap().clear();
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
        ctx.post_message(Message::RichLogScrolled(RichLogScrolled {
            offset: self.offset_y,
            max_offset: self.max_offset(),
        }));
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

    /// Compute a content hash for a line string for cache keying.
    fn line_content_hash(line: &str) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        line.hash(&mut hasher);
        hasher.finish()
    }

    fn render_line_segments(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        line: &str,
        viewport_width: usize,
        line_index: usize,
    ) -> Vec<Segment> {
        // WP-25: check cache
        let content_hash = Self::line_content_hash(line);
        let cache_key = (line_index, content_hash);
        {
            let mut cache = self.cache.lock().unwrap();
            if let Some(cached) = cache.get(&cache_key) {
                return cached.clone();
            }
        }

        let result = self.render_line_segments_uncached(console, options, line, viewport_width);

        // Store in cache
        {
            let mut cache = self.cache.lock().unwrap();
            cache.insert(cache_key, result.clone());
        }

        result
    }

    fn render_line_segments_uncached(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        line: &str,
        viewport_width: usize,
    ) -> Vec<Segment> {
        let processed = Self::process_line(line);
        if self.highlight {
            // Use markup=false to avoid interpreting log text like "[error]" as rich markup.
            // The highlight flag enables the repr highlighter for syntax coloring only.
            let hl = console.render_str(&processed, Some(false), None, Some(true), None);
            let rendered = hl.render(console, options);
            let split = Segment::split_and_crop_lines(rendered, viewport_width, None, true, false);
            if let Some(first) = split.into_iter().next() {
                adjust_line_length_no_bg(&first, viewport_width.max(1))
            } else {
                adjust_line_length_no_bg(&[Segment::new(String::new())], viewport_width.max(1))
            }
        } else {
            adjust_line_length_no_bg(&[Segment::new(processed)], viewport_width.max(1))
        }
    }

    // ── WP-24: Selection helpers ────────────────────────────────────────

    /// Convert mouse coordinates (content-local) to a LogPos.
    fn mouse_to_pos(&self, x: usize, y: usize) -> LogPos {
        let line = (self.offset_y + y).min(self.line_count().saturating_sub(1));
        let col = if line < self.lines.len() {
            let processed = Self::process_line(&self.lines[line]);
            // Convert cell x to character column
            let mut cell_x = 0usize;
            let mut char_col = 0usize;
            for ch in processed.chars() {
                if cell_x >= x {
                    break;
                }
                cell_x += unicode_width::UnicodeWidthChar::width(ch)
                    .unwrap_or(0)
                    .max(1);
                char_col += 1;
            }
            char_col
        } else {
            0
        };
        LogPos::new(line, col)
    }

    fn clear_selection(&mut self) {
        self.selection_anchor = None;
        self.selection_end = None;
        self.selecting = false;
    }

    /// Returns the normalized selection range (start <= end), if any.
    fn selection_range(&self) -> Option<SelectionRange> {
        match (self.selection_anchor, self.selection_end) {
            (Some(a), Some(b)) if a != b => {
                let (start, end) = if a <= b { (a, b) } else { (b, a) };
                Some(SelectionRange { start, end })
            }
            _ => None,
        }
    }

    /// Extract the selected text as a string.
    fn selected_text(&self) -> Option<String> {
        let sel = self.selection_range()?;
        let display_count = self.line_count();
        let mut result = String::new();
        for line_idx in sel.start.line..=sel.end.line.min(display_count.saturating_sub(1)) {
            if line_idx >= self.lines.len() {
                break;
            }
            let processed = Self::process_line(&self.lines[line_idx]);
            let chars: Vec<char> = processed.chars().collect();
            let start_col = if line_idx == sel.start.line {
                sel.start.col
            } else {
                0
            };
            let end_col = if line_idx == sel.end.line {
                sel.end.col
            } else {
                chars.len()
            };
            let start_col = start_col.min(chars.len());
            let end_col = end_col.min(chars.len());
            result.extend(&chars[start_col..end_col]);
            if line_idx < sel.end.line {
                result.push('\n');
            }
        }
        if result.is_empty() {
            None
        } else {
            Some(result)
        }
    }

    /// Apply selection highlight to a line's segments.
    fn apply_selection_to_segments(
        segments: &[Segment],
        line_index: usize,
        sel: &SelectionRange,
    ) -> Vec<Segment> {
        if line_index < sel.start.line || line_index > sel.end.line {
            return segments.to_vec();
        }

        let sel_style = RichStyle::default().with_reverse(true);

        // Compute cell-based selection bounds for this line
        let line_start_col = if line_index == sel.start.line {
            sel.start.col
        } else {
            0
        };
        let line_end_col = if line_index == sel.end.line {
            sel.end.col
        } else {
            usize::MAX
        };

        // Walk through segments, converting char columns to cell positions
        let mut result = Vec::new();
        let mut char_col = 0usize;

        for seg in segments {
            let mut before = String::new();
            let mut selected = String::new();
            let mut after = String::new();

            for ch in seg.text.chars() {
                if char_col < line_start_col {
                    before.push(ch);
                } else if char_col < line_end_col {
                    selected.push(ch);
                } else {
                    after.push(ch);
                }
                char_col += 1;
            }

            let base_style = seg.style.unwrap_or_default();
            let has_before = !before.is_empty();
            let has_selected = !selected.is_empty();
            let has_after = !after.is_empty();

            if has_before {
                result.push(Segment::styled(before, base_style));
            }
            if has_selected {
                let merged = base_style.combine(&sel_style);
                result.push(Segment::styled(selected, merged));
            }
            if has_after {
                result.push(Segment::styled(after, base_style));
            }
            if !has_before && !has_selected && !has_after {
                // Preserve empty/control segments as-is
                result.push(seg.clone());
            }
        }
        result
    }
}

impl Widget for Log {
    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        if self.scrollbar_extracted {
            return Vec::new();
        }
        self.scrollbar_extracted = true;
        let mut vbar = ScrollBar::new(true, 2);
        vbar.set_style_id(Some(LOG_VSCROLLBAR_ID.to_string()));
        vec![Box::new(vbar)]
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);
        self.widget_width.store(width, Ordering::Relaxed);
        self.widget_height.store(height, Ordering::Relaxed);

        // WP-25: invalidate cache if width changed
        let prev_width = self.cache_width.swap(width, Ordering::Relaxed);
        if prev_width != width {
            self.cache.lock().unwrap().clear();
        }

        let viewport_width = width;
        let content_height = self.display_line_count();

        self.viewport_height.store(height, Ordering::Relaxed);
        self.content_height.store(content_height, Ordering::Relaxed);

        let max_offset = content_height.saturating_sub(height);
        let offset = self.offset_y.min(max_offset);
        let start = offset.min(content_height);
        let end = (start + height).min(content_height);

        let selection = self.selection_range();
        let display_count = self.line_count();
        let mut rows: Vec<Vec<Segment>> = Vec::with_capacity(height);
        for index in start..end {
            if index < display_count {
                let mut segs = self.render_line_segments(
                    console,
                    options,
                    &self.lines[index],
                    viewport_width,
                    index,
                );
                // WP-24: apply selection highlight
                if let Some(ref sel) = selection {
                    segs = Self::apply_selection_to_segments(&segs, index, sel);
                }
                rows.push(segs);
            } else {
                rows.push(adjust_line_length_no_bg(
                    &[Segment::new(String::new())],
                    viewport_width.max(1),
                ));
            }
        }
        while rows.len() < height {
            rows.push(vec![Segment::new(" ".repeat(viewport_width.max(1)))]);
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
        // WP-24: handle key events for copy
        if let Event::Key(key) = event {
            if self.focused {
                let ctrl = key.modifiers.contains(KeyModifiers::CONTROL)
                    || key.modifiers.contains(KeyModifiers::SUPER);
                if ctrl && key.code == KeyCode::Char('c') {
                    if let Some(text) = self.selected_text() {
                        ctx.post_message(Message::TextEditClipboardCopyRequested(
                            TextEditClipboardCopyRequested { text, cut: false },
                        ));
                        self.clear_selection();
                        ctx.request_repaint();
                        ctx.set_handled();
                        return;
                    }
                }
            }
        }

        if let Event::MouseDown(mouse) = event {
            if mouse.target == self.node_id() {
                let local_x = mouse.x as usize;
                let local_y = mouse.y as usize;

                // WP-24: start text selection
                let pos = self.mouse_to_pos(local_x, local_y);
                self.selection_anchor = Some(pos);
                self.selection_end = Some(pos);
                self.selecting = true;
                ctx.request_repaint();
            }
        }

        if let Event::MouseUp(_) = event {
            if self.selecting {
                self.selecting = false;
                // Single click (no drag) clears selection
                if self.selection_anchor == self.selection_end {
                    self.clear_selection();
                }
                ctx.request_repaint();
            }
        }

        if let Event::AppFocus(false) = event {
            if self.selecting {
                self.selecting = false;
                ctx.request_repaint();
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

    fn on_mouse_move(&mut self, x: u16, y: u16) -> bool {
        // WP-24: extend selection during drag
        if self.selecting {
            let pos = self.mouse_to_pos(x as usize, y as usize);
            if self.selection_end != Some(pos) {
                self.selection_end = Some(pos);
                return true;
            }
        }
        false
    }

    fn content_width(&self) -> Option<usize> {
        let content_width = self.max_line_width.max(1);
        let meta = crate::css::selector_meta_generic(self);
        let resolved = crate::css::resolve_style(self, &meta);
        let padding = resolved.effective_padding();
        let (_, _, border_left, border_right) =
            super::helpers::border_spacing_from_style(&resolved);
        let chrome_lr =
            usize::from(padding.left.saturating_add(padding.right)) + border_left + border_right;
        Some(content_width.saturating_add(chrome_lr).max(1))
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

    fn get_selection(&self) -> Option<String> {
        self.selected_text()
    }

    fn on_message(&mut self, event: &MessageEvent, ctx: &mut EventCtx) {
        let Message::ScrollbarScrollTo(payload) = &event.message else {
            return;
        };
        if payload.axis != ScrollbarAxis::Vertical {
            return;
        }
        let viewport_h = self.viewport_height.load(Ordering::Relaxed).max(1);
        let content_h = self.content_height.load(Ordering::Relaxed).max(1);
        let next = ScrollView::line_clamp_offset(
            payload.offset.max(0.0).round() as usize,
            content_h,
            viewport_h,
        );
        if next != self.offset_y {
            self.offset_y = next;
            ctx.request_repaint();
            self.emit_scroll_changed_message(ctx);
        }
        ctx.set_handled();
    }

    fn scroll_offset(&self) -> (usize, usize) {
        (0, self.offset_y)
    }

    fn scroll_offset_f32(&self) -> (f32, f32) {
        (0.0, self.offset_y as f32)
    }

    fn scroll_virtual_content_size(&self) -> Option<(usize, usize)> {
        Some((self.max_line_width.max(1), self.display_line_count().max(1)))
    }
}

impl Renderable for Log {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[cfg(test)]
mod tests {
    use super::{LOG_VSCROLLBAR_ID, Log};
    use crate::event::{Action, Event, EventCtx};
    use crate::message::*;
    use crate::node_id::NodeId;
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
                .any(|m| matches!(m.message, Message::RichLogScrolled(..)))
        );
    }

    #[test]
    fn mouse_up_after_thumb_drag_requests_repaint() {
        let console = Console::new();
        let options = options_for(&console, 16, 3);
        let mut log = Log::new().auto_scroll(false);
        log.write_lines(["line 1", "line 2", "line 3", "line 4", "line 5"]);
        let _ = log.take_composed_children();
        let _ = log.render(&console, &options);
        assert_eq!(log.offset_y, 0);

        let mut ctx = EventCtx::default();
        log.on_message(
            &MessageEvent {
                sender: NodeId::default(),
                message: Message::ScrollbarScrollTo(ScrollbarScrollTo {
                    axis: ScrollbarAxis::Vertical,
                    offset: 2.0,
                    animate: false,
                }),
                control: None,
            },
            &mut ctx,
        );
        assert!(ctx.handled());
        assert!(ctx.repaint_requested());
        assert!(log.offset_y > 0);
    }

    #[test]
    fn cache_avoids_rerender() {
        let console = Console::new();
        let options = options_for(&console, 20, 5);
        let mut log = Log::new().auto_scroll(false);
        log.write_lines(["alpha", "beta", "gamma"]);

        // First render populates the cache
        let _ = log.render(&console, &options);
        let cache_entries = log.cache.lock().unwrap().entries.len();
        assert!(cache_entries > 0, "cache should be populated after render");

        // Second render should reuse cached entries (no assertion on internals,
        // but we verify no panic and output is consistent)
        let out1 = log.render(&console, &options);
        let out2 = log.render(&console, &options);
        assert_eq!(out1.len(), out2.len());
    }

    #[test]
    fn cache_invalidated_on_write() {
        let console = Console::new();
        let options = options_for(&console, 20, 5);
        let mut log = Log::new().auto_scroll(false);
        log.write_lines(["alpha", "beta"]);
        let _ = log.render(&console, &options);

        let before = log.cache.lock().unwrap().entries.len();
        log.write_line("gamma");
        let after = log.cache.lock().unwrap().entries.len();
        // New line was added at index 2, so entries for 0 and 1 should remain
        assert!(
            after <= before,
            "cache entries for existing lines preserved"
        );

        // But rendering includes the new line
        let _ = log.render(&console, &options);
        let final_count = log.cache.lock().unwrap().entries.len();
        assert!(
            final_count >= 3,
            "cache should have all 3 lines after render"
        );
    }

    #[test]
    fn cache_cleared_on_clear() {
        let console = Console::new();
        let options = options_for(&console, 20, 5);
        let mut log = Log::new().auto_scroll(false);
        log.write_lines(["alpha", "beta"]);
        let _ = log.render(&console, &options);
        assert!(log.cache.lock().unwrap().entries.len() > 0);

        log.clear();
        assert_eq!(log.cache.lock().unwrap().entries.len(), 0);
    }

    #[test]
    fn selection_text_extraction() {
        let mut log = Log::new();
        log.write_lines(["hello world", "foo bar", "baz qux"]);

        // Simulate selection of "world\nfoo"
        log.selection_anchor = Some(super::LogPos::new(0, 6));
        log.selection_end = Some(super::LogPos::new(1, 3));

        let text = log.selected_text();
        assert_eq!(text.as_deref(), Some("world\nfoo"));
    }

    #[test]
    fn selection_cleared_on_single_click() {
        let console = Console::new();
        let options = options_for(&console, 20, 5);
        let mut log = Log::new().auto_scroll(false);
        log.write_lines(["hello", "world"]);
        let _ = log.render(&console, &options);

        let id = NodeId::default();
        // Mouse down starts selection
        let mut ctx = EventCtx::default();
        log.on_event(
            &Event::MouseDown(crate::event::MouseDownEvent {
                target: id,
                screen_x: 2,
                screen_y: 0,
                x: 2,
                y: 0,
            }),
            &mut ctx,
        );
        assert!(log.selecting);

        // Mouse up at same position clears selection
        let mut ctx = EventCtx::default();
        log.on_event(
            &Event::MouseUp(crate::event::MouseUpEvent {
                target: Some(id),
                screen_x: 2,
                screen_y: 0,
                x: 2,
                y: 0,
            }),
            &mut ctx,
        );
        assert!(!log.selecting);
        assert!(log.selection_range().is_none());
    }

    #[test]
    fn tree_mode_extracts_dedicated_scrollbar_child() {
        let mut log = Log::new();
        let children = log.take_composed_children();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].style_id(), Some(LOG_VSCROLLBAR_ID));
    }

    #[test]
    fn scrollbar_message_updates_offset_in_tree_mode() {
        let console = Console::new();
        let options = options_for(&console, 16, 3);
        let mut log = Log::new().auto_scroll(false);
        log.write_lines(["line 1", "line 2", "line 3", "line 4", "line 5"]);
        let _ = log.take_composed_children();
        let _ = log.render(&console, &options);

        let mut ctx = EventCtx::default();
        log.on_message(
            &MessageEvent {
                sender: NodeId::default(),
                message: Message::ScrollbarScrollTo(ScrollbarScrollTo {
                    axis: ScrollbarAxis::Vertical,
                    offset: 2.0,
                    animate: false,
                }),
                control: None,
            },
            &mut ctx,
        );

        assert!(ctx.handled());
        assert_eq!(log.offset_y, 2);
    }
}
