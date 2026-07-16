//! Wrap-aware cursor navigation (port of Python
//! `textual/document/_document_navigator.py`).
//!
//! The selection is stored in document space, but cursor *movement* happens
//! in visual space: pressing down may move the cursor to a position further
//! along the current raw document line rather than onto the next raw line.
//!
//! Borrow shape (deviation from Python, which holds the wrapped document):
//! only `last_x_offset` lives here; every method takes
//! `(&Document, &WrappedDocument)`, which must be in sync.
//!
//! Byte-column deviations (spec 3.2): the three Python `-1` codepoint steps
//! (`is_end_of_wrapped_line`, `get_location_end`, and the section clamp in
//! `get_target_document_column`) are grapheme-boundary steps here.

use super::graphemes::{cell_len_prefix, prev_grapheme_boundary};
use super::{Document, Location, WrappedDocument};

/// Wrap-aware movement over a [`WrappedDocument`].
#[derive(Debug, Clone, Default)]
pub struct DocumentNavigator {
    /// The last x offset (cell width) the cursor was deliberately moved to
    /// horizontally, restored (as a maximum with the current visual offset,
    /// the Python rule) on vertical movement.
    pub last_x_offset: usize,
}

/// Locate `value` in the sorted `sequence` via bisection (Python `index`).
fn index_of(sequence: &[usize], value: usize) -> Option<usize> {
    sequence.binary_search(&value).ok()
}

impl DocumentNavigator {
    pub fn new() -> Self {
        Self::default()
    }

    /// True when the location is at column 0.
    pub fn is_start_of_document_line(&self, location: Location) -> bool {
        location.1 == 0
    }

    /// True when the location is at the start of a wrapped section.
    pub fn is_start_of_wrapped_line(&self, wrapped: &WrappedDocument, location: Location) -> bool {
        if self.is_start_of_document_line(location) {
            return true;
        }
        let (row, column) = location;
        index_of(wrapped.get_offsets(row).unwrap_or(&[]), column).is_some()
    }

    /// True if the location is at the end of a document line (the "end" is
    /// one past the final index; there is a space for the cursor to rest).
    pub fn is_end_of_document_line(&self, document: &Document, location: Location) -> bool {
        let (row, column) = location;
        column == document.line(row).len()
    }

    /// True if the location is on the last cell of a wrapped section.
    pub fn is_end_of_wrapped_line(
        &self,
        document: &Document,
        wrapped: &WrappedDocument,
        location: Location,
    ) -> bool {
        if self.is_end_of_document_line(document, location) {
            return true;
        }
        let (row, column) = location;
        // Python: `index(wrap_offsets, column - 1)`; byte space steps back a
        // grapheme instead.
        let prev = prev_grapheme_boundary(document.line(row), column);
        index_of(wrapped.get_offsets(row).unwrap_or(&[]), prev).is_some()
    }

    /// True when the location is on the first line of the document.
    pub fn is_first_document_line(&self, location: Location) -> bool {
        location.0 == 0
    }

    /// True when the location is on the first wrapped section of the first
    /// line.
    pub fn is_first_wrapped_line(&self, wrapped: &WrappedDocument, location: Location) -> bool {
        if !self.is_first_document_line(location) {
            return false;
        }
        let (row, column) = location;
        let wrap_offsets = wrapped.get_offsets(row).unwrap_or(&[]);
        wrap_offsets.is_empty() || column < wrap_offsets[0]
    }

    /// True when the location is on the last line of the document.
    pub fn is_last_document_line(&self, document: &Document, location: Location) -> bool {
        location.0 == document.line_count() - 1
    }

    /// True when the location is on the last wrapped section of the last
    /// line (visually the last rendered row).
    pub fn is_last_wrapped_line(
        &self,
        document: &Document,
        wrapped: &WrappedDocument,
        location: Location,
    ) -> bool {
        if !self.is_last_document_line(document, location) {
            return false;
        }
        let (row, column) = location;
        let wrap_offsets = wrapped.get_offsets(row).unwrap_or(&[]);
        wrap_offsets.is_empty() || column >= *wrap_offsets.last().expect("non-empty")
    }

    /// True when the location is `(0, 0)`.
    pub fn is_start_of_document(&self, location: Location) -> bool {
        location == (0, 0)
    }

    /// True when the location is at the very end of the document.
    pub fn is_end_of_document(&self, document: &Document, location: Location) -> bool {
        self.is_last_document_line(document, location)
            && self.is_end_of_document_line(document, location)
    }

    /// The location one grapheme to the left, crossing line boundaries
    /// (Python moves one codepoint; Rust deliberately moves one grapheme).
    pub fn get_location_left(&self, document: &Document, location: Location) -> Location {
        if location == (0, 0) {
            return (0, 0);
        }
        let (row, column) = location;
        if column == 0 {
            (row - 1, document.line(row - 1).len())
        } else {
            (row, prev_grapheme_boundary(document.line(row), column))
        }
    }

    /// The location one grapheme to the right, crossing line boundaries.
    pub fn get_location_right(&self, document: &Document, location: Location) -> Location {
        if self.is_end_of_document(document, location) {
            return location;
        }
        let (row, column) = location;
        if self.is_end_of_document_line(document, location) {
            (row + 1, 0)
        } else {
            (
                row,
                super::graphemes::next_grapheme_boundary(document.line(row), column),
            )
        }
    }

    /// The location visually aligned with the cell above the given location.
    ///
    /// Python-parity boundary: moving up from the first wrapped line goes to
    /// `(0, 0)`.
    pub fn get_location_above(
        &self,
        document: &Document,
        wrapped: &WrappedDocument,
        location: Location,
    ) -> Location {
        let (line_index, column_index) = location;
        let wrap_offsets = wrapped.get_offsets(line_index).unwrap_or(&[]);
        let section_index = wrap_offsets.partition_point(|&offset| offset <= column_index);
        let section_start = if section_index == 0 {
            0
        } else {
            wrap_offsets[section_index - 1]
        };
        let sections = wrapped.get_sections(document, line_index);
        let section = sections[section_index.min(sections.len() - 1)];
        let offset_within_section = column_index.saturating_sub(section_start);

        // Convert the cursor offset to a cell (visual) offset; the vertical
        // target keeps the LARGER of the current x and the remembered x
        // (Python `max` rule).
        let current_visual_offset =
            cell_len_prefix(section, offset_within_section.min(section.len()));
        let target_offset = current_visual_offset.max(self.last_x_offset);

        if section_index == 0 {
            // Moving up from a position on the first visual line moves us to
            // the start of the document.
            if self.is_first_wrapped_line(wrapped, location) {
                return (0, 0);
            }
            // The last section of the line above.
            let target_row = line_index - 1;
            let target_column =
                wrapped.get_target_document_column(document, target_row, target_offset, -1);
            (target_row, target_column)
        } else {
            // Stay on the same document line, but move to the section above
            // (which could be shorter, hence the clamp inside).
            let target_column = wrapped.get_target_document_column(
                document,
                line_index,
                target_offset,
                section_index as isize - 1,
            );
            (line_index, target_column)
        }
    }

    /// The location visually below the given location.
    ///
    /// Python-parity boundary: moving down from the last wrapped line goes
    /// to the end of the last line.
    pub fn get_location_below(
        &self,
        document: &Document,
        wrapped: &WrappedDocument,
        location: Location,
    ) -> Location {
        let (line_index, column_index) = location;
        let wrap_offsets = wrapped.get_offsets(line_index).unwrap_or(&[]);
        let section_index = wrap_offsets.partition_point(|&offset| offset <= column_index);
        let section_start = if section_index == 0 {
            0
        } else {
            wrap_offsets[section_index - 1]
        };
        let sections = wrapped.get_sections(document, line_index);
        let section = sections[section_index.min(sections.len() - 1)];
        let offset_within_section = column_index.saturating_sub(section_start);
        let current_visual_offset =
            cell_len_prefix(section, offset_within_section.min(section.len()));
        let target_offset = current_visual_offset.max(self.last_x_offset);

        if section_index == sections.len() - 1 {
            // Last section of the last line: go to the end of the document.
            if self.is_last_document_line(document, location) {
                return (line_index, document.line(line_index).len());
            }
            // The first section of the line below.
            let target_row = line_index + 1;
            let target_column =
                wrapped.get_target_document_column(document, target_row, target_offset, 0);
            (target_row, target_column)
        } else {
            let target_column = wrapped.get_target_document_column(
                document,
                line_index,
                target_offset,
                section_index as isize + 1,
            );
            (line_index, target_column)
        }
    }

    /// The location at the end of the current wrapped section (or document
    /// line when unwrapped).
    pub fn get_location_end(
        &self,
        document: &Document,
        wrapped: &WrappedDocument,
        location: Location,
    ) -> Location {
        let (line_index, column_offset) = location;
        let wrap_offsets = wrapped.get_offsets(line_index).unwrap_or(&[]);
        if !wrap_offsets.is_empty() {
            let next_offset_right = wrap_offsets.partition_point(|&offset| offset <= column_offset);
            if next_offset_right == wrap_offsets.len() {
                // No more wrapping to the right: go to the line end.
                return (line_index, document.line(line_index).len());
            }
            // Python: `wrap_offsets[i] - 1`; byte space steps back one
            // grapheme (never splits a cluster).
            let line = document.line(line_index);
            (
                line_index,
                prev_grapheme_boundary(line, wrap_offsets[next_offset_right]),
            )
        } else {
            (line_index, document.line(line_index).len())
        }
    }

    /// The "home" location for the given location: the previous wrap offset
    /// when wrapped, else column 0 (with optional smart-home behavior).
    pub fn get_location_home(
        &self,
        document: &Document,
        wrapped: &WrappedDocument,
        location: Location,
        smart_home: bool,
    ) -> Location {
        let (line_index, column_offset) = location;
        let wrap_offsets = wrapped.get_offsets(line_index).unwrap_or(&[]);
        if !wrap_offsets.is_empty() {
            let next_offset_left = wrap_offsets.partition_point(|&offset| offset <= column_offset);
            if next_offset_left == 0 {
                return (line_index, 0);
            }
            (line_index, wrap_offsets[next_offset_left - 1])
        } else {
            if smart_home {
                let line = document.line(line_index);
                let target_column = line
                    .char_indices()
                    .find(|(_, ch)| !ch.is_whitespace())
                    .map(|(index, _)| index)
                    .unwrap_or(0);
                if column_offset == 0 || column_offset > target_column {
                    return (line_index, target_column);
                }
            }
            (line_index, 0)
        }
    }

    /// Apply a visual vertical offset to a location (used for page up/down).
    pub fn get_location_at_y_offset(
        &self,
        document: &Document,
        wrapped: &WrappedDocument,
        location: Location,
        vertical_offset: isize,
    ) -> Location {
        let (x_offset, y_offset) = wrapped.location_to_offset(document, location);
        wrapped.offset_to_location(
            document,
            x_offset as isize,
            y_offset as isize + vertical_offset,
        )
    }

    /// The nearest reachable location in the document.
    pub fn clamp_reachable(&self, document: &Document, location: Location) -> Location {
        let (row, column) = location;
        let clamped_row = row.min(document.line_count() - 1);
        let line = document.line(clamped_row);
        (clamped_row, column.min(line.len()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Python fixture from tests/document/test_document_navigator.py:
    //
    //   "01 3456\n01234" at wrap width 4:
    //   line 0 | "01 " / "3456"
    //   line 1 | "0123" (offsets [4] -> "0123"? no: [4] -> "0123"/"4")
    const TEXT: &str = "01 3456\n01234";

    fn make(text: &str, width: usize) -> (Document, WrappedDocument, DocumentNavigator) {
        let document = Document::new(text);
        let wrapped = WrappedDocument::new(&document, width, 4);
        (document, wrapped, DocumentNavigator::new())
    }

    #[test]
    fn test_get_location_above() {
        let cases: &[(Location, Location)] = &[
            ((0, 0), (0, 0)),
            ((0, 1), (0, 0)),
            ((0, 2), (0, 0)),
            ((0, 3), (0, 0)),
            ((0, 4), (0, 1)),
            ((0, 5), (0, 2)),
            ((0, 6), (0, 2)), // clamps to valid index
            ((0, 7), (0, 2)), // clamps to the last valid index
            ((1, 0), (0, 3)),
            ((1, 1), (0, 4)),
            ((1, 5), (1, 1)),
        ];
        let (document, wrapped, navigator) = make(TEXT, 4);
        for &(start, end) in cases {
            assert_eq!(
                navigator.get_location_above(&document, &wrapped, start),
                end,
                "above({start:?})"
            );
        }
    }

    #[test]
    fn test_get_location_below() {
        let cases: &[(Location, Location)] = &[
            ((0, 0), (0, 3)),
            ((0, 1), (0, 4)),
            ((0, 2), (0, 5)),
            ((0, 3), (1, 0)),
            ((0, 4), (1, 1)),
            ((0, 5), (1, 2)),
            ((0, 6), (1, 3)),
            ((0, 7), (1, 3)),
            ((1, 3), (1, 5)),
        ];
        let (document, wrapped, navigator) = make(TEXT, 4);
        for &(start, end) in cases {
            assert_eq!(
                navigator.get_location_below(&document, &wrapped, start),
                end,
                "below({start:?})"
            );
        }
    }

    #[test]
    fn test_get_location_home() {
        let cases: &[(Location, Location)] = &[
            ((0, 0), (0, 0)),
            ((0, 2), (0, 0)),
            ((0, 3), (0, 3)),
            ((0, 6), (0, 3)),
            ((0, 7), (0, 3)),
            ((1, 0), (1, 0)),
            ((1, 3), (1, 0)),
            ((1, 4), (1, 4)),
            ((1, 5), (1, 4)),
        ];
        let (document, wrapped, navigator) = make(TEXT, 4);
        for &(start, end) in cases {
            assert_eq!(
                navigator.get_location_home(&document, &wrapped, start, false),
                end,
                "home({start:?})"
            );
        }
    }

    #[test]
    fn test_get_location_end() {
        let cases: &[(Location, Location)] = &[
            ((0, 0), (0, 2)),
            ((0, 2), (0, 2)),
            ((0, 3), (0, 7)),
            ((0, 5), (0, 7)),
            ((1, 2), (1, 3)),
        ];
        let (document, wrapped, navigator) = make(TEXT, 4);
        for &(start, end) in cases {
            assert_eq!(
                navigator.get_location_end(&document, &wrapped, start),
                end,
                "end({start:?})"
            );
        }
    }

    #[test]
    fn vertical_move_uses_max_of_current_and_remembered_x() {
        // Python rule: target x = max(current visual offset, last_x_offset).
        let (document, wrapped, mut navigator) = make("abcdef\nab\nabcdef", 0);
        navigator.last_x_offset = 2;
        // From (1, 4)? line 1 is "ab": column clamped input (1, 2), current
        // visual x = 2 which ties the remembered 2 -> target column 2.
        assert_eq!(
            navigator.get_location_below(&document, &wrapped, (1, 2)),
            (2, 2)
        );
        // Current x (5) LARGER than remembered (2): Python takes the max,
        // where remembered-else-current would take 2.
        navigator.last_x_offset = 2;
        assert_eq!(
            navigator.get_location_below(&document, &wrapped, (0, 5)),
            (1, 2)
        );
        // Remembered larger than current: restored on the longer line.
        navigator.last_x_offset = 5;
        assert_eq!(
            navigator.get_location_below(&document, &wrapped, (1, 1)),
            (2, 5)
        );
    }

    #[test]
    fn boundary_navigation_matches_python() {
        // First-line Up -> (0, 0); last-line Down -> line end, in both wrap
        // modes (the intended Phase 4 behavior change).
        for width in [0usize, 4] {
            let (document, wrapped, navigator) = make(TEXT, width);
            assert_eq!(
                navigator.get_location_above(&document, &wrapped, (0, 2)),
                (0, 0),
                "width {width}"
            );
            let below = navigator.get_location_below(&document, &wrapped, (1, 5));
            assert_eq!(below, (1, 5), "width {width}");
            let below_mid = navigator.get_location_below(
                &document,
                &wrapped,
                if width == 0 { (1, 2) } else { (1, 5) },
            );
            // Down from the last wrapped line lands on the line end.
            assert_eq!(below_mid, (1, 5), "width {width}");
        }
    }

    #[test]
    fn wrapped_navigation_over_wide_graphemes_lands_on_boundaries() {
        // CJK/emoji wrapped lines: up/down land on grapheme boundaries.
        let emoji = "\u{1F469}\u{200D}\u{1F680}";
        let text = format!("\u{597D}\u{597D}\u{597D}\u{597D}\n{emoji}{emoji}");
        let (document, wrapped, navigator) = make(&text, 4);
        // line 0: two sections of two CJK chars each; line 1: two emoji fit.
        assert_eq!(wrapped.get_offsets(0), Some(&[6usize][..]));
        // From the 2nd CJK char (visual x=2), down lands on the 2nd CJK
        // char of the section below (byte 9), a grapheme boundary.
        let down = navigator.get_location_below(&document, &wrapped, (0, 3));
        assert!(document.line(0).is_char_boundary(down.1));
        assert_eq!(down, (0, 9));
        let up = navigator.get_location_above(&document, &wrapped, (1, emoji.len()));
        assert!(document.line(0).is_char_boundary(up.1));
        assert_eq!(up, (0, 9));
    }

    #[test]
    fn end_of_wrapped_line_predicate_uses_grapheme_step() {
        // is_end_of_wrapped_line: Python checks `index(offsets, column - 1)`
        // (one codepoint back); byte space steps back one GRAPHEME. For the
        // CJK line wrapped at [6], the location whose previous grapheme
        // boundary IS the wrap offset is byte 9 (Python codepoint 3).
        let text = "\u{597D}\u{597D}\u{597D}\u{597D}"; // 4 wide chars
        let (document, wrapped, navigator) = make(text, 4);
        assert_eq!(wrapped.get_offsets(0), Some(&[6usize][..]));
        assert!(navigator.is_end_of_wrapped_line(&document, &wrapped, (0, 9)));
        assert!(!navigator.is_end_of_wrapped_line(&document, &wrapped, (0, 6)));
        assert!(!navigator.is_end_of_wrapped_line(&document, &wrapped, (0, 3)));
        // The document-line end is always an end of wrapped line.
        assert!(navigator.is_end_of_wrapped_line(&document, &wrapped, (0, 12)));
    }

    #[test]
    fn clamp_reachable_clamps_rows_and_columns() {
        let (document, _wrapped, navigator) = make(TEXT, 4);
        assert_eq!(navigator.clamp_reachable(&document, (100, 100)), (1, 5));
        assert_eq!(navigator.clamp_reachable(&document, (0, 100)), (0, 7));
        assert_eq!(navigator.clamp_reachable(&document, (0, 3)), (0, 3));
    }
}
