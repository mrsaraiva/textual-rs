use crate::node_id::NodeId;
use crate::style::Dock;
use crate::widget_tree::WidgetTree;

use super::common::get_node_style;
use super::region::{Region, border_spacing};
use super::split::compute_carve_box;

// ---------------------------------------------------------------------------
// Dock layout (P2-12)
// ---------------------------------------------------------------------------

/// Position docked children, carving space from the available region.
///
/// Iterates children with a `dock` style (top/bottom/left/right), positions
/// each one, and returns the reduced available [`Region`] for flow children.
///
/// Reference: Python Textual's `_arrange.py` (`_arrange_dock_widgets`).
pub fn arrange_dock(
    tree: &mut WidgetTree,
    docked: &[NodeId],
    available: Region,
    viewport: (u16, u16),
) -> Region {
    // Python parity (`_arrange.py::_arrange_dock_widgets`): every docked widget is
    // sized and placed against the SAME (full) dock region — docks do NOT consume
    // each other's space, they overlap at the corners. Only the accumulated
    // `dock_spacing` (max widget extent per edge) shrinks the region returned for
    // the non-docked flow children.
    let region_x = available.x;
    let region_y = available.y;
    let width = available.width;
    let height = available.height;

    // Accumulated reserved spacing per edge (Python `top/right/bottom/left`).
    let (mut top, mut right, mut bottom, mut left) = (0u16, 0u16, 0u16, 0u16);

    for &child in docked {
        let style = get_node_style(tree, child);
        let edge = match style.dock {
            Some(d) => d,
            None => continue,
        };

        // Box model against the FULL region (not a progressively-shrunk one).
        let bx = compute_carve_box(tree, child, width, height, viewport);
        let widget_width = bx.outer_w;
        let widget_height = bx.outer_h;

        // Region-local placement of the OUTER (margin-inclusive) box, mirroring
        // Python's `Region(...)` per edge. Width/height that are not constrained
        // by the edge axis still come from the widget's own resolved size — a
        // `width: 100%` top dock spans the full region width; a `width: auto`
        // left dock keeps its content width.
        let (mut ox, mut oy, ow, oh) = match edge {
            Dock::Top => {
                top = top.max(widget_height);
                (0u16, 0u16, widget_width, widget_height)
            }
            Dock::Bottom => {
                bottom = bottom.max(widget_height);
                (0u16, height.saturating_sub(widget_height), widget_width, widget_height)
            }
            Dock::Left => {
                left = left.max(widget_width);
                (0u16, 0u16, widget_width, widget_height)
            }
            Dock::Right => {
                right = right.max(widget_width);
                (width.saturating_sub(widget_width), 0u16, widget_width, widget_height)
            }
        };

        // `dock_region.shrink(margin)`: remove margin to get the placed layout box.
        ox = ox.saturating_add(bx.margin.left);
        oy = oy.saturating_add(bx.margin.top);
        let layout_w = ow.saturating_sub(bx.margin.left + bx.margin.right);
        let layout_h = oh.saturating_sub(bx.margin.top + bx.margin.bottom);

        // Translate to absolute coordinates.
        let layout_x = region_x.saturating_add(ox);
        let layout_y = region_y.saturating_add(oy);

        // Inner content rect (placed box minus border+padding gutter).
        let content_x = layout_x.saturating_add(bx.border_left + bx.padding_left);
        let content_y = layout_y.saturating_add(bx.border_top + bx.padding_top);
        let content_w = layout_w.saturating_sub(bx.chrome_w);
        let content_h = layout_h.saturating_sub(bx.chrome_h);

        if let Some(node) = tree.get_mut(child) {
            node.layout_rect = Region::new(layout_x, layout_y, layout_w, layout_h).to_rect();
            node.content_rect =
                Region::new(content_x, content_y, content_w, content_h).to_rect();
        }
    }

    // Shrink the region by accumulated spacing for the flow children.
    let inner_x = region_x.saturating_add(left);
    let inner_y = region_y.saturating_add(top);
    let inner_w = width.saturating_sub(left + right);
    let inner_h = height.saturating_sub(top + bottom);
    Region::new(inner_x, inner_y, inner_w, inner_h)
}

/// Place a Dock fill child into the remaining region.
///
/// Tree layout must preserve Dock semantics where one non-docked child (fill)
/// consumes all space left after docking.
pub(crate) fn layout_dock_fill(tree: &mut WidgetTree, child: NodeId, inner: Region) {
    let style = get_node_style(tree, child);
    let padding = style.effective_padding();
    let (bt, bb, bl, br) = border_spacing(&style);

    let border_top = bt;
    let border_bottom = bb;
    let border_left = bl;
    let border_right = br;

    let chrome_w = border_left + border_right + padding.left + padding.right;
    let chrome_h = border_top + border_bottom + padding.top + padding.bottom;

    let content_x = inner.x.saturating_add(border_left + padding.left);
    let content_y = inner.y.saturating_add(border_top + padding.top);
    let content_w = inner.width.saturating_sub(chrome_w);
    let content_h = inner.height.saturating_sub(chrome_h);

    if let Some(node) = tree.get_mut(child) {
        node.layout_rect = inner.to_rect();
        node.content_rect = Region::new(content_x, content_y, content_w, content_h).to_rect();
    }
}
