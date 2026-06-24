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
    // Collect per-child CSS `offset` displacements (visual shift applied after
    // flow position, mirroring Python `layouts/vertical.py` WidgetPlacement).
    let mut offsets: Vec<Option<Offset>> = Vec::with_capacity(children.len());
    for &child in children {
        let mut style = get_node_style(tree, child);

        // Transparent styling wrappers (`Node`) stand in for the wrapped widget,
        // so an UNSET axis must adopt that widget's sizing intent: when the
        // wrapped child is `width:auto`/`height:auto`, make the wrapper behave as
        // `auto` (shrink-to-content) on that axis; otherwise it keeps the `1fr`
        // fill of an unset dimension. Done before spec extraction so the auto
        // arms (which size to the measured intrinsic) are selected correctly.
        let (wrapper_w_auto_pre, _wrapper_h_auto_pre) =
            super::common::wrapper_child_auto_axes(tree, child);
        if wrapper_w_auto_pre && style.width.is_none() {
            style.width = Some(crate::style::Scalar::Auto);
        }
        // A transparent wrapper's unset height mirrors the wrapped child's intent
        // (`auto` → shrink, otherwise `1fr` flex-fill); it must NOT fall through to
        // the bare-leaf "unset fills the whole container" rule, or a `Node`-wrapped
        // `1fr` container would overflow instead of sharing its track.
        if style.height.is_none()
            && let Some(h) = super::common::wrapper_unset_height(tree, child)
        {
            style.height = Some(h);
        }

        // This `child` is a wrapped widget (its parent — the node being laid out
        // here — is a transparent wrapper for which `child` is the sole flow
        // child). On an axis the wrapper sized by ADOPTING this widget's extent
        // (its own axis unset), the widget must FILL the wrapper instead of
        // re-applying its own explicit size against it (which would shrink a
        // `height: 50%` widget to 50% of an already-50%-sized wrapper —
        // `min_height`). The widget's own min/max on that axis were applied at the
        // wrapper; clear them to avoid double-application. Axes where the wrapper
        // carries its OWN extent are left untouched so the widget keeps its
        // natural size for the wrapper's `content-align` (`docs_center07`).
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

        let width_is_auto = matches!(
            style.width.as_ref(),
            None | Some(crate::style::Scalar::Auto)
        );
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
        // Transparent wrappers (`Node`) pass their content box straight through to
        // their single drained child, but `on_layout` on the wrapper is a no-op.
        // Seed the wrapped subtree with the wrapper's content width so width-
        // dependent intrinsic height (e.g. a wrapping Static/Label) measures at
        // the correct width instead of its stale full-viewport width.
        super::common::seed_wrapper_subtree_widths(
            tree,
            child,
            seed_content_w.max(1),
            seed_content_h,
        );

        let mut intrinsic_height = tree
            .get(child)
            .and_then(|node| node.widget.layout_height())
            .and_then(|h| u16::try_from(h).ok());
        let mut intrinsic_width = tree
            .get(child)
            .and_then(|node| node.widget.content_width())
            .and_then(|w| u16::try_from(w).ok());

        // Bottom-up intrinsic measurement for EXPLICITLY auto-sized containers
        // whose renderable children were drained into the arena tree
        // (`content_width()`/`layout_height()` == None). Only an explicit
        // `width: auto` / `height: auto` opts in — an UNSET dimension (None)
        // keeps the prior flex-fill behaviour so default `1fr` containers and
        // the Screen still fill. This narrows the blast radius to deliberately
        // author-marked `auto` containers.
        //
        // `style.width`/`style.height` were already normalized to `Some(Auto)`
        // above for transparent wrappers whose wrapped child is auto-sized, so a
        // plain `Some(Auto)` check now covers both real auto widgets and those
        // wrappers.
        let width_is_explicit_auto =
            matches!(style.width.as_ref(), Some(crate::style::Scalar::Auto));
        let height_is_explicit_auto =
            matches!(style.height.as_ref(), Some(crate::style::Scalar::Auto));
        // The measured value is the children's content extent (the container's
        // OWN border+padding are NOT included). `extract_child_spec` adds chrome
        // asymmetrically: the auto-WIDTH arm adds the FULL horizontal chrome
        // (margin+border+padding), while the auto-HEIGHT arm adds ONLY margin.
        // So we pre-add the container's own vertical chrome (border+padding) to
        // the measured HEIGHT — otherwise a measured auto container with its own
        // border (e.g. RadioSet `border: tall`) is clipped — but we must NOT
        // pre-add horizontal chrome, or it would be double-counted against the
        // width arm's `full_h_chrome` (e.g. a bordered `width: auto` Static box).
        let (_own_h_chrome, own_v_chrome) = super::common::own_box_chrome(&style);
        if intrinsic_width.is_none() && width_is_explicit_auto {
            intrinsic_width = measure_intrinsic_content_width(tree, child, viewport);
        }
        if intrinsic_height.is_none() && height_is_explicit_auto {
            // Available CONTENT height this auto container would receive (its
            // outer fill minus own margins + border/padding). Lets Python's
            // all-dynamic-children rule fill an `fr` child (e.g. `Center >
            // Middle(1fr)`); `measure_intrinsic_content_height` adds chrome back
            // via the caller's `+ own_v_chrome`.
            let avail_content_h = available
                .height
                .saturating_sub(style.effective_margin().top + style.effective_margin().bottom)
                .saturating_sub(own_v_chrome);
            intrinsic_height = measure_intrinsic_content_height(tree, child, viewport, avail_content_h)
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
        if style.expand == Some(true) && spec.height_edge.size.is_some() {
            spec.height_edge.size = None;
            spec.height_edge.fraction = spec.height_edge.fraction.max(1);
        }

        specs.push(spec);
        offsets.push(style.offset);
    }

    // Phase 2: build edges for height distribution.
    //
    // Python parity (`_resolve.resolve_box_models` + `layouts/vertical.py`): the
    // COLLAPSED total vertical margin is reserved from the container height
    // BEFORE the fraction distribution, then the remainder is divided among the
    // children's BOX heights (margin-excluded). `extract_child_spec` folds each
    // child's FULL top+bottom margin into a FIXED edge's `size` (flexible
    // `fr`/`auto` edges carry no margin); strip it here and reserve the single
    // collapsed total from the resolver input so a `fr` child also has its share
    // of the margin reserved (otherwise two `1fr` children split the FULL height
    // and each loses its margin in Phase 3, under-sizing every flexible box).
    let collapsed_margin_total: u16 = {
        let interior: u16 = specs
            .windows(2)
            .map(|pair| pair[0].margin.bottom.max(pair[1].margin.top))
            .sum();
        interior
            .saturating_add(specs[0].margin.top)
            .saturating_add(specs[specs.len() - 1].margin.bottom)
    };
    let edges: Vec<Edge> = specs
        .iter()
        .map(|s| {
            let margin_tb = s.margin.top + s.margin.bottom;
            Edge {
                size: s.height_edge.size.map(|sz| sz.saturating_sub(margin_tb)),
                fraction: s.height_edge.fraction,
                min_size: s.height_edge.min_size.saturating_sub(margin_tb),
            }
        })
        .collect();
    let resolve_total = available.height.saturating_sub(collapsed_margin_total);
    // EXACT cumulative-floor resolution (Python `_resolve.resolve` +
    // `layouts/vertical.py`): fixed and `fr` children alike are sized to exact
    // `f64` cells, then floored on the RUNNING position so a stack of non-integer
    // heights (e.g. 12.5h = 3.75) fence-posts like Python instead of each child
    // truncating independently AND the `fr` children reserving space against the
    // un-carried integer fixed sizes (which overflowed the row by the carry).
    let fixed_exact: Vec<Option<f64>> = specs.iter().map(|s| s.frac_height).collect();
    let heights = layout_resolve_1d_exact(resolve_total, &edges, &fixed_exact);

    // Phase 3: compute rects and write to tree (mutable borrow).
    // Track previous child's bottom margin for CSS-style margin collapsing:
    // the gap between adjacent siblings is max(prev.bottom, cur.top) not the sum.
    let mut y = available.y;
    let mut prev_margin_bottom: u16 = 0;
    for (i, &child) in children.iter().enumerate() {
        let spec = &specs[i];
        // Resolved heights are already box (margin-excluded), cumulative-floored.
        let layout_h = heights[i];

        // Margins are positioned explicitly below (top margin added to `y`, the
        // gap between siblings collapsed). The first child's top margin advances
        // `y`; collapse the overlap with the previous child's bottom margin.
        let collapse = prev_margin_bottom.min(spec.margin.top);
        y = y.saturating_sub(collapse);

        // Layout rect excludes margin.
        let layout_x = available.x.saturating_add(spec.margin.left);
        let layout_y = y.saturating_add(spec.margin.top);
        let base_w = available
            .width
            .saturating_sub(spec.margin.left + spec.margin.right);
        let mut layout_w = base_w;

        // Apply explicit width constraint (P2-25: width_edge.size includes chrome).
        if let Some(edge_w) = spec.width_edge.size {
            let explicit_w = edge_w.saturating_sub(spec.margin.left + spec.margin.right);
            if allow_h_overflow {
                // Horizontally-scrollable parent (`overflow-x: auto|scroll`): the
                // child keeps its RESOLVED width even when it exceeds the viewport,
                // so the content overflows and can be scrolled instead of wrapping
                // to the viewport. This covers BOTH `width: auto` (intrinsic
                // width) AND an explicit oversized width like `width: 150%`
                // (Python `_resolve.resolve_box_models` calls `_get_box_model`
                // WITHOUT `constrain_width`, so an explicit percentage width
                // resolves to e.g. 1.5x the container and is NOT clamped — the
                // compositor clips it to the viewport at render time). The grid
                // layout is the only Python layout that passes `constrain_width`.
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

        // Apply CSS `offset` displacement (visual shift; does NOT alter flow).
        // Mirrors Python `layouts/vertical.py`: offset is stored per WidgetPlacement
        // and applied when computing the widget's rendered position.
        let (visual_x, visual_y) = if let Some(ref off) = offsets[i] {
            apply_offset(layout_x, layout_y, off, layout_w, layout_h)
        } else {
            (layout_x, layout_y)
        };

        // Content rect: inner area after border + padding.
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

        // Advance past this child's full outer box: top margin + box height +
        // bottom margin. `layout_h` is the resolved (max-clamped) box height.
        // Flow position uses layout_y (not visual_y) — offset is visual-only.
        y = layout_y.saturating_add(layout_h).saturating_add(spec.margin.bottom);
        prev_margin_bottom = spec.margin.bottom;
    }
}
