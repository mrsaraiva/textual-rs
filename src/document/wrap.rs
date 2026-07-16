//! Word-wrapping primitives (port of Python `textual/_wrap.py` and
//! `textual/expand_tabs.get_tab_widths`).
//!
//! Deviations (per the grapheme-correctness policy):
//!
//! - Offsets are byte offsets on grapheme cluster boundaries. When folding
//!   an over-wide word, Python breaks per codepoint and can split a
//!   combining sequence; Rust folds on grapheme cluster boundaries (a wide
//!   cluster is kept atomic) and guarantees PROGRESS: every section carries
//!   at least one grapheme even when a single grapheme's cell width exceeds
//!   the wrap width.
//! - Cell widths come from `grapheme_cell_width` (cluster-level), while
//!   Python `cell_len` sums per-codepoint widths, so wrap points on
//!   ZWJ-emoji lines differ from Python BY DESIGN (Python overcounts a ZWJ
//!   family emoji; Rust counts its real 2 cells).
//! - DEGENERATE TAB MODEL: every `'\t'` counts as exactly 1 cell, pinned
//!   consistently across wrap offsets, offset<->location mapping, and
//!   render (`grapheme_cell_width("\t") == 1`). Full tab expansion is a
//!   self-contained follow-up that must flip [`get_tab_widths`] and the
//!   render path at the same time; it must never be enabled for a subset
//!   of the consumers.

use unicode_segmentation::UnicodeSegmentation;

use super::graphemes::{cell_len, grapheme_cell_width};

/// Split `line` into `(section, tab_width)` pairs: each section is the text
/// preceding a tab character (excluding the tab), paired with the width the
/// tab expands to; a trailing tabless section has width 0.
///
/// Degenerate tab model: every tab expands to exactly 1 cell regardless of
/// `tab_size` (see the module docs).
pub fn get_tab_widths(line: &str) -> Vec<(&str, usize)> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    for (index, ch) in line.char_indices() {
        if ch == '\t' {
            parts.push((&line[start..index], 1));
            start = index + 1;
        }
    }
    if start < line.len() {
        parts.push((&line[start..], 0));
    }
    parts
}

/// Yield each "chunk" of `text` as `(start_byte, end_byte, chunk)`: a word
/// with its trailing whitespace, or a run of whitespace (Python
/// `re_chunk = \S+\s*|\s+`).
fn chunks(text: &str) -> Vec<(usize, usize, &str)> {
    let mut result = Vec::new();
    let mut pos = 0usize;
    while pos < text.len() {
        let rest = &text[pos..];
        let first_is_whitespace = rest
            .chars()
            .next()
            .map(char::is_whitespace)
            .unwrap_or(false);
        let end = if first_is_whitespace {
            // `\s+`: a run of whitespace.
            rest.char_indices()
                .find(|(_, ch)| !ch.is_whitespace())
                .map(|(index, _)| index)
                .unwrap_or(rest.len())
        } else {
            // `\S+\s*`: a word plus its trailing whitespace.
            let after_word = rest
                .char_indices()
                .find(|(_, ch)| ch.is_whitespace())
                .map(|(index, _)| index)
                .unwrap_or(rest.len());
            rest[after_word..]
                .char_indices()
                .find(|(_, ch)| !ch.is_whitespace())
                .map(|(index, _)| after_word + index)
                .unwrap_or(rest.len())
        };
        result.push((pos, pos + end, &text[pos..pos + end]));
        pos += end;
    }
    result
}

/// Given a line of text and a width in cells, return the byte offsets at
/// which the line should be split so that it fits within `width` (port of
/// Python `compute_wrap_offsets`; see the module docs for deviations).
///
/// `width == 0` means no wrapping (returns no offsets). With `fold`, words
/// wider than `width` are folded onto new lines at grapheme cluster
/// boundaries; without it they are cropped visually (no break inside).
pub fn compute_wrap_offsets(text: &str, width: usize, fold: bool) -> Vec<usize> {
    if width == 0 {
        return Vec::new();
    }
    let mut break_positions: Vec<usize> = Vec::new();
    let mut cell_offset = 0usize;

    for (start, _end, chunk) in chunks(text) {
        let chunk_width = cell_len(chunk);
        let remaining_space = width.saturating_sub(cell_offset);
        let chunk_fits = remaining_space >= chunk_width;

        if chunk_fits {
            // Simplest case: the word fits within the remaining width.
            cell_offset += chunk_width;
        } else if chunk_width > width {
            // The word doesn't fit on any line.
            if fold {
                // Break before the over-wide word (Python appends the chunk
                // start on the first fold iteration when non-zero) ...
                if start > 0 {
                    break_positions.push(start);
                }
                // ... then fold it on grapheme cluster boundaries. Progress
                // guarantee: a section always keeps at least one grapheme,
                // even when that grapheme alone exceeds `width`.
                let mut line_start = start;
                let mut total_width = 0usize;
                for (grapheme_index, grapheme) in chunk.grapheme_indices(true) {
                    let cell_width = grapheme_cell_width(grapheme);
                    let grapheme_start = start + grapheme_index;
                    if total_width + cell_width > width && grapheme_start > line_start {
                        break_positions.push(grapheme_start);
                        line_start = grapheme_start;
                        total_width = cell_width;
                    } else {
                        total_width += cell_width;
                    }
                }
                cell_offset = total_width;
            } else {
                // Folding isn't allowed, so crop the word.
                if start > 0 {
                    break_positions.push(start);
                }
                cell_offset = chunk_width;
            }
        } else if cell_offset > 0 && start > 0 {
            // The word doesn't fit in the remaining space on this line, but
            // it fits on the next (empty) line.
            break_positions.push(start);
            cell_offset = chunk_width;
        }
    }

    break_positions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_wrap_needed() {
        assert_eq!(compute_wrap_offsets("123", 4, true), Vec::<usize>::new());
        assert_eq!(compute_wrap_offsets("", 4, true), Vec::<usize>::new());
    }

    #[test]
    fn width_zero_never_wraps() {
        assert_eq!(
            compute_wrap_offsets("123 4567 89", 0, true),
            Vec::<usize>::new()
        );
    }

    #[test]
    fn word_wrap_and_fold() {
        // The SIMPLE_TEXT table from Python test_wrapped_document.py.
        assert_eq!(compute_wrap_offsets("123 4567", 4, true), vec![4]);
        assert_eq!(compute_wrap_offsets("12345", 4, true), vec![4]);
        assert_eq!(compute_wrap_offsets("123456789", 4, true), vec![4, 8]);
    }

    #[test]
    fn navigator_fixture_offsets() {
        // "01 3456" wraps at width 4 into "01 " / "3456".
        assert_eq!(compute_wrap_offsets("01 3456", 4, true), vec![3]);
        assert_eq!(compute_wrap_offsets("01234", 4, true), vec![4]);
    }

    #[test]
    fn fold_never_splits_a_grapheme_cluster() {
        let text = "a\u{1F469}\u{200D}\u{1F680}\u{1F469}\u{200D}\u{1F680}b";
        let emoji_len = "\u{1F469}\u{200D}\u{1F680}".len();
        let offsets = compute_wrap_offsets(text, 2, true);
        assert_eq!(offsets, vec![1, 1 + emoji_len, 1 + 2 * emoji_len]);
        // Every offset is a grapheme boundary (sections: a / emoji / emoji / b).
        for offset in offsets {
            assert!(text.is_char_boundary(offset));
        }
    }

    #[test]
    fn fold_makes_progress_when_one_grapheme_exceeds_width() {
        // A 2-cell emoji at width 1: each section keeps one grapheme, no
        // empty section, no infinite loop (Python would emit a duplicate
        // break here).
        let text = "\u{1F469}\u{200D}\u{1F680}\u{1F469}\u{200D}\u{1F680}";
        let emoji_len = "\u{1F469}\u{200D}\u{1F680}".len();
        assert_eq!(compute_wrap_offsets(text, 1, true), vec![emoji_len]);
    }

    #[test]
    fn zwj_emoji_wrap_points_use_cluster_widths() {
        // Deviation pin: Rust counts the ZWJ emoji as its real 2 cells, so
        // two of them fit at width 4 (Python `cell_len` overcounts to 8 and
        // would wrap).
        let text = "\u{1F469}\u{200D}\u{1F680}\u{1F469}\u{200D}\u{1F680}";
        assert_eq!(compute_wrap_offsets(text, 4, true), Vec::<usize>::new());
    }

    #[test]
    fn tabs_count_one_cell_in_wrap_offsets() {
        // Degenerate tab model: "a\tb" is 3 cells; at width 2 the break
        // falls before 'b'.
        assert_eq!(compute_wrap_offsets("a\tb", 2, true), vec![2]);
    }

    #[test]
    fn get_tab_widths_degenerate_model() {
        assert_eq!(get_tab_widths("abc"), vec![("abc", 0)]);
        assert_eq!(get_tab_widths("a\tb"), vec![("a", 1), ("b", 0)]);
        assert_eq!(get_tab_widths("a\t"), vec![("a", 1)]);
        assert_eq!(get_tab_widths("\t\t"), vec![("", 1), ("", 1)]);
        assert_eq!(get_tab_widths(""), Vec::<(&str, usize)>::new());
    }

    #[test]
    fn crop_mode_breaks_before_over_wide_word_only() {
        assert_eq!(compute_wrap_offsets("ab cdefgh", 4, false), vec![3]);
    }
}
