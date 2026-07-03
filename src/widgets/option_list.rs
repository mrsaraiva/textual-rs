use crossterm::event::KeyCode;
use rich_rs::{Console, ConsoleOptions, MetaValue, Renderable, Segment, Segments, Text};

use crate::event::{Action, Event};
use crate::message::*;

#[path = "toggle_option.rs"]
pub(crate) mod toggle_option;

use super::{NodeSeed, ScrollBar, Widget, helpers::adjust_line_length_no_bg};
use toggle_option::OptionCursorState;
pub use toggle_option::{OptionContent, OptionId, OptionItem};

pub(crate) const OPTION_LIST_VSCROLLBAR_ID: &str = "__option_list_vscrollbar";

/// Width of the vertical scrollbar (Python `scrollbar-size-vertical` default is 2).
/// Renderable items must not expand into this zone or the scrollbar will overwrite them.
const SCROLLBAR_THICKNESS: usize = 2;

/// Tag a segment `textual:no_style = true` so the widget-level style pass
/// (`apply_style_to_segments`) leaves it untouched. Used for the highlighted
/// option's cells, whose opaque `$block-cursor` background is fully composed
/// here; without this the `OptionList:focus` `background-tint: $foreground 5%`
/// would be re-applied to it (Python does not tint component-painted surfaces).
fn tag_segment_no_style(seg: &mut Segment) {
    let mut meta = seg.meta.take().unwrap_or_default();
    let mut map: std::collections::BTreeMap<String, MetaValue> = meta
        .meta
        .as_ref()
        .map(|m| (**m).clone())
        .unwrap_or_default();
    map.insert("textual:no_style".to_string(), MetaValue::Bool(true));
    meta.meta = Some(std::sync::Arc::new(map));
    seg.meta = Some(meta);
}

/// Python `DIM_FACTOR` (textual/constants.py): how much of the foreground
/// survives when a `dim` attribute is converted to a colour blend (0 = pure
/// background, 1 = unchanged foreground).
const DIM_FACTOR: f64 = 0.66;

/// Replace a `dim` attribute with a foreground pre-blended toward `bg`
/// (Python `textual/filter.py` `dim_color`: `bg + (fg - bg) * DIM_FACTOR`,
/// truncated per channel like rich's `Color.from_rgb`).
///
/// Python's ALWAYS-ON `ANSIToTruecolor` line filter performs this conversion
/// on every rendered line and strips the `dim` attribute, so a dim glyph
/// never reaches the terminal as SGR dim — its dimming is baked into the
/// colour against the segment's own (composed) background. Rust forwards SGR
/// dim to the terminal, which is wrong over the opaque block-cursor fill: the
/// cursor fg would paint at full strength (terminals rarely dim truecolor).
fn dim_fg_toward_bg(fg: rich_rs::SimpleColor, bg: rich_rs::SimpleColor) -> rich_rs::SimpleColor {
    let f = crate::style::color_from_simple(fg);
    let b = crate::style::color_from_simple(bg);
    let blend =
        |bc: u8, fc: u8| -> u8 { (f64::from(bc) + (f64::from(fc) - f64::from(bc)) * DIM_FACTOR) as u8 };
    rich_rs::SimpleColor::Rgb {
        r: blend(b.r, f.r),
        g: blend(b.g, f.g),
        b: blend(b.b, f.b),
    }
}

/// Rebuild a highlighted option's display line so its opaque `$block-cursor`
/// background spans the full option width (Python paints the whole highlighted
/// row, including the trailing pad) and every cell is tagged `no_style`.
///
/// `fill` carries the fully-composed highlight foreground/background; content
/// cells keep their own foreground where present, but all cells are forced to
/// the opaque highlight background. A cell carrying a `dim` attribute (e.g.
/// the `Select` overlay's blank prompt row) keeps its dimming as a COLOUR: the
/// applied foreground is pre-blended toward the highlight background and the
/// attribute is stripped, exactly as Python's `ANSIToTruecolor` filter
/// composites dim over the block cursor (see [`dim_fg_toward_bg`]).
fn finalize_highlight_line(line: &[Segment], width: usize, fill: rich_rs::Style) -> Vec<Segment> {
    let mut out: Vec<Segment> = Vec::new();
    let mut used = 0usize;
    for seg in line {
        if seg.control.is_some() {
            out.push(seg.clone());
            continue;
        }
        let seg_w = rich_rs::cell_len(&seg.text);
        if used + seg_w > width {
            break;
        }
        let mut style = seg.style.unwrap_or_default();
        style.bgcolor = fill.bgcolor;
        if style.color.is_none() {
            style.color = fill.color;
        }
        // Python parity: dim + colour => pre-blended colour, dim stripped
        // (`truecolor_style` only converts when a foreground is present).
        if style.dim == Some(true) {
            if let (Some(fg), Some(bg)) = (style.color, style.bgcolor) {
                if fg != rich_rs::SimpleColor::Default && bg != rich_rs::SimpleColor::Default {
                    style.color = Some(dim_fg_toward_bg(fg, bg));
                    style.dim = None;
                }
            }
        }
        let mut s = Segment::styled(seg.text.clone(), style);
        tag_segment_no_style(&mut s);
        out.push(s);
        used += seg_w;
    }
    if used < width {
        let mut pad = Segment::styled(" ".repeat(width - used), fill);
        tag_segment_no_style(&mut pad);
        out.push(pad);
    }
    out
}

/// A scrollable, navigable list of selectable options.
///
/// Supports separators between groups, disabled items, keyboard and mouse navigation,
/// and emits [`OptionHighlighted`] / [`OptionSelected`] messages.
#[derive(Debug, Clone)]
pub struct OptionList {
    items: Vec<OptionItem>,
    cursor: OptionCursorState,
    disabled: bool,
    offset: usize,
    hovered_index: Option<usize>,
    viewport_height: usize,
    /// Most recent layout width (stored so Renderable item heights can be computed).
    layout_width: usize,
    scroll_step: usize,
    scrollbar_extracted: bool,
    /// Per-option horizontal inset (Python's `.option-list--option { padding }`).
    /// `0` for a bare `OptionList`; the `Select` overlay sets it to `1`. Kept as
    /// an explicit field (not resolved from the parent-contextual CSS rule) so the
    /// wrap width used by `item_height` and by `render` agree — a mismatch would
    /// clip wrapped rows.
    option_pad_left: usize,
    seed: NodeSeed,
}

impl Default for OptionList {
    fn default() -> Self {
        Self::new()
    }
}

impl OptionList {
    crate::seed_ident_methods!();

    /// Create an empty `OptionList`.
    pub fn new() -> Self {
        let seed = NodeSeed {
            classes: vec!["option-list".to_string()],
            ..NodeSeed::default()
        };
        Self {
            items: Vec::new(),
            cursor: OptionCursorState::default(),
            disabled: false,
            offset: 0,
            hovered_index: None,
            viewport_height: 1,
            layout_width: 80,
            scroll_step: 1,
            scrollbar_extracted: false,
            option_pad_left: 0,
            seed,
        }
    }

    /// Set a per-option left inset (used by the `Select` overlay to mirror
    /// Python's `.option-list--option { padding: 0 1 }`). Both the render indent
    /// and the wrap-width measurement use this, so they stay consistent.
    pub(crate) fn set_option_pad_left(&mut self, pad: usize) {
        self.option_pad_left = pad;
    }

    /// Create an `OptionList` pre-populated with items.
    pub fn with_items(items: Vec<OptionItem>) -> Self {
        let mut list = Self::new();
        list.items = items;
        list.cursor.set_highlighted(list.first_selectable());
        list
    }

    /// Builder: set the scroll step (number of rows per scroll tick).
    pub fn scroll_step(mut self, step: usize) -> Self {
        self.scroll_step = step.max(1);
        self
    }

    /// Builder: set disabled state for the entire list.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    // ── Public API ──────────────────────────────────────────────────

    /// Add a selectable option.
    pub fn add_option(&mut self, prompt: impl Into<String>, id: Option<OptionId>, disabled: bool) {
        let was_empty = self.cursor.highlighted().is_none();
        self.items.push(OptionItem::Option {
            prompt: prompt.into(),
            content: None,
            id,
            disabled,
        });
        if was_empty && !disabled {
            self.cursor.set_highlighted(Some(self.items.len() - 1));
        }
    }

    /// Add a selectable option with rich [`Text`] content.
    pub fn add_rich_option(
        &mut self,
        label: impl Into<String>,
        content: Text,
        id: Option<OptionId>,
        disabled: bool,
    ) {
        let was_empty = self.cursor.highlighted().is_none();
        self.items.push(OptionItem::Option {
            prompt: label.into(),
            content: Some(OptionContent::Text(content)),
            id,
            disabled,
        });
        if was_empty && !disabled {
            self.cursor.set_highlighted(Some(self.items.len() - 1));
        }
    }

    /// Add a selectable option with arbitrary [`Renderable`] content.
    ///
    /// The renderable is stored as `Arc<dyn Renderable>` and rendered live at
    /// the runtime widget width. Use this for `Table`, `Panel`, and other
    /// multi-row or dynamically-sized renderables.
    pub fn add_renderable_option(
        &mut self,
        label: impl Into<String>,
        renderable: impl Renderable + 'static,
        id: Option<OptionId>,
        disabled: bool,
    ) {
        let was_empty = self.cursor.highlighted().is_none();
        self.items.push(OptionItem::Option {
            prompt: label.into(),
            content: Some(OptionContent::Renderable(std::sync::Arc::new(renderable))),
            id,
            disabled,
        });
        if was_empty && !disabled {
            self.cursor.set_highlighted(Some(self.items.len() - 1));
        }
    }

    /// Add a visual separator.
    pub fn add_separator(&mut self) {
        self.items.push(OptionItem::Separator);
    }

    /// Remove all items from the list.
    pub fn clear_options(&mut self) {
        self.items.clear();
        self.cursor.clear();
        self.offset = 0;
        self.hovered_index = None;
    }

    /// Number of items (including separators).
    pub fn option_count(&self) -> usize {
        self.items.len()
    }

    /// Get a reference to an item by index.
    pub fn get_option(&self, index: usize) -> Option<&OptionItem> {
        self.items.get(index)
    }

    /// The currently highlighted index, or `None`.
    pub fn highlighted(&self) -> Option<usize> {
        self.cursor.highlighted()
    }

    /// The currently hovered option index, or `None`.
    pub fn hovered_index(&self) -> Option<usize> {
        self.hovered_index
    }

    /// The current scroll offset (first visible item index).
    pub fn offset_for_click(&self) -> usize {
        self.offset
    }

    /// Programmatically move the highlight to `index`.
    /// Ignores separators and disabled items.
    pub fn set_highlighted(&mut self, index: usize) {
        if index < self.items.len() && self.items[index].is_selectable() {
            self.cursor.set_highlighted(Some(index));
            self.ensure_visible();
        }
    }

    /// Clear the current highlighted option.
    pub fn clear_highlighted(&mut self) {
        self.cursor.set_highlighted(None);
        self.ensure_visible();
    }

    /// Return the first selectable index, if any.
    pub fn first_selectable_index(&self) -> Option<usize> {
        self.first_selectable()
    }

    /// Replace all items at once.
    pub fn set_items(&mut self, items: Vec<OptionItem>) {
        self.items = items;
        self.cursor.set_highlighted(self.first_selectable());
        self.offset = 0;
        self.hovered_index = None;
        self.ensure_visible();
    }

    // ── Internals ───────────────────────────────────────────────────

    /// Render rich [`Text`] option content into one-or-more display lines, applying the
    /// resolved `line_style` as a base (for highlight/hover/disabled backgrounds)
    /// underneath the content's own styling.
    ///
    /// Multi-row renderables (for example a `rich` table pre-rendered into a `Text`
    /// containing newlines) are preserved as multiple lines — mirroring Python
    /// `OptionList`, where each option occupies as many lines as its visual height.
    fn render_rich_lines(
        &self,
        content: &Text,
        line_style: rich_rs::Style,
        width: usize,
        console: &Console,
        options: &ConsoleOptions,
    ) -> Vec<Vec<Segment>> {
        let content_width = width;
        let mut content_options = options.clone();
        content_options.size = (content_width, options.size.1.max(1));
        content_options.max_width = content_width;

        let rendered: Vec<Segment> = content
            .render(console, &content_options)
            .into_iter()
            .collect();

        // Split the rendered segments into display lines. Newlines arrive either as
        // line-control segments (when rich-rs wraps) or as `\n` characters embedded
        // inside a single text segment (the unwrapped fast path). Handle both so a
        // pre-rendered multi-row renderable (for example a table) keeps every line.
        // Each non-control segment merges `line_style` as a base so the
        // highlight/hover background paints across the whole option.
        let mut lines: Vec<Vec<Segment>> = Vec::new();
        let mut current: Vec<Segment> = Vec::new();
        for seg in &rendered {
            if seg.is_control() {
                lines.push(std::mem::take(&mut current));
                continue;
            }
            if seg.text.is_empty() {
                continue;
            }
            let merged = line_style.combine(&seg.style.unwrap_or_default());
            if seg.text.contains('\n') {
                let parts: Vec<&str> = seg.text.split('\n').collect();
                for (i, part) in parts.iter().enumerate() {
                    if i > 0 {
                        lines.push(std::mem::take(&mut current));
                    }
                    if !part.is_empty() {
                        current.push(Segment::styled((*part).to_string(), merged));
                    }
                }
            } else {
                current.push(Segment::styled(seg.text.clone(), merged));
            }
        }
        if !current.is_empty() || lines.is_empty() {
            lines.push(current);
        }

        lines
            .into_iter()
            .map(|line| adjust_line_length_no_bg(&line, width))
            .collect()
    }

    /// Render an arbitrary [`Renderable`] option content into display lines,
    /// applying `line_style` as a base for highlight/hover backgrounds.
    ///
    /// Used for `OptionContent::Renderable` items (tables, panels, etc.) where
    /// the content is rendered live at the runtime `width` rather than being
    /// pre-rendered into a `Text`.
    fn render_renderable_lines(
        &self,
        renderable: &dyn Renderable,
        line_style: rich_rs::Style,
        width: usize,
        console: &Console,
        options: &ConsoleOptions,
    ) -> Vec<Vec<Segment>> {
        let mut content_options = options.clone();
        content_options.size = (width, options.size.1.max(40).max(options.size.1));
        content_options.max_width = width;
        content_options.max_height = 40;

        let segs: Vec<Segment> = renderable.render(console, &content_options).into_iter().collect();
        let split = Segment::split_and_crop_lines(segs, width, None, true, false);
        if split.is_empty() {
            return vec![adjust_line_length_no_bg(&[], width)];
        }
        split
            .into_iter()
            .map(|line| {
                // Apply line_style as base for highlight/hover backgrounds.
                let styled: Vec<Segment> = line
                    .into_iter()
                    .map(|seg| {
                        if seg.control.is_some() {
                            return seg;
                        }
                        let merged = line_style.combine(&seg.style.unwrap_or_default());
                        Segment::styled(seg.text.clone(), merged)
                    })
                    .collect();
                adjust_line_length_no_bg(&styled, width)
            })
            .collect()
    }

    /// Number of display lines an option occupies.
    ///
    /// For `Text` content, the height is its newline-separated line count
    /// (width-independent). For `Renderable` content, we render at `layout_width`
    /// to count lines (matches Python's per-option height measurement). Plain
    /// options and separators are a single line.
    /// Number of display lines a plain prompt occupies when word-wrapped to the
    /// current (inset) option content width. Mirrors the render path's plain
    /// branch (`render_rich_lines` at the same width) so measured height and
    /// rendered lines never disagree.
    fn plain_wrapped_line_count(&self, prompt: &str) -> usize {
        if prompt.is_empty() {
            return 1;
        }
        let content_w = self.layout_width.saturating_sub(self.option_pad_left).max(1);
        let console = Console::new();
        let options = ConsoleOptions {
            size: (content_w, 100),
            max_width: content_w,
            max_height: 100,
            ..Default::default()
        };
        let text = rich_rs::Text::from(prompt);
        self.render_rich_lines(&text, rich_rs::Style::default(), content_w, &console, &options)
            .len()
            .max(1)
    }

    fn item_height(&self, item: &OptionItem) -> usize {
        match item {
            OptionItem::Separator => 1,
            OptionItem::Option { prompt, content, .. } => match content {
                Some(OptionContent::Text(text)) => text.plain_text().split('\n').count().max(1),
                Some(OptionContent::Renderable(r)) => {
                    let console = Console::new();
                    let width = self.layout_width.max(1);
                    let options = ConsoleOptions {
                        size: (width, 40),
                        max_width: width,
                        max_height: 40,
                        ..Default::default()
                    };
                    let segs = r.render(&console, &options);
                    Segment::split_lines(segs).len().max(1)
                }
                None => self.plain_wrapped_line_count(prompt),
            },
        }
    }

    /// Total display-line height of all items (sum of per-item heights).
    fn total_lines(&self) -> usize {
        self.items.iter().map(|item| self.item_height(item)).sum()
    }

    /// Widest option content in cells, excluding widget chrome (padding/border).
    /// Mirrors the inner-width computation used by [`Widget::content_width`].
    fn content_width_inner(&self) -> usize {
        self.items
            .iter()
            .map(|item| match item {
                OptionItem::Option {
                    prompt, content, ..
                } => {
                    let text_width = match content {
                        Some(OptionContent::Text(rich)) => rich.cell_len(),
                        Some(OptionContent::Renderable(r)) => {
                            // Measure renderable using rich_rs measure API.
                            let console = Console::new();
                            let options = ConsoleOptions::default();
                            rich_rs::Renderable::measure(r.as_ref(), &console, &options).maximum
                        }
                        None => rich_rs::cell_len(prompt),
                    };
                    text_width.saturating_add(2) // 2-char indent
                }
                OptionItem::Separator => 3,
            })
            .max()
            .unwrap_or(2)
            .max(1)
    }

    /// Flat list of `(item_index, line_offset)` entries, one per display line,
    /// in order. Mirrors Python `OptionList._lines`.
    fn line_map(&self) -> Vec<(usize, usize)> {
        let mut map = Vec::new();
        for (index, item) in self.items.iter().enumerate() {
            let height = self.item_height(item);
            for line_no in 0..height {
                map.push((index, line_no));
            }
        }
        map
    }

    /// First display line of `index` within the flattened line map.
    fn item_first_line(&self, index: usize) -> usize {
        self.items
            .iter()
            .take(index)
            .map(|item| self.item_height(item))
            .sum()
    }

    /// Map a viewport row (relative to the top of the visible area) to the item
    /// index whose content occupies that line, accounting for the line-based
    /// scroll offset and multi-line options.
    fn item_at_row(&self, row: usize) -> Option<usize> {
        let line = self.offset.saturating_add(row);
        self.line_map().get(line).map(|(index, _)| *index)
    }

    fn first_selectable(&self) -> Option<usize> {
        self.items.iter().position(|item| item.is_selectable())
    }

    fn last_selectable(&self) -> Option<usize> {
        self.items
            .iter()
            .enumerate()
            .rev()
            .find(|(_, item)| item.is_selectable())
            .map(|(i, _)| i)
    }

    /// Total number of *selectable* items (excludes separators and disabled items).
    fn selectable_count(&self) -> usize {
        self.items
            .iter()
            .filter(|item| item.is_selectable())
            .count()
    }

    fn max_offset(&self) -> usize {
        self.total_lines().saturating_sub(self.viewport_height.max(1))
    }

    fn clamp_offsets(&mut self) {
        if self.items.is_empty() {
            self.cursor.set_highlighted(None);
            self.offset = 0;
            self.hovered_index = None;
            return;
        }
        self.offset = self.offset.min(self.max_offset());
        if let Some(index) = self.hovered_index {
            if index >= self.items.len() {
                self.hovered_index = None;
            }
        }
    }

    fn ensure_visible(&mut self) {
        self.clamp_offsets();
        let Some(highlighted) = self.cursor.highlighted() else {
            return;
        };
        // Scroll in line space: keep the whole highlighted option visible.
        let viewport = self.viewport_height.max(1);
        let first_line = self.item_first_line(highlighted);
        let height = self
            .items
            .get(highlighted)
            .map(|item| self.item_height(item))
            .unwrap_or(1);
        let last_line = first_line + height.saturating_sub(1);
        if first_line < self.offset {
            self.offset = first_line;
        } else if last_line >= self.offset + viewport {
            self.offset = last_line + 1 - viewport;
        }
        self.offset = self.offset.min(self.max_offset());
    }

    fn emit_highlighted(&self, ctx: &mut crate::event::WidgetCtx) {
        if let Some(index) = self.cursor.highlighted() {
            ctx.post_message(OptionHighlighted { index });
        }
    }

    fn emit_selected(&self, ctx: &mut crate::event::WidgetCtx) {
        if let Some(index) = self.cursor.highlighted() {
            ctx.post_message(OptionSelected { index });
        }
    }

    /// Move highlight to a specific index. Skips separators and disabled items.
    fn highlight_index(&mut self, index: usize, ctx: &mut crate::event::WidgetCtx) {
        if index >= self.items.len() {
            return;
        }
        if !self.items[index].is_selectable() {
            return;
        }
        let changed = self.cursor.highlighted() != Some(index);
        self.cursor.set_highlighted(Some(index));
        self.ensure_visible();
        if changed {
            self.emit_highlighted(ctx);
            ctx.request_repaint();
        }
    }

    /// Move highlight by `delta`, skipping separators and disabled items.
    fn move_highlight(&mut self, delta: isize, ctx: &mut crate::event::WidgetCtx) {
        if self.selectable_count() == 0 {
            return;
        }
        if self.cursor.highlighted().is_none() {
            let target = if delta.is_negative() {
                self.last_selectable()
            } else {
                self.first_selectable()
            };
            if let Some(target) = target {
                self.highlight_index(target, ctx);
            }
            return;
        }
        let current = self.cursor.highlighted().unwrap_or(0) as isize;
        let max = (self.items.len() - 1) as isize;
        let mut target = (current + delta).clamp(0, max) as usize;

        // Walk in the direction of delta to find the next selectable item.
        let step: isize = if delta >= 0 { 1 } else { -1 };
        while target < self.items.len() && !self.items[target].is_selectable() {
            let next = target as isize + step;
            if next < 0 || next > max {
                // Can't move further; stay at current position.
                return;
            }
            target = next as usize;
        }
        self.highlight_index(target, ctx);
    }

    fn page_step(&self) -> usize {
        self.viewport_height.saturating_sub(1).max(1)
    }

    fn scroll_by_rows(&mut self, delta_rows: isize, ctx: &mut crate::event::WidgetCtx) {
        let before = self.offset;
        if delta_rows.is_negative() {
            self.offset = self.offset.saturating_sub(delta_rows.unsigned_abs());
        } else {
            self.offset = self.offset.saturating_add(delta_rows as usize);
        }
        self.offset = self.offset.min(self.max_offset());
        if self.offset != before {
            ctx.request_repaint();
            ctx.set_handled();
        }
    }

    /// Confirm the currently highlighted item (Enter or click).
    fn confirm_selection(&mut self, ctx: &mut crate::event::WidgetCtx) {
        let Some(index) = self.cursor.highlighted() else {
            return;
        };
        if !self.items[index].is_selectable() {
            return;
        }
        self.emit_selected(ctx);
    }
}

impl Widget for OptionList {
    fn focusable(&self) -> bool {
        !self.disabled
    }

    fn on_node_state_changed(
        &mut self,
        _old: crate::widgets::NodeState,
        new: crate::widgets::NodeState,
    ) {
        if !new.hovered {
            self.hovered_index = None;
        }
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.layout_width = usize::from(width).max(1);
        self.viewport_height = usize::from(height).max(1);
        self.ensure_visible();
    }

    fn on_event(&mut self, event: &Event, ctx: &mut crate::event::WidgetCtx) {
        if self.disabled {
            return;
        }
        match event {
            Event::MouseDown(mouse) if mouse.target == self.node_id() => {
                if let Some(index) = self.item_at_row(mouse.y as usize) {
                    if self.items[index].is_selectable() {
                        self.highlight_index(index, ctx);
                        self.confirm_selection(ctx);
                        ctx.set_handled();
                    }
                }
            }
            Event::Action(action) if self.node_state().focused => match action {
                Action::ScrollUp => {
                    self.move_highlight(-1, ctx);
                    ctx.set_handled();
                }
                Action::ScrollDown => {
                    self.move_highlight(1, ctx);
                    ctx.set_handled();
                }
                Action::ScrollPageUp => {
                    if self.cursor.highlighted().is_none() {
                        if let Some(first) = self.first_selectable() {
                            self.highlight_index(first, ctx);
                        }
                    } else {
                        self.move_highlight(-(self.page_step() as isize), ctx);
                    }
                    ctx.set_handled();
                }
                Action::ScrollPageDown => {
                    if self.cursor.highlighted().is_none() {
                        if let Some(last) = self.last_selectable() {
                            self.highlight_index(last, ctx);
                        }
                    } else {
                        self.move_highlight(self.page_step() as isize, ctx);
                    }
                    ctx.set_handled();
                }
                _ => {}
            },
            Event::Key(key) if self.node_state().focused => match key.code {
                KeyCode::Up => {
                    self.move_highlight(-1, ctx);
                    ctx.set_handled();
                }
                KeyCode::Down => {
                    self.move_highlight(1, ctx);
                    ctx.set_handled();
                }
                KeyCode::PageUp => {
                    if self.cursor.highlighted().is_none() {
                        if let Some(first) = self.first_selectable() {
                            self.highlight_index(first, ctx);
                        }
                    } else {
                        self.move_highlight(-(self.page_step() as isize), ctx);
                    }
                    ctx.set_handled();
                }
                KeyCode::PageDown => {
                    if self.cursor.highlighted().is_none() {
                        if let Some(last) = self.last_selectable() {
                            self.highlight_index(last, ctx);
                        }
                    } else {
                        self.move_highlight(self.page_step() as isize, ctx);
                    }
                    ctx.set_handled();
                }
                KeyCode::Home => {
                    if let Some(first) = self.first_selectable() {
                        self.highlight_index(first, ctx);
                    }
                    ctx.set_handled();
                }
                KeyCode::End => {
                    if let Some(last) = self.last_selectable() {
                        self.highlight_index(last, ctx);
                    }
                    ctx.set_handled();
                }
                KeyCode::Enter => {
                    self.confirm_selection(ctx);
                    ctx.set_handled();
                }
                _ => {}
            },
            Event::AppFocus(false)
                if (self.node_state().hovered || self.hovered_index.is_some()) => {
                    self.hovered_index = None;
                    ctx.request_repaint();
                }
            _ => {}
        }
    }

    fn on_mouse_move(&mut self, _x: u16, y: u16) -> bool {
        if self.disabled {
            return false;
        }
        if self.items.is_empty() {
            return false;
        }
        let hovered = match self.item_at_row(y as usize) {
            Some(index) if self.items[index].is_selectable() => Some(index),
            _ => None,
        };
        if hovered != self.hovered_index {
            self.hovered_index = hovered;
            return true;
        }
        false
    }

    fn on_mouse_scroll(&mut self, _delta_x: i32, delta_y: i32, ctx: &mut crate::event::WidgetCtx) {
        if self.disabled {
            return;
        }
        if delta_y == 0 {
            return;
        }
        self.scroll_by_rows(
            delta_y.saturating_mul(self.scroll_step as i32) as isize,
            ctx,
        );
    }

    fn on_unmount(&mut self) {
        self.hovered_index = None;
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);
        let mut out = Segments::new();

        // When the vertical scrollbar is visible (content overflows viewport), it
        // occupies the rightmost SCROLLBAR_THICKNESS columns. Renderable items
        // (e.g. Tables with expand=True) must be rendered at a narrower width so
        // they don't bleed into the scrollbar zone — Python uses
        // `scrollable_content_region.width` which already subtracts the
        // scrollbar_size_vertical (default 2). Text items use `width` as before
        // because `adjust_line_length_no_bg` pads with spaces that the scrollbar
        // overlay naturally overwrites.
        let scrollbar_visible = self.total_lines() > height;
        let renderable_width = if scrollbar_visible {
            width.saturating_sub(SCROLLBAR_THICKNESS)
        } else {
            width
        };

        // The OptionList's own composited surface (its `$surface` bg plus the
        // `:focus` `background-tint`, if focused). Python composes option text
        // and separator colours over this surface (`background_colors`), so all
        // semi-transparent foregrounds (`$foreground 15%` separators,
        // `$text-disabled`) and the highlighted `$block-cursor(-blurred)`
        // background must flatten over it — resolving them standalone would
        // flatten over `$background`/black and shift the result.
        let surface_bg = crate::css::current_composited_background();
        let surface_flat = surface_bg.unwrap_or_else(|| {
            crate::style::parse_color_like("$surface")
                .unwrap_or(crate::style::Color::rgb(0, 0, 0))
        });

        // Resolve an option component style. Uses an empty-type leaf meta so
        // only the component-class rules (`.option-list--option*`,
        // `.option-list--separator`) match — NOT the `OptionList { background:
        // $surface }` base rule, which would otherwise stamp an opaque surface
        // background on every component and cause semi-transparent foregrounds
        // (separators, disabled) to flatten over the untinted surface. The
        // OptionList node meta is already on the selector stack (pushed by
        // `render_widget_with_meta`), so `OptionList:focus > .X` descendant
        // rules still resolve, exactly like Python's `get_visual_style`.
        let resolve_comp = |classes: &[&str]| -> crate::style::Style {
            crate::css::resolve_style_for_meta(&crate::css::selector_meta_component("", classes))
        };

        let base_style = resolve_comp(&["option-list--option"])
            .to_rich_over(surface_flat)
            .unwrap_or_default();

        // Flatten options into display lines and render the visible window in
        // line space (Python parity: a multi-row option occupies multiple lines).
        let line_map = self.line_map();

        // Render each item's lines once, on first reference, then index into them.
        let mut rendered_items: std::collections::HashMap<usize, Vec<Vec<Segment>>> =
            std::collections::HashMap::new();

        for row in 0..height {
            let line_index = self.offset + row;

            let line = match line_map.get(line_index) {
                None => {
                    // Past the last line — emit an empty padded line.
                    adjust_line_length_no_bg(&[], width)
                }
                Some(&(index, line_offset)) => {
                    let item = &self.items[index];
                    match item {
                        OptionItem::Separator => adjust_line_length_no_bg(
                            &[Segment::styled(
                                "─".repeat(width),
                                resolve_comp(&["option-list--separator"])
                                    .to_rich_over(surface_flat)
                                    .unwrap_or(base_style),
                            )],
                            width,
                        ),
                        OptionItem::Option {
                            prompt,
                            content,
                            disabled,
                            ..
                        } => {
                            let highlighted = self.cursor.highlighted() == Some(index);
                            let hovered = self.hovered_index == Some(index);
                            // Mirror Python `_get_option_style`: combine the base
                            // `option-list--option` with the state-specific
                            // component class (disabled > highlighted > hover).
                            // The `:focus` variant of the highlighted colours is
                            // supplied by the `OptionList:focus > ...` CSS rule,
                            // matched via the focused OptionList meta on the stack.
                            let mut classes = vec!["option-list--option"];
                            if *disabled {
                                classes.push("option-list--option-disabled");
                            } else if highlighted {
                                classes.push("option-list--option-highlighted");
                            } else if hovered {
                                classes.push("option-list--option-hover");
                            }
                            let mut style_crate = resolve_comp(&classes);
                            // Resolve an auto-contrast foreground (e.g.
                            // `$text-disabled` = `auto 38%`) against the widget
                            // surface. `to_rich_over` only handles concrete `fg`;
                            // the compositor resolves `fg_auto` but only from the
                            // WIDGET style, not per-option component styles, so a
                            // disabled option would otherwise fall back to the
                            // widget foreground. Mirror the compositor's math over
                            // the (tinted) surface.
                            if style_crate.fg.is_none() {
                                if let Some(auto) = style_crate.fg_auto {
                                    let contrast = crate::style::contrast_text(surface_flat);
                                    style_crate.fg =
                                        Some(contrast.blend_over_float(surface_flat, auto.alpha()));
                                    style_crate.fg_auto = None;
                                }
                            }
                            // Compose the highlighted option's (possibly
                            // semi-transparent) background over the widget surface,
                            // matching Python's `background_colors` compositing.
                            if highlighted {
                                if let Some(bg) = style_crate.bg {
                                    style_crate.bg = Some(bg.flatten_over(surface_flat));
                                }
                            }
                            // Per-option left inset (Python `.option-list--option
                            // { padding }`), inside the option background and on top
                            // of the container padding. Sourced from the explicit
                            // `option_pad_left` field so it agrees with the wrap
                            // width used in `item_height`.
                            let pad_left = self.option_pad_left;
                            let content_w = width.saturating_sub(pad_left).max(1);
                            let style = style_crate.to_rich_over(surface_flat).unwrap_or(base_style);

                            let lines = rendered_items.entry(index).or_insert_with(|| {
                                let mut raw = match content {
                                    Some(OptionContent::Text(rich)) => {
                                        self.render_rich_lines(rich, style, content_w, console, options)
                                    }
                                    Some(OptionContent::Renderable(r)) => {
                                        // Render at renderable_width (< width when scrollbar
                                        // is visible) so the table/renderable doesn't bleed
                                        // into the scrollbar overlay zone. Python uses
                                        // scrollable_content_region.width which already
                                        // subtracts scrollbar_size_vertical (default 2).
                                        let rw = renderable_width.saturating_sub(pad_left).max(1);
                                        self.render_renderable_lines(r.as_ref(), style, rw, console, options)
                                    }
                                    None => {
                                        // Plain text, word-wrapped to the (inset)
                                        // content width (Python OptionList wraps long
                                        // prompts). Routed through the rich-text line
                                        // renderer so wrapping matches `item_height`'s
                                        // measurement exactly.
                                        let text = rich_rs::Text::from(prompt.as_str());
                                        self.render_rich_lines(&text, style, content_w, console, options)
                                    }
                                };
                                // Prepend the option's left padding as styled blanks so
                                // the (highlighted) option background covers the inset,
                                // bringing each line back up to full `width`.
                                if pad_left > 0 {
                                    let indent = Segment::styled(" ".repeat(pad_left), style);
                                    for line in raw.iter_mut() {
                                        line.insert(0, indent.clone());
                                    }
                                }
                                // The highlighted option paints its opaque
                                // `$block-cursor` background across the full width
                                // and must not be re-tinted by the widget style
                                // pass; rebuild + tag `no_style` per line.
                                if highlighted {
                                    raw.into_iter()
                                        .map(|line| finalize_highlight_line(&line, width, style))
                                        .collect()
                                } else {
                                    raw
                                }
                            });
                            lines
                                .get(line_offset)
                                .cloned()
                                .unwrap_or_else(|| adjust_line_length_no_bg(&[], width))
                        }
                    }
                }
            };
            out.extend(line);

            if row + 1 < height {
                out.push(Segment::line());
            }
        }

        out
    }

    fn layout_height(&self) -> Option<usize> {
        // Sum of per-option display-line heights (multi-row options count fully).
        Some(self.total_lines().max(1))
    }

    fn content_width(&self) -> Option<usize> {
        let content_width = self.content_width_inner();
        let meta = crate::css::selector_meta_generic(self);
        let resolved = crate::css::resolve_style(self, &meta);
        let padding = resolved.effective_padding();
        let (_, _, border_left, border_right) =
            super::helpers::border_spacing_from_style(&resolved);
        let chrome_lr =
            usize::from(padding.left.saturating_add(padding.right)) + border_left + border_right;
        Some(content_width.saturating_add(chrome_lr).max(1))
    }

    fn style_type(&self) -> &'static str {
        "OptionList"
    }

    fn set_inline_style(&mut self, style: crate::style::Style) {
        self.seed.styles.style = style;
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }

    fn compose(&mut self) -> crate::compose::ComposeResult {
        if self.scrollbar_extracted {
            return Vec::new();
        }
        self.scrollbar_extracted = true;
        let mut vbar = ScrollBar::new(true, 2);
        vbar.seed.css_id = Some(OPTION_LIST_VSCROLLBAR_ID.to_string());
        vec![crate::compose::ChildDecl::new(Box::new(vbar))]
    }

    fn on_message(&mut self, event: &MessageEvent, ctx: &mut crate::event::WidgetCtx) {
        let Some(payload) = event.downcast_ref::<ScrollbarScrollTo>() else {
            return;
        };
        if payload.axis != ScrollbarAxis::Vertical {
            return;
        }
        let next = (payload.offset.max(0.0).round() as usize).min(self.max_offset());
        if next != self.offset {
            self.offset = next;
            ctx.request_repaint();
        }
        ctx.set_handled();
    }

    fn scroll_offset(&self) -> (usize, usize) {
        (0, self.offset)
    }

    fn scroll_offset_f32(&self) -> (f32, f32) {
        (0.0, self.offset as f32)
    }

    fn scroll_virtual_content_size(&self) -> Option<(usize, usize)> {
        // Width: the widest option content (no chrome) so the host reserves a
        // horizontal lane only on genuine overflow. Height: the total flattened
        // display-line count so a vertical lane appears when the list overflows
        // the viewport. OptionList renders its own content (no child widgets),
        // so the host falls back to this for the virtual extent.
        Some((self.content_width_inner().max(1), self.total_lines().max(1)))
    }
}

impl Renderable for OptionList {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EventCtx;
    use crate::node_id::NodeId;
    use crate::runtime::dispatch_ctx::set_dispatch_recipient;
    use crate::widgets::NodeState;

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

    /// Dim text under the block cursor keeps its dimming as a pre-blended
    /// COLOUR (Python `ANSIToTruecolor`/`dim_color`: `bg + (fg - bg) * 0.66`),
    /// with the `dim` attribute stripped — the `Select` overlay blank-prompt
    /// row on the highlighted line. #ddedf9 over #0178d4 must give #92c5ec
    /// (the exact Python value), never full-strength fg + SGR dim.
    #[test]
    fn finalize_highlight_line_pre_blends_dim_fg_over_cursor_bg() {
        let fill = rich_rs::Style::new()
            .with_color(rich_rs::SimpleColor::Rgb {
                r: 0xdd,
                g: 0xed,
                b: 0xf9,
            })
            .with_bgcolor(rich_rs::SimpleColor::Rgb {
                r: 0x01,
                g: 0x78,
                b: 0xd4,
            });
        let mut dim = rich_rs::Style::new();
        dim.dim = Some(true);
        let line = [Segment::styled("Select".to_string(), dim)];
        let out = finalize_highlight_line(&line, 10, fill);

        let glyphs = &out[0];
        assert_eq!(glyphs.text, "Select");
        let style = glyphs.style.expect("styled");
        assert_eq!(
            style.color,
            Some(rich_rs::SimpleColor::Rgb {
                r: 0x92,
                g: 0xc5,
                b: 0xec
            }),
            "dim fg must pre-blend toward the cursor bg at DIM_FACTOR 0.66"
        );
        assert_ne!(style.dim, Some(true), "dim attribute must be stripped");
        assert_eq!(style.bgcolor, fill.bgcolor);

        // A non-dim segment keeps the cursor fg at full strength.
        let plain = [Segment::styled(
            "Select".to_string(),
            rich_rs::Style::new(),
        )];
        let out = finalize_highlight_line(&plain, 10, fill);
        assert_eq!(out[0].style.expect("styled").color, fill.color);
    }

    #[test]
    fn option_list_navigation_skips_separators() {
        let items = vec![
            OptionItem::new("Alpha"),
            OptionItem::Separator,
            OptionItem::new("Beta"),
        ];
        let mut list = OptionList::with_items(items);
        let id = make_node_id();
        let _guard = set_dispatch_recipient(id, focused_state());
        list.on_layout(40, 10);

        assert_eq!(list.highlighted(), Some(0));

        let mut ctx = EventCtx::default();
        { let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx); list.move_highlight(1, &mut __w) };
        // Should skip the separator and land on Beta (index 2).
        assert_eq!(list.highlighted(), Some(2));
    }

    #[test]
    fn option_list_navigation_skips_disabled() {
        let items = vec![
            OptionItem::new("Alpha"),
            OptionItem::disabled("Bravo"),
            OptionItem::new("Charlie"),
        ];
        let mut list = OptionList::with_items(items);
        list.on_layout(40, 10);

        assert_eq!(list.highlighted(), Some(0));

        let mut ctx = EventCtx::default();
        { let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx); list.move_highlight(1, &mut __w) };
        assert_eq!(list.highlighted(), Some(2));
    }

    #[test]
    fn option_list_home_end() {
        let items = vec![
            OptionItem::new("First"),
            OptionItem::new("Middle"),
            OptionItem::Separator,
            OptionItem::new("Last"),
        ];
        let mut list = OptionList::with_items(items);
        list.on_layout(40, 10);

        // End goes to last selectable
        let mut ctx = EventCtx::default();
        if let Some(last) = list.last_selectable() {
            { let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx); list.highlight_index(last, &mut __w) };
        }
        assert_eq!(list.highlighted(), Some(3));

        // Home goes to first selectable
        if let Some(first) = list.first_selectable() {
            { let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx); list.highlight_index(first, &mut __w) };
        }
        assert_eq!(list.highlighted(), Some(0));
    }

    #[test]
    fn option_list_confirm_emits_selected() {
        let items = vec![OptionItem::new("Only")];
        let mut list = OptionList::with_items(items);
        list.on_layout(40, 10);

        let mut ctx = EventCtx::default();
        { let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx); list.confirm_selection(&mut __w) };
        let messages = ctx.take_messages();
        assert!(messages.iter().any(|m| {
            m.downcast_ref::<OptionSelected>()
                .is_some_and(|s| s.index == 0)
        }));
    }

    #[test]
    fn option_list_mouse_click_emits_highlighted_before_selected() {
        let items = vec![OptionItem::new("Alpha"), OptionItem::new("Beta")];
        let mut list = OptionList::with_items(items);
        list.on_layout(40, 10);

        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            list.on_event(
            &Event::MouseDown(crate::event::MouseDownEvent {
                target: NodeId::default(),
                screen_x: 0,
                screen_y: 1,
                x: 0,
                y: 1,
            }),
            &mut __w);
        }

        assert!(ctx.handled());
        let messages = ctx.take_messages();
        let highlighted_pos = messages.iter().position(|m| {
            m.downcast_ref::<OptionHighlighted>()
                .is_some_and(|h| h.index == 1)
        });
        let selected_pos = messages.iter().position(|m| {
            m.downcast_ref::<OptionSelected>()
                .is_some_and(|s| s.index == 1)
        });
        assert!(
            highlighted_pos.is_some() && selected_pos.is_some() && highlighted_pos < selected_pos
        );
    }

    #[test]
    fn option_list_clear_resets_state() {
        let items = vec![OptionItem::new("A"), OptionItem::new("B")];
        let mut list = OptionList::with_items(items);
        list.set_highlighted(1);
        list.clear_options();

        assert_eq!(list.highlighted(), None);
        assert_eq!(list.option_count(), 0);
    }

    #[test]
    fn option_item_with_typed_id_round_trips() {
        let item = OptionItem::with_id("Alpha", "alpha");
        assert_eq!(item.string_id(), Some("alpha"));
    }

    #[test]
    fn option_list_up_from_none_selects_last_enabled() {
        let items = vec![
            OptionItem::new("Alpha"),
            OptionItem::disabled("Bravo"),
            OptionItem::new("Charlie"),
        ];
        let mut list = OptionList::with_items(items);
        list.cursor.set_highlighted(None);
        list.on_layout(40, 10);

        let mut ctx = EventCtx::default();
        { let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx); list.move_highlight(-1, &mut __w) };
        assert_eq!(list.highlighted(), Some(2));
    }

    #[test]
    fn option_list_page_down_from_none_selects_last_enabled() {
        let items = vec![
            OptionItem::new("Alpha"),
            OptionItem::disabled("Bravo"),
            OptionItem::new("Charlie"),
        ];
        let mut list = OptionList::with_items(items);
        list.cursor.set_highlighted(None);
        list.on_layout(40, 10);

        let id = make_node_id();
        let _guard = set_dispatch_recipient(id, focused_state());
        let key = crate::keys::KeyEventData::from_crossterm(crossterm::event::KeyEvent::new(
            KeyCode::PageDown,
            crossterm::event::KeyModifiers::NONE,
        ));
        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            list.on_event(&Event::Key(key), &mut __w);
        }
        assert_eq!(list.highlighted(), Some(2));
        assert!(ctx.handled());
    }

    #[test]
    fn option_list_disabled_ignores_input() {
        let items = vec![OptionItem::new("Alpha"), OptionItem::new("Beta")];
        let mut list = OptionList::with_items(items).disabled(true);
        list.on_layout(40, 10);

        let before = list.highlighted();
        let id = make_node_id();
        let _guard = set_dispatch_recipient(id, focused_state());
        let key = crate::keys::KeyEventData::from_crossterm(crossterm::event::KeyEvent::new(
            KeyCode::Down,
            crossterm::event::KeyModifiers::NONE,
        ));
        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            list.on_event(&Event::Key(key), &mut __w);
        }

        assert_eq!(list.highlighted(), before);
        assert!(!ctx.handled());
        assert!(!list.focusable());
    }

    #[test]
    fn app_focus_loss_clears_hover_state() {
        let items = vec![OptionItem::new("Alpha"), OptionItem::new("Beta")];
        let mut list = OptionList::with_items(items);
        // on_mouse_move sets hovered_index; AppFocus(false) should clear it.
        assert!(list.on_mouse_move(0, 0));

        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            list.on_event(&Event::AppFocus(false), &mut __w);
        }

        assert!(list.hovered_index.is_none());
        assert!(ctx.repaint_requested());
    }

    // ── WP-21: Rich content support ──────────────────────────────────

    #[test]
    fn option_item_rich_stores_content() {
        let content = rich_rs::Text::plain("Bold option");
        let item = OptionItem::rich("Bold option", content);
        assert!(item.content().is_some());
        assert_eq!(item.prompt(), Some("Bold option"));
        assert!(item.is_selectable());
    }

    #[test]
    fn option_item_with_content_builder() {
        let item = OptionItem::new("Plain").with_content(rich_rs::Text::plain("Rich"));
        assert!(item.content().is_some());
        assert_eq!(item.text_content().map(|t| t.plain_text()).as_deref(), Some("Rich"));
    }

    #[test]
    fn option_item_rich_with_id_stores_both() {
        let content = rich_rs::Text::plain("Styled");
        let item = OptionItem::rich_with_id("label", content, "my-id");
        assert_eq!(item.string_id(), Some("my-id"));
        assert!(item.content().is_some());
    }

    #[test]
    fn option_list_add_rich_option() {
        let mut list = OptionList::new();
        list.add_rich_option(
            "Bold",
            rich_rs::Text::plain("Bold"),
            Some(OptionId::new("b")),
            false,
        );
        assert_eq!(list.option_count(), 1);
        assert!(list.get_option(0).unwrap().content().is_some());
        assert_eq!(list.highlighted(), Some(0));
    }

    #[test]
    fn option_list_content_width_uses_rich_content() {
        let content = rich_rs::Text::plain("Long rich content");
        let items = vec![OptionItem::new("Short"), OptionItem::rich("Label", content)];
        let list = OptionList::with_items(items);
        let cw = list.content_width().unwrap();
        // "Long rich content" = 17 chars + 2 indent = 19
        assert_eq!(cw, 19);
    }

    #[test]
    fn option_list_rich_render_produces_segments() {
        let content = rich_rs::Text::plain("Hello");
        let items = vec![OptionItem::rich("Hello", content)];
        let list = OptionList::with_items(items);
        let console = rich_rs::Console::new();
        let options = rich_rs::ConsoleOptions {
            size: (20, 1),
            max_width: 20,
            max_height: 1,
            ..Default::default()
        };
        let segments: Vec<_> = Widget::render(&list, &console, &options)
            .into_iter()
            .collect();
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(text.contains("Hello"));
    }

    // ── WP-27: Line-based virtual scrolling ──────────────────────────

    #[test]
    fn total_lines_counts_single_line_options() {
        let items = vec![OptionItem::new("A"), OptionItem::new("B")];
        let list = OptionList::with_items(items);
        assert_eq!(list.total_lines(), 2);
    }

    #[test]
    fn line_offset_keeps_highlighted_option_visible() {
        let items: Vec<_> = (0..20)
            .map(|i| OptionItem::new(format!("Item {i}")))
            .collect();
        let mut list = OptionList::with_items(items);
        list.on_layout(40, 5);
        list.set_highlighted(10);
        // The highlighted option's line must lie within the visible line window.
        let first = list.item_first_line(10);
        assert!(first >= list.offset);
        assert!(first < list.offset + 5);
    }

    // ── Multi-row options (Python parity: each option spans its visual height) ──

    #[test]
    fn multi_line_rich_option_height_is_line_count() {
        let content = rich_rs::Text::plain("line1\nline2\nline3");
        let items = vec![OptionItem::rich("label", content)];
        let list = OptionList::with_items(items);
        assert_eq!(list.total_lines(), 3);
        assert_eq!(list.layout_height(), Some(3));
    }

    #[test]
    fn multi_line_rich_option_renders_all_lines() {
        let content = rich_rs::Text::plain("alpha\nbravo\ncharlie");
        let items = vec![OptionItem::rich("label", content)];
        let list = OptionList::with_items(items);
        let console = rich_rs::Console::new();
        let options = rich_rs::ConsoleOptions {
            size: (20, 3),
            max_width: 20,
            max_height: 3,
            ..Default::default()
        };
        let segments: Vec<_> = Widget::render(&list, &console, &options)
            .into_iter()
            .collect();
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(text.contains("alpha"), "missing first line");
        assert!(text.contains("bravo"), "missing second line");
        assert!(text.contains("charlie"), "missing third line");
    }

    #[test]
    fn line_map_flattens_multi_row_options() {
        let items = vec![
            OptionItem::new("single"),
            OptionItem::rich("multi", rich_rs::Text::plain("a\nb\nc")),
            OptionItem::new("last"),
        ];
        let list = OptionList::with_items(items);
        let map = list.line_map();
        assert_eq!(map.len(), 5); // 1 + 3 + 1
        assert_eq!(map[0], (0, 0));
        assert_eq!(map[1], (1, 0));
        assert_eq!(map[3], (1, 2));
        assert_eq!(map[4], (2, 0));
    }

    // ── OptionItem equality ignores content ──────────────────────────

    #[test]
    fn option_item_eq_ignores_rich_content() {
        let a = OptionItem::new("Hello");
        let b = OptionItem::new("Hello").with_content(rich_rs::Text::plain("Hello"));
        assert_eq!(a, b);
    }

    // ── P1-14 dispatch-context regression tests ─────────────────────────

    #[test]
    fn mouse_click_with_dispatch_context_is_handled() {
        let items = vec![OptionItem::new("Alpha"), OptionItem::new("Beta")];
        let mut list = OptionList::with_items(items);
        list.on_layout(40, 10);

        let id = make_node_id();
        let _guard = set_dispatch_recipient(id, NodeState::default());

        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            list.on_event(
            &Event::MouseDown(crate::event::MouseDownEvent {
                target: id,
                screen_x: 0,
                screen_y: 1,
                x: 0,
                y: 1,
            }),
            &mut __w);
        }
        assert!(ctx.handled());
    }

    #[test]
    fn mouse_click_with_wrong_target_is_ignored() {
        use slotmap::SlotMap;

        let items = vec![OptionItem::new("Alpha"), OptionItem::new("Beta")];
        let mut list = OptionList::with_items(items);
        list.on_layout(40, 10);

        let mut sm: SlotMap<NodeId, ()> = SlotMap::new();
        let my_id = sm.insert(());
        let other_id = sm.insert(());
        let _guard = set_dispatch_recipient(my_id, NodeState::default());

        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            list.on_event(
            &Event::MouseDown(crate::event::MouseDownEvent {
                target: other_id,
                screen_x: 0,
                screen_y: 1,
                x: 0,
                y: 1,
            }),
            &mut __w);
        }
        assert!(!ctx.handled());
    }

    // ── Dedicated vertical scrollbar (host-scrollbar path, mirrors RichLog) ──

    #[test]
    fn tree_mode_extracts_dedicated_scrollbar_child() {
        let mut list = OptionList::with_items(vec![OptionItem::new("A")]);
        let mut children = list.compose();
        assert_eq!(children.len(), 1);
        assert_eq!(
            children[0].widget_mut().take_node_seed().css_id.as_deref(),
            Some(OPTION_LIST_VSCROLLBAR_ID)
        );
        // Extraction is idempotent: a second call yields no further children.
        assert!(list.compose().is_empty());
    }

    #[test]
    fn scroll_virtual_content_size_reports_total_lines() {
        let items: Vec<_> = (0..20)
            .map(|i| OptionItem::new(format!("Item {i}")))
            .collect();
        let list = OptionList::with_items(items);
        let (_w, h) = list.scroll_virtual_content_size().unwrap();
        assert_eq!(h, 20);
    }

    #[test]
    fn scrollbar_message_updates_offset() {
        let items: Vec<_> = (0..20)
            .map(|i| OptionItem::new(format!("Item {i}")))
            .collect();
        let mut list = OptionList::with_items(items);
        list.on_layout(40, 5); // viewport 5 lines, 20 lines total -> overflow

        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            list.on_message(
            &MessageEvent::new(
                NodeId::default(),
                ScrollbarScrollTo {
                    axis: ScrollbarAxis::Vertical,
                    offset: 4.0,
                    animate: false,
                    scroll_duration: None,
                },
            ),
            &mut __w);
        }
        assert!(ctx.handled());
        assert_eq!(list.offset_for_click(), 4);
        assert_eq!(list.scroll_offset_f32(), (0.0, 4.0));
    }

    #[test]
    fn scrollbar_message_clamps_to_max_offset() {
        let items: Vec<_> = (0..10)
            .map(|i| OptionItem::new(format!("Item {i}")))
            .collect();
        let mut list = OptionList::with_items(items);
        list.on_layout(40, 6); // max offset = 10 - 6 = 4

        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            list.on_message(
            &MessageEvent::new(
                NodeId::default(),
                ScrollbarScrollTo {
                    axis: ScrollbarAxis::Vertical,
                    offset: 999.0,
                    animate: false,
                    scroll_duration: None,
                },
            ),
            &mut __w);
        }
        assert_eq!(list.offset_for_click(), 4);
    }

    #[test]
    fn scrollbar_message_ignores_horizontal_axis() {
        let mut list = OptionList::with_items(vec![OptionItem::new("A"), OptionItem::new("B")]);
        list.on_layout(40, 5);
        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            list.on_message(
            &MessageEvent::new(
                NodeId::default(),
                ScrollbarScrollTo {
                    axis: ScrollbarAxis::Horizontal,
                    offset: 1.0,
                    animate: false,
                    scroll_duration: None,
                },
            ),
            &mut __w);
        }
        assert!(!ctx.handled());
        assert_eq!(list.offset_for_click(), 0);
    }
}
