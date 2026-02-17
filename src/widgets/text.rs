use rich_rs::markdown::Markdown as RichMarkdown;
use rich_rs::{Console, ConsoleOptions, Renderable, Segments, Text};
use std::collections::VecDeque;
use std::sync::RwLock;

use crate::render::FrameBuffer;
use crate::style::HorizontalAlign;

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
            text: text.into(),
            wrap: true,
            markup: false,
            expand: false,
            shrink: true,
            layout_width: 0,
            variant: None,
            classes: vec!["label".to_string()],
            styles: WidgetStyles::default(),
        }
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

    /// When true, the widget shrinks to its content width (default: true).
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

    fn apply_horizontal_alignment(
        line: &mut Vec<rich_rs::Segment>,
        width: usize,
        align: HorizontalAlign,
        style: rich_rs::Style,
    ) {
        if matches!(align, HorizontalAlign::Left) || line.is_empty() {
            return;
        }
        let line_width = rich_rs::Segment::get_line_length(line);
        if line_width >= width {
            return;
        }
        let left_pad = match align {
            HorizontalAlign::Left => 0,
            HorizontalAlign::Center => (width - line_width) / 2,
            HorizontalAlign::Right => width - line_width,
        };
        if left_pad == 0 {
            return;
        }
        line.insert(0, rich_rs::Segment::styled(" ".repeat(left_pad), style));
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

    fn clamp_anchor(
        cache: &MarkdownRenderCache,
        anchor: WidgetSelectionAnchor,
    ) -> WidgetSelectionAnchor {
        if cache.lines.is_empty() {
            return WidgetSelectionAnchor::default();
        }
        let row = anchor.row.min(cache.lines.len() - 1);
        let col = anchor.col.min(Self::line_text_len(&cache.lines[row]));
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
            let slice = if row == start.row && row == end.row {
                Self::slice_cells(line, start.col.min(line_len), end.col.min(line_len))
            } else if row == start.row {
                Self::slice_cells(line, start.col.min(line_len), line_len)
            } else if row == end.row {
                Self::slice_cells(line, 0, end.col.min(line_len))
            } else {
                line.to_string()
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

    fn apply_selection_highlight(
        lines: &[Vec<rich_rs::Segment>],
        width: usize,
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
        let highlight = rich_rs::Style::new().with_reverse(true);

        for row in start.row..=end.row {
            if row >= frame.height {
                break;
            }
            let row_start = if row == start.row { start.col } else { 0 };
            let row_end = if row == end.row { end.col } else { frame.width };
            for col in row_start.min(frame.width)..row_end.min(frame.width) {
                let cell = frame.get_mut(col, row);
                cell.style = Some(match cell.style {
                    Some(existing) => highlight.combine(&existing),
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

        if !headings.is_empty() {
            let mut active_heading: Option<(usize, String)> = None;
            for line in &mut lines {
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
                if let Some((level, remaining)) = active_heading.take() {
                    if let Some(rest) = Self::consume_heading_fragment(&remaining, trimmed) {
                        matched_level = Some(level);
                        if rest.is_empty() {
                            headings.pop_front();
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
                        if rest.is_empty() {
                            headings.pop_front();
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
                let style = component_style
                    .to_rich()
                    .unwrap_or_else(rich_rs::Style::new);
                for segment in line.iter_mut().filter(|segment| segment.control.is_none()) {
                    // Override markdown heading style to match CSS, avoid inheriting rich heading underline.
                    segment.style = Some(style);
                }
                if let Some(content_align) = component_style.content_align {
                    Self::apply_horizontal_alignment(
                        line,
                        options.size.0.max(1),
                        content_align.horizontal,
                        style,
                    );
                }
            }
        }

        let width = options.size.0.max(1);
        let height = lines.len().max(1);
        let cache_frame = FrameBuffer::from_lines(&lines, width, height, None);
        let cache = MarkdownRenderCache {
            width,
            lines: cache_frame.as_plain_lines(),
        };
        if let Ok(mut cached_lines) = self.render_cache.write() {
            *cached_lines = cache;
        }

        Self::apply_selection_highlight(&lines, width, self.selection)
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
        let intrinsic = if self.layout_width > 0 {
            self.markup
                .lines()
                .map(|line| rich_rs::cell_len(line).div_ceil(self.layout_width).max(1))
                .sum::<usize>()
                .max(1)
        } else {
            self.markup.lines().count().max(1)
        };
        fixed_height_from_constraints(self.layout_constraints()).or(Some(intrinsic))
    }

    fn content_width(&self) -> Option<usize> {
        let content_width = self
            .markup
            .lines()
            .map(rich_rs::cell_len)
            .max()
            .unwrap_or(0)
            .max(1);
        // Keep `width: auto` consistent with Textual defaults: intrinsic width
        // should include horizontal padding from resolved CSS.
        let meta = crate::css::selector_meta_generic(self);
        let resolved = crate::css::resolve_style(self, &meta);
        let padding = resolved.effective_padding();
        let pad_lr = usize::from(padding.left.saturating_add(padding.right));
        Some(content_width.saturating_add(pad_lr))
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
        let max_col = Self::line_text_len(&cache.lines[row]);
        let col = usize::from(x).min(max_col);
        Some(WidgetSelectionAnchor { row, col, index: 0 })
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
}
