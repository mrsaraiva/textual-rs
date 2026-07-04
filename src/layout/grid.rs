use crate::node_id::NodeId;
use crate::style::{BoxSizing, KeylineType, Scalar, Style};
use crate::widget_tree::WidgetTree;

use super::common::{
    get_node_style, measure_intrinsic_content_height, measure_intrinsic_content_width,
    own_box_chrome, resolve_scalar_to_cells,
};
use super::region::{Region, border_spacing};

/// Resolve a child's OUTER size (excluding margin) on one axis within its grid
/// cell, mirroring Python's `widget._get_box_model(cell_size)` (layouts/grid.py):
/// an UNSET dimension (or `1fr`) fills the cell, `auto` sizes to the child's
/// content, and an explicit size resolves against the cell. Python does NOT
/// stretch a child to its cell height unless it asks to (`100%`/`1fr`) — so a
/// `height: auto` Button sits at its natural height, top-aligned, instead of
/// filling a tall grid row.
fn resolve_grid_box_dim(
    scalar: Option<&Scalar>,
    cell_outer: u16,
    margin_total: u16,
    viewport: (u16, u16),
    own_chrome: u16,
    box_sizing: BoxSizing,
    intrinsic_outer: Option<u16>,
) -> u16 {
    let avail = cell_outer.saturating_sub(margin_total);
    match scalar {
        // Unset or fractional → fill the cell (a single grid cell is the whole
        // track, so `1fr` resolves to the full available space).
        None | Some(Scalar::Fraction(_)) => avail,
        Some(Scalar::Auto) => intrinsic_outer.unwrap_or(avail).min(avail),
        Some(s) => {
            let cells = resolve_scalar_to_cells(s, avail, viewport);
            let outer = if box_sizing == BoxSizing::BorderBox {
                cells
            } else {
                cells.saturating_add(own_chrome)
            };
            outer.min(avail)
        }
    }
}
/// A non-negative rational, used to mirror Python's `Fraction` exactly during
/// grid track resolution (`textual/_resolve.py::resolve`). Keeping the offsets
/// in exact rationals before flooring is what makes the cumulative offsets match
/// Python cell-for-cell (e.g. `2fr 1fr 1fr` splits, mixed `1fr 6 25%` rows).
#[derive(Clone, Copy, Debug)]
struct Rat {
    num: i64,
    den: i64,
}

fn gcd(mut a: i64, mut b: i64) -> i64 {
    a = a.abs();
    b = b.abs();
    while b != 0 {
        let t = a % b;
        a = b;
        b = t;
    }
    a.max(1)
}

impl Rat {
    fn new(num: i64, den: i64) -> Self {
        debug_assert!(den != 0);
        let (num, den) = if den < 0 { (-num, -den) } else { (num, den) };
        let g = gcd(num, den);
        Rat {
            num: num / g,
            den: den / g,
        }
    }
    fn whole(n: i64) -> Self {
        Rat { num: n, den: 1 }
    }
    fn zero() -> Self {
        Rat { num: 0, den: 1 }
    }
    fn add(self, o: Rat) -> Rat {
        Rat::new(self.num * o.den + o.num * self.den, self.den * o.den)
    }
    fn sub(self, o: Rat) -> Rat {
        Rat::new(self.num * o.den - o.num * self.den, self.den * o.den)
    }
    fn mul(self, o: Rat) -> Rat {
        Rat::new(self.num * o.num, self.den * o.den)
    }
    fn div(self, o: Rat) -> Rat {
        Rat::new(self.num * o.den, self.den * o.num)
    }
    /// Floor toward negative infinity (matches Python `Fraction.__floor__`).
    fn floor(self) -> i64 {
        if self.num >= 0 {
            self.num / self.den
        } else {
            -((-self.num + self.den - 1) / self.den)
        }
    }
    fn is_positive(self) -> bool {
        self.num > 0
    }
}

/// Resolve a non-fractional grid track scalar to an exact rational size in cells
/// against `size` (the container dimension) / `viewport`. Mirrors
/// `Scalar.resolve`, but `auto`/`fr` are handled by the caller and never reach
/// here.
fn resolve_fixed_scalar(scalar: &Scalar, size: u16, viewport: u16) -> Rat {
    // Percentages keep the exact rational `value * size / 100` (Python does NOT
    // round here — rounding happens once at the cumulative-floor step). `value`
    // is integral in practice but quantize to 1/1000 to be safe.
    let exact = |v: f32, base: u16| -> Rat {
        if v.fract() == 0.0 {
            Rat::new(v as i64 * base as i64, 100)
        } else {
            Rat::new((v as f64 * 1000.0).round() as i64 * base as i64, 100_000)
        }
    };
    match scalar {
        Scalar::Cells(n) => Rat::whole(*n as i64),
        Scalar::Percent(p) => exact(*p, size),
        // `w`/`h` track units are rare; resolve against the track-axis size (the
        // grid track resolver only knows one axis here).
        Scalar::Width(p) | Scalar::Height(p) => exact(*p, size),
        Scalar::ViewWidth(p) | Scalar::ViewHeight(p) => exact(*p, viewport),
        // Auto / Fraction are handled before calling this; treat defensively as 0.
        Scalar::Auto | Scalar::Fraction(_) => Rat::zero(),
    }
}

/// Faithful port of Python Textual `_resolve.resolve()` (no expand/shrink/min):
/// divide `total` cells across `dimensions` honoring `fr` weights (by value),
/// fixed sizes (cells/percent/vw/vh resolved against `size`), with `gutter`
/// between tracks. Returns `(offset, length)` per track, computed via cumulative
/// floor of exact rationals — identical rounding to Python.
fn resolve_tracks(
    dimensions: &[Scalar],
    total: u16,
    gutter: u16,
    size: u16,
    viewport: u16,
) -> Vec<(u16, u16)> {
    let n = dimensions.len();
    if n == 0 {
        return Vec::new();
    }

    // (scalar, Some(resolved fixed) | None for fractional)
    let resolved: Vec<(Scalar, Option<Rat>)> = dimensions
        .iter()
        .map(|s| {
            if matches!(s, Scalar::Fraction(_)) {
                (*s, None)
            } else {
                (*s, Some(resolve_fixed_scalar(s, size, viewport)))
            }
        })
        .collect();

    // Sum of fr `value`s (e.g. 2fr -> 2). Python uses `scalar.value`.
    let total_fraction: Rat = resolved.iter().fold(Rat::zero(), |acc, (s, f)| {
        if f.is_none() {
            if let Scalar::Fraction(v) = s {
                return acc.add(frac_value(*v));
            }
        }
        acc
    });

    let total_gutter = (gutter as i64) * (n as i64 - 1);

    let resolved_fractions: Vec<Rat> = if total_fraction.is_positive() {
        let consumed: Rat = resolved
            .iter()
            .filter_map(|(_, f)| *f)
            .fold(Rat::zero(), |a, f| a.add(f));
        let mut remaining = Rat::whole(total as i64 - total_gutter).sub(consumed);
        if !remaining.is_positive() {
            remaining = Rat::zero();
        }
        let fraction_unit = remaining.div(total_fraction);
        resolved
            .iter()
            .map(|(s, f)| match f {
                Some(fixed) => *fixed,
                None => {
                    if let Scalar::Fraction(v) = s {
                        frac_value(*v).mul(fraction_unit)
                    } else {
                        Rat::zero()
                    }
                }
            })
            .collect()
    } else {
        resolved
            .iter()
            .map(|(_, f)| f.unwrap_or(Rat::zero()))
            .collect()
    };

    // Interleave [frac, gutter, frac, gutter, ...] and accumulate, then floor,
    // matching Python's `accumulate` + `__floor__` per offset.
    let fraction_gutter = Rat::whole(gutter as i64);
    let mut offsets: Vec<i64> = Vec::with_capacity(n * 2 + 1);
    offsets.push(0);
    let mut acc = Rat::zero();
    for frac in &resolved_fractions {
        acc = acc.add(*frac);
        offsets.push(acc.floor());
        acc = acc.add(fraction_gutter);
        offsets.push(acc.floor());
    }

    // results = zip(offsets[::2], offsets[1::2]) -> (offset, length)
    let mut results = Vec::with_capacity(n);
    for i in 0..n {
        let o1 = offsets[i * 2];
        let o2 = offsets[i * 2 + 1];
        let off = o1.max(0) as u16;
        let len = (o2 - o1).max(0) as u16;
        results.push((off, len));
    }
    results
}

/// Convert an `fr` weight (`f32`) to an exact rational, mirroring Python's
/// `Fraction.from_float(scalar.value)`. Grid `fr` weights are integers in
/// practice (`1fr`, `2fr`), so quantize to 1/1000 to stay exact and bounded.
fn frac_value(v: f32) -> Rat {
    if v.fract() == 0.0 {
        Rat::whole(v as i64)
    } else {
        Rat::new((v as f64 * 1000.0).round() as i64, 1000)
    }
}

/// Apply `min-width`/`max-width` limits to an auto-column candidate width
/// (Python `apply_width_limits`). Limits resolve against the container size.
fn apply_width_limits(style: &Style, mut width: u16, size: u16, viewport: (u16, u16)) -> u16 {
    if let Some(ref s) = style.min_width {
        width = width.max(resolve_scalar_to_cells(s, size, viewport));
    }
    if let Some(ref s) = style.max_width {
        width = width.min(resolve_scalar_to_cells(s, size, viewport));
    }
    width
}

/// Apply `min-height`/`max-height` limits to an auto-row candidate height
/// (Python `apply_height_limits`).
fn apply_height_limits(style: &Style, mut height: u16, size: u16, viewport: (u16, u16)) -> u16 {
    if let Some(ref s) = style.min_height {
        height = height.max(resolve_scalar_to_cells(s, size, viewport));
    }
    if let Some(ref s) = style.max_height {
        height = height.min(resolve_scalar_to_cells(s, size, viewport));
    }
    height
}

/// Cycle `scalars` to produce exactly `count` values (Python `repeat_scalars`),
/// defaulting to `default` when none are supplied.
fn repeat_scalars(scalars: Option<&[Scalar]>, count: usize, default: Scalar) -> Vec<Scalar> {
    match scalars {
        Some(s) if !s.is_empty() => (0..count).map(|i| s[i % s.len()]).collect(),
        _ => vec![default; count],
    }
}

/// Compute the number of grid rows needed when children have column/row spans.
///
/// Simulates placement on a growable occupancy grid and returns the total
/// row count required.
#[allow(clippy::needless_range_loop)] // r/c used as 2D grid indices (occupied[r][c])
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
#[allow(clippy::needless_range_loop)] // r/c/row/col used as 2D grid indices
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
            available.x + 1,
            available.y + 1,
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
    let min_rows = children.len().div_ceil(num_cols);
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

    // --- Placement: assign each child to its primary cell (occupancy-based) ---
    // Mirrors Python's `cell_size_map`: per widget -> (start_col, start_row,
    // col_span, row_span). Computed BEFORE track resolution so `auto` tracks can
    // measure the widgets that occupy them.
    struct Placement {
        child: NodeId,
        start_col: usize,
        start_row: usize,
        col_span: usize,
        row_span: usize,
    }
    let mut placements: Vec<Placement> = Vec::with_capacity(children.len());
    // cell_map[(col, row)] -> index into `placements` for the widget whose
    // PRIMARY cell is (col, row); used for auto-track measurement.
    let mut occupied = vec![vec![false; num_cols]; num_rows];
    let mut cell_owner = vec![vec![usize::MAX; num_cols]; num_rows];
    let mut next_row = 0usize;
    let mut next_col = 0usize;

    for &child in children.iter() {
        let style = get_node_style(tree, child);
        let col_span = (style.column_span.unwrap_or(1).max(1) as usize).min(num_cols);
        let row_span = (style.row_span.unwrap_or(1).max(1) as usize).min(num_rows);

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

        let end_col = (start_col + col_span).min(num_cols);
        let end_row = (start_row + row_span).min(num_rows);
        for r in start_row..end_row {
            for c in start_col..end_col {
                occupied[r][c] = true;
            }
        }
        let idx = placements.len();
        cell_owner[start_row][start_col] = idx;
        placements.push(Placement {
            child,
            start_col,
            start_row,
            col_span,
            row_span,
        });
    }

    // --- Resolve column scalars (with auto handling) ---
    let mut column_scalars = repeat_scalars(
        parent_style.grid_columns.as_deref(),
        num_cols,
        Scalar::Fraction(1.0),
    );
    // An `auto` column sizes to the widest content of single-column widgets that
    // start in that column (Python: `get_content_width + gutter.width`, with
    // min/max-width limits). Mirrors grid.py "Handle any auto columns".
    for col in 0..num_cols {
        if !matches!(column_scalars[col], Scalar::Auto) {
            continue;
        }
        let mut width: u16 = 0;
        for row in 0..num_rows {
            let owner = cell_owner[row][col];
            if owner == usize::MAX {
                continue;
            }
            let p = &placements[owner];
            if p.col_span != 1 {
                continue;
            }
            let cstyle = get_node_style(tree, p.child);
            let (own_h_chrome, _) = own_box_chrome(&cstyle);
            let content = measure_intrinsic_content_width(tree, p.child, viewport).unwrap_or(0);
            let mut w = content.saturating_add(own_h_chrome);
            w = apply_width_limits(&cstyle, w, grid_available.width, viewport);
            width = width.max(w);
        }
        column_scalars[col] = Scalar::Cells(width);
    }

    let columns = resolve_tracks(
        &column_scalars,
        grid_available.width,
        gutter_v,
        grid_available.width,
        viewport.0,
    );
    let col_offsets: Vec<u16> = columns.iter().map(|&(o, _)| o).collect();
    let col_widths: Vec<u16> = columns.iter().map(|&(_, l)| l).collect();

    // --- Resolve row scalars (with auto handling) ---
    // Python default: `1fr` rows when the grid has a real height, but `auto`
    // rows when the grid is auto-height (so rows size to their content).
    let row_default = if matches!(parent_style.height, Some(Scalar::Auto)) {
        Scalar::Auto
    } else {
        Scalar::Fraction(1.0)
    };
    let mut row_scalars = repeat_scalars(parent_style.grid_rows.as_deref(), num_rows, row_default);
    // An `auto` row sizes to the tallest content of single-row widgets that start
    // in that row, measured against their resolved column width (Python:
    // `get_content_height(size, viewport, column_width - gutter_width)`).
    for row in 0..num_rows {
        if !matches!(row_scalars[row], Scalar::Auto) {
            continue;
        }
        let mut height: u16 = 0;
        for col in 0..num_cols {
            let owner = cell_owner[row][col];
            if owner == usize::MAX {
                continue;
            }
            let p = &placements[owner];
            if p.row_span != 1 {
                continue;
            }
            let cstyle = get_node_style(tree, p.child);
            let (own_h_chrome, own_v_chrome) = own_box_chrome(&cstyle);
            let avail_content_w = col_widths[col].saturating_sub(own_h_chrome);
            let content =
                measure_intrinsic_content_height(tree, p.child, viewport, avail_content_w)
                    .unwrap_or(0);
            let mut h = content.saturating_add(own_v_chrome);
            h = apply_height_limits(&cstyle, h, grid_available.height, viewport);
            height = height.max(h);
        }
        row_scalars[row] = Scalar::Cells(height);
    }

    let rows = resolve_tracks(
        &row_scalars,
        grid_available.height,
        gutter_h,
        grid_available.height,
        viewport.1,
    );
    let row_offsets: Vec<u16> = rows.iter().map(|&(o, _)| o).collect();
    let row_heights: Vec<u16> = rows.iter().map(|&(_, l)| l).collect();

    // --- Place children into their resolved cell rects ---
    for p in &placements {
        let child = p.child;
        let style = get_node_style(tree, child);
        let start_col = p.start_col;
        let start_row = p.start_row;
        let end_col = (start_col + p.col_span).min(num_cols);
        let end_row = (start_row + p.row_span).min(num_rows);

        // Compute spanned cell area (includes inter-span gutters), mirroring
        // Python: cell_size = (cols[last][0] + cols[last][1] - cols[first][0]).
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

        // Layout rect: cell + available offset, margin inset. Positions are
        // signed (the grid container itself may originate off-viewport).
        let layout_x = grid_available.x + i32::from(cell_x) + i32::from(margin.left);
        let layout_y = grid_available.y + i32::from(cell_y) + i32::from(margin.top);

        // Size the child by its OWN box model within the cell (Python grid:
        // `widget._get_box_model(cell_size)`), not by stretching it to the cell.
        // Only `auto` dimensions need an intrinsic measurement.
        let (own_h_chrome, own_v_chrome) = own_box_chrome(&style);
        let intrinsic_w_outer = if matches!(style.width.as_ref(), Some(Scalar::Auto)) {
            // `content_width()`/`auto_content_width()` and the container fallback
            // both report PURE content width, so add the child's own chrome.
            measure_intrinsic_content_width(tree, child, viewport)
                .map(|w| w.saturating_add(own_h_chrome))
        } else {
            None
        };
        let intrinsic_h_outer = if matches!(style.height.as_ref(), Some(Scalar::Auto)) {
            // Post-keystone `layout_height()` (surfaced via
            // `measure_intrinsic_content_height`) is PURE content on both the leaf
            // and drained-container paths; the grid adds the child's own
            // (context-resolved) vertical chrome to get the OUTER cell height.
            let avail_content_h = cell_h.saturating_sub(own_v_chrome);
            measure_intrinsic_content_height(tree, child, viewport, avail_content_h)
                .map(|h| h.saturating_add(own_v_chrome))
        } else {
            None
        };
        let mut layout_w = resolve_grid_box_dim(
            style.width.as_ref(),
            cell_w,
            margin.left + margin.right,
            viewport,
            own_h_chrome,
            box_sizing,
            intrinsic_w_outer,
        );
        let mut layout_h = resolve_grid_box_dim(
            style.height.as_ref(),
            cell_h,
            margin.top + margin.bottom,
            viewport,
            own_v_chrome,
            box_sizing,
            intrinsic_h_outer,
        );

        // Apply max-width constraint.
        if let Some(ref s) = style.max_width {
            let max_w = resolve_scalar_to_cells(s, available.width, viewport);
            let max_w_outer = if box_sizing == BoxSizing::BorderBox {
                max_w
            } else {
                max_w.saturating_add(bl + br + padding.left + padding.right)
            };
            layout_w = layout_w.min(max_w_outer);
        }
        // Apply min-width constraint.
        if let Some(ref s) = style.min_width {
            let min_w = resolve_scalar_to_cells(s, available.width, viewport);
            let min_w_outer = if box_sizing == BoxSizing::BorderBox {
                min_w
            } else {
                min_w.saturating_add(bl + br + padding.left + padding.right)
            };
            layout_w = layout_w.max(min_w_outer);
        }
        // Apply max-height constraint.
        if let Some(ref s) = style.max_height {
            let max_h = resolve_scalar_to_cells(s, available.height, viewport);
            let max_h_outer = if box_sizing == BoxSizing::BorderBox {
                max_h
            } else {
                max_h.saturating_add(bt + bb + padding.top + padding.bottom)
            };
            layout_h = layout_h.min(max_h_outer);
        }
        // Apply min-height constraint.
        if let Some(ref s) = style.min_height {
            let min_h = resolve_scalar_to_cells(s, available.height, viewport);
            let min_h_outer = if box_sizing == BoxSizing::BorderBox {
                min_h
            } else {
                min_h.saturating_add(bt + bb + padding.top + padding.bottom)
            };
            layout_h = layout_h.max(min_h_outer);
        }

        // Content rect: inner area after border + padding.
        let content_x = layout_x + i32::from(bl + padding.left);
        let content_y = layout_y + i32::from(bt + padding.top);
        let content_w = layout_w.saturating_sub(bl + br + padding.left + padding.right);
        let content_h = layout_h.saturating_sub(bt + bb + padding.top + padding.bottom);

        if let Some(node) = tree.get_mut(child) {
            node.layout_rect = Region::new(layout_x, layout_y, layout_w, layout_h).to_rect();
            node.content_rect = Region::new(content_x, content_y, content_w, content_h).to_rect();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: spanned cell extent across resolved tracks, mirroring the grid
    /// placement math (`offset(last) + len(last) - offset(first)`).
    fn span_extent(tracks: &[(u16, u16)], start: usize, span: usize) -> (u16, u16) {
        let last = (start + span - 1).min(tracks.len() - 1);
        let off = tracks[start].0;
        let len = (tracks[last].0 + tracks[last].1).saturating_sub(off);
        (off, len)
    }

    #[test]
    fn fr_tracks_split_by_weight() {
        // `2fr 1fr 1fr` over 120 cells, no gutter → 60 / 30 / 30.
        let scalars = [Scalar::Fraction(2.0), Scalar::Fraction(1.0), Scalar::Fraction(1.0)];
        let tracks = resolve_tracks(&scalars, 120, 0, 120, 80);
        assert_eq!(tracks, vec![(0, 60), (60, 30), (90, 30)]);
    }

    #[test]
    fn percent_rows_use_cumulative_floor() {
        // `25% 75%` over 30 cells → floor(7.5)=7 then 30-7=23 (Python parity:
        // cumulative floor, NOT independent rounding which would give 8 + 23 = 31).
        let scalars = [Scalar::Percent(25.0), Scalar::Percent(75.0)];
        let tracks = resolve_tracks(&scalars, 30, 0, 30, 24);
        assert_eq!(tracks, vec![(0, 7), (7, 23)]);
    }

    #[test]
    fn mixed_fr_fixed_percent_rows() {
        // `grid-rows: 1fr 6 25%` cycled over 5 rows on a 40-cell column, no gutter.
        // Fixed consume: 6 + floor-of-percent contributions; fr fills the rest.
        // Verify the fixed (`6`) and percent (`25%` of 40 = 10) tracks are exact
        // and the fr tracks absorb the remainder.
        let scalars = repeat_scalars(
            Some(&[Scalar::Fraction(1.0), Scalar::Cells(6), Scalar::Percent(25.0)]),
            5,
            Scalar::Fraction(1.0),
        );
        // rows: 1fr, 6, 25%, 1fr, 6
        let tracks = resolve_tracks(&scalars, 40, 0, 40, 24);
        // Two `6` fixed tracks + one 25% (=10) track = 22 consumed; remaining 18
        // split across two 1fr → 9 each.
        assert_eq!(tracks[1].1, 6);
        assert_eq!(tracks[4].1, 6);
        assert_eq!(tracks[2].1, 10);
        assert_eq!(tracks[0].1, 9);
        assert_eq!(tracks[3].1, 9);
        // Tracks tile without gaps/overlap.
        let total: u16 = tracks.iter().map(|&(_, l)| l).sum();
        assert_eq!(total, 40);
    }

    #[test]
    fn gutter_is_baked_into_offsets() {
        // Two `1fr` over 20 cells with a 2-cell gutter → 9 / 9 with a 2 gap.
        let scalars = [Scalar::Fraction(1.0), Scalar::Fraction(1.0)];
        let tracks = resolve_tracks(&scalars, 20, 2, 20, 24);
        assert_eq!(tracks, vec![(0, 9), (11, 9)]);
    }

    #[test]
    fn column_span_unions_tracks_and_gutters() {
        // 4 columns of `1fr` over 46 cells with a 2-cell gutter between them:
        // total_gutter = 6, remaining = 40, fraction_unit = 10 → each track 10
        // wide at offsets 0,12,24,36. A column-span of 2 from column 0 must cover
        // the two tracks AND the gutter between them: 22 wide.
        let scalars = vec![Scalar::Fraction(1.0); 4];
        let tracks = resolve_tracks(&scalars, 46, 2, 46, 24);
        assert_eq!(tracks, vec![(0, 10), (12, 10), (24, 10), (36, 10)]);
        // span 2 from col 0 → covers cols 0..=1 incl. gutter: 0..(12+10)=22.
        assert_eq!(span_extent(&tracks, 0, 2), (0, 22));
        // span 3 from col 1 → cols 1..=3: offset 12, len (36+10)-12 = 34.
        assert_eq!(span_extent(&tracks, 1, 3), (12, 34));
    }

    #[test]
    fn auto_resolved_to_cells_passes_through() {
        // After auto-track measurement, an `auto` column becomes `Cells(n)`; the
        // resolver must treat it as a fixed track that does not absorb fr space.
        // `auto(=14) 1fr 1fr` over 120, no gutter → 14 / 53 / 53.
        let scalars = [Scalar::Cells(14), Scalar::Fraction(1.0), Scalar::Fraction(1.0)];
        let tracks = resolve_tracks(&scalars, 120, 0, 120, 24);
        assert_eq!(tracks, vec![(0, 14), (14, 53), (67, 53)]);
    }
}
