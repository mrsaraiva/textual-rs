use std::f32::consts::PI;

use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::style::{Color, blend_colors};

/// Vertical background gradient renderable.
///
/// Rust counterpart to Python Textual `VerticalGradient`.
#[derive(Debug, Clone)]
pub struct VerticalGradient {
    from: Color,
    to: Color,
}

impl VerticalGradient {
    pub fn new(from: Color, to: Color) -> Self {
        Self { from, to }
    }
}

impl Renderable for VerticalGradient {
    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.max_width.max(options.size.0).max(1);
        let height = options.size.1.max(1);
        let mut out = Segments::new();

        for y in 0..height {
            let pct = if height <= 1 {
                0
            } else {
                ((y as f32 / (height - 1) as f32) * 100.0).round() as u8
            };
            let bg = blend_colors(self.from, self.to, pct);
            let style = rich_rs::Style::new().with_bgcolor(bg.to_simple_opaque());
            out.push(Segment::styled(" ".repeat(width), style));
            if y + 1 < height {
                out.push(Segment::line());
            }
        }
        out
    }
}

/// Linear gradient renderable with arbitrary stop list.
///
/// Rust counterpart to Python Textual `LinearGradient`.
#[derive(Debug, Clone)]
pub struct LinearGradient {
    angle_deg: f32,
    stops: Vec<(f32, Color)>,
}

impl LinearGradient {
    pub fn new(angle_deg: f32, mut stops: Vec<(f32, Color)>) -> Self {
        if stops.is_empty() {
            stops.push((0.0, Color::rgb(0, 0, 0)));
            stops.push((1.0, Color::rgb(255, 255, 255)));
        }
        stops.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        Self { angle_deg, stops }
    }

    fn sample_color(&self, t: f32) -> Color {
        let t = t.clamp(0.0, 1.0);
        if t <= self.stops[0].0 {
            return self.stops[0].1;
        }
        if let Some((_, color)) = self.stops.last()
            && t >= self.stops[self.stops.len() - 1].0
        {
            return *color;
        }
        for pair in self.stops.windows(2) {
            let (s0, c0) = pair[0];
            let (s1, c1) = pair[1];
            if t >= s0 && t <= s1 {
                let local = if (s1 - s0).abs() < f32::EPSILON {
                    0.0
                } else {
                    (t - s0) / (s1 - s0)
                };
                let pct = (local * 100.0).round() as u8;
                return blend_colors(c0, c1, pct);
            }
        }
        self.stops
            .last()
            .map(|(_, c)| *c)
            .unwrap_or(Color::rgb(0, 0, 0))
    }
}

impl Renderable for LinearGradient {
    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.max_width.max(options.size.0).max(1);
        let height = options.size.1.max(1);
        let mut out = Segments::new();

        let angle = -self.angle_deg * PI / 180.0;
        let cos_a = angle.cos();
        let sin_a = angle.sin();

        for y in 0..height {
            let mut row = Segments::new();
            for x in 0..width {
                let nx = if width <= 1 {
                    0.0
                } else {
                    x as f32 / (width - 1) as f32
                };
                let ny = if height <= 1 {
                    0.0
                } else {
                    y as f32 / (height - 1) as f32
                };
                let cx = nx - 0.5;
                let cy = ny - 0.5;
                let t = (cx * cos_a + cy * sin_a + 0.5).clamp(0.0, 1.0);
                let bg = self.sample_color(t);
                let style = rich_rs::Style::new().with_bgcolor(bg.to_simple_opaque());
                row.push(Segment::styled(" ", style));
            }
            out.extend(Segment::simplify(row));
            if y + 1 < height {
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
    fn vertical_gradient_matches_dimensions() {
        let gradient = VerticalGradient::new(Color::rgb(0, 0, 0), Color::rgb(255, 255, 255));
        let console = Console::new();
        let options = ConsoleOptions {
            size: (4, 3),
            max_width: 4,
            ..Default::default()
        };
        let rendered = gradient.render(&console, &options);
        let lines = Segment::split_lines(rendered);
        assert_eq!(lines.len(), 3);
        assert!(lines.iter().all(|line| Segment::get_line_length(line) == 4));
    }

    #[test]
    fn linear_gradient_matches_dimensions() {
        let gradient = LinearGradient::new(
            0.0,
            vec![(0.0, Color::rgb(0, 0, 0)), (1.0, Color::rgb(255, 255, 255))],
        );
        let console = Console::new();
        let options = ConsoleOptions {
            size: (5, 2),
            max_width: 5,
            ..Default::default()
        };
        let rendered = gradient.render(&console, &options);
        let lines = Segment::split_lines(rendered);
        assert_eq!(lines.len(), 2);
        assert!(lines.iter().all(|line| Segment::get_line_length(line) == 5));
    }
}
