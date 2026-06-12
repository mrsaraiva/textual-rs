use crate::node_id::NodeId;
use crate::style::{BoxSizing, OffsetValue, Scalar};
use crate::widget_tree::WidgetTree;

use super::common::{get_node_style, resolve_scalar_to_cells};
use super::region::{CarveDir, Region, border_spacing};

// ---------------------------------------------------------------------------
// Edge-carving: shared by dock (P2-12) and split (P2-26)
// ---------------------------------------------------------------------------

/// Position a single edge-carving child and shrink the available bounds.
///
/// Used by both dock and split layout: the child's size is resolved from its
/// style, then it is placed along the specified edge, and the bounds (x0/y0/x1/y1)
/// are reduced by the child's outer size.
pub(crate) fn carve_edge(
    tree: &mut WidgetTree,
    child: NodeId,
    direction: CarveDir,
    x0: &mut u16,
    y0: &mut u16,
    x1: &mut u16,
    y1: &mut u16,
    viewport: (u16, u16),
) {
    let style = get_node_style(tree, child);
    let margin = style.effective_margin();
    let padding = style.effective_padding();
    let (bt, bb, bl, br) = border_spacing(&style);
    let border_top = bt as u16;
    let border_bottom = bb as u16;
    let border_left = bl as u16;
    let border_right = br as u16;
    let box_sizing = style.box_sizing.unwrap_or(BoxSizing::BorderBox);

    let current_w = x1.saturating_sub(*x0);
    let current_h = y1.saturating_sub(*y0);

    // Resolve child's content size from its style.
    //
    // For `Scalar::Auto`, use widget intrinsic dimensions (mirroring how
    // `extract_child_spec` handles auto for flow children). This is critical
    // for dock-left/right children with `width: auto` — without it, Auto
    // resolves to 0 and the widget becomes invisible.
    let height_is_explicit = matches!(style.height, Some(ref s) if !matches!(s, Scalar::Auto));
    let width_is_explicit = matches!(style.width, Some(ref s) if !matches!(s, Scalar::Auto));

    let child_h = match style.height.as_ref() {
        None => 1, // truly unset → 1 row for dock/split children
        Some(Scalar::Auto) => {
            // Use widget's intrinsic height, fall back to current available.
            tree.get(child)
                .and_then(|node| node.widget.layout_height())
                .and_then(|h| u16::try_from(h).ok())
                .unwrap_or(current_h)
        }
        Some(s) => resolve_scalar_to_cells(s, current_h, viewport.1),
    };
    let child_w = match style.width.as_ref() {
        None => current_w, // truly unset → full available width
        Some(Scalar::Auto) => {
            // Use widget's intrinsic width (content_width), fall back to
            // layout_constraints max_width, then to current available.
            let intrinsic = tree
                .get(child)
                .and_then(|node| node.widget.content_width())
                .and_then(|w| u16::try_from(w).ok());
            if let Some(w) = intrinsic {
                w
            } else {
                let max_w = tree
                    .styles(child)
                    .and_then(|s| s.layout.max_width)
                    .and_then(|w| u16::try_from(w).ok());
                max_w.unwrap_or(current_w)
            }
        }
        Some(s) => resolve_scalar_to_cells(s, current_w, viewport.0),
    };

    // Apply min/max width constraints from CSS style.
    let child_w = {
        let mut w = child_w;
        if let Some(ref s) = style.min_width {
            w = w.max(resolve_scalar_to_cells(s, current_w, viewport.0));
        }
        if let Some(ref s) = style.max_width {
            w = w.min(resolve_scalar_to_cells(s, current_w, viewport.0));
        }
        w
    };
    let child_h = {
        let mut h = child_h;
        if let Some(ref s) = style.min_height {
            h = h.max(resolve_scalar_to_cells(s, current_h, viewport.1));
        }
        if let Some(ref s) = style.max_height {
            h = h.min(resolve_scalar_to_cells(s, current_h, viewport.1));
        }
        h
    };

    let chrome_h = border_top + border_bottom + padding.top + padding.bottom;
    let chrome_w = border_left + border_right + padding.left + padding.right;

    // For border-box with explicit size, the specified value already includes
    // padding+border. Only add margin for the outer dimension.
    let outer_h = if box_sizing == BoxSizing::BorderBox && height_is_explicit {
        child_h.saturating_add(margin.top + margin.bottom)
    } else {
        child_h
            .saturating_add(chrome_h)
            .saturating_add(margin.top + margin.bottom)
    };
    let outer_w = if box_sizing == BoxSizing::BorderBox && width_is_explicit {
        child_w.saturating_add(margin.left + margin.right)
    } else {
        child_w
            .saturating_add(chrome_w)
            .saturating_add(margin.left + margin.right)
    };

    let (layout_x, layout_y, layout_w, layout_h) = match direction {
        CarveDir::Top => {
            let lx = x0.saturating_add(margin.left);
            let ly = y0.saturating_add(margin.top);
            let lw = current_w.saturating_sub(margin.left + margin.right);
            let lh = outer_h.saturating_sub(margin.top + margin.bottom);
            *y0 = y0.saturating_add(outer_h);
            (lx, ly, lw, lh)
        }
        CarveDir::Bottom => {
            let lx = x0.saturating_add(margin.left);
            let ly = y1.saturating_sub(outer_h).saturating_add(margin.top);
            let lw = current_w.saturating_sub(margin.left + margin.right);
            let lh = outer_h.saturating_sub(margin.top + margin.bottom);
            *y1 = y1.saturating_sub(outer_h);
            (lx, ly, lw, lh)
        }
        CarveDir::Left => {
            let lx = x0.saturating_add(margin.left);
            let ly = y0.saturating_add(margin.top);
            let lw = outer_w.saturating_sub(margin.left + margin.right);
            let lh = current_h.saturating_sub(margin.top + margin.bottom);
            *x0 = x0.saturating_add(outer_w);
            (lx, ly, lw, lh)
        }
        CarveDir::Right => {
            let lx = x1.saturating_sub(outer_w).saturating_add(margin.left);
            let ly = y0.saturating_add(margin.top);
            let lw = outer_w.saturating_sub(margin.left + margin.right);
            let lh = current_h.saturating_sub(margin.top + margin.bottom);
            *x1 = x1.saturating_sub(outer_w);
            (lx, ly, lw, lh)
        }
    };

    // Content rect.
    let content_x = layout_x.saturating_add(border_left + padding.left);
    let content_y = layout_y.saturating_add(border_top + padding.top);
    let content_w = layout_w.saturating_sub(chrome_w);
    let content_h = layout_h.saturating_sub(chrome_h);

    if let Some(node) = tree.get_mut(child) {
        node.layout_rect = Region::new(layout_x, layout_y, layout_w, layout_h).to_rect();
        node.content_rect = Region::new(content_x, content_y, content_w, content_h).to_rect();
    }
}

// ---------------------------------------------------------------------------
// Split layout (P2-26)
// ---------------------------------------------------------------------------

/// Position split children, carving space from the available region.
///
/// Split is processed before dock in `resolve_layout`. It divides the
/// parent into persistent regions along the specified edge (top/right/bottom/left).
/// Semantically similar to dock but used for screen-level partitioning.
pub(crate) fn arrange_split(
    tree: &mut WidgetTree,
    split_children: &[NodeId],
    available: Region,
    viewport: (u16, u16),
) -> Region {
    let mut x0 = available.x;
    let mut y0 = available.y;
    let mut x1 = available.x.saturating_add(available.width);
    let mut y1 = available.y.saturating_add(available.height);

    for &child in split_children {
        let style = get_node_style(tree, child);
        let split = match style.split {
            Some(s) => s,
            None => continue,
        };
        carve_edge(
            tree,
            child,
            CarveDir::from(split),
            &mut x0,
            &mut y0,
            &mut x1,
            &mut y1,
            viewport,
        );
    }

    Region::new(x0, y0, x1.saturating_sub(x0), y1.saturating_sub(y0))
}

// ---------------------------------------------------------------------------
// Absolute positioning (P2-24)
// ---------------------------------------------------------------------------

/// Position absolutely-positioned children.
///
/// Absolute children are removed from normal flow. They are placed relative to
/// the parent's available region, using their specified width/height (or full
/// available if auto) plus any `offset` displacement.
pub(crate) fn layout_absolute(
    tree: &mut WidgetTree,
    children: &[NodeId],
    available: Region,
    viewport: (u16, u16),
) {
    for &child in children {
        let style = get_node_style(tree, child);
        let margin = style.effective_margin();
        let padding = style.effective_padding();
        let (bt, bb, bl, br) = border_spacing(&style);
        let box_sizing = style.box_sizing.unwrap_or(BoxSizing::BorderBox);

        let chrome_w = bl + br + padding.left + padding.right;
        let chrome_h = bt + bb + padding.top + padding.bottom;

        let height_is_explicit = style.height.is_some();
        let width_is_explicit = style.width.is_some();

        // Resolve width/height (default to full available minus margin).
        let mut layout_w = match style.width.as_ref() {
            Some(s) => {
                let content_w = resolve_scalar_to_cells(s, available.width, viewport.0);
                if box_sizing == BoxSizing::BorderBox && width_is_explicit {
                    content_w
                } else {
                    content_w.saturating_add(chrome_w)
                }
            }
            None => available.width.saturating_sub(margin.left + margin.right),
        };
        let mut layout_h = match style.height.as_ref() {
            Some(s) => {
                let content_h = resolve_scalar_to_cells(s, available.height, viewport.1);
                if box_sizing == BoxSizing::BorderBox && height_is_explicit {
                    content_h
                } else {
                    content_h.saturating_add(chrome_h)
                }
            }
            None => available.height.saturating_sub(margin.top + margin.bottom),
        };

        // Apply min/max constraints for absolute children (P2-24 follow-up).
        if let Some(ref s) = style.min_width {
            let min_w = resolve_scalar_to_cells(s, available.width, viewport.0);
            let min_w_outer = if box_sizing == BoxSizing::BorderBox {
                min_w
            } else {
                min_w.saturating_add(chrome_w)
            };
            layout_w = layout_w.max(min_w_outer);
        }
        if let Some(ref s) = style.max_width {
            let max_w = resolve_scalar_to_cells(s, available.width, viewport.0);
            let max_w_outer = if box_sizing == BoxSizing::BorderBox {
                max_w
            } else {
                max_w.saturating_add(chrome_w)
            };
            layout_w = layout_w.min(max_w_outer);
        }
        if let Some(ref s) = style.min_height {
            let min_h = resolve_scalar_to_cells(s, available.height, viewport.1);
            let min_h_outer = if box_sizing == BoxSizing::BorderBox {
                min_h
            } else {
                min_h.saturating_add(chrome_h)
            };
            layout_h = layout_h.max(min_h_outer);
        }
        if let Some(ref s) = style.max_height {
            let max_h = resolve_scalar_to_cells(s, available.height, viewport.1);
            let max_h_outer = if box_sizing == BoxSizing::BorderBox {
                max_h
            } else {
                max_h.saturating_add(chrome_h)
            };
            layout_h = layout_h.min(max_h_outer);
        }

        // Position: at parent origin + margin + offset.
        let offset = style.offset.unwrap_or_default();
        let base_x = available.x.saturating_add(margin.left);
        let base_y = available.y.saturating_add(margin.top);
        let layout_x = {
            let dx = match offset.x {
                OffsetValue::Cells(c) => c as i32,
                OffsetValue::Percent(p) => (layout_w as f32 * p / 100.0).round() as i32,
            };
            if dx >= 0 {
                base_x.saturating_add(dx as u16)
            } else {
                base_x.saturating_sub(dx.unsigned_abs() as u16)
            }
        };
        let layout_y = {
            let dy = match offset.y {
                OffsetValue::Cells(c) => c as i32,
                OffsetValue::Percent(p) => (layout_h as f32 * p / 100.0).round() as i32,
            };
            if dy >= 0 {
                base_y.saturating_add(dy as u16)
            } else {
                base_y.saturating_sub(dy.unsigned_abs() as u16)
            }
        };

        let content_x = layout_x.saturating_add(bl + padding.left);
        let content_y = layout_y.saturating_add(bt + padding.top);
        let content_w = layout_w.saturating_sub(chrome_w);
        let content_h = layout_h.saturating_sub(chrome_h);

        if let Some(node) = tree.get_mut(child) {
            node.layout_rect = Region::new(layout_x, layout_y, layout_w, layout_h).to_rect();
            node.content_rect = Region::new(content_x, content_y, content_w, content_h).to_rect();
        }
    }
}
