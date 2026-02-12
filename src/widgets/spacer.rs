use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use super::{Widget, WidgetStyles, helpers::fixed_height_from_constraints};

pub struct Spacer {
    height: usize,
    width_hint: Option<usize>,
    styles: WidgetStyles,
}

impl Spacer {
    pub fn new(height: usize) -> Self {
        Self {
            height: height.max(1),
            width_hint: None,
            styles: WidgetStyles::default(),
        }
    }

    pub fn width(mut self, width: usize) -> Self {
        self.width_hint = Some(width.max(1));
        self
    }
}

impl Widget for Spacer {
    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let line = " ".repeat(width);
        let mut out = Segments::new();
        for idx in 0..self.height {
            out.push(Segment::new(line.clone()));
            if idx + 1 < self.height {
                out.push(Segment::line());
            }
        }
        out
    }

    fn layout_height(&self) -> Option<usize> {
        fixed_height_from_constraints(self.layout_constraints()).or(Some(self.height))
    }

    fn content_width(&self) -> Option<usize> {
        Some(self.width_hint.unwrap_or(1).max(1))
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

impl Renderable for Spacer {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}
