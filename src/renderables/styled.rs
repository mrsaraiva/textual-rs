use rich_rs::{Console, ConsoleOptions, Renderable, Segments};

/// Renderable style sandwich: `pre_style + renderable_style + post_style`.
///
/// Rust counterpart to Python Textual `renderables/styled.py`.
#[derive(Debug, Clone)]
pub struct Styled<R> {
    renderable: R,
    pre_style: rich_rs::Style,
    post_style: rich_rs::Style,
}

impl<R> Styled<R> {
    pub fn new(renderable: R, pre_style: rich_rs::Style, post_style: rich_rs::Style) -> Self {
        Self {
            renderable,
            pre_style,
            post_style,
        }
    }

    pub fn into_inner(self) -> R {
        self.renderable
    }

    fn apply_styles(
        segments: Segments,
        pre_style: rich_rs::Style,
        post_style: rich_rs::Style,
    ) -> Segments {
        segments
            .into_iter()
            .map(|mut seg| {
                if seg.control.is_some() {
                    return seg;
                }
                let styled = pre_style
                    .combine(&seg.style.unwrap_or_else(rich_rs::Style::new))
                    .combine(&post_style);
                seg.style = Some(styled);
                seg
            })
            .collect()
    }
}

impl<R: Renderable> Renderable for Styled<R> {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let rendered = self.renderable.render(console, options);
        Self::apply_styles(rendered, self.pre_style, self.post_style)
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
            vec![Segment::styled("x", rich_rs::Style::new().with_bold(true))].into()
        }
    }

    #[test]
    fn styled_applies_pre_and_post() {
        let pre = rich_rs::Style::new().with_dim(true);
        let post = rich_rs::Style::new().with_underline(true);
        let styled = Styled::new(Sample, pre, post);
        let console = Console::new();
        let options = ConsoleOptions {
            size: (1, 1),
            max_width: 1,
            ..Default::default()
        };
        let rendered = styled.render(&console, &options);
        let style = rendered
            .iter()
            .next()
            .and_then(|seg| seg.style)
            .expect("style");
        assert_eq!(style.bold, Some(true));
        assert_eq!(style.dim, Some(true));
        assert_eq!(style.underline, Some(true));
    }
}
