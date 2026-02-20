use rich_rs::markdown::Markdown as RichMarkdown;
use rich_rs::{Console, ConsoleOptions, Renderable, Segments, Text};
use std::collections::VecDeque;
use std::sync::RwLock;

use crate::render::FrameBuffer;
use crate::style::{Color, HorizontalAlign, parse_color_like};

use super::{Widget, WidgetSelectionAnchor, WidgetStyles, helpers::fixed_height_from_constraints};

/// Visual variant for a [`Label`], which adds a CSS class like `label--success`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelVariant {
    Success,
    Error,
    Warning,
    Primary,
    Secondary,
    Accent,
}

impl LabelVariant {
    fn css_class(self) -> &'static str {
        match self {
            LabelVariant::Success => "label--success",
            LabelVariant::Error => "label--error",
            LabelVariant::Warning => "label--warning",
            LabelVariant::Primary => "label--primary",
            LabelVariant::Secondary => "label--secondary",
            LabelVariant::Accent => "label--accent",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Label {
    id: Option<String>,
    text: String,
    wrap: bool,
    markup: bool,
    expand: bool,
    shrink: bool,
    layout_width: usize,
    variant: Option<LabelVariant>,
    classes: Vec<String>,
    styles: WidgetStyles,
}

impl Label {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            id: None,
            text: text.into(),
            wrap: true,
            markup: false,
            expand: false,
            // Match Textual Label defaults: labels don't shrink to intrinsic width
            // unless explicitly requested.
            shrink: false,
            layout_width: 0,
            variant: None,
            classes: vec!["label".to_string()],
            styles: WidgetStyles::default(),
        }
    }

    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
    }

    pub fn wrap(mut self, wrap: bool) -> Self {
        self.wrap = wrap;
        self
    }

    /// Enable or disable Rich markup parsing for this label's text content.
    pub fn with_markup(mut self, markup: bool) -> Self {
        self.markup = markup;
        self
    }

    /// When true, the widget expands to fill the available width.
    pub fn with_expand(mut self, expand: bool) -> Self {
        self.expand = expand;
        self
    }

    /// When true, the widget shrinks to its content width (default: false).
    pub fn with_shrink(mut self, shrink: bool) -> Self {
        self.shrink = shrink;
        self
    }

    /// Set the visual variant, adding a CSS class like `label--success`.
    pub fn with_variant(mut self, variant: LabelVariant) -> Self {
        self.variant = Some(variant);
        self.rebuild_classes();
        self
    }

    /// Get the current variant, if any.
    pub fn variant(&self) -> Option<LabelVariant> {
        self.variant
    }

    /// Set the variant at runtime.
    pub fn set_variant(&mut self, variant: Option<LabelVariant>) {
        self.variant = variant;
        self.rebuild_classes();
    }

    fn rebuild_classes(&mut self) {
        self.classes = vec!["label".to_string()];
        if let Some(v) = self.variant {
            self.classes.push(v.css_class().to_string());
        }
    }

    fn intrinsic_height(&self) -> usize {
        let width = self.layout_width;
        let mut lines = 0usize;
        for line in self.text.lines() {
            if self.wrap && width > 0 {
                let len = rich_rs::cell_len(line);
                lines += len.div_ceil(width).max(1);
            } else {
                lines += 1;
            }
        }
        lines.max(1)
    }

    fn intrinsic_content_width(&self) -> usize {
        self.text
            .lines()
            .map(rich_rs::cell_len)
            .max()
            .unwrap_or(0)
            .max(1)
    }
}

impl Widget for Label {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        if self.markup {
            let rendered = console.render_str(&self.text, Some(true), None, None, None);
            rendered.render(console, options)
        } else {
            let text = Text::plain(self.text.clone());
            text.render(console, options)
        }
    }

    fn on_layout(&mut self, width: u16, _height: u16) {
        // Hidden/disconnected nodes can transiently receive width=0/1 during
        // tree display toggles. Keep the last stable width (>1) so wrapped-height
        // calculations remain stable across tab switches.
        if width > 1 {
            self.layout_width = usize::from(width);
        }
    }

    fn content_width(&self) -> Option<usize> {
        if self.expand {
            // No intrinsic width constraint — fill available space.
            None
        } else if self.shrink {
            Some(self.intrinsic_content_width())
        } else {
            // Neither expand nor shrink — no width hint.
            None
        }
    }

    fn layout_height(&self) -> Option<usize> {
        fixed_height_from_constraints(self.layout_constraints()).or(Some(self.intrinsic_height()))
    }

    fn style_classes(&self) -> &[String] {
        &self.classes
    }

    fn style_id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

impl Renderable for Label {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[derive(Debug)]
pub struct Markdown {
    markup: String,
    id: Option<String>,
    layout_width: usize,
    selection: Option<(WidgetSelectionAnchor, WidgetSelectionAnchor)>,
    render_cache: RwLock<MarkdownRenderCache>,
    styles: WidgetStyles,
}

impl Clone for Markdown {
    fn clone(&self) -> Self {
        let cached = self
            .render_cache
            .read()
            .map(|cache| cache.clone())
            .unwrap_or_default();
        Self {
            markup: self.markup.clone(),
            id: self.id.clone(),
            layout_width: self.layout_width,
            selection: self.selection,
            render_cache: RwLock::new(cached),
            styles: self.styles.clone(),
        }
    }
}

#[derive(Debug, Clone, Default)]
struct MarkdownRenderCache {
    width: usize,
    lines: Vec<String>,
    content_bounds: Vec<(usize, usize)>,
}

impl Markdown {
    pub fn new(markup: impl Into<String>) -> Self {
        Self {
            markup: markup.into(),
            id: None,
            layout_width: 0,
            selection: None,
            render_cache: RwLock::new(MarkdownRenderCache::default()),
            styles: WidgetStyles::default(),
        }
    }

    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    pub fn set_markup(&mut self, markup: impl Into<String>) {
        self.markup = markup.into();
        self.selection = None;
        if let Ok(mut cache) = self.render_cache.write() {
            *cache = MarkdownRenderCache::default();
        }
    }

    /// Extract all headings from the markdown as `(level, title)` pairs.
    ///
    /// Used by `MarkdownTableOfContents` to build the sidebar tree.
    pub fn extract_headings(&self) -> Vec<(usize, String)> {
        self.markup
            .lines()
            .filter_map(|line| {
                Self::heading_level_and_text(line)
                    .map(|(level, title)| (level, title.to_string()))
            })
            .collect()
    }

    fn consume_heading_fragment<'a>(remaining: &'a str, fragment: &str) -> Option<&'a str> {
        let remaining = remaining.trim_start();
        let fragment = fragment.trim();
        if fragment.is_empty() {
            return Some(remaining);
        }
        if remaining == fragment {
            return Some("");
        }
        remaining.strip_prefix(fragment)
    }

    fn heading_level_and_text(line: &str) -> Option<(usize, &str)> {
        let trimmed = line.trim_start();
        let marker_len = trimmed.chars().take_while(|ch| *ch == '#').count();
        if marker_len == 0 || marker_len > 6 {
            return None;
        }
        let title = trimmed[marker_len..].trim();
        if title.is_empty() {
            return None;
        }
        Some((marker_len, title))
    }

    fn apply_horizontal_alignment(
        line: &mut Vec<rich_rs::Segment>,
        width: usize,
        align: HorizontalAlign,
        style: rich_rs::Style,
    ) -> usize {
        if matches!(align, HorizontalAlign::Left) || line.is_empty() {
            return 0;
        }
        let line_width = rich_rs::Segment::get_line_length(line);
        if line_width >= width {
            return 0;
        }
        let left_pad = match align {
            HorizontalAlign::Left => 0,
            HorizontalAlign::Center => (width - line_width) / 2,
            HorizontalAlign::Right => width - line_width,
        };
        if left_pad == 0 {
            return 0;
        }
        line.insert(0, rich_rs::Segment::styled(" ".repeat(left_pad), style));
        left_pad
    }

    fn normalize_selection(
        selection: (WidgetSelectionAnchor, WidgetSelectionAnchor),
    ) -> (WidgetSelectionAnchor, WidgetSelectionAnchor) {
        if selection.0.row < selection.1.row {
            return selection;
        }
        if selection.0.row > selection.1.row {
            return (selection.1, selection.0);
        }
        if selection.0.col <= selection.1.col {
            selection
        } else {
            (selection.1, selection.0)
        }
    }

    fn line_text_len(line: &str) -> usize {
        rich_rs::cell_len(line.trim_end())
    }

    fn line_content_bounds(cache: &MarkdownRenderCache, row: usize) -> (usize, usize) {
        let line_len = cache
            .lines
            .get(row)
            .map(|line| Self::line_text_len(line))
            .unwrap_or(0);
        let (start, end) = cache
            .content_bounds
            .get(row)
            .copied()
            .unwrap_or((0, line_len));
        (
            start.min(line_len),
            end.min(line_len).max(start.min(line_len)),
        )
    }

    fn clamp_anchor(
        cache: &MarkdownRenderCache,
        anchor: WidgetSelectionAnchor,
    ) -> WidgetSelectionAnchor {
        if cache.lines.is_empty() {
            return WidgetSelectionAnchor::default();
        }
        let row = anchor.row.min(cache.lines.len() - 1);
        let (start, end) = Self::line_content_bounds(cache, row);
        let col = anchor.col.min(end).max(start);
        WidgetSelectionAnchor { row, col, index: 0 }
    }

    fn cell_to_byte_index(line: &str, cell: usize) -> usize {
        let mut width = 0usize;
        let mut idx = 0usize;
        for (byte_idx, ch) in line.char_indices() {
            let w = rich_rs::cell_len(&ch.to_string());
            if width >= cell {
                idx = byte_idx;
                return idx;
            }
            width = width.saturating_add(w);
            idx = byte_idx + ch.len_utf8();
            if width >= cell {
                return idx;
            }
        }
        idx
    }

    fn slice_cells(line: &str, start_col: usize, end_col: usize) -> String {
        if start_col >= end_col {
            return String::new();
        }
        let start = Self::cell_to_byte_index(line, start_col);
        let end = Self::cell_to_byte_index(line, end_col);
        line.get(start..end).unwrap_or("").to_string()
    }

    fn selected_text_from_cache(
        cache: &MarkdownRenderCache,
        selection: (WidgetSelectionAnchor, WidgetSelectionAnchor),
    ) -> Option<String> {
        if cache.lines.is_empty() {
            return None;
        }
        let (start, end) = Self::normalize_selection(selection);
        let start = Self::clamp_anchor(cache, start);
        let end = Self::clamp_anchor(cache, end);
        if start.row == end.row && start.col == end.col {
            return None;
        }

        let mut out = Vec::new();
        for row in start.row..=end.row {
            let line = cache.lines[row].trim_end();
            let line_len = rich_rs::cell_len(line);
            let (content_start, content_end) = Self::line_content_bounds(cache, row);
            let slice = if row == start.row && row == end.row {
                let slice_start = start.col.min(line_len).max(content_start);
                let slice_end = end.col.min(line_len).min(content_end);
                Self::slice_cells(line, slice_start, slice_end)
            } else if row == start.row {
                let slice_start = start.col.min(line_len).max(content_start);
                Self::slice_cells(line, slice_start, content_end.min(line_len))
            } else if row == end.row {
                let slice_end = end.col.min(line_len).min(content_end);
                Self::slice_cells(line, content_start, slice_end)
            } else {
                Self::slice_cells(line, content_start, content_end)
            };
            out.push(slice);
        }
        let joined = out.join("\n");
        if joined.is_empty() {
            None
        } else {
            Some(joined)
        }
    }

    fn word_range_at(
        cache: &MarkdownRenderCache,
        x: u16,
        y: u16,
    ) -> Option<(WidgetSelectionAnchor, WidgetSelectionAnchor)> {
        if cache.lines.is_empty() {
            return None;
        }
        let row = usize::from(y).min(cache.lines.len().saturating_sub(1));
        let line = cache.lines[row].trim_end();
        if line.is_empty() {
            return None;
        }
        let line_len = rich_rs::cell_len(line);
        if line_len == 0 {
            return None;
        }

        let mut spans: Vec<(usize, usize, char)> = Vec::new();
        let mut cell = 0usize;
        for ch in line.chars() {
            let width = rich_rs::cell_len(&ch.to_string()).max(1);
            let start = cell;
            let end = cell.saturating_add(width);
            spans.push((start, end, ch));
            cell = end;
        }
        if spans.is_empty() {
            return None;
        }

        let target_col = usize::from(x).min(line_len.saturating_sub(1));
        let mut idx = spans
            .iter()
            .position(|(_, end, _)| target_col < *end)
            .unwrap_or(spans.len().saturating_sub(1));

        if spans[idx].2.is_whitespace() {
            if let Some(right) = ((idx + 1)..spans.len()).find(|&i| !spans[i].2.is_whitespace()) {
                idx = right;
            } else if let Some(left) = (0..idx).rev().find(|&i| !spans[i].2.is_whitespace()) {
                idx = left;
            } else {
                return None;
            }
        }

        let mut left = idx;
        while left > 0 && !spans[left - 1].2.is_whitespace() {
            left -= 1;
        }
        let mut right = idx;
        while right + 1 < spans.len() && !spans[right + 1].2.is_whitespace() {
            right += 1;
        }

        Some((
            WidgetSelectionAnchor {
                row,
                col: spans[left].0,
                index: 0,
            },
            WidgetSelectionAnchor {
                row,
                col: spans[right].1,
                index: 0,
            },
        ))
    }

    fn all_range(
        cache: &MarkdownRenderCache,
    ) -> Option<(WidgetSelectionAnchor, WidgetSelectionAnchor)> {
        if cache.lines.is_empty() {
            return None;
        }
        let last_row = cache.lines.len().saturating_sub(1);
        let end_col = Self::line_text_len(&cache.lines[last_row]);
        Some((
            WidgetSelectionAnchor::default(),
            WidgetSelectionAnchor {
                row: last_row,
                col: end_col,
                index: 0,
            },
        ))
    }

    fn apply_selection_highlight(
        lines: &[Vec<rich_rs::Segment>],
        width: usize,
        cache: &MarkdownRenderCache,
        selection: Option<(WidgetSelectionAnchor, WidgetSelectionAnchor)>,
    ) -> Segments {
        let Some(selection) = selection else {
            let mut out = Segments::new();
            let line_count = lines.len();
            for (index, line) in lines.iter().enumerate() {
                out.extend(line.clone());
                if index + 1 < line_count {
                    out.push(rich_rs::Segment::line());
                }
            }
            return out;
        };
        if lines.is_empty() {
            return Segments::new();
        }
        let height = lines.len().max(1);
        let mut frame = FrameBuffer::from_lines(lines, width.max(1), height, None);
        let (start, end) = Self::normalize_selection(selection);
        let selection_bg = parse_color_like("#094573").unwrap_or_else(|| Color::rgb(0, 120, 215));
        let selection_fg = parse_color_like("#ffffff")
            .unwrap_or_else(|| Color::rgb(255, 255, 255))
            .flatten_over(selection_bg);
        let highlight = rich_rs::Style::new()
            .with_bgcolor(selection_bg.to_simple_opaque())
            .with_color(selection_fg.to_simple_opaque());

        for row in start.row..=end.row {
            if row >= frame.height {
                break;
            }
            let row_start = if row == start.row { start.col } else { 0 };
            let row_line_len = lines
                .get(row)
                .map(|line| rich_rs::Segment::get_line_length(line))
                .unwrap_or(frame.width);
            let row_end = if row == end.row {
                end.col
            } else {
                row_line_len
            };
            let (content_start, content_end) = Self::line_content_bounds(cache, row);
            let paint_start = row_start.min(frame.width).max(content_start);
            let paint_end = row_end.min(frame.width).min(content_end);
            for col in paint_start..paint_end {
                let cell = frame.get_mut(col, row);
                cell.style = Some(match cell.style {
                    Some(existing) => existing.combine(&highlight),
                    None => highlight,
                });
            }
        }

        frame.to_segments()
    }
}

impl Widget for Markdown {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let rendered = RichMarkdown::new(self.markup.clone()).render(console, options);
        let mut lines = rich_rs::Segment::split_lines(rendered);

        let mut headings = self
            .markup
            .lines()
            .filter_map(|line| {
                let trimmed = line.trim_start();
                let marker_len = trimmed.chars().take_while(|ch| *ch == '#').count();
                if marker_len == 0 || marker_len > 6 {
                    return None;
                }
                let title = trimmed[marker_len..].trim();
                if title.is_empty() {
                    return None;
                }
                Some((marker_len, title.to_string()))
            })
            .collect::<VecDeque<_>>();
        let mut aligned_bounds: Vec<Option<(usize, usize)>> = vec![None; lines.len()];

        if !headings.is_empty() {
            let mut active_heading: Option<(usize, String)> = None;
            let mut heading_margins: Vec<(usize, usize)> = vec![(0, 0); lines.len()];
            for (line_index, line) in lines.iter_mut().enumerate() {
                if headings.is_empty() {
                    break;
                }
                let plain = line
                    .iter()
                    .filter(|segment| segment.control.is_none())
                    .map(|segment| segment.text.as_ref())
                    .collect::<String>();
                let trimmed = plain.trim();
                if trimmed.is_empty() {
                    continue;
                }

                let mut matched_level: Option<usize> = None;
                let mut heading_start = false;
                let mut heading_end = false;
                if let Some((level, remaining)) = active_heading.take() {
                    if let Some(rest) = Self::consume_heading_fragment(&remaining, trimmed) {
                        matched_level = Some(level);
                        if rest.is_empty() {
                            headings.pop_front();
                            heading_end = true;
                        } else {
                            active_heading = Some((level, rest.to_string()));
                        }
                    }
                }

                if matched_level.is_none() {
                    let Some((level, title)) = headings.front() else {
                        break;
                    };
                    if let Some(rest) = Self::consume_heading_fragment(title, trimmed) {
                        matched_level = Some(*level);
                        heading_start = true;
                        if rest.is_empty() {
                            headings.pop_front();
                            heading_end = true;
                        } else {
                            active_heading = Some((*level, rest.to_string()));
                        }
                    } else {
                        continue;
                    }
                }

                let level = matched_level.expect("matched heading level must be set");
                let class_name = format!("markdown--h{level}");
                let component_style =
                    crate::css::resolve_component_style(self, &[class_name.as_str()]);
                let fallback_style = line
                    .iter()
                    .find(|segment| segment.control.is_none())
                    .and_then(|segment| segment.style)
                    .unwrap_or_else(rich_rs::Style::new);
                let style = component_style.to_rich().unwrap_or(fallback_style);
                let heading_text = plain.trim().to_string();
                line.clear();
                if !heading_text.is_empty() {
                    line.push(rich_rs::Segment::styled(heading_text, style));
                }
                let pre_align_width = rich_rs::Segment::get_line_length(line);
                let pre_align_content_bounds = (0, pre_align_width);
                let horizontal_align = component_style
                    .content_align
                    .map(|align| align.horizontal)
                    .or_else(|| {
                        if level == 1 {
                            Some(HorizontalAlign::Center)
                        } else {
                            None
                        }
                    });
                if let Some(horizontal) = horizontal_align {
                    let left_pad = Self::apply_horizontal_alignment(
                        line,
                        options.size.0.max(1),
                        horizontal,
                        style,
                    );
                    aligned_bounds[line_index] = Some((
                        left_pad + pre_align_content_bounds.0,
                        left_pad + pre_align_content_bounds.1,
                    ));
                }
                let margin = component_style.effective_margin();
                heading_margins[line_index] = (
                    if heading_start {
                        usize::from(margin.top)
                    } else {
                        0
                    },
                    if heading_end {
                        usize::from(margin.bottom)
                    } else {
                        0
                    },
                );
            }

            if heading_margins
                .iter()
                .any(|(top, bottom)| *top > 0 || *bottom > 0)
            {
                let mut expanded_lines: Vec<Vec<rich_rs::Segment>> = Vec::new();
                let mut expanded_bounds: Vec<Option<(usize, usize)>> = Vec::new();
                for (index, line) in lines.into_iter().enumerate() {
                    let (top, bottom) = heading_margins[index];
                    for _ in 0..top {
                        expanded_lines.push(Vec::new());
                        expanded_bounds.push(Some((0, 0)));
                    }
                    expanded_lines.push(line);
                    expanded_bounds.push(aligned_bounds[index]);
                    for _ in 0..bottom {
                        expanded_lines.push(Vec::new());
                        expanded_bounds.push(Some((0, 0)));
                    }
                }
                lines = expanded_lines;
                aligned_bounds = expanded_bounds;
            }
        }

        let width = options.size.0.max(1);
        let height = lines.len().max(1);
        let cache_frame = FrameBuffer::from_lines(&lines, width, height, None);
        let plain_lines = cache_frame.as_plain_lines();
        let mut content_bounds = plain_lines
            .iter()
            .map(|line| (0, Self::line_text_len(line)))
            .collect::<Vec<_>>();
        for (row, bounds) in aligned_bounds.into_iter().enumerate() {
            if let Some((start, end)) = bounds
                && row < content_bounds.len()
            {
                content_bounds[row] = (start, end);
            }
        }
        let cache = MarkdownRenderCache {
            width,
            lines: plain_lines,
            content_bounds,
        };
        if let Ok(mut cached_lines) = self.render_cache.write() {
            *cached_lines = cache.clone();
        }

        Self::apply_selection_highlight(&lines, width, &cache, self.selection)
    }

    fn on_layout(&mut self, width: u16, _height: u16) {
        // Hidden/disconnected nodes can transiently receive width=0/1 during
        // tree display toggles. Keep the last stable width (>1) so wrapped-height
        // calculations remain stable across tab switches.
        if width > 1 {
            self.layout_width = usize::from(width);
        }
    }

    fn layout_height(&self) -> Option<usize> {
        let base = if self.layout_width > 0 {
            self.markup
                .lines()
                .map(|line| {
                    let display = Self::heading_level_and_text(line)
                        .map(|(_, heading)| heading)
                        .unwrap_or(line);
                    rich_rs::cell_len(display)
                        .div_ceil(self.layout_width)
                        .max(1)
                })
                .sum::<usize>()
                .max(1)
        } else {
            self.markup.lines().count().max(1)
        };
        let heading_margin_rows = self
            .markup
            .lines()
            .filter_map(Self::heading_level_and_text)
            .map(|(level, _)| {
                let class_name = format!("markdown--h{level}");
                let component_style =
                    crate::css::resolve_component_style(self, &[class_name.as_str()]);
                let margin = component_style.effective_margin();
                usize::from(margin.top) + usize::from(margin.bottom)
            })
            .sum::<usize>();
        let intrinsic = base.saturating_add(heading_margin_rows).max(1);
        fixed_height_from_constraints(self.layout_constraints()).or(Some(intrinsic))
    }

    fn content_width(&self) -> Option<usize> {
        // Python Textual's Markdown block model expands horizontally and applies
        // heading/content alignment within that full region. Returning an intrinsic
        // width hint here makes `width:auto` shrink to longest line, which breaks
        // H1 centering parity in TabbedContent panes.
        None
    }

    fn style_id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }

    fn allow_select(&self) -> bool {
        true
    }

    fn selection_at(&self, x: u16, y: u16) -> Option<WidgetSelectionAnchor> {
        let Ok(cache) = self.render_cache.read() else {
            return None;
        };
        if cache.lines.is_empty() || cache.width == 0 {
            return None;
        }
        let row = usize::from(y).min(cache.lines.len().saturating_sub(1));
        let (content_start, content_end) = Self::line_content_bounds(&cache, row);
        let col = usize::from(x).min(content_end).max(content_start);
        Some(WidgetSelectionAnchor { row, col, index: 0 })
    }

    fn selection_word_range_at(
        &self,
        x: u16,
        y: u16,
    ) -> Option<(WidgetSelectionAnchor, WidgetSelectionAnchor)> {
        let Ok(cache) = self.render_cache.read() else {
            return None;
        };
        Self::word_range_at(&cache, x, y)
    }

    fn selection_all_range(&self) -> Option<(WidgetSelectionAnchor, WidgetSelectionAnchor)> {
        let Ok(cache) = self.render_cache.read() else {
            return None;
        };
        Self::all_range(&cache)
    }

    fn update_selection(&mut self, from: WidgetSelectionAnchor, to: WidgetSelectionAnchor) -> bool {
        let normalized = Self::normalize_selection((from, to));
        let changed = self.selection != Some(normalized);
        self.selection = Some(normalized);
        changed
    }

    fn clear_selection(&mut self) -> bool {
        let changed = self.selection.is_some();
        self.selection = None;
        changed
    }

    fn get_selection(&self) -> Option<String> {
        let selection = self.selection?;
        let Ok(cache) = self.render_cache.read() else {
            return None;
        };
        Self::selected_text_from_cache(&cache, selection)
    }
}

impl Renderable for Markdown {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[cfg(test)]
mod tests {
    use super::{Label, Markdown, MarkdownRenderCache};
    use crate::render::FrameBuffer;
    use crate::style::parse_color_like;
    use crate::widgets::{Widget, WidgetSelectionAnchor};
    use rich_rs::Console;

    #[test]
    fn markdown_layout_height_ignores_transient_zero_width_layout_updates() {
        let mut markdown = Markdown::new(
            r#"
# Duke Leto I Atreides

Head of House Atreides.
"#,
        );
        markdown.on_layout(27, 8);
        let stable = markdown.layout_height().expect("markdown height");
        assert!(stable < 20, "sanity: wrapped markdown should stay compact");

        markdown.on_layout(1, 0);
        let after_one = markdown.layout_height().expect("markdown height");
        assert_eq!(
            after_one, stable,
            "provisional width=1 updates must not inflate markdown height"
        );

        markdown.on_layout(0, 0);
        let after_zero = markdown.layout_height().expect("markdown height");
        assert_eq!(
            after_zero, stable,
            "zero-width hidden layout updates must not collapse width to 1 and inflate height"
        );
    }

    #[test]
    fn label_layout_height_ignores_transient_zero_width_layout_updates() {
        let mut label = Label::new("Bene Gesserit and concubine of Leto, and mother of Paul.");
        label.on_layout(32, 1);
        let stable = label.layout_height().expect("label height");
        assert!(stable < 10, "sanity: wrapped label should stay compact");

        label.on_layout(1, 0);
        let after_one = label.layout_height().expect("label height");
        assert_eq!(
            after_one, stable,
            "provisional width=1 updates must not inflate label height"
        );

        label.on_layout(0, 0);
        let after_zero = label.layout_height().expect("label height");
        assert_eq!(
            after_zero, stable,
            "zero-width hidden layout updates must not collapse width to 1 and inflate height"
        );
    }

    #[test]
    fn label_defaults_to_non_shrinking_width_hint() {
        let label = Label::new("I must not fear.");
        assert_eq!(
            label.content_width(),
            None,
            "Label default should match Textual: no intrinsic shrink width unless explicitly enabled"
        );
    }

    #[test]
    fn markdown_selection_returns_selected_text() {
        let mut markdown = Markdown::new("Bene Gesserit and concubine.");
        let console = Console::new();
        let mut options = console.options().clone();
        options.size = (64, 6);
        Widget::render(&markdown, &console, &options);

        let start = markdown.selection_at(0, 0).expect("selection start");
        let end = markdown.selection_at(4, 0).expect("selection end");
        assert!(markdown.update_selection(start, end));
        assert_eq!(markdown.get_selection().as_deref(), Some("Bene"));
    }

    #[test]
    fn markdown_selection_can_span_multiple_lines() {
        let cache = MarkdownRenderCache {
            width: 20,
            lines: vec!["first line".to_string(), "second line".to_string()],
            content_bounds: vec![(0, 10), (0, 11)],
        };
        let from = WidgetSelectionAnchor {
            row: 0,
            col: 0,
            index: 0,
        };
        let to = WidgetSelectionAnchor {
            row: 1,
            col: 6,
            index: 0,
        };
        let selected = Markdown::selected_text_from_cache(&cache, (from, to));
        assert_eq!(selected.as_deref(), Some("first line\nsecond"));
    }

    #[test]
    fn markdown_word_range_selects_word_under_pointer() {
        let mut markdown = Markdown::new("alpha beta gamma");
        let console = Console::new();
        let mut options = console.options().clone();
        options.size = (64, 6);
        Widget::render(&markdown, &console, &options);

        let (from, to) = markdown
            .selection_word_range_at(8, 0)
            .expect("word range should resolve");
        assert!(markdown.update_selection(from, to));
        assert_eq!(markdown.get_selection().as_deref(), Some("beta"));
    }

    #[test]
    fn markdown_selection_all_range_covers_full_content() {
        let mut markdown = Markdown::new("alpha\nbeta");
        let console = Console::new();
        let mut options = console.options().clone();
        options.size = (64, 6);
        Widget::render(&markdown, &console, &options);

        let (from, to) = markdown
            .selection_all_range()
            .expect("selection all should resolve");
        assert!(markdown.update_selection(from, to));
        assert_eq!(markdown.get_selection().as_deref(), Some("alpha beta"));
    }

    #[test]
    fn markdown_multiline_selection_does_not_fill_to_widget_width() {
        let mut markdown = Markdown::new("alpha\nbeta");
        let console = Console::new();
        let mut options = console.options().clone();
        options.size = (40, 6);
        Widget::render(&markdown, &console, &options);

        let start = markdown.selection_at(0, 0).expect("start");
        let end = markdown.selection_at(2, 1).expect("end");
        assert!(markdown.update_selection(start, end));

        let frame = FrameBuffer::from_renderable(&console, &options, &markdown, None);
        let selection_bg = parse_color_like("#094573")
            .expect("selection bg token")
            .to_simple_opaque();
        assert_eq!(
            frame.get(0, 0).style.as_ref().and_then(|s| s.bgcolor),
            Some(selection_bg),
            "selected text should use selection background"
        );
        assert_ne!(
            frame.get(30, 0).style.as_ref().and_then(|s| s.bgcolor),
            Some(selection_bg),
            "selection should not bleed to end of row"
        );
    }

    #[test]
    fn markdown_selection_uses_consistent_fg_bg_across_text_styles() {
        let mut markdown = Markdown::new("# Lady Jessica\nBene Gesserit");
        let console = Console::new();
        let mut options = console.options().clone();
        options.size = (40, 6);
        let initial = FrameBuffer::from_renderable(&console, &options, &markdown, None);
        let initial_lines = initial.as_plain_lines();
        let heading_x = initial_lines[0].find("Lady").expect("heading text x");
        let (body_row, body_x) = initial_lines
            .iter()
            .enumerate()
            .find_map(|(row, line)| line.find("Bene").map(|x| (row, x)))
            .expect("body text x");

        let start = markdown
            .selection_at(heading_x as u16, 0)
            .expect("start selection");
        let end = markdown
            .selection_at((body_x + 2) as u16, body_row as u16)
            .expect("end selection");
        assert!(markdown.update_selection(start, end));
        let frame = FrameBuffer::from_renderable(&console, &options, &markdown, None);
        let heading_style = frame.get(heading_x, 0).style.expect("heading style");
        let body_style = frame.get(body_x, body_row).style.expect("body style");
        assert_eq!(heading_style.bgcolor, body_style.bgcolor);
        assert_eq!(heading_style.color, body_style.color);
    }
}
