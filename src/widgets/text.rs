use pulldown_cmark::{
    Event as MdEvent, Options as MdOptions, Parser as MdParser, Tag as MdTag, TagEnd as MdTagEnd,
};
use rich_rs::{Console, ConsoleOptions, MetaValue, Renderable, Segment, Segments, StyleMeta, Text};
use std::sync::{Arc, RwLock};
use unicode_width::UnicodeWidthChar;

use crate::event::{Event, EventCtx};
use crate::message::ActionDispatchRequested;
use crate::widgets::markdown_model::{
    MarkdownBlock, parse_markdown_blocks, parse_markdown_headings,
};

use super::{NodeSeed, Vertical, Widget, helpers::border_spacing_from_style};

/// Tag a segment with `textual:no_text_style = true` so `apply_style_to_segments`
/// skips re-applying widget CSS text attributes (bold, italic, etc.) that have
/// already been baked into the segment by `Content::render_strips`.
fn tag_segment_no_text_style(seg: &mut Segment) {
    let mut meta = seg.meta.take().unwrap_or_else(StyleMeta::new);
    let mut map: std::collections::BTreeMap<String, MetaValue> = meta
        .meta
        .as_ref()
        .map(|m| (**m).clone())
        .unwrap_or_default();
    map.insert(
        "textual:no_text_style".to_string(),
        MetaValue::Bool(true),
    );
    meta.meta = Some(Arc::new(map));
    seg.meta = Some(meta);
}

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
    border_title: Option<String>,
    border_subtitle: Option<String>,
    seed: NodeSeed,
}

impl Label {
    pub fn new(text: impl Into<String>) -> Self {
        let mut seed = NodeSeed::default();
        seed.classes = vec!["label".to_string()];
        Self {
            text: text.into(),
            wrap: true,
            // Python Textual's Label/Static interpret console markup by default
            // (`markup=True`); `[link=…]`, `[@click=…]` and `[b]…[/]` are parsed.
            // Use `.with_markup(false)` to render tags literally.
            markup: true,
            expand: false,
            // Match Textual Label defaults: labels don't shrink to intrinsic width
            // unless explicitly requested.
            shrink: false,
            layout_width: 0,
            variant: None,
            border_title: None,
            border_subtitle: None,
            seed,
        }
    }

    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.seed.css_id = Some(id.into());
        self
    }

    crate::seed_ident_methods!();

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
    }

    /// Set the text rendered on the top border (Python `widget.border_title`).
    /// Only visible when the widget has a border; align/colors come from the
    /// `border-title-*` CSS properties.
    pub fn with_border_title(mut self, title: impl Into<String>) -> Self {
        self.border_title = Some(title.into());
        self
    }

    /// Set the text rendered on the bottom border (Python `widget.border_subtitle`).
    pub fn with_border_subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.border_subtitle = Some(subtitle.into());
        self
    }

    /// Set or clear the border title at runtime (Python `widget.border_title = ...`).
    pub fn set_border_title(&mut self, title: Option<impl Into<String>>) {
        self.border_title = title.map(Into::into);
    }

    /// Set or clear the border subtitle at runtime.
    pub fn set_border_subtitle(&mut self, subtitle: Option<impl Into<String>>) {
        self.border_subtitle = subtitle.map(Into::into);
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
        self.seed.classes = vec!["label".to_string()];
        if let Some(v) = self.variant {
            self.seed.classes.push(v.css_class().to_string());
        }
    }

    /// Mutable access to the pre-mount `NodeSeed` (css_id, classes, inline styles).
    ///
    /// Valid until the widget is mounted into the arena tree; after mount the
    /// node record is the single source of truth and seed changes have no effect.
    pub fn seed_mut(&mut self) -> &mut NodeSeed {
        &mut self.seed
    }

    fn intrinsic_height(&self) -> usize {
        // Route through the shared word-wrap line counter so the trailing-blank
        // semantics match Python: `Content.split(allow_blank=True)` keeps a final
        // empty line when the text ends with '\n' (Rust `str::lines()` drops it).
        // This is what `Static(TEXT * N)` (TEXT ending in '\n') relies on for its
        // auto height to match Python's content height (e.g. 71 vs 70 rows), which
        // in turn drives the scroll/overflow/scrollbar geometry.
        intrinsic_wrapped_height(&self.text, self.layout_width, self.wrap)
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
    fn border_title(&self) -> Option<&str> {
        self.border_title.as_deref()
    }

    fn border_subtitle(&self) -> Option<&str> {
        self.border_subtitle.as_deref()
    }

    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);

        // Get the widget's resolved visual style (pushed by render_widget_with_meta
        // onto the style stack before calling render()).
        let visual_style = crate::css::current_self_style().unwrap_or_default();

        // Determine text alignment from resolved CSS.
        let text_align = visual_style
            .text_align
            .unwrap_or(crate::style::TextAlign::Left);

        // Build the effective background: flatten the widget's own bg over the
        // composited ancestor background so transparent-bg labels still get the
        // correct surface color baked into every segment.
        //
        // Use current_ancestor_composited_background() which excludes the current
        // widget's own style from the composite. This matches what
        // apply_style_to_segments sees when it runs AFTER render() returns (at
        // which point the current widget's style has been popped from the stack).
        let parent_bg = crate::css::current_ancestor_composited_background().unwrap_or_else(|| {
            crate::style::parse_color_like("$background")
                .unwrap_or(crate::style::Color::rgb(0, 0, 0))
        });
        let effective_bg = visual_style
            .bg
            .map(|c| c.flatten_over(parent_bg))
            .unwrap_or(parent_bg);

        // Construct the render-time visual style: always has an explicit bg so
        // make_segment never falls back to black. fg/attrs come from the resolved
        // style (same source as apply_style_to_segments would use).
        let mut render_style = visual_style.clone();
        render_style.bg = Some(effective_bg);

        // Build Content from the label text.
        let mut content = if self.markup {
            crate::content::Content::from_markup(&self.text)
        } else {
            crate::content::Content::from_text(&self.text)
        };

        // Apply link-* CSS styling to `[@click=...]` spans.
        //
        // Python's `widget.link_style` is applied to any segment whose meta
        // carries `@click` (see `widget.py` `_StyledRenderable.__rich_console__`).
        // We mirror this: detect spans whose raw_tag starts with `@click=` and
        // overlay the link style computed from the widget's link-* CSS properties.
        //
        // `[link=url]` spans do NOT get link styling — only `@click` spans do.
        if self.markup {
            if let Some(link_span_style) =
                compute_link_span_style(&visual_style, effective_bg)
            {
                // Collect @click span ranges first to avoid borrow conflicts.
                let link_ranges: Vec<(usize, usize)> = content
                    .spans()
                    .iter()
                    .filter(|span| {
                        matches!(&span.span_style, crate::content::SpanStyle::Raw(raw)
                            if raw.starts_with("@click=") || raw == "@click")
                    })
                    .map(|span| (span.start, span.end))
                    .collect();
                for (start, end) in link_ranges {
                    content = content.stylize(link_span_style.clone(), start, end);
                }
            }
        }

        // Resolve theme tokens in span styles using parse_tag_style, which calls
        // parse_color_like internally and handles $primary/$surface etc.
        let resolve_fn = |raw: &str| {
            crate::content::markup::parse_tag_style(raw)
                .map(|t| t.style)
                .unwrap_or_default()
        };

        // Render via Content::render_strips.
        // - No height cap: let wrap_and_format determine row count.
        // - overflow="fold": word-wrap (render.rs handles ellipsis/clip for no_wrap).
        // - no_wrap=false: always word-wrap here; render.rs applies overflow later.
        // - line_pad=0: handled by render_widget_with_meta.
        let strips = content.render_strips(
            width,
            None,
            &render_style,
            text_align,
            "fold",
            false,
            0,
            resolve_fn,
        );

        // Flatten strips into Segments joined by newlines.
        // Tag each data segment with textual:no_text_style so apply_style_to_segments
        // skips re-applying widget CSS text attrs (bold/italic/etc.), which have
        // already been baked in by render_strips (visual_style + span_style combined).
        // bg and fg are also already baked in; apply_style_to_segments will skip them
        // because: explicit_bg.is_some() → no bg override; s.color.is_some() → no fg
        // override (for concrete fg); fg_auto is handled when color is None.
        // tint and text_opacity are still applied by apply_style_to_segments on top.
        let mut segments = Segments::new();
        let n_strips = strips.len();
        for (i, strip) in strips.into_iter().enumerate() {
            for mut seg in strip {
                tag_segment_no_text_style(&mut seg);
                segments.push(seg);
            }
            if i + 1 < n_strips {
                segments.push(Segment::line());
            }
        }
        segments
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
            // Neither expand nor shrink — no width hint. (See `auto_content_width`
            // for the `width: auto` measurement path, which does report the
            // rendered text width without affecting the unset-width fill default.)
            None
        }
    }

    fn auto_content_width(&self) -> Option<usize> {
        if self.expand {
            None
        } else {
            // For `width: auto` sizing, report the rendered text's cell width so
            // the box shrinks to its content (Python parity). Kept separate from
            // `content_width()` so an UNSET width (fill default, e.g. a bare
            // `Static`) is not turned into a content-width hint.
            Some(self.intrinsic_content_width())
        }
    }

    fn layout_height(&self) -> Option<usize> {
        // `layout_height()` is the widget's OUTER height (content + own
        // padding/border), the convention the flow layout's `extract_child_spec`
        // height arm relies on (it adds only margin on top). Include the resolved
        // vertical chrome so a styled `Label { padding: 1 2 }` occupies its full
        // box height instead of letting the content overflow its 1-row box.
        let chrome = crate::widgets::helpers::resolved_vertical_chrome(self);
        Some(self.intrinsic_height().saturating_add(chrome))
    }

    fn style(&self) -> Option<crate::style::Style> {
        if self.seed.styles.style != Default::default() {
            Some(self.seed.styles.style.clone())
        } else {
            None
        }
    }

    fn set_inline_style(&mut self, style: crate::style::Style) {
        self.seed.styles.style = style;
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }
}

impl Renderable for Label {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

pub struct Markdown {
    markup: String,
    /// Shared content reference for parent-driven content updates (e.g. from MarkdownViewer).
    /// When set, `on_layout()` syncs `self.markup` from this shared state before computing height.
    shared_markup: Option<Arc<RwLock<String>>>,
    layout_width: usize,
    intrinsic_height: usize,
    can_focus: bool,
    composed_children: Vec<Box<dyn Widget>>,
    pending_recompose: bool,
    seed: NodeSeed,
}

impl std::fmt::Debug for Markdown {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Markdown")
            .field("markup_len", &self.markup.len())
            .field("pending_recompose", &self.pending_recompose)
            .finish()
    }
}

impl Clone for Markdown {
    fn clone(&self) -> Self {
        let mut cloned = Self {
            markup: self.markup.clone(),
            shared_markup: self.shared_markup.clone(),
            layout_width: self.layout_width,
            intrinsic_height: self.intrinsic_height,
            can_focus: self.can_focus,
            composed_children: build_markdown_children(&self.markup),
            pending_recompose: self.pending_recompose,
            seed: self.seed.clone(),
        };
        cloned.recompute_intrinsic_height();
        cloned
    }
}

fn count_rendered_lines(segments: Segments) -> usize {
    let mut lines = Segment::split_lines(segments);
    while lines
        .last()
        .is_some_and(|line| Segment::get_line_length(line) == 0)
    {
        lines.pop();
    }
    lines.len().max(1)
}

fn rendered_markdown_height(markup: &str, width: usize) -> usize {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (width.max(1), 1);
    options.max_width = width.max(1);
    options.max_height = 1;
    let rendered = rich_rs::markdown::Markdown::new(markup.to_string()).render(&console, &options);
    count_rendered_lines(rendered)
}

pub(crate) fn rendered_plain_height(text: &str, width: usize) -> usize {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (width.max(1), 1);
    options.max_width = width.max(1);
    options.max_height = 1;
    let rendered = Text::plain(text.to_string()).render(&console, &options);
    count_rendered_lines(rendered)
}

/// Word-wrap-aware intrinsic line count for a `Static`/`Label`-style plain text
/// body, shared so widgets don't re-implement (and under-count) wrapping.
///
/// When `wrap` is enabled and `width > 0`, this routes through the real Rich
/// word-wrap line counter (`rendered_plain_height`) instead of a naive
/// `cell_len.div_ceil(width)` char-count — a paragraph wider than `width` breaks
/// at WORD boundaries, producing MORE lines than the char-count estimate, so the
/// char-count under-counts and the wrapped tail gets clipped (padding02).
///
/// A trailing `\n` is counted as one extra blank row to match Python Rich, which
/// `rendered_plain_height` (via `count_rendered_lines`) would otherwise pop.
pub(crate) fn intrinsic_wrapped_height(text: &str, width: usize, wrap: bool) -> usize {
    let mut lines = if wrap && width > 0 {
        rendered_plain_height(text, width)
    } else {
        // No wrap (or unknown width): count hard line breaks only.
        text.lines().count().max(1)
    };
    // `str::lines()` / `count_rendered_lines` both drop a trailing newline's
    // blank row; Python Rich keeps it.
    if text.ends_with('\n') {
        lines += 1;
    }
    lines.max(1)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InlineRun {
    text: String,
    classes: Vec<&'static str>,
    link_href: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct InlineTextDoc {
    plain: String,
    runs: Vec<InlineRun>,
}

impl InlineTextDoc {
    fn parse(markup: &str) -> Self {
        let mut options = MdOptions::empty();
        options.insert(MdOptions::ENABLE_TABLES);
        options.insert(MdOptions::ENABLE_STRIKETHROUGH);
        options.insert(MdOptions::ENABLE_TASKLISTS);
        options.insert(MdOptions::ENABLE_HEADING_ATTRIBUTES);
        let parser = MdParser::new_ext(markup, options);

        let mut runs: Vec<InlineRun> = Vec::new();
        let mut emphasis = 0usize;
        let mut strong = 0usize;
        let mut strike = 0usize;
        let mut link_stack: Vec<String> = Vec::new();

        for event in parser {
            match event {
                MdEvent::Start(MdTag::Emphasis) => emphasis = emphasis.saturating_add(1),
                MdEvent::End(MdTagEnd::Emphasis) => emphasis = emphasis.saturating_sub(1),
                MdEvent::Start(MdTag::Strong) => strong = strong.saturating_add(1),
                MdEvent::End(MdTagEnd::Strong) => strong = strong.saturating_sub(1),
                MdEvent::Start(MdTag::Strikethrough) => strike = strike.saturating_add(1),
                MdEvent::End(MdTagEnd::Strikethrough) => strike = strike.saturating_sub(1),
                MdEvent::Start(MdTag::Link { dest_url, .. }) => {
                    link_stack.push(dest_url.to_string());
                }
                MdEvent::End(MdTagEnd::Link) => {
                    link_stack.pop();
                }
                MdEvent::Text(text) => {
                    push_inline_run(
                        &mut runs,
                        collapse_inline_whitespace(&text),
                        emphasis,
                        strong,
                        strike,
                        link_stack.last().map(String::as_str),
                        false,
                    );
                }
                MdEvent::Code(text) => {
                    push_inline_run(
                        &mut runs,
                        text.to_string(),
                        emphasis,
                        strong,
                        strike,
                        link_stack.last().map(String::as_str),
                        true,
                    );
                }
                MdEvent::SoftBreak => {
                    push_inline_run(
                        &mut runs,
                        " ".to_string(),
                        emphasis,
                        strong,
                        strike,
                        link_stack.last().map(String::as_str),
                        false,
                    );
                }
                MdEvent::HardBreak => {
                    push_inline_run(
                        &mut runs,
                        "\n".to_string(),
                        emphasis,
                        strong,
                        strike,
                        link_stack.last().map(String::as_str),
                        false,
                    );
                }
                _ => {}
            }
        }

        let plain = runs.iter().map(|run| run.text.as_str()).collect::<String>();
        Self { plain, runs }
    }

    fn rendered_height(&self, width: usize) -> usize {
        rendered_plain_height(&self.plain, width)
    }

    fn render_for_widget(
        &self,
        widget: &dyn Widget,
        console: &Console,
        options: &ConsoleOptions,
        hovered_link: Option<&str>,
    ) -> Segments {
        let mut text = Text::plain(self.plain.clone());
        let meta = crate::css::selector_meta_generic(widget);
        let resolved = crate::css::resolve_style(widget, &meta);
        let mut offset = 0usize;
        for run in &self.runs {
            let start = offset;
            let end = start + run.text.chars().count();
            offset = end;

            let mut style = rich_rs::Style::new();
            let mut has_style = false;
            for class in &run.classes {
                if let Some(component) =
                    crate::css::resolve_component_style(widget, &[class]).to_rich()
                {
                    style = style.combine(&component);
                    has_style = true;
                }
            }

            if let Some(href) = run.link_href.as_deref() {
                let hovered = hovered_link.is_some_and(|active| active == href);
                if hovered {
                    if let Some(color) = resolved.link_color_hover.or(resolved.link_color) {
                        style = style.with_color(color.to_simple_opaque());
                        has_style = true;
                    }
                    if let Some(bg) = resolved.link_background_hover.or(resolved.link_background) {
                        if bg.a > 0.0 {
                            style = style.with_bgcolor(bg.to_simple_opaque());
                            has_style = true;
                        }
                    }
                    if let Some(flags) = resolved.link_style_hover.or(resolved.link_style) {
                        apply_text_style_flags(&mut style, &flags);
                        has_style = true;
                    }
                } else {
                    if let Some(color) = resolved.link_color {
                        style = style.with_color(color.to_simple_opaque());
                        has_style = true;
                    }
                    if let Some(bg) = resolved.link_background {
                        if bg.a > 0.0 {
                            style = style.with_bgcolor(bg.to_simple_opaque());
                            has_style = true;
                        }
                    }
                    if let Some(flags) = resolved.link_style {
                        apply_text_style_flags(&mut style, &flags);
                        has_style = true;
                    }
                }
            }

            if has_style {
                text.stylize(start, end, style);
            }

            if let Some(href) = run.link_href.as_deref() {
                let mut link_meta = std::collections::BTreeMap::new();
                link_meta.insert(
                    "@click".to_string(),
                    rich_rs::MetaValue::str(format_markdown_link_action(href)),
                );
                text.apply_meta(link_meta, start as isize, Some(end as isize));
            }
        }
        text.render(console, options)
    }

    fn link_at_coords(&self, x: u16, y: u16, width: usize) -> Option<&str> {
        let idx = self.char_index_at_coords(usize::from(x), usize::from(y), width.max(1))?;
        self.link_at_char_index(idx)
    }

    fn link_at_char_index(&self, idx: usize) -> Option<&str> {
        let mut start = 0usize;
        for run in &self.runs {
            let end = start + run.text.chars().count();
            if idx >= start && idx < end {
                return run.link_href.as_deref();
            }
            start = end;
        }
        None
    }

    fn char_index_at_coords(&self, x: usize, y: usize, width: usize) -> Option<usize> {
        let mut row = 0usize;
        let mut col = 0usize;
        let mut idx = 0usize;
        let mut row_has_content = false;

        for ch in self.plain.chars() {
            if ch == '\n' {
                if row == y {
                    return None;
                }
                row = row.saturating_add(1);
                col = 0;
                idx = idx.saturating_add(1);
                row_has_content = false;
                continue;
            }

            let cell_width = UnicodeWidthChar::width(ch).unwrap_or(0).max(1);
            if col + cell_width > width {
                row = row.saturating_add(1);
                col = 0;
                row_has_content = false;
            }
            if row > y {
                return None;
            }
            if row == y {
                row_has_content = true;
                if x >= col && x < col + cell_width {
                    return Some(idx);
                }
            }

            col += cell_width;
            idx = idx.saturating_add(1);
            if col >= width {
                row = row.saturating_add(1);
                col = 0;
                row_has_content = false;
            }
        }

        if row == y && row_has_content {
            return None;
        }
        None
    }
}

fn push_inline_run(
    runs: &mut Vec<InlineRun>,
    text: String,
    emphasis: usize,
    strong: usize,
    strike: usize,
    link_href: Option<&str>,
    code_inline: bool,
) {
    if text.is_empty() {
        return;
    }

    let mut classes = Vec::new();
    if code_inline {
        classes.push("code_inline");
    }
    if emphasis > 0 {
        classes.push("em");
    }
    if strong > 0 {
        classes.push("strong");
    }
    if strike > 0 {
        classes.push("s");
    }
    if link_href.is_some() {
        classes.push("link");
    }

    if let Some(last) = runs.last_mut()
        && last.classes == classes
        && last.link_href.as_deref() == link_href
    {
        last.text.push_str(&text);
        return;
    }
    runs.push(InlineRun {
        text,
        classes,
        link_href: link_href.map(ToString::to_string),
    });
}

fn collapse_inline_whitespace(text: &str) -> String {
    let mut out = String::new();
    let mut prior_space = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !prior_space {
                out.push(' ');
            }
            prior_space = true;
        } else {
            prior_space = false;
            out.push(ch);
        }
    }
    out
}

/// Compute the link span style from a widget's resolved CSS properties.
///
/// Mirrors Python `Widget.link_style`:
/// ```python
/// link_background = background + styles.link_background
/// link_color = link_background + styles.link_color  # (when auto_link_color=False)
/// style = styles.link_style + Style.from_color(link_color.rich_color,
///     link_background.rich_color if styles.link_background.a else None)
/// ```
///
/// Returns `None` if no link_color is set (no visible link styling to apply).
/// This matches Python: `link-color` defaults to the contrast text, which
/// has alpha 0.87 — always Some in practice.
pub(crate) fn compute_link_span_style(
    visual_style: &crate::style::Style,
    effective_bg: crate::style::Color,
) -> Option<crate::style::Style> {
    let link_color = visual_style.link_color?;

    // Compose link_background over the effective background.
    let link_bg = if let Some(lb) = visual_style.link_background {
        if lb.a > 0.0 {
            lb.flatten_over(effective_bg)
        } else {
            effective_bg
        }
    } else {
        effective_bg
    };

    // Compose link_color over link_bg.
    //
    // For `link-color: auto` (the default `$link-color`/`$text`), Python computes
    // `link_background.get_contrast_text(alpha)` — a contrast color resolved
    // against the LINK background, NOT a fixed color resolved against the screen.
    // (widget.py `link_style`: `link_background.get_contrast_text(styles.link_color.a)`
    // when `auto_link_color`.) Recompute the contrast against `link_bg` here so a
    // bright link background (e.g. `link-background: $accent`) yields dark link text.
    let computed_fg = if let Some(auto) = visual_style.link_color_auto {
        crate::style::contrast_text(link_bg).blend_over_float(link_bg, auto.alpha())
    } else {
        link_color.flatten_over(link_bg)
    };

    let mut style = crate::style::Style::new();
    style.fg = Some(computed_fg);

    // Only set link background in the span style when link_background has alpha.
    if let Some(lb) = visual_style.link_background {
        if lb.a > 0.0 {
            style.bg = Some(lb.flatten_over(effective_bg));
        }
    }

    // Apply text style flags (bold, italic, underline, etc.).
    if let Some(flags) = visual_style.link_style {
        if flags.bold {
            style.bold = Some(true);
        }
        if flags.dim {
            style.dim = Some(true);
        }
        if flags.italic {
            style.italic = Some(true);
        }
        if flags.underline {
            style.underline = Some(true);
        }
        if flags.reverse {
            style.reverse = Some(true);
        }
        if flags.strike {
            style.strike = Some(true);
        }
    }

    Some(style)
}

fn apply_text_style_flags(style: &mut rich_rs::Style, flags: &crate::style::TextStyleFlags) {
    if flags.bold {
        *style = style.with_bold(true);
    }
    if flags.dim {
        *style = style.with_dim(true);
    }
    if flags.italic {
        *style = style.with_italic(true);
    }
    if flags.underline {
        *style = style.with_underline(true);
    }
    if flags.reverse {
        *style = style.with_reverse(true);
    }
    if flags.strike {
        *style = style.with_strike(true);
    }
}

fn format_markdown_link_action(href: &str) -> String {
    let escaped = href.replace('\\', "\\\\").replace('\'', "\\'");
    format!("link('{escaped}')")
}

#[derive(Debug)]
struct MarkdownHeadingBlock {
    level: usize,
    text: String,
    layout_width: usize,
    seed: NodeSeed,
}

impl MarkdownHeadingBlock {
    fn new(level: usize, text: String) -> Self {
        let mut seed = NodeSeed::default();
        seed.classes = vec![format!("markdown--h{}", level.clamp(1, 6))];
        Self {
            level,
            text,
            layout_width: 0,
            seed,
        }
    }
}

impl Widget for MarkdownHeadingBlock {
    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);

        // Get the resolved visual style (pushed by render_widget_with_meta).
        let visual_style = crate::css::current_self_style().unwrap_or_default();

        // Flatten the widget's own bg over the composited ancestor background.
        let parent_bg = crate::css::current_ancestor_composited_background().unwrap_or_else(|| {
            crate::style::parse_color_like("$background")
                .unwrap_or(crate::style::Color::rgb(0, 0, 0))
        });
        let effective_bg = visual_style
            .bg
            .map(|c| c.flatten_over(parent_bg))
            .unwrap_or(parent_bg);
        let mut render_style = visual_style.clone();
        render_style.bg = Some(effective_bg);

        let content = crate::content::Content::from_text(&self.text);

        let resolve_fn = |raw: &str| {
            crate::content::markup::parse_tag_style(raw)
                .map(|t| t.style)
                .unwrap_or_default()
        };

        // Heading blocks wrap to width and may have multiple output lines.
        let strips = content.render_strips(
            width,
            None,
            &render_style,
            crate::style::TextAlign::Left,
            "fold",
            false,
            0,
            resolve_fn,
        );

        let mut segments = Segments::new();
        let n_strips = strips.len();
        for (i, strip) in strips.into_iter().enumerate() {
            for mut seg in strip {
                tag_segment_no_text_style(&mut seg);
                segments.push(seg);
            }
            if i + 1 < n_strips {
                segments.push(Segment::line());
            }
        }
        segments
    }

    fn style_type(&self) -> &'static str {
        match self.level {
            1 => "MarkdownH1",
            2 => "MarkdownH2",
            3 => "MarkdownH3",
            4 => "MarkdownH4",
            5 => "MarkdownH5",
            _ => "MarkdownH6",
        }
    }

    fn style_type_aliases(&self) -> &[&'static str] {
        &["MarkdownHeader", "MarkdownBlock"]
    }

    fn on_layout(&mut self, width: u16, _height: u16) {
        if width > 1 {
            self.layout_width = usize::from(width);
        }
    }

    fn content_width(&self) -> Option<usize> {
        if self.level == 1 {
            None
        } else {
            Some(rich_rs::cell_len(&self.text).max(1))
        }
    }

    fn layout_height(&self) -> Option<usize> {
        Some(rendered_plain_height(&self.text, self.layout_width.max(1)))
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }
}

#[derive(Debug)]
struct MarkdownParagraphBlock {
    inline_doc: InlineTextDoc,
    layout_width: usize,
    hovered_link: Option<String>,
}

impl MarkdownParagraphBlock {
    fn new(raw: String) -> Self {
        Self {
            inline_doc: InlineTextDoc::parse(&raw),
            layout_width: 0,
            hovered_link: None,
        }
    }
}

impl Widget for MarkdownParagraphBlock {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        self.inline_doc
            .render_for_widget(self, console, options, self.hovered_link.as_deref())
    }

    fn style_type(&self) -> &'static str {
        "MarkdownParagraph"
    }

    fn style_type_aliases(&self) -> &[&'static str] {
        &["MarkdownBlock"]
    }

    fn on_layout(&mut self, width: u16, _height: u16) {
        if width > 1 {
            self.layout_width = usize::from(width);
        }
    }

    fn mouse_interactive(&self) -> bool {
        true
    }

    fn on_mouse_move(&mut self, x: u16, y: u16) -> bool {
        let hovered = self
            .inline_doc
            .link_at_coords(x, y, self.layout_width.max(1))
            .map(ToString::to_string);
        if hovered != self.hovered_link {
            self.hovered_link = hovered;
            return true;
        }
        false
    }

    fn on_node_state_changed(
        &mut self,
        _old: crate::widgets::NodeState,
        new: crate::widgets::NodeState,
    ) {
        if !new.hovered {
            self.hovered_link = None;
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if let Event::MouseUp(mouse) = event
            && mouse.target.is_some_and(|t| t == self.node_id())
            && let Some(href) =
                self.inline_doc
                    .link_at_coords(mouse.x, mouse.y, self.layout_width.max(1))
        {
            ctx.post_message(ActionDispatchRequested {
                action: format_markdown_link_action(href),
            });
            ctx.set_handled();
        }
    }

    fn layout_height(&self) -> Option<usize> {
        Some(self.inline_doc.rendered_height(self.layout_width.max(1)))
    }
}

#[derive(Debug)]
struct MarkdownFenceBlock {
    raw: String,
    layout_width: usize,
}

impl MarkdownFenceBlock {
    fn new(raw: String) -> Self {
        Self {
            raw,
            layout_width: 0,
        }
    }
}

impl Widget for MarkdownFenceBlock {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        rich_rs::markdown::Markdown::new(self.raw.clone()).render(console, options)
    }

    fn style_type(&self) -> &'static str {
        "MarkdownFence"
    }

    fn style_type_aliases(&self) -> &[&'static str] {
        &["MarkdownBlock"]
    }

    fn on_layout(&mut self, width: u16, _height: u16) {
        if width > 1 {
            self.layout_width = usize::from(width);
        }
    }

    fn layout_height(&self) -> Option<usize> {
        Some(rendered_markdown_height(
            &self.raw,
            self.layout_width.max(1),
        ))
    }
}

#[derive(Debug)]
struct MarkdownHorizontalRuleBlock;

impl MarkdownHorizontalRuleBlock {
    fn new() -> Self {
        Self
    }
}

impl Widget for MarkdownHorizontalRuleBlock {
    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        Segments::from(vec![Segment::new("─".repeat(width))])
    }

    fn style_type(&self) -> &'static str {
        "MarkdownHorizontalRule"
    }

    fn style_type_aliases(&self) -> &[&'static str] {
        &["MarkdownBlock"]
    }

    fn layout_height(&self) -> Option<usize> {
        Some(1)
    }
}

/// One child of a blockquote: either a paragraph (with inline markup) or a
/// nested blockquote. Mirrors Python `MarkdownBlockQuote` containing a mix of
/// `MarkdownParagraph` blocks and nested `MarkdownBlockQuote` blocks.
#[derive(Debug, Clone)]
enum QuoteChild {
    Paragraph(InlineTextDoc),
    Quote(Vec<QuoteChild>),
}

/// Left bar drawn by Python's `border-left: outer ...` on `MarkdownBlockQuote`.
const QUOTE_BAR: char = '\u{258c}'; // ▌

/// Build the children of a blockquote from a pulldown-cmark event stream that is
/// already positioned just after a `Start(BlockQuote)` event. Consumes events up
/// to and including the matching `End(BlockQuote)`.
fn parse_quote_children<'a, I>(parser: &mut std::iter::Peekable<I>) -> Vec<QuoteChild>
where
    I: Iterator<Item = MdEvent<'a>>,
{
    let mut children: Vec<QuoteChild> = Vec::new();
    while let Some(event) = parser.next() {
        match event {
            MdEvent::End(MdTagEnd::BlockQuote(_)) => break,
            MdEvent::Start(MdTag::BlockQuote(_)) => {
                children.push(QuoteChild::Quote(parse_quote_children(parser)));
            }
            MdEvent::Start(MdTag::Paragraph) => {
                let raw = collect_inline_markup_until_paragraph_end(parser);
                if !raw.trim().is_empty() {
                    children.push(QuoteChild::Paragraph(InlineTextDoc::parse(&raw)));
                }
            }
            _ => {}
        }
    }
    children
}

/// Reconstruct a paragraph's inline Markdown source from events until the
/// matching `End(Paragraph)`. We rebuild a small markup string so the existing
/// `InlineTextDoc::parse` can apply emphasis/strong/code/link styling uniformly.
fn collect_inline_markup_until_paragraph_end<'a, I>(parser: &mut std::iter::Peekable<I>) -> String
where
    I: Iterator<Item = MdEvent<'a>>,
{
    let mut out = String::new();
    let mut depth_emphasis = 0usize;
    let mut depth_strong = 0usize;
    let mut depth_strike = 0usize;
    let mut link_stack: Vec<String> = Vec::new();
    while let Some(event) = parser.next() {
        match event {
            MdEvent::End(MdTagEnd::Paragraph) => break,
            MdEvent::Start(MdTag::Emphasis) => {
                out.push('*');
                depth_emphasis += 1;
            }
            MdEvent::End(MdTagEnd::Emphasis) => {
                out.push('*');
                depth_emphasis = depth_emphasis.saturating_sub(1);
            }
            MdEvent::Start(MdTag::Strong) => {
                out.push_str("**");
                depth_strong += 1;
            }
            MdEvent::End(MdTagEnd::Strong) => {
                out.push_str("**");
                depth_strong = depth_strong.saturating_sub(1);
            }
            MdEvent::Start(MdTag::Strikethrough) => {
                out.push_str("~~");
                depth_strike += 1;
            }
            MdEvent::End(MdTagEnd::Strikethrough) => {
                out.push_str("~~");
                depth_strike = depth_strike.saturating_sub(1);
            }
            MdEvent::Start(MdTag::Link { dest_url, .. }) => {
                out.push('[');
                link_stack.push(dest_url.to_string());
            }
            MdEvent::End(MdTagEnd::Link) => {
                out.push(']');
                if let Some(url) = link_stack.pop() {
                    out.push('(');
                    out.push_str(&url);
                    out.push(')');
                }
            }
            MdEvent::Text(text) => out.push_str(&text),
            MdEvent::Code(text) => {
                out.push('`');
                out.push_str(&text);
                out.push('`');
            }
            MdEvent::SoftBreak | MdEvent::HardBreak => out.push(' '),
            _ => {}
        }
    }
    let _ = (depth_emphasis, depth_strong, depth_strike);
    out
}

/// Wrap a plain string to `width` columns using rich's text layout, returning
/// one `String` per visual line (trailing blank lines dropped).
fn wrap_plain_lines(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (width, 1);
    options.max_width = width;
    options.max_height = 1;
    let rendered = Text::plain(text.to_string()).render(&console, &options);
    let mut lines: Vec<String> = Segment::split_lines(rendered)
        .into_iter()
        .map(|line| {
            line.iter()
                .map(|seg| seg.text.as_ref())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect();
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// A blockquote Markdown block, rendered with a left `▌` bar per nesting level
/// and blank bar lines around nested quotes — matching Python `MarkdownBlockQuote`
/// (`border-left: outer`, `margin: 1 0`, nested `> BlockQuote { margin-left: 2; margin-top: 1 }`).
#[derive(Debug, Clone)]
struct MarkdownBlockQuoteBlock {
    children: Vec<QuoteChild>,
    layout_width: usize,
}

impl MarkdownBlockQuoteBlock {
    fn new(children: Vec<QuoteChild>) -> Self {
        Self {
            children,
            layout_width: 0,
        }
    }

    /// Produce the plain text lines for this blockquote subtree.
    ///
    /// `depth` is the nesting level *relative to this widget's content box*. The
    /// outermost bar + left padding are drawn by the framework from the
    /// `MarkdownBlockQuote { border-left: outer; padding: 0 1 }` default CSS, so
    /// content starts at `depth = 0` (no prefix). Each additional nesting level
    /// adds a `▌ ` prefix; nested quotes are surrounded by a single blank bar line
    /// at the parent depth (Python's vertical margins around `MarkdownBlockQuote`).
    fn render_lines(children: &[QuoteChild], depth: usize, total_width: usize) -> Vec<String> {
        let prefix = format!("{} ", QUOTE_BAR).repeat(depth);
        let prefix_width = rich_rs::cell_len(&prefix);
        let content_width = total_width.saturating_sub(prefix_width).max(1);
        // A blank bar line at the current depth (bars only, trailing space trimmed).
        let blank_bar = prefix.trim_end().to_string();

        let mut lines: Vec<String> = Vec::new();
        for child in children {
            match child {
                QuoteChild::Paragraph(doc) => {
                    for wrapped in wrap_plain_lines(&doc.plain, content_width) {
                        lines.push(format!("{}{}", prefix, wrapped));
                    }
                }
                QuoteChild::Quote(nested) => {
                    // Vertical margin above the nested quote.
                    lines.push(blank_bar.clone());
                    lines.extend(Self::render_lines(nested, depth + 1, total_width));
                    // Vertical margin below the nested quote.
                    lines.push(blank_bar.clone());
                }
            }
        }
        lines
    }
}

impl Widget for MarkdownBlockQuoteBlock {
    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(self.layout_width.max(1));
        let lines = Self::render_lines(&self.children, 0, width);
        let mut segments: Vec<Segment> = Vec::new();
        for (idx, line) in lines.iter().enumerate() {
            if idx > 0 {
                segments.push(Segment::new("\n"));
            }
            segments.push(Segment::new(line.clone()));
        }
        Segments::from(segments)
    }

    fn style_type(&self) -> &'static str {
        "MarkdownBlockQuote"
    }

    fn style_type_aliases(&self) -> &[&'static str] {
        &["MarkdownBlock"]
    }

    fn on_layout(&mut self, width: u16, _height: u16) {
        if width > 1 {
            self.layout_width = usize::from(width);
        }
    }

    fn layout_height(&self) -> Option<usize> {
        Some(Self::render_lines(&self.children, 0, self.layout_width.max(1)).len())
    }
}

#[derive(Debug)]
struct MarkdownBullet {
    symbol: String,
}

impl MarkdownBullet {
    fn new(symbol: String) -> Self {
        Self { symbol }
    }
}

impl Widget for MarkdownBullet {
    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        Segments::from(vec![Segment::new(self.symbol.clone())])
    }

    fn style_type(&self) -> &'static str {
        "MarkdownBullet"
    }

    fn content_width(&self) -> Option<usize> {
        Some(rich_rs::cell_len(&self.symbol).max(1))
    }
}

#[derive(Debug)]
struct MarkdownInlineItem {
    inline_doc: InlineTextDoc,
    layout_width: usize,
    hovered_link: Option<String>,
}

impl MarkdownInlineItem {
    fn new(raw: String) -> Self {
        Self {
            inline_doc: InlineTextDoc::parse(&raw),
            layout_width: 0,
            hovered_link: None,
        }
    }
}

impl Widget for MarkdownInlineItem {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        self.inline_doc
            .render_for_widget(self, console, options, self.hovered_link.as_deref())
    }

    fn style_type(&self) -> &'static str {
        "MarkdownParagraph"
    }

    fn style_type_aliases(&self) -> &[&'static str] {
        &["MarkdownBlock"]
    }

    fn on_layout(&mut self, width: u16, _height: u16) {
        if width > 1 {
            self.layout_width = usize::from(width);
        }
    }

    fn mouse_interactive(&self) -> bool {
        true
    }

    fn on_mouse_move(&mut self, x: u16, y: u16) -> bool {
        let hovered = self
            .inline_doc
            .link_at_coords(x, y, self.layout_width.max(1))
            .map(ToString::to_string);
        if hovered != self.hovered_link {
            self.hovered_link = hovered;
            return true;
        }
        false
    }

    fn on_node_state_changed(
        &mut self,
        _old: crate::widgets::NodeState,
        new: crate::widgets::NodeState,
    ) {
        if !new.hovered {
            self.hovered_link = None;
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if let Event::MouseUp(mouse) = event
            && mouse.target.is_some_and(|t| t == self.node_id())
            && let Some(href) =
                self.inline_doc
                    .link_at_coords(mouse.x, mouse.y, self.layout_width.max(1))
        {
            ctx.post_message(ActionDispatchRequested {
                action: format_markdown_link_action(href),
            });
            ctx.set_handled();
        }
    }

    fn layout_height(&self) -> Option<usize> {
        Some(self.inline_doc.rendered_height(self.layout_width.max(1)))
    }
}

struct MarkdownListItemBlock {
    symbol: String,
    item_inline_doc: InlineTextDoc,
    layout_width: usize,
    children: Vec<Box<dyn Widget>>,
}

impl MarkdownListItemBlock {
    fn new(symbol: String, _item_text: String, item_markup: String) -> Self {
        let content = Vertical::new().with_child(MarkdownInlineItem::new(item_markup.clone()));
        let children: Vec<Box<dyn Widget>> = vec![
            Box::new(MarkdownBullet::new(symbol.clone())),
            Box::new(content),
        ];
        Self {
            symbol,
            item_inline_doc: InlineTextDoc::parse(&item_markup),
            layout_width: 0,
            children,
        }
    }
}

impl Widget for MarkdownListItemBlock {
    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        Segments::new()
    }

    fn style_type(&self) -> &'static str {
        "MarkdownListItem"
    }

    fn on_layout(&mut self, width: u16, _height: u16) {
        if width > 1 {
            self.layout_width = usize::from(width);
        }
    }

    fn layout_height(&self) -> Option<usize> {
        let bullet_width = rich_rs::cell_len(&self.symbol).max(1);
        let content_width = self.layout_width.saturating_sub(bullet_width).max(1);
        Some(self.item_inline_doc.rendered_height(content_width))
    }

    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        std::mem::take(&mut self.children)
    }
}

struct MarkdownListBlock {
    ordered: bool,
    items: Vec<String>,
    item_inline_docs: Vec<InlineTextDoc>,
    layout_width: usize,
    children: Vec<Box<dyn Widget>>,
}

impl MarkdownListBlock {
    fn new(ordered: bool, items: Vec<String>, item_markups: Vec<String>) -> Self {
        let items_copy = items.clone();
        let children: Vec<Box<dyn Widget>> = if ordered {
            let width = items.len().to_string().len().saturating_add(2).max(2);
            items
                .into_iter()
                .enumerate()
                .map(|(index, item)| {
                    Box::new(MarkdownListItemBlock::new(
                        format!("{:>width$}", format!("{}. ", index + 1), width = width),
                        item,
                        item_markups.get(index).cloned().unwrap_or_else(String::new),
                    )) as Box<dyn Widget>
                })
                .collect()
        } else {
            const BULLET: &str = "• ";
            items
                .into_iter()
                .enumerate()
                .map(|(index, item)| {
                    Box::new(MarkdownListItemBlock::new(
                        BULLET.to_string(),
                        item,
                        item_markups.get(index).cloned().unwrap_or_else(String::new),
                    )) as Box<dyn Widget>
                })
                .collect()
        };
        Self {
            ordered,
            items: items_copy,
            item_inline_docs: item_markups
                .into_iter()
                .map(|item| InlineTextDoc::parse(&item))
                .collect(),
            layout_width: 0,
            children,
        }
    }
}

impl Widget for MarkdownListBlock {
    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        Segments::new()
    }

    fn style_type(&self) -> &'static str {
        if self.ordered {
            "MarkdownOrderedList"
        } else {
            "MarkdownBulletList"
        }
    }

    fn style_type_aliases(&self) -> &[&'static str] {
        &["MarkdownList", "MarkdownBlock"]
    }

    fn on_layout(&mut self, width: u16, _height: u16) {
        if width > 1 {
            self.layout_width = usize::from(width);
        }
    }

    fn layout_height(&self) -> Option<usize> {
        let width = self.layout_width.max(1);
        let bullet_width = if self.ordered {
            self.items.len().to_string().len().saturating_add(2).max(2)
        } else {
            2
        };
        let text_width = width.saturating_sub(bullet_width).max(1);
        let content_height = self
            .item_inline_docs
            .iter()
            .map(|item| item.rendered_height(text_width))
            .sum::<usize>()
            .max(1);
        Some(content_height)
    }

    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        std::mem::take(&mut self.children)
    }
}

#[derive(Debug)]
struct MarkdownTableCell {
    text: String,
    inline_doc: InlineTextDoc,
    layout_width: usize,
    hovered_link: Option<String>,
    seed: NodeSeed,
}

impl MarkdownTableCell {
    fn new(text: String, raw: String, classes: Vec<String>) -> Self {
        let mut seed = NodeSeed::default();
        seed.classes = classes;
        Self {
            text,
            inline_doc: InlineTextDoc::parse(&raw),
            layout_width: 0,
            hovered_link: None,
            seed,
        }
    }
}

impl Widget for MarkdownTableCell {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let mut one_line = options.clone();
        one_line.max_height = 1;
        one_line.size.1 = 1;
        self.inline_doc
            .render_for_widget(self, console, &one_line, self.hovered_link.as_deref())
    }

    fn style_type(&self) -> &'static str {
        "MarkdownTableCell"
    }

    fn style_type_aliases(&self) -> &[&'static str] {
        &["MarkdownBlock"]
    }

    fn on_layout(&mut self, width: u16, _height: u16) {
        if width > 1 {
            self.layout_width = usize::from(width);
        }
    }

    fn mouse_interactive(&self) -> bool {
        true
    }

    fn on_mouse_move(&mut self, x: u16, y: u16) -> bool {
        let hovered = self
            .inline_doc
            .link_at_coords(x, y, self.layout_width.max(1))
            .map(ToString::to_string);
        if hovered != self.hovered_link {
            self.hovered_link = hovered;
            return true;
        }
        false
    }

    fn on_node_state_changed(
        &mut self,
        _old: crate::widgets::NodeState,
        new: crate::widgets::NodeState,
    ) {
        if !new.hovered {
            self.hovered_link = None;
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if let Event::MouseUp(mouse) = event
            && mouse.target.is_some_and(|t| t == self.node_id())
            && let Some(href) =
                self.inline_doc
                    .link_at_coords(mouse.x, mouse.y, self.layout_width.max(1))
        {
            ctx.post_message(ActionDispatchRequested {
                action: format_markdown_link_action(href),
            });
            ctx.set_handled();
        }
    }

    fn layout_height(&self) -> Option<usize> {
        let _ = &self.text;
        Some(1)
    }

    fn tooltip(&self) -> Option<String> {
        let value = self.text.trim();
        if value.is_empty() {
            None
        } else {
            Some(value.to_string())
        }
    }

    fn tooltip_anchor(&self) -> Option<(u16, u16)> {
        // Keep tooltip placement pinned to this cell's local center so runtime
        // can convert through scroll-aware content-local coordinates.
        let x = (self.layout_width.max(1) / 2).min(u16::MAX as usize) as u16;
        Some((x, 0))
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }
}

fn table_cell_content_width(markup: &str) -> usize {
    rich_rs::cell_len(&InlineTextDoc::parse(markup).plain).max(1)
}

fn compute_markdown_table_column_fractions(
    header_markups: &[String],
    row_markups: &[Vec<String>],
    column_count: usize,
) -> Vec<crate::style::Scalar> {
    let columns = column_count.max(1);
    (0..columns)
        .map(|column| {
            let header_width = header_markups
                .get(column)
                .map(|cell| table_cell_content_width(cell))
                .unwrap_or(1);
            let mut max_content = header_width;
            for row in row_markups {
                if let Some(cell) = row.get(column) {
                    max_content = max_content.max(table_cell_content_width(cell));
                }
            }
            let growth = max_content.saturating_sub(header_width);
            // Python's grid-auto behavior keeps narrow semantic columns readable
            // while allowing content-heavy columns to grow. Approximate this by
            // smoothing row-driven growth so a single long token doesn't dominate.
            let smoothed = header_width as f32 + (growth as f32).sqrt();
            let weight = (smoothed.ceil() as usize)
                .max(header_width)
                .saturating_add(2)
                .max(1);
            // Include left/right padding from default CSS (`padding: 0 1`).
            crate::style::Scalar::Fraction(weight as f32)
        })
        .collect()
}

fn compute_markdown_table_column_widths(
    header_markups: &[String],
    row_markups: &[Vec<String>],
    table_width: usize,
    column_count: usize,
) -> Vec<usize> {
    let columns = column_count.max(1);
    let horizontal_gutter = columns.saturating_sub(1); // `grid-gutter: 1 1`
    let budget = table_width.saturating_sub(horizontal_gutter).max(columns);

    let mut desired = vec![3usize; columns];
    let mut minimum = vec![3usize; columns];

    for column in 0..columns {
        let header_content = header_markups
            .get(column)
            .map(|cell| table_cell_content_width(cell))
            .unwrap_or(1);
        let mut max_content = header_content;

        for row in row_markups {
            if let Some(cell) = row.get(column) {
                max_content = max_content.max(table_cell_content_width(cell));
            }
        }

        // Cell padding is `0 1` in default CSS.
        desired[column] = max_content.saturating_add(2).max(3);
        // Keep headers readable under compaction; row values may wrap/crop.
        minimum[column] = header_content.saturating_add(2).max(3);
        if desired[column] < minimum[column] {
            desired[column] = minimum[column];
        }
    }

    let mut widths = desired;
    let mut total = widths.iter().sum::<usize>();

    if total < budget {
        let grow_col = columns.saturating_sub(1);
        widths[grow_col] = widths[grow_col].saturating_add(budget - total);
        return widths;
    }

    while total > budget {
        let mut best_col = None;
        let mut best_slack = 0usize;
        for col in (0..columns).rev() {
            let slack = widths[col].saturating_sub(minimum[col]);
            if slack > best_slack {
                best_slack = slack;
                best_col = Some(col);
            }
        }
        if let Some(col) = best_col {
            widths[col] = widths[col].saturating_sub(1);
            total = total.saturating_sub(1);
        } else {
            break;
        }
    }

    if total > budget {
        // If the hard minimum sum still exceeds the budget, shrink the widest
        // columns first (typically the description column) instead of shrinking
        // round-robin. This preserves narrow semantic columns (`Type`, `Default`)
        // and better matches Python's table readability under tight widths.
        const HARD_FLOOR: usize = 3;
        while total > budget {
            let mut best_col = None;
            let mut best_width = 0usize;
            for col in 0..columns {
                if widths[col] > HARD_FLOOR
                    && (widths[col] > best_width
                        || (widths[col] == best_width && best_col.is_some_and(|idx| col > idx)))
                {
                    best_width = widths[col];
                    best_col = Some(col);
                }
            }
            let Some(col) = best_col else {
                break;
            };
            widths[col] -= 1;
            total -= 1;
        }
    }

    widths
}

fn estimate_markdown_table_row_heights(
    header_markups: &[String],
    row_markups: &[Vec<String>],
    column_widths: &[usize],
    row_count_hint: usize,
) -> Vec<usize> {
    let _ = (header_markups, row_markups, column_widths);
    let mut row_heights: Vec<usize> = vec![1];
    row_heights.extend(vec![1; row_markups.len()]);

    if row_heights.len() < row_count_hint {
        row_heights.resize(row_count_hint, 1);
    }
    if row_heights.is_empty() {
        row_heights.push(1);
    }
    row_heights
}

fn estimate_markdown_table_height(
    header_markups: &[String],
    row_markups: &[Vec<String>],
    table_width: usize,
    column_count: usize,
    row_count_hint: usize,
) -> usize {
    // Python grid keyline reserves a 1-cell ring around content.
    let table_width = table_width.saturating_sub(2).max(1);
    let column_widths = compute_markdown_table_column_widths(
        header_markups,
        row_markups,
        table_width,
        column_count,
    );
    let row_heights = estimate_markdown_table_row_heights(
        header_markups,
        row_markups,
        &column_widths,
        row_count_hint,
    );
    let vertical_gutter = row_heights.len().saturating_sub(1); // `grid-gutter: 1 1`
    row_heights
        .into_iter()
        .sum::<usize>()
        .saturating_add(vertical_gutter)
        .saturating_add(2)
        .max(1)
}

struct MarkdownTableContentBlock {
    column_count: usize,
    header_markups: Vec<String>,
    row_count: usize,
    row_markups: Vec<Vec<String>>,
    layout_width: usize,
    children: Vec<Box<dyn Widget>>,
    /// Computed row heights from the last on_layout call; contributed via style() hook.
    grid_rows: Option<Vec<crate::style::Scalar>>,
    seed: NodeSeed,
}

impl MarkdownTableContentBlock {
    fn new(
        headers: Vec<String>,
        header_markups: Vec<String>,
        rows: Vec<Vec<String>>,
        row_markups: Vec<Vec<String>>,
    ) -> Self {
        let column_count = headers.len().max(1) as u16;
        let mut effective_header_markups = Vec::with_capacity(headers.len());
        for (index, header) in headers.iter().enumerate() {
            effective_header_markups.push(
                header_markups
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| header.clone()),
            );
        }
        let mut effective_row_markups: Vec<Vec<String>> = Vec::with_capacity(rows.len());
        for (row_index, row) in rows.iter().enumerate() {
            let mut effective_row = Vec::with_capacity(row.len());
            for (cell_index, cell) in row.iter().enumerate() {
                effective_row.push(
                    row_markups
                        .get(row_index)
                        .and_then(|cells| cells.get(cell_index))
                        .cloned()
                        .unwrap_or_else(|| cell.clone()),
                );
            }
            effective_row_markups.push(effective_row);
        }
        let row_count = rows.len().saturating_add(1).max(1);
        let column_fractions = compute_markdown_table_column_fractions(
            &effective_header_markups,
            &effective_row_markups,
            column_count as usize,
        );
        let mut children: Vec<Box<dyn Widget>> = Vec::new();
        for (index, header) in headers.into_iter().enumerate() {
            children.push(Box::new(MarkdownTableCell::new(
                header,
                effective_header_markups
                    .get(index)
                    .cloned()
                    .unwrap_or_else(String::new),
                vec!["header".to_string(), "markdown-table--header".to_string()],
            )));
        }
        for (row_index, row) in rows.into_iter().enumerate() {
            for (cell_index, cell) in row.into_iter().enumerate() {
                children.push(Box::new(MarkdownTableCell::new(
                    cell,
                    effective_row_markups
                        .get(row_index)
                        .and_then(|cells| cells.get(cell_index))
                        .cloned()
                        .unwrap_or_else(String::new),
                    vec!["cell".to_string()],
                )));
            }
        }
        let mut seed = NodeSeed::default();
        seed.styles.style.grid_size_columns = Some(column_count);
        seed.styles.style.grid_size_rows = Some(row_count as u16);
        seed.styles.style.grid_columns = Some(column_fractions);
        Self {
            column_count: column_count as usize,
            header_markups: effective_header_markups,
            row_count,
            row_markups: effective_row_markups,
            layout_width: 0,
            children,
            grid_rows: None,
            seed,
        }
    }
}

impl Widget for MarkdownTableContentBlock {
    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        Segments::new()
    }

    fn style_type(&self) -> &'static str {
        "MarkdownTableContent"
    }

    fn style_type_aliases(&self) -> &[&'static str] {
        &["MarkdownBlock"]
    }

    fn on_layout(&mut self, width: u16, _height: u16) {
        if width > 1 {
            self.layout_width = usize::from(width);
        }
        let content_width = self.layout_width.saturating_sub(2).max(1);
        let column_widths = compute_markdown_table_column_widths(
            &self.header_markups,
            &self.row_markups,
            content_width,
            self.column_count,
        );
        let mut row_heights = estimate_markdown_table_row_heights(
            &self.header_markups,
            &self.row_markups,
            &column_widths,
            self.row_count,
        );
        if row_heights.len() < self.row_count {
            row_heights.resize(self.row_count, 1);
        }
        // Rule 6: post-layout style contribution via style() hook instead of
        // mutating stored inline styles.
        self.grid_rows = Some(
            row_heights
                .into_iter()
                .map(|height| crate::style::Scalar::Cells(height.min(u16::MAX as usize) as u16))
                .collect(),
        );
    }

    fn style(&self) -> Option<crate::style::Style> {
        self.grid_rows.as_ref().map(|rows| crate::style::Style {
            grid_rows: Some(rows.clone()),
            ..Default::default()
        })
    }

    fn layout_height(&self) -> Option<usize> {
        Some(estimate_markdown_table_height(
            &self.header_markups,
            &self.row_markups,
            self.layout_width.max(1),
            self.column_count,
            self.row_count,
        ))
    }

    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        std::mem::take(&mut self.children)
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }
}

struct MarkdownTableBlock {
    column_count: usize,
    header_markups: Vec<String>,
    row_count: usize,
    row_markups: Vec<Vec<String>>,
    layout_width: usize,
    children: Vec<Box<dyn Widget>>,
}

impl MarkdownTableBlock {
    fn new(
        headers: Vec<String>,
        header_markups: Vec<String>,
        rows: Vec<Vec<String>>,
        row_markups: Vec<Vec<String>>,
    ) -> Self {
        let mut effective_header_markups = Vec::with_capacity(headers.len());
        for (index, header) in headers.iter().enumerate() {
            effective_header_markups.push(
                header_markups
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| header.clone()),
            );
        }
        let mut effective_row_markups: Vec<Vec<String>> = Vec::with_capacity(rows.len());
        for (row_index, row) in rows.iter().enumerate() {
            let mut effective_row = Vec::with_capacity(row.len());
            for (cell_index, cell) in row.iter().enumerate() {
                effective_row.push(
                    row_markups
                        .get(row_index)
                        .and_then(|cells| cells.get(cell_index))
                        .cloned()
                        .unwrap_or_else(|| cell.clone()),
                );
            }
            effective_row_markups.push(effective_row);
        }
        let column_count = headers.len().max(1);
        let row_count = rows.len().saturating_add(1).max(1);
        Self {
            column_count,
            header_markups: effective_header_markups.clone(),
            row_count,
            row_markups: effective_row_markups.clone(),
            layout_width: 0,
            children: vec![Box::new(MarkdownTableContentBlock::new(
                headers,
                effective_header_markups,
                rows,
                effective_row_markups,
            ))],
        }
    }
}

impl Widget for MarkdownTableBlock {
    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        Segments::new()
    }

    fn style_type(&self) -> &'static str {
        "MarkdownTable"
    }

    fn style_type_aliases(&self) -> &[&'static str] {
        &["MarkdownBlock"]
    }

    fn on_layout(&mut self, width: u16, _height: u16) {
        if width > 1 {
            self.layout_width = usize::from(width);
        }
    }

    fn layout_height(&self) -> Option<usize> {
        Some(estimate_markdown_table_height(
            &self.header_markups,
            &self.row_markups,
            self.layout_width.max(1),
            self.column_count,
            self.row_count,
        ))
    }

    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        std::mem::take(&mut self.children)
    }
}

/// Build the per-block child widgets for a Markdown document.
///
/// Blockquotes are handled here (Python renders them with a `▌` border bar via a
/// dedicated `MarkdownBlockQuote` widget). To keep all other block types using
/// the shared `parse_markdown_blocks` path unchanged, we walk the top-level event
/// stream: blockquote spans are turned into [`MarkdownBlockQuoteBlock`], and every
/// other top-level span is delegated to `parse_markdown_blocks` on its source
/// slice so document order is preserved.
fn build_markdown_children(markup: &str) -> Vec<Box<dyn Widget>> {
    let mut options = MdOptions::empty();
    options.insert(MdOptions::ENABLE_TABLES);
    options.insert(MdOptions::ENABLE_STRIKETHROUGH);
    options.insert(MdOptions::ENABLE_TASKLISTS);
    options.insert(MdOptions::ENABLE_HEADING_ATTRIBUTES);

    let mut top_level = MdParser::new_ext(markup, options)
        .into_offset_iter()
        .peekable();
    let mut children: Vec<Box<dyn Widget>> = Vec::new();

    while let Some((event, range)) = top_level.next() {
        match &event {
            MdEvent::Start(MdTag::BlockQuote(_)) => {
                // Re-parse the blockquote source span into a nested quote tree so
                // we control the `▌` bar/indent rendering ourselves.
                let mut inner_opts = MdOptions::empty();
                inner_opts.insert(MdOptions::ENABLE_TABLES);
                inner_opts.insert(MdOptions::ENABLE_STRIKETHROUGH);
                inner_opts.insert(MdOptions::ENABLE_TASKLISTS);
                inner_opts.insert(MdOptions::ENABLE_HEADING_ATTRIBUTES);
                let slice = markup.get(range.clone()).unwrap_or("");
                let mut inner = MdParser::new_ext(slice, inner_opts).peekable();
                // Advance to the opening BlockQuote of the slice.
                let mut quote_children = Vec::new();
                while let Some(ev) = inner.next() {
                    if let MdEvent::Start(MdTag::BlockQuote(_)) = ev {
                        quote_children = parse_quote_children(&mut inner);
                        break;
                    }
                }
                if !quote_children.is_empty() {
                    children.push(Box::new(MarkdownBlockQuoteBlock::new(quote_children)));
                }
                // Skip the rest of this blockquote's events in the top-level stream.
                skip_until_blockquote_end(&mut top_level);
            }
            MdEvent::Start(_) | MdEvent::Rule => {
                let slice = markup.get(range.clone()).unwrap_or("");
                for block in parse_markdown_blocks(slice) {
                    push_block_widget(&mut children, block);
                }
                // Consume the rest of this top-level block's events.
                skip_balanced_block(&mut top_level, &event);
            }
            _ => {}
        }
    }
    children
}

/// Skip events until the matching `End(BlockQuote)` for the blockquote whose
/// `Start` was just consumed (balanced over nested blockquotes).
fn skip_until_blockquote_end<'a, I>(parser: &mut std::iter::Peekable<I>)
where
    I: Iterator<Item = (MdEvent<'a>, std::ops::Range<usize>)>,
{
    let mut depth = 1usize;
    for (event, _) in parser.by_ref() {
        match event {
            MdEvent::Start(MdTag::BlockQuote(_)) => depth += 1,
            MdEvent::End(MdTagEnd::BlockQuote(_)) => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            _ => {}
        }
    }
}

/// Consume the remaining events of a top-level container block whose `Start`
/// was just read, stopping after its matching `End`. Leaf events (`Rule`) and
/// blocks already fully consumed are handled by the caller.
fn skip_balanced_block<'a, I>(parser: &mut std::iter::Peekable<I>, start: &MdEvent<'a>)
where
    I: Iterator<Item = (MdEvent<'a>, std::ops::Range<usize>)>,
{
    let target_end = match start {
        MdEvent::Start(tag) => tag.to_end(),
        _ => return,
    };
    let mut depth = 1usize;
    for (event, _) in parser.by_ref() {
        match event {
            MdEvent::Start(tag) if tag.to_end() == target_end => depth += 1,
            MdEvent::End(end) if end == target_end => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            _ => {}
        }
    }
}

fn push_block_widget(children: &mut Vec<Box<dyn Widget>>, block: MarkdownBlock) {
    match block {
        MarkdownBlock::Heading { level, text, .. } => {
            children.push(Box::new(MarkdownHeadingBlock::new(level, text)));
        }
        MarkdownBlock::Paragraph { raw, .. } => {
            children.push(Box::new(MarkdownParagraphBlock::new(raw)));
        }
        MarkdownBlock::List {
            ordered,
            items,
            item_markups,
            ..
        } => {
            children.push(Box::new(MarkdownListBlock::new(
                ordered,
                items,
                item_markups,
            )));
        }
        MarkdownBlock::Table {
            headers,
            header_markups,
            rows,
            row_markups,
            ..
        } => {
            children.push(Box::new(MarkdownTableBlock::new(
                headers,
                header_markups,
                rows,
                row_markups,
            )));
        }
        MarkdownBlock::CodeFence { raw, .. } => {
            children.push(Box::new(MarkdownFenceBlock::new(raw)));
        }
        MarkdownBlock::HorizontalRule => {
            children.push(Box::new(MarkdownHorizontalRuleBlock::new()));
        }
    }
}

impl Markdown {
    fn measure_intrinsic_height(&self, width: usize) -> usize {
        let width = width.max(1);
        let mut children = build_markdown_children(&self.markup);
        if children.is_empty() {
            return 1;
        }

        let parent_meta = crate::css::selector_meta_generic(self);
        let parent_style = crate::css::resolve_style(self, &parent_meta);
        crate::css::push_style_context(parent_meta, parent_style);

        let layout_width = width.min(u16::MAX as usize) as u16;
        let mut total = 0usize;
        let mut prev_bottom = 0usize;
        for (idx, child) in children.iter_mut().enumerate() {
            let child_meta = crate::css::selector_meta_generic(child.as_ref());
            let child_style = crate::css::resolve_style(child.as_ref(), &child_meta);
            let margin = child_style.effective_margin();
            let margin_top = margin.top as usize;
            let margin_bottom = margin.bottom as usize;
            let padding = child_style.effective_padding();
            let (_border_top, _border_bottom, border_left, border_right) =
                border_spacing_from_style(&child_style);
            let horizontal_inset = margin.left as usize
                + margin.right as usize
                + padding.left as usize
                + padding.right as usize
                + border_left
                + border_right;
            let child_content_width = (layout_width as usize)
                .saturating_sub(horizontal_inset)
                .max(1)
                .min(u16::MAX as usize) as u16;
            child.on_layout(child_content_width, 1);
            let child_height = child.layout_height().unwrap_or(1).max(1);

            if idx == 0 {
                total = total.saturating_add(margin_top);
            } else {
                total = total.saturating_add(prev_bottom.max(margin_top));
            }
            total = total.saturating_add(child_height);
            prev_bottom = margin_bottom;
        }
        total = total.saturating_add(prev_bottom);

        crate::css::pop_style_context();
        total.max(1)
    }

    fn recompute_intrinsic_height(&mut self) {
        self.intrinsic_height = self.measure_intrinsic_height(self.layout_width.max(1));
    }

    pub fn new(markup: impl Into<String>) -> Self {
        let markup = markup.into();
        let composed_children = build_markdown_children(&markup);
        let mut markdown = Self {
            markup,
            shared_markup: None,
            layout_width: 1,
            intrinsic_height: 1,
            can_focus: false,
            composed_children,
            pending_recompose: false,
            seed: NodeSeed::default(),
        };
        markdown.recompute_intrinsic_height();
        markdown
    }

    /// Create a Markdown widget with shared content driven by a parent widget.
    ///
    /// The parent (e.g. `MarkdownViewer`) writes new content into the `Arc<RwLock<String>>`,
    /// and `on_layout()` syncs `self.markup` from it before the next height computation.
    pub fn with_shared_markup(shared: Arc<RwLock<String>>) -> Self {
        let initial = shared.read().map(|s| s.clone()).unwrap_or_default();
        let composed_children = build_markdown_children(&initial);
        let mut markdown = Self {
            markup: initial,
            shared_markup: Some(shared),
            layout_width: 1,
            intrinsic_height: 1,
            can_focus: false,
            composed_children,
            pending_recompose: false,
            seed: NodeSeed::default(),
        };
        markdown.recompute_intrinsic_height();
        markdown
    }

    pub fn with_can_focus(mut self, can_focus: bool) -> Self {
        self.can_focus = can_focus;
        self
    }

    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.seed.css_id = Some(id.into());
        self
    }

    pub fn set_markup(&mut self, markup: impl Into<String>) {
        self.markup = markup.into();
        self.composed_children = build_markdown_children(&self.markup);
        self.recompute_intrinsic_height();
        self.pending_recompose = true;
    }

    /// Extract all headings from the markdown as `(level, title)` pairs.
    ///
    /// Used by `MarkdownTableOfContents` to build the sidebar tree.
    pub fn extract_headings(&self) -> Vec<(usize, String)> {
        parse_markdown_headings(&self.markup)
    }
}

impl Widget for Markdown {
    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        Segments::new()
    }

    fn focusable(&self) -> bool {
        self.can_focus
    }

    fn can_focus(&self) -> bool {
        self.can_focus
    }

    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        std::mem::take(&mut self.composed_children)
    }

    fn on_layout(&mut self, width: u16, _height: u16) {
        if let Some(shared) = self.shared_markup.clone() {
            if let Ok(current) = shared.read()
                && *current != self.markup
            {
                let new_content = current.clone();
                drop(current);
                self.set_markup(new_content);
            }
        }
        if width > 1 {
            let layout_width = usize::from(width);
            self.layout_width = layout_width;
            self.recompute_intrinsic_height();
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if matches!(event, Event::Tick(_)) && self.pending_recompose {
            ctx.request_recompose();
            self.pending_recompose = false;
        }
    }

    fn layout_height(&self) -> Option<usize> {
        Some(self.intrinsic_height)
    }

    fn content_width(&self) -> Option<usize> {
        None
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }

    fn allow_select(&self) -> bool {
        false
    }
}

impl Renderable for Markdown {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}
#[cfg(test)]
mod tests {
    use super::{Label, Markdown, MarkdownTableCell, MarkdownTableContentBlock};
    use crate::widgets::Widget;
    use rich_rs::Console;

    #[test]
    fn markdown_layout_height_tracks_composed_block_geometry() {
        let mut markdown = Markdown::new(
            r#"
# Markdown Viewer

## Features

- Typography *emphasis*, **strong**, `inline code` etc.
- Headers
- Lists (bullet and ordered)
- Syntax highlighted code blocks
- Tables!

## Tables

| Name         | Type | Default | Description |
|--------------|------|---------|-------------|
| show_header  | bool | True    | Show the table header |
| fixed_rows   | int  | 0       | Number of fixed rows |
| fixed_columns| int  | 0       | Number of fixed columns |
| zebra_stripes| bool | False   | Display alternating colors on rows |
| header_height| int  | 1       | Height of header row |
| show_cursor  | bool | True    | Show a cell cursor |

## Code Blocks

```python
class ListViewExample(App):
    def compose(self) -> ComposeResult:
        yield ListView(
            ListItem(Label("One")),
            ListItem(Label("Two")),
            ListItem(Label("Three")),
        )
        yield Footer()
```

## Litany Against Fear

I must not fear. Fear is the mind-killer. Fear is the little-death that brings total obliteration.
"#,
        );
        markdown.on_layout(47, 24);
        let measured = markdown.layout_height().expect("markdown height");
        let source_lines = markdown.markup.lines().count().max(1);
        assert!(
            measured < source_lines,
            "intrinsic markdown height should reflect composed markdown blocks (collapsed table/list/code structure), not raw source line count"
        );
    }

    #[test]
    fn markdown_focusable_when_enabled() {
        let markdown = Markdown::new("# Heading").with_can_focus(true);
        assert!(markdown.focusable());
        assert!(markdown.can_focus());
        let markdown_not_focusable = Markdown::new("# Heading");
        assert!(!markdown_not_focusable.focusable());
        assert!(!markdown_not_focusable.can_focus());
    }

    #[test]
    fn inline_text_doc_marks_link_runs_with_link_class() {
        let doc = super::InlineTextDoc::parse("See [example.md](./example.md) for details.");
        assert!(
            doc.runs
                .iter()
                .any(|run| run.classes.iter().any(|class| *class == "link")),
            "expected at least one inline run to carry the link class"
        );
    }

    #[test]
    fn inline_text_doc_link_coords_resolve_href() {
        let doc = super::InlineTextDoc::parse("See [example.md](./example.md) for details.");
        assert_eq!(doc.link_at_coords(5, 0, 80), Some("./example.md"));
        assert_eq!(doc.link_at_coords(0, 0, 80), None);
    }

    #[test]
    fn markdown_paragraph_renders_click_action_meta_for_links() {
        let paragraph = super::MarkdownParagraphBlock::new(
            "See [example.md](./example.md) for details.".to_string(),
        );
        let console = Console::new();
        let mut options = console.options().clone();
        options.size = (80, 1);
        options.max_width = 80;
        options.max_height = 1;
        let rendered = paragraph.render(&console, &options);
        let found = rendered.iter().any(|seg| {
            seg.meta
                .as_ref()
                .and_then(|meta| meta.meta.as_ref())
                .and_then(|meta| meta.get("@click"))
                == Some(&rich_rs::MetaValue::str("link('./example.md')"))
        });
        assert!(
            found,
            "expected markdown links to carry @click action metadata"
        );
    }

    #[test]
    fn markdown_paragraph_link_default_background_is_transparent() {
        let paragraph = super::MarkdownParagraphBlock::new(
            "See [example.md](./example.md) for details.".to_string(),
        );
        let console = Console::new();
        let mut options = console.options().clone();
        options.size = (80, 1);
        options.max_width = 80;
        options.max_height = 1;
        let rendered = paragraph.render(&console, &options);
        let link_segment = rendered
            .iter()
            .find(|seg| seg.text.as_ref().contains("example.md"))
            .expect("expected rendered link segment");
        let bg = link_segment.style.and_then(|s| s.bgcolor);
        assert!(
            bg.is_none(),
            "transparent link background should not force an opaque background color"
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
    fn markdown_table_cell_tooltip_anchor_uses_local_center() {
        let mut cell = MarkdownTableCell::new(
            "True".to_string(),
            "True".to_string(),
            vec!["cell".to_string()],
        );
        cell.on_layout(12, 1);
        assert_eq!(cell.tooltip_anchor(), Some((6, 0)));
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
    fn label_auto_content_width_reports_rendered_text_width() {
        // `width: auto` measurement sizes to the rendered text width (Python
        // parity) via `auto_content_width()`, without turning the unset-width
        // fill default into a content-width hint (`content_width()` stays None).
        let text = "I must not fear.";
        let label = Label::new(text);
        assert_eq!(label.content_width(), None);
        assert_eq!(
            Widget::auto_content_width(&label),
            Some(rich_rs::cell_len(text))
        );
    }

    #[test]
    fn markdown_bullet_render_has_explicit_style() {
        let mut root =
            crate::widgets::Container::new().with_child(Markdown::new("- first\n- second"));
        let mut tree =
            crate::runtime::build_widget_tree_from_root(&mut root).expect("tree should exist");
        let console = Console::new();
        let frame = crate::runtime::render_tree_to_frame(&mut tree, &mut root, &console, 40, 8);
        let lines = frame.as_plain_lines();
        let (row, col) = lines
            .iter()
            .enumerate()
            .find_map(|(row, line)| line.find('•').map(|col| (row, col)))
            .expect("expected bullet glyph in rendered markdown list");
        let bullet_style = frame
            .get(col, row)
            .style
            .expect("bullet cell should carry resolved style");
        assert!(
            bullet_style.color.is_some(),
            "bullet should resolve an explicit foreground color"
        );
    }

    #[test]
    fn markdown_table_header_style_differs_from_cell_style() {
        let mut root = crate::widgets::Container::new().with_child(Markdown::new(
            "| Name | Type |\n| --- | --- |\n| show_header | bool |\n| fixed_rows | int |\n",
        ));
        let mut tree =
            crate::runtime::build_widget_tree_from_root(&mut root).expect("tree should exist");
        let console = Console::new();
        let frame = crate::runtime::render_tree_to_frame(&mut tree, &mut root, &console, 80, 16);
        let lines = frame.as_plain_lines();
        let (header_row, header_col) = lines
            .iter()
            .enumerate()
            .find_map(|(row, line)| line.find("Name").map(|col| (row, col)))
            .expect("header text should exist");
        let (cell_row, cell_col) = lines
            .iter()
            .enumerate()
            .find_map(|(row, line)| line.find("show_header").map(|col| (row, col)))
            .expect("data cell text should exist");
        let header_style = frame
            .get(header_col, header_row)
            .style
            .expect("header style");
        let cell_style = frame.get(cell_col, cell_row).style.expect("cell style");
        assert_ne!(
            header_style.color, cell_style.color,
            "header foreground should differ from body cell foreground"
        );
    }

    #[test]
    fn markdown_table_content_sets_non_uniform_grid_column_weights() {
        let content = MarkdownTableContentBlock::new(
            vec![
                "Name".to_string(),
                "Type".to_string(),
                "Default".to_string(),
                "Description".to_string(),
            ],
            vec![
                "Name".to_string(),
                "Type".to_string(),
                "Default".to_string(),
                "Description".to_string(),
            ],
            vec![
                vec![
                    "`show_header`".to_string(),
                    "`bool`".to_string(),
                    "`True`".to_string(),
                    "Show the table header".to_string(),
                ],
                vec![
                    "`fixed_columns`".to_string(),
                    "`int`".to_string(),
                    "`0`".to_string(),
                    "Number of fixed columns".to_string(),
                ],
            ],
            vec![
                vec![
                    "`show_header`".to_string(),
                    "`bool`".to_string(),
                    "`True`".to_string(),
                    "Show the table header".to_string(),
                ],
                vec![
                    "`fixed_columns`".to_string(),
                    "`int`".to_string(),
                    "`0`".to_string(),
                    "Number of fixed columns".to_string(),
                ],
            ],
        );

        let style = content.seed.styles.style.clone();
        let columns = style.grid_columns.as_ref().expect("grid columns");
        assert_eq!(columns.len(), 4);
        let first_weight = match columns.first().expect("first column") {
            crate::style::Scalar::Fraction(weight) => *weight,
            _ => panic!("expected fraction column width"),
        };
        let has_distinct_weight = columns.iter().any(|column| match column {
            crate::style::Scalar::Fraction(weight) => (weight - first_weight).abs() > f32::EPSILON,
            _ => false,
        });
        assert!(
            has_distinct_weight,
            "table content should assign non-uniform column weights from cell contents"
        );
    }

    #[test]
    fn markdown_table_fraction_weights_keep_type_and_default_columns_readable() {
        let fractions = super::compute_markdown_table_column_fractions(
            &[
                "Name".to_string(),
                "Type".to_string(),
                "Default".to_string(),
                "Description".to_string(),
            ],
            &[
                vec![
                    "`show_header`".to_string(),
                    "`bool`".to_string(),
                    "`True`".to_string(),
                    "Show the table header".to_string(),
                ],
                vec![
                    "`fixed_columns`".to_string(),
                    "`int`".to_string(),
                    "`0`".to_string(),
                    "Number of fixed columns".to_string(),
                ],
            ],
            4,
        );
        let weights: Vec<f32> = fractions
            .iter()
            .map(|scalar| match scalar {
                crate::style::Scalar::Fraction(value) => *value,
                _ => panic!("expected fraction scalar"),
            })
            .collect();
        assert_eq!(weights.len(), 4);
        assert!(
            weights[1] >= 6.0,
            "type column should preserve a readable weight"
        );
        assert!(
            weights[2] >= 9.0,
            "default column should preserve a readable weight"
        );
        assert!(
            weights[3] > weights[1] && weights[3] > weights[2],
            "description column should still be the widest"
        );
    }

    #[test]
    fn markdown_table_column_compaction_preserves_narrow_semantic_columns() {
        let headers = vec![
            "Name".to_string(),
            "Type".to_string(),
            "Default".to_string(),
            "Description".to_string(),
        ];
        let rows = vec![
            vec![
                "`show_header`".to_string(),
                "`bool`".to_string(),
                "`True`".to_string(),
                "Show the table header".to_string(),
            ],
            vec![
                "`fixed_columns`".to_string(),
                "`int`".to_string(),
                "`0`".to_string(),
                "Number of fixed columns".to_string(),
            ],
        ];
        let widths = super::compute_markdown_table_column_widths(&headers, &rows, 47, 4);
        assert_eq!(widths.len(), 4);
        assert!(
            widths[1] >= 6,
            "type column should retain readable width under compaction"
        );
        assert!(
            widths[2] >= 9,
            "default column should retain readable width under compaction"
        );
        assert!(
            widths[3] >= widths[1] && widths[3] >= widths[2],
            "description column should absorb most tight-width shrink"
        );
    }

    #[test]
    fn markdown_table_content_style_survives_tree_build() {
        let mut root = crate::widgets::Container::new().with_child(Markdown::new(
            "| Name | Type | Default | Description |\n| --- | --- | --- | --- |\n| `show_header` | `bool` | `True` | Show the table header |\n| `fixed_columns` | `int` | `0` | Number of fixed columns |\n",
        ));
        let tree =
            crate::runtime::build_widget_tree_from_root(&mut root).expect("tree should exist");
        let node_id = *tree
            .query("MarkdownTableContent")
            .expect("query should parse")
            .first()
            .expect("table content node should exist");
        let styles = tree.styles(node_id).expect("table content node styles");
        let style = styles.style.clone();
        assert_eq!(style.grid_size_columns, Some(4));
        let columns = style.grid_columns.expect("grid columns");
        assert_eq!(columns.len(), 4);
    }

    #[test]
    fn markdown_nested_blockquote_renders_bar_and_indent() {
        // A blockquote span must produce a single `MarkdownBlockQuote` block.
        let markup = "> a\n> > b\n> > > c\n";
        let children = super::build_markdown_children(markup);
        assert_eq!(
            children
                .iter()
                .filter(|c| c.style_type() == "MarkdownBlockQuote")
                .count(),
            1,
            "exactly one MarkdownBlockQuote block should be produced"
        );

        // Nested blockquotes must render with one `▌` bar per nesting level,
        // plus blank bar lines around each nested quote (Python parity). The
        // outermost bar + left padding come from the `MarkdownBlockQuote`
        // default CSS, so this widget's own content starts at depth 0.
        let mut inner = pulldown_cmark::Parser::new(markup).peekable();
        let mut quote_children = Vec::new();
        while let Some(ev) = inner.next() {
            if let pulldown_cmark::Event::Start(pulldown_cmark::Tag::BlockQuote(_)) = ev {
                quote_children = super::parse_quote_children(&mut inner);
                break;
            }
        }
        let lines = super::MarkdownBlockQuoteBlock::render_lines(&quote_children, 0, 80);
        assert_eq!(
            lines,
            vec![
                "a".to_string(),     // depth 0 paragraph (CSS adds outer bar)
                String::new(),       // margin before nested quote (CSS adds bar)
                "▌ b".to_string(),   // depth 1 paragraph
                "▌".to_string(),     // margin before deeper quote
                "▌ ▌ c".to_string(), // depth 2 paragraph
                "▌".to_string(),     // margin after depth-2 quote
                String::new(),       // margin after depth-1 quote
            ]
        );
    }

    #[test]
    fn markdown_blockquote_preserves_document_order() {
        // Blocks before/after a blockquote must keep their order and type.
        let markup = "# Title\n\n> quote\n\nAfter.\n";
        let children = super::build_markdown_children(markup);
        let types: Vec<&str> = children.iter().map(|c| c.style_type()).collect();
        assert_eq!(types, vec!["MarkdownH1", "MarkdownBlockQuote", "MarkdownParagraph"]);
    }

    #[test]
    fn intrinsic_wrapped_height_counts_trailing_blank_line() {
        // Python `Content.split(allow_blank=True)`: text ending in '\n' keeps a
        // final empty line. Rust `str::lines()` drops it, so without the fix
        // "a\nb\n" measures 2 (Rust) vs 3 (Python). No-wrap path:
        assert_eq!(super::intrinsic_wrapped_height("a\nb", 20, false), 2);
        assert_eq!(super::intrinsic_wrapped_height("a\nb\n", 20, false), 3);
        // Repeated block (TEXT * N where TEXT ends in '\n'): the intermediate
        // '\n's are line separators (3 lines per block), and the WHOLE string ends
        // in '\n' so exactly ONE trailing blank is added — matching Python's
        // `Label(TEXT * N)` height (3*N + 1), not `str::lines()`'s 3*N.
        let block = "x\ny\nz\n";
        let n = 10;
        let text = block.repeat(n);
        assert_eq!(super::intrinsic_wrapped_height(&text, 20, false), 3 * n + 1);
    }

    #[test]
    fn label_layout_height_counts_trailing_blank_line() {
        // `Label(TEXT)` whose TEXT ends in '\n' must include the trailing blank
        // row in its auto/content height — the keystone for the scrollbars2
        // scroll geometry (Python 71 vs Rust 70 before the fix).
        let no_nl = Label::new("line1\nline2");
        let with_nl = Label::new("line1\nline2\n");
        // Wide layout so wrapping does not add rows; isolate the trailing-blank.
        let mut a = no_nl;
        let mut b = with_nl;
        a.on_layout(40, 1);
        b.on_layout(40, 1);
        let ha = a.layout_height().expect("height");
        let hb = b.layout_height().expect("height");
        assert_eq!(hb, ha + 1, "trailing '\\n' must add exactly one blank row");
    }
}
