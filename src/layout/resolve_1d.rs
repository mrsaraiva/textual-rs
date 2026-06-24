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

/// Resolve 1D sizes EXACTLY (Python `_resolve.resolve` + `layouts/*.py`): every
/// edge is sized to a `f64` (the analogue of Python's `Fraction`), then the
/// running position is floored CUMULATIVELY so the integer sizes fence-post like
/// Python (`next.__floor__() - cur.__floor__()`) instead of each edge truncating
/// independently.
///
/// `fixed_exact[i]` is the pre-floor box size for a SIMPLE fixed-scalar edge
/// (`%`/`w`/`h`/`vw`/`vh`/cells; see `resolve_scalar_exact`); `None` falls back
/// to the edge's integer `size`. Flexible (`size: None`) edges receive their
/// EXACT share of the remaining space: `fraction * (remaining_exact /
/// total_fraction)`, where `remaining_exact` is `total` minus the exact fixed
/// sizes (and minus any flexible edge pinned to its `min_size`). This keeps the
/// fr distribution coherent with the cumulative floor — without it, the fr
/// children reserve space against the un-carried INTEGER fixed sizes while the
/// fixed children DISPLAY their carried (possibly +1) sizes, overflowing the row
/// by the accumulated carry.
///
/// The min-size clamp loop mirrors `layout_resolve_1d` / Python
/// `resolve_fraction_unit`: a flexible edge whose exact share would fall below
/// its `min_size` is pinned at `min_size` and removed from the fraction pool,
/// repeating until stable.
pub fn layout_resolve_1d_exact(
    total: u16,
    edges: &[Edge],
    fixed_exact: &[Option<f64>],
) -> Vec<u16> {
    if edges.is_empty() {
        return Vec::new();
    }

    // Exact size per edge: fixed edges resolve immediately; flexible edges are
    // filled in after the fraction pass (`None` until then).
    let mut exact: Vec<Option<f64>> = edges
        .iter()
        .enumerate()
        .map(|(i, e)| {
            e.size
                .map(|sz| fixed_exact.get(i).copied().flatten().unwrap_or(sz as f64))
        })
        .collect();

    // Flexible edges: (index, fraction, min_size).
    let mut flexible: Vec<(usize, u16, u16)> = Vec::new();
    for (i, e) in edges.iter().enumerate() {
        if e.size.is_none() {
            flexible.push((i, e.fraction.max(1), e.min_size));
        }
    }

    if !flexible.is_empty() {
        let fixed_sum: f64 = exact.iter().map(|s| s.unwrap_or(0.0)).sum();
        let mut remaining = (total as f64 - fixed_sum).max(0.0);
        let mut total_fraction: f64 = flexible.iter().map(|&(_, f, _)| f as f64).sum();

        // Iteratively pin flexible edges that would underflow their min_size.
        loop {
            if flexible.is_empty() || total_fraction <= 0.0 {
                break;
            }
            let unit = remaining / total_fraction;
            let mut pinned = false;
            for idx in 0..flexible.len() {
                let (edge_idx, fraction, min_size) = flexible[idx];
                if min_size > 0 && unit * (fraction as f64) < (min_size as f64) {
                    exact[edge_idx] = Some(min_size as f64);
                    remaining = (remaining - min_size as f64).max(0.0);
                    total_fraction -= fraction as f64;
                    flexible.remove(idx);
                    pinned = true;
                    break;
                }
            }
            if !pinned {
                let unit = if total_fraction > 0.0 {
                    remaining / total_fraction
                } else {
                    0.0
                };
                for &(edge_idx, fraction, _) in &flexible {
                    exact[edge_idx] = Some(unit * fraction as f64);
                }
                break;
            }
        }
        // Any flexible edge left unresolved (e.g. total_fraction hit 0) → 0.
        for e in exact.iter_mut() {
            if e.is_none() {
                *e = Some(0.0);
            }
        }
    }

    // Cumulative floor (Python `accumulate(...).__floor__()`): displayed size =
    // floor(cum + exact) - floor(cum).
    let mut cum = 0.0_f64;
    exact
        .iter()
        .map(|s| {
            let e = s.unwrap_or(0.0);
            let disp = ((cum + e).floor() - cum.floor()) as u16;
            cum += e;
            disp
        })
        .collect()
}

/// Core 1D space allocation algorithm.
///
/// Divides `total` cells among `edges` according to their size, fraction, and
/// min_size constraints. Port of Python Textual's `_layout_resolve.layout_resolve()`.
///
/// Uses deterministic integer arithmetic — no floating point.
///
/// The returned sizes normally sum to `total`, but may exceed it when minimum
/// constraints force it (e.g. two edges with min_size=20 in 30 cells of space).
#[allow(clippy::manual_checked_ops)] // guarded by if total_fraction > 0
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression for the fr-distribution keystone: a mix of non-integer fixed
    /// sizes and `fr` children must NOT overflow the total. This reproduces the
    /// `width_comparison` row in 120 cells: fixed exact sizes 9, 15, 12, 7.5, 18,
    /// 7.5 + auto 5 + fr(1) + fr(3). Python sums to exactly 120 (fr3=35); the old
    /// per-child independent floor gave the fixed children their carried sizes
    /// (vh=8) while `fr` reserved against the integer fixed sizes (vh=7), summing
    /// to 121 (fr3=36). The exact resolver fences-posts everything together.
    #[test]
    fn fr_distribution_no_overflow_with_fractional_fixed() {
        let edges = vec![
            Edge { size: Some(9), fraction: 1, min_size: 0 },
            Edge { size: Some(15), fraction: 1, min_size: 0 },
            Edge { size: Some(12), fraction: 1, min_size: 0 },
            Edge { size: Some(7), fraction: 1, min_size: 0 }, // 25h = 7.5
            Edge { size: Some(18), fraction: 1, min_size: 0 },
            Edge { size: Some(7), fraction: 1, min_size: 0 }, // 25vh = 7.5
            Edge { size: Some(5), fraction: 1, min_size: 0 }, // auto
            Edge { size: None, fraction: 1, min_size: 0 },    // 1fr
            Edge { size: None, fraction: 3, min_size: 0 },    // 3fr
        ];
        let fixed_exact = vec![
            Some(9.0),
            Some(15.0),
            Some(12.0),
            Some(7.5),
            Some(18.0),
            Some(7.5),
            None, // auto: integer
            None,
            None,
        ];
        let sizes = layout_resolve_1d_exact(120, &edges, &fixed_exact);
        // Cumulative floor: 9,15,12, (7.5→7), 18, (carry makes 8), 5, then fr.
        assert_eq!(sizes, vec![9, 15, 12, 7, 18, 8, 5, 11, 35]);
        assert_eq!(sizes.iter().sum::<u16>(), 120, "must not overflow the total");
    }

    /// A pure `fr` split with no fractional fixed edges still distributes exactly
    /// (1fr/1fr/1fr in 120 → 40/40/40), matching the integer resolver.
    #[test]
    fn pure_fr_split_is_exact() {
        let edges = vec![Edge::default(), Edge::default(), Edge::default()];
        let fixed_exact = vec![None, None, None];
        assert_eq!(layout_resolve_1d_exact(120, &edges, &fixed_exact), vec![40, 40, 40]);
        // Uneven total carries the remainder forward (Python fence-post).
        assert_eq!(layout_resolve_1d_exact(121, &edges, &fixed_exact), vec![40, 40, 41]);
    }

    /// Flexible edges below their `min_size` are pinned (and removed from the
    /// fraction pool), like the integer resolver / Python `resolve_fraction_unit`.
    #[test]
    fn min_size_pins_flexible_edge() {
        let edges = vec![
            Edge { size: None, fraction: 1, min_size: 30 },
            Edge { size: None, fraction: 1, min_size: 0 },
        ];
        let fixed_exact = vec![None, None];
        // 20 cells, 1fr/1fr: each would get 10, but edge 0's min is 30 → pinned at
        // 30, edge 1 gets the remainder (0 → clamped to its share of nothing).
        let sizes = layout_resolve_1d_exact(20, &edges, &fixed_exact);
        assert_eq!(sizes[0], 30);
    }
}
