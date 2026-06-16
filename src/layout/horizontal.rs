use crate::node_id::NodeId;
use crate::style::BoxSizing;
use crate::widget_tree::WidgetTree;

use super::common::{
    ChildSpec, extract_child_spec, get_node_style, measure_intrinsic_content_height,
    measure_intrinsic_content_width,
};
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
            let mut style = get_node_style(tree, child);
            // Transparent wrappers (`Node`): adopt the wrapped child's auto-sizing
            // on any unset axis (see vertical.rs for rationale).
            let (wrapper_w_auto_pre, wrapper_h_auto_pre) =
                super::common::wrapper_child_auto_axes(tree, child);
            if wrapper_w_auto_pre && style.width.is_none() {
                style.width = Some(crate::style::Scalar::Auto);
            }
            if wrapper_h_auto_pre && style.height.is_none() {
                style.height = Some(crate::style::Scalar::Auto);
            }
            // `style.width`/`style.height` were normalized to `Some(Auto)` above
            // for transparent wrappers with auto children, so a plain `Some(Auto)`
            // check covers both real auto widgets and those wrappers.
            let width_is_auto =
                matches!(style.width.as_ref(), Some(crate::style::Scalar::Auto));
            let height_is_auto =
                matches!(style.height.as_ref(), Some(crate::style::Scalar::Auto));
            let mut intrinsic_height = tree
                .get(child)
                .and_then(|node| node.widget.layout_height())
                .and_then(|h| u16::try_from(h).ok());
            let mut intrinsic_width = tree
                .get(child)
                .and_then(|node| node.widget.content_width())
                .and_then(|w| u16::try_from(w).ok());
            // `extract_child_spec` adds the FULL horizontal chrome on the
            // auto-WIDTH arm but only margin on the auto-HEIGHT arm, so pre-add
            // only the container's own vertical chrome to the measured height
            // (see vertical.rs for the full rationale). Pre-adding horizontal
            // chrome here would double-count against the width arm.
            let (_own_h_chrome, own_v_chrome) = super::common::own_box_chrome(&style);
            if intrinsic_width.is_none() && width_is_auto {
                intrinsic_width = measure_intrinsic_content_width(tree, child, viewport);
            }
            if intrinsic_height.is_none() && height_is_auto {
                // Available CONTENT height this auto child would receive (full
                // container height minus own margins + chrome) so Python's
                // all-dynamic-children rule can fill an `fr`-height child.
                let avail_content_h = available
                    .height
                    .saturating_sub(style.effective_margin().top + style.effective_margin().bottom)
                    .saturating_sub(own_v_chrome);
                intrinsic_height =
                    measure_intrinsic_content_height(tree, child, viewport, avail_content_h)
                        .map(|h| h.saturating_add(own_v_chrome));
            }
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
    //
    // Python parity (`layouts/horizontal.py`): adjacent horizontal margins
    // COLLAPSE — the gap between child `i` and `i+1` is `max(margin_i.right,
    // margin_{i+1}.left)`, not their sum. Each `width_edge.size` from
    // `extract_child_spec` already folds in this child's FULL left+right margin,
    // which would double-count the shared gap. Subtract the overlap (the smaller
    // of each interior pair of adjacent margins) from the first child's reserved
    // edge so the TOTAL width reserved for fixed children matches Python's single
    // collapsed `resolve_margin`, freeing that space for any flexible (`fr`)
    // siblings. Positioning (Phase 3) applies the collapse per-gap.
    let mut edges: Vec<Edge> = specs.iter().map(|s| s.width_edge).collect();
    let collapse_overlap: u16 = specs
        .windows(2)
        .map(|pair| pair[0].margin.right.min(pair[1].margin.left))
        .sum();
    // Only fixed-size (`Some`) edges fold margin into their size; a flexible
    // (`fr`/`auto`) first child keeps its margin out of the edge size, so the
    // overlap must NOT be removed from (nor later restored to) it.
    let mut overlap_removed_from_first = 0u16;
    if collapse_overlap > 0 {
        if let Some(first) = edges.first_mut() {
            if let Some(size) = first.size.as_mut() {
                *size = size.saturating_sub(collapse_overlap);
                overlap_removed_from_first = collapse_overlap;
            }
        }
    }
    let widths = layout_resolve_1d(available.width, &edges);

    // Phase 3: compute rects and write to tree.
    //
    // `layout_left` is the left edge of the current child's LAYOUT box (margin
    // already applied). The first child's left edge is `available.x +
    // margin.left`; each subsequent child's left edge is the previous layout
    // box's right edge plus the COLLAPSED gap (`max(this.right, next.left)`), so
    // adjacent margins overlap instead of summing (Python
    // `layouts/horizontal.py`).
    let mut layout_left = available.x.saturating_add(specs[0].margin.left);
    for (i, &child) in children.iter().enumerate() {
        let spec = &specs[i];
        // If `collapse_overlap` was removed from child 0's (fixed) edge in Phase
        // 2, add it back here to recover its true outer width; otherwise (flexible
        // first child, or no overlap) the resolved width is already correct.
        let total_w = if i == 0 {
            widths[i].saturating_add(overlap_removed_from_first)
        } else {
            widths[i]
        };

        // Layout rect excludes margin.
        let layout_x = layout_left;
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

        // Advance to the next child's layout-box left edge: this layout box's
        // right edge plus the COLLAPSED gap between the two boxes
        // (`max(this.margin.right, next.margin.left)`). The last child has no
        // successor, so its trailing margin simply ends the row.
        if let Some(next) = specs.get(i + 1) {
            let gap = spec.margin.right.max(next.margin.left);
            layout_left = layout_x.saturating_add(layout_w).saturating_add(gap);
        }
    }
}
