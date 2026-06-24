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

/// Number of precomputed steps in the gradient color ramp.
///
/// Mirrors Python Textual `Gradient(quality=50)` so the quantized color lookup
/// matches the reference implementation.
const GRADIENT_QUALITY: usize = 50;

/// Linear gradient renderable with arbitrary stop list.
///
/// Rust counterpart to Python Textual `LinearGradient`. Renders the gradient
/// with upper-half-block glyphs (`▀`) so each terminal cell encodes two vertical
/// samples (foreground = top sample, background = bottom sample), doubling the
/// effective vertical resolution exactly like the Python reference.
#[derive(Debug, Clone)]
pub struct LinearGradient {
    angle_deg: f32,
    /// Precomputed quantized color ramp (`GRADIENT_QUALITY` entries).
    ramp: Vec<Color>,
}

impl LinearGradient {
    pub fn new(angle_deg: f32, mut stops: Vec<(f32, Color)>) -> Self {
        if stops.is_empty() {
            stops.push((0.0, Color::rgb(0, 0, 0)));
            stops.push((1.0, Color::rgb(255, 255, 255)));
        }
        stops.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let ramp = build_ramp(&stops);
        Self { angle_deg, ramp }
    }

    /// Sample the quantized gradient ramp at `position` in `[0, 1]`.
    ///
    /// Matches Python `Gradient.get_color`: index into the precomputed
    /// `quality`-step ramp and blend between the two nearest entries.
    pub fn get_color(&self, position: f32) -> Color {
        let position = position.clamp(0.0, 1.0);
        if position <= 0.0 {
            return self.ramp[0];
        }
        if position >= 1.0 {
            return self.ramp[self.ramp.len() - 1];
        }
        let color_position = position * (GRADIENT_QUALITY - 1) as f32;
        let color_index = color_position.floor() as usize;
        let c1 = self.ramp[color_index];
        let c2 = self.ramp[(color_index + 1).min(self.ramp.len() - 1)];
        let frac = color_position - color_index as f32;
        let pct = (frac * 100.0).round() as u8;
        blend_colors(c1, c2, pct)
    }
}

/// Precompute the quality-step color ramp from the (sorted, non-empty) stops.
///
/// Mirrors Python `Gradient.colors`: walk `quality` evenly spaced steps and, for
/// each, blend between the surrounding stop pair.
fn build_ramp(stops: &[(f32, Color)]) -> Vec<Color> {
    let mut colors = Vec::with_capacity(GRADIENT_QUALITY);
    let mut position = 0usize;
    for step_position in 0..GRADIENT_QUALITY {
        let step = step_position as f32 / (GRADIENT_QUALITY - 1) as f32;
        // Advance to the stop pair that brackets `step`.
        while position + 1 < stops.len() && step > stops[position + 1].0 {
            position += 1;
        }
        let (stop1, color1) = stops[position];
        let (stop2, color2) = if position + 1 < stops.len() {
            stops[position + 1]
        } else {
            stops[position]
        };
        let span = stop2 - stop1;
        let local = if span.abs() < f32::EPSILON {
            0.0
        } else {
            (step - stop1) / span
        };
        let pct = (local.clamp(0.0, 1.0) * 100.0).round() as u8;
        colors.push(blend_colors(color1, color2, pct));
    }
    colors
}

impl Renderable for LinearGradient {
    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.max_width.max(options.size.0).max(1);
        let height = options.size.1.max(1);
        let mut out = Segments::new();

        let angle_radians = -self.angle_deg * PI / 180.0;
        let sin_angle = angle_radians.sin();
        let cos_angle = angle_radians.cos();

        let width_f = width as f32;
        let center_x = width_f / 2.0;
        let center_y = height as f32;

        for line_y in 0..height {
            let point_y = line_y as f32 * 2.0 - center_y;
            let point_x = 0.0 - center_x;

            let x1 = (center_x + (point_x * cos_angle - point_y * sin_angle)) / width_f;
            let x2 =
                (center_x + (point_x * cos_angle - (point_y + 1.0) * sin_angle)) / width_f;
            let point_x_end = width_f - center_x;
            let end_x1 =
                (center_x + (point_x_end * cos_angle - point_y * sin_angle)) / width_f;
            let delta_x = (end_x1 - x1) / width_f;

            if delta_x.abs() < 0.0001 {
                // Vertical special case: a single uniform run across the row.
                let top = self.get_color(x1);
                let bottom = self.get_color(x2);
                let style = rich_rs::Style::new()
                    .with_color(top.to_simple_opaque())
                    .with_bgcolor(bottom.to_simple_opaque());
                out.push(Segment::styled("\u{2580}".repeat(width), style));
            } else {
                let mut row = Segments::new();
                for x in 0..width {
                    let xf = x as f32;
                    let top = self.get_color(x1 + xf * delta_x);
                    let bottom = self.get_color(x2 + xf * delta_x);
                    let style = rich_rs::Style::new()
                        .with_color(top.to_simple_opaque())
                        .with_bgcolor(bottom.to_simple_opaque());
                    row.push(Segment::styled("\u{2580}", style));
                }
                out.extend(Segment::simplify(row));
            }
            if line_y + 1 < height {
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

    #[test]
    fn linear_gradient_uses_half_block_glyphs() {
        // Parity with Python `LinearGradient`: every painted cell is an upper
        // half-block glyph (`▀`) carrying fg (top sample) + bg (bottom sample),
        // not a blank space. A plain-text capture must therefore show glyphs.
        let gradient = LinearGradient::new(
            90.0,
            vec![(0.0, Color::rgb(0, 0, 0)), (1.0, Color::rgb(255, 255, 255))],
        );
        let console = Console::new();
        let options = ConsoleOptions {
            size: (6, 3),
            max_width: 6,
            ..Default::default()
        };
        let rendered = gradient.render(&console, &options);
        let text: String = rendered
            .iter()
            .filter(|s| s.control.is_none())
            .map(|s| s.text.as_ref())
            .collect();
        assert!(
            text.contains('\u{2580}'),
            "gradient should render half-block glyphs, got: {text:?}"
        );
        assert_eq!(
            text.matches('\u{2580}').count(),
            6 * 3,
            "every cell across all rows should be a half-block glyph"
        );
        // Each painted glyph segment carries both a foreground and background
        // color (skip structural newline segments).
        assert!(
            rendered
                .iter()
                .filter(|s| s.control.is_none() && s.text.contains('\u{2580}'))
                .all(|s| s
                    .style
                    .as_ref()
                    .is_some_and(|st| st.color.is_some() && st.bgcolor.is_some())),
            "each gradient glyph must carry fg (top) + bg (bottom) colors"
        );
    }
}
