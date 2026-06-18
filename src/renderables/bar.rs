use std::collections::BTreeMap;

use rich_rs::{Console, ConsoleOptions, MetaValue, Renderable, Segment, Segments};

use crate::style::Color;

/// Thin horizontal bar with highlighted range.
///
/// Rust counterpart to Python Textual's `renderables/bar.py`.
/// Supports half-cell boundary glyphs, optional gradient coloring across the
/// highlighted span, and optional clickable ranges.
#[derive(Debug, Clone)]
pub struct Bar {
    highlight_range: (f32, f32),
    highlight_style: rich_rs::Style,
    background_style: rich_rs::Style,
    highlight_char: char,
    background_char: char,
    half_left_char: char,
    half_right_char: char,
    clickable_ranges: BTreeMap<String, (usize, usize)>,
    width: Option<usize>,
    gradient: Option<(Color, Color)>,
}

impl Bar {
    pub const HALF_BAR_LEFT: char = '╺';
    pub const BAR: char = '━';
    pub const HALF_BAR_RIGHT: char = '╸';

    pub fn new(
        highlight_range: (f32, f32),
        highlight_style: rich_rs::Style,
        background_style: rich_rs::Style,
    ) -> Self {
        Self {
            highlight_range,
            highlight_style,
            background_style,
            highlight_char: Self::BAR,
            background_char: Self::BAR,
            half_left_char: Self::HALF_BAR_LEFT,
            half_right_char: Self::HALF_BAR_RIGHT,
            clickable_ranges: BTreeMap::new(),
            width: None,
            gradient: None,
        }
    }

    pub fn width(mut self, width: usize) -> Self {
        self.width = Some(width.max(1));
        self
    }

    pub fn gradient(mut self, start: Color, end: Color) -> Self {
        self.gradient = Some((start, end));
        self
    }

    pub fn clickable_range(mut self, name: impl Into<String>, range: (usize, usize)) -> Self {
        self.clickable_ranges.insert(name.into(), range);
        self
    }

    /// Configure the full-cell glyphs used for highlighted and background
    /// portions.
    pub fn chars(mut self, highlight: char, background: char) -> Self {
        self.highlight_char = highlight;
        self.background_char = background;
        self
    }

    /// Configure half-cell edge glyphs used at transition boundaries.
    pub fn half_chars(mut self, left: char, right: char) -> Self {
        self.half_left_char = left;
        self.half_right_char = right;
        self
    }

    /// Render this bar to a fixed width without requiring console options.
    ///
    /// Useful for widget internals that already computed target width and want
    /// to compose segments directly.
    pub fn render_for_width(&self, width: usize) -> Segments {
        self.render_segments_for_width(width)
    }

    fn effective_width(&self, options: &ConsoleOptions) -> usize {
        self.width
            .unwrap_or(options.max_width.max(options.size.0).max(1))
            .max(1)
    }

    fn render_segments_for_width(&self, width: usize) -> Segments {
        if width == 0 {
            return Segments::new();
        }

        let (mut start, mut end) = self.highlight_range;
        start = start.max(0.0);
        end = end.min(width as f32);

        let mut segments: Vec<Segment> = Vec::new();
        if (start == 0.0 && end == 0.0) || end < 0.0 || start > end {
            segments.push(Segment::styled(
                self.background_char.to_string().repeat(width),
                self.background_style,
            ));
            return segments.into();
        }

        start = (start * 2.0).round() / 2.0;
        end = (end * 2.0).round() / 2.0;

        let half_start = (start - start.trunc()).abs() > f32::EPSILON;
        let half_end = (end - end.trunc()).abs() > f32::EPSILON;

        let initial_len = (start - 0.5) as i32;
        if initial_len > 0 {
            segments.push(Segment::styled(
                self.background_char
                    .to_string()
                    .repeat(initial_len as usize),
                self.background_style,
            ));
        }

        if !half_start && start > 0.0 {
            segments.push(Segment::styled(
                self.half_right_char.to_string(),
                self.background_style,
            ));
        }

        let bar_width = (end as i32) - (start as i32);
        if half_start {
            let mut highlight = String::from(self.half_left_char);
            if bar_width > 1 {
                highlight.push_str(
                    &self
                        .highlight_char
                        .to_string()
                        .repeat((bar_width - 1) as usize),
                );
            }
            segments.push(Segment::styled(highlight, self.highlight_style));
        } else if bar_width > 0 {
            segments.push(Segment::styled(
                self.highlight_char.to_string().repeat(bar_width as usize),
                self.highlight_style,
            ));
        }

        if half_end {
            segments.push(Segment::styled(
                self.half_right_char.to_string(),
                self.highlight_style,
            ));
        }

        if !half_end && (end - width as f32).abs() > f32::EPSILON {
            segments.push(Segment::styled(
                self.half_left_char.to_string(),
                self.background_style,
            ));
        }

        let tail_len = (width as i32) - (end as i32) - 1;
        if tail_len > 0 {
            segments.push(Segment::styled(
                self.background_char.to_string().repeat(tail_len as usize),
                self.background_style,
            ));
        }

        let cell_segments = self.segmentize_for_gradient_and_meta(segments, width);
        Segment::simplify(cell_segments)
    }

    fn segmentize_for_gradient_and_meta(
        &self,
        segments: Vec<Segment>,
        width: usize,
    ) -> Vec<Segment> {
        let mut cells: Vec<Segment> = Vec::with_capacity(width);
        let mut x = 0usize;

        for seg in segments {
            if seg.control.is_some() {
                cells.push(seg);
                continue;
            }
            let style = seg.style.unwrap_or_else(rich_rs::Style::new);
            for ch in seg.text.chars() {
                let mut style = style;
                if let Some((start, end)) = self.gradient {
                    if style == self.highlight_style {
                        let t = if width <= 1 {
                            0.0
                        } else {
                            x as f32 / (width - 1) as f32
                        };
                        let c = lerp_color(start, end, t);
                        style = style.with_color(c.to_simple_opaque());
                    }
                }

                let maybe_meta = self.click_meta_for_x(x);
                if let Some(meta) = maybe_meta {
                    cells.push(Segment::styled_with_meta(ch.to_string(), style, meta));
                } else {
                    cells.push(Segment::styled(ch.to_string(), style));
                }
                x += 1;
            }
        }
        cells
    }

    fn click_meta_for_x(&self, x: usize) -> Option<rich_rs::StyleMeta> {
        self.clickable_ranges
            .iter()
            .find_map(|(name, (start, end))| {
                if x >= *start && x < *end {
                    let handler = format!("range_clicked('{name}')");
                    Some(rich_rs::Style::on(
                        None,
                        [("click", MetaValue::str(handler))],
                    ))
                } else {
                    None
                }
            })
    }
}

impl Renderable for Bar {
    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = self.effective_width(options);
        self.render_segments_for_width(width)
    }
}

fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let inv = 1.0 - t;
    Color::rgba_f(
        (a.r as f32 * inv + b.r as f32 * t).round() as u8,
        (a.g as f32 * inv + b.g as f32 * t).round() as u8,
        (a.b as f32 * inv + b.b as f32 * t).round() as u8,
        a.a * inv + b.a * t,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bar_renders_background_when_no_highlight() {
        let bar = Bar::new((0.0, 0.0), rich_rs::Style::new(), rich_rs::Style::new()).width(6);
        let console = Console::new();
        let options = ConsoleOptions {
            size: (6, 1),
            max_width: 6,
            ..Default::default()
        };
        let rendered = bar.render(&console, &options);
        let text: String = rendered.iter().map(|s| s.text.as_ref()).collect();
        assert_eq!(rich_rs::cell_len(&text), 6);
    }

    #[test]
    fn bar_half_step_boundaries_render_half_glyphs() {
        let bar = Bar::new((1.5, 3.5), rich_rs::Style::new(), rich_rs::Style::new()).width(8);
        let console = Console::new();
        let options = ConsoleOptions {
            size: (8, 1),
            max_width: 8,
            ..Default::default()
        };
        let rendered = bar.render(&console, &options);
        let text: String = rendered.iter().map(|s| s.text.as_ref()).collect();
        assert!(text.contains(Bar::HALF_BAR_LEFT));
        assert!(text.contains(Bar::HALF_BAR_RIGHT));
    }

    #[test]
    fn bar_clickable_range_applies_click_meta() {
        let bar = Bar::new((0.0, 4.0), rich_rs::Style::new(), rich_rs::Style::new())
            .width(8)
            .clickable_range("alpha", (1, 3));
        let console = Console::new();
        let options = ConsoleOptions {
            size: (8, 1),
            max_width: 8,
            ..Default::default()
        };
        let rendered = bar.render(&console, &options);
        let has_click = rendered.iter().any(|seg| {
            seg.meta
                .as_ref()
                .and_then(|meta| meta.meta.as_ref())
                .and_then(|meta| meta.get("@click"))
                == Some(&MetaValue::str("range_clicked('alpha')"))
        });
        assert!(has_click);
    }
}
