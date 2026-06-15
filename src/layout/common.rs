use crate::node_id::NodeId;
use crate::style::{BoxSizing, Scalar, Spacing, Style, resolve_scalar};
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
        let inner_inset = (bl as u16) + (br as u16) + padding.left + padding.right;
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
    /// Whether `width` is `auto` (no explicit/percentage/fraction width). Used by
    /// horizontally-scrollable parents to let auto-width children keep their intrinsic
    /// width (exceeding the viewport) instead of being clamped/wrapped to it.
    pub(crate) width_is_auto: bool,
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
    let h = (bl as u16) + (br as u16) + padding.left + padding.right;
    let v = (bt as u16) + (bb as u16) + padding.top + padding.bottom;
    (h, v)
}

pub(crate) fn resolve_scalar_to_cells(
    scalar: &Scalar,
    parent_size: u16,
    viewport_size: u16,
) -> u16 {
    resolve_scalar(scalar, parent_size, viewport_size, 0.0, 0)
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
    let border_top = bt as u16;
    let border_bottom = bb as u16;
    let border_left = bl as u16;
    let border_right = br as u16;

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

    // Resolve min sizes to cells.
    let min_h_cells = style
        .min_height
        .as_ref()
        .map(|s| resolve_scalar_to_cells(s, parent_height, viewport.1))
        .unwrap_or(0);
    let min_w_cells = style
        .min_width
        .as_ref()
        .map(|s| resolve_scalar_to_cells(s, parent_width, viewport.0))
        .unwrap_or(0);

    let max_h_cells = style
        .max_height
        .as_ref()
        .map(|s| resolve_scalar_to_cells(s, parent_height, viewport.1));
    let max_w_cells = style
        .max_width
        .as_ref()
        .map(|s| resolve_scalar_to_cells(s, parent_width, viewport.0));

    // Full horizontal chrome (margin + border + padding), independent of
    // box-sizing. For `width: auto` the intrinsic `content_width()` represents
    // PURE content (post-RA-2: widgets no longer fold their own padding/border
    // into intrinsic width — the layout side owns chrome). So an auto width edge
    // is always `content + full chrome`, regardless of box-sizing (box-sizing
    // only changes how an EXPLICIT width is interpreted, handled by the `_ =>`
    // scalar arm below). Height keeps its existing margin-only behavior:
    // `layout_height()` already accounts for border/padding chrome for the
    // widgets that report it, and changing it regresses bordered grid cells
    // (e.g. five_by_five GameCell).
    let full_h_chrome = horizontal_chrome(&margin, &padding, border_left, border_right);

    // Build height edge for 1D resolver.
    //
    // For `height: auto`, prefer widget intrinsic layout height when available.
    // `layout_height()` represents the widget's natural rendered height
    // (excluding margins), so only margins are added here.
    let height_edge = match style.height.as_ref() {
        None | Some(Scalar::Auto) => {
            if let Some(intrinsic) = intrinsic_height {
                let min_size = min_h_cells.saturating_add(v_chrome);
                let auto_size = intrinsic.saturating_add(margin.top + margin.bottom);
                Edge {
                    size: Some(auto_size.max(min_size)),
                    fraction: 1,
                    min_size,
                }
            } else {
                scalar_to_edge(None, parent_height, viewport.1, min_h_cells, v_chrome)
            }
        }
        // Explicit height. A percentage resolves against the space available
        // AFTER this widget's own vertical margins (Python parity): `height:
        // 100%; margin: 1` in 27 rows is 25 (=27-2), not 27. Margin-free
        // widgets (e.g. five_by_five GameCell) are unaffected.
        _ => scalar_to_edge(
            style.height.as_ref(),
            parent_height.saturating_sub(margin.top + margin.bottom),
            viewport.1,
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
    // a fill leaf like a bare `Static`), so passing `None` here means "no hint →
    // flex-fill", matching Python's `1fr` default for an unset width.
    let width_edge = match style.width.as_ref() {
        None | Some(Scalar::Auto) => {
            if let Some(intrinsic) = intrinsic_width {
                let min_size = min_w_cells.saturating_add(h_chrome);
                let auto_size = intrinsic.saturating_add(full_h_chrome).max(min_size);
                Edge {
                    size: Some(auto_size),
                    fraction: 1,
                    min_size,
                }
            } else {
                scalar_to_edge(None, parent_width, viewport.0, min_w_cells, h_chrome)
            }
        }
        _ => scalar_to_edge(
            style.width.as_ref(),
            parent_width,
            viewport.0,
            min_w_cells,
            h_chrome,
        ),
    };

    let width_is_auto = matches!(style.width.as_ref(), None | Some(Scalar::Auto));

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
        width_is_auto,
    }
}

/// Bottom-up intrinsic measurement for auto-sized containers.
///
/// A container that had its renderable children drained into the arena tree by
/// `take_composed_children` reports `content_width()`/`layout_height()` == None,
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
        return None;
    }
    let style = get_node_style(tree, node);
    let layout = style.layout.unwrap_or(crate::style::Layout::Vertical);

    let mut horizontal_sum: u16 = 0;
    let mut vertical_max: u16 = 0;
    let mut any = false;
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
        horizontal_sum = horizontal_sum.saturating_add(outer);
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
    if let Some(h) = node_ref
        .widget
        .layout_height()
        .and_then(|h| u16::try_from(h).ok())
    {
        return Some(h);
    }
    let children: Vec<NodeId> = tree.children(node).to_vec();
    if children.is_empty() {
        return None;
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
    for (child, child_style) in &displayed {
        // Recursive measurement keeps the existing "arrange at 0" behaviour
        // (pass 0) — only a directly-laid-out auto container gets the real
        // available height from the layout call sites.
        let outer = measure_child_outer_height(tree, *child, child_style, viewport);
        vertical_sum = vertical_sum.saturating_add(outer);
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
        Some(other) => resolve_scalar_to_cells(other, 0, viewport.0),
    };
    let mut outer = content.saturating_add(h_chrome);
    // Respect min/max-width: Textual treats these as outer-size bounds for the
    // common (border-box) widgets that drive auto-container measurement (e.g. a
    // Button's `min-width: 16`). Without the min clamp a narrow label would make
    // its auto-width parent under-size and clip the widget.
    if let Some(min_w) = style.min_width.as_ref() {
        outer = outer.max(resolve_scalar_to_cells(min_w, 0, viewport.0));
    }
    if let Some(max_w) = style.max_width.as_ref() {
        let max = resolve_scalar_to_cells(max_w, 0, viewport.0);
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

    let content = match style.height.as_ref() {
        Some(Scalar::Cells(n)) => {
            if box_sizing == BoxSizing::BorderBox {
                return n.saturating_add(margin.top + margin.bottom);
            }
            *n
        }
        None | Some(Scalar::Auto) | Some(Scalar::Fraction(_)) => {
            measure_intrinsic_content_height(tree, node, viewport, 0).unwrap_or(0)
        }
        Some(other) => resolve_scalar_to_cells(other, 0, viewport.1),
    };
    let mut outer = content.saturating_add(v_chrome);
    if let Some(min_h) = style.min_height.as_ref() {
        outer = outer.max(resolve_scalar_to_cells(min_h, 0, viewport.1));
    }
    if let Some(max_h) = style.max_height.as_ref() {
        let max = resolve_scalar_to_cells(max_h, 0, viewport.1);
        if max > 0 {
            outer = outer.min(max);
        }
    }
    outer
}

/// Convert a CSS [`Scalar`] size into an [`Edge`] for the 1D resolver.
///
/// `chrome` is the total border+padding+margin for this axis.
fn scalar_to_edge(
    scalar: Option<&Scalar>,
    parent_size: u16,
    viewport_size: u16,
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
            // Percent, ViewWidth, ViewHeight — resolve to cells.
            let cells = resolve_scalar_to_cells(scalar, parent_size, viewport_size);
            Edge {
                size: Some(cells.saturating_add(chrome)),
                fraction: 1,
                min_size: min_cells.saturating_add(chrome),
            }
        }
    }
}
