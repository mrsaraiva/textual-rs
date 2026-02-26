use pulldown_cmark::{
    Event as MdEvent, Options as MdOptions, Parser as MdParser, Tag as MdTag, TagEnd as MdTagEnd,
};
use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments, Text};
use std::sync::{Arc, RwLock};

use crate::event::{Event, EventCtx};
use crate::widgets::markdown_model::{
    MarkdownBlock, parse_markdown_blocks, parse_markdown_headings,
};

use super::{
    Vertical, Widget, WidgetStyles,
    helpers::{border_spacing_from_style, fixed_height_from_constraints},
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

pub struct Markdown {
    markup: String,
    /// Shared content reference for parent-driven content updates (e.g. from MarkdownViewer).
    /// When set, `on_layout()` syncs `self.markup` from this shared state before computing height.
    shared_markup: Option<Arc<RwLock<String>>>,
    id: Option<String>,
    layout_width: usize,
    intrinsic_height: usize,
    composed_children: Vec<Box<dyn Widget>>,
    pending_recompose: bool,
    styles: WidgetStyles,
}

impl std::fmt::Debug for Markdown {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Markdown")
            .field("markup_len", &self.markup.len())
            .field("id", &self.id)
            .field("pending_recompose", &self.pending_recompose)
            .finish()
    }
}

impl Clone for Markdown {
    fn clone(&self) -> Self {
        let mut cloned = Self {
            markup: self.markup.clone(),
            shared_markup: self.shared_markup.clone(),
            id: self.id.clone(),
            layout_width: self.layout_width,
            intrinsic_height: self.intrinsic_height,
            composed_children: build_markdown_children(&self.markup),
            pending_recompose: self.pending_recompose,
            styles: self.styles.clone(),
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

        for event in parser {
            match event {
                MdEvent::Start(MdTag::Emphasis) => emphasis = emphasis.saturating_add(1),
                MdEvent::End(MdTagEnd::Emphasis) => emphasis = emphasis.saturating_sub(1),
                MdEvent::Start(MdTag::Strong) => strong = strong.saturating_add(1),
                MdEvent::End(MdTagEnd::Strong) => strong = strong.saturating_sub(1),
                MdEvent::Start(MdTag::Strikethrough) => strike = strike.saturating_add(1),
                MdEvent::End(MdTagEnd::Strikethrough) => strike = strike.saturating_sub(1),
                MdEvent::Text(text) => {
                    push_inline_run(
                        &mut runs,
                        collapse_inline_whitespace(&text),
                        emphasis,
                        strong,
                        strike,
                        false,
                    );
                }
                MdEvent::Code(text) => {
                    push_inline_run(&mut runs, text.to_string(), emphasis, strong, strike, true);
                }
                MdEvent::SoftBreak => {
                    push_inline_run(&mut runs, " ".to_string(), emphasis, strong, strike, false);
                }
                MdEvent::HardBreak => {
                    push_inline_run(&mut runs, "\n".to_string(), emphasis, strong, strike, false);
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
    ) -> Segments {
        let mut text = Text::plain(self.plain.clone());
        let mut offset = 0usize;
        for run in &self.runs {
            let start = offset;
            let end = start + run.text.chars().count();
            offset = end;

            if run.classes.is_empty() {
                continue;
            }

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
            if has_style {
                text.stylize(start, end, style);
            }
        }
        text.render(console, options)
    }
}

fn push_inline_run(
    runs: &mut Vec<InlineRun>,
    text: String,
    emphasis: usize,
    strong: usize,
    strike: usize,
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

    if let Some(last) = runs.last_mut()
        && last.classes == classes
    {
        last.text.push_str(&text);
        return;
    }
    runs.push(InlineRun { text, classes });
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

#[derive(Debug)]
struct MarkdownHeadingBlock {
    level: usize,
    text: String,
    layout_width: usize,
    classes: Vec<String>,
    styles: WidgetStyles,
}

impl MarkdownHeadingBlock {
    fn new(level: usize, text: String) -> Self {
        Self {
            level,
            text,
            layout_width: 0,
            classes: vec![format!("markdown--h{}", level.clamp(1, 6))],
            styles: WidgetStyles::default(),
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
        fixed_height_from_constraints(self.layout_constraints()).or(Some(rendered_plain_height(
            &self.text,
            self.layout_width.max(1),
        )))
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

#[derive(Debug)]
struct MarkdownParagraphBlock {
    inline_doc: InlineTextDoc,
    layout_width: usize,
    styles: WidgetStyles,
}

impl MarkdownParagraphBlock {
    fn new(raw: String) -> Self {
        Self {
            inline_doc: InlineTextDoc::parse(&raw),
            layout_width: 0,
            styles: WidgetStyles::default(),
        }
    }
}

impl Widget for MarkdownParagraphBlock {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        self.inline_doc.render_for_widget(self, console, options)
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

    fn layout_height(&self) -> Option<usize> {
        fixed_height_from_constraints(self.layout_constraints()).or(Some(
            self.inline_doc.rendered_height(self.layout_width.max(1)),
        ))
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

#[derive(Debug)]
struct MarkdownFenceBlock {
    raw: String,
    layout_width: usize,
    styles: WidgetStyles,
}

impl MarkdownFenceBlock {
    fn new(raw: String) -> Self {
        Self {
            raw,
            layout_width: 0,
            styles: WidgetStyles::default(),
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
        fixed_height_from_constraints(self.layout_constraints()).or(Some(rendered_markdown_height(
            &self.raw,
            self.layout_width.max(1),
        )))
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

#[derive(Debug)]
struct MarkdownHorizontalRuleBlock {
    styles: WidgetStyles,
}

impl MarkdownHorizontalRuleBlock {
    fn new() -> Self {
        Self {
            styles: WidgetStyles::default(),
        }
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
        fixed_height_from_constraints(self.layout_constraints()).or(Some(1))
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

#[derive(Debug)]
struct MarkdownBullet {
    symbol: String,
    styles: WidgetStyles,
}

impl MarkdownBullet {
    fn new(symbol: String) -> Self {
        Self {
            symbol,
            styles: WidgetStyles::default(),
        }
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

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

#[derive(Debug)]
struct MarkdownInlineItem {
    inline_doc: InlineTextDoc,
    layout_width: usize,
    styles: WidgetStyles,
}

impl MarkdownInlineItem {
    fn new(raw: String) -> Self {
        Self {
            inline_doc: InlineTextDoc::parse(&raw),
            layout_width: 0,
            styles: WidgetStyles::default(),
        }
    }
}

impl Widget for MarkdownInlineItem {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        self.inline_doc.render_for_widget(self, console, options)
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

    fn layout_height(&self) -> Option<usize> {
        fixed_height_from_constraints(self.layout_constraints()).or(Some(
            self.inline_doc.rendered_height(self.layout_width.max(1)),
        ))
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

struct MarkdownListItemBlock {
    symbol: String,
    item_inline_doc: InlineTextDoc,
    layout_width: usize,
    children: Vec<Box<dyn Widget>>,
    styles: WidgetStyles,
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
            styles: WidgetStyles::default(),
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
        fixed_height_from_constraints(self.layout_constraints())
            .or(Some(self.item_inline_doc.rendered_height(content_width)))
    }

    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        std::mem::take(&mut self.children)
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

struct MarkdownListBlock {
    ordered: bool,
    items: Vec<String>,
    item_inline_docs: Vec<InlineTextDoc>,
    layout_width: usize,
    children: Vec<Box<dyn Widget>>,
    styles: WidgetStyles,
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
            styles: WidgetStyles::default(),
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
        if let Some(h) = fixed_height_from_constraints(self.layout_constraints()) {
            return Some(h);
        }
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

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

#[derive(Debug)]
struct MarkdownTableCell {
    text: String,
    inline_doc: InlineTextDoc,
    layout_width: usize,
    classes: Vec<String>,
    styles: WidgetStyles,
}

impl MarkdownTableCell {
    fn new(text: String, raw: String, classes: Vec<String>) -> Self {
        Self {
            text,
            inline_doc: InlineTextDoc::parse(&raw),
            layout_width: 0,
            classes,
            styles: WidgetStyles::default(),
        }
    }
}

impl Widget for MarkdownTableCell {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let mut one_line = options.clone();
        one_line.max_height = 1;
        one_line.size.1 = 1;
        self.inline_doc.render_for_widget(self, console, &one_line)
    }

    fn style_type(&self) -> &'static str {
        "MarkdownTableCell"
    }

    fn style_type_aliases(&self) -> &[&'static str] {
        &["MarkdownBlock"]
    }

    fn style_classes(&self) -> &[String] {
        &self.classes
    }

    fn on_layout(&mut self, width: u16, _height: u16) {
        if width > 1 {
            self.layout_width = usize::from(width);
        }
    }

    fn layout_height(&self) -> Option<usize> {
        let _ = &self.text;
        fixed_height_from_constraints(self.layout_constraints()).or(Some(1))
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
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
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
    styles: WidgetStyles,
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
        Self {
            column_count: column_count as usize,
            header_markups: effective_header_markups,
            row_count,
            row_markups: effective_row_markups,
            layout_width: 0,
            children,
            styles: WidgetStyles {
                style: crate::style::Style {
                    grid_size_columns: Some(column_count),
                    grid_size_rows: Some(row_count as u16),
                    grid_columns: Some(column_fractions),
                    ..Default::default()
                },
                ..Default::default()
            },
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
        self.styles.style.grid_rows = Some(
            row_heights
                .into_iter()
                .map(|height| crate::style::Scalar::Cells(height.min(u16::MAX as usize) as u16))
                .collect(),
        );
    }

    fn layout_height(&self) -> Option<usize> {
        if let Some(h) = fixed_height_from_constraints(self.layout_constraints()) {
            return Some(h);
        }
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
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

struct MarkdownTableBlock {
    column_count: usize,
    header_markups: Vec<String>,
    row_count: usize,
    row_markups: Vec<Vec<String>>,
    layout_width: usize,
    children: Vec<Box<dyn Widget>>,
    styles: WidgetStyles,
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
            styles: WidgetStyles::default(),
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
        if let Some(h) = fixed_height_from_constraints(self.layout_constraints()) {
            return Some(h);
        }
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
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
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
            id: None,
            layout_width: 1,
            intrinsic_height: 1,
            composed_children,
            pending_recompose: false,
            styles: WidgetStyles::default(),
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
            id: None,
            layout_width: 1,
            intrinsic_height: 1,
            composed_children,
            pending_recompose: false,
            styles: WidgetStyles::default(),
        };
        markdown.recompute_intrinsic_height();
        markdown
    }

    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
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
        fixed_height_from_constraints(self.layout_constraints()).or(Some(self.intrinsic_height))
    }

    fn content_width(&self) -> Option<usize> {
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

        let style = content.style().expect("table content style");
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
        let node = tree.get(node_id).expect("table content node");
        let style = node.widget.style().expect("inline style");
        assert_eq!(style.grid_size_columns, Some(4));
        let columns = style.grid_columns.expect("grid columns");
        assert_eq!(columns.len(), 4);
    }
}
