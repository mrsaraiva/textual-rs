use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments, Text};

use super::helpers::{adjust_line_length_no_bg, empty_classes, fixed_height_from_constraints};
use super::{Widget, WidgetId, WidgetStyles};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FooterBinding {
    pub key: String,
    pub description: String,
}

impl FooterBinding {
    pub fn new(key: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            description: description.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Footer {
    id: WidgetId,
    bindings: Vec<FooterBinding>,
    compact: bool,
    classes: Vec<String>,
    styles: WidgetStyles,
}

impl Footer {
    pub fn new() -> Self {
        Self {
            id: WidgetId::new(),
            bindings: Vec::new(),
            compact: false,
            classes: Vec::new(),
            styles: WidgetStyles::default(),
        }
    }

    pub fn with_binding(mut self, key: impl Into<String>, description: impl Into<String>) -> Self {
        self.bindings.push(FooterBinding::new(key, description));
        self
    }

    pub fn set_bindings(&mut self, bindings: Vec<FooterBinding>) {
        self.bindings = bindings;
    }

    pub fn clear_bindings(&mut self) {
        self.bindings.clear();
    }

    pub fn compact(mut self, compact: bool) -> Self {
        self.compact = compact;
        self
    }

    fn line_text(&self) -> String {
        if self.bindings.is_empty() {
            return String::new();
        }

        let separator = if self.compact { " " } else { "   " };
        let mut line = String::new();
        for (index, binding) in self.bindings.iter().enumerate() {
            if index > 0 {
                line.push_str(separator);
            }
            line.push(' ');
            line.push_str(&binding.key);
            line.push(' ');
            line.push_str(&binding.description);
        }
        line
    }
}

impl Widget for Footer {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let text = Text::plain(self.line_text());
        let rendered = text.render(console, options);
        let split = Segment::split_and_crop_lines(rendered, width, None, true, false);
        let mut out = Segments::new();
        if let Some(line) = split.first() {
            out.extend(adjust_line_length_no_bg(line, width));
        } else {
            out.push(Segment::new(" ".repeat(width)));
        }
        out
    }

    fn layout_height(&self) -> Option<usize> {
        fixed_height_from_constraints(self.layout_constraints()).or(Some(1))
    }

    fn style_classes(&self) -> &[String] {
        if self.classes.is_empty() {
            empty_classes()
        } else {
            &self.classes
        }
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

impl Renderable for Footer {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}
