use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use unicode_width::UnicodeWidthChar;

use rich_rs::highlighter::repr_highlighter;
use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments, Text};
use textual_macros::widget;

use crate::event::{Action, Event};
use crate::message::*;

use super::helpers::adjust_line_length_no_bg;

use super::{NodeSeed, ScrollBar, ScrollView};
use crate::reactive::{ReactiveChange, ReactiveCtx, ReactiveFlags, ReactiveWidget};

pub(crate) const RICH_LOG_VSCROLLBAR_ID: &str = "__rich_log_vscrollbar";

/// Simple LRU cache for rendered line segments.
#[derive(Debug)]
struct LineCache {
    entries: HashMap<(usize, u64), Vec<Vec<Segment>>>,
    order: Vec<(usize, u64)>,
    max_size: usize,
}

impl LineCache {
    fn new(max_size: usize) -> Self {
        Self {
            entries: HashMap::new(),
            order: Vec::new(),
            max_size: max_size.max(1),
        }
    }

    fn get(&mut self, key: &(usize, u64)) -> Option<&Vec<Vec<Segment>>> {
        if self.entries.contains_key(key) {
            // Move to end (most recently used)
            self.order.retain(|k| k != key);
            self.order.push(*key);
            self.entries.get(key)
        } else {
            None
        }
    }

    fn insert(&mut self, key: (usize, u64), value: Vec<Vec<Segment>>) {
        if self.entries.contains_key(&key) {
            self.order.retain(|k| *k != key);
        } else if self.entries.len() >= self.max_size {
            // Evict least recently used
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

#[derive(Debug)]
#[widget(Focus, Interactive, Scrollable)]
pub struct RichLog {
    lines: Vec<LogLine>,
    max_lines: Option<usize>,
    auto_scroll: bool,
    wrap: bool,
    highlight: bool,
    markup: bool,
    highlighter: rich_rs::highlighter::RegexHighlighter,
    min_width: usize,
    scroll_step: usize,
    offset_y: usize,
    content_height: AtomicUsize,
    viewport_height: AtomicUsize,
    widget_width: AtomicUsize,
    widget_height: AtomicUsize,
    scrollbar_extracted: bool,
    seed: NodeSeed,
    cache: Mutex<LineCache>,
    cache_width: AtomicUsize,
    /// Whether this widget has been rendered at least once (size is known).
    sized: bool,
}

enum LogLine {
    Plain(String),
    Markup(String),
    Styled(Vec<Segment>),
    Renderable(Box<dyn Renderable>),
}

impl LogLine {
    fn content_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        std::mem::discriminant(self).hash(&mut hasher);
        match self {
            LogLine::Plain(s) | LogLine::Markup(s) => s.hash(&mut hasher),
            LogLine::Styled(segs) => {
                for seg in segs {
                    seg.text.hash(&mut hasher);
                }
            }
            // Renderables are opaque — use a unique sentinel so they never match
            LogLine::Renderable(_) => 0xDEAD_BEEF_u64.hash(&mut hasher),
        }
        hasher.finish()
    }
}

impl std::fmt::Debug for LogLine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogLine::Plain(content) => f.debug_tuple("Plain").field(content).finish(),
            LogLine::Markup(content) => f.debug_tuple("Markup").field(content).finish(),
            LogLine::Styled(segments) => f.debug_tuple("Styled").field(&segments.len()).finish(),
            LogLine::Renderable(_) => f.write_str("Renderable(..)"),
        }
    }
}

impl Default for RichLog {
    fn default() -> Self {
        Self::new()
    }
}

impl RichLog {
    crate::seed_ident_methods!();

    pub fn new() -> Self {
        Self {
            lines: Vec::new(),
            max_lines: None,
            auto_scroll: true,
            wrap: false,
            highlight: false,
            markup: false,
            highlighter: repr_highlighter(),
            min_width: 78,
            scroll_step: 1,
            offset_y: 0,
            content_height: AtomicUsize::new(1),
            viewport_height: AtomicUsize::new(1),
            widget_width: AtomicUsize::new(1),
            widget_height: AtomicUsize::new(1),
            scrollbar_extracted: false,
            seed: NodeSeed::default(),
            cache: Mutex::new(LineCache::new(1000)),
            cache_width: AtomicUsize::new(0),
            sized: false,
        }
    }

    pub fn cache_size(mut self, max_entries: usize) -> Self {
        self.cache = Mutex::new(LineCache::new(max_entries));
        self
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
        if self.wrap != wrap {
            self.wrap = wrap;
            self.cache.lock().unwrap().clear();
        }
        self
    }

    pub fn highlight(mut self, highlight: bool) -> Self {
        if self.highlight != highlight {
            self.highlight = highlight;
            self.cache.lock().unwrap().clear();
        }
        self
    }

    pub fn markup(mut self, markup: bool) -> Self {
        if self.markup != markup {
            self.markup = markup;
            self.cache.lock().unwrap().clear();
        }
        self
    }

    pub fn min_width(mut self, min_width: usize) -> Self {
        self.min_width = min_width;
        self
    }

    pub fn scroll_step(mut self, step: usize) -> Self {
        self.scroll_step = step.max(1);
        self
    }

    // ── Reactive getters ─────────────────────────────────────────────────

    /// Reactive getter for `wrap`.
    pub fn get_wrap(&self) -> bool {
        self.wrap
    }

    /// Reactive getter for `highlight`.
    pub fn get_highlight(&self) -> bool {
        self.highlight
    }

    /// Reactive getter for `markup`.
    pub fn get_markup(&self) -> bool {
        self.markup
    }

    /// Reactive getter for `max_lines`.
    pub fn get_max_lines(&self) -> Option<usize> {
        self.max_lines
    }

    /// Reactive getter for `auto_scroll`.
    pub fn get_auto_scroll(&self) -> bool {
        self.auto_scroll
    }

    // ── Reactive setters ─────────────────────────────────────────────────

    /// Reactive setter for `wrap`. Records the change and triggers
    /// watcher dispatch to clear cache.
    pub fn set_wrap(&mut self, value: bool, ctx: &mut ReactiveCtx) {
        if self.wrap != value {
            let old = self.wrap;
            self.wrap = value;
            ctx.record_change(
                "wrap",
                ReactiveFlags::reactive(),
                Box::new(old),
                Box::new(value),
            );
        }
    }

    /// Reactive setter for `highlight`. Records the change and triggers
    /// watcher dispatch to clear cache.
    pub fn set_highlight(&mut self, value: bool, ctx: &mut ReactiveCtx) {
        if self.highlight != value {
            let old = self.highlight;
            self.highlight = value;
            ctx.record_change(
                "highlight",
                ReactiveFlags::reactive(),
                Box::new(old),
                Box::new(value),
            );
        }
    }

    /// Reactive setter for `markup`. Records the change and triggers
    /// watcher dispatch to clear cache.
    pub fn set_markup(&mut self, value: bool, ctx: &mut ReactiveCtx) {
        if self.markup != value {
            let old = self.markup;
            self.markup = value;
            ctx.record_change(
                "markup",
                ReactiveFlags::reactive(),
                Box::new(old),
                Box::new(value),
            );
        }
    }

    /// Reactive setter for `max_lines`. Records the change in the provided
    /// [`ReactiveCtx`].
    pub fn set_max_lines(&mut self, value: Option<usize>, ctx: &mut ReactiveCtx) {
        let value = value.map(|v| v.max(1));
        if self.max_lines != value {
            let old = self.max_lines;
            self.max_lines = value;
            ctx.record_change(
                "max_lines",
                ReactiveFlags::reactive(),
                Box::new(old),
                Box::new(value),
            );
        }
    }

    /// Reactive setter for `auto_scroll`. Records the change in the provided
    /// [`ReactiveCtx`].
    pub fn set_auto_scroll(&mut self, value: bool, ctx: &mut ReactiveCtx) {
        if self.auto_scroll != value {
            let old = self.auto_scroll;
            self.auto_scroll = value;
            ctx.record_change(
                "auto_scroll",
                ReactiveFlags::reactive(),
                Box::new(old),
                Box::new(value),
            );
        }
    }

    // ── Watchers ─────────────────────────────────────────────────────────

    fn watch_wrap(&mut self, _old: &bool, _new: &bool, _ctx: &mut ReactiveCtx) {
        self.cache.lock().unwrap().clear();
    }

    fn watch_highlight(&mut self, _old: &bool, _new: &bool, _ctx: &mut ReactiveCtx) {
        self.cache.lock().unwrap().clear();
    }

    fn watch_markup(&mut self, _old: &bool, _new: &bool, _ctx: &mut ReactiveCtx) {
        self.cache.lock().unwrap().clear();
    }

    /// Returns true if the widget has been rendered at least once (size is known).
    /// After first render, widget_width is set to actual width (>= min_width).
    fn is_sized(&self) -> bool {
        self.sized || self.widget_width.load(Ordering::Relaxed) > 1
    }

    /// Lazily mark sized=true once widget_width indicates a render happened.
    fn mark_sized_if_ready(&mut self) {
        if !self.sized && self.widget_width.load(Ordering::Relaxed) > 1 {
            self.sized = true;
        }
    }

    pub fn write(&mut self, content: impl Into<String>) -> &mut Self {
        self.mark_sized_if_ready();
        let content = content.into();
        let insert_from = self.lines.len();
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
        self.cache.lock().unwrap().invalidate_from(insert_from);
        self.trim_to_max_lines();
        if self.auto_scroll {
            self.scroll_end();
        } else {
            self.clamp_offset();
        }
        self
    }

    pub fn write_segments(&mut self, segments: Vec<Segment>) -> &mut Self {
        self.mark_sized_if_ready();
        let insert_from = self.lines.len();
        // Skip expensive estimation when widget hasn't been sized yet
        // (widget_width is still default=1, estimation would be inaccurate)
        let estimated_added_lines = if self.is_sized() {
            self.estimate_segment_lines(&segments)
        } else {
            1
        };
        self.lines.push(LogLine::Styled(segments));
        self.cache.lock().unwrap().invalidate_from(insert_from);
        self.trim_to_max_lines();
        if self.auto_scroll {
            if self.is_sized() {
                self.scroll_end_with_estimated_added_lines(estimated_added_lines);
            } else {
                self.scroll_end();
            }
        } else {
            self.clamp_offset();
        }
        self
    }

    pub fn write_markup(&mut self, content: impl Into<String>) -> &mut Self {
        self.mark_sized_if_ready();
        let content = content.into();
        let insert_from = self.lines.len();
        if content.is_empty() {
            self.lines.push(LogLine::Markup(String::new()));
        } else {
            self.lines.extend(
                content
                    .split('\n')
                    .map(std::string::ToString::to_string)
                    .map(LogLine::Markup),
            );
        }
        self.cache.lock().unwrap().invalidate_from(insert_from);
        self.trim_to_max_lines();
        if self.auto_scroll {
            self.scroll_end();
        } else {
            self.clamp_offset();
        }
        self
    }

    pub fn write_renderable(&mut self, renderable: impl Renderable + 'static) -> &mut Self {
        self.mark_sized_if_ready();
        let insert_from = self.lines.len();
        // Skip expensive estimation when widget hasn't been sized yet
        let estimated_added_lines = if self.is_sized() {
            self.estimate_renderable_lines(&renderable)
        } else {
            1
        };
        self.lines.push(LogLine::Renderable(Box::new(renderable)));
        self.cache.lock().unwrap().invalidate_from(insert_from);
        self.trim_to_max_lines();
        if self.auto_scroll {
            if self.is_sized() {
                self.scroll_end_with_estimated_added_lines(estimated_added_lines);
            } else {
                self.scroll_end();
            }
        } else {
            self.clamp_offset();
        }
        self
    }

    /// Write a value as a pretty-printed repr renderable.
    ///
    /// Mirrors Python `RichLog.write(Pretty(content))`: the value's `Debug`
    /// output is fed through the `rich_rs` pretty-printer (syntax highlighting,
    /// indentation, Python-repr-style string quoting) rather than being written
    /// as a plain text string.
    pub fn write_debug<T: std::fmt::Debug>(&mut self, value: T) -> &mut Self {
        let pretty = rich_rs::pretty::Pretty::from_str(format!("{value:?}"));
        self.write_renderable(pretty)
    }

    pub fn clear(&mut self) -> &mut Self {
        self.lines.clear();
        self.offset_y = 0;
        self.content_height.store(1, Ordering::Relaxed);
        self.cache.lock().unwrap().clear();
        self
    }

    fn trim_to_max_lines(&mut self) {
        if let Some(max_lines) = self.max_lines {
            if self.lines.len() > max_lines {
                let excess = self.lines.len() - max_lines;
                self.lines.drain(0..excess);
                self.offset_y = self.offset_y.saturating_sub(excess);
                // Indices shifted — clear the whole cache
                self.cache.lock().unwrap().clear();
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

    fn scroll_end_with_estimated_added_lines(&mut self, estimated_added_lines: usize) {
        let estimated_content_height = self
            .content_height
            .load(Ordering::Relaxed)
            .saturating_add(estimated_added_lines.max(1));
        self.offset_y = ScrollView::line_scroll_end(
            self.lines.len().max(estimated_content_height),
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

    fn emit_scroll_changed_message(&self, ctx: &mut crate::event::WidgetCtx) {
        ctx.post_message(RichLogScrolled {
            offset: self.offset_y,
            max_offset: self.max_offset(),
        });
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

        // Invalidate cache if width changed
        let prev_width = self.cache_width.swap(width, Ordering::Relaxed);
        if prev_width != width {
            self.cache.lock().unwrap().clear();
        }

        let mut out: Vec<Vec<Segment>> = Vec::new();
        for (line_idx, line) in self.lines.iter().enumerate() {
            // Renderable lines are opaque/dynamic — skip caching
            let cacheable = !matches!(line, LogLine::Renderable(_));

            if cacheable {
                let content_hash = line.content_hash();
                let cache_key = (line_idx, content_hash);

                // Try cache first
                {
                    let mut cache = self.cache.lock().unwrap();
                    if let Some(cached) = cache.get(&cache_key) {
                        out.extend(cached.iter().cloned());
                        continue;
                    }
                }

                let rendered_lines = self.render_line(line, console, options, width);

                // Store in cache
                {
                    let mut cache = self.cache.lock().unwrap();
                    cache.insert(cache_key, rendered_lines.clone());
                }

                out.extend(rendered_lines);
            } else {
                out.extend(self.render_line(line, console, options, width));
            }
        }

        if out.is_empty() {
            out.push(vec![Segment::new(String::new())]);
        }
        out
    }

    fn render_line(
        &self,
        line: &LogLine,
        console: &Console,
        options: &ConsoleOptions,
        width: usize,
    ) -> Vec<Vec<Segment>> {
        match line {
            LogLine::Plain(content) => {
                let text = console.render_str(
                    content,
                    Some(self.markup),
                    None,
                    Some(self.highlight),
                    Some(&self.highlighter),
                );
                if self.wrap {
                    let mut lines = Vec::new();
                    for wrapped in wrap_line(content, width.max(1)) {
                        let rendered = if self.markup || self.highlight {
                            console
                                .render_str(
                                    &wrapped,
                                    Some(self.markup),
                                    None,
                                    Some(self.highlight),
                                    Some(&self.highlighter),
                                )
                                .render(console, options)
                        } else {
                            Text::plain(wrapped).render(console, options)
                        };
                        let split =
                            Segment::split_and_crop_lines(rendered, width, None, true, false);
                        if let Some(first) = split.first() {
                            lines.push(first.clone());
                        } else {
                            lines.push(vec![Segment::new(String::new())]);
                        }
                    }
                    lines
                } else {
                    let rendered = text.render(console, options);
                    let split = Segment::split_and_crop_lines(rendered, width, None, true, false);
                    if let Some(first) = split.first() {
                        vec![first.clone()]
                    } else {
                        vec![vec![Segment::new(String::new())]]
                    }
                }
            }
            LogLine::Markup(content) => {
                let rendered = console.render_str(content, Some(true), None, None, None);
                let split = Segment::split_and_crop_lines(
                    rendered.render(console, options),
                    width,
                    None,
                    true,
                    false,
                );
                if split.is_empty() {
                    vec![vec![Segment::new(String::new())]]
                } else {
                    split
                }
            }
            LogLine::Styled(segments) => {
                let split =
                    Segment::split_and_crop_lines(segments.clone(), width, None, true, false);
                if split.is_empty() {
                    vec![vec![Segment::new(String::new())]]
                } else {
                    split
                }
            }
            LogLine::Renderable(renderable) => {
                let split = Segment::split_and_crop_lines(
                    renderable.render(console, options),
                    width,
                    None,
                    true,
                    false,
                );
                if split.is_empty() {
                    vec![vec![Segment::new(String::new())]]
                } else {
                    split
                }
            }
        }
    }

    fn estimate_segment_lines(&self, segments: &[Segment]) -> usize {
        let width = self.widget_width.load(Ordering::Relaxed).max(1);
        Segment::split_and_crop_lines(segments.to_vec(), width, None, true, false)
            .len()
            .max(1)
    }

    fn estimate_renderable_lines(&self, renderable: &dyn Renderable) -> usize {
        let width = self.widget_width.load(Ordering::Relaxed).max(1);
        let console = Console::new();
        let mut options = console.options().clone();
        options.size = (width, 1);
        options.max_width = width;
        options.max_height = 1;
        Segment::split_and_crop_lines(
            renderable.render(&console, &options),
            width,
            None,
            true,
            false,
        )
        .len()
        .max(1)
    }
}

impl crate::widgets::Focus for RichLog {
    fn focusable(&self) -> bool {
        true
    }
}

impl crate::widgets::Interactive for RichLog {
    fn on_event(&mut self, event: &Event, ctx: &mut crate::event::WidgetCtx) {
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

    fn on_mouse_move(&mut self, _x: u16, _y: u16) -> bool {
        false
    }

    fn on_message(&mut self, event: &MessageEvent, ctx: &mut crate::event::WidgetCtx) {
        let Some(payload) = event.downcast_ref::<ScrollbarScrollTo>() else {
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
}

impl crate::widgets::Scrollable for RichLog {
    fn on_mouse_scroll(&mut self, _delta_x: i32, delta_y: i32, ctx: &mut crate::event::WidgetCtx) {
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

    fn scroll_offset(&self) -> (usize, usize) {
        (0, self.offset_y)
    }

    fn scroll_offset_f32(&self) -> (f32, f32) {
        (0.0, self.offset_y as f32)
    }

    fn scroll_virtual_content_size(&self) -> Option<(usize, usize)> {
        let width = self
            .widget_width
            .load(Ordering::Relaxed)
            .max(self.min_width)
            .max(1);
        let height = self
            .content_height
            .load(Ordering::Relaxed)
            .max(self.lines.len().max(1));
        Some((width, height))
    }
}

impl crate::widgets::Render for RichLog {
    fn compose(&mut self) -> crate::compose::ComposeResult {
        if self.scrollbar_extracted {
            return Vec::new();
        }
        self.scrollbar_extracted = true;
        let mut vbar = ScrollBar::new(true, 2);
        vbar.seed.css_id = Some(RICH_LOG_VSCROLLBAR_ID.to_string());
        vec![crate::compose::ChildDecl::new(Box::new(vbar))]
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(self.min_width).max(1);
        let height = options.size.1.max(1);
        self.widget_width.store(width, Ordering::Relaxed);
        self.widget_height.store(height, Ordering::Relaxed);

        let viewport_width = width;
        let physical = self.physical_lines(console, options, viewport_width);
        let content_height = physical.len().max(1);

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

        let mut out = Segments::new();
        for (index, row) in rows.into_iter().enumerate() {
            out.extend(row);
            if index + 1 < height {
                out.push(Segment::line());
            }
        }
        out
    }
}
impl ReactiveWidget for RichLog {
    fn reactive_dispatch(&mut self, changes: &[ReactiveChange], ctx: &mut ReactiveCtx) {
        for change in changes {
            match change.field_name {
                "wrap" => {
                    if let (Some(old), Some(new)) = (
                        change.old_value.downcast_ref::<bool>(),
                        change.new_value.downcast_ref::<bool>(),
                    ) {
                        self.watch_wrap(old, new, ctx);
                    }
                }
                "highlight" => {
                    if let (Some(old), Some(new)) = (
                        change.old_value.downcast_ref::<bool>(),
                        change.new_value.downcast_ref::<bool>(),
                    ) {
                        self.watch_highlight(old, new, ctx);
                    }
                }
                "markup" => {
                    if let (Some(old), Some(new)) = (
                        change.old_value.downcast_ref::<bool>(),
                        change.new_value.downcast_ref::<bool>(),
                    ) {
                        self.watch_markup(old, new, ctx);
                    }
                }
                _ => {}
            }
        }
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
    use super::{RICH_LOG_VSCROLLBAR_ID, RichLog};
    use crate::event::{Action, Event, EventCtx};
    use crate::message::*;
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
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            log.on_event(&Event::Action(Action::ScrollDown), &mut __w);
        }
        let messages = ctx.take_messages();
        assert!(messages.iter().any(|m| m.is::<RichLogScrolled>()));
    }

    #[test]
    fn tree_mode_extracts_dedicated_scrollbar_child() {
        let mut log = RichLog::new();
        let mut children = log.compose();
        assert_eq!(children.len(), 1);
        assert_eq!(
            children[0].widget_mut().take_node_seed().css_id.as_deref(),
            Some(RICH_LOG_VSCROLLBAR_ID)
        );
    }

    #[test]
    fn scrollbar_message_updates_offset_in_tree_mode() {
        let console = Console::new();
        let options = options_for(&console, 20, 3);
        let mut log = RichLog::new().auto_scroll(false);
        log.write("line 1");
        log.write("line 2");
        log.write("line 3");
        log.write("line 4");
        let _ = log.compose();
        let _ = log.render(&console, &options);

        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            log.on_message(
            &MessageEvent::new(
                crate::node_id::NodeId::default(),
                ScrollbarScrollTo {
                    axis: ScrollbarAxis::Vertical,
                    offset: 1.0,
                    animate: false,
                    scroll_duration: None,
                },
            ),
            &mut __w);
        }
        assert!(ctx.handled());
        assert_eq!(log.offset_y, 1);
    }
}
