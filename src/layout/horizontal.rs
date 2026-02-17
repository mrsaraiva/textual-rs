use crate::node_id::NodeId;
use crate::style::BoxSizing;
use crate::widget_tree::WidgetTree;

use super::common::{ChildSpec, extract_child_spec, get_node_style};
use super::region::Region;
use super::resolve_1d::{Edge, layout_resolve_1d};
pub fn layout_horizontal(
    tree: &mut WidgetTree,
    children: &[NodeId],
    available: Region,
    viewport: (u16, u16),
) {
    if children.is_empty() {
        return;
    }

    // Phase 1: collect style specs.
    let specs: Vec<ChildSpec> = children
        .iter()
        .map(|&child| {
            let style = get_node_style(tree, child);
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
            if style.expand == Some(true) && spec.width_edge.size.is_some() {
                spec.width_edge.size = None;
                spec.width_edge.fraction = spec.width_edge.fraction.max(1);
            }

            spec
        })
        .collect();

    // Phase 2: build edges for width distribution.
    let edges: Vec<Edge> = specs.iter().map(|s| s.width_edge).collect();
    let widths = layout_resolve_1d(available.width, &edges);

    // Phase 3: compute rects and write to tree.
    let mut x = available.x;
    for (i, &child) in children.iter().enumerate() {
        let spec = &specs[i];
        let total_w = widths[i];

        // Layout rect excludes margin.
        let layout_x = x.saturating_add(spec.margin.left);
        let layout_y = available.y.saturating_add(spec.margin.top);
        let layout_w = total_w.saturating_sub(spec.margin.left + spec.margin.right);
        let mut layout_h = available
            .height
            .saturating_sub(spec.margin.top + spec.margin.bottom);

        // Apply explicit height constraint (P2-25: height_edge.size includes chrome).
        if let Some(edge_h) = spec.height_edge.size {
            let explicit_h = edge_h.saturating_sub(spec.margin.top + spec.margin.bottom);
            layout_h = layout_h.min(explicit_h);
        }

        // Apply max constraints (border-box: value already includes chrome).
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

        // Content rect.
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

        x = x.saturating_add(total_w);
    }
}
