use rich_rs::markdown::Markdown as RichMarkdown;
use rich_rs::{Console, ConsoleOptions, Renderable, Segments, Text};
use std::collections::VecDeque;

use super::{Widget, WidgetId, WidgetStyles, helpers::fixed_height_from_constraints};

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
    id: WidgetId,
    text: String,
    wrap: bool,
    layout_width: usize,
    variant: Option<LabelVariant>,
    classes: Vec<String>,
    styles: WidgetStyles,
}

impl Label {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            id: WidgetId::new(),
            text: text.into(),
            wrap: true,
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
}

impl Widget for Label {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let text = Text::plain(self.text.clone());
        text.render(console, options)
    }

    fn on_layout(&mut self, width: u16, _height: u16) {
        self.layout_width = usize::from(width).max(1);
    }

    fn content_width(&self) -> Option<usize> {
        Some(
            self.text
                .lines()
                .map(rich_rs::cell_len)
                .max()
                .unwrap_or(0)
                .max(1),
        )
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

#[derive(Debug, Clone)]
pub struct Markdown {
    id: WidgetId,
    markup: String,
    layout_width: usize,
    styles: WidgetStyles,
}

impl Markdown {
    pub fn new(markup: impl Into<String>) -> Self {
        Self {
            id: WidgetId::new(),
            markup: markup.into(),
            layout_width: 0,
            styles: WidgetStyles::default(),
        }
    }

    pub fn set_markup(&mut self, markup: impl Into<String>) {
        self.markup = markup.into();
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
}

impl Widget for Markdown {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let rendered = RichMarkdown::new(self.markup.clone()).render(console, options);

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

        if headings.is_empty() {
            return rendered;
        }

        let mut lines = rich_rs::Segment::split_lines(rendered);
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
            let style = crate::css::resolve_component_style(self, &[class_name.as_str()])
                .to_rich()
                .unwrap_or_else(rich_rs::Style::new);
            for segment in line.iter_mut().filter(|segment| segment.control.is_none()) {
                segment.style = Some(segment.style.unwrap_or_default().combine(&style));
            }
        }

        let mut out = Segments::new();
        let line_count = lines.len();
        for (index, line) in lines.into_iter().enumerate() {
            out.extend(line);
            if index + 1 < line_count {
                out.push(rich_rs::Segment::line());
            }
        }
        out
    }

    fn on_layout(&mut self, width: u16, _height: u16) {
        self.layout_width = usize::from(width).max(1);
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
        let width = self
            .markup
            .lines()
            .map(rich_rs::cell_len)
            .max()
            .unwrap_or(0)
            .max(1);
        Some(width)
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

impl Renderable for Markdown {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}
