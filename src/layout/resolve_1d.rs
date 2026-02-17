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
