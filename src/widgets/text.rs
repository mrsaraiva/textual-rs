use rich_rs::markdown::Markdown as RichMarkdown;
use rich_rs::{Console, ConsoleOptions, Renderable, Segments, Text};
use std::collections::VecDeque;

use super::{Widget, WidgetId, WidgetStyles, helpers::fixed_height_from_constraints};

#[derive(Debug, Clone)]
pub struct Label {
    id: WidgetId,
    text: String,
    wrap: bool,
    layout_width: usize,
    styles: WidgetStyles,
}

impl Label {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            id: WidgetId::new(),
            text: text.into(),
            wrap: true,
            layout_width: 0,
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
        for line in &mut lines {
            if headings.is_empty() {
                break;
            }
            let plain = line
                .iter()
                .filter(|segment| segment.control.is_none())
                .map(|segment| segment.text.as_ref())
                .collect::<String>();
            if plain.trim().is_empty() {
                continue;
            }
            let Some((level, title)) = headings.front() else {
                break;
            };
            if plain.trim() != title {
                continue;
            }

            let class_name = format!("markdown--h{level}");
            let style = crate::css::resolve_component_style(self, &[class_name.as_str()])
                .to_rich()
                .unwrap_or_else(rich_rs::Style::new);
            for segment in line.iter_mut().filter(|segment| segment.control.is_none()) {
                segment.style = Some(segment.style.unwrap_or_default().combine(&style));
            }
            headings.pop_front();
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
