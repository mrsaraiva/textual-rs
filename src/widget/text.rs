use rich_rs::markdown::Markdown as RichMarkdown;
use rich_rs::{Console, ConsoleOptions, Renderable, Segments, Text};

use super::{Widget, WidgetId, WidgetStyles, helpers::fixed_height_from_constraints};

#[derive(Debug, Clone)]
pub struct Label {
    id: WidgetId,
    text: String,
    styles: WidgetStyles,
}

impl Label {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            id: WidgetId::new(),
            text: text.into(),
            styles: WidgetStyles::default(),
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
    }
}

impl Widget for Label {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let text = Text::plain(&self.text);
        text.render(console, options)
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

impl Renderable for Label {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[derive(Debug, Clone)]
pub struct Markdown {
    id: WidgetId,
    markup: String,
    styles: WidgetStyles,
}

impl Markdown {
    pub fn new(markup: impl Into<String>) -> Self {
        Self {
            id: WidgetId::new(),
            markup: markup.into(),
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
        RichMarkdown::new(self.markup.clone()).render(console, options)
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
