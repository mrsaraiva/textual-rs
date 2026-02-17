use rich_rs::{Console, ConsoleOptions, Renderable, Segments};

use crate::style::{Color, color_from_simple};

/// Renderable that applies a tint color over foreground/background colors.
///
/// Rust counterpart to Python Textual `renderables/tint.py`.
#[derive(Debug, Clone)]
pub struct Tint<R> {
    renderable: R,
    color: Color,
}

impl<R> Tint<R> {
    pub fn new(renderable: R, color: Color) -> Self {
        Self { renderable, color }
    }

    pub fn process_segments(segments: Segments, color: Color) -> Segments {
        let percent = Self::percent_from_alpha(color);
        if percent == 0 {
            return segments;
        }
        let tint_color = Color::rgba(color.r, color.g, color.b, 255);

        segments
            .into_iter()
            .map(|mut seg| {
                if seg.control.is_some() {
                    return seg;
                }
                let mut style = seg.style.unwrap_or_else(rich_rs::Style::new);
                if let Some(bg) = style.bgcolor {
                    let blended =
                        Self::blend_color_with_percent(color_from_simple(bg), tint_color, percent);
                    style.bgcolor = Some(blended.to_simple_opaque());
                }
                if let Some(fg) = style.color {
                    let blended =
                        Self::blend_color_with_percent(color_from_simple(fg), tint_color, percent);
                    style.color = Some(blended.to_simple_opaque());
                }
                seg.style = Some(style);
                seg
            })
            .collect()
    }

    pub fn blend_color_with_percent(base: Color, tint: Color, percent: u8) -> Color {
        crate::style::blend_colors(base, tint, percent)
    }

    pub fn percent_from_alpha(color: Color) -> u8 {
        ((color.a as f32 / 255.0) * 100.0).round() as u8
    }
}

impl<R: Renderable> Renderable for Tint<R> {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let rendered = self.renderable.render(console, options);
        Self::process_segments(rendered, self.color)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rich_rs::Segment;

    #[derive(Debug, Clone)]
    struct Sample;

    impl Renderable for Sample {
        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
            let style = rich_rs::Style::new()
                .with_bgcolor(rich_rs::SimpleColor::Rgb { r: 0, g: 0, b: 0 })
                .with_color(rich_rs::SimpleColor::Rgb {
                    r: 255,
                    g: 255,
                    b: 255,
                });
            vec![Segment::styled("X", style)].into()
        }
    }

    #[test]
    fn tint_changes_foreground() {
        let tint = Color::rgba(255, 0, 0, 128);
        let wrapped = Tint::new(Sample, tint);
        let console = Console::new();
        let options = ConsoleOptions {
            size: (1, 1),
            max_width: 1,
            ..Default::default()
        };
        let rendered = wrapped.render(&console, &options);
        let fg = rendered
            .iter()
            .next()
            .and_then(|seg| seg.style)
            .and_then(|s| s.color)
            .expect("fg");
        let fg = color_from_simple(fg);
        assert!(fg.r >= fg.g);
        assert!(fg.r >= fg.b);
    }
}
