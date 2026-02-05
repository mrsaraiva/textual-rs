use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use super::{Widget, WidgetId, WidgetStyles, helpers::fixed_height_from_constraints};

pub struct Spacer {
    id: WidgetId,
    height: usize,
    styles: WidgetStyles,
}

impl Spacer {
    pub fn new(height: usize) -> Self {
        Self {
            id: WidgetId::new(),
            height: height.max(1),
            styles: WidgetStyles::default(),
        }
    }
}

impl Widget for Spacer {
    fn id(&self) -> WidgetId {
        self.id
    }

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
