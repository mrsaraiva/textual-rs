//! Regression: a bordered container with horizontally-overflowing children must
//! keep BOTH of its border columns intact and clip child content to its content
//! box (inside the border/padding).
//!
//! Motivating defect: `docs/examples/how-to/containers06` — a bordered
//! `Horizontal` holds 10 boxes whose combined width (160) exceeds the viewport
//! (120). Before the fix, the overflowing children painted their (background-
//! filled) cells over the container's right `┃` border column on every content
//! row. Python's compositor clips every container's children to
//! `region.shrink(gutter)` (`_compositor.py` `add_widget`:
//! `sub_clip = clip.intersection(child_region)`), so the border is always
//! preserved and overflow is clipped.
//!
//! The fix lives in `src/runtime/render.rs`: descendants are clipped to the
//! node's content box whenever the node reserves any gutter (border/padding),
//! not only when the widget opts in via `clips_descendants_to_content()`.

use rich_rs::{Console, ConsoleOptions, Segment, Segments};
use textual::prelude::*;
use textual::render::FrameBuffer;
use textual::style::{BorderEdge, BorderType, Color, Layout, Scalar, Style};
use textual::runtime::{build_widget_tree_from_root, render_tree_to_frame};

/// Leaf widget that paints a solid block of `glyph` across its whole box.
///
/// Its blank/filled cells are exactly what would overwrite a parent's border
/// column if descendant clipping were missing.
struct Block {
    seed: NodeSeed,
    glyph: char,
}

impl Block {
    fn new(width: u16, height: u16, glyph: char) -> Self {
        let mut seed = NodeSeed::default();
        seed.styles.style.width = Some(Scalar::Cells(width));
        seed.styles.style.height = Some(Scalar::Cells(height));
        Self { seed, glyph }
    }
}

impl Widget for Block {
    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let w = options.size.0.max(1);
        let h = options.size.1.max(1);
        let mut out = Segments::new();
        for y in 0..h {
            out.push(Segment::new(self.glyph.to_string().repeat(w)));
            if y + 1 < h {
                out.push(Segment::line());
            }
        }
        out
    }

    fn style_type(&self) -> &'static str {
        "Block"
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }
}

#[test]
fn bordered_horizontal_keeps_both_borders_when_children_overflow() {
    // Inner container: heavy border on all edges, horizontal layout, fixed 12x6.
    // Mounted as a child (mirroring `containers06`, where the bordered Horizontal
    // is a child of the app root), so layout reserves the border gutter and the
    // content box is cols 1..=10 (width 10), rows 1..=4 (height 4).
    let edge = BorderEdge::Edge {
        border_type: BorderType::Heavy,
        color: Color::parse("green").unwrap(),
    };
    let mut style = Style::new();
    style.border_top = edge;
    style.border_right = edge;
    style.border_bottom = edge;
    style.border_left = edge;
    style.layout = Some(Layout::Horizontal);
    style.width = Some(Scalar::Cells(12));
    style.height = Some(Scalar::Cells(6));

    let mut bordered = Container::new();
    bordered.seed_mut().styles.style = style;
    // Three 8-wide blocks = 24 cells, far wider than the 10-cell content box.
    bordered.push(Block::new(8, 4, 'a'));
    bordered.push(Block::new(8, 4, 'b'));
    bordered.push(Block::new(8, 4, 'c'));

    // Plain (gutterless) outer container as the tree root.
    let mut root = Container::new().with_child(bordered);

    let console = Console::new();
    let mut tree = build_widget_tree_from_root(&mut root).expect("tree builds");
    let frame: FrameBuffer = render_tree_to_frame(&mut tree, &mut root, &console, 12, 6);
    let lines = frame.as_plain_lines();

    let left = '┃';
    let right = '┃';

    // Top + bottom borders span the full width with heavy corners.
    assert!(
        lines[0].starts_with('┏') && lines[0].trim_end().ends_with('┓'),
        "top border row: {:?}",
        lines[0]
    );
    assert!(
        lines[5].starts_with('┗') && lines[5].trim_end().ends_with('┛'),
        "bottom border row: {:?}",
        lines[5]
    );

    // Every interior row (1..=4) keeps BOTH border columns, even though the
    // children overflow horizontally past the content box.
    for row in 1..=4usize {
        let chars: Vec<char> = lines[row].chars().collect();
        assert_eq!(
            chars.first().copied(),
            Some(left),
            "row {row} must keep its LEFT border column: {:?}",
            lines[row]
        );
        // Right border lives at column 11 (0-indexed) of the 12-wide box.
        assert_eq!(
            chars.get(11).copied(),
            Some(right),
            "row {row} must keep its RIGHT border column (child overflow must be \
             clipped to the content box): {:?}",
            lines[row]
        );
        // Child content must not bleed into the border columns.
        assert_ne!(
            chars.first().copied(),
            Some('a'),
            "left border overwritten by child content: {:?}",
            lines[row]
        );
        assert_ne!(
            chars.get(11).copied(),
            Some('c'),
            "right border overwritten by overflowing child content: {:?}",
            lines[row]
        );
    }
}
