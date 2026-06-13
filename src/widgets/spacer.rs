use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use super::{NodeSeed, Widget};

pub struct Spacer {
    height: usize,
    width_hint: Option<usize>,
    seed: NodeSeed,
}

impl Spacer {
    pub fn new(height: usize) -> Self {
        Self {
            height: height.max(1),
            width_hint: None,
            seed: NodeSeed::default(),
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
        Some(self.height)
    }

    fn content_width(&self) -> Option<usize> {
        Some(self.width_hint.unwrap_or(1).max(1))
    }

    fn set_inline_style(&mut self, style: crate::style::Style) {
        self.seed.styles.style = style;
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }
}

impl Renderable for Spacer {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}
