use crate::node_id::NodeId;
use crate::style::BoxSizing;
use crate::widget_tree::WidgetTree;

use super::common::{ChildSpec, extract_child_spec, get_node_style};
use super::region::Region;
use super::resolve_1d::{Edge, layout_resolve_1d};
pub fn layout_vertical(
    tree: &mut WidgetTree,
    children: &[NodeId],
    available: Region,
    viewport: (u16, u16),
    allow_h_overflow: bool,
) {
    if children.is_empty() {
        return;
    }

    // Phase 1: collect style specs (immutable borrow of tree).
    let mut specs: Vec<ChildSpec> = Vec::with_capacity(children.len());
    for &child in children {
        let style = get_node_style(tree, child);

        let width_is_auto =
            matches!(style.width.as_ref(), None | Some(crate::style::Scalar::Auto));
        // Intrinsic content-width hint, used to (a) widen the height-measurement seed and
        // (b) let auto-width children overflow a horizontally-scrollable parent.
        let pre_intrinsic_w = tree
            .get(child)
            .and_then(|node| node.widget.content_width())
            .and_then(|w| u16::try_from(w).ok());

        // Seed auto-height widgets with a realistic content width before we ask
        // for intrinsic height. Without this, widgets that depend on width
        // (e.g. Markdown) can measure at width=1 and inflate their first-frame
        // height by orders of magnitude.
        let seed_spec = extract_child_spec(
            &style,
            available.width,
            available.height,
            viewport,
            None,
            None,
        );
        let mut seed_layout_w = available
            .width
            .saturating_sub(seed_spec.margin.left + seed_spec.margin.right);
        if let Some(edge_w) = seed_spec.width_edge.size {
            let explicit_w = edge_w.saturating_sub(seed_spec.margin.left + seed_spec.margin.right);
            seed_layout_w = seed_layout_w.min(explicit_w);
        }
        // Horizontally-scrollable parent: measure auto-width children at their intrinsic
        // width so wrapping widgets (e.g. Label) report unwrapped height.
        if allow_h_overflow
            && width_is_auto
            && let Some(iw) = pre_intrinsic_w
        {
            let iw_outer = iw.saturating_add(
                seed_spec.border_left
                    + seed_spec.border_right
                    + seed_spec.padding.left
                    + seed_spec.padding.right,
            );
            seed_layout_w = seed_layout_w.max(iw_outer);
        }
        if let Some(max_w) = seed_spec.max_width_cells {
            let max_w_outer = if seed_spec.box_sizing == BoxSizing::BorderBox {
                max_w
            } else {
                max_w.saturating_add(
                    seed_spec.border_left
                        + seed_spec.border_right
                        + seed_spec.padding.left
                        + seed_spec.padding.right,
                )
            };
            seed_layout_w = seed_layout_w.min(max_w_outer);
        }
        let seed_content_w = seed_layout_w.saturating_sub(
            seed_spec.border_left
                + seed_spec.border_right
                + seed_spec.padding.left
                + seed_spec.padding.right,
        );
        let seed_content_h = available.height.max(1);
        if let Some(node) = tree.get_mut(child) {
            node.widget.on_layout(seed_content_w.max(1), seed_content_h);
        }

        let intrinsic_height = tree
            .get(child)
            .and_then(|node| node.widget.layout_height())
            .and_then(|h| u16::try_from(h).ok());
        let intrinsic_width = tree
            .get(child)
            .and_then(|node| node.widget.content_width())
            .and_then(|w| u16::try_from(w).ok());
        let mut spec = extract_child_spec(
            &style,
            available.width,
            available.height,
            viewport,
            intrinsic_height,
            intrinsic_width,
        );

        // P2-35: `expand: true` opts this child into flex-grow behavior on
        // the layout axis even when intrinsic auto sizing would otherwise
        // produce a fixed size.
        if style.expand == Some(true) && spec.height_edge.size.is_some() {
            spec.height_edge.size = None;
            spec.height_edge.fraction = spec.height_edge.fraction.max(1);
        }

        specs.push(spec);
    }

    // Phase 2: build edges for height distribution.
    let edges: Vec<Edge> = specs.iter().map(|s| s.height_edge).collect();
    let heights = layout_resolve_1d(available.height, &edges);

    // Phase 3: compute rects and write to tree (mutable borrow).
    // Track previous child's bottom margin for CSS-style margin collapsing:
    // the gap between adjacent siblings is max(prev.bottom, cur.top) not the sum.
    let mut y = available.y;
    let mut prev_margin_bottom: u16 = 0;
    for (i, &child) in children.iter().enumerate() {
        let spec = &specs[i];
        let total_h = heights[i];

        // Collapse vertical margins between adjacent children: subtract the
        // overlap so the effective gap equals max(prev_bottom, cur_top).
        let collapse = prev_margin_bottom.min(spec.margin.top);
        y = y.saturating_sub(collapse);

        // Layout rect excludes margin.
        let layout_x = available.x.saturating_add(spec.margin.left);
        let layout_y = y.saturating_add(spec.margin.top);
        let base_w = available
            .width
            .saturating_sub(spec.margin.left + spec.margin.right);
        let mut layout_w = base_w;
        let layout_h = total_h.saturating_sub(spec.margin.top + spec.margin.bottom);

        // Apply explicit width constraint (P2-25: width_edge.size includes chrome).
        if let Some(edge_w) = spec.width_edge.size {
            let explicit_w = edge_w.saturating_sub(spec.margin.left + spec.margin.right);
            if allow_h_overflow && spec.width_is_auto {
                // Horizontally-scrollable parent: let auto-width children keep their
                // intrinsic width (which may exceed the viewport) so the content
                // overflows and can be scrolled, instead of wrapping to the viewport.
                layout_w = explicit_w;
            } else {
                layout_w = base_w.min(explicit_w);
            }
        }

        // Apply max-width constraint (border-box: value already includes chrome).
        let layout_w = if let Some(max_w) = spec.max_width_cells {
            let max_w_outer = if spec.box_sizing == BoxSizing::BorderBox {
                max_w
            } else {
                max_w.saturating_add(
                    spec.border_left + spec.border_right + spec.padding.left + spec.padding.right,
                )
            };
            layout_w.min(max_w_outer)
        } else {
            layout_w
        };

        // Apply max-height constraint (border-box: value already includes chrome).
        let layout_h = if let Some(max_h) = spec.max_height_cells {
            let max_h_outer = if spec.box_sizing == BoxSizing::BorderBox {
                max_h
            } else {
                max_h.saturating_add(
                    spec.border_top + spec.border_bottom + spec.padding.top + spec.padding.bottom,
                )
            };
            layout_h.min(max_h_outer)
        } else {
            layout_h
        };

        // Content rect: inner area after border + padding.
        let content_x = layout_x.saturating_add(spec.border_left + spec.padding.left);
        let content_y = layout_y.saturating_add(spec.border_top + spec.padding.top);
        let content_w = layout_w.saturating_sub(
            spec.border_left + spec.border_right + spec.padding.left + spec.padding.right,
        );
        let content_h = layout_h.saturating_sub(
            spec.border_top + spec.border_bottom + spec.padding.top + spec.padding.bottom,
        );

        if let Some(node) = tree.get_mut(child) {
            node.layout_rect = Region::new(layout_x, layout_y, layout_w, layout_h).to_rect();
            node.content_rect = Region::new(content_x, content_y, content_w, content_h).to_rect();
        }

        y = y.saturating_add(total_h);
        prev_margin_bottom = spec.margin.bottom;
    }
}
