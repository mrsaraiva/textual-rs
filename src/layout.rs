//! Layout solver infrastructure.
//!
//! Ports Python Textual's layout pipeline:
//! - 1D space allocation ([`layout_resolve_1d`])
//! - Vertical stacking ([`layout_vertical`])
//! - Horizontal stacking ([`layout_horizontal`])
//! - Grid layout ([`layout_grid`])
//! - Dock positioning ([`arrange_dock`])
//! - Top-level dispatch ([`resolve_layout`])

use crate::node_id::NodeId;
use crate::style::{
    BoxSizing, Dock, Layout, OffsetValue, Position, Scalar, Spacing, Split, Style, resolve_scalar,
};
use crate::widget_tree::{Rect, WidgetTree};

/// Extract border spacing (top, bottom, left, right) from a style.
fn border_spacing(style: &Style) -> (u16, u16, u16, u16) {
    let top = if style.border_top.is_set() { 1 } else { 0 };
    let right = if style.border_right.is_set() { 1 } else { 0 };
    let bottom = if style.border_bottom.is_set() { 1 } else { 0 };
    let left = if style.border_left.is_set() { 1 } else { 0 };
    (top, bottom, left, right)
}

// ---------------------------------------------------------------------------
// Region
// ---------------------------------------------------------------------------

/// A positioned rectangle in terminal cells (x, y, width, height form).
///
/// Complements [`Rect`] (x0/y0/x1/y1 form) used by `WidgetTree` for storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

impl Region {
    pub const ZERO: Self = Self {
        x: 0,
        y: 0,
        width: 0,
        height: 0,
    };

    pub fn new(x: u16, y: u16, width: u16, height: u16) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Convert to the x0/y0/x1/y1 [`Rect`] used by `WidgetTree`.
    pub(crate) fn to_rect(self) -> Rect {
        Rect {
            x0: self.x,
            y0: self.y,
            x1: self.x.saturating_add(self.width),
            y1: self.y.saturating_add(self.height),
        }
    }
}

// ---------------------------------------------------------------------------
// CarveDir: shared direction for dock and split edge-carving
// ---------------------------------------------------------------------------

/// Direction for edge-carving layout (shared by dock and split).
#[derive(Clone, Copy)]
enum CarveDir {
    Top,
    Right,
    Bottom,
    Left,
}

impl From<Dock> for CarveDir {
    fn from(d: Dock) -> Self {
        match d {
            Dock::Top => CarveDir::Top,
            Dock::Right => CarveDir::Right,
            Dock::Bottom => CarveDir::Bottom,
            Dock::Left => CarveDir::Left,
        }
    }
}

impl From<Split> for CarveDir {
    fn from(s: Split) -> Self {
        match s {
            Split::Top => CarveDir::Top,
            Split::Right => CarveDir::Right,
            Split::Bottom => CarveDir::Bottom,
            Split::Left => CarveDir::Left,
        }
    }
}

// ---------------------------------------------------------------------------
// Edge (1D resolver input)
// ---------------------------------------------------------------------------

/// Edge descriptor for the 1D resolver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Edge {
    /// Fixed size in cells, or `None` for flexible.
    pub size: Option<u16>,
    /// Fraction weight for flexible edges (default 1).
    pub fraction: u16,
    /// Minimum size in cells.
    pub min_size: u16,
}

impl Default for Edge {
    fn default() -> Self {
        Self {
            size: None,
            fraction: 1,
            min_size: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// 1D space allocation
// ---------------------------------------------------------------------------

/// Core 1D space allocation algorithm.
///
/// Divides `total` cells among `edges` according to their size, fraction, and
/// min_size constraints. Port of Python Textual's `_layout_resolve.layout_resolve()`.
///
/// Uses deterministic integer arithmetic — no floating point.
///
/// The returned sizes normally sum to `total`, but may exceed it when minimum
/// constraints force it (e.g. two edges with min_size=20 in 30 cells of space).
pub fn layout_resolve_1d(total: u16, edges: &[Edge]) -> Vec<u16> {
    if edges.is_empty() {
        return Vec::new();
    }

    // Initial sizes: Some(fixed) or None (flexible).
    let mut sizes: Vec<Option<u16>> = edges.iter().map(|e| e.size).collect();

    // Fast path: all edges are fixed.
    if sizes.iter().all(|s| s.is_some()) {
        return sizes.iter().map(|s| s.unwrap()).collect();
    }

    // Collect flexible edges: (original_index, fraction, min_size).
    let mut flexible: Vec<(usize, u16, u16)> = Vec::new();
    for (i, (sz, edge)) in sizes.iter().zip(edges.iter()).enumerate() {
        if sz.is_none() {
            flexible.push((i, edge.fraction.max(1), edge.min_size));
        }
    }

    // Remaining space after fixed edges.
    let fixed_sum: u32 = sizes.iter().map(|s| s.unwrap_or(0) as u32).sum();
    let remaining_signed = total as i32 - fixed_sum as i32;

    if remaining_signed <= 0 {
        // No room for flexible edges — assign min_size (at least 1).
        // Matches Python: `(edge.min_size or 1) if size is None else size`.
        return sizes
            .iter()
            .zip(edges.iter())
            .map(|(sz, edge)| match sz {
                Some(s) => *s,
                None => edge.min_size.max(1),
            })
            .collect();
    }

    let mut remaining = remaining_signed as u64;
    let mut total_fraction: u64 = flexible.iter().map(|&(_, f, _)| f as u64).sum();

    // Iteratively fix edges whose proportional share falls below their min_size.
    loop {
        if flexible.is_empty() || total_fraction == 0 {
            break;
        }

        // Check: for each flexible edge, would `remaining * fraction / total_fraction`
        // be less than its min_size? Equivalent integer test (no division):
        //   remaining * fraction < min_size * total_fraction
        let mut fixed_one = false;
        for idx in 0..flexible.len() {
            let (edge_idx, fraction, min_size) = flexible[idx];
            let lhs = remaining * (fraction as u64);
            let rhs = (min_size as u64) * total_fraction;
            if min_size > 0 && lhs < rhs {
                // Fix this edge at its minimum size.
                sizes[edge_idx] = Some(min_size);
                remaining = remaining.saturating_sub(min_size as u64);
                total_fraction -= fraction as u64;
                flexible.remove(idx);
                fixed_one = true;
                break;
            }
        }

        if !fixed_one {
            // Distribute remaining space with deterministic rounding.
            //
            // Conceptually each edge gets `remaining * fraction / total_fraction`
            // cells. We track a fractional remainder as an integer numerator
            // (denominator = total_fraction) so rounding errors cascade forward
            // instead of accumulating.
            if total_fraction > 0 {
                let mut rem_num: u64 = 0;
                for &(edge_idx, fraction, _) in &flexible {
                    let raw = remaining * fraction as u64 + rem_num;
                    sizes[edge_idx] = Some((raw / total_fraction) as u16);
                    rem_num = raw % total_fraction;
                }
            }
            break;
        }
    }

    sizes.iter().map(|s| s.unwrap_or(0)).collect()
}

// ---------------------------------------------------------------------------
// Node style helpers
// ---------------------------------------------------------------------------

/// Extract the effective style for a tree node.
///
/// Resolves from the CSS stylesheet (if loaded) combined with the widget's
/// inline style.  When no stylesheet is active (unit tests), returns just the
/// inline style.
fn get_node_style(tree: &WidgetTree, node: NodeId) -> Style {
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
struct ChildSpec {
    /// Total height of edge for 1D resolver (content + chrome + margin).
    height_edge: Edge,
    /// Total width of edge for 1D resolver (content + chrome + margin).
    width_edge: Edge,
    margin: Spacing,
    padding: Spacing,
    border_top: u16,
    border_right: u16,
    border_bottom: u16,
    border_left: u16,
    /// Max width in cells (None = unconstrained).
    max_width_cells: Option<u16>,
    /// Max height in cells (None = unconstrained).
    max_height_cells: Option<u16>,
    /// Box-sizing model (P2-25).
    box_sizing: BoxSizing,
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

fn resolve_scalar_to_cells(scalar: &Scalar, parent_size: u16, viewport_size: u16) -> u16 {
    resolve_scalar(scalar, parent_size, viewport_size, 0.0, 0)
}

/// Build a [`ChildSpec`] from a resolved style.
fn extract_child_spec(
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

    let box_sizing = style.box_sizing.unwrap_or(BoxSizing::ContentBox);

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

// ---------------------------------------------------------------------------
// Top-level dispatch
// ---------------------------------------------------------------------------

/// Resolve layout for a subtree rooted at `node`.
///
/// 1. Resolves the node's own style for layout strategy.
/// 2. Separates children into docked vs flow.
/// 3. Calls [`arrange_dock`] for docked children → reduced available region.
/// 4. Dispatches flow children to [`layout_vertical`] / [`layout_horizontal`].
///    Grid dispatches to [`layout_grid`] with parent style for track sizing.
pub fn resolve_layout(
    tree: &mut WidgetTree,
    node: NodeId,
    available: Region,
    viewport: (u16, u16),
) {
    let style = get_node_style(tree, node);
    let strategy = style.layout.unwrap_or(Layout::Vertical);
    let is_dock_parent = tree
        .get(node)
        .map(|n| n.widget.style_type() == "Dock")
        .unwrap_or(false);

    // Collect children (snapshot to avoid borrow conflict).
    let children: Vec<NodeId> = tree.children(node).to_vec();
    if children.is_empty() {
        return;
    }

    // Separate children into categories: split, docked, absolute, and flow.
    let mut split = Vec::new();
    let mut docked = Vec::new();
    let mut absolute = Vec::new();
    let mut flow = Vec::new();
    for &child in &children {
        let child_style = get_node_style(tree, child);
        if child_style.display == Some(crate::style::Display::None) {
            continue;
        }
        if child_style.split.is_some() {
            split.push(child);
        } else if child_style.dock.is_some() {
            docked.push(child);
        } else if child_style.position == Some(Position::Absolute) {
            absolute.push(child);
        } else {
            flow.push(child);
        }
    }

    // Arrange split children first → reduced available region.
    let after_split = if split.is_empty() {
        available
    } else {
        arrange_split(tree, &split, available, viewport)
    };

    // Arrange docked children → further reduced available region.
    let inner = if docked.is_empty() {
        after_split
    } else {
        arrange_dock(tree, &docked, after_split, viewport)
    };

    // Dispatch flow children to the appropriate layout.
    if !flow.is_empty() {
        if is_dock_parent && flow.len() == 1 {
            layout_dock_fill(tree, flow[0], inner);
        } else {
            match strategy {
                Layout::Vertical => {
                    layout_vertical(tree, &flow, inner, viewport);
                }
                Layout::Grid => {
                    layout_grid(tree, &flow, inner, viewport, &style);
                }
                Layout::Horizontal => {
                    layout_horizontal(tree, &flow, inner, viewport);
                }
            }
        }
    }

    // Place absolute children on top of the original available region (P2-24).
    if !absolute.is_empty() {
        layout_absolute(tree, &absolute, available, viewport);
    }

    // Recurse into all laid-out descendants so every node receives
    // layout/content rectangles, not only one level under `node`.
    for child in children {
        let Some(child_node) = tree.get(child) else {
            continue;
        };
        if !child_node.display || child_node.visibility != crate::style::Visibility::Visible {
            continue;
        }
        let rect = child_node.content_rect;
        let w = rect.x1.saturating_sub(rect.x0);
        let h = rect.y1.saturating_sub(rect.y0);
        if w == 0 || h == 0 {
            continue;
        }
        resolve_layout(tree, child, Region::new(rect.x0, rect.y0, w, h), viewport);
    }
}

// ---------------------------------------------------------------------------
// Vertical layout (P2-09)
// ---------------------------------------------------------------------------

/// Lay out children vertically (top-to-bottom stacking).
///
/// Distributes the available height among children using [`layout_resolve_1d`],
/// then assigns each child a horizontal span of `available.width` (minus margins,
/// clamped by min/max width constraints).
pub fn layout_vertical(
    tree: &mut WidgetTree,
    children: &[NodeId],
    available: Region,
    viewport: (u16, u16),
) {
    if children.is_empty() {
        return;
    }

    // Phase 1: collect style specs (immutable borrow of tree).
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
            extract_child_spec(
                &style,
                available.width,
                available.height,
                viewport,
                intrinsic_height,
                intrinsic_width,
            )
        })
        .collect();

    // Phase 2: build edges for height distribution.
    let edges: Vec<Edge> = specs.iter().map(|s| s.height_edge).collect();
    let heights = layout_resolve_1d(available.height, &edges);

    // Phase 3: compute rects and write to tree (mutable borrow).
    let mut y = available.y;
    for (i, &child) in children.iter().enumerate() {
        let spec = &specs[i];
        let total_h = heights[i];

        // Layout rect excludes margin.
        let layout_x = available.x.saturating_add(spec.margin.left);
        let layout_y = y.saturating_add(spec.margin.top);
        let mut layout_w = available
            .width
            .saturating_sub(spec.margin.left + spec.margin.right);
        let layout_h = total_h.saturating_sub(spec.margin.top + spec.margin.bottom);

        // Apply explicit width constraint (P2-25: width_edge.size includes chrome).
        if let Some(edge_w) = spec.width_edge.size {
            let explicit_w = edge_w.saturating_sub(spec.margin.left + spec.margin.right);
            layout_w = layout_w.min(explicit_w);
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
    }
}

// ---------------------------------------------------------------------------
// Horizontal layout (P2-10)
// ---------------------------------------------------------------------------

/// Lay out children horizontally (left-to-right stacking).
///
/// Distributes the available width among children using [`layout_resolve_1d`],
/// then assigns each child a vertical span of `available.height` (minus margins,
/// clamped by min/max height constraints).
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
            extract_child_spec(
                &style,
                available.width,
                available.height,
                viewport,
                intrinsic_height,
                intrinsic_width,
            )
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

// ---------------------------------------------------------------------------
// Grid layout (P2-11)
// ---------------------------------------------------------------------------

/// Convert a grid-track [`Scalar`] to an [`Edge`] for the 1D resolver.
///
/// Grid tracks have no chrome of their own (child chrome is applied separately).
///
/// Note: `Auto` is treated as `1fr` (flexible). True content-based auto-sizing
/// requires content measurement APIs that are not yet available. This matches
/// the default behavior when no scalars are specified.
fn grid_scalar_to_edge(scalar: &Scalar, parent_size: u16, viewport_size: u16) -> Edge {
    match scalar {
        Scalar::Auto => Edge {
            size: None,
            fraction: 1,
            min_size: 0,
        },
        Scalar::Fraction(f) => Edge {
            size: None,
            fraction: f.ceil().max(1.0) as u16,
            min_size: 0,
        },
        _ => {
            // Cells, Percent, ViewWidth, ViewHeight → resolve to fixed cells.
            let cells = resolve_scalar_to_cells(scalar, parent_size, viewport_size);
            Edge {
                size: Some(cells),
                fraction: 1,
                min_size: 0,
            }
        }
    }
}

/// Build [`Edge`] list for grid tracks (columns or rows).
///
/// Cycles the provided scalars to fill `count` tracks. When no scalars are
/// provided, defaults to `1fr` for each track.
fn build_grid_track_edges(
    scalars: Option<&[Scalar]>,
    count: usize,
    parent_size: u16,
    viewport_size: u16,
) -> Vec<Edge> {
    let default_scalar = Scalar::Fraction(1.0);
    (0..count)
        .map(|i| {
            let scalar = match scalars {
                Some(s) if !s.is_empty() => &s[i % s.len()],
                _ => &default_scalar,
            };
            grid_scalar_to_edge(scalar, parent_size, viewport_size)
        })
        .collect()
}

/// Compute the number of grid rows needed when children have column/row spans.
///
/// Simulates placement on a growable occupancy grid and returns the total
/// row count required.
fn compute_rows_with_spans(tree: &WidgetTree, children: &[NodeId], num_cols: usize) -> usize {
    let mut max_row_end = 0usize;
    let mut occupied: Vec<Vec<bool>> = Vec::new();

    for &child in children {
        let style = get_node_style(tree, child);
        let cs = (style.column_span.unwrap_or(1).max(1) as usize).min(num_cols);
        let rs = style.row_span.unwrap_or(1).max(1) as usize;

        let (start_col, start_row) = find_next_grid_slot(&occupied, cs, rs, num_cols);

        // Ensure rows exist in the occupancy grid.
        while occupied.len() < start_row + rs {
            occupied.push(vec![false; num_cols]);
        }

        // Mark occupied cells.
        for r in start_row..start_row + rs {
            for c in start_col..start_col + cs {
                occupied[r][c] = true;
            }
        }

        max_row_end = max_row_end.max(start_row + rs);
    }

    max_row_end.max(1)
}

/// Find the next available (col, row) position that can fit the given span.
fn find_next_grid_slot(
    occupied: &[Vec<bool>],
    col_span: usize,
    row_span: usize,
    num_cols: usize,
) -> (usize, usize) {
    // Guard: if span exceeds columns (or is zero), place at origin to avoid infinite loop.
    if col_span == 0 || row_span == 0 || col_span > num_cols {
        return (0, 0);
    }

    let mut row = 0usize;
    let mut col = 0usize;

    loop {
        if col + col_span > num_cols {
            col = 0;
            row += 1;
            continue;
        }

        let fits = (row..row + row_span).all(|r| {
            if r >= occupied.len() {
                return true; // unallocated row → available
            }
            (col..col + col_span).all(|c| !occupied[r][c])
        });

        if fits {
            return (col, row);
        }

        col += 1;
    }
}

/// Lay out children in a 2D grid.
///
/// Grid tracks (columns and rows) are resolved independently using
/// [`layout_resolve_1d`], then each child is placed into its grid cell
/// with margin/border/padding chrome from the child's style.
///
/// Children are placed left-to-right, top-to-bottom (row-major order).
/// If there are more children than cells, additional rows are created.
/// Supports `row-span` / `column-span` (P2-33) via occupancy-based placement.
///
/// Reference: Python Textual's `layouts/grid.py`.
pub fn layout_grid(
    tree: &mut WidgetTree,
    children: &[NodeId],
    available: Region,
    viewport: (u16, u16),
    parent_style: &Style,
) {
    if children.is_empty() {
        return;
    }

    // --- Grid configuration from parent style ---
    let num_cols = parent_style.grid_size_columns.unwrap_or(1).max(1) as usize;
    let gutter_h = parent_style.grid_gutter_horizontal.unwrap_or(0);
    let gutter_v = parent_style.grid_gutter_vertical.unwrap_or(0);

    // Check whether any child uses spans (enables occupancy-based placement).
    let any_spans = children.iter().any(|&c| {
        let s = get_node_style(tree, c);
        s.column_span.unwrap_or(1) > 1 || s.row_span.unwrap_or(1) > 1
    });

    // Row count: auto-detect from children, ensure enough rows for all.
    let min_rows = (children.len() + num_cols - 1) / num_cols;
    let num_rows = if any_spans {
        let span_rows = compute_rows_with_spans(tree, children, num_cols);
        match parent_style.grid_size_rows {
            Some(r) => span_rows.max(r.max(1) as usize),
            None => span_rows,
        }
    } else {
        match parent_style.grid_size_rows {
            Some(r) => (r.max(1) as usize).max(min_rows),
            None => min_rows,
        }
    };

    // --- Resolve column widths ---
    let total_gutter_v = if num_cols > 1 {
        (num_cols as u16 - 1).saturating_mul(gutter_v)
    } else {
        0
    };
    let col_budget = available.width.saturating_sub(total_gutter_v);
    let col_edges = build_grid_track_edges(
        parent_style.grid_columns.as_deref(),
        num_cols,
        available.width,
        viewport.0,
    );
    let col_widths = layout_resolve_1d(col_budget, &col_edges);

    // --- Resolve row heights ---
    let total_gutter_h = if num_rows > 1 {
        (num_rows as u16 - 1).saturating_mul(gutter_h)
    } else {
        0
    };
    let row_budget = available.height.saturating_sub(total_gutter_h);
    let row_edges = build_grid_track_edges(
        parent_style.grid_rows.as_deref(),
        num_rows,
        available.height,
        viewport.1,
    );
    let row_heights = layout_resolve_1d(row_budget, &row_edges);

    // --- Precompute column x-offsets ---
    let mut col_offsets = Vec::with_capacity(num_cols);
    {
        let mut x: u16 = 0;
        for c in 0..num_cols {
            col_offsets.push(x);
            x = x.saturating_add(col_widths[c]);
            if c + 1 < num_cols {
                x = x.saturating_add(gutter_v);
            }
        }
    }

    // --- Precompute row y-offsets ---
    let mut row_offsets = Vec::with_capacity(num_rows);
    {
        let mut y: u16 = 0;
        for r in 0..num_rows {
            row_offsets.push(y);
            y = y.saturating_add(row_heights[r]);
            if r + 1 < num_rows {
                y = y.saturating_add(gutter_h);
            }
        }
    }

    // --- Place children into cells (occupancy-based when spans exist) ---
    let mut occupied = vec![vec![false; num_cols]; num_rows];
    let mut next_row = 0usize;
    let mut next_col = 0usize;

    for &child in children.iter() {
        let style = get_node_style(tree, child);
        let col_span = (style.column_span.unwrap_or(1).max(1) as usize).min(num_cols);
        let row_span = (style.row_span.unwrap_or(1).max(1) as usize).min(num_rows);

        // Find next available cell for this child.
        let (start_col, start_row) = if any_spans {
            let pos = find_next_grid_slot(&occupied, col_span, row_span, num_cols);
            next_row = pos.1;
            next_col = pos.0;
            pos
        } else {
            let c = next_col;
            let r = next_row;
            next_col += 1;
            if next_col >= num_cols {
                next_col = 0;
                next_row += 1;
            }
            (c, r)
        };

        if start_row >= num_rows {
            break; // No space left in the grid.
        }

        // Mark occupied cells.
        let end_col = (start_col + col_span).min(num_cols);
        let end_row = (start_row + row_span).min(num_rows);
        for r in start_row..end_row {
            for c in start_col..end_col {
                occupied[r][c] = true;
            }
        }

        // Compute spanned cell area (includes inter-span gutters).
        let cell_x = col_offsets[start_col];
        let cell_y = row_offsets[start_row];
        let last_col = end_col - 1;
        let last_row = end_row - 1;
        let cell_w = (col_offsets[last_col] + col_widths[last_col]).saturating_sub(cell_x);
        let cell_h = (row_offsets[last_row] + row_heights[last_row]).saturating_sub(cell_y);

        // Child style for chrome.
        let margin = style.effective_margin();
        let padding = style.effective_padding();
        let (bt, bb, bl, br) = border_spacing(&style);
        let box_sizing = style.box_sizing.unwrap_or(BoxSizing::ContentBox);

        // Layout rect: cell + available offset, margin inset.
        let layout_x = available
            .x
            .saturating_add(cell_x)
            .saturating_add(margin.left);
        let layout_y = available
            .y
            .saturating_add(cell_y)
            .saturating_add(margin.top);
        let mut layout_w = cell_w.saturating_sub(margin.left + margin.right);
        let mut layout_h = cell_h.saturating_sub(margin.top + margin.bottom);

        // Apply max-width constraint.
        if let Some(ref s) = style.max_width {
            let max_w = resolve_scalar_to_cells(s, available.width, viewport.0);
            let max_w_outer = if box_sizing == BoxSizing::BorderBox {
                max_w
            } else {
                max_w.saturating_add(bl + br + padding.left + padding.right)
            };
            layout_w = layout_w.min(max_w_outer);
        }
        // Apply min-width constraint.
        if let Some(ref s) = style.min_width {
            let min_w = resolve_scalar_to_cells(s, available.width, viewport.0);
            let min_w_outer = if box_sizing == BoxSizing::BorderBox {
                min_w
            } else {
                min_w.saturating_add(bl + br + padding.left + padding.right)
            };
            layout_w = layout_w.max(min_w_outer);
        }
        // Apply max-height constraint.
        if let Some(ref s) = style.max_height {
            let max_h = resolve_scalar_to_cells(s, available.height, viewport.1);
            let max_h_outer = if box_sizing == BoxSizing::BorderBox {
                max_h
            } else {
                max_h.saturating_add(bt + bb + padding.top + padding.bottom)
            };
            layout_h = layout_h.min(max_h_outer);
        }
        // Apply min-height constraint.
        if let Some(ref s) = style.min_height {
            let min_h = resolve_scalar_to_cells(s, available.height, viewport.1);
            let min_h_outer = if box_sizing == BoxSizing::BorderBox {
                min_h
            } else {
                min_h.saturating_add(bt + bb + padding.top + padding.bottom)
            };
            layout_h = layout_h.max(min_h_outer);
        }

        // Content rect: inner area after border + padding.
        let content_x = layout_x.saturating_add(bl + padding.left);
        let content_y = layout_y.saturating_add(bt + padding.top);
        let content_w = layout_w.saturating_sub(bl + br + padding.left + padding.right);
        let content_h = layout_h.saturating_sub(bt + bb + padding.top + padding.bottom);

        if let Some(node) = tree.get_mut(child) {
            node.layout_rect = Region::new(layout_x, layout_y, layout_w, layout_h).to_rect();
            node.content_rect = Region::new(content_x, content_y, content_w, content_h).to_rect();
        }
    }
}

// ---------------------------------------------------------------------------
// Edge-carving: shared by dock (P2-12) and split (P2-26)
// ---------------------------------------------------------------------------

/// Position a single edge-carving child and shrink the available bounds.
///
/// Used by both dock and split layout: the child's size is resolved from its
/// style, then it is placed along the specified edge, and the bounds (x0/y0/x1/y1)
/// are reduced by the child's outer size.
fn carve_edge(
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
    let box_sizing = style.box_sizing.unwrap_or(BoxSizing::ContentBox);

    let current_w = x1.saturating_sub(*x0);
    let current_h = y1.saturating_sub(*y0);

    // Resolve child's content size from its style.
    let height_is_explicit = style.height.is_some();
    let width_is_explicit = style.width.is_some();

    let child_h = match style.height.as_ref() {
        Some(s) => resolve_scalar_to_cells(s, current_h, viewport.1),
        None => 1, // auto → 1 row for dock/split children
    };
    let child_w = match style.width.as_ref() {
        Some(s) => resolve_scalar_to_cells(s, current_w, viewport.0),
        None => current_w, // auto → full available width for dock/split children
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
// Dock layout (P2-12)
// ---------------------------------------------------------------------------

/// Position docked children, carving space from the available region.
///
/// Iterates children with a `dock` style (top/bottom/left/right), positions
/// each one, and returns the reduced available [`Region`] for flow children.
///
/// Reference: Python Textual's `_arrange.py` (`_arrange_dock_widgets`).
pub fn arrange_dock(
    tree: &mut WidgetTree,
    docked: &[NodeId],
    available: Region,
    viewport: (u16, u16),
) -> Region {
    let mut x0 = available.x;
    let mut y0 = available.y;
    let mut x1 = available.x.saturating_add(available.width);
    let mut y1 = available.y.saturating_add(available.height);

    for &child in docked {
        let style = get_node_style(tree, child);
        let dock = match style.dock {
            Some(d) => d,
            None => continue,
        };
        carve_edge(
            tree,
            child,
            CarveDir::from(dock),
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
// Split layout (P2-26)
// ---------------------------------------------------------------------------

/// Position split children, carving space from the available region.
///
/// Split is processed before dock in `resolve_layout`. It divides the
/// parent into persistent regions along the specified edge (top/right/bottom/left).
/// Semantically similar to dock but used for screen-level partitioning.
fn arrange_split(
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
fn layout_absolute(
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
        let box_sizing = style.box_sizing.unwrap_or(BoxSizing::ContentBox);

        let chrome_w = bl + br + padding.left + padding.right;
        let chrome_h = bt + bb + padding.top + padding.bottom;

        let height_is_explicit = style.height.is_some();
        let width_is_explicit = style.width.is_some();

        // Resolve width/height (default to full available minus margin).
        let layout_w = match style.width.as_ref() {
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
        let layout_h = match style.height.as_ref() {
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

/// Place a Dock fill child into the remaining region.
///
/// Tree layout must preserve Dock semantics where one non-docked child (fill)
/// consumes all space left after docking.
fn layout_dock_fill(tree: &mut WidgetTree, child: NodeId, inner: Region) {
    let style = get_node_style(tree, child);
    let padding = style.effective_padding();
    let (bt, bb, bl, br) = border_spacing(&style);

    let border_top = bt as u16;
    let border_bottom = bb as u16;
    let border_left = bl as u16;
    let border_right = br as u16;

    let chrome_w = border_left + border_right + padding.left + padding.right;
    let chrome_h = border_top + border_bottom + padding.top + padding.bottom;

    let content_x = inner.x.saturating_add(border_left + padding.left);
    let content_y = inner.y.saturating_add(border_top + padding.top);
    let content_w = inner.width.saturating_sub(chrome_w);
    let content_h = inner.height.saturating_sub(chrome_h);

    if let Some(node) = tree.get_mut(child) {
        node.layout_rect = inner.to_rect();
        node.content_rect = Region::new(content_x, content_y, content_w, content_h).to_rect();
    }
}

// ---------------------------------------------------------------------------
// Layout inspection (public API for testing)
// ---------------------------------------------------------------------------

/// Return the layout and content rects for a tree node.
///
/// Returns `Some((layout: (x0,y0,x1,y1), content: (x0,y0,x1,y1)))`,
/// or `None` if the node doesn't exist.
pub fn inspect_node_rects(
    tree: &WidgetTree,
    node: NodeId,
) -> Option<((u16, u16, u16, u16), (u16, u16, u16, u16))> {
    tree.get(node).map(|n| {
        (
            (
                n.layout_rect.x0,
                n.layout_rect.y0,
                n.layout_rect.x1,
                n.layout_rect.y1,
            ),
            (
                n.content_rect.x0,
                n.content_rect.y0,
                n.content_rect.x1,
                n.content_rect.y1,
            ),
        )
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::{Color, Display, Scalar, Spacing, Style};
    use crate::widget_tree::WidgetTree;
    use crate::widgets::Widget;
    use rich_rs::{Console, ConsoleOptions, Segments};

    // -- Test widget ----------------------------------------------------------

    struct LayoutTestWidget {
        label: &'static str,
        inline_style: Option<Style>,
        intrinsic_height: Option<usize>,
        intrinsic_width: Option<usize>,
    }

    impl LayoutTestWidget {
        fn new(label: &'static str) -> Self {
            Self {
                label,
                inline_style: None,
                intrinsic_height: None,
                intrinsic_width: None,
            }
        }

        fn with_style(mut self, style: Style) -> Self {
            self.inline_style = Some(style);
            self
        }

        fn boxed(label: &'static str) -> Box<dyn Widget> {
            Box::new(Self::new(label))
        }

        fn boxed_with_style(label: &'static str, style: Style) -> Box<dyn Widget> {
            Box::new(Self::new(label).with_style(style))
        }

        fn with_intrinsic_height(mut self, height: usize) -> Self {
            self.intrinsic_height = Some(height.max(1));
            self
        }

        fn with_intrinsic_width(mut self, width: usize) -> Self {
            self.intrinsic_width = Some(width.max(1));
            self
        }

        fn boxed_with_style_and_intrinsic_height(
            label: &'static str,
            style: Style,
            height: usize,
        ) -> Box<dyn Widget> {
            Box::new(
                Self::new(label)
                    .with_style(style)
                    .with_intrinsic_height(height),
            )
        }

        fn boxed_with_style_and_intrinsic_width(
            label: &'static str,
            style: Style,
            width: usize,
        ) -> Box<dyn Widget> {
            Box::new(
                Self::new(label)
                    .with_style(style)
                    .with_intrinsic_width(width),
            )
        }
    }

    impl Widget for LayoutTestWidget {
        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
            Segments::new()
        }

        fn style_type(&self) -> &'static str {
            self.label
        }

        fn style(&self) -> Option<Style> {
            self.inline_style.clone()
        }

        fn layout_height(&self) -> Option<usize> {
            self.intrinsic_height
        }

        fn content_width(&self) -> Option<usize> {
            self.intrinsic_width
        }
    }

    // -- Helpers --------------------------------------------------------------

    fn assert_layout_rect(tree: &WidgetTree, node: NodeId, x0: u16, y0: u16, x1: u16, y1: u16) {
        let n = tree.get(node).unwrap();
        assert_eq!(
            n.layout_rect,
            Rect { x0, y0, x1, y1 },
            "layout_rect mismatch for node"
        );
    }

    fn assert_content_rect(tree: &WidgetTree, node: NodeId, x0: u16, y0: u16, x1: u16, y1: u16) {
        let n = tree.get(node).unwrap();
        assert_eq!(
            n.content_rect,
            Rect { x0, y0, x1, y1 },
            "content_rect mismatch for node"
        );
    }

    // =========================================================================
    // 1D resolver tests
    // =========================================================================

    #[test]
    fn resolve_1d_all_fixed() {
        let edges = vec![
            Edge {
                size: Some(10),
                fraction: 1,
                min_size: 0,
            },
            Edge {
                size: Some(20),
                fraction: 1,
                min_size: 0,
            },
            Edge {
                size: Some(30),
                fraction: 1,
                min_size: 0,
            },
        ];
        assert_eq!(layout_resolve_1d(100, &edges), vec![10, 20, 30]);
    }

    #[test]
    fn resolve_1d_all_flexible_equal() {
        let edges = vec![
            Edge {
                size: None,
                fraction: 1,
                min_size: 0,
            },
            Edge {
                size: None,
                fraction: 1,
                min_size: 0,
            },
            Edge {
                size: None,
                fraction: 1,
                min_size: 0,
            },
        ];
        // 90 / 3 = 30 each
        assert_eq!(layout_resolve_1d(90, &edges), vec![30, 30, 30]);
    }

    #[test]
    fn resolve_1d_all_flexible_weighted() {
        let edges = vec![
            Edge {
                size: None,
                fraction: 1,
                min_size: 0,
            },
            Edge {
                size: None,
                fraction: 2,
                min_size: 0,
            },
            Edge {
                size: None,
                fraction: 3,
                min_size: 0,
            },
        ];
        // total_fraction = 6, 60/6 = 10 per fraction
        assert_eq!(layout_resolve_1d(60, &edges), vec![10, 20, 30]);
    }

    #[test]
    fn resolve_1d_mixed_fixed_and_flexible() {
        let edges = vec![
            Edge {
                size: Some(20),
                fraction: 1,
                min_size: 0,
            },
            Edge {
                size: None,
                fraction: 1,
                min_size: 0,
            },
            Edge {
                size: None,
                fraction: 1,
                min_size: 0,
            },
        ];
        // remaining = 100 - 20 = 80, split equally: 40 each
        assert_eq!(layout_resolve_1d(100, &edges), vec![20, 40, 40]);
    }

    #[test]
    fn resolve_1d_min_size_kicks_in() {
        let edges = vec![
            Edge {
                size: None,
                fraction: 1,
                min_size: 50,
            },
            Edge {
                size: None,
                fraction: 1,
                min_size: 0,
            },
        ];
        // 60 total, equal weight. 60/2 = 30 each, but edge 0 needs min 50.
        // Fix edge 0 at 50, remaining = 10, edge 1 gets 10.
        assert_eq!(layout_resolve_1d(60, &edges), vec![50, 10]);
    }

    #[test]
    fn resolve_1d_zero_total() {
        let edges = vec![
            Edge {
                size: None,
                fraction: 1,
                min_size: 0,
            },
            Edge {
                size: None,
                fraction: 1,
                min_size: 0,
            },
        ];
        // remaining = 0 - 0 = 0, which is <= 0. Flexible edges get max(min_size, 1) = 1.
        assert_eq!(layout_resolve_1d(0, &edges), vec![1, 1]);
    }

    #[test]
    fn resolve_1d_single_edge_flexible() {
        let edges = vec![Edge {
            size: None,
            fraction: 1,
            min_size: 0,
        }];
        assert_eq!(layout_resolve_1d(50, &edges), vec![50]);
    }

    #[test]
    fn resolve_1d_single_edge_fixed() {
        let edges = vec![Edge {
            size: Some(30),
            fraction: 1,
            min_size: 0,
        }];
        assert_eq!(layout_resolve_1d(50, &edges), vec![30]);
    }

    #[test]
    fn resolve_1d_empty() {
        assert_eq!(layout_resolve_1d(100, &[]), Vec::<u16>::new());
    }

    #[test]
    fn resolve_1d_remainder_distribution() {
        // 100 / 3 = 33 remainder 1. The remainder cascades forward.
        let edges = vec![
            Edge {
                size: None,
                fraction: 1,
                min_size: 0,
            },
            Edge {
                size: None,
                fraction: 1,
                min_size: 0,
            },
            Edge {
                size: None,
                fraction: 1,
                min_size: 0,
            },
        ];
        let sizes = layout_resolve_1d(100, &edges);
        // Sum must equal 100.
        assert_eq!(sizes.iter().copied().sum::<u16>(), 100);
        // First two get 33, last gets 34 (remainder cascades).
        assert_eq!(sizes, vec![33, 33, 34]);
    }

    #[test]
    fn resolve_1d_many_edges() {
        let edges: Vec<Edge> = (0..10)
            .map(|_| Edge {
                size: None,
                fraction: 1,
                min_size: 0,
            })
            .collect();
        let sizes = layout_resolve_1d(100, &edges);
        assert_eq!(sizes.iter().copied().sum::<u16>(), 100);
        assert_eq!(sizes.len(), 10);
        // Each gets 10.
        assert!(sizes.iter().all(|&s| s == 10));
    }

    #[test]
    fn resolve_1d_no_room_assigns_min() {
        let edges = vec![
            Edge {
                size: Some(80),
                fraction: 1,
                min_size: 0,
            },
            Edge {
                size: None,
                fraction: 1,
                min_size: 5,
            },
        ];
        // remaining = 100 - 80 = 20, flexible gets 20.
        assert_eq!(layout_resolve_1d(100, &edges), vec![80, 20]);

        // Now with total=80: remaining = 80 - 80 = 0 → flexible gets max(5, 1) = 5.
        assert_eq!(layout_resolve_1d(80, &edges), vec![80, 5]);
    }

    #[test]
    fn resolve_1d_all_min_sizes_forced() {
        let edges = vec![
            Edge {
                size: None,
                fraction: 1,
                min_size: 40,
            },
            Edge {
                size: None,
                fraction: 1,
                min_size: 40,
            },
        ];
        // 50 total, each needs min 40. 50/2=25 < 40, so first fixed at 40.
        // remaining = 10, total_fr=1, 10*1 < 40*1 → second also fixed at 40.
        // Total (80) exceeds available (50), which the docstring allows.
        let sizes = layout_resolve_1d(50, &edges);
        assert_eq!(sizes, vec![40, 40]);
    }

    #[test]
    fn resolve_1d_fraction_weights_with_remainder() {
        let edges = vec![
            Edge {
                size: None,
                fraction: 1,
                min_size: 0,
            },
            Edge {
                size: None,
                fraction: 3,
                min_size: 0,
            },
        ];
        // total=100, total_fraction=4
        // edge 0: 100*1/4 = 25
        // edge 1: 100*3/4 = 75
        assert_eq!(layout_resolve_1d(100, &edges), vec![25, 75]);
    }

    #[test]
    fn resolve_1d_cascading_min_size() {
        // Three flexible edges, but min_size forces sequential fixation.
        let edges = vec![
            Edge {
                size: None,
                fraction: 1,
                min_size: 30,
            },
            Edge {
                size: None,
                fraction: 1,
                min_size: 25,
            },
            Edge {
                size: None,
                fraction: 1,
                min_size: 20,
            },
        ];
        // total=60, 60/3=20. Edge 0 needs 30 → fix at 30.
        // remaining=30, total_fr=2, 30/2=15. Edge 1 needs 25 → fix at 25.
        // remaining=5, total_fr=1, edge 2 gets 5 (but min_size 20 is not triggered
        // because we're in the distribution phase now and 5 >= 20 is false...wait)
        // Actually: remaining=5, fraction=1, 5*1 < 20*1 → fix at 20.
        // remaining = 5 - 20 = underflow → 0, no more flexible.
        // Hmm but remaining is u64, 5 < 20 so saturating_sub gives 0.
        // After fixing edge 2 at 20, flexible is empty, loop breaks.
        let sizes = layout_resolve_1d(60, &edges);
        assert_eq!(sizes, vec![30, 25, 20]);
    }

    // =========================================================================
    // Region tests
    // =========================================================================

    #[test]
    fn region_to_rect() {
        let r = Region::new(5, 10, 20, 15);
        let rect = r.to_rect();
        assert_eq!(
            rect,
            Rect {
                x0: 5,
                y0: 10,
                x1: 25,
                y1: 25
            }
        );
    }

    #[test]
    fn region_zero() {
        assert_eq!(Region::ZERO.to_rect(), Rect::ZERO);
    }

    // =========================================================================
    // Vertical layout tests
    // =========================================================================

    #[test]
    fn vertical_basic_stacking() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let a = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("A", Style::new().height(Scalar::Cells(10))),
        );
        let b = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("B", Style::new().height(Scalar::Cells(20))),
        );

        let available = Region::new(0, 0, 80, 50);
        layout_vertical(&mut tree, &[a, b], available, (80, 50));

        // A: 80x10 at (0,0)
        assert_layout_rect(&tree, a, 0, 0, 80, 10);
        assert_content_rect(&tree, a, 0, 0, 80, 10);

        // B: 80x20 at (0,10)
        assert_layout_rect(&tree, b, 0, 10, 80, 30);
        assert_content_rect(&tree, b, 0, 10, 80, 30);
    }

    #[test]
    fn vertical_fixed_plus_flex() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let fixed = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Fixed", Style::new().height(Scalar::Cells(10))),
        );
        let flex = tree.mount(root, LayoutTestWidget::boxed("Flex"));

        let available = Region::new(0, 0, 80, 50);
        layout_vertical(&mut tree, &[fixed, flex], available, (80, 50));

        assert_layout_rect(&tree, fixed, 0, 0, 80, 10);
        // Flex gets remaining: 50 - 10 = 40.
        assert_layout_rect(&tree, flex, 0, 10, 80, 50);
    }

    #[test]
    fn vertical_auto_height_uses_intrinsic_layout_height() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let a = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style_and_intrinsic_height(
                "A",
                Style::new().height(Scalar::Auto),
                3,
            ),
        );
        let b = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style_and_intrinsic_height(
                "B",
                Style::new().height(Scalar::Auto),
                3,
            ),
        );

        let available = Region::new(0, 0, 80, 20);
        layout_vertical(&mut tree, &[a, b], available, (80, 20));

        assert_layout_rect(&tree, a, 0, 0, 80, 3);
        assert_layout_rect(&tree, b, 0, 3, 80, 6);
    }

    #[test]
    fn vertical_auto_width_uses_intrinsic_content_width() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let a = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style_and_intrinsic_width(
                "A",
                Style::new().width(Scalar::Auto).height(Scalar::Cells(3)),
                12,
            ),
        );
        let b = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("B", Style::new().height(Scalar::Cells(3))),
        );

        let available = Region::new(0, 0, 80, 20);
        layout_vertical(&mut tree, &[a, b], available, (80, 20));

        // Auto-width with intrinsic width should not expand to full parent width.
        assert_layout_rect(&tree, a, 0, 0, 12, 3);
        // Non-auto/flex sibling still uses full width.
        assert_layout_rect(&tree, b, 0, 3, 80, 6);
    }

    #[test]
    fn vertical_with_margin() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let child = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style(
                "Child",
                Style::new()
                    .height(Scalar::Cells(10))
                    .margin(Spacing::new(2, 3, 2, 3)),
            ),
        );

        let available = Region::new(0, 0, 80, 50);
        layout_vertical(&mut tree, &[child], available, (80, 50));

        // Edge total = 10 + 2 + 2 = 14, but height_edge has chrome=4 + Cells(10) = 14 via scalar_to_edge.
        // layout_rect: x=0+3=3, y=0+2=2, w=80-3-3=74, h=14-2-2=10
        assert_layout_rect(&tree, child, 3, 2, 77, 12);
        assert_content_rect(&tree, child, 3, 2, 77, 12);
    }

    #[test]
    fn vertical_with_padding_and_border() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let child = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style(
                "Child",
                Style::new()
                    .height(Scalar::Cells(10))
                    .padding(Spacing::new(1, 2, 1, 2))
                    .border_top(Color::rgb(255, 255, 255))
                    .border_bottom(Color::rgb(255, 255, 255))
                    .border_left(Color::rgb(255, 255, 255))
                    .border_right(Color::rgb(255, 255, 255)),
            ),
        );

        let available = Region::new(0, 0, 80, 50);
        layout_vertical(&mut tree, &[child], available, (80, 50));

        // Chrome: border(1+1+1+1) + padding(1+1+2+2) = 4+6 = 10
        // Edge total height = 10 + 1+1+1+1 = 14 (content + vertical chrome)
        // layout: x=0, y=0, w=80, h=14
        assert_layout_rect(&tree, child, 0, 0, 80, 14);
        // content: x=0+1+2=3, y=0+1+1=2, w=80-1-1-2-2=74, h=14-1-1-1-1=10
        assert_content_rect(&tree, child, 3, 2, 77, 12);
    }

    #[test]
    fn vertical_min_max_constraints() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let child = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Child", {
                let mut s = Style::new();
                s.max_width = Some(Scalar::Cells(40));
                s
            }),
        );

        let available = Region::new(0, 0, 80, 50);
        layout_vertical(&mut tree, &[child], available, (80, 50));

        // Max width 40 should constrain the layout width.
        let n = tree.get(child).unwrap();
        let w = n.layout_rect.x1 - n.layout_rect.x0;
        assert_eq!(w, 40);
    }

    // =========================================================================
    // Horizontal layout tests
    // =========================================================================

    #[test]
    fn horizontal_basic_stacking() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let a = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("A", Style::new().width(Scalar::Cells(20))),
        );
        let b = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("B", Style::new().width(Scalar::Cells(30))),
        );

        let available = Region::new(0, 0, 80, 50);
        layout_horizontal(&mut tree, &[a, b], available, (80, 50));

        // A: 20x50 at (0,0)
        assert_layout_rect(&tree, a, 0, 0, 20, 50);
        // B: 30x50 at (20,0)
        assert_layout_rect(&tree, b, 20, 0, 50, 50);
    }

    #[test]
    fn horizontal_fixed_plus_flex() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let fixed = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Fixed", Style::new().width(Scalar::Cells(20))),
        );
        let flex = tree.mount(root, LayoutTestWidget::boxed("Flex"));

        let available = Region::new(0, 0, 80, 50);
        layout_horizontal(&mut tree, &[fixed, flex], available, (80, 50));

        assert_layout_rect(&tree, fixed, 0, 0, 20, 50);
        // Flex: remaining = 80 - 20 = 60.
        assert_layout_rect(&tree, flex, 20, 0, 80, 50);
    }

    #[test]
    fn horizontal_with_margin() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let child = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style(
                "Child",
                Style::new()
                    .width(Scalar::Cells(20))
                    .margin(Spacing::new(2, 3, 2, 3)),
            ),
        );

        let available = Region::new(0, 0, 80, 50);
        layout_horizontal(&mut tree, &[child], available, (80, 50));

        // Edge total width = 20 + 3 + 3 = 26
        // layout: x=0+3=3, y=0+2=2, w=26-3-3=20, h=50-2-2=46
        assert_layout_rect(&tree, child, 3, 2, 23, 48);
    }

    // =========================================================================
    // Dock tests
    // =========================================================================

    #[test]
    fn dock_top() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let docked = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Header", {
                let mut s = Style::new().height(Scalar::Cells(3));
                s.dock = Some(Dock::Top);
                s
            }),
        );

        let available = Region::new(0, 0, 80, 50);
        let remaining = arrange_dock(&mut tree, &[docked], available, (80, 50));

        // Docked at top: 80x3 at (0,0)
        assert_layout_rect(&tree, docked, 0, 0, 80, 3);
        // Remaining region starts at y=3.
        assert_eq!(remaining, Region::new(0, 3, 80, 47));
    }

    #[test]
    fn dock_bottom() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let docked = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Footer", {
                let mut s = Style::new().height(Scalar::Cells(2));
                s.dock = Some(Dock::Bottom);
                s
            }),
        );

        let available = Region::new(0, 0, 80, 50);
        let remaining = arrange_dock(&mut tree, &[docked], available, (80, 50));

        // Docked at bottom: 80x2 at (0,48)
        assert_layout_rect(&tree, docked, 0, 48, 80, 50);
        assert_eq!(remaining, Region::new(0, 0, 80, 48));
    }

    #[test]
    fn dock_left() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let docked = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Sidebar", {
                let mut s = Style::new().width(Scalar::Cells(20));
                s.dock = Some(Dock::Left);
                s
            }),
        );

        let available = Region::new(0, 0, 80, 50);
        let remaining = arrange_dock(&mut tree, &[docked], available, (80, 50));

        // Docked at left: 20x50 at (0,0)
        assert_layout_rect(&tree, docked, 0, 0, 20, 50);
        assert_eq!(remaining, Region::new(20, 0, 60, 50));
    }

    #[test]
    fn dock_right() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let docked = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Panel", {
                let mut s = Style::new().width(Scalar::Cells(25));
                s.dock = Some(Dock::Right);
                s
            }),
        );

        let available = Region::new(0, 0, 80, 50);
        let remaining = arrange_dock(&mut tree, &[docked], available, (80, 50));

        // Docked at right: 25x50 at (55,0)
        assert_layout_rect(&tree, docked, 55, 0, 80, 50);
        assert_eq!(remaining, Region::new(0, 0, 55, 50));
    }

    #[test]
    fn dock_multiple() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let header = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Header", {
                let mut s = Style::new().height(Scalar::Cells(3));
                s.dock = Some(Dock::Top);
                s
            }),
        );
        let footer = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Footer", {
                let mut s = Style::new().height(Scalar::Cells(2));
                s.dock = Some(Dock::Bottom);
                s
            }),
        );
        let sidebar = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Sidebar", {
                let mut s = Style::new().width(Scalar::Cells(20));
                s.dock = Some(Dock::Left);
                s
            }),
        );

        let available = Region::new(0, 0, 80, 50);
        let remaining = arrange_dock(&mut tree, &[header, footer, sidebar], available, (80, 50));

        // Header: top, full width, 3 tall.
        assert_layout_rect(&tree, header, 0, 0, 80, 3);

        // Footer: bottom, full width, 2 tall. y1 = 50 → footer at y=48.
        assert_layout_rect(&tree, footer, 0, 48, 80, 50);

        // Sidebar: left, after header carved top (y0=3) and footer carved bottom (y1=48).
        // Height = 48 - 3 = 45. Width = 20.
        assert_layout_rect(&tree, sidebar, 0, 3, 20, 48);

        // Remaining: x=20, y=3, w=60, h=45
        assert_eq!(remaining, Region::new(20, 3, 60, 45));
    }

    #[test]
    fn dock_plus_layout_children() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed_with_style("Container", {
            let mut s = Style::new();
            s.layout = Some(Layout::Vertical);
            s
        }));
        let header = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Header", {
                let mut s = Style::new().height(Scalar::Cells(3));
                s.dock = Some(Dock::Top);
                s
            }),
        );
        let body = tree.mount(root, LayoutTestWidget::boxed("Body"));

        let available = Region::new(0, 0, 80, 50);
        resolve_layout(&mut tree, root, available, (80, 50));

        // Header docked at top.
        assert_layout_rect(&tree, header, 0, 0, 80, 3);

        // Body fills remaining space: y=3, h=47.
        assert_layout_rect(&tree, body, 0, 3, 80, 50);
    }

    #[test]
    fn dock_parent_top_plus_fill_uses_remaining_region() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Dock"));
        let header = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Header", {
                let mut s = Style::new().height(Scalar::Cells(3));
                s.dock = Some(Dock::Top);
                s
            }),
        );
        let fill = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Fill", Style::new().height(Scalar::Cells(1))),
        );

        resolve_layout(&mut tree, root, Region::new(0, 0, 80, 50), (80, 50));

        assert_layout_rect(&tree, header, 0, 0, 80, 3);
        assert_layout_rect(&tree, fill, 0, 3, 80, 50);
    }

    // =========================================================================
    // resolve_layout dispatch tests
    // =========================================================================

    #[test]
    fn resolve_layout_vertical_default() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let a = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("A", Style::new().height(Scalar::Cells(10))),
        );
        let b = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("B", Style::new().height(Scalar::Cells(20))),
        );

        resolve_layout(&mut tree, root, Region::new(0, 0, 80, 50), (80, 50));

        assert_layout_rect(&tree, a, 0, 0, 80, 10);
        assert_layout_rect(&tree, b, 0, 10, 80, 30);
    }

    #[test]
    fn resolve_layout_horizontal() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed_with_style("Container", {
            let mut s = Style::new();
            s.layout = Some(Layout::Horizontal);
            s
        }));
        let a = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("A", Style::new().width(Scalar::Cells(30))),
        );
        let b = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("B", Style::new().width(Scalar::Cells(50))),
        );

        resolve_layout(&mut tree, root, Region::new(0, 0, 80, 50), (80, 50));

        assert_layout_rect(&tree, a, 0, 0, 30, 50);
        assert_layout_rect(&tree, b, 30, 0, 80, 50);
    }

    #[test]
    fn resolve_layout_grid_dispatch() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed_with_style("Container", {
            let mut s = Style::new();
            s.layout = Some(Layout::Grid);
            s.grid_size_columns = Some(2);
            s
        }));
        let a = tree.mount(root, LayoutTestWidget::boxed("A"));
        let b = tree.mount(root, LayoutTestWidget::boxed("B"));
        let c = tree.mount(root, LayoutTestWidget::boxed("C"));
        let d = tree.mount(root, LayoutTestWidget::boxed("D"));

        resolve_layout(&mut tree, root, Region::new(0, 0, 80, 50), (80, 50));

        // 2 cols × 2 rows, all 1fr.
        // col_widths = [40, 40], row_heights = [25, 25].
        assert_layout_rect(&tree, a, 0, 0, 40, 25);
        assert_layout_rect(&tree, b, 40, 0, 80, 25);
        assert_layout_rect(&tree, c, 0, 25, 40, 50);
        assert_layout_rect(&tree, d, 40, 25, 80, 50);
    }

    #[test]
    fn resolve_layout_display_none_excluded() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let visible = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Visible", Style::new().height(Scalar::Cells(10))),
        );
        let hidden = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Hidden", {
                let mut s = Style::new().height(Scalar::Cells(10));
                s.display = Some(Display::None);
                s
            }),
        );

        resolve_layout(&mut tree, root, Region::new(0, 0, 80, 50), (80, 50));

        // Visible child gets laid out.
        assert_layout_rect(&tree, visible, 0, 0, 80, 10);
        // Hidden child's rects should remain at ZERO (never touched by layout).
        assert_layout_rect(&tree, hidden, 0, 0, 0, 0);
    }

    #[test]
    fn resolve_layout_no_children_is_noop() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Empty"));
        // Should not panic.
        resolve_layout(&mut tree, root, Region::new(0, 0, 80, 50), (80, 50));
    }

    #[test]
    fn resolve_layout_recurses_into_grandchildren() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Root"));
        let parent = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Parent", Style::new().height(Scalar::Cells(20))),
        );
        let child = tree.mount(
            parent,
            LayoutTestWidget::boxed_with_style("Child", Style::new().height(Scalar::Cells(5))),
        );

        resolve_layout(&mut tree, root, Region::new(0, 0, 80, 50), (80, 50));

        assert_layout_rect(&tree, parent, 0, 0, 80, 20);
        assert_layout_rect(&tree, child, 0, 0, 80, 5);
    }

    // =========================================================================
    // Edge case tests
    // =========================================================================

    #[test]
    fn resolve_1d_large_min_exceeds_total() {
        let edges = vec![
            Edge {
                size: None,
                fraction: 1,
                min_size: 60,
            },
            Edge {
                size: None,
                fraction: 1,
                min_size: 60,
            },
        ];
        // Both need 60, only 50 available. First gets fixed at 60,
        // remaining = 0 (saturating_sub). Second also triggers min_size (0 < 60).
        // Total (120) exceeds available (50), which the docstring allows.
        let sizes = layout_resolve_1d(50, &edges);
        assert_eq!(sizes, vec![60, 60]);
    }

    #[test]
    fn vertical_offset_region() {
        // Layout at a non-zero starting position.
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let a = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("A", Style::new().height(Scalar::Cells(5))),
        );

        let available = Region::new(10, 20, 60, 30);
        layout_vertical(&mut tree, &[a], available, (80, 50));

        // A should be at x=10, y=20.
        assert_layout_rect(&tree, a, 10, 20, 70, 25);
    }

    // =========================================================================
    // Grid layout tests
    // =========================================================================

    #[test]
    fn grid_basic_2x2() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let a = tree.mount(root, LayoutTestWidget::boxed("A"));
        let b = tree.mount(root, LayoutTestWidget::boxed("B"));
        let c = tree.mount(root, LayoutTestWidget::boxed("C"));
        let d = tree.mount(root, LayoutTestWidget::boxed("D"));

        let parent_style = {
            let mut s = Style::new();
            s.grid_size_columns = Some(2);
            s
        };
        let available = Region::new(0, 0, 80, 50);
        layout_grid(&mut tree, &[a, b, c, d], available, (80, 50), &parent_style);

        // 2 cols × 2 rows, all 1fr.
        // col_widths = [40, 40], row_heights = [25, 25].
        assert_layout_rect(&tree, a, 0, 0, 40, 25);
        assert_layout_rect(&tree, b, 40, 0, 80, 25);
        assert_layout_rect(&tree, c, 0, 25, 40, 50);
        assert_layout_rect(&tree, d, 40, 25, 80, 50);
    }

    #[test]
    fn grid_3x2() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let children: Vec<NodeId> = (0..6)
            .map(|_| tree.mount(root, LayoutTestWidget::boxed("Cell")))
            .collect();

        let parent_style = {
            let mut s = Style::new();
            s.grid_size_columns = Some(3);
            s
        };
        let available = Region::new(0, 0, 90, 60);
        layout_grid(&mut tree, &children, available, (90, 60), &parent_style);

        // 3 cols × 2 rows. col_widths = [30, 30, 30], row_heights = [30, 30].
        assert_layout_rect(&tree, children[0], 0, 0, 30, 30);
        assert_layout_rect(&tree, children[1], 30, 0, 60, 30);
        assert_layout_rect(&tree, children[2], 60, 0, 90, 30);
        assert_layout_rect(&tree, children[3], 0, 30, 30, 60);
        assert_layout_rect(&tree, children[4], 30, 30, 60, 60);
        assert_layout_rect(&tree, children[5], 60, 30, 90, 60);
    }

    #[test]
    fn grid_non_uniform_columns() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let a = tree.mount(root, LayoutTestWidget::boxed("A"));
        let b = tree.mount(root, LayoutTestWidget::boxed("B"));

        let parent_style = {
            let mut s = Style::new();
            s.grid_size_columns = Some(2);
            s.grid_columns = Some(vec![Scalar::Fraction(1.0), Scalar::Fraction(2.0)]);
            s
        };
        let available = Region::new(0, 0, 90, 30);
        layout_grid(&mut tree, &[a, b], available, (90, 30), &parent_style);

        // 1fr + 2fr = 3fr. 90/3 = 30 per fr.
        // col_widths = [30, 60].
        assert_layout_rect(&tree, a, 0, 0, 30, 30);
        assert_layout_rect(&tree, b, 30, 0, 90, 30);
    }

    #[test]
    fn grid_with_gutter() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let a = tree.mount(root, LayoutTestWidget::boxed("A"));
        let b = tree.mount(root, LayoutTestWidget::boxed("B"));
        let c = tree.mount(root, LayoutTestWidget::boxed("C"));
        let d = tree.mount(root, LayoutTestWidget::boxed("D"));

        let parent_style = {
            let mut s = Style::new();
            s.grid_size_columns = Some(2);
            s.grid_gutter_horizontal = Some(2);
            s.grid_gutter_vertical = Some(4);
            s
        };
        let available = Region::new(0, 0, 84, 52);
        layout_grid(&mut tree, &[a, b, c, d], available, (84, 52), &parent_style);

        // col_budget = 84 - 4 = 80. col_widths = [40, 40].
        // row_budget = 52 - 2 = 50. row_heights = [25, 25].
        // col_offsets = [0, 44] (40 + 4 gutter).
        // row_offsets = [0, 27] (25 + 2 gutter).
        assert_layout_rect(&tree, a, 0, 0, 40, 25);
        assert_layout_rect(&tree, b, 44, 0, 84, 25);
        assert_layout_rect(&tree, c, 0, 27, 40, 52);
        assert_layout_rect(&tree, d, 44, 27, 84, 52);
    }

    #[test]
    fn grid_overflow_extra_rows() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let children: Vec<NodeId> = (0..4)
            .map(|_| tree.mount(root, LayoutTestWidget::boxed("Cell")))
            .collect();

        let parent_style = {
            let mut s = Style::new();
            s.grid_size_columns = Some(2);
            s.grid_size_rows = Some(1); // only 1 row configured
            s
        };
        let available = Region::new(0, 0, 80, 40);
        layout_grid(&mut tree, &children, available, (80, 40), &parent_style);

        // 4 children, 2 cols, 1 explicit row → actual rows = max(1, 2) = 2.
        // col_widths = [40, 40], row_heights = [20, 20].
        assert_layout_rect(&tree, children[0], 0, 0, 40, 20);
        assert_layout_rect(&tree, children[1], 40, 0, 80, 20);
        assert_layout_rect(&tree, children[2], 0, 20, 40, 40);
        assert_layout_rect(&tree, children[3], 40, 20, 80, 40);
    }

    #[test]
    fn grid_fewer_children_than_cells() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let a = tree.mount(root, LayoutTestWidget::boxed("A"));
        let b = tree.mount(root, LayoutTestWidget::boxed("B"));

        let parent_style = {
            let mut s = Style::new();
            s.grid_size_columns = Some(3);
            s
        };
        let available = Region::new(0, 0, 90, 30);
        layout_grid(&mut tree, &[a, b], available, (90, 30), &parent_style);

        // 2 children in 3-col grid → 1 row. col_widths = [30, 30, 30].
        // A in (0,0), B in (1,0). Third cell empty.
        assert_layout_rect(&tree, a, 0, 0, 30, 30);
        assert_layout_rect(&tree, b, 30, 0, 60, 30);
    }

    #[test]
    fn grid_explicit_rows() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let a = tree.mount(root, LayoutTestWidget::boxed("A"));
        let b = tree.mount(root, LayoutTestWidget::boxed("B"));

        let parent_style = {
            let mut s = Style::new();
            s.grid_size_columns = Some(2);
            s.grid_size_rows = Some(3);
            s
        };
        let available = Region::new(0, 0, 80, 60);
        layout_grid(&mut tree, &[a, b], available, (80, 60), &parent_style);

        // 2 children, 3 explicit rows → 3 rows (even though only 1 needed).
        // row_heights: 60/3 = [20, 20, 20].
        // A in col 0/row 0, B in col 1/row 0.
        assert_layout_rect(&tree, a, 0, 0, 40, 20);
        assert_layout_rect(&tree, b, 40, 0, 80, 20);
    }

    #[test]
    fn grid_default_single_column() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let a = tree.mount(root, LayoutTestWidget::boxed("A"));
        let b = tree.mount(root, LayoutTestWidget::boxed("B"));
        let c = tree.mount(root, LayoutTestWidget::boxed("C"));

        // No grid properties set → default 1 column.
        let parent_style = Style::new();
        let available = Region::new(0, 0, 80, 90);
        layout_grid(&mut tree, &[a, b, c], available, (80, 90), &parent_style);

        // 1 col, 3 rows. col_widths = [80], row_heights = [30, 30, 30].
        assert_layout_rect(&tree, a, 0, 0, 80, 30);
        assert_layout_rect(&tree, b, 0, 30, 80, 60);
        assert_layout_rect(&tree, c, 0, 60, 80, 90);
    }

    #[test]
    fn grid_fixed_column_widths() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let a = tree.mount(root, LayoutTestWidget::boxed("A"));
        let b = tree.mount(root, LayoutTestWidget::boxed("B"));

        let parent_style = {
            let mut s = Style::new();
            s.grid_size_columns = Some(2);
            s.grid_columns = Some(vec![Scalar::Cells(20), Scalar::Cells(30)]);
            s
        };
        let available = Region::new(0, 0, 80, 40);
        layout_grid(&mut tree, &[a, b], available, (80, 40), &parent_style);

        // Fixed columns: 20 + 30 = 50 (leaves 30 unused).
        assert_layout_rect(&tree, a, 0, 0, 20, 40);
        assert_layout_rect(&tree, b, 20, 0, 50, 40);
    }

    #[test]
    fn grid_column_scalar_cycling() {
        // grid-columns: 1fr 2fr applied to a 4-column grid cycles as 1fr 2fr 1fr 2fr.
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let children: Vec<NodeId> = (0..4)
            .map(|_| tree.mount(root, LayoutTestWidget::boxed("Cell")))
            .collect();

        let parent_style = {
            let mut s = Style::new();
            s.grid_size_columns = Some(4);
            s.grid_columns = Some(vec![Scalar::Fraction(1.0), Scalar::Fraction(2.0)]);
            s
        };
        let available = Region::new(0, 0, 60, 20);
        layout_grid(&mut tree, &children, available, (60, 20), &parent_style);

        // 1fr 2fr 1fr 2fr → total 6fr. 60/6 = 10 per fr.
        // col_widths = [10, 20, 10, 20].
        assert_layout_rect(&tree, children[0], 0, 0, 10, 20);
        assert_layout_rect(&tree, children[1], 10, 0, 30, 20);
        assert_layout_rect(&tree, children[2], 30, 0, 40, 20);
        assert_layout_rect(&tree, children[3], 40, 0, 60, 20);
    }

    #[test]
    fn grid_with_child_max_width() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let a = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("A", {
                let mut s = Style::new();
                s.max_width = Some(Scalar::Cells(15));
                s
            }),
        );
        let b = tree.mount(root, LayoutTestWidget::boxed("B"));

        let parent_style = {
            let mut s = Style::new();
            s.grid_size_columns = Some(2);
            s
        };
        let available = Region::new(0, 0, 80, 30);
        layout_grid(&mut tree, &[a, b], available, (80, 30), &parent_style);

        // Cell width = 40, but A has max_width=15.
        let n = tree.get(a).unwrap();
        let w = n.layout_rect.x1 - n.layout_rect.x0;
        assert_eq!(w, 15);

        // B fills its cell normally.
        assert_layout_rect(&tree, b, 40, 0, 80, 30);
    }

    #[test]
    fn grid_empty_children() {
        let mut tree = WidgetTree::new();
        let _root = tree.set_root(LayoutTestWidget::boxed("Container"));

        let parent_style = {
            let mut s = Style::new();
            s.grid_size_columns = Some(2);
            s
        };
        // Should not panic with empty children.
        layout_grid(
            &mut tree,
            &[],
            Region::new(0, 0, 80, 50),
            (80, 50),
            &parent_style,
        );
    }

    #[test]
    fn grid_offset_region() {
        // Grid at a non-zero starting position.
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let a = tree.mount(root, LayoutTestWidget::boxed("A"));
        let b = tree.mount(root, LayoutTestWidget::boxed("B"));

        let parent_style = {
            let mut s = Style::new();
            s.grid_size_columns = Some(2);
            s
        };
        let available = Region::new(10, 20, 60, 40);
        layout_grid(&mut tree, &[a, b], available, (80, 50), &parent_style);

        // col_widths = [30, 30], row_heights = [40].
        // A: x=10+0=10, B: x=10+30=40.
        assert_layout_rect(&tree, a, 10, 20, 40, 60);
        assert_layout_rect(&tree, b, 40, 20, 70, 60);
    }
}
