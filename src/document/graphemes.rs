//! Grapheme-cluster and cell-width helpers shared by the document model and
//! the text-editing widgets.
//!
//! Moved here from `widgets/text_edit.rs` so that `crate::document` (a
//! framework primitive) does not depend on `crate::widgets`;
//! `widgets/text_edit.rs` re-exports them for widget code.

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// The nearest grapheme cluster boundary at or before `idx`, excluding `idx`
/// itself (i.e. the boundary strictly before `idx`, or 0).
pub fn prev_grapheme_boundary(s: &str, idx: usize) -> usize {
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

/// The nearest grapheme cluster boundary strictly after `idx` (or `s.len()`).
pub fn next_grapheme_boundary(s: &str, idx: usize) -> usize {
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

/// Clamp `idx` to the nearest grapheme cluster boundary at or before it.
pub fn clamp_grapheme_boundary(s: &str, idx: usize) -> usize {
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

/// Total cell width of the prefix of `s` ending at byte offset `byte_end`.
pub fn cell_len_prefix(s: &str, byte_end: usize) -> usize {
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

/// Total cell width of `s` (grapheme-cluster based; a tab counts as 1 cell,
/// matching the pinned degenerate tab model).
pub fn cell_len(s: &str) -> usize {
    cell_len_prefix(s, s.len())
}

/// Map a target cell offset to the byte index of the nearest grapheme
/// boundary in `s` (used for hit-testing and vertical cursor movement).
pub fn byte_index_from_cell_x(s: &str, target_cell: usize) -> usize {
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

/// Cell width of a single grapheme cluster. Zero-width clusters (including
/// the degenerate 1-cell tab model for `"\t"`) are widened to 1 cell.
pub fn grapheme_cell_width(grapheme: &str) -> usize {
    UnicodeWidthStr::width(grapheme).max(1)
}

/// The start of the word at or before `idx` (whitespace-delimited).
pub fn prev_word_boundary(s: &str, idx: usize) -> usize {
    if s.is_empty() {
        return 0;
    }
    let mut cursor = clamp_grapheme_boundary(s, idx);
    while cursor > 0 {
        let prev = prev_grapheme_boundary(s, cursor);
        if !s[prev..cursor].chars().all(char::is_whitespace) {
            break;
        }
        cursor = prev;
    }
    while cursor > 0 {
        let prev = prev_grapheme_boundary(s, cursor);
        if s[prev..cursor].chars().all(char::is_whitespace) {
            break;
        }
        cursor = prev;
    }
    cursor
}

/// The end of the word at or after `idx` (whitespace-delimited).
pub fn next_word_boundary(s: &str, idx: usize) -> usize {
    if s.is_empty() {
        return 0;
    }
    let mut cursor = clamp_grapheme_boundary(s, idx);
    while cursor < s.len() {
        let next = next_grapheme_boundary(s, cursor);
        if !s[cursor..next].chars().all(char::is_whitespace) {
            break;
        }
        cursor = next;
    }
    while cursor < s.len() {
        let next = next_grapheme_boundary(s, cursor);
        if s[cursor..next].chars().all(char::is_whitespace) {
            break;
        }
        cursor = next;
    }
    cursor
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

    #[test]
    fn word_boundaries_skip_whitespace_and_clusters() {
        let s = "go  a\u{0301} 👩‍🚀 end";
        let end_word_start = s.find("end").unwrap();
        assert_eq!(prev_word_boundary(s, s.len()), end_word_start);
        assert_eq!(next_word_boundary(s, 0), 2);
        assert_eq!(next_word_boundary(s, 2), 7);
    }

    #[test]
    fn tab_counts_as_one_cell() {
        // Pinned degenerate tab model: '\t' is 1 cell everywhere.
        assert_eq!(grapheme_cell_width("\t"), 1);
        assert_eq!(cell_len("a\tb"), 3);
    }
}
