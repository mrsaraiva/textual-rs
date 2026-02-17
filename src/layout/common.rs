use crate::node_id::NodeId;
use crate::style::{BoxSizing, Scalar, Spacing, Style, resolve_scalar};
use crate::widget_tree::WidgetTree;

use super::region::border_spacing;
use super::resolve_1d::Edge;

pub(crate) fn get_node_style(tree: &WidgetTree, node: NodeId) -> Style {
    let Some(node_ref) = tree.get(node) else {
        return Style::default();
    };

    // Layout must resolve with full ancestor selector context so combinators
    // like `Horizontal > VerticalScroll` affect width/height distribution.
    let ancestors = tree.ancestors(node);
    let mut pushed = 0usize;
    for ancestor in ancestors.iter().rev() {
        let Some(ancestor_node) = tree.get(*ancestor) else {
            continue;
        };
        let ancestor_meta = crate::css::selector_meta_generic(ancestor_node.widget.as_ref());
        let ancestor_style =
            crate::css::resolve_style(ancestor_node.widget.as_ref(), &ancestor_meta);
        crate::css::push_style_context(ancestor_meta, ancestor_style);
        pushed += 1;
    }

    let meta = crate::css::selector_meta_generic(node_ref.widget.as_ref());
    let resolved = crate::css::resolve_style(node_ref.widget.as_ref(), &meta);

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

pub(crate) fn resolve_scalar_to_cells(scalar: &Scalar, parent_size: u16, viewport_size: u16) -> u16 {
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
        _ => scalar_to_edge(
            style.height.as_ref(),
            parent_height,
            viewport.1,
            min_h_cells,
            v_chrome,
        ),
    };

    // Build width edge for 1D resolver.
    //
    // For `width: auto`, prefer widget intrinsic content width when available.
    // `content_width()` represents natural content width, so only horizontal
    // chrome is added to compute the outer edge size.
    let width_edge = match style.width.as_ref() {
        None | Some(Scalar::Auto) => {
            if let Some(intrinsic) = intrinsic_width {
                let min_size = min_w_cells.saturating_add(h_chrome);
                let auto_size = if box_sizing == BoxSizing::BorderBox {
                    // Border-box width already includes border+padding.
                    intrinsic
                        .saturating_add(margin.left + margin.right)
                        .max(min_size)
                } else {
                    intrinsic.saturating_add(h_chrome).max(min_size)
                };
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
    }
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
