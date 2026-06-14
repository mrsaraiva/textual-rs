//! Layout solver infrastructure.
//!
//! Ports Python Textual's layout pipeline:
//! - 1D space allocation ([`layout_resolve_1d`])
//! - Vertical stacking ([`layout_vertical`])
//! - Horizontal stacking ([`layout_horizontal`])
//! - Grid layout ([`layout_grid`])
//! - Dock positioning ([`arrange_dock`])
//! - Top-level dispatch ([`resolve_layout`])

use crate::node_id::NodeId;
#[cfg(test)]
use crate::style::Dock;
use crate::style::{Align, Display, HorizontalAlign, Layout, VerticalAlign};
#[cfg(test)]
use crate::widget_tree::Rect;
use crate::widget_tree::WidgetTree;

mod common;
mod dock;
mod grid;
mod horizontal;
mod region;
mod resolve_1d;
mod split;
mod vertical;

pub use dock::arrange_dock;
pub use grid::layout_grid;
pub use horizontal::layout_horizontal;
pub use region::Region;
pub use resolve_1d::{Edge, layout_resolve_1d};
pub use vertical::layout_vertical;

use common::get_node_style;
use dock::layout_dock_fill;
use split::{arrange_split, layout_absolute};

fn shift_rect_x(rect: crate::widget_tree::Rect, delta: i32) -> crate::widget_tree::Rect {
    let x0 = (rect.x0 as i32 + delta).max(0) as u16;
    let x1 = (rect.x1 as i32 + delta).max(0) as u16;
    crate::widget_tree::Rect {
        x0,
        y0: rect.y0,
        x1,
        y1: rect.y1,
    }
}

fn shift_rect_y(rect: crate::widget_tree::Rect, delta: i32) -> crate::widget_tree::Rect {
    let y0 = (rect.y0 as i32 + delta).max(0) as u16;
    let y1 = (rect.y1 as i32 + delta).max(0) as u16;
    crate::widget_tree::Rect {
        x0: rect.x0,
        y0,
        x1: rect.x1,
        y1,
    }
}

fn apply_parent_align(
    tree: &mut WidgetTree,
    children: &[NodeId],
    available: Region,
    strategy: Layout,
    align: Option<Align>,
) {
    let Some(align) = align else {
        return;
    };
    if children.is_empty() {
        return;
    }
    if strategy == Layout::Grid {
        return;
    }

    // Per-child margins, resolved once. Python's `WidgetPlacement.get_bounds`
    // grows each placement region by its margin before aligning, so the
    // alignment bounds include margins (otherwise a `margin` + `100%` child is
    // shifted by half its own margins — double-counting the gap it already
    // occupies). Mirror that here.
    let margins: Vec<crate::style::Spacing> = children
        .iter()
        .map(|&child| get_node_style(tree, child).effective_margin())
        .collect();

    // Python parity (`_arrange.py`): alignment translates the WHOLE arrangement
    // by a SINGLE offset, not each child independently. The offset is derived
    // from the bounding box of all (margin-grown) placements vs the parent
    // region (`Styles._align_size`). Computing one offset for both axes and
    // applying it to every child preserves children's relative positions — so a
    // narrow buttons row and a wide content box both shift by the same dx and
    // stay left-aligned with each other (block centering), instead of each being
    // independently centered on the cross axis.
    let mut min_x = u16::MAX;
    let mut min_y = u16::MAX;
    let mut max_x = 0u16;
    let mut max_y = 0u16;
    for (idx, &child) in children.iter().enumerate() {
        if let Some(node) = tree.get(child) {
            let margin = margins[idx];
            let layout = node.layout_rect;
            min_x = min_x.min(layout.x0.saturating_sub(margin.left));
            min_y = min_y.min(layout.y0.saturating_sub(margin.top));
            max_x = max_x.max(layout.x1.saturating_add(margin.right));
            max_y = max_y.max(layout.y1.saturating_add(margin.bottom));
        }
    }
    if min_x == u16::MAX {
        return;
    }
    let used_w = max_x.saturating_sub(min_x);
    let used_h = max_y.saturating_sub(min_y);

    let dx = match align.horizontal {
        HorizontalAlign::Left => 0i32,
        HorizontalAlign::Center => {
            (available.x as i32 + (available.width.saturating_sub(used_w) / 2) as i32)
                - min_x as i32
        }
        HorizontalAlign::Right => {
            (available.x as i32 + available.width.saturating_sub(used_w) as i32) - min_x as i32
        }
    };
    let dy = match align.vertical {
        VerticalAlign::Top => 0i32,
        VerticalAlign::Middle => {
            (available.y as i32 + (available.height.saturating_sub(used_h) / 2) as i32)
                - min_y as i32
        }
        VerticalAlign::Bottom => {
            (available.y as i32 + available.height.saturating_sub(used_h) as i32) - min_y as i32
        }
    };

    if dx != 0 || dy != 0 {
        for &child in children {
            if let Some(node) = tree.get_mut(child) {
                if dx != 0 {
                    node.layout_rect = shift_rect_x(node.layout_rect, dx);
                    node.content_rect = shift_rect_x(node.content_rect, dx);
                }
                if dy != 0 {
                    node.layout_rect = shift_rect_y(node.layout_rect, dy);
                    node.content_rect = shift_rect_y(node.content_rect, dy);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Top-level dispatch
// ---------------------------------------------------------------------------

/// Resolve layout for a subtree rooted at `node`.
///
/// 1. Resolves the node's own style for layout strategy.
/// 2. Separates children into docked vs flow.
/// 3. Calls [`arrange_dock`] for docked children → reduced available region.
/// 4. Dispatches flow children to [`layout_vertical`] / [`layout_horizontal`].
///    Grid dispatches to [`layout_grid`] with parent style for track sizing.
pub fn resolve_layout(
    tree: &mut WidgetTree,
    node: NodeId,
    available: Region,
    viewport: (u16, u16),
) {
    let style = get_node_style(tree, node);
    let strategy = style.layout.unwrap_or(Layout::Vertical);
    let is_dock_parent = tree
        .get(node)
        .map(|n| n.widget.style_type() == "Dock")
        .unwrap_or(false);
    let is_overlay_parent = tree
        .get(node)
        .map(|n| n.widget.style_type() == "Overlay")
        .unwrap_or(false);

    // Collect children (snapshot to avoid borrow conflict).
    let children: Vec<NodeId> = tree.children(node).to_vec();
    if children.is_empty() {
        return;
    }

    // Widgets may reserve internal chrome before child layout (for example
    // tab bars). Convert that chrome into an inset child-available region.
    let child_available = if let Some(node_ref) = tree.get(node) {
        let (ct, cr, cb, cl) = node_ref.widget.tree_child_content_inset();
        let x = available.x.saturating_add(cl);
        let y = available.y.saturating_add(ct);
        let horizontal = cl.saturating_add(cr);
        let vertical = ct.saturating_add(cb);
        let width = available.width.saturating_sub(horizontal);
        let height = available.height.saturating_sub(vertical);
        Region::new(x, y, width.max(1), height.max(1))
    } else {
        available
    };

    // Overlay children are layered in the same region (base + modal stack),
    // not arranged in normal flow.
    if is_overlay_parent {
        let mut layered = Vec::new();
        for &child in &children {
            if tree.get(child).map(|n| !n.display).unwrap_or(true) {
                continue;
            }
            let child_style = get_node_style(tree, child);
            if child_style.display == Some(Display::None) {
                continue;
            }
            layered.push(child);
        }
        if !layered.is_empty() {
            layout_absolute(tree, &layered, child_available, viewport);
        }
        for child in children {
            let Some(child_node) = tree.get(child) else {
                continue;
            };
            if !child_node.display || child_node.visibility != crate::style::Visibility::Visible {
                continue;
            }
            let rect = child_node.content_rect;
            let w = rect.x1.saturating_sub(rect.x0);
            let h = rect.y1.saturating_sub(rect.y0);
            if w == 0 || h == 0 {
                continue;
            }
            resolve_layout(tree, child, Region::new(rect.x0, rect.y0, w, h), viewport);
        }
        return;
    }

    // Separate children into categories: split, docked, absolute, and flow.
    let mut split = Vec::new();
    let mut docked = Vec::new();
    let mut absolute = Vec::new();
    let mut flow = Vec::new();
    for &child in &children {
        // Runtime/widget-driven hidden nodes should not participate in layout.
        if tree.get(child).map(|n| !n.display).unwrap_or(true) {
            continue;
        }
        let child_style = get_node_style(tree, child);
        if child_style.display == Some(Display::None) {
            continue;
        }
        if child_style.split.is_some() {
            split.push(child);
        } else if child_style.dock.is_some() {
            docked.push(child);
        } else if child_style.position == Some(crate::style::Position::Absolute) {
            absolute.push(child);
        } else {
            flow.push(child);
        }
    }

    // Arrange split children first → reduced available region.
    let after_split = if split.is_empty() {
        child_available
    } else {
        arrange_split(tree, &split, child_available, viewport)
    };

    // Arrange docked children → further reduced available region.
    let inner = if docked.is_empty() {
        after_split
    } else {
        arrange_dock(tree, &docked, after_split, viewport)
    };

    // Dispatch flow children to the appropriate layout.
    if !flow.is_empty() {
        if is_dock_parent && flow.len() == 1 {
            layout_dock_fill(tree, flow[0], inner);
        } else {
            // A horizontally-scrollable parent (overflow-x: auto/scroll) lets auto-width
            // children keep their intrinsic width (overflowing the viewport) so the content
            // can be scrolled horizontally instead of wrapping to the viewport width.
            let allow_h_overflow = matches!(
                style.overflow_x.or(style.overflow),
                Some(crate::style::Overflow::Auto) | Some(crate::style::Overflow::Scroll)
            );
            match strategy {
                Layout::Vertical => {
                    layout_vertical(tree, &flow, inner, viewport, allow_h_overflow);
                    apply_parent_align(tree, &flow, inner, Layout::Vertical, style.align);
                }
                Layout::Grid => {
                    layout_grid(tree, &flow, inner, viewport, &style);
                }
                Layout::Horizontal => {
                    layout_horizontal(tree, &flow, inner, viewport);
                    apply_parent_align(tree, &flow, inner, Layout::Horizontal, style.align);
                }
            }
        }
    }

    // Place absolute children on top of the original available region (P2-24).
    if !absolute.is_empty() {
        layout_absolute(tree, &absolute, child_available, viewport);
    }

    // Recurse into all laid-out descendants so every node receives
    // layout/content rectangles, not only one level under `node`.
    for child in children {
        let Some(child_node) = tree.get(child) else {
            continue;
        };
        if !child_node.display || child_node.visibility != crate::style::Visibility::Visible {
            continue;
        }
        let rect = child_node.content_rect;
        let w = rect.x1.saturating_sub(rect.x0);
        let h = rect.y1.saturating_sub(rect.y0);
        if w == 0 || h == 0 {
            continue;
        }
        resolve_layout(tree, child, Region::new(rect.x0, rect.y0, w, h), viewport);
    }
}

// ---------------------------------------------------------------------------
// Layout inspection (public API for testing)
// ---------------------------------------------------------------------------

/// Return the layout and content rects for a tree node.
///
/// Returns `Some((layout: (x0,y0,x1,y1), content: (x0,y0,x1,y1)))`,
/// or `None` if the node doesn't exist.
pub fn inspect_node_rects(
    tree: &WidgetTree,
    node: NodeId,
) -> Option<((u16, u16, u16, u16), (u16, u16, u16, u16))> {
    tree.get(node).map(|n| {
        (
            (
                n.layout_rect.x0,
                n.layout_rect.y0,
                n.layout_rect.x1,
                n.layout_rect.y1,
            ),
            (
                n.content_rect.x0,
                n.content_rect.y0,
                n.content_rect.x1,
                n.content_rect.y1,
            ),
        )
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::{Color, Display, Scalar, Spacing, Style};
    use crate::widget_tree::WidgetTree;
    use crate::widgets::Widget;
    use rich_rs::{Console, ConsoleOptions, Segments};

    // -- Test widget ----------------------------------------------------------

    struct LayoutTestWidget {
        label: &'static str,
        inline_style: Option<Style>,
        intrinsic_height: Option<usize>,
        intrinsic_width: Option<usize>,
    }

    impl LayoutTestWidget {
        fn new(label: &'static str) -> Self {
            Self {
                label,
                inline_style: None,
                intrinsic_height: None,
                intrinsic_width: None,
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

        fn with_intrinsic_height(mut self, height: usize) -> Self {
            self.intrinsic_height = Some(height.max(1));
            self
        }

        fn with_intrinsic_width(mut self, width: usize) -> Self {
            self.intrinsic_width = Some(width.max(1));
            self
        }

        fn boxed_with_style_and_intrinsic_height(
            label: &'static str,
            style: Style,
            height: usize,
        ) -> Box<dyn Widget> {
            Box::new(
                Self::new(label)
                    .with_style(style)
                    .with_intrinsic_height(height),
            )
        }

        fn boxed_with_style_and_intrinsic_width(
            label: &'static str,
            style: Style,
            width: usize,
        ) -> Box<dyn Widget> {
            Box::new(
                Self::new(label)
                    .with_style(style)
                    .with_intrinsic_width(width),
            )
        }
    }

    impl Widget for LayoutTestWidget {
        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
            Segments::new()
        }

        fn style_type(&self) -> &'static str {
            self.label
        }

        fn style(&self) -> Option<Style> {
            self.inline_style.clone()
        }

        fn layout_height(&self) -> Option<usize> {
            self.intrinsic_height
        }

        fn content_width(&self) -> Option<usize> {
            self.intrinsic_width
        }
    }

    // -- Helpers --------------------------------------------------------------

    fn assert_layout_rect(tree: &WidgetTree, node: NodeId, x0: u16, y0: u16, x1: u16, y1: u16) {
        let n = tree.get(node).unwrap();
        assert_eq!(
            n.layout_rect,
            Rect { x0, y0, x1, y1 },
            "layout_rect mismatch for node"
        );
    }

    fn assert_content_rect(tree: &WidgetTree, node: NodeId, x0: u16, y0: u16, x1: u16, y1: u16) {
        let n = tree.get(node).unwrap();
        assert_eq!(
            n.content_rect,
            Rect { x0, y0, x1, y1 },
            "content_rect mismatch for node"
        );
    }

    // =========================================================================
    // 1D resolver tests
    // =========================================================================

    #[test]
    fn resolve_1d_all_fixed() {
        let edges = vec![
            Edge {
                size: Some(10),
                fraction: 1,
                min_size: 0,
            },
            Edge {
                size: Some(20),
                fraction: 1,
                min_size: 0,
            },
            Edge {
                size: Some(30),
                fraction: 1,
                min_size: 0,
            },
        ];
        assert_eq!(layout_resolve_1d(100, &edges), vec![10, 20, 30]);
    }

    #[test]
    fn resolve_1d_all_flexible_equal() {
        let edges = vec![
            Edge {
                size: None,
                fraction: 1,
                min_size: 0,
            },
            Edge {
                size: None,
                fraction: 1,
                min_size: 0,
            },
            Edge {
                size: None,
                fraction: 1,
                min_size: 0,
            },
        ];
        // 90 / 3 = 30 each
        assert_eq!(layout_resolve_1d(90, &edges), vec![30, 30, 30]);
    }

    #[test]
    fn resolve_1d_all_flexible_weighted() {
        let edges = vec![
            Edge {
                size: None,
                fraction: 1,
                min_size: 0,
            },
            Edge {
                size: None,
                fraction: 2,
                min_size: 0,
            },
            Edge {
                size: None,
                fraction: 3,
                min_size: 0,
            },
        ];
        // total_fraction = 6, 60/6 = 10 per fraction
        assert_eq!(layout_resolve_1d(60, &edges), vec![10, 20, 30]);
    }

    #[test]
    fn resolve_1d_mixed_fixed_and_flexible() {
        let edges = vec![
            Edge {
                size: Some(20),
                fraction: 1,
                min_size: 0,
            },
            Edge {
                size: None,
                fraction: 1,
                min_size: 0,
            },
            Edge {
                size: None,
                fraction: 1,
                min_size: 0,
            },
        ];
        // remaining = 100 - 20 = 80, split equally: 40 each
        assert_eq!(layout_resolve_1d(100, &edges), vec![20, 40, 40]);
    }

    #[test]
    fn resolve_1d_min_size_kicks_in() {
        let edges = vec![
            Edge {
                size: None,
                fraction: 1,
                min_size: 50,
            },
            Edge {
                size: None,
                fraction: 1,
                min_size: 0,
            },
        ];
        // 60 total, equal weight. 60/2 = 30 each, but edge 0 needs min 50.
        // Fix edge 0 at 50, remaining = 10, edge 1 gets 10.
        assert_eq!(layout_resolve_1d(60, &edges), vec![50, 10]);
    }

    #[test]
    fn resolve_1d_zero_total() {
        let edges = vec![
            Edge {
                size: None,
                fraction: 1,
                min_size: 0,
            },
            Edge {
                size: None,
                fraction: 1,
                min_size: 0,
            },
        ];
        // remaining = 0 - 0 = 0, which is <= 0. Flexible edges get max(min_size, 1) = 1.
        assert_eq!(layout_resolve_1d(0, &edges), vec![1, 1]);
    }

    #[test]
    fn resolve_1d_single_edge_flexible() {
        let edges = vec![Edge {
            size: None,
            fraction: 1,
            min_size: 0,
        }];
        assert_eq!(layout_resolve_1d(50, &edges), vec![50]);
    }

    #[test]
    fn resolve_1d_single_edge_fixed() {
        let edges = vec![Edge {
            size: Some(30),
            fraction: 1,
            min_size: 0,
        }];
        assert_eq!(layout_resolve_1d(50, &edges), vec![30]);
    }

    #[test]
    fn resolve_1d_empty() {
        assert_eq!(layout_resolve_1d(100, &[]), Vec::<u16>::new());
    }

    #[test]
    fn resolve_1d_remainder_distribution() {
        // 100 / 3 = 33 remainder 1. The remainder cascades forward.
        let edges = vec![
            Edge {
                size: None,
                fraction: 1,
                min_size: 0,
            },
            Edge {
                size: None,
                fraction: 1,
                min_size: 0,
            },
            Edge {
                size: None,
                fraction: 1,
                min_size: 0,
            },
        ];
        let sizes = layout_resolve_1d(100, &edges);
        // Sum must equal 100.
        assert_eq!(sizes.iter().copied().sum::<u16>(), 100);
        // First two get 33, last gets 34 (remainder cascades).
        assert_eq!(sizes, vec![33, 33, 34]);
    }

    #[test]
    fn resolve_1d_many_edges() {
        let edges: Vec<Edge> = (0..10)
            .map(|_| Edge {
                size: None,
                fraction: 1,
                min_size: 0,
            })
            .collect();
        let sizes = layout_resolve_1d(100, &edges);
        assert_eq!(sizes.iter().copied().sum::<u16>(), 100);
        assert_eq!(sizes.len(), 10);
        // Each gets 10.
        assert!(sizes.iter().all(|&s| s == 10));
    }

    #[test]
    fn resolve_1d_no_room_assigns_min() {
        let edges = vec![
            Edge {
                size: Some(80),
                fraction: 1,
                min_size: 0,
            },
            Edge {
                size: None,
                fraction: 1,
                min_size: 5,
            },
        ];
        // remaining = 100 - 80 = 20, flexible gets 20.
        assert_eq!(layout_resolve_1d(100, &edges), vec![80, 20]);

        // Now with total=80: remaining = 80 - 80 = 0 → flexible gets max(5, 1) = 5.
        assert_eq!(layout_resolve_1d(80, &edges), vec![80, 5]);
    }

    #[test]
    fn resolve_1d_all_min_sizes_forced() {
        let edges = vec![
            Edge {
                size: None,
                fraction: 1,
                min_size: 40,
            },
            Edge {
                size: None,
                fraction: 1,
                min_size: 40,
            },
        ];
        // 50 total, each needs min 40. 50/2=25 < 40, so first fixed at 40.
        // remaining = 10, total_fr=1, 10*1 < 40*1 → second also fixed at 40.
        // Total (80) exceeds available (50), which the docstring allows.
        let sizes = layout_resolve_1d(50, &edges);
        assert_eq!(sizes, vec![40, 40]);
    }

    #[test]
    fn resolve_1d_fraction_weights_with_remainder() {
        let edges = vec![
            Edge {
                size: None,
                fraction: 1,
                min_size: 0,
            },
            Edge {
                size: None,
                fraction: 3,
                min_size: 0,
            },
        ];
        // total=100, total_fraction=4
        // edge 0: 100*1/4 = 25
        // edge 1: 100*3/4 = 75
        assert_eq!(layout_resolve_1d(100, &edges), vec![25, 75]);
    }

    #[test]
    fn resolve_1d_cascading_min_size() {
        // Three flexible edges, but min_size forces sequential fixation.
        let edges = vec![
            Edge {
                size: None,
                fraction: 1,
                min_size: 30,
            },
            Edge {
                size: None,
                fraction: 1,
                min_size: 25,
            },
            Edge {
                size: None,
                fraction: 1,
                min_size: 20,
            },
        ];
        // total=60, 60/3=20. Edge 0 needs 30 → fix at 30.
        // remaining=30, total_fr=2, 30/2=15. Edge 1 needs 25 → fix at 25.
        // remaining=5, total_fr=1, edge 2 gets 5 (but min_size 20 is not triggered
        // because we're in the distribution phase now and 5 >= 20 is false...wait)
        // Actually: remaining=5, fraction=1, 5*1 < 20*1 → fix at 20.
        // remaining = 5 - 20 = underflow → 0, no more flexible.
        // Hmm but remaining is u64, 5 < 20 so saturating_sub gives 0.
        // After fixing edge 2 at 20, flexible is empty, loop breaks.
        let sizes = layout_resolve_1d(60, &edges);
        assert_eq!(sizes, vec![30, 25, 20]);
    }

    // =========================================================================
    // Region tests
    // =========================================================================

    #[test]
    fn region_to_rect() {
        let r = Region::new(5, 10, 20, 15);
        let rect = r.to_rect();
        assert_eq!(
            rect,
            Rect {
                x0: 5,
                y0: 10,
                x1: 25,
                y1: 25
            }
        );
    }

    #[test]
    fn region_zero() {
        assert_eq!(Region::ZERO.to_rect(), Rect::ZERO);
    }

    // =========================================================================
    // Vertical layout tests
    // =========================================================================

    #[test]
    fn vertical_basic_stacking() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let a = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("A", Style::new().height(Scalar::Cells(10))),
        );
        let b = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("B", Style::new().height(Scalar::Cells(20))),
        );

        let available = Region::new(0, 0, 80, 50);
        layout_vertical(&mut tree, &[a, b], available, (80, 50), false);

        // A: 80x10 at (0,0)
        assert_layout_rect(&tree, a, 0, 0, 80, 10);
        assert_content_rect(&tree, a, 0, 0, 80, 10);

        // B: 80x20 at (0,10)
        assert_layout_rect(&tree, b, 0, 10, 80, 30);
        assert_content_rect(&tree, b, 0, 10, 80, 30);
    }

    #[test]
    fn vertical_fixed_plus_flex() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let fixed = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Fixed", Style::new().height(Scalar::Cells(10))),
        );
        let flex = tree.mount(root, LayoutTestWidget::boxed("Flex"));

        let available = Region::new(0, 0, 80, 50);
        layout_vertical(&mut tree, &[fixed, flex], available, (80, 50), false);

        assert_layout_rect(&tree, fixed, 0, 0, 80, 10);
        // Flex gets remaining: 50 - 10 = 40.
        assert_layout_rect(&tree, flex, 0, 10, 80, 50);
    }

    #[test]
    fn vertical_auto_height_uses_intrinsic_layout_height() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let a = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style_and_intrinsic_height(
                "A",
                Style::new().height(Scalar::Auto),
                3,
            ),
        );
        let b = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style_and_intrinsic_height(
                "B",
                Style::new().height(Scalar::Auto),
                3,
            ),
        );

        let available = Region::new(0, 0, 80, 20);
        layout_vertical(&mut tree, &[a, b], available, (80, 20), false);

        assert_layout_rect(&tree, a, 0, 0, 80, 3);
        assert_layout_rect(&tree, b, 0, 3, 80, 6);
    }

    #[test]
    fn vertical_auto_width_uses_intrinsic_content_width() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let a = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style_and_intrinsic_width(
                "A",
                Style::new().width(Scalar::Auto).height(Scalar::Cells(3)),
                12,
            ),
        );
        let b = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("B", Style::new().height(Scalar::Cells(3))),
        );

        let available = Region::new(0, 0, 80, 20);
        layout_vertical(&mut tree, &[a, b], available, (80, 20), false);

        // Auto-width with intrinsic width should not expand to full parent width.
        assert_layout_rect(&tree, a, 0, 0, 12, 3);
        // Non-auto/flex sibling still uses full width.
        assert_layout_rect(&tree, b, 0, 3, 80, 6);
    }

    #[test]
    fn vertical_with_margin() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let child = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style(
                "Child",
                Style::new()
                    .height(Scalar::Cells(10))
                    .margin(Spacing::new(2, 3, 2, 3)),
            ),
        );

        let available = Region::new(0, 0, 80, 50);
        layout_vertical(&mut tree, &[child], available, (80, 50), false);

        // Edge total = 10 + 2 + 2 = 14, but height_edge has chrome=4 + Cells(10) = 14 via scalar_to_edge.
        // layout_rect: x=0+3=3, y=0+2=2, w=80-3-3=74, h=14-2-2=10
        assert_layout_rect(&tree, child, 3, 2, 77, 12);
        assert_content_rect(&tree, child, 3, 2, 77, 12);
    }

    #[test]
    fn vertical_with_padding_and_border() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let child = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style(
                "Child",
                Style::new()
                    .height(Scalar::Cells(10))
                    .padding(Spacing::new(1, 2, 1, 2))
                    .border_top(Color::rgb(255, 255, 255))
                    .border_bottom(Color::rgb(255, 255, 255))
                    .border_left(Color::rgb(255, 255, 255))
                    .border_right(Color::rgb(255, 255, 255)),
            ),
        );

        let available = Region::new(0, 0, 80, 50);
        layout_vertical(&mut tree, &[child], available, (80, 50), false);

        // Default box-sizing is border-box (Textual parity):
        // explicit height includes border+padding chrome.
        // layout: x=0, y=0, w=80, h=10
        assert_layout_rect(&tree, child, 0, 0, 80, 10);
        // content: x=0+1+2=3, y=0+1+1=2, w=80-1-1-2-2=74, h=10-1-1-1-1=6
        assert_content_rect(&tree, child, 3, 2, 77, 8);
    }

    #[test]
    fn vertical_min_max_constraints() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let child = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Child", {
                let mut s = Style::new();
                s.max_width = Some(Scalar::Cells(40));
                s
            }),
        );

        let available = Region::new(0, 0, 80, 50);
        layout_vertical(&mut tree, &[child], available, (80, 50), false);

        // Max width 40 should constrain the layout width.
        let n = tree.get(child).unwrap();
        let w = n.layout_rect.x1 - n.layout_rect.x0;
        assert_eq!(w, 40);
    }

    // =========================================================================
    // Horizontal layout tests
    // =========================================================================

    #[test]
    fn horizontal_basic_stacking() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let a = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("A", Style::new().width(Scalar::Cells(20))),
        );
        let b = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("B", Style::new().width(Scalar::Cells(30))),
        );

        let available = Region::new(0, 0, 80, 50);
        layout_horizontal(&mut tree, &[a, b], available, (80, 50));

        // A: 20x50 at (0,0)
        assert_layout_rect(&tree, a, 0, 0, 20, 50);
        // B: 30x50 at (20,0)
        assert_layout_rect(&tree, b, 20, 0, 50, 50);
    }

    #[test]
    fn horizontal_fixed_plus_flex() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let fixed = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Fixed", Style::new().width(Scalar::Cells(20))),
        );
        let flex = tree.mount(root, LayoutTestWidget::boxed("Flex"));

        let available = Region::new(0, 0, 80, 50);
        layout_horizontal(&mut tree, &[fixed, flex], available, (80, 50));

        assert_layout_rect(&tree, fixed, 0, 0, 20, 50);
        // Flex: remaining = 80 - 20 = 60.
        assert_layout_rect(&tree, flex, 20, 0, 80, 50);
    }

    #[test]
    fn horizontal_with_margin() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let child = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style(
                "Child",
                Style::new()
                    .width(Scalar::Cells(20))
                    .margin(Spacing::new(2, 3, 2, 3)),
            ),
        );

        let available = Region::new(0, 0, 80, 50);
        layout_horizontal(&mut tree, &[child], available, (80, 50));

        // Edge total width = 20 + 3 + 3 = 26
        // layout: x=0+3=3, y=0+2=2, w=26-3-3=20, h=50-2-2=46
        assert_layout_rect(&tree, child, 3, 2, 23, 48);
    }

    // =========================================================================
    // Dock tests
    // =========================================================================

    #[test]
    fn dock_top() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let docked = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Header", {
                let mut s = Style::new().height(Scalar::Cells(3));
                s.dock = Some(Dock::Top);
                s
            }),
        );

        let available = Region::new(0, 0, 80, 50);
        let remaining = arrange_dock(&mut tree, &[docked], available, (80, 50));

        // Docked at top: 80x3 at (0,0)
        assert_layout_rect(&tree, docked, 0, 0, 80, 3);
        // Remaining region starts at y=3.
        assert_eq!(remaining, Region::new(0, 3, 80, 47));
    }

    #[test]
    fn dock_bottom() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let docked = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Footer", {
                let mut s = Style::new().height(Scalar::Cells(2));
                s.dock = Some(Dock::Bottom);
                s
            }),
        );

        let available = Region::new(0, 0, 80, 50);
        let remaining = arrange_dock(&mut tree, &[docked], available, (80, 50));

        // Docked at bottom: 80x2 at (0,48)
        assert_layout_rect(&tree, docked, 0, 48, 80, 50);
        assert_eq!(remaining, Region::new(0, 0, 80, 48));
    }

    #[test]
    fn dock_left() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let docked = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Sidebar", {
                let mut s = Style::new().width(Scalar::Cells(20));
                s.dock = Some(Dock::Left);
                s
            }),
        );

        let available = Region::new(0, 0, 80, 50);
        let remaining = arrange_dock(&mut tree, &[docked], available, (80, 50));

        // Docked at left: 20x50 at (0,0)
        assert_layout_rect(&tree, docked, 0, 0, 20, 50);
        assert_eq!(remaining, Region::new(20, 0, 60, 50));
    }

    #[test]
    fn dock_right() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let docked = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Panel", {
                let mut s = Style::new().width(Scalar::Cells(25));
                s.dock = Some(Dock::Right);
                s
            }),
        );

        let available = Region::new(0, 0, 80, 50);
        let remaining = arrange_dock(&mut tree, &[docked], available, (80, 50));

        // Docked at right: 25x50 at (55,0)
        assert_layout_rect(&tree, docked, 55, 0, 80, 50);
        assert_eq!(remaining, Region::new(0, 0, 55, 50));
    }

    #[test]
    fn dock_multiple() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let header = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Header", {
                let mut s = Style::new().height(Scalar::Cells(3));
                s.dock = Some(Dock::Top);
                s
            }),
        );
        let footer = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Footer", {
                let mut s = Style::new().height(Scalar::Cells(2));
                s.dock = Some(Dock::Bottom);
                s
            }),
        );
        let sidebar = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Sidebar", {
                let mut s = Style::new().width(Scalar::Cells(20));
                s.dock = Some(Dock::Left);
                s
            }),
        );

        let available = Region::new(0, 0, 80, 50);
        let remaining = arrange_dock(&mut tree, &[header, footer, sidebar], available, (80, 50));

        // Header: top, full width, 3 tall.
        assert_layout_rect(&tree, header, 0, 0, 80, 3);

        // Footer: bottom, full width, 2 tall. y1 = 50 → footer at y=48.
        assert_layout_rect(&tree, footer, 0, 48, 80, 50);

        // Sidebar: left, after header carved top (y0=3) and footer carved bottom (y1=48).
        // Height = 48 - 3 = 45. Width = 20.
        assert_layout_rect(&tree, sidebar, 0, 3, 20, 48);

        // Remaining: x=20, y=3, w=60, h=45
        assert_eq!(remaining, Region::new(20, 3, 60, 45));
    }

    // =========================================================================
    // Dock auto-width tests (carve_edge Scalar::Auto handling)
    // =========================================================================

    #[test]
    fn dock_left_auto_width_uses_content_width() {
        // Widget with content_width() = Some(25) docked left with width: auto
        // should get 25 columns, not 0 or full available.
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let docked = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style_and_intrinsic_width(
                "Sidebar",
                {
                    let mut s = Style::new().width(Scalar::Auto);
                    s.dock = Some(crate::style::Dock::Left);
                    s
                },
                25,
            ),
        );

        let available = Region::new(0, 0, 80, 50);
        let remaining = arrange_dock(&mut tree, &[docked], available, (80, 50));

        // Docked at left: 25x50 at (0,0).
        assert_layout_rect(&tree, docked, 0, 0, 25, 50);
        assert_eq!(remaining, Region::new(25, 0, 55, 50));
    }

    #[test]
    fn dock_left_auto_width_falls_back_to_available() {
        // Widget without content_width() (returns None) docked left with
        // width: auto should fall back to current available width.
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let docked = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Sidebar", {
                let mut s = Style::new().width(Scalar::Auto);
                s.dock = Some(crate::style::Dock::Left);
                s
            }),
        );

        let available = Region::new(0, 0, 80, 50);
        let remaining = arrange_dock(&mut tree, &[docked], available, (80, 50));

        // No intrinsic width → falls back to full available (80).
        assert_layout_rect(&tree, docked, 0, 0, 80, 50);
        assert_eq!(remaining, Region::new(80, 0, 0, 50));
    }

    #[test]
    fn dock_left_explicit_width_unchanged() {
        // Widget with explicit width: 20 (Cells) should still get 20,
        // regardless of content_width.
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let docked = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style_and_intrinsic_width(
                "Sidebar",
                {
                    let mut s = Style::new().width(Scalar::Cells(20));
                    s.dock = Some(crate::style::Dock::Left);
                    s
                },
                50,
            ), // intrinsic_width=50, but explicit Cells(20) should win
        );

        let available = Region::new(0, 0, 80, 50);
        let remaining = arrange_dock(&mut tree, &[docked], available, (80, 50));

        // Docked at left: 20x50, NOT 50.
        assert_layout_rect(&tree, docked, 0, 0, 20, 50);
        assert_eq!(remaining, Region::new(20, 0, 60, 50));
    }

    #[test]
    fn dock_top_auto_height_uses_intrinsic_height() {
        // Widget with layout_height() = Some(5) docked top with height: auto
        // should get 5 rows.
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let docked = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style_and_intrinsic_height(
                "Header",
                {
                    let mut s = Style::new().height(Scalar::Auto);
                    s.dock = Some(crate::style::Dock::Top);
                    s
                },
                5,
            ),
        );

        let available = Region::new(0, 0, 80, 50);
        let remaining = arrange_dock(&mut tree, &[docked], available, (80, 50));

        // Docked at top: 80x5 at (0,0).
        assert_layout_rect(&tree, docked, 0, 0, 80, 5);
        assert_eq!(remaining, Region::new(0, 5, 80, 45));
    }

    #[test]
    fn dock_top_unset_height_uses_intrinsic_height() {
        // Regression: carve_edge previously defaulted to h=1 when style.height
        // is None (unset), ignoring the widget's layout_height(). This caused
        // docked widgets with no explicit CSS height (e.g. an Input inside a Node
        // with `dock: top`) to be sized as 1 row instead of their natural height.
        // The fix aligns None with Some(Scalar::Auto): both use layout_height().
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let docked = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style_and_intrinsic_height(
                "Header",
                {
                    let mut s = Style::new();
                    // height is intentionally NOT set (None)
                    s.dock = Some(crate::style::Dock::Top);
                    s
                },
                3,
            ),
        );

        let available = Region::new(0, 0, 80, 50);
        let remaining = arrange_dock(&mut tree, &[docked], available, (80, 50));

        // Must use intrinsic height 3, not the old default of 1.
        assert_layout_rect(&tree, docked, 0, 0, 80, 3);
        assert_eq!(remaining, Region::new(0, 3, 80, 47));
    }

    #[test]
    fn dock_left_auto_width_with_max_width_clamp() {
        // Widget with content_width()=60 but max_width=40 — max_width should clamp.
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let docked = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style_and_intrinsic_width(
                "Sidebar",
                {
                    let mut s = Style::new().width(Scalar::Auto);
                    s.dock = Some(crate::style::Dock::Left);
                    s.max_width = Some(Scalar::Cells(40));
                    s
                },
                60,
            ),
        );

        let available = Region::new(0, 0, 80, 50);
        let remaining = arrange_dock(&mut tree, &[docked], available, (80, 50));

        // content_width=60 clamped to max_width=40.
        assert_layout_rect(&tree, docked, 0, 0, 40, 50);
        assert_eq!(remaining, Region::new(40, 0, 40, 50));
    }

    #[test]
    fn dock_plus_layout_children() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed_with_style("Container", {
            let mut s = Style::new();
            s.layout = Some(Layout::Vertical);
            s
        }));
        let header = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Header", {
                let mut s = Style::new().height(Scalar::Cells(3));
                s.dock = Some(Dock::Top);
                s
            }),
        );
        let body = tree.mount(root, LayoutTestWidget::boxed("Body"));

        let available = Region::new(0, 0, 80, 50);
        resolve_layout(&mut tree, root, available, (80, 50));

        // Header docked at top.
        assert_layout_rect(&tree, header, 0, 0, 80, 3);

        // Body fills remaining space: y=3, h=47.
        assert_layout_rect(&tree, body, 0, 3, 80, 50);
    }

    #[test]
    fn dock_parent_top_plus_fill_uses_remaining_region() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Dock"));
        let header = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Header", {
                let mut s = Style::new().height(Scalar::Cells(3));
                s.dock = Some(Dock::Top);
                s
            }),
        );
        let fill = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Fill", Style::new().height(Scalar::Cells(1))),
        );

        resolve_layout(&mut tree, root, Region::new(0, 0, 80, 50), (80, 50));

        assert_layout_rect(&tree, header, 0, 0, 80, 3);
        assert_layout_rect(&tree, fill, 0, 3, 80, 50);
    }

    // =========================================================================
    // resolve_layout dispatch tests
    // =========================================================================

    #[test]
    fn resolve_layout_vertical_default() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let a = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("A", Style::new().height(Scalar::Cells(10))),
        );
        let b = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("B", Style::new().height(Scalar::Cells(20))),
        );

        resolve_layout(&mut tree, root, Region::new(0, 0, 80, 50), (80, 50));

        assert_layout_rect(&tree, a, 0, 0, 80, 10);
        assert_layout_rect(&tree, b, 0, 10, 80, 30);
    }

    #[test]
    fn resolve_layout_horizontal() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed_with_style("Container", {
            let mut s = Style::new();
            s.layout = Some(Layout::Horizontal);
            s
        }));
        let a = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("A", Style::new().width(Scalar::Cells(30))),
        );
        let b = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("B", Style::new().width(Scalar::Cells(50))),
        );

        resolve_layout(&mut tree, root, Region::new(0, 0, 80, 50), (80, 50));

        assert_layout_rect(&tree, a, 0, 0, 30, 50);
        assert_layout_rect(&tree, b, 30, 0, 80, 50);
    }

    #[test]
    fn resolve_layout_grid_dispatch() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed_with_style("Container", {
            let mut s = Style::new();
            s.layout = Some(Layout::Grid);
            s.grid_size_columns = Some(2);
            s
        }));
        let a = tree.mount(root, LayoutTestWidget::boxed("A"));
        let b = tree.mount(root, LayoutTestWidget::boxed("B"));
        let c = tree.mount(root, LayoutTestWidget::boxed("C"));
        let d = tree.mount(root, LayoutTestWidget::boxed("D"));

        resolve_layout(&mut tree, root, Region::new(0, 0, 80, 50), (80, 50));

        // 2 cols × 2 rows, all 1fr.
        // col_widths = [40, 40], row_heights = [25, 25].
        assert_layout_rect(&tree, a, 0, 0, 40, 25);
        assert_layout_rect(&tree, b, 40, 0, 80, 25);
        assert_layout_rect(&tree, c, 0, 25, 40, 50);
        assert_layout_rect(&tree, d, 40, 25, 80, 50);
    }

    #[test]
    fn resolve_layout_display_none_excluded() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let visible = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Visible", Style::new().height(Scalar::Cells(10))),
        );
        let hidden = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Hidden", {
                let mut s = Style::new().height(Scalar::Cells(10));
                s.display = Some(Display::None);
                s
            }),
        );

        resolve_layout(&mut tree, root, Region::new(0, 0, 80, 50), (80, 50));

        // Visible child gets laid out.
        assert_layout_rect(&tree, visible, 0, 0, 80, 10);
        // Hidden child's rects should remain at ZERO (never touched by layout).
        assert_layout_rect(&tree, hidden, 0, 0, 0, 0);
    }

    #[test]
    fn resolve_layout_no_children_is_noop() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Empty"));
        // Should not panic.
        resolve_layout(&mut tree, root, Region::new(0, 0, 80, 50), (80, 50));
    }

    #[test]
    fn resolve_layout_recurses_into_grandchildren() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Root"));
        let parent = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Parent", Style::new().height(Scalar::Cells(20))),
        );
        let child = tree.mount(
            parent,
            LayoutTestWidget::boxed_with_style("Child", Style::new().height(Scalar::Cells(5))),
        );

        resolve_layout(&mut tree, root, Region::new(0, 0, 80, 50), (80, 50));

        assert_layout_rect(&tree, parent, 0, 0, 80, 20);
        assert_layout_rect(&tree, child, 0, 0, 80, 5);
    }

    // =========================================================================
    // Edge case tests
    // =========================================================================

    #[test]
    fn resolve_1d_large_min_exceeds_total() {
        let edges = vec![
            Edge {
                size: None,
                fraction: 1,
                min_size: 60,
            },
            Edge {
                size: None,
                fraction: 1,
                min_size: 60,
            },
        ];
        // Both need 60, only 50 available. First gets fixed at 60,
        // remaining = 0 (saturating_sub). Second also triggers min_size (0 < 60).
        // Total (120) exceeds available (50), which the docstring allows.
        let sizes = layout_resolve_1d(50, &edges);
        assert_eq!(sizes, vec![60, 60]);
    }

    #[test]
    fn vertical_offset_region() {
        // Layout at a non-zero starting position.
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let a = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("A", Style::new().height(Scalar::Cells(5))),
        );

        let available = Region::new(10, 20, 60, 30);
        layout_vertical(&mut tree, &[a], available, (80, 50), false);

        // A should be at x=10, y=20.
        assert_layout_rect(&tree, a, 10, 20, 70, 25);
    }

    // =========================================================================
    // Grid layout tests
    // =========================================================================

    #[test]
    fn grid_basic_2x2() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let a = tree.mount(root, LayoutTestWidget::boxed("A"));
        let b = tree.mount(root, LayoutTestWidget::boxed("B"));
        let c = tree.mount(root, LayoutTestWidget::boxed("C"));
        let d = tree.mount(root, LayoutTestWidget::boxed("D"));

        let parent_style = {
            let mut s = Style::new();
            s.grid_size_columns = Some(2);
            s
        };
        let available = Region::new(0, 0, 80, 50);
        layout_grid(&mut tree, &[a, b, c, d], available, (80, 50), &parent_style);

        // 2 cols × 2 rows, all 1fr.
        // col_widths = [40, 40], row_heights = [25, 25].
        assert_layout_rect(&tree, a, 0, 0, 40, 25);
        assert_layout_rect(&tree, b, 40, 0, 80, 25);
        assert_layout_rect(&tree, c, 0, 25, 40, 50);
        assert_layout_rect(&tree, d, 40, 25, 80, 50);
    }

    #[test]
    fn grid_3x2() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let children: Vec<NodeId> = (0..6)
            .map(|_| tree.mount(root, LayoutTestWidget::boxed("Cell")))
            .collect();

        let parent_style = {
            let mut s = Style::new();
            s.grid_size_columns = Some(3);
            s
        };
        let available = Region::new(0, 0, 90, 60);
        layout_grid(&mut tree, &children, available, (90, 60), &parent_style);

        // 3 cols × 2 rows. col_widths = [30, 30, 30], row_heights = [30, 30].
        assert_layout_rect(&tree, children[0], 0, 0, 30, 30);
        assert_layout_rect(&tree, children[1], 30, 0, 60, 30);
        assert_layout_rect(&tree, children[2], 60, 0, 90, 30);
        assert_layout_rect(&tree, children[3], 0, 30, 30, 60);
        assert_layout_rect(&tree, children[4], 30, 30, 60, 60);
        assert_layout_rect(&tree, children[5], 60, 30, 90, 60);
    }

    #[test]
    fn grid_non_uniform_columns() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let a = tree.mount(root, LayoutTestWidget::boxed("A"));
        let b = tree.mount(root, LayoutTestWidget::boxed("B"));

        let parent_style = {
            let mut s = Style::new();
            s.grid_size_columns = Some(2);
            s.grid_columns = Some(vec![Scalar::Fraction(1.0), Scalar::Fraction(2.0)]);
            s
        };
        let available = Region::new(0, 0, 90, 30);
        layout_grid(&mut tree, &[a, b], available, (90, 30), &parent_style);

        // 1fr + 2fr = 3fr. 90/3 = 30 per fr.
        // col_widths = [30, 60].
        assert_layout_rect(&tree, a, 0, 0, 30, 30);
        assert_layout_rect(&tree, b, 30, 0, 90, 30);
    }

    #[test]
    fn grid_with_gutter() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let a = tree.mount(root, LayoutTestWidget::boxed("A"));
        let b = tree.mount(root, LayoutTestWidget::boxed("B"));
        let c = tree.mount(root, LayoutTestWidget::boxed("C"));
        let d = tree.mount(root, LayoutTestWidget::boxed("D"));

        let parent_style = {
            let mut s = Style::new();
            s.grid_size_columns = Some(2);
            s.grid_gutter_horizontal = Some(2);
            s.grid_gutter_vertical = Some(4);
            s
        };
        let available = Region::new(0, 0, 84, 52);
        layout_grid(&mut tree, &[a, b, c, d], available, (84, 52), &parent_style);

        // col_budget = 84 - 4 = 80. col_widths = [40, 40].
        // row_budget = 52 - 2 = 50. row_heights = [25, 25].
        // col_offsets = [0, 44] (40 + 4 gutter).
        // row_offsets = [0, 27] (25 + 2 gutter).
        assert_layout_rect(&tree, a, 0, 0, 40, 25);
        assert_layout_rect(&tree, b, 44, 0, 84, 25);
        assert_layout_rect(&tree, c, 0, 27, 40, 52);
        assert_layout_rect(&tree, d, 44, 27, 84, 52);
    }

    #[test]
    fn grid_overflow_extra_rows() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let children: Vec<NodeId> = (0..4)
            .map(|_| tree.mount(root, LayoutTestWidget::boxed("Cell")))
            .collect();

        let parent_style = {
            let mut s = Style::new();
            s.grid_size_columns = Some(2);
            s.grid_size_rows = Some(1); // only 1 row configured
            s
        };
        let available = Region::new(0, 0, 80, 40);
        layout_grid(&mut tree, &children, available, (80, 40), &parent_style);

        // 4 children, 2 cols, 1 explicit row → actual rows = max(1, 2) = 2.
        // col_widths = [40, 40], row_heights = [20, 20].
        assert_layout_rect(&tree, children[0], 0, 0, 40, 20);
        assert_layout_rect(&tree, children[1], 40, 0, 80, 20);
        assert_layout_rect(&tree, children[2], 0, 20, 40, 40);
        assert_layout_rect(&tree, children[3], 40, 20, 80, 40);
    }

    #[test]
    fn grid_fewer_children_than_cells() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let a = tree.mount(root, LayoutTestWidget::boxed("A"));
        let b = tree.mount(root, LayoutTestWidget::boxed("B"));

        let parent_style = {
            let mut s = Style::new();
            s.grid_size_columns = Some(3);
            s
        };
        let available = Region::new(0, 0, 90, 30);
        layout_grid(&mut tree, &[a, b], available, (90, 30), &parent_style);

        // 2 children in 3-col grid → 1 row. col_widths = [30, 30, 30].
        // A in (0,0), B in (1,0). Third cell empty.
        assert_layout_rect(&tree, a, 0, 0, 30, 30);
        assert_layout_rect(&tree, b, 30, 0, 60, 30);
    }

    #[test]
    fn grid_explicit_rows() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let a = tree.mount(root, LayoutTestWidget::boxed("A"));
        let b = tree.mount(root, LayoutTestWidget::boxed("B"));

        let parent_style = {
            let mut s = Style::new();
            s.grid_size_columns = Some(2);
            s.grid_size_rows = Some(3);
            s
        };
        let available = Region::new(0, 0, 80, 60);
        layout_grid(&mut tree, &[a, b], available, (80, 60), &parent_style);

        // 2 children, 3 explicit rows → 3 rows (even though only 1 needed).
        // row_heights: 60/3 = [20, 20, 20].
        // A in col 0/row 0, B in col 1/row 0.
        assert_layout_rect(&tree, a, 0, 0, 40, 20);
        assert_layout_rect(&tree, b, 40, 0, 80, 20);
    }

    #[test]
    fn grid_default_single_column() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let a = tree.mount(root, LayoutTestWidget::boxed("A"));
        let b = tree.mount(root, LayoutTestWidget::boxed("B"));
        let c = tree.mount(root, LayoutTestWidget::boxed("C"));

        // No grid properties set → default 1 column.
        let parent_style = Style::new();
        let available = Region::new(0, 0, 80, 90);
        layout_grid(&mut tree, &[a, b, c], available, (80, 90), &parent_style);

        // 1 col, 3 rows. col_widths = [80], row_heights = [30, 30, 30].
        assert_layout_rect(&tree, a, 0, 0, 80, 30);
        assert_layout_rect(&tree, b, 0, 30, 80, 60);
        assert_layout_rect(&tree, c, 0, 60, 80, 90);
    }

    #[test]
    fn grid_fixed_column_widths() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let a = tree.mount(root, LayoutTestWidget::boxed("A"));
        let b = tree.mount(root, LayoutTestWidget::boxed("B"));

        let parent_style = {
            let mut s = Style::new();
            s.grid_size_columns = Some(2);
            s.grid_columns = Some(vec![Scalar::Cells(20), Scalar::Cells(30)]);
            s
        };
        let available = Region::new(0, 0, 80, 40);
        layout_grid(&mut tree, &[a, b], available, (80, 40), &parent_style);

        // Fixed columns: 20 + 30 = 50 (leaves 30 unused).
        assert_layout_rect(&tree, a, 0, 0, 20, 40);
        assert_layout_rect(&tree, b, 20, 0, 50, 40);
    }

    #[test]
    fn grid_column_scalar_cycling() {
        // grid-columns: 1fr 2fr applied to a 4-column grid cycles as 1fr 2fr 1fr 2fr.
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let children: Vec<NodeId> = (0..4)
            .map(|_| tree.mount(root, LayoutTestWidget::boxed("Cell")))
            .collect();

        let parent_style = {
            let mut s = Style::new();
            s.grid_size_columns = Some(4);
            s.grid_columns = Some(vec![Scalar::Fraction(1.0), Scalar::Fraction(2.0)]);
            s
        };
        let available = Region::new(0, 0, 60, 20);
        layout_grid(&mut tree, &children, available, (60, 20), &parent_style);

        // 1fr 2fr 1fr 2fr → total 6fr. 60/6 = 10 per fr.
        // col_widths = [10, 20, 10, 20].
        assert_layout_rect(&tree, children[0], 0, 0, 10, 20);
        assert_layout_rect(&tree, children[1], 10, 0, 30, 20);
        assert_layout_rect(&tree, children[2], 30, 0, 40, 20);
        assert_layout_rect(&tree, children[3], 40, 0, 60, 20);
    }

    #[test]
    fn grid_with_child_max_width() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let a = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("A", {
                let mut s = Style::new();
                s.max_width = Some(Scalar::Cells(15));
                s
            }),
        );
        let b = tree.mount(root, LayoutTestWidget::boxed("B"));

        let parent_style = {
            let mut s = Style::new();
            s.grid_size_columns = Some(2);
            s
        };
        let available = Region::new(0, 0, 80, 30);
        layout_grid(&mut tree, &[a, b], available, (80, 30), &parent_style);

        // Cell width = 40, but A has max_width=15.
        let n = tree.get(a).unwrap();
        let w = n.layout_rect.x1 - n.layout_rect.x0;
        assert_eq!(w, 15);

        // B fills its cell normally.
        assert_layout_rect(&tree, b, 40, 0, 80, 30);
    }

    #[test]
    fn grid_empty_children() {
        let mut tree = WidgetTree::new();
        let _root = tree.set_root(LayoutTestWidget::boxed("Container"));

        let parent_style = {
            let mut s = Style::new();
            s.grid_size_columns = Some(2);
            s
        };
        // Should not panic with empty children.
        layout_grid(
            &mut tree,
            &[],
            Region::new(0, 0, 80, 50),
            (80, 50),
            &parent_style,
        );
    }

    #[test]
    fn grid_offset_region() {
        // Grid at a non-zero starting position.
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let a = tree.mount(root, LayoutTestWidget::boxed("A"));
        let b = tree.mount(root, LayoutTestWidget::boxed("B"));

        let parent_style = {
            let mut s = Style::new();
            s.grid_size_columns = Some(2);
            s
        };
        let available = Region::new(10, 20, 60, 40);
        layout_grid(&mut tree, &[a, b], available, (80, 50), &parent_style);

        // col_widths = [30, 30], row_heights = [40].
        // A: x=10+0=10, B: x=10+30=40.
        assert_layout_rect(&tree, a, 10, 20, 40, 60);
        assert_layout_rect(&tree, b, 40, 20, 70, 60);
    }

    #[test]
    fn overlay_parent_layers_base_and_modal_in_same_region() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Root"));
        let overlay = tree.mount(root, LayoutTestWidget::boxed("Overlay"));
        let base = tree.mount(overlay, LayoutTestWidget::boxed("Base"));
        let modal = tree.mount(overlay, LayoutTestWidget::boxed("Modal"));

        resolve_layout(&mut tree, root, Region::new(0, 0, 80, 20), (80, 20));

        assert_layout_rect(&tree, overlay, 0, 0, 80, 20);
        assert_layout_rect(&tree, base, 0, 0, 80, 20);
        assert_layout_rect(&tree, modal, 0, 0, 80, 20);
    }
}
