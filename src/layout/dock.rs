use crate::node_id::NodeId;
use crate::widget_tree::WidgetTree;

use super::common::get_node_style;
use super::region::{CarveDir, Region, border_spacing};
use super::split::carve_edge;

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
    let mut x0 = available.x;
    let mut y0 = available.y;
    let mut x1 = available.x.saturating_add(available.width);
    let mut y1 = available.y.saturating_add(available.height);

    for &child in docked {
        let style = get_node_style(tree, child);
        let dock = match style.dock {
            Some(d) => d,
            None => continue,
        };
        carve_edge(
            tree,
            child,
            CarveDir::from(dock),
            &mut x0,
            &mut y0,
            &mut x1,
            &mut y1,
            viewport,
        );
    }

    Region::new(x0, y0, x1.saturating_sub(x0), y1.saturating_sub(y0))
}

/// Place a Dock fill child into the remaining region.
///
/// Tree layout must preserve Dock semantics where one non-docked child (fill)
/// consumes all space left after docking.
pub(crate) fn layout_dock_fill(tree: &mut WidgetTree, child: NodeId, inner: Region) {
    let style = get_node_style(tree, child);
    let padding = style.effective_padding();
    let (bt, bb, bl, br) = border_spacing(&style);

    let border_top = bt as u16;
    let border_bottom = bb as u16;
    let border_left = bl as u16;
    let border_right = br as u16;

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
