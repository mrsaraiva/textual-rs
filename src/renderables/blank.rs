use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::style::Color;

/// Renderable that paints a solid background surface.
///
/// Rust counterpart to Python Textual `renderables/blank.py`.
#[derive(Debug, Clone, Default)]
pub struct Blank {
    color: Option<Color>,
}

impl Blank {
    pub fn new(color: Color) -> Self {
        Self { color: Some(color) }
    }

    pub fn transparent() -> Self {
        Self { color: None }
    }

    fn style(&self) -> rich_rs::Style {
        match self.color {
            Some(bg) => rich_rs::Style::new().with_bgcolor(bg.to_simple_opaque()),
            None => rich_rs::Style::new(),
        }
    }
}

impl Renderable for Blank {
    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.max_width.max(options.size.0).max(1);
        let height = options.size.1.max(1);
        let style = self.style();

        let mut out = Segments::new();
        for row in 0..height {
            out.push(Segment::styled(" ".repeat(width), style));
            if row + 1 < height {
                out.push(Segment::line());
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_renders_requested_size() {
        let blank = Blank::new(Color::rgb(1, 2, 3));
        let console = Console::new();
        let options = ConsoleOptions {
            size: (5, 3),
            max_width: 5,
            ..Default::default()
        };
        let rendered = blank.render(&console, &options);
        let lines = Segment::split_lines(rendered);
        assert_eq!(lines.len(), 3);
        assert!(lines.iter().all(|line| Segment::get_line_length(line) == 5));
    }
}
