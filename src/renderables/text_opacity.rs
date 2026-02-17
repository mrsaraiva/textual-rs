use rich_rs::{Console, ConsoleOptions, Renderable, Segments};

use crate::style::color_from_simple;

/// Renderable that blends text foreground into background by opacity.
///
/// Rust counterpart to Python Textual `renderables/text_opacity.py`.
#[derive(Debug, Clone)]
pub struct TextOpacity<R> {
    renderable: R,
    opacity: f32,
}

impl<R> TextOpacity<R> {
    pub fn new(renderable: R, opacity: f32) -> Self {
        Self {
            renderable,
            opacity: opacity.clamp(0.0, 1.0),
        }
    }

    pub fn from_percent(renderable: R, opacity_percent: u8) -> Self {
        Self::new(renderable, (opacity_percent as f32 / 100.0).clamp(0.0, 1.0))
    }

    pub fn process_segments(segments: Segments, opacity: f32) -> Segments {
        let opacity = opacity.clamp(0.0, 1.0);
        if (opacity - 1.0).abs() < f32::EPSILON {
            return segments;
        }

        segments
            .into_iter()
            .map(|mut seg| {
                if seg.control.is_some() {
                    return seg;
                }

                let style = seg.style.unwrap_or_else(rich_rs::Style::new);
                if opacity <= 0.0 {
                    let invisible_style = rich_rs::Style::from_color(None, style.bgcolor);
                    seg.text = " ".repeat(rich_rs::cell_len(seg.text.as_ref())).into();
                    seg.style = Some(invisible_style);
                    return seg;
                }

                let Some(fg_simple) = style.color else {
                    return seg;
                };
                let Some(bg_simple) = style.bgcolor else {
                    return seg;
                };

                let fg = color_from_simple(fg_simple);
                let bg = color_from_simple(bg_simple);
                let percent = (opacity * 100.0).round() as u8;
                let blended = crate::style::blend_colors(bg, fg, percent);
                seg.style = Some(style.with_color(blended.to_simple_opaque()));
                seg
            })
            .collect()
    }
}

impl<R: Renderable> Renderable for TextOpacity<R> {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let rendered = self.renderable.render(console, options);
        Self::process_segments(rendered, self.opacity)
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
    fn text_opacity_zero_hides_text() {
        let wrapped = TextOpacity::new(Sample, 0.0);
        let console = Console::new();
        let options = ConsoleOptions {
            size: (1, 1),
            max_width: 1,
            ..Default::default()
        };
        let rendered = wrapped.render(&console, &options);
        let first = rendered.iter().next().expect("segment");
        assert_eq!(first.text.as_ref(), " ");
    }

    #[test]
    fn text_opacity_blends_fg_to_bg() {
        let wrapped = TextOpacity::new(Sample, 0.5);
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
        assert!(fg.r >= 120 && fg.r <= 136);
        assert_eq!(fg.g, fg.r);
        assert_eq!(fg.b, fg.r);
    }
}
