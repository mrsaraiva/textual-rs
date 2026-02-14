//! P2 layout CSS gate tests.
//!
//! Tests for layout-affecting CSS properties wired in Phase 2:
//! - P2G-24: position (relative|absolute)
//! - P2G-25: box-sizing (border-box|content-box)
//! - P2G-26: split (top|right|bottom|left)
//! - P2G-27: per-side spacing (effective_padding / effective_margin)
//! - P2G-33: row-span / column-span

use rich_rs::{Console, ConsoleOptions, Segments};
use textual::layout::{inspect_node_rects, resolve_layout, Region};
use textual::node_id::NodeId;
use textual::style::{
    BoxSizing, Color, Dock, Layout, Offset, Position, Scalar, Spacing, Split, Style,
};
use textual::widget_tree::WidgetTree;
use textual::widgets::Widget;

// ---------------------------------------------------------------------------
// Test widget
// ---------------------------------------------------------------------------

struct TestWidget {
    label: &'static str,
    inline_style: Option<Style>,
}

impl TestWidget {
    fn new(label: &'static str) -> Self {
        Self {
            label,
            inline_style: None,
        }
    }

    fn with_style(mut self, style: Style) -> Self {
        self.inline_style = Some(style);
        self
    }

    fn boxed(label: &'static str) -> Box<dyn Widget> {
        Box::new(Self::new(label))
    }

    fn boxed_with_style(label: &'static str, style: Style) -> Box<dyn Widget> {
        Box::new(Self::new(label).with_style(style))
    }
}

impl Widget for TestWidget {
    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        Segments::new()
    }

    fn style_type(&self) -> &'static str {
        self.label
    }

    fn style(&self) -> Option<Style> {
        self.inline_style.clone()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Assert layout rect matches expected (x0, y0, x1, y1).
fn assert_layout(tree: &WidgetTree, node: NodeId, x0: u16, y0: u16, x1: u16, y1: u16) {
    let (layout, _content) = inspect_node_rects(tree, node).expect("node not found");
    assert_eq!(layout, (x0, y0, x1, y1), "layout_rect mismatch");
}


// =========================================================================
// P2G-24: position (relative|absolute)
// =========================================================================

#[test]
fn p2g24_absolute_removed_from_flow() {
    // An absolute child should not affect the layout of flow siblings.
    let mut tree = WidgetTree::new();
    let root = tree.set_root(TestWidget::boxed("Container"));
    let abs_child = tree.mount(
        root,
        TestWidget::boxed_with_style("Abs", {
            let mut s = Style::new().height(Scalar::Cells(10)).width(Scalar::Cells(20));
            s.position = Some(Position::Absolute);
            s
        }),
    );
    let flow_child = tree.mount(
        root,
        TestWidget::boxed_with_style("Flow", Style::new().height(Scalar::Cells(5))),
    );

    resolve_layout(&mut tree, root, Region::new(0, 0, 80, 50), (80, 50));

    // Flow child starts at y=0 (absolute child doesn't consume flow space).
    assert_layout(&tree, flow_child, 0, 0, 80, 5);
    // Absolute child is placed over the available region.
    assert_layout(&tree, abs_child, 0, 0, 20, 10);
}

#[test]
fn p2g24_absolute_with_offset() {
    // Absolute children can use offset for displacement.
    let mut tree = WidgetTree::new();
    let root = tree.set_root(TestWidget::boxed("Container"));
    let abs_child = tree.mount(
        root,
        TestWidget::boxed_with_style("Abs", {
            let mut s = Style::new().height(Scalar::Cells(10)).width(Scalar::Cells(20));
            s.position = Some(Position::Absolute);
            s.offset = Some(Offset { x: 5, y: 3 });
            s
        }),
    );

    resolve_layout(&mut tree, root, Region::new(0, 0, 80, 50), (80, 50));

    // Should be displaced by offset.
    assert_layout(&tree, abs_child, 5, 3, 25, 13);
}

#[test]
fn p2g24_absolute_does_not_reduce_dock_space() {
    // Absolute children should not reduce space for dock or flow children.
    let mut tree = WidgetTree::new();
    let root = tree.set_root(TestWidget::boxed("Container"));
    let abs_child = tree.mount(
        root,
        TestWidget::boxed_with_style("Abs", {
            let mut s = Style::new().height(Scalar::Cells(30));
            s.position = Some(Position::Absolute);
            s
        }),
    );
    let docked = tree.mount(
        root,
        TestWidget::boxed_with_style("Header", {
            let mut s = Style::new().height(Scalar::Cells(3));
            s.dock = Some(Dock::Top);
            s
        }),
    );
    let flow_child = tree.mount(root, TestWidget::boxed("Body"));

    resolve_layout(&mut tree, root, Region::new(0, 0, 80, 50), (80, 50));

    // Dock header at top.
    assert_layout(&tree, docked, 0, 0, 80, 3);
    // Flow body fills remaining (50-3=47).
    assert_layout(&tree, flow_child, 0, 3, 80, 50);
    // Absolute child uses full available, not reduced.
    assert_layout(&tree, abs_child, 0, 0, 80, 30);
}

// =========================================================================
// P2G-25: box-sizing (border-box|content-box)
// =========================================================================

#[test]
fn p2g25_content_box_adds_chrome() {
    // content-box (default): specified width/height is content only; chrome is added.
    let mut tree = WidgetTree::new();
    let root = tree.set_root(TestWidget::boxed("Container"));
    let child = tree.mount(
        root,
        TestWidget::boxed_with_style(
            "Child",
            Style::new()
                .height(Scalar::Cells(10))
                .width(Scalar::Cells(20))
                .padding(Spacing::new(1, 2, 1, 2))
                .border_top(Color::rgb(255, 255, 255))
                .border_bottom(Color::rgb(255, 255, 255))
                .border_left(Color::rgb(255, 255, 255))
                .border_right(Color::rgb(255, 255, 255)),
        ),
    );

    resolve_layout(&mut tree, root, Region::new(0, 0, 80, 50), (80, 50));

    // Layout size = content + padding + border.
    // Width: 20 + 2 + 2 + 1 + 1 = 26
    // Height: 10 + 1 + 1 + 1 + 1 = 14
    let (layout, content) = inspect_node_rects(&tree, child).unwrap();
    let layout_w = layout.2 - layout.0;
    let layout_h = layout.3 - layout.1;
    assert_eq!(layout_w, 26, "content-box: layout width should include chrome");
    assert_eq!(layout_h, 14, "content-box: layout height should include chrome");

    // Content size should match the specified 20x10.
    let content_w = content.2 - content.0;
    let content_h = content.3 - content.1;
    assert_eq!(content_w, 20, "content-box: content width");
    assert_eq!(content_h, 10, "content-box: content height");
}

#[test]
fn p2g25_border_box_includes_chrome() {
    // border-box: specified width/height already includes padding+border.
    let mut tree = WidgetTree::new();
    let root = tree.set_root(TestWidget::boxed("Container"));
    let child = tree.mount(
        root,
        TestWidget::boxed_with_style("Child", {
            let mut s = Style::new()
                .height(Scalar::Cells(14))
                .width(Scalar::Cells(26))
                .padding(Spacing::new(1, 2, 1, 2))
                .border_top(Color::rgb(255, 255, 255))
                .border_bottom(Color::rgb(255, 255, 255))
                .border_left(Color::rgb(255, 255, 255))
                .border_right(Color::rgb(255, 255, 255));
            s.box_sizing = Some(BoxSizing::BorderBox);
            s
        }),
    );

    resolve_layout(&mut tree, root, Region::new(0, 0, 80, 50), (80, 50));

    // Layout size should equal the specified border-box size.
    let (layout, content) = inspect_node_rects(&tree, child).unwrap();
    let layout_w = layout.2 - layout.0;
    let layout_h = layout.3 - layout.1;
    assert_eq!(layout_w, 26, "border-box: layout width == specified width");
    assert_eq!(layout_h, 14, "border-box: layout height == specified height");

    // Content = border-box - chrome. Chrome_w = 1+1+2+2 = 6, Chrome_h = 1+1+1+1 = 4
    let content_w = content.2 - content.0;
    let content_h = content.3 - content.1;
    assert_eq!(content_w, 20, "border-box: content width = 26 - 6");
    assert_eq!(content_h, 10, "border-box: content height = 14 - 4");
}

#[test]
fn p2g25_border_box_horizontal_layout() {
    // Verify border-box works in horizontal layout too.
    let mut tree = WidgetTree::new();
    let root = tree.set_root(TestWidget::boxed_with_style("Container", {
        let mut s = Style::new();
        s.layout = Some(Layout::Horizontal);
        s
    }));
    let a = tree.mount(
        root,
        TestWidget::boxed_with_style("A", {
            let mut s = Style::new()
                .width(Scalar::Cells(30))
                .padding(Spacing::new(0, 5, 0, 5));
            s.box_sizing = Some(BoxSizing::BorderBox);
            s
        }),
    );
    let b = tree.mount(root, TestWidget::boxed("B"));

    resolve_layout(&mut tree, root, Region::new(0, 0, 80, 50), (80, 50));

    // A's layout width should be 30 (border-box includes the 10 padding).
    let (a_layout, a_content) = inspect_node_rects(&tree, a).unwrap();
    let a_w = a_layout.2 - a_layout.0;
    assert_eq!(a_w, 30, "border-box in horizontal: layout width");
    // Content = 30 - 5 - 5 = 20
    let a_cw = a_content.2 - a_content.0;
    assert_eq!(a_cw, 20, "border-box in horizontal: content width");

    // B gets the remaining space.
    let (b_layout, _) = inspect_node_rects(&tree, b).unwrap();
    assert_eq!(b_layout.0, 30, "B starts after A");
}

// =========================================================================
// P2G-26: split (top|right|bottom|left)
// =========================================================================

#[test]
fn p2g26_split_top_carves_space() {
    // split: top should carve space from the top, like dock: top.
    let mut tree = WidgetTree::new();
    let root = tree.set_root(TestWidget::boxed("Container"));
    let split_child = tree.mount(
        root,
        TestWidget::boxed_with_style("SplitTop", {
            let mut s = Style::new().height(Scalar::Cells(5));
            s.split = Some(Split::Top);
            s
        }),
    );
    let flow_child = tree.mount(
        root,
        TestWidget::boxed_with_style("Body", Style::new().height(Scalar::Cells(10))),
    );

    resolve_layout(&mut tree, root, Region::new(0, 0, 80, 50), (80, 50));

    // Split child at top: 80x5 at (0,0).
    assert_layout(&tree, split_child, 0, 0, 80, 5);
    // Flow child starts after split.
    assert_layout(&tree, flow_child, 0, 5, 80, 15);
}

#[test]
fn p2g26_split_left_carves_space() {
    // split: left should carve space from the left.
    let mut tree = WidgetTree::new();
    let root = tree.set_root(TestWidget::boxed_with_style("Container", {
        let mut s = Style::new();
        s.layout = Some(Layout::Horizontal);
        s
    }));
    let split_child = tree.mount(
        root,
        TestWidget::boxed_with_style("SplitLeft", {
            let mut s = Style::new().width(Scalar::Cells(20));
            s.split = Some(Split::Left);
            s
        }),
    );
    let flow_child = tree.mount(root, TestWidget::boxed("Body"));

    resolve_layout(&mut tree, root, Region::new(0, 0, 80, 50), (80, 50));

    // Split child at left: 20x50 at (0,0).
    assert_layout(&tree, split_child, 0, 0, 20, 50);
    // Flow child fills remaining: x=20, width=60.
    let (flow_l, _) = inspect_node_rects(&tree, flow_child).unwrap();
    assert_eq!(flow_l.0, 20, "flow starts after split");
}

#[test]
fn p2g26_split_processed_before_dock() {
    // Split should be processed before dock, reducing available region for dock.
    let mut tree = WidgetTree::new();
    let root = tree.set_root(TestWidget::boxed("Container"));
    let split_child = tree.mount(
        root,
        TestWidget::boxed_with_style("SplitTop", {
            let mut s = Style::new().height(Scalar::Cells(5));
            s.split = Some(Split::Top);
            s
        }),
    );
    let dock_child = tree.mount(
        root,
        TestWidget::boxed_with_style("DockTop", {
            let mut s = Style::new().height(Scalar::Cells(3));
            s.dock = Some(Dock::Top);
            s
        }),
    );
    let flow_child = tree.mount(root, TestWidget::boxed("Body"));

    resolve_layout(&mut tree, root, Region::new(0, 0, 80, 50), (80, 50));

    // Split at top: y=0..5
    assert_layout(&tree, split_child, 0, 0, 80, 5);
    // Dock at top after split: y=5..8
    assert_layout(&tree, dock_child, 0, 5, 80, 8);
    // Flow fills remaining from y=8.
    let (flow_l, _) = inspect_node_rects(&tree, flow_child).unwrap();
    assert_eq!(flow_l.1, 8, "flow starts after split + dock");
}

// =========================================================================
// P2G-27: per-side spacing
// =========================================================================

#[test]
fn p2g27_effective_padding_per_side_overrides() {
    // Per-side padding overrides the shorthand.
    let mut s = Style::new().padding(Spacing::new(1, 2, 3, 4));
    s.padding_top = Some(10);
    s.padding_right = Some(20);

    let eff = s.effective_padding();
    assert_eq!(eff.top, 10, "per-side top overrides shorthand");
    assert_eq!(eff.right, 20, "per-side right overrides shorthand");
    assert_eq!(eff.bottom, 3, "shorthand bottom preserved");
    assert_eq!(eff.left, 4, "shorthand left preserved");
}

#[test]
fn p2g27_effective_margin_per_side_overrides() {
    // Per-side margin overrides the shorthand.
    let mut s = Style::new().margin(Spacing::new(5, 6, 7, 8));
    s.margin_bottom = Some(99);

    let eff = s.effective_margin();
    assert_eq!(eff.top, 5);
    assert_eq!(eff.right, 6);
    assert_eq!(eff.bottom, 99, "per-side bottom overrides shorthand");
    assert_eq!(eff.left, 8);
}

#[test]
fn p2g27_effective_padding_no_shorthand() {
    // Per-side padding with no shorthand → base is zero.
    let mut s = Style::new();
    s.padding_left = Some(3);

    let eff = s.effective_padding();
    assert_eq!(eff.top, 0);
    assert_eq!(eff.right, 0);
    assert_eq!(eff.bottom, 0);
    assert_eq!(eff.left, 3);
}

#[test]
fn p2g27_per_side_padding_in_layout() {
    // Per-side padding should affect the content rect in layout.
    let mut tree = WidgetTree::new();
    let root = tree.set_root(TestWidget::boxed("Container"));
    let child = tree.mount(
        root,
        TestWidget::boxed_with_style("Child", {
            let mut s = Style::new()
                .height(Scalar::Cells(10))
                .padding(Spacing::new(1, 1, 1, 1));
            s.padding_left = Some(5); // override left from 1 to 5
            s
        }),
    );

    resolve_layout(&mut tree, root, Region::new(0, 0, 80, 50), (80, 50));

    let (_layout, content) = inspect_node_rects(&tree, child).unwrap();
    // Content x should be 0 + 5 (left padding) = 5
    assert_eq!(content.0, 5, "content_x reflects per-side padding_left");
    // Content right edge: layout_w - padding_right(1) = width - 1 (from right side)
    // Content width should be 80 - 5 - 1 = 74
    let content_w = content.2 - content.0;
    assert_eq!(content_w, 74, "content_w reflects per-side override");
}

#[test]
fn p2g27_per_side_margin_in_layout() {
    // Per-side margin should affect the layout rect position.
    let mut tree = WidgetTree::new();
    let root = tree.set_root(TestWidget::boxed("Container"));
    let child = tree.mount(
        root,
        TestWidget::boxed_with_style("Child", {
            let mut s = Style::new()
                .height(Scalar::Cells(10))
                .margin(Spacing::new(2, 2, 2, 2));
            s.margin_left = Some(10); // override left from 2 to 10
            s
        }),
    );

    resolve_layout(&mut tree, root, Region::new(0, 0, 80, 50), (80, 50));

    let (layout, _content) = inspect_node_rects(&tree, child).unwrap();
    // Layout x should be 0 + 10 (left margin) = 10
    assert_eq!(layout.0, 10, "layout_x reflects per-side margin_left");
    // Layout width: 80 - 10 - 2 = 68
    let layout_w = layout.2 - layout.0;
    assert_eq!(layout_w, 68, "layout_w reflects per-side margin override");
}

// =========================================================================
// P2G-33: row-span / column-span
// =========================================================================

#[test]
fn p2g33_column_span_2_in_2col_grid() {
    // A child with column-span: 2 in a 2-column grid should span the full width.
    let mut tree = WidgetTree::new();
    let root = tree.set_root(TestWidget::boxed("Container"));
    let wide = tree.mount(
        root,
        TestWidget::boxed_with_style("Wide", {
            let mut s = Style::new();
            s.column_span = Some(2);
            s
        }),
    );
    let normal = tree.mount(root, TestWidget::boxed("Normal"));

    let parent_style = {
        let mut s = Style::new();
        s.layout = Some(Layout::Grid);
        s.grid_size_columns = Some(2);
        s
    };

    // Mount children under root but call layout_grid directly.
    textual::layout::layout_grid(
        &mut tree,
        &[wide, normal],
        Region::new(0, 0, 80, 60),
        (80, 60),
        &parent_style,
    );

    // Wide spans 2 columns (full width = 80), placed in row 0.
    let (wide_l, _) = inspect_node_rects(&tree, wide).unwrap();
    let wide_w = wide_l.2 - wide_l.0;
    assert_eq!(wide_w, 80, "column-span: 2 should span full width");

    // Normal is in row 1, col 0, width = 40.
    let (normal_l, _) = inspect_node_rects(&tree, normal).unwrap();
    let normal_w = normal_l.2 - normal_l.0;
    assert_eq!(normal_w, 40, "normal child gets one column");
    assert_eq!(normal_l.1, wide_l.3, "normal starts in next row after spanning child");
}

#[test]
fn p2g33_row_span_2_in_2row_grid() {
    // A child with row-span: 2 in a 2x2 grid should span 2 rows.
    let mut tree = WidgetTree::new();
    let root = tree.set_root(TestWidget::boxed("Container"));
    let tall = tree.mount(
        root,
        TestWidget::boxed_with_style("Tall", {
            let mut s = Style::new();
            s.row_span = Some(2);
            s
        }),
    );
    let b = tree.mount(root, TestWidget::boxed("B"));
    let c = tree.mount(root, TestWidget::boxed("C"));

    let parent_style = {
        let mut s = Style::new();
        s.layout = Some(Layout::Grid);
        s.grid_size_columns = Some(2);
        s
    };

    textual::layout::layout_grid(
        &mut tree,
        &[tall, b, c],
        Region::new(0, 0, 80, 60),
        (80, 60),
        &parent_style,
    );

    // Tall spans rows 0-1, col 0. Height should be 60 (full height for 2 rows).
    let (tall_l, _) = inspect_node_rects(&tree, tall).unwrap();
    let tall_h = tall_l.3 - tall_l.1;
    assert_eq!(tall_h, 60, "row-span: 2 should span full height");
    assert_eq!(tall_l.0, 0, "tall starts at col 0");

    // B should be in (col=1, row=0).
    let (b_l, _) = inspect_node_rects(&tree, b).unwrap();
    assert_eq!(b_l.0, 40, "B at col 1");
    assert_eq!(b_l.1, 0, "B at row 0");

    // C should be in (col=1, row=1) since col 0 is occupied by tall.
    let (c_l, _) = inspect_node_rects(&tree, c).unwrap();
    assert_eq!(c_l.0, 40, "C at col 1");
    assert_eq!(c_l.1, 30, "C at row 1");
}

#[test]
fn p2g33_span_with_gutter() {
    // Column-span should include gutter space between spanned columns.
    let mut tree = WidgetTree::new();
    let root = tree.set_root(TestWidget::boxed("Container"));
    let wide = tree.mount(
        root,
        TestWidget::boxed_with_style("Wide", {
            let mut s = Style::new();
            s.column_span = Some(2);
            s
        }),
    );
    let a = tree.mount(root, TestWidget::boxed("A"));
    let b = tree.mount(root, TestWidget::boxed("B"));

    let parent_style = {
        let mut s = Style::new();
        s.layout = Some(Layout::Grid);
        s.grid_size_columns = Some(3);
        s.grid_gutter_vertical = Some(2);
        s
    };

    // Available: 94 wide. 3 cols with 2px gutter between each.
    // Gutter budget: 2 * 2 = 4. Col budget: 90. Col widths: [30, 30, 30].
    // Wide spans cols 0+1: width = 30 + 2 (gutter) + 30 = 62.
    textual::layout::layout_grid(
        &mut tree,
        &[wide, a, b],
        Region::new(0, 0, 94, 30),
        (94, 30),
        &parent_style,
    );

    let (wide_l, _) = inspect_node_rects(&tree, wide).unwrap();
    let wide_w = wide_l.2 - wide_l.0;
    assert_eq!(wide_w, 62, "span width includes inter-column gutter");

    // A should be at col 2 (offset = 30 + 2 + 30 + 2 = 64).
    let (a_l, _) = inspect_node_rects(&tree, a).unwrap();
    assert_eq!(a_l.0, 64, "A starts at col 2 offset");
}

#[test]
fn p2g33_no_span_preserves_existing_behavior() {
    // Without spans, grid behavior should be identical to before.
    let mut tree = WidgetTree::new();
    let root = tree.set_root(TestWidget::boxed("Container"));
    let a = tree.mount(root, TestWidget::boxed("A"));
    let b = tree.mount(root, TestWidget::boxed("B"));
    let c = tree.mount(root, TestWidget::boxed("C"));
    let d = tree.mount(root, TestWidget::boxed("D"));

    let parent_style = {
        let mut s = Style::new();
        s.layout = Some(Layout::Grid);
        s.grid_size_columns = Some(2);
        s
    };

    textual::layout::layout_grid(
        &mut tree,
        &[a, b, c, d],
        Region::new(0, 0, 80, 50),
        (80, 50),
        &parent_style,
    );

    assert_layout(&tree, a, 0, 0, 40, 25);
    assert_layout(&tree, b, 40, 0, 80, 25);
    assert_layout(&tree, c, 0, 25, 40, 50);
    assert_layout(&tree, d, 40, 25, 80, 50);
}

// =========================================================================
// P2-33 behavioral: span exceeding grid is clamped
// =========================================================================

#[test]
fn p2_33_span_exceeding_grid_is_clamped() {
    // A child with column-span: 4 in a 2-column grid should be clamped to 2.
    let mut tree = WidgetTree::new();
    let root = tree.set_root(TestWidget::boxed("Container"));
    let wide = tree.mount(
        root,
        TestWidget::boxed_with_style("Wide", {
            let mut s = Style::new();
            s.column_span = Some(4); // exceeds grid columns
            s
        }),
    );
    let normal = tree.mount(root, TestWidget::boxed("Normal"));

    let parent_style = {
        let mut s = Style::new();
        s.layout = Some(Layout::Grid);
        s.grid_size_columns = Some(2);
        s
    };

    textual::layout::layout_grid(
        &mut tree,
        &[wide, normal],
        Region::new(0, 0, 80, 60),
        (80, 60),
        &parent_style,
    );

    // Wide should be clamped to 2 columns (full width = 80), not crash or overflow.
    let (wide_l, _) = inspect_node_rects(&tree, wide).unwrap();
    let wide_w = wide_l.2 - wide_l.0;
    assert_eq!(
        wide_w, 80,
        "column-span: 4 should be clamped to grid width (2 cols = 80)"
    );
}

// =========================================================================
// P2-33 behavioral: overlapping spans use occupancy grid
// =========================================================================

#[test]
fn p2_33_overlapping_spans_use_occupancy() {
    // Two children each with column-span: 2 in a 3-column grid.
    // The first takes cols 0-1 in row 0. The second can't fit in row 0
    // (only col 2 is free, needs 2), so it wraps to row 1.
    let mut tree = WidgetTree::new();
    let root = tree.set_root(TestWidget::boxed("Container"));
    let a = tree.mount(
        root,
        TestWidget::boxed_with_style("A", {
            let mut s = Style::new();
            s.column_span = Some(2);
            s
        }),
    );
    let b = tree.mount(
        root,
        TestWidget::boxed_with_style("B", {
            let mut s = Style::new();
            s.column_span = Some(2);
            s
        }),
    );

    let parent_style = {
        let mut s = Style::new();
        s.layout = Some(Layout::Grid);
        s.grid_size_columns = Some(3);
        s
    };

    textual::layout::layout_grid(
        &mut tree,
        &[a, b],
        Region::new(0, 0, 90, 60),
        (90, 60),
        &parent_style,
    );

    // A spans cols 0-1 in row 0: width = 60 (2 * 30).
    let (a_l, _) = inspect_node_rects(&tree, a).unwrap();
    let a_w = a_l.2 - a_l.0;
    assert_eq!(a_w, 60, "A should span 2 columns");
    assert_eq!(a_l.1, 0, "A should be in row 0");

    // B needs 2 columns but row 0 only has 1 free (col 2).
    // Occupancy grid should push B to row 1.
    let (b_l, _) = inspect_node_rects(&tree, b).unwrap();
    assert!(
        b_l.1 > a_l.1,
        "B should be in a later row than A (occupancy prevents overlap): B.y0={} A.y0={}",
        b_l.1,
        a_l.1
    );
    let b_w = b_l.2 - b_l.0;
    assert_eq!(b_w, 60, "B should also span 2 columns");
}
