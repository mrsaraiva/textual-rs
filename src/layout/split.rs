use crate::node_id::NodeId;
use crate::style::{BoxSizing, OffsetValue, Scalar};
use crate::widget_tree::WidgetTree;

use super::common::{
    get_node_style, measure_intrinsic_content_height, measure_intrinsic_content_width,
    resolve_scalar_to_cells,
};
use super::region::{CarveDir, Region, border_spacing};

// ---------------------------------------------------------------------------
// Edge-carving: shared by dock (P2-12) and split (P2-26)
// ---------------------------------------------------------------------------

/// Position a single edge-carving child and shrink the available bounds.
///
/// Used by both dock and split layout: the child's size is resolved from its
/// style, then it is placed along the specified edge, and the bounds (x0/y0/x1/y1)
/// are reduced by the child's outer size.
/// Box-model result of an edge-carving child computed against an available area.
///
/// All values are in cells. `outer_w`/`outer_h` include border+padding+margin.
/// `border_*`/`padding`/`chrome_*` describe the gutter used to derive the inner
/// content rect from the placed (margin-excluded) layout rect.
pub(crate) struct CarveBox {
    pub outer_w: u16,
    pub outer_h: u16,
    pub margin: crate::style::Spacing,
    pub border_top: u16,
    pub border_left: u16,
    pub padding_top: u16,
    pub padding_left: u16,
    pub chrome_w: u16,
    pub chrome_h: u16,
}

/// Compute the box model (outer size + gutter) of an edge-carving child against
/// an available content area of `avail_w` x `avail_h`.
///
/// This is the size-resolution half of edge carving, shared by split (which
/// computes against the progressively-reduced bounds) and dock (which, matching
/// Python `_arrange_dock_widgets`, computes every dock against the *full* region
/// so docks overlap at the corners). It does not mutate the tree.
pub(crate) fn compute_carve_box(
    tree: &WidgetTree,
    child: NodeId,
    avail_w: u16,
    avail_h: u16,
    viewport: (u16, u16),
) -> CarveBox {
    let style = get_node_style(tree, child);
    let margin = style.effective_margin();
    let padding = style.effective_padding();
    let (bt, bb, bl, br) = border_spacing(&style);
    let border_top = bt;
    let border_bottom = bb;
    let border_left = bl;
    let border_right = br;
    let box_sizing = style.box_sizing.unwrap_or(BoxSizing::BorderBox);

    let current_w = avail_w;
    let current_h = avail_h;

    // Resolve child's content size from its style.
    //
    // For `Scalar::Auto`, use widget intrinsic dimensions (mirroring how
    // `extract_child_spec` handles auto for flow children). This is critical
    // for dock-left/right children with `width: auto` — without it, Auto
    // resolves to 0 and the widget becomes invisible.
    let height_is_explicit = matches!(style.height, Some(ref s) if !matches!(s, Scalar::Auto));
    let width_is_explicit = matches!(style.width, Some(ref s) if !matches!(s, Scalar::Auto));

    // Post-keystone, `layout_height()` reports PURE content height (no folded
    // border/padding), so a reported value is treated exactly like any other
    // content size and the full vertical chrome is added below — symmetric with
    // the width axis and with `common::measure_child_outer_height`.
    let child_h = match style.height.as_ref() {
        None => {
            // Unset: use widget intrinsic content height if available, fall back to
            // full available height (fill behaviour for unset height, same as
            // `extract_child_spec` for None with no intrinsic).
            match tree
                .get(child)
                .and_then(|node| node.widget.layout_height())
                .and_then(|h| u16::try_from(h).ok())
            {
                Some(h) => h,
                None => current_h,
            }
        }
        Some(Scalar::Auto) => {
            // Explicit `height: auto`: size to content, NOT to the remaining
            // available height. Python parity (`_get_box_model`: `is_auto_height`
            // branch calls `get_content_height` instead of filling the container).
            //
            // Try intrinsic leaf content height first (Button/Checkbox report their
            // own pure content height). Fall back to `measure_intrinsic_content_height`
            // for containers whose children were drained into the arena tree
            // (layout_height == None). Only if measurement also yields nothing
            // (truly empty / unmeasurable) do we fall back to filling the height.
            let leaf = tree
                .get(child)
                .and_then(|node| node.widget.layout_height())
                .and_then(|h| u16::try_from(h).ok());
            if let Some(h) = leaf {
                h
            } else {
                measure_intrinsic_content_height(tree, child, viewport, current_h)
                    .unwrap_or(current_h)
            }
        }
        // A docked/split widget sized in `fr` on an axis fills that axis: Python's
        // box model resolves a lone `1fr` against the available size (the dock
        // region's own extent), so it behaves like `100%`. `resolve_scalar_to_cells`
        // cannot do this — it has no sibling-fr context and returns 0 — so resolve
        // it to the full available height here.
        Some(Scalar::Fraction(_)) => current_h,
        Some(s) => resolve_scalar_to_cells(s, current_h, viewport),
    };
    let child_w = match style.width.as_ref() {
        None => current_w, // truly unset → full available width
        Some(Scalar::Auto) => {
            // Explicit `width: auto`: size to content, NOT to the remaining
            // available width (the mirror of the `height: auto` branch above).
            // Python parity (`_get_box_model`: `is_auto_width` branch calls
            // `get_content_width` instead of filling the container).
            //
            // Try the widget's own intrinsic width first (fast path for leaf
            // widgets that report `content_width()`). When the widget reports
            // None — true for a docked *container* whose children were drained
            // into the arena tree (e.g. `Container(Label("left"))` docked left
            // with `width: auto`) — recursively measure the intrinsic content
            // width of its subtree. Only if measurement also yields nothing do
            // we fall back to `layout.max_width`, then to the available width.
            let intrinsic = tree
                .get(child)
                .and_then(|node| node.widget.content_width())
                .and_then(|w| u16::try_from(w).ok());
            if let Some(w) = intrinsic {
                w
            } else if let Some(w) = measure_intrinsic_content_width(tree, child, viewport) {
                w
            } else {
                let max_w = tree
                    .styles(child)
                    .and_then(|s| s.layout.max_width)
                    .and_then(|w| u16::try_from(w).ok());
                max_w.unwrap_or(current_w)
            }
        }
        // `width: 1fr` on a dock/split widget fills the available width (see the
        // height `Fraction` arm above for the rationale).
        Some(Scalar::Fraction(_)) => current_w,
        Some(s) => resolve_scalar_to_cells(s, current_w, viewport),
    };

    // Apply min/max width constraints from CSS style.
    let child_w = {
        let mut w = child_w;
        if let Some(ref s) = style.min_width {
            w = w.max(resolve_scalar_to_cells(s, current_w, viewport));
        }
        if let Some(ref s) = style.max_width {
            w = w.min(resolve_scalar_to_cells(s, current_w, viewport));
        }
        w
    };
    let child_h = {
        let mut h = child_h;
        if let Some(ref s) = style.min_height {
            h = h.max(resolve_scalar_to_cells(s, current_h, viewport));
        }
        if let Some(ref s) = style.max_height {
            h = h.min(resolve_scalar_to_cells(s, current_h, viewport));
        }
        h
    };

    let chrome_h = border_top + border_bottom + padding.top + padding.bottom;
    let chrome_w = border_left + border_right + padding.left + padding.right;

    // For border-box with explicit size, the specified value already includes
    // padding+border. Only add margin for the outer dimension.
    //
    // Python parity (`Widget.get_box_model`): a border-box explicit size is
    // `content = size - gutter`, clamped to `max(0, content)`, then the box is
    // `content + gutter`. So when the specified size is smaller than the box's
    // own chrome (border + padding), the box does NOT collapse below its chrome
    // — it stays at chrome size with zero content. Without this clamp an Input
    // with `height: 1` + `border: tall` (chrome 2) renders only its top border
    // row instead of top + bottom border rows.
    let border_box_size = |specified: u16, chrome: u16| -> u16 { specified.max(chrome) };
    let outer_h = if box_sizing == BoxSizing::BorderBox && height_is_explicit {
        border_box_size(child_h, chrome_h).saturating_add(margin.top + margin.bottom)
    } else {
        child_h
            .saturating_add(chrome_h)
            .saturating_add(margin.top + margin.bottom)
    };
    let outer_w = if box_sizing == BoxSizing::BorderBox && width_is_explicit {
        border_box_size(child_w, chrome_w).saturating_add(margin.left + margin.right)
    } else {
        child_w
            .saturating_add(chrome_w)
            .saturating_add(margin.left + margin.right)
    };

    CarveBox {
        outer_w,
        outer_h,
        margin,
        border_top,
        border_left,
        padding_top: padding.top,
        padding_left: padding.left,
        chrome_w,
        chrome_h,
    }
}

/// Position a single edge-carving child and shrink the available bounds.
///
/// Used by SPLIT layout: the child's size is resolved from its style against the
/// current (progressively-reduced) bounds, it is placed along the edge, and the
/// bounds (x0/y0/x1/y1) are reduced by the child's outer size so the next split
/// sees a smaller area. (Dock layout no longer routes through here — see
/// `arrange_dock`, which follows Python's overlapping-dock model.)
#[allow(clippy::too_many_arguments)]
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
    let current_w = x1.saturating_sub(*x0);
    let current_h = y1.saturating_sub(*y0);
    let bx = compute_carve_box(tree, child, current_w, current_h, viewport);
    let CarveBox {
        outer_w,
        outer_h,
        margin,
        border_top,
        border_left,
        padding_top,
        padding_left,
        chrome_w,
        chrome_h,
    } = bx;

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
    let content_x = layout_x.saturating_add(border_left + padding_left);
    let content_y = layout_y.saturating_add(border_top + padding_top);
    let content_w = layout_w.saturating_sub(chrome_w);
    let content_h = layout_h.saturating_sub(chrome_h);

    if let Some(node) = tree.get_mut(child) {
        node.layout_rect =
            Region::new(i32::from(layout_x), i32::from(layout_y), layout_w, layout_h).to_rect();
        node.content_rect =
            Region::new(i32::from(content_x), i32::from(content_y), content_w, content_h).to_rect();
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
    // Split partitions the screen into non-negative regions (no offset), so the
    // edge-carving bounds are unsigned. The signed `available.x/y` (which is
    // non-negative for a split container) is converted at the boundary.
    let mut x0 = available.x.max(0) as u16;
    let mut y0 = available.y.max(0) as u16;
    let mut x1 = x0.saturating_add(available.width);
    let mut y1 = y0.saturating_add(available.height);

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

    Region::new(
        i32::from(x0),
        i32::from(y0),
        x1.saturating_sub(x0),
        y1.saturating_sub(y0),
    )
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

        // Resolve width/height. Mirrors Python `_get_box_model`: an `auto`
        // dimension shrinks to the child's intrinsic content (plus its own
        // chrome), NOT the full available region. Only a `None` (unset)
        // dimension falls back to filling the available region. This makes an
        // absolutely-positioned `Label` (default `width: auto`) size to its
        // text instead of stretching across the screen.
        let mut layout_w = match style.width.as_ref() {
            Some(Scalar::Auto) => measure_intrinsic_content_width(tree, child, viewport)
                .map(|w| w.saturating_add(chrome_w))
                .unwrap_or_else(|| available.width.saturating_sub(margin.left + margin.right)),
            Some(s) => {
                let content_w = resolve_scalar_to_cells(s, available.width, viewport);
                if box_sizing == BoxSizing::BorderBox && width_is_explicit {
                    content_w
                } else {
                    content_w.saturating_add(chrome_w)
                }
            }
            None => available.width.saturating_sub(margin.left + margin.right),
        };
        let mut layout_h = match style.height.as_ref() {
            Some(Scalar::Auto) => {
                // `measure_intrinsic_content_height` returns PURE content height
                // (post-keystone; symmetric with `auto_content_width` on the width
                // arm above), so this widget's own vertical chrome is added here to
                // get the outer box height. Mirrors Python `_get_box_model` sizing
                // an auto-height widget to its content box + chrome.
                let avail_content_h = available
                    .height
                    .saturating_sub(margin.top + margin.bottom)
                    .saturating_sub(chrome_h);
                measure_intrinsic_content_height(tree, child, viewport, avail_content_h)
                    .map(|h| h.saturating_add(chrome_h))
                    .unwrap_or_else(|| available.height.saturating_sub(margin.top + margin.bottom))
            }
            Some(s) => {
                let content_h = resolve_scalar_to_cells(s, available.height, viewport);
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
            let min_w = resolve_scalar_to_cells(s, available.width, viewport);
            let min_w_outer = if box_sizing == BoxSizing::BorderBox {
                min_w
            } else {
                min_w.saturating_add(chrome_w)
            };
            layout_w = layout_w.max(min_w_outer);
        }
        if let Some(ref s) = style.max_width {
            let max_w = resolve_scalar_to_cells(s, available.width, viewport);
            let max_w_outer = if box_sizing == BoxSizing::BorderBox {
                max_w
            } else {
                max_w.saturating_add(chrome_w)
            };
            layout_w = layout_w.min(max_w_outer);
        }
        if let Some(ref s) = style.min_height {
            let min_h = resolve_scalar_to_cells(s, available.height, viewport);
            let min_h_outer = if box_sizing == BoxSizing::BorderBox {
                min_h
            } else {
                min_h.saturating_add(chrome_h)
            };
            layout_h = layout_h.max(min_h_outer);
        }
        if let Some(ref s) = style.max_height {
            let max_h = resolve_scalar_to_cells(s, available.height, viewport);
            let max_h_outer = if box_sizing == BoxSizing::BorderBox {
                max_h
            } else {
                max_h.saturating_add(chrome_h)
            };
            layout_h = layout_h.min(max_h_outer);
        }

        // Position: at parent origin + margin + absolute_offset + offset.
        // Positions are signed so a negative offset (`position: absolute; offset:
        // -x -y`) survives to the render clip instead of being clamped to 0.
        //
        // `absolute_offset` is a runtime-supplied screen anchor (Python
        // `Widget._absolute_offset`, e.g. the tooltip's `mouse_position`). It is
        // added BEFORE the CSS `offset` so `offset-x: -50%` centers the box on the
        // anchor. `None` for every node that does not opt in — those keep the
        // exact prior `base = origin + margin` placement.
        let offset = style.offset.unwrap_or_default();
        let (abs_x, abs_y) = tree
            .get(child)
            .and_then(|n| n.absolute_offset)
            .unwrap_or((0, 0));
        let base_x = available.x + i32::from(margin.left) + abs_x;
        let base_y = available.y + i32::from(margin.top) + abs_y;
        let layout_x = {
            let dx = match offset.x {
                OffsetValue::Cells(c) => c as i32,
                OffsetValue::Percent(p) => (layout_w as f32 * p / 100.0).round() as i32,
            };
            base_x + dx
        };
        let layout_y = {
            let dy = match offset.y {
                OffsetValue::Cells(c) => c as i32,
                OffsetValue::Percent(p) => (layout_h as f32 * p / 100.0).round() as i32,
            };
            base_y + dy
        };

        let content_x = layout_x + i32::from(bl + padding.left);
        let content_y = layout_y + i32::from(bt + padding.top);
        let content_w = layout_w.saturating_sub(chrome_w);
        let content_h = layout_h.saturating_sub(chrome_h);

        if let Some(node) = tree.get_mut(child) {
            node.layout_rect = Region::new(layout_x, layout_y, layout_w, layout_h).to_rect();
            node.content_rect = Region::new(content_x, content_y, content_w, content_h).to_rect();
        }
    }
}
