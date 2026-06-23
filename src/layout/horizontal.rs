use crate::node_id::NodeId;
use crate::style::{BoxSizing, Offset, OffsetValue};
use crate::widget_tree::WidgetTree;

use super::common::{
    ChildSpec, extract_child_spec, get_node_style, measure_intrinsic_content_height,
    measure_intrinsic_content_width,
};
use super::region::Region;
use super::resolve_1d::{Edge, layout_resolve_1d_exact};

/// Apply a CSS `offset` displacement to a (x, y) coordinate pair.
///
/// Mirrors Python `_arrange.py`: offset is a visual shift applied after the
/// normal-flow position is established. Percentage offsets resolve against the
/// widget's own layout box dimensions (matching `layout_absolute`).
fn apply_offset(x: u16, y: u16, offset: &Offset, layout_w: u16, layout_h: u16) -> (u16, u16) {
    let dx = match offset.x {
        OffsetValue::Cells(c) => c as i32,
        OffsetValue::Percent(p) => (layout_w as f32 * p / 100.0).round() as i32,
    };
    let dy = match offset.y {
        OffsetValue::Cells(c) => c as i32,
        OffsetValue::Percent(p) => (layout_h as f32 * p / 100.0).round() as i32,
    };
    let new_x = if dx >= 0 {
        x.saturating_add(dx as u16)
    } else {
        x.saturating_sub(dx.unsigned_abs() as u16)
    };
    let new_y = if dy >= 0 {
        y.saturating_add(dy as u16)
    } else {
        y.saturating_sub(dy.unsigned_abs() as u16)
    };
    (new_x, new_y)
}
pub fn layout_horizontal(
    tree: &mut WidgetTree,
    children: &[NodeId],
    available: Region,
    viewport: (u16, u16),
    allow_v_overflow: bool,
) {
    if children.is_empty() {
        return;
    }

    // Phase 1: collect style specs.
    let mut specs: Vec<ChildSpec> = Vec::with_capacity(children.len());
    // Collect per-child CSS `offset` displacements (visual shift applied after
    // flow position, mirroring Python `layouts/horizontal.py` WidgetPlacement).
    let mut offsets: Vec<Option<Offset>> = Vec::with_capacity(children.len());
    // Retain the (normalized) per-child style + intrinsic-width hint so the
    // width-aware height remeasure (Phase 2.5) can rebuild the height edge for
    // content-sized-height children once the resolved fr/fixed width is known.
    let mut styles: Vec<crate::style::Style> = Vec::with_capacity(children.len());
    let mut intrinsic_widths: Vec<Option<u16>> = Vec::with_capacity(children.len());
    for &child in children {
        let mut style = get_node_style(tree, child);
            // Transparent wrappers (`Node`): adopt the wrapped child's auto-sizing
            // on any unset axis (see vertical.rs for rationale).
            let (wrapper_w_auto_pre, _wrapper_h_auto_pre) =
                super::common::wrapper_child_auto_axes(tree, child);
            if wrapper_w_auto_pre && style.width.is_none() {
                style.width = Some(crate::style::Scalar::Auto);
            }
            // A transparent wrapper's unset height mirrors the wrapped child's
            // intent (`auto` → shrink, otherwise `1fr` flex-fill), NOT the
            // bare-leaf fill-the-container rule.
            if style.height.is_none()
                && let Some(h) = super::common::wrapper_unset_height(tree, child)
            {
                style.height = Some(h);
            }

            // A wrapped widget (this `child`'s parent is a transparent wrapper and
            // `child` is its sole flow child) must FILL the wrapper on each axis
            // the wrapper sized by ADOPTING the widget's extent — re-applying the
            // widget's own explicit size against the wrapper would shrink it (a
            // `height: 50%` widget would become 50% of an already-sized wrapper).
            // Own min/max on a filled axis were applied at the wrapper; clear them.
            // Axes where the wrapper has its OWN extent keep the widget's natural
            // size for the wrapper's `content-align` (`docs_center07`).
            let (fill_w, fill_h) = super::common::wrapper_child_fill_axes(tree, child);
            if fill_h {
                style.height = Some(crate::style::Scalar::Percent(100.0));
                style.min_height = None;
                style.max_height = None;
            }
            if fill_w {
                style.width = Some(crate::style::Scalar::Percent(100.0));
                style.min_width = None;
                style.max_width = None;
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

        specs.push(spec);
        offsets.push(style.offset);
        intrinsic_widths.push(intrinsic_width);
        styles.push(style);
    }

    // Phase 2: build edges for width distribution.
    //
    // Python parity (`_resolve.resolve_box_models` + `layouts/horizontal.py`):
    // the COLLAPSED total margin is reserved from the container width BEFORE the
    // fraction distribution, then the remaining space is divided among the
    // children's BOX widths (margin-excluded). Adjacent horizontal margins
    // COLLAPSE — the interior gap between child `i` and `i+1` is
    // `max(margin_i.right, margin_{i+1}.left)`, not their sum.
    //
    // `extract_child_spec` folds each child's FULL left+right margin into a FIXED
    // edge's `size` (flexible `fr`/`auto` edges carry no margin). To divide on a
    // uniform, margin-excluded basis we strip each fixed edge's own margin here
    // and reserve the single collapsed margin total from the resolver input — so
    // a `fr` child also has its share of the margin reserved (without this, two
    // `1fr` children split the FULL width and then each loses its margin in
    // Phase 3, under-sizing every flexible box by its margin).
    let collapsed_margin_total: u16 = {
        let interior: u16 = specs
            .windows(2)
            .map(|pair| pair[0].margin.right.max(pair[1].margin.left))
            .sum();
        interior
            .saturating_add(specs[0].margin.left)
            .saturating_add(specs[specs.len() - 1].margin.right)
    };
    let edges: Vec<Edge> = specs
        .iter()
        .map(|s| {
            let margin_lr = s.margin.left + s.margin.right;
            Edge {
                // Fixed edges include margin in `size`/`min_size`; strip it so the
                // resolver works on box widths. Flexible edges (`size: None`)
                // carry no margin in `size` already.
                size: s.width_edge.size.map(|sz| sz.saturating_sub(margin_lr)),
                fraction: s.width_edge.fraction,
                min_size: s.width_edge.min_size.saturating_sub(margin_lr),
            }
        })
        .collect();
    let resolve_total = available.width.saturating_sub(collapsed_margin_total);
    // EXACT cumulative-floor resolution (Python `_resolve.resolve` +
    // `layouts/horizontal.py`): fixed and `fr` children alike are sized to exact
    // `f64` cells, then floored on the running position so non-integer widths
    // (e.g. 25vh = 7.5) fence-post like Python and the `fr` children reserve space
    // against the EXACT fixed sizes (not the un-carried integer ones). See
    // `layout_resolve_1d_exact`.
    let fixed_exact: Vec<Option<f64>> = specs.iter().map(|s| s.frac_width).collect();
    let widths = layout_resolve_1d_exact(resolve_total, &edges, &fixed_exact);

    // Phase 2.5: width-aware height remeasure for content-sized-height children.
    //
    // Python parity (`_resolve.resolve_box_models`): a child's auto/unset height
    // is measured by `_get_box_model` at the child's RESOLVED width — for an
    // `fr`/fixed-width child that width is only known after the fraction pass.
    // Phase 1 here measured intrinsic height from the widget's STALE
    // `layout_height()` (whatever width it was last laid out at), so a wrapping
    // Label in a `width: 1fr` horizontal row (e.g. `text_style`) reported the
    // wrong wrapped-line count and under/over-sized its box. Re-seed each
    // content-height child's measurement width to its resolved content width and
    // rebuild the height edge. Fires for BOTH `height: auto` and an UNSET height
    // (a content leaf that reports an intrinsic height) — Phase 1's remeasure
    // only covered explicit `auto`, never the unset-height + fr-width case.
    for (i, &child) in children.iter().enumerate() {
        let style = &styles[i];
        // Only content-sized-height children depend on the wrap width. Explicit
        // (non-auto) heights and pure fr/flex fills do not.
        let height_is_content = matches!(
            style.height.as_ref(),
            None | Some(crate::style::Scalar::Auto)
        );
        if !height_is_content {
            continue;
        }
        let spec = &specs[i];
        // Box (margin-excluded) width resolved for this child, minus its own
        // horizontal chrome → the content width the widget wraps at.
        let resolved_box_w = widths[i];
        let resolved_content_w = resolved_box_w
            .saturating_sub(
                spec.border_left + spec.border_right + spec.padding.left + spec.padding.right,
            )
            .max(1);
        let avail_content_h = available
            .height
            .saturating_sub(style.effective_margin().top + style.effective_margin().bottom);
        // Re-seed the widget (and any wrapped subtree) at the resolved width so
        // `layout_height()` reflects the final wrap, then re-read it.
        if let Some(node) = tree.get_mut(child) {
            node.widget.on_layout(resolved_content_w, avail_content_h.max(1));
        }
        super::common::seed_wrapper_subtree_widths(
            tree,
            child,
            resolved_content_w,
            avail_content_h.max(1),
        );
        let remeasured_height = tree
            .get(child)
            .and_then(|node| node.widget.layout_height())
            .and_then(|h| u16::try_from(h).ok());
        if remeasured_height.is_none() {
            // No intrinsic content height at this width (e.g. a fill leaf or an
            // explicit-auto container drained into the arena): keep Phase 1's
            // spec, which already handled the fallback (full-fill or measured).
            continue;
        }
        // Rebuild the height edge at the remeasured intrinsic height, preserving
        // the resolved width edge / max / box-sizing of the Phase 1 spec.
        let rebuilt = extract_child_spec(
            style,
            available.width,
            available.height,
            viewport,
            remeasured_height,
            intrinsic_widths[i],
        );
        specs[i].height_edge = rebuilt.height_edge;
    }

    // Phase 3: compute rects and write to tree.
    //
    // `layout_left` is the left edge of the current child's LAYOUT (box) region.
    // The resolved `widths[i]` are already margin-excluded box widths. The first
    // child's left edge is `available.x + margin.left`; each subsequent child's
    // left edge is the previous box's right edge plus the COLLAPSED gap
    // (`max(this.right, next.left)`), so adjacent margins overlap instead of
    // summing (Python `layouts/horizontal.py`).
    let mut layout_left = available.x.saturating_add(specs[0].margin.left);
    for (i, &child) in children.iter().enumerate() {
        let spec = &specs[i];
        // Resolved widths are already box (margin-excluded), cumulative-floored.
        let layout_w = widths[i];

        // Layout rect excludes margin.
        let layout_x = layout_left;
        let layout_y = available.y.saturating_add(spec.margin.top);
        let mut layout_h = available
            .height
            .saturating_sub(spec.margin.top + spec.margin.bottom);

        // Apply explicit height constraint (P2-25: height_edge.size includes chrome).
        if let Some(edge_h) = spec.height_edge.size {
            let explicit_h = edge_h.saturating_sub(spec.margin.top + spec.margin.bottom);
            if allow_v_overflow {
                // Vertically-scrollable parent (`overflow-y: auto|scroll`): let the
                // child keep its resolved height (which may exceed the viewport, e.g.
                // a `min-height` larger than the container) so the content overflows
                // and can be scrolled, rather than clamping it to the viewport height.
                layout_h = explicit_h;
            } else {
                layout_h = layout_h.min(explicit_h);
            }
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

        // Apply CSS `offset` displacement (visual shift; does NOT alter flow).
        // Mirrors Python `layouts/horizontal.py`: offset is stored per WidgetPlacement
        // and applied when computing the widget's rendered position.
        let (visual_x, visual_y) = if let Some(ref off) = offsets[i] {
            apply_offset(layout_x, layout_y, off, layout_w, layout_h)
        } else {
            (layout_x, layout_y)
        };

        // Content rect.
        let content_x = visual_x.saturating_add(spec.border_left + spec.padding.left);
        let content_y = visual_y.saturating_add(spec.border_top + spec.padding.top);
        let content_w = layout_w.saturating_sub(
            spec.border_left + spec.border_right + spec.padding.left + spec.padding.right,
        );
        let content_h = layout_h.saturating_sub(
            spec.border_top + spec.border_bottom + spec.padding.top + spec.padding.bottom,
        );

        if let Some(node) = tree.get_mut(child) {
            node.layout_rect = Region::new(visual_x, visual_y, layout_w, layout_h).to_rect();
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
