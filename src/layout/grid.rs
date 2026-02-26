use crate::node_id::NodeId;
use crate::style::{BoxSizing, KeylineType, Scalar, Style};
use crate::widget_tree::WidgetTree;

use super::common::{get_node_style, resolve_scalar_to_cells};
use super::region::{Region, border_spacing};
use super::resolve_1d::{Edge, layout_resolve_1d};
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
    // Python parity: when keyline is enabled on a grid container, reserve a
    // 1-cell ring around all children so keyline borders don't overwrite cell
    // content. See textual/layouts/grid.py (`size -= (2, 2)`, `offset=(1,1)`).
    let keyline_enabled = parent_style
        .keyline
        .is_some_and(|k| k.keyline_type != KeylineType::None);
    let grid_available = if keyline_enabled && available.width > 2 && available.height > 2 {
        Region::new(
            available.x.saturating_add(1),
            available.y.saturating_add(1),
            available.width.saturating_sub(2),
            available.height.saturating_sub(2),
        )
    } else {
        available
    };

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
    let col_budget = grid_available.width.saturating_sub(total_gutter_v);
    let col_edges = build_grid_track_edges(
        parent_style.grid_columns.as_deref(),
        num_cols,
        grid_available.width,
        viewport.0,
    );
    let col_widths = layout_resolve_1d(col_budget, &col_edges);

    // --- Resolve row heights ---
    let total_gutter_h = if num_rows > 1 {
        (num_rows as u16 - 1).saturating_mul(gutter_h)
    } else {
        0
    };
    let row_budget = grid_available.height.saturating_sub(total_gutter_h);
    let row_edges = build_grid_track_edges(
        parent_style.grid_rows.as_deref(),
        num_rows,
        grid_available.height,
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
        let box_sizing = style.box_sizing.unwrap_or(BoxSizing::BorderBox);

        // Layout rect: cell + available offset, margin inset.
        let layout_x = grid_available
            .x
            .saturating_add(cell_x)
            .saturating_add(margin.left);
        let layout_y = grid_available
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
