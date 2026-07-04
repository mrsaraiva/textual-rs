use rich_rs::{Console, ConsoleOptions, Segment, Segments};
use textual_macros::widget;

use super::{Layout, NodeSeed, Render};

// Widget trait split — pilot migration (Phase 1). The `#[widget(Layout)]`
// attribute generates `impl Widget for Spacer` (forwarding render → `Render`,
// layout methods → `Layout`, seed plumbing autowired from the `seed` field) and
// `impl Renderable`. The authoring surface is now the 3 methods below across two
// small named traits — down from a 63-method `impl Widget` stare-down.
#[widget(Layout)]
pub struct Spacer {
    height: usize,
    width_hint: Option<usize>,
    seed: NodeSeed,
}

impl Spacer {
    crate::seed_ident_methods!();

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

impl Render for Spacer {
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
}

impl Layout for Spacer {
    fn layout_height(&self) -> Option<usize> {
        Some(self.height)
    }

    fn content_width(&self) -> Option<usize> {
        Some(self.width_hint.unwrap_or(1).max(1))
    }
}
