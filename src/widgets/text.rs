use pulldown_cmark::{
    Event as MdEvent, Options as MdOptions, Parser as MdParser, Tag as MdTag, TagEnd as MdTagEnd,
};
use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments, Text};
use std::sync::{Arc, RwLock};
use unicode_width::UnicodeWidthChar;

use crate::event::{Event, EventCtx};
use crate::message::ActionDispatchRequested;
use crate::widgets::markdown_model::{
    MarkdownBlock, parse_markdown_blocks, parse_markdown_headings,
};

use super::{
    NodeSeed, Vertical, Widget, WidgetStyles,
    helpers::border_spacing_from_style,
};

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
    seed: NodeSeed,
}

impl Label {
    pub fn new(text: impl Into<String>) -> Self {
        let mut seed = NodeSeed::default();
        seed.classes = vec!["label".to_string()];
        Self {
            text: text.into(),
            wrap: true,
            markup: false,
            expand: false,
            // Match Textual Label defaults: labels don't shrink to intrinsic width
            // unless explicitly requested.
            shrink: false,
            layout_width: 0,
            variant: None,
            seed,
        }
    }

    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.seed.css_id = Some(id.into());
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
        self.seed.classes = vec!["label".to_string()];
        if let Some(v) = self.variant {
            self.seed.classes.push(v.css_class().to_string());
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
        Some(self.intrinsic_height())
    }

    fn style_classes(&self) -> &[String] {
        &self.seed.classes
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.seed.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.seed.styles)
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        let seed = std::mem::take(&mut self.seed);
        self.seed.classes = seed.classes.clone();
        seed
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

fn rendered_plain_height(text: &str, width: usize) -> usize {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (width.max(1), 1);
    options.max_width = width.max(1);
    options.max_height = 1;
    let rendered = Text::plain(text.to_string()).render(&console, &options);
    count_rendered_lines(rendered)
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
                        if bg.a > 0 {
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
                        if bg.a > 0 {
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
    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        Text::plain(self.text.clone()).render(_console, _options)
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
        Some(rendered_plain_height(
            &self.text,
            self.layout_width.max(1),
        ))
    }

    fn style_classes(&self) -> &[String] {
        &self.seed.classes
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.seed.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.seed.styles)
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        let seed = std::mem::take(&mut self.seed);
        self.seed.classes = seed.classes.clone();
        seed
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

    fn style_classes(&self) -> &[String] {
        &self.seed.classes
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

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.seed.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.seed.styles)
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        let seed = std::mem::take(&mut self.seed);
        self.seed.classes = seed.classes.clone();
        seed
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

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.seed.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.seed.styles)
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

fn build_markdown_children(markup: &str) -> Vec<Box<dyn Widget>> {
    let mut children: Vec<Box<dyn Widget>> = Vec::new();
    for block in parse_markdown_blocks(markup) {
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
    children
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

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.seed.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.seed.styles)
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

        let style = content.styles().expect("table content styles").style.clone();
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
}
