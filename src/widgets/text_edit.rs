use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub(crate) fn prev_grapheme_boundary(s: &str, idx: usize) -> usize {
    let idx = idx.min(s.len());
    let idx = if s.is_char_boundary(idx) {
        idx
    } else {
        prev_char_boundary(s, idx)
    };
    let mut prev = 0usize;
    for boundary in grapheme_boundaries(s) {
        if boundary >= idx {
            break;
        }
        prev = boundary;
    }
    prev
}

pub(crate) fn next_grapheme_boundary(s: &str, idx: usize) -> usize {
    let idx = idx.min(s.len());
    if idx >= s.len() {
        return s.len();
    }
    let idx = if s.is_char_boundary(idx) {
        idx
    } else {
        next_char_boundary(s, idx)
    };
    for boundary in grapheme_boundaries(s) {
        if boundary > idx {
            return boundary;
        }
    }
    s.len()
}

pub(crate) fn clamp_grapheme_boundary(s: &str, idx: usize) -> usize {
    if idx >= s.len() {
        return s.len();
    }
    let idx = if s.is_char_boundary(idx) {
        idx
    } else {
        prev_char_boundary(s, idx)
    };
    let mut clamped = 0usize;
    for boundary in grapheme_boundaries(s) {
        if boundary > idx {
            break;
        }
        clamped = boundary;
    }
    clamped
}

pub(crate) fn cell_len_prefix(s: &str, byte_end: usize) -> usize {
    let mut cells = 0usize;
    let end = byte_end.min(s.len());
    for (start, grapheme) in s.grapheme_indices(true) {
        if start >= end {
            break;
        }
        cells = cells.saturating_add(grapheme_cell_width(grapheme));
    }
    cells
}

pub(crate) fn byte_index_from_cell_x(s: &str, target_cell: usize) -> usize {
    let mut cells = 0usize;
    let mut last = 0usize;
    for (start, grapheme) in s.grapheme_indices(true) {
        let width = grapheme_cell_width(grapheme);
        let mid = cells.saturating_add(width / 2);
        if target_cell <= mid {
            return start;
        }
        cells = cells.saturating_add(width);
        last = start + grapheme.len();
        if target_cell < cells {
            return last;
        }
    }
    last
}

pub(crate) fn grapheme_cell_width(grapheme: &str) -> usize {
    UnicodeWidthStr::width(grapheme).max(1)
}

fn prev_char_boundary(s: &str, mut idx: usize) -> usize {
    idx = idx.min(s.len());
    while idx > 0 && !s.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

fn next_char_boundary(s: &str, mut idx: usize) -> usize {
    idx = idx.min(s.len());
    if idx >= s.len() {
        return s.len();
    }
    idx += 1;
    while idx < s.len() && !s.is_char_boundary(idx) {
        idx += 1;
    }
    idx.min(s.len())
}

fn grapheme_boundaries(s: &str) -> impl Iterator<Item = usize> + '_ {
    s.grapheme_indices(true)
        .map(|(start, _)| start)
        .chain(std::iter::once(s.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boundaries_follow_grapheme_clusters() {
        let s = "a\u{0301}👩‍🚀z";
        let a_acute_end = "a\u{0301}".len();
        let astronaut_start = a_acute_end;
        let astronaut_end = astronaut_start + "👩‍🚀".len();

        assert_eq!(next_grapheme_boundary(s, 0), a_acute_end);
        assert_eq!(next_grapheme_boundary(s, astronaut_start), astronaut_end);
        assert_eq!(prev_grapheme_boundary(s, astronaut_end), astronaut_start);
    }

    #[test]
    fn cell_x_maps_to_grapheme_boundaries() {
        let s = "a\u{0301}👩‍🚀b";
        let astr_start = "a\u{0301}".len();
        let astr_end = astr_start + "👩‍🚀".len();

        assert_eq!(byte_index_from_cell_x(s, 0), 0);
        assert_eq!(byte_index_from_cell_x(s, 1), astr_start);
        assert_eq!(byte_index_from_cell_x(s, 2), astr_start);
        assert_eq!(byte_index_from_cell_x(s, 3), astr_end);
    }
}
