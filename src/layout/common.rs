use crate::node_id::NodeId;
use crate::style::{BoxSizing, Scalar, Spacing, Style, resolve_scalar, resolve_scalar_exact};
use crate::widget_tree::WidgetTree;

use super::region::border_spacing;
use super::resolve_1d::Edge;

/// For a transparent styling wrapper (`Node`, from `.id()`/`.class()`), report
/// whether its wrapped arena child wants auto-sizing on `(width, height)`.
///
/// Python applies `#id`/`.class` styles directly to the target widget, so a
/// wrapper whose own axis is UNSET must inherit that axis's sizing intent from
/// the widget it stands in for: shrink-to-content when the wrapped widget is
/// `auto` (e.g. `Static { height: auto }`), but flex-fill when the wrapped
/// widget defaults to `1fr` (e.g. `Static`/`Widget` width). Returns `(false,
/// false)` for non-wrappers or wrappers with no resolvable child.
pub(crate) fn wrapper_child_auto_axes(tree: &WidgetTree, wrapper: NodeId) -> (bool, bool) {
    let is_wrapper = tree
        .get(wrapper)
        .map(|n| n.widget.is_transparent_wrapper())
        .unwrap_or(false);
    if !is_wrapper {
        return (false, false);
    }
    let children = tree.children(wrapper);
    let Some(&child) = children.first() else {
        return (false, false);
    };
    let child_style = get_node_style(tree, child);
    // Only an explicit `auto` on the child's resolved axis (including via a type
    // default such as `Static { height: auto }` / `Label { width: auto }`) means
    // shrink-to-content. An UNSET axis (None) keeps the leaf's flex-fill default
    // (e.g. `Static`/`Widget` width fills), so the wrapper fills that axis too.
    let width_auto = matches!(child_style.width.as_ref(), Some(Scalar::Auto));
    let height_auto = matches!(child_style.height.as_ref(), Some(Scalar::Auto));
    (width_auto, height_auto)
}

/// Height scalar to assign to a transparent wrapper (`Node`) whose own height is
/// UNSET, mirroring the wrapped child's sizing intent.
///
/// A `Node` is a Rust-only styling pass-through with no Python analogue; it must
/// mirror the sizing intent of the single widget it wraps. Returns:
/// - `Some(Auto)` when the wrapped child is `height: auto` (shrink-to-content),
/// - `None` when the wrapped child's height is itself UNSET (a bare *leaf* such
///   as `Placeholder`): the wrapper inherits the leaf's fill-the-whole-container
///   rule in `extract_child_spec` so each `Node`-wrapped unset leaf independently
///   fills the container and overflows (Python `Widget._get_box_model`), instead
///   of N siblings splitting one track via `1fr` (fixes docs/how-to/layout05),
/// - `Some(Fraction(1.0))` for any explicit non-auto child height (e.g. a `1fr`
///   container): flex-fill, keeping a `Node`-wrapped `1fr` `Horizontal`/`Vertical`
///   sharing the viewport with its siblings (docs_containers04),
/// - `None` for a non-wrapper, leaving a genuine leaf's unset height to the
///   fill-the-container rule.
pub(crate) fn wrapper_unset_height(tree: &WidgetTree, wrapper: NodeId) -> Option<Scalar> {
    let is_wrapper = tree
        .get(wrapper)
        .map(|n| n.widget.is_transparent_wrapper())
        .unwrap_or(false);
    if !is_wrapper {
        return None;
    }
    let children = tree.children(wrapper);
    let &child = children.first()?;
    let child_style = get_node_style(tree, child);
    match child_style.height.as_ref() {
        Some(Scalar::Auto) => Some(Scalar::Auto),
        // An UNSET wrapped-child height is the bare-*leaf* "fill the whole
        // container" intent (e.g. a `Placeholder`, which omits `height` in its
        // DEFAULT_CSS, exactly like Python), NOT a `1fr` share. Returning `None`
        // leaves the wrapper's own height unset so it adopts that same
        // fill-the-container rule in `extract_child_spec` — each `Node`-wrapped
        // unset-height leaf independently receives the full container height and
        // overflows (Python `Widget._get_box_model`), instead of N siblings
        // splitting one track via `1fr`. (Fixes docs/how-to/layout05, where 19
        // `Node`-wrapped `Tweet` placeholders must overflow their column.)
        None => None,
        // Any explicit, non-auto wrapped-child height passes THROUGH verbatim so
        // the wrapper sizes exactly as the widget it stands in for. For a `1fr`
        // container this is `Fraction(1.0)` (flex-fill share — `Node`-wrapped
        // `1fr` containers still split their track, docs_containers04). For an
        // explicit `%`/cells/`w`/`h`/`vw`/`vh` height (e.g. `Node`-wrapped
        // `Placeholder { height: 50% }` carrying a `#pN { min-height: … }` id —
        // Python applies both the explicit height AND the min to the SAME widget,
        // since it has no wrapper), it is that exact scalar — NOT a `1fr` share,
        // which would discard the explicit height and skip the cross-axis
        // min-clamp (`extract_child_spec` only clamps a CONCRETE edge size to its
        // minimum). Fixes docs/examples/styles/min_height.
        Some(&scalar) => Some(scalar),
    }
}

/// For a node whose parent is a transparent styling wrapper (`Node`) and which is
/// that wrapper's sole flow child, report on which axes `(width, height)` the
/// child must FILL the wrapper region.
///
/// A wrapper sized BY the child (its OWN axis is unset, so it adopted the child's
/// `width`/`height` via [`wrapper_unset_height`]/[`wrapper_child_auto_axes`])
/// must have the child fill that axis — otherwise the child re-applies its own
/// explicit size against the already-sized wrapper and shrinks: a
/// `Node("#p3") { min-height: 30 } > Placeholder { height: 50% }` makes the
/// wrapper 30 tall (adopted height + its own min), and the Placeholder must fill
/// all 30 (Python has no wrapper — `height` and `min-height` apply to ONE box).
///
/// But when the wrapper carries its OWN explicit extent on an axis (e.g.
/// `Node("#hello") { height: 9; content-align: … middle } > Static(text)`), the
/// child must KEEP its natural (auto-content) size on that axis so the wrapper's
/// `content-align` can position it — forcing it to fill would defeat the
/// centering (regression guard: `docs_center07`). Returns `(false, false)` for
/// non-wrappers / non-sole children.
pub(crate) fn wrapper_child_fill_axes(tree: &WidgetTree, node: NodeId) -> (bool, bool) {
    let Some(parent) = tree.parent(node) else {
        return (false, false);
    };
    let parent_is_wrapper = tree
        .get(parent)
        .map(|n| n.widget.is_transparent_wrapper())
        .unwrap_or(false);
    if !parent_is_wrapper {
        return (false, false);
    }
    // Sole *flow* child (display-on, not `display: none`); a wrapper conceptually
    // wraps exactly one widget, but guard against stray hidden siblings.
    let flow: Vec<NodeId> = tree
        .children(parent)
        .iter()
        .copied()
        .filter(|&c| {
            tree.get(c).map(|n| n.display).unwrap_or(false)
                && get_node_style(tree, c).display != Some(crate::style::Display::None)
        })
        .collect();
    if flow.as_slice() != [node] {
        return (false, false);
    }
    // Fill an axis only when the wrapper has NO explicit extent of its own on it
    // (so it was sized by adopting the child). An explicit wrapper extent means
    // the wrapper owns the box and the child should keep its natural size for the
    // wrapper's `content-align` to act on.
    let wrapper_style = get_node_style(tree, parent);
    (
        wrapper_style.width.is_none(),
        wrapper_style.height.is_none(),
    )
}

/// Seed width-dependent intrinsic measurement for the children of a transparent
/// styling wrapper (`Node`). The wrapper's own `on_layout` is a no-op, so its
/// drained child never learns the real content width before the layout asks for
/// its intrinsic height — leaving wrapping widgets (Static/Label) measuring at a
/// stale, too-wide width and under-reporting their line count. This walks the
/// wrapper's subtree and calls `on_layout` with the width that each level will
/// actually receive (parent content width minus that child's own chrome).
pub(crate) fn seed_wrapper_subtree_widths(
    tree: &mut WidgetTree,
    wrapper: NodeId,
    content_width: u16,
    content_height: u16,
) {
    let is_wrapper = tree
        .get(wrapper)
        .map(|n| n.widget.is_transparent_wrapper())
        .unwrap_or(false);
    if !is_wrapper {
        return;
    }
    let children: Vec<NodeId> = tree.children(wrapper).to_vec();
    for child in children {
        let child_style = get_node_style(tree, child);
        let margin = child_style.effective_margin();
        let (_, _, bl, br) = border_spacing(&child_style);
        let padding = child_style.effective_padding();
        let outer_inset = margin.left + margin.right;
        let inner_inset = bl + br + padding.left + padding.right;
        let child_outer_w = content_width.saturating_sub(outer_inset).max(1);
        let child_content_w = child_outer_w.saturating_sub(inner_inset).max(1);
        if let Some(node) = tree.get_mut(child) {
            node.widget.on_layout(child_content_w, content_height);
        }
        // Recurse in case of nested wrappers (e.g. `.id().class()`).
        seed_wrapper_subtree_widths(tree, child, child_content_w, content_height);
    }
}

pub(crate) fn get_node_style(tree: &WidgetTree, node: NodeId) -> Style {
    if tree.get(node).is_none() {
        return Style::default();
    }

    // Layout must resolve with full ancestor selector context so combinators
    // like `Horizontal > VerticalScroll` affect width/height distribution.
    // Use node_selector_meta so tree-assigned classes (e.g. tab highlight) are
    // included — matching the resolution path used in render_tree_node.
    let ancestors = tree.ancestors(node);
    let mut pushed = 0usize;
    for &ancestor in ancestors.iter().rev() {
        let ancestor_meta = crate::css::node_selector_meta(tree, ancestor);
        let ancestor_style = crate::css::resolve_node_style(tree, ancestor, &ancestor_meta);
        crate::css::push_style_context(ancestor_meta, ancestor_style);
        pushed += 1;
    }

    let meta = crate::css::node_selector_meta(tree, node);
    let resolved = crate::css::resolve_node_style(tree, node, &meta);

    for _ in 0..pushed {
        crate::css::pop_style_context();
    }

    resolved
}

/// Collected layout-relevant properties for one child, resolved to cells.
pub(crate) struct ChildSpec {
    /// Total height of edge for 1D resolver (content + chrome + margin).
    pub(crate) height_edge: Edge,
    /// Total width of edge for 1D resolver (content + chrome + margin).
    pub(crate) width_edge: Edge,
    pub(crate) margin: Spacing,
    pub(crate) padding: Spacing,
    pub(crate) border_top: u16,
    pub(crate) border_right: u16,
    pub(crate) border_bottom: u16,
    pub(crate) border_left: u16,
    /// Max width in cells (None = unconstrained).
    pub(crate) max_width_cells: Option<u16>,
    /// Max height in cells (None = unconstrained).
    pub(crate) max_height_cells: Option<u16>,
    /// Box-sizing model (P2-25).
    pub(crate) box_sizing: BoxSizing,
    /// EXACT (pre-floor) box height in cells for a simple fixed-scalar height
    /// (`%`/`w`/`h`/`vw`/`vh`/cells, border-box, no border/padding, not min/max
    /// clamped), margin-excluded — same units as the resolver's integer result.
    /// `Some` only when cumulative flooring (Python parity) can be applied safely;
    /// `None` falls back to the integer result. See `resolve_scalar_exact`.
    pub(crate) frac_height: Option<f64>,
    /// EXACT (pre-floor) box width, analogous to `frac_height`.
    pub(crate) frac_width: Option<f64>,
}

/// Vertical chrome = margin.top + border_top + padding.top + padding.bottom + border_bottom + margin.bottom.
fn vertical_chrome(
    margin: &Spacing,
    padding: &Spacing,
    border_top: u16,
    border_bottom: u16,
) -> u16 {
    margin.top + border_top + padding.top + padding.bottom + border_bottom + margin.bottom
}

/// Horizontal chrome = margin.left + border_left + padding.left + padding.right + border_right + margin.right.
fn horizontal_chrome(
    margin: &Spacing,
    padding: &Spacing,
    border_left: u16,
    border_right: u16,
) -> u16 {
    margin.left + border_left + padding.left + padding.right + border_right + margin.right
}

/// A node's OWN box chrome (border + padding, excluding margin), as `(horizontal,
/// vertical)`. Used to make a bottom-up-measured container's intrinsic size
/// chrome-inclusive — `measure_intrinsic_content_*` returns only the children's
/// summed extents (it is called recursively and must not double-count the node's
/// own chrome), so the call site adds the container's own border+padding to match
/// the contract that a widget's reported intrinsic is its content + own chrome.
pub(crate) fn own_box_chrome(style: &Style) -> (u16, u16) {
    let (bt, bb, bl, br) = border_spacing(style);
    let padding = style.effective_padding();
    let h = bl + br + padding.left + padding.right;
    let v = bt + bb + padding.top + padding.bottom;
    (h, v)
}

/// Resolve a scalar to cells against an axis size.
///
/// `parent_size` is the parent extent on the scalar's OWN axis (what `Percent`
/// uses). `parent_width`/`parent_height` are both parent dims, needed by the
/// axis-absolute `w`/`h` (`Scalar::Width`/`Scalar::Height`) units. `viewport` is
/// the FULL viewport `(width, height)`, needed by the axis-absolute `vw`/`vh`
/// (`Scalar::ViewWidth`/`Scalar::ViewHeight`) units which always resolve against
/// a fixed viewport axis regardless of the property they appear on. Most call
/// sites only have one axis dim handy; use [`resolve_scalar_to_cells`] which
/// assumes the scalar's own axis equals `parent_size` for `w`/`h` too (correct
/// when sizing the same axis), or [`resolve_scalar_to_cells_2d`] to pass both.
pub(crate) fn resolve_scalar_to_cells_2d(
    scalar: &Scalar,
    parent_size: u16,
    parent_width: u16,
    parent_height: u16,
    viewport: (u16, u16),
) -> u16 {
    resolve_scalar(
        scalar,
        parent_size,
        parent_width,
        parent_height,
        viewport.0,
        viewport.1,
        0.0,
        0,
    )
}

/// Resolve a scalar to cells when only the scalar's own-axis parent size is
/// known. For `w`/`h` units this approximates the other axis with `parent_size`
/// — acceptable for measurement-only call sites (intrinsic content sizing) where
/// `w`/`h` are rare; the flow layouts use [`resolve_scalar_to_cells_2d`] with the
/// real parent width AND height for correct `w`/`h` resolution. `vw`/`vh` always
/// resolve against the correct viewport axis (the full `viewport` is threaded).
pub(crate) fn resolve_scalar_to_cells(
    scalar: &Scalar,
    parent_size: u16,
    viewport: (u16, u16),
) -> u16 {
    resolve_scalar(
        scalar,
        parent_size,
        parent_size,
        parent_size,
        viewport.0,
        viewport.1,
        0.0,
        0,
    )
}

/// Build a [`ChildSpec`] from a resolved style.
pub(crate) fn extract_child_spec(
    style: &Style,
    parent_width: u16,
    parent_height: u16,
    viewport: (u16, u16),
    intrinsic_height: Option<u16>,
    intrinsic_width: Option<u16>,
) -> ChildSpec {
    let margin = style.effective_margin();
    let padding = style.effective_padding();
    let (bt, bb, bl, br) = border_spacing(style);
    let border_top = bt;
    let border_bottom = bb;
    let border_left = bl;
    let border_right = br;

    let box_sizing = style.box_sizing.unwrap_or(BoxSizing::BorderBox);

    // For border-box, width/height already include padding+border.
    // Edge chrome should only add margin.
    let (v_chrome, h_chrome) = if box_sizing == BoxSizing::BorderBox {
        (margin.top + margin.bottom, margin.left + margin.right)
    } else {
        (
            vertical_chrome(&margin, &padding, border_top, border_bottom),
            horizontal_chrome(&margin, &padding, border_left, border_right),
        )
    };

    // Resolve min/max sizes to cells. Use the 2D form so `w`/`h` units resolve
    // against the correct parent axis (e.g. `min-height: 40w` = 40% of parent
    // WIDTH), while `%`/`cells` keep resolving against the property's own axis.
    let min_h_cells = style
        .min_height
        .as_ref()
        .map(|s| resolve_scalar_to_cells_2d(s, parent_height, parent_width, parent_height, viewport))
        .unwrap_or(0);
    let min_w_cells = style
        .min_width
        .as_ref()
        .map(|s| resolve_scalar_to_cells_2d(s, parent_width, parent_width, parent_height, viewport))
        .unwrap_or(0);

    let max_h_cells = style
        .max_height
        .as_ref()
        .map(|s| resolve_scalar_to_cells_2d(s, parent_height, parent_width, parent_height, viewport));
    let max_w_cells = style
        .max_width
        .as_ref()
        .map(|s| resolve_scalar_to_cells_2d(s, parent_width, parent_width, parent_height, viewport));

    // Margin-adjusted parent dims for `w`/`h` units (Python resolves these
    // against `container - margin.totals` on BOTH axes).
    let parent_width_adj = parent_width.saturating_sub(margin.left + margin.right);
    let parent_height_adj = parent_height.saturating_sub(margin.top + margin.bottom);

    // Full chrome (margin + border + padding) per axis, independent of
    // box-sizing. For an auto/unset edge the intrinsic content size represents
    // PURE content (widgets report `content_width()`/`layout_height()` WITHOUT
    // folding their own padding/border — the layout side owns chrome). So an
    // auto edge is always `content + full chrome`, regardless of box-sizing
    // (box-sizing only changes how an EXPLICIT size is interpreted, handled by
    // the scalar arms below). HEIGHT is now symmetric with WIDTH: the resolved
    // `full_v_chrome` is added HERE (with full ancestor CSS context), instead of
    // relying on each widget's context-free `layout_height()` chrome baking —
    // which could not resolve descendant-selected chrome (`#questions .button`)
    // and collapsed such boxes.
    let full_h_chrome = horizontal_chrome(&margin, &padding, border_left, border_right);
    let full_v_chrome = vertical_chrome(&margin, &padding, border_top, border_bottom);

    // Build height edge for 1D resolver.
    //
    // For `height: auto`, prefer widget intrinsic layout height when available.
    // `layout_height()` represents the widget's natural rendered height
    // (excluding margins), so only margins are added here.
    let mut height_edge = match style.height.as_ref() {
        Some(Scalar::Auto) => {
            if let Some(intrinsic) = intrinsic_height {
                let min_size = min_h_cells.saturating_add(v_chrome);
                let auto_size = intrinsic.saturating_add(full_v_chrome);
                Edge {
                    size: Some(auto_size.max(min_size)),
                    fraction: 1,
                    min_size,
                }
            } else {
                // `height: auto` with no measurable content: flex-fill (existing
                // behavior — distinct from an UNSET height, handled below).
                scalar_to_edge(
                    None,
                    parent_height,
                    parent_width_adj,
                    parent_height_adj,
                    viewport,
                    min_h_cells,
                    v_chrome,
                )
            }
        }
        None => {
            if let Some(intrinsic) = intrinsic_height {
                // A widget that reports an intrinsic height despite an unset CSS
                // height still sizes to its content (preserves auto-content leaves
                // that omit an explicit `height: auto`).
                let min_size = min_h_cells.saturating_add(v_chrome);
                let auto_size = intrinsic.saturating_add(full_v_chrome);
                Edge {
                    size: Some(auto_size.max(min_size)),
                    fraction: 1,
                    min_size,
                }
            } else {
                // Python parity (`Widget._get_box_model`): an UNSET height with no
                // intrinsic content fills the FULL container height
                // (`content_container.height`), it is NOT a `1fr` share. Each
                // unset-height sibling independently receives the container height,
                // so multiple bare unset children (e.g. two `Placeholder`s in a
                // Screen) overflow and scroll rather than splitting the viewport.
                // A single unset child still fills the container (identical to the
                // old flex-fill). Emitting a FIXED edge of the full container
                // height (margin included; the vertical layout subtracts margin
                // from the resolved total) reproduces that — unlike a `1fr` edge,
                // which `layout_resolve_1d` would divide among siblings.
                let min_size = min_h_cells.saturating_add(v_chrome);
                Edge {
                    size: Some(parent_height.max(min_size)),
                    fraction: 1,
                    min_size,
                }
            }
        }
        // Explicit height. A percentage resolves against the space available
        // AFTER this widget's own vertical margins (Python parity): `height:
        // 100%; margin: 1` in 27 rows is 25 (=27-2), not 27. Margin-free
        // widgets (e.g. five_by_five GameCell) are unaffected.
        _ => scalar_to_edge(
            style.height.as_ref(),
            parent_height.saturating_sub(margin.top + margin.bottom),
            parent_width_adj,
            parent_height_adj,
            viewport,
            min_h_cells,
            v_chrome,
        ),
    };

    // Build width edge for 1D resolver.
    //
    // For `width: auto` (and an UNSET width when an `intrinsic_width` hint is
    // supplied by the caller), size to the widget's intrinsic content width.
    // `content_width()` is pure content, so the full horizontal chrome is added
    // to compute the outer edge size (see `full_h_chrome` note above). The arena
    // flow layouts decide WHICH widgets contribute an `intrinsic_width` for the
    // unset case (only `width: auto` widgets and measured auto containers — never
    // a fill leaf like a bare `Static`).
    let mut width_edge = match style.width.as_ref() {
        Some(Scalar::Auto) => {
            if let Some(intrinsic) = intrinsic_width {
                let min_size = min_w_cells.saturating_add(h_chrome);
                let auto_size = intrinsic.saturating_add(full_h_chrome).max(min_size);
                Edge {
                    size: Some(auto_size),
                    fraction: 1,
                    min_size,
                }
            } else {
                // `width: auto` with no measurable content: flex-fill (existing
                // behavior — distinct from an UNSET width, handled below).
                scalar_to_edge(
                    None,
                    parent_width,
                    parent_width_adj,
                    parent_height_adj,
                    viewport,
                    min_w_cells,
                    h_chrome,
                )
            }
        }
        None => {
            if let Some(intrinsic) = intrinsic_width {
                // A widget that reports an intrinsic width despite an unset CSS
                // width still sizes to its content (preserves auto-content leaves
                // that omit an explicit `width: auto`).
                let min_size = min_w_cells.saturating_add(h_chrome);
                let auto_size = intrinsic.saturating_add(full_h_chrome).max(min_size);
                Edge {
                    size: Some(auto_size),
                    fraction: 1,
                    min_size,
                }
            } else {
                // Python parity (`Widget._get_box_model`): an UNSET width with no
                // intrinsic content fills the FULL container width
                // (`content_container.width - margin.width`), it is NOT a `1fr`
                // share. Each unset-width sibling independently receives the
                // container width, so multiple bare unset children in a horizontal
                // row overflow and scroll rather than splitting the viewport.
                // A single unset child still fills the container (identical to the
                // old flex-fill). Emitting a FIXED edge of the full container
                // width (margin included; the horizontal layout subtracts margin
                // from the resolved total) reproduces that — unlike a `1fr` edge,
                // which `layout_resolve_1d` would divide among siblings.
                // Mirrors the unset-HEIGHT arm above.
                let min_size = min_w_cells.saturating_add(h_chrome);
                Edge {
                    size: Some(parent_width.max(min_size)),
                    fraction: 1,
                    min_size,
                }
            }
        }
        // Explicit width. Like the height arm above, a percentage resolves against
        // the space available AFTER this widget's own horizontal margins (Python
        // `Widget._get_box_model`: `styles_width.resolve(container - margin.totals,
        // …)`). So `width: 80%; margin: 1` in an 80-col parent is `80% of 78` (=62),
        // NOT `80% of 80` (=64) — which otherwise centers the box one column early
        // (compound01). Margin-free widths are unaffected (`parent_width_adj ==
        // parent_width`).
        _ => scalar_to_edge(
            style.width.as_ref(),
            parent_width_adj,
            parent_width_adj,
            parent_height_adj,
            viewport,
            min_w_cells,
            h_chrome,
        ),
    };

    // Python parity (`Widget.get_box_model`): for a border-box explicit size,
    // `content = max(0, size - gutter)` and the box is `content + gutter`. So a
    // specified size smaller than the box's own chrome (border + padding) does
    // NOT collapse the box below its chrome — content goes to zero but both
    // borders still render. For border-box the `*_chrome` added by the explicit
    // arm is margin-only, so the edge size = `specified_cells + margin`. Clamp it
    // up so the box never shrinks below `border + padding + margin` (e.g. an
    // `Input { height: 1; border: tall }` keeps its full 2-row box).
    let height_explicit = !matches!(style.height.as_ref(), None | Some(Scalar::Auto));
    if box_sizing == BoxSizing::BorderBox && height_explicit {
        if let Some(size) = height_edge.size.as_mut() {
            let own_chrome = border_top + border_bottom + padding.top + padding.bottom;
            *size = (*size).max(own_chrome.saturating_add(margin.top + margin.bottom));
        }
    }
    let width_explicit = !matches!(style.width.as_ref(), None | Some(Scalar::Auto));
    if box_sizing == BoxSizing::BorderBox && width_explicit {
        if let Some(size) = width_edge.size.as_mut() {
            let own_chrome = border_left + border_right + padding.left + padding.right;
            *size = (*size).max(own_chrome.saturating_add(margin.left + margin.right));
        }
    }

    // Python parity (`Widget._get_box_model`): after resolving an explicit/auto
    // size, the box is grown up to its minimum — `content = max(content, min, 0)`
    // — BEFORE the final box model is emitted, regardless of layout axis. A
    // CONCRETE edge `size` (an explicit width/height, or an auto/unset axis the
    // caller measured to a fixed intrinsic) must therefore never fall below
    // `min-width`/`min-height`.
    //
    // `Edge.min_size` already carries `min_cells + chrome` in the same
    // outer-with-margin units as `Edge.size` (see `scalar_to_edge`), so it is the
    // correct lower bound. Flexible edges (`size == None`) are left untouched:
    // the 1D resolver enforces `min_size` for the MAIN axis, and the cross-axis
    // fill case already receives the full container extent (>= min). Without this
    // clamp a child like `width: 50%; min-width: 60` resolved to 40 cells and
    // ignored its minimum on the cross axis (vertical layout), and likewise
    // `height: 50%; min-height: 30` ignored its minimum in a horizontal row —
    // the consumers (`layout_vertical`/`layout_horizontal`) clamp max but never
    // min. (Max is handled by those consumers, including the flexible case.)
    let height_min = height_edge.min_size;
    if let Some(size) = height_edge.size.as_mut() {
        *size = (*size).max(height_min);
    }
    let width_min = width_edge.min_size;
    if let Some(size) = width_edge.size.as_mut() {
        *size = (*size).max(width_min);
    }

    // Exact (pre-floor) box sizes for cumulative flooring (Python parity, see
    // `resolve_scalar_exact`). Only emitted for a SIMPLE fixed scalar: border-box
    // with no border/padding (so the box equals the resolved scalar with no chrome
    // offset), and only when the resolver-fed integer edge equals `floor(exact)`
    // (i.e. min/max/box-sizing clamps did NOT override it). Otherwise `None` keeps
    // the existing integer behaviour for that axis.
    let no_v_chrome =
        border_top == 0 && border_bottom == 0 && padding.top == 0 && padding.bottom == 0;
    let no_h_chrome =
        border_left == 0 && border_right == 0 && padding.left == 0 && padding.right == 0;
    let frac_height = if box_sizing == BoxSizing::BorderBox && no_v_chrome {
        style.height.as_ref().and_then(|s| {
            resolve_scalar_exact(
                s,
                parent_height_adj,
                parent_width_adj,
                parent_height_adj,
                viewport.0,
                viewport.1,
            )
        })
    } else {
        None
    }
    .filter(|exact| {
        // Box edge (margin-excluded) the resolver will receive equals floor(exact)
        // only when no min/max clamp moved it.
        height_edge
            .size
            .map(|sz| sz.saturating_sub(margin.top + margin.bottom) == exact.floor() as u16)
            .unwrap_or(false)
    });
    let frac_width = if box_sizing == BoxSizing::BorderBox && no_h_chrome {
        style.width.as_ref().and_then(|s| {
            // The integer width resolver (`scalar_to_edge` explicit arm) now
            // resolves a `%` width against the margin-adjusted `parent_width_adj`
            // (matching Python `container - margin.totals`, symmetric with the
            // height path). Mirror that base here so a simple `width: 12.5%`
            // produces an exact value whose floor equals the integer edge (the
            // `.filter()` below would otherwise reject it).
            resolve_scalar_exact(
                s,
                parent_width_adj,
                parent_width_adj,
                parent_height_adj,
                viewport.0,
                viewport.1,
            )
        })
    } else {
        None
    }
    .filter(|exact| {
        width_edge
            .size
            .map(|sz| sz.saturating_sub(margin.left + margin.right) == exact.floor() as u16)
            .unwrap_or(false)
    });

    ChildSpec {
        height_edge,
        width_edge,
        margin,
        padding,
        border_top,
        border_right,
        border_bottom,
        border_left,
        max_width_cells: max_w_cells,
        max_height_cells: max_h_cells,
        box_sizing,
        frac_height,
        frac_width,
    }
}

/// Bottom-up intrinsic measurement for auto-sized containers.
///
/// A container that had its renderable children drained into the arena tree by
/// `compose` reports `content_width()`/`layout_height()` == None,
/// because the widget itself no longer holds content to measure. For
/// EXPLICITLY auto-sized containers this means `extract_child_spec` would treat
/// them as a flex edge (fill) instead of sizing them to their content.
///
/// These helpers reconstruct the intrinsic CONTENT size from the node's arena
/// children, mirroring Python `Widget.get_content_width`/`get_content_height`
/// (which sum/max child outer sizes according to the layout axis). Fractional
/// (`fr`) children contribute their minimum size, not a fill — Python's
/// content-size measurement uses each child's minimum on the flex axis.
///
/// The returned value is PURE CONTENT width/height (chrome added by the caller
/// via `extract_child_spec`'s `full_h_chrome`/margin handling), so it slots
/// directly into the `intrinsic_width`/`intrinsic_height` parameters.
/// Render a childless LEAF widget to measure its natural content size, mirroring
/// Python `Widget.get_content_width`/`get_content_height` (which render the
/// widget's `render()` result and measure the produced lines).
///
/// Used ONLY for a leaf whose widget reports no `auto_content_width`/
/// `auto_content_height` hint AND has no arena children — i.e. a custom widget
/// that draws content directly (e.g. a `Static`-subclass port such as `Counter`/
/// `ColorButton`/`Name`, or a widget rendering a `rich_rs::Table` like
/// `FizzBuzz`). Without this a `width: auto`/`height: auto` leaf with no reported
/// intrinsic flex-fills the whole container instead of shrinking to its content
/// (Python `Widget._get_box_model`: `is_auto_*` → `get_content_*`), which also
/// defeats `align: center middle` (the full-size box has nothing to center).
///
/// Returns `(content_width, content_height)` in cells. `render_width` is the width
/// the widget is rendered at; trailing padding on each rendered line is trimmed so
/// a widget that pads its output to the render width (e.g. `Static`/`Label`) still
/// reports its true content width.
fn measure_rendered_leaf(
    tree: &WidgetTree,
    node: NodeId,
    render_width: u16,
) -> Option<(u16, u16)> {
    let node_ref = tree.get(node)?;
    let console = rich_rs::Console::new();
    let mut opts = rich_rs::ConsoleOptions::default();
    let w = usize::from(render_width.max(1));
    opts.size = (w, 1);
    opts.max_width = w;
    // Do not clip height: a tall auto widget (e.g. a 19-row table) must report
    // ALL its lines, not just the seeded single-row hint.
    opts.max_height = usize::from(u16::MAX);
    let segments = node_ref.widget.render(&console, &opts);
    let lines = rich_rs::Segment::split_lines(segments);
    if lines.is_empty() {
        return None;
    }
    // Drop a single trailing blank line produced by a content-terminating newline
    // (Rust `render()` output often ends with a line break) so the reported height
    // matches the visible line count.
    let mut height = lines.len();
    if height > 1
        && lines
            .last()
            .map(|l| rich_rs::Segment::get_line_length(l) == 0)
            .unwrap_or(false)
    {
        height -= 1;
    }
    // Natural content width = the widest line after trimming trailing fill (a
    // padding widget renders spaces out to `render_width`; those are not content).
    let mut width = 0usize;
    for line in lines.iter().take(height) {
        let text: String = line
            .iter()
            .filter(|s| s.control.is_none())
            .map(|s| s.text.as_ref())
            .collect();
        width = width.max(rich_rs::cell_len(text.trim_end()));
    }
    Some((
        u16::try_from(width).unwrap_or(u16::MAX),
        u16::try_from(height).unwrap_or(u16::MAX),
    ))
}

pub(crate) fn measure_intrinsic_content_width(
    tree: &WidgetTree,
    node: NodeId,
    viewport: (u16, u16),
) -> Option<u16> {
    let node_ref = tree.get(node)?;
    // Prefer the widget's own report when available. Use `auto_content_width()`
    // (which defaults to `content_width()`) so widgets that shrink-to-content
    // under `width: auto` but flex-fill when unset (Label/Static) still measure
    // correctly here without leaking a fill-default content-width hint elsewhere.
    if let Some(w) = node_ref
        .widget
        .auto_content_width()
        .and_then(|w| u16::try_from(w).ok())
    {
        return Some(w);
    }
    let children: Vec<NodeId> = tree.children(node).to_vec();
    if children.is_empty() {
        // Childless leaf with no reported intrinsic: render it to measure its
        // natural content width (Python `get_content_width`). Rendered at the full
        // viewport width so non-wrapping content (e.g. a `Table`) reports its true
        // width; trailing padding is trimmed inside the helper.
        return measure_rendered_leaf(tree, node, viewport.0).map(|(w, _)| w);
    }
    let style = get_node_style(tree, node);
    let layout = style.layout.unwrap_or(crate::style::Layout::Vertical);

    let mut horizontal_sum: u16 = 0;
    let mut vertical_max: u16 = 0;
    let mut any = false;
    // Adjacent horizontal margins COLLAPSE in the horizontal arrange (the gap is
    // `max(prev.right, next.left)`, Python `layouts/horizontal.py`), so the
    // intrinsic width of an auto container must count each interior gap ONCE.
    // `measure_child_outer_width` folds each child's FULL left+right margin into
    // `outer`; subtract the per-pair overlap `min(prev.right, next.left)` so the
    // measured width matches the arranged width (Python `get_content_width`
    // measures via the arrangement itself). Without this an auto `Horizontal`
    // with margined children (e.g. `Greeter > Label { margin: 0 1 }`) measured
    // 1 cell too wide per interior gap and mis-centered under `align: center`.
    let mut prev_margin_right: Option<u16> = None;
    for child in children {
        let Some(child_ref) = tree.get(child) else {
            continue;
        };
        if !child_ref.display {
            continue;
        }
        let child_style = get_node_style(tree, child);
        if child_style.display == Some(crate::style::Display::None) {
            continue;
        }
        let outer = measure_child_outer_width(tree, child, &child_style, viewport);
        any = true;
        let margin = child_style.effective_margin();
        let overlap = prev_margin_right
            .map(|prev_right| prev_right.min(margin.left))
            .unwrap_or(0);
        horizontal_sum = horizontal_sum.saturating_add(outer).saturating_sub(overlap);
        prev_margin_right = Some(margin.right);
        vertical_max = vertical_max.max(outer);
    }
    if !any {
        return None;
    }
    Some(match layout {
        crate::style::Layout::Horizontal => horizontal_sum,
        _ => vertical_max,
    })
}

/// Does the style declare a *dynamic* (non-fixed) height? Mirrors Python
/// `StylesBase.is_dynamic_height`: an EXPLICITLY set height whose unit is one of
/// `auto`/`fr`/`%`. An UNSET height (`None`) or a fixed `cells` height is not
/// dynamic. (Python's set is `{AUTO, FRACTION, PERCENT}` — `vw`/`vh` excluded.)
fn is_dynamic_height(style: &Style) -> bool {
    matches!(
        style.height,
        Some(Scalar::Auto) | Some(Scalar::Fraction(_)) | Some(Scalar::Percent(_))
    )
}

pub(crate) fn measure_intrinsic_content_height(
    tree: &WidgetTree,
    node: NodeId,
    viewport: (u16, u16),
    avail_content_h: u16,
) -> Option<u16> {
    let node_ref = tree.get(node)?;
    // Prefer the widget's own report. Use `auto_content_height()` (which defaults
    // to `layout_height()`) so widgets that shrink-to-content under `height: auto`
    // but flex-fill when the height is UNSET (Placeholder) measure correctly here
    // without leaking a fill-default height hint elsewhere — symmetric with the
    // `auto_content_width()` path in `measure_intrinsic_content_width`.
    if let Some(h) = node_ref
        .widget
        .auto_content_height()
        .and_then(|h| u16::try_from(h).ok())
    {
        return Some(h);
    }
    let children: Vec<NodeId> = tree.children(node).to_vec();
    if children.is_empty() {
        // Childless leaf with no reported intrinsic: render it to measure its
        // natural content height (Python `get_content_height`). Rendered at the
        // full viewport width so a wrapping leaf counts the right number of lines;
        // a non-wrapping leaf (a `Table` or single line) is unaffected.
        return measure_rendered_leaf(tree, node, viewport.0).map(|(_, h)| h);
    }
    let style = get_node_style(tree, node);
    let layout = style.layout.unwrap_or(crate::style::Layout::Vertical);

    // Collect displayed children + their resolved styles once.
    let mut displayed: Vec<(NodeId, Style)> = Vec::new();
    for child in children {
        let Some(child_ref) = tree.get(child) else {
            continue;
        };
        if !child_ref.display {
            continue;
        }
        let child_style = get_node_style(tree, child);
        if child_style.display == Some(crate::style::Display::None) {
            continue;
        }
        displayed.push((child, child_style));
    }
    if displayed.is_empty() {
        return None;
    }

    // Python parity (`Layout.get_content_height`): a non-docked auto container
    // whose displayed children are ALL dynamic-height is arranged against the
    // FULL container height (`Size(width, container.height)`), so `fr` children
    // fill it. When at least one child is `fr`, that arrangement's total height
    // equals the available content height (the `fr` consumes the remaining
    // space). Without this an auto container around a `1fr` child (e.g.
    // `Center > Middle(1fr)`) collapses to the child's minimum and cannot center.
    // Scoped narrowly to the all-dynamic + has-`fr` case to preserve the
    // size-to-content behaviour of every other auto container.
    let all_dynamic = displayed.iter().all(|(_, cs)| is_dynamic_height(cs));
    let any_fraction = displayed
        .iter()
        .any(|(_, cs)| matches!(cs.height, Some(Scalar::Fraction(_))));
    if style.dock.is_none() && all_dynamic && any_fraction && avail_content_h > 0 {
        return Some(avail_content_h);
    }

    let mut vertical_sum: u16 = 0;
    let mut horizontal_max: u16 = 0;
    // Adjacent vertical margins COLLAPSE in the vertical arrange (the gap is
    // `max(prev.bottom, next.top)`, Python `layouts/vertical.py`), so the
    // intrinsic height of an auto container counts each interior gap once —
    // symmetric with `measure_intrinsic_content_width`'s horizontal arm.
    let mut prev_margin_bottom: Option<u16> = None;
    for (child, child_style) in &displayed {
        // Recursive measurement keeps the existing "arrange at 0" behaviour
        // (pass 0) — only a directly-laid-out auto container gets the real
        // available height from the layout call sites.
        let outer = measure_child_outer_height(tree, *child, child_style, viewport);
        let margin = child_style.effective_margin();
        let overlap = prev_margin_bottom
            .map(|prev_bottom| prev_bottom.min(margin.top))
            .unwrap_or(0);
        vertical_sum = vertical_sum.saturating_add(outer).saturating_sub(overlap);
        prev_margin_bottom = Some(margin.bottom);
        horizontal_max = horizontal_max.max(outer);
    }
    Some(match layout {
        crate::style::Layout::Horizontal => horizontal_max,
        _ => vertical_sum,
    })
}

/// Outer (margin+border+padding included) width of a child for intrinsic
/// container measurement. Explicit cell widths use their value; `fr`/`auto`
/// children use their intrinsic content (recursively) — `fr` is treated as its
/// minimum/content contribution, never a fill, per Python's content-size model.
fn measure_child_outer_width(
    tree: &WidgetTree,
    node: NodeId,
    style: &Style,
    viewport: (u16, u16),
) -> u16 {
    let margin = style.effective_margin();
    let (_, _, bl, br) = border_spacing(style);
    let padding = style.effective_padding();
    let box_sizing = style.box_sizing.unwrap_or(BoxSizing::BorderBox);
    let h_chrome = margin.left + margin.right + bl + br + padding.left + padding.right;

    let content = match style.width.as_ref() {
        Some(Scalar::Cells(n)) => {
            // border-box: n already includes border+padding.
            if box_sizing == BoxSizing::BorderBox {
                return n.saturating_add(margin.left + margin.right);
            }
            *n
        }
        None | Some(Scalar::Auto) | Some(Scalar::Fraction(_)) => {
            measure_intrinsic_content_width(tree, node, viewport).unwrap_or(0)
        }
        Some(other) => resolve_scalar_to_cells(other, 0, viewport),
    };
    let mut outer = content.saturating_add(h_chrome);
    // Respect min/max-width: Textual treats these as outer-size bounds for the
    // common (border-box) widgets that drive auto-container measurement (e.g. a
    // Button's `min-width: 16`). Without the min clamp a narrow label would make
    // its auto-width parent under-size and clip the widget.
    if let Some(min_w) = style.min_width.as_ref() {
        outer = outer.max(resolve_scalar_to_cells(min_w, 0, viewport));
    }
    if let Some(max_w) = style.max_width.as_ref() {
        let max = resolve_scalar_to_cells(max_w, 0, viewport);
        if max > 0 {
            outer = outer.min(max);
        }
    }
    outer
}

fn measure_child_outer_height(
    tree: &WidgetTree,
    node: NodeId,
    style: &Style,
    viewport: (u16, u16),
) -> u16 {
    let margin = style.effective_margin();
    let (bt, bb, _, _) = border_spacing(style);
    let padding = style.effective_padding();
    let box_sizing = style.box_sizing.unwrap_or(BoxSizing::BorderBox);
    let v_chrome = margin.top + margin.bottom + bt + bb + padding.top + padding.bottom;

    // `outer` = PURE content height + full vertical chrome. Post-keystone,
    // widgets report PURE content from `layout_height()` (no folded border/
    // padding), symmetric with the width axis, so the layout side owns ALL chrome
    // uniformly:
    //
    // - explicit `cells` / percent: value is pure content → add full v_chrome.
    // - auto/None/fr: `measure_intrinsic_content_height` returns pure content
    //   (the widget's own `layout_height()` when reported, else the summed
    //   children content) → add full v_chrome. (Formerly leaves reported an OUTER
    //   height and this arm added only margin; that asymmetry — and its
    //   descendant-selector chrome miss — is what the keystone retires.)
    let mut outer = match style.height.as_ref() {
        Some(Scalar::Cells(n)) => {
            if box_sizing == BoxSizing::BorderBox {
                return n.saturating_add(margin.top + margin.bottom);
            }
            (*n).saturating_add(v_chrome)
        }
        None | Some(Scalar::Auto) | Some(Scalar::Fraction(_)) => {
            let content =
                measure_intrinsic_content_height(tree, node, viewport, 0).unwrap_or(0);
            content.saturating_add(v_chrome)
        }
        Some(other) => resolve_scalar_to_cells(other, 0, viewport).saturating_add(v_chrome),
    };
    if let Some(min_h) = style.min_height.as_ref() {
        outer = outer.max(resolve_scalar_to_cells(min_h, 0, viewport));
    }
    if let Some(max_h) = style.max_height.as_ref() {
        let max = resolve_scalar_to_cells(max_h, 0, viewport);
        if max > 0 {
            outer = outer.min(max);
        }
    }
    outer
}

/// Convert a CSS [`Scalar`] size into an [`Edge`] for the 1D resolver.
///
/// `parent_size` is the parent extent on the scalar's OWN axis (what `Percent`
/// resolves against). `parent_width`/`parent_height` are both parent dims, used
/// by the axis-absolute `w`/`h` (`Scalar::Width`/`Scalar::Height`) units. All
/// three should be margin-adjusted by the caller to match Python
/// (`container - margin.totals`). `chrome` is the total border+padding+margin
/// for this axis.
#[allow(clippy::too_many_arguments)]
fn scalar_to_edge(
    scalar: Option<&Scalar>,
    parent_size: u16,
    parent_width: u16,
    parent_height: u16,
    viewport: (u16, u16),
    min_cells: u16,
    chrome: u16,
) -> Edge {
    match scalar {
        None | Some(Scalar::Auto) => Edge {
            size: None,
            fraction: 1,
            min_size: min_cells.saturating_add(chrome),
        },
        Some(Scalar::Cells(n)) => Edge {
            size: Some(n.saturating_add(chrome)),
            fraction: 1,
            min_size: min_cells.saturating_add(chrome),
        },
        Some(Scalar::Fraction(f)) => Edge {
            size: None,
            fraction: f.ceil().max(1.0) as u16,
            min_size: min_cells.saturating_add(chrome),
        },
        Some(scalar) => {
            // Percent, Width (`w`), Height (`h`), ViewWidth, ViewHeight.
            let cells = resolve_scalar_to_cells_2d(
                scalar,
                parent_size,
                parent_width,
                parent_height,
                viewport,
            );
            Edge {
                size: Some(cells.saturating_add(chrome)),
                fraction: 1,
                min_size: min_cells.saturating_add(chrome),
            }
        }
    }
}
