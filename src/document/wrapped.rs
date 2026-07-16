//! A wrapped view over a [`Document`] (port of Python
//! `textual/document/_wrapped_document.py`).

use super::Document;
use super::Location;
use super::graphemes::{byte_index_from_cell_x, cell_len_prefix, prev_grapheme_boundary};
use super::wrap::{compute_wrap_offsets, get_tab_widths};

/// A view into a [`Document`] which wraps the document at a certain width
/// and can be queried to retrieve lines from the *wrapped* version of the
/// document. Allows for incremental updates, ensuring that we only re-wrap
/// ranges of the document that were influenced by edits.
///
/// Borrow shape (deviation from Python, which holds the document): the
/// caches live here and every method that needs line content takes
/// `&Document`. The caches and the document MUST be kept in sync: after an
/// edit, call [`WrappedDocument::wrap_range`] (or
/// [`WrappedDocument::wrap`]) before querying.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrappedDocument {
    /// Per document line, the byte offsets within the line where wrapping
    /// breaks occur.
    wrap_offsets: Vec<Vec<usize>>,
    /// Maps y-offsets (from the top of the wrapped document) to
    /// `(line_index, section_offset_within_line)`.
    offset_to_line_info: Vec<(usize, usize)>,
    /// Maps document line indices to all the vertical offsets which
    /// correspond to that line.
    line_index_to_offsets: Vec<Vec<usize>>,
    /// Expanded tab widths per line. Under the pinned degenerate tab model
    /// every entry is 1; kept so that full tab expansion can flip a single
    /// source of widths later.
    tab_width_cache: Vec<Vec<usize>>,
    /// The width the document is currently wrapped at (0 = no wrapping).
    width: usize,
    /// The maximum width to expand tabs to (unused under the degenerate
    /// 1-cell tab model; kept for the full-expansion flip).
    tab_width: usize,
}

impl WrappedDocument {
    /// Construct and wrap immediately (width 0 = no wrapping).
    pub fn new(document: &Document, width: usize, tab_width: usize) -> Self {
        let mut wrapped = Self {
            wrap_offsets: Vec::new(),
            offset_to_line_info: Vec::new(),
            line_index_to_offsets: Vec::new(),
            tab_width_cache: Vec::new(),
            width,
            tab_width,
        };
        wrapped.wrap(document, width, Some(tab_width));
        wrapped
    }

    /// The width the document is wrapped at (0 = no wrapping).
    pub fn width(&self) -> usize {
        self.width
    }

    /// The tab stop width (degenerate model: informational only).
    pub fn tab_width(&self) -> usize {
        self.tab_width
    }

    /// The height (visual line count) of the wrapped document.
    pub fn height(&self) -> usize {
        self.wrap_offsets
            .iter()
            .map(|offsets| offsets.len() + 1)
            .sum()
    }

    /// Wrap and cache all lines in the document.
    pub fn wrap(&mut self, document: &Document, width: usize, tab_width: Option<usize>) {
        self.width = width;
        if let Some(tab_width) = tab_width {
            self.tab_width = tab_width;
        }

        let mut new_wrap_offsets = Vec::with_capacity(document.line_count());
        let mut offset_to_line_info = Vec::new();
        let mut line_index_to_offsets = Vec::with_capacity(document.line_count());
        let mut line_tab_widths = Vec::with_capacity(document.line_count());

        let mut current_offset = 0usize;
        for (line_index, line) in document.lines().iter().enumerate() {
            let tab_sections = get_tab_widths(line);
            let wrap_offsets = if width > 0 {
                compute_wrap_offsets(line, width, true)
            } else {
                Vec::new()
            };
            line_tab_widths.push(tab_sections.iter().map(|&(_, w)| w).collect::<Vec<usize>>());
            let mut offsets_for_line = Vec::with_capacity(wrap_offsets.len() + 1);
            for section_y_offset in 0..=wrap_offsets.len() {
                offset_to_line_info.push((line_index, section_y_offset));
                offsets_for_line.push(current_offset);
                current_offset += 1;
            }
            new_wrap_offsets.push(wrap_offsets);
            line_index_to_offsets.push(offsets_for_line);
        }

        self.wrap_offsets = new_wrap_offsets;
        self.offset_to_line_info = offset_to_line_info;
        self.line_index_to_offsets = line_index_to_offsets;
        self.tab_width_cache = line_tab_widths;
    }

    /// The wrapped content: for each document line, its wrapped sections.
    /// Expensive; intended for tests and debugging (Python `lines`).
    pub fn wrapped_lines(&self, document: &Document) -> Vec<Vec<String>> {
        (0..document.line_count())
            .map(|line_index| {
                self.get_sections(document, line_index)
                    .into_iter()
                    .map(str::to_string)
                    .collect()
            })
            .collect()
    }

    /// Incrementally recompute wrapping based on a performed edit.
    ///
    /// Must be called *after* the source document has been edited.
    ///
    /// - `start`: the start location of the edit (document-space).
    /// - `old_end`: the old end location of the edit.
    /// - `new_end`: the new end location of the edit.
    pub fn wrap_range(
        &mut self,
        document: &Document,
        start: Location,
        old_end: Location,
        new_end: Location,
    ) {
        let (start_line_index, _) = start;
        let (old_end_line_index, _) = old_end;
        let (new_end_line_index, _) = new_end;

        // Programmers can pass whatever they wish to the edit API, so clamp
        // the ranges to the bounds of the wrapped document.
        let old_max_index = self.line_index_to_offsets.len().saturating_sub(1);
        let new_max_index = document.line_count().saturating_sub(1);

        let start_line_index = start_line_index.min(old_max_index).min(new_max_index);
        let old_end_line_index = old_end_line_index.min(old_max_index);
        let new_end_line_index = new_end_line_index.min(new_max_index);

        let (top_line_index, old_bottom_line_index) = if start_line_index <= old_end_line_index {
            (start_line_index, old_end_line_index)
        } else {
            (old_end_line_index, start_line_index)
        };
        let new_bottom_line_index = start_line_index.max(new_end_line_index);

        let top_y_offset = self.line_index_to_offsets[top_line_index][0];
        let old_bottom_y_offset = *self.line_index_to_offsets[old_bottom_line_index]
            .last()
            .expect("every line has at least one y-offset");

        // Re-wrap the new range of the edit from top to bottom.
        let new_lines = &document.lines()[top_line_index..=new_bottom_line_index];
        let new_line_count = new_lines.len();

        let mut new_wrap_offsets: Vec<Vec<usize>> = Vec::with_capacity(new_line_count);
        let mut new_line_index_to_offsets: Vec<Vec<usize>> = Vec::with_capacity(new_line_count);
        let mut new_offset_to_line_info: Vec<(usize, usize)> = Vec::new();
        let mut new_tab_widths: Vec<Vec<usize>> = Vec::with_capacity(new_line_count);

        let width = self.width;
        let mut current_y_offset = top_y_offset;
        for (index, line) in new_lines.iter().enumerate() {
            let line_index = top_line_index + index;
            let tab_sections = get_tab_widths(line);
            let wrap_offsets = if width > 0 {
                compute_wrap_offsets(line, width, true)
            } else {
                Vec::new()
            };
            new_tab_widths.push(tab_sections.iter().map(|&(_, w)| w).collect());

            let mut y_offsets_for_line = Vec::with_capacity(wrap_offsets.len() + 1);
            for section_offset in 0..=wrap_offsets.len() {
                y_offsets_for_line.push(current_y_offset);
                new_offset_to_line_info.push((line_index, section_offset));
                current_y_offset += 1;
            }
            new_wrap_offsets.push(wrap_offsets);
            new_line_index_to_offsets.push(y_offsets_for_line);
        }

        // Replace the range start -> old with the new wrapped lines.
        let new_height = new_offset_to_line_info.len();
        self.offset_to_line_info
            .splice(top_y_offset..=old_bottom_y_offset, new_offset_to_line_info);
        self.line_index_to_offsets.splice(
            top_line_index..=old_bottom_line_index,
            new_line_index_to_offsets,
        );
        self.tab_width_cache
            .splice(top_line_index..=old_bottom_line_index, new_tab_widths);

        // How much did the edit/rewrap alter the offsets?
        let old_height = old_bottom_y_offset - top_y_offset + 1;
        let offset_shift = new_height as isize - old_height as isize;
        let line_shift = new_bottom_line_index as isize - old_bottom_line_index as isize;

        // Update the line info at all offsets below the edit region.
        if line_shift != 0 {
            for y_offset in (top_y_offset + new_height)..self.offset_to_line_info.len() {
                let (old_line_index, section_offset) = self.offset_to_line_info[y_offset];
                let new_line_index = (old_line_index as isize + line_shift) as usize;
                self.offset_to_line_info[y_offset] = (new_line_index, section_offset);
            }
        }

        // Update the offsets at all lines below the edit region.
        if offset_shift != 0 {
            for line_index in (top_line_index + new_line_count)..self.line_index_to_offsets.len() {
                for offset in &mut self.line_index_to_offsets[line_index] {
                    *offset = (*offset as isize + offset_shift) as usize;
                }
            }
        }

        self.wrap_offsets
            .splice(top_line_index..=old_bottom_line_index, new_wrap_offsets);
    }

    /// Given an offset within the wrapped/visual display of the document,
    /// return the corresponding document location. Out-of-range offsets are
    /// clamped to valid locations.
    pub fn offset_to_location(&self, document: &Document, x: isize, y: isize) -> Location {
        let x = x.max(0) as usize;
        let y = y.max(0) as usize;

        if self.width == 0 {
            // No wrapping: directly map the offset to a location and clamp.
            let line_index = y.min(self.wrap_offsets.len().saturating_sub(1));
            let column_index = byte_index_from_cell_x(document.line(line_index), x);
            return (line_index, column_index);
        }

        // Find the line corresponding to the given y-offset; a y-offset
        // below the document lands on the bottom wrapped line.
        let (line_index, section_y) = self
            .offset_to_line_info
            .get(y)
            .or_else(|| self.offset_to_line_info.last())
            .copied()
            .unwrap_or((0, 0));
        let column = self.get_target_document_column(document, line_index, x, section_y as isize);
        (line_index, column)
    }

    /// Convert a document location to an `(x, y)` offset within the wrapped
    /// visual display of the document.
    pub fn location_to_offset(&self, document: &Document, location: Location) -> (usize, usize) {
        let (line_index, column_index) = location;
        let line_index = line_index.min(self.line_index_to_offsets.len().saturating_sub(1));

        // Find the section this location falls in, to know which y-offset
        // to use.
        let wrap_offsets = &self.wrap_offsets[line_index];
        let section_index = wrap_offsets.partition_point(|&offset| offset <= column_index);
        let section_start = if section_index == 0 {
            0
        } else {
            wrap_offsets[section_index - 1]
        };

        let y_offsets = &self.line_index_to_offsets[line_index];
        let line = document.line(line_index);
        let section_end = wrap_offsets
            .get(section_index)
            .copied()
            .unwrap_or(line.len());
        let section = &line[section_start..section_end];
        let section_column_index = column_index.saturating_sub(section_start);
        let x_offset = cell_len_prefix(section, section_column_index.min(section.len()));

        (x_offset, y_offsets[section_index])
    }

    /// Given a line index and offsets within the wrapped version of that
    /// line, return the corresponding column index in the raw document.
    /// `y_offset` supports negative indexing (`-1` = the final section).
    pub fn get_target_document_column(
        &self,
        document: &Document,
        line_index: usize,
        x_offset: usize,
        y_offset: isize,
    ) -> usize {
        let sections = self.get_sections(document, line_index);
        let section_count = sections.len();
        let section_index = if y_offset < 0 {
            (section_count as isize + y_offset).max(0) as usize
        } else {
            (y_offset as usize).min(section_count - 1)
        };

        let target_section = sections[section_index];
        // Add the byte lengths of the wrapped sections above this one (from
        // the same raw document line).
        let target_section_start: usize = sections[..section_index]
            .iter()
            .map(|section| section.len())
            .sum();

        let mut target_column_index =
            target_section_start + byte_index_from_cell_x(target_section, x_offset);

        // If we're on the final section of a line, the cursor can legally
        // rest one cell beyond the end. Otherwise keep the cursor within
        // the target section: Python clamps to `len(section) - 1`
        // (codepoints); the byte-space equivalent is the previous grapheme
        // boundary (never split a cluster).
        if section_index != section_count - 1 {
            target_column_index = target_column_index.min(
                target_section_start + prev_grapheme_boundary(target_section, target_section.len()),
            );
        }

        target_column_index
    }

    /// The wrapped sections of one document line (Python `get_sections`).
    pub fn get_sections<'a>(&self, document: &'a Document, line_index: usize) -> Vec<&'a str> {
        let line = document.line(line_index);
        let offsets: &[usize] = self
            .wrap_offsets
            .get(line_index)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let mut sections = Vec::with_capacity(offsets.len() + 1);
        let mut start = 0usize;
        for &offset in offsets {
            sections.push(&line[start..offset]);
            start = offset;
        }
        sections.push(&line[start..]);
        sections
    }

    /// The wrap offsets of one document line, or `None` when `line_index`
    /// is out of bounds (Python raises `ValueError`).
    pub fn get_offsets(&self, line_index: usize) -> Option<&[usize]> {
        self.wrap_offsets.get(line_index).map(Vec::as_slice)
    }

    /// The expanded tab widths of one document line (degenerate model:
    /// always 1 per tab).
    pub fn get_tab_widths(&self, line_index: usize) -> &[usize] {
        self.tab_width_cache
            .get(line_index)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// The `(line_index, section_offset)` at a visual y-offset, if any.
    pub fn offset_line_info(&self, y_offset: usize) -> Option<(usize, usize)> {
        self.offset_to_line_info.get(y_offset).copied()
    }

    /// The visual y-offsets covering a document line.
    pub fn line_offsets(&self, line_index: usize) -> &[usize] {
        self.line_index_to_offsets
            .get(line_index)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIMPLE_TEXT: &str = "123 4567\n12345\n123456789\n";

    fn make(text: &str, width: usize) -> (Document, WrappedDocument) {
        let document = Document::new(text);
        let wrapped = WrappedDocument::new(&document, width, 4);
        (document, wrapped)
    }

    fn lines_of(wrapped: &WrappedDocument, document: &Document) -> Vec<Vec<String>> {
        wrapped.wrapped_lines(document)
    }

    #[test]
    fn test_wrap() {
        let (document, wrapped) = make(SIMPLE_TEXT, 4);
        assert_eq!(
            lines_of(&wrapped, &document),
            vec![
                vec!["123 ", "4567"],
                vec!["1234", "5"],
                vec!["1234", "5678", "9"],
                vec![""],
            ]
        );
    }

    #[test]
    fn test_wrap_empty_document() {
        let (document, wrapped) = make("", 4);
        assert_eq!(lines_of(&wrapped, &document), vec![vec![""]]);
    }

    #[test]
    fn test_wrap_width_zero_no_wrapping() {
        let (document, wrapped) = make(SIMPLE_TEXT, 0);
        assert_eq!(
            lines_of(&wrapped, &document),
            vec![vec!["123 4567"], vec!["12345"], vec!["123456789"], vec![""],]
        );
    }

    #[test]
    fn test_refresh_range() {
        // The post-edit content is not wrapped.
        let (mut document, mut wrapped) = make(SIMPLE_TEXT, 4);
        let start_location = (1, 0);
        let old_end_location = (3, 0);

        let edit_result = document.replace_range(start_location, old_end_location, "123");
        wrapped.wrap_range(
            &document,
            start_location,
            old_end_location,
            edit_result.end_location,
        );

        assert_eq!(
            lines_of(&wrapped, &document),
            vec![vec!["123 ", "4567"], vec!["123"]]
        );
    }

    #[test]
    fn test_refresh_range_new_text_wrapped() {
        // The post-edit content itself must be wrapped.
        let (mut document, mut wrapped) = make(SIMPLE_TEXT, 4);
        let start_location = (1, 0);
        let old_end_location = (3, 0);

        let edit_result = document.replace_range(start_location, old_end_location, "12 34567 8901");
        wrapped.wrap_range(
            &document,
            start_location,
            old_end_location,
            edit_result.end_location,
        );

        assert_eq!(
            lines_of(&wrapped, &document),
            vec![vec!["123 ", "4567"], vec!["12 ", "3456", "7 ", "8901"],]
        );
    }

    #[test]
    fn test_refresh_range_wrapping_at_previously_unavailable_range() {
        // Content inserted at the end of the document wraps correctly.
        let (mut document, mut wrapped) = make(SIMPLE_TEXT, 4);
        let edit_result = document.replace_range((3, 0), (3, 0), "012 3456\n78 90123\n45");
        wrapped.wrap_range(&document, (3, 0), (3, 0), edit_result.end_location);

        assert_eq!(
            lines_of(&wrapped, &document),
            vec![
                vec!["123 ", "4567"],
                vec!["1234", "5"],
                vec!["1234", "5678", "9"],
                vec!["012 ", "3456"],
                vec!["78 ", "9012", "3"],
                vec!["45"],
            ]
        );
    }

    #[test]
    fn test_refresh_range_wrapping_disabled_previously_unavailable_range() {
        let (mut document, mut wrapped) = make(SIMPLE_TEXT, 0);
        let edit_result = document.replace_range((3, 0), (3, 0), "012 3456\n78 90123\n45");
        wrapped.wrap_range(&document, (3, 0), (3, 0), edit_result.end_location);

        assert_eq!(
            lines_of(&wrapped, &document),
            vec![
                vec!["123 4567"],
                vec!["12345"],
                vec!["123456789"],
                vec!["012 3456"],
                vec!["78 90123"],
                vec!["45"],
            ]
        );
    }

    #[test]
    fn test_offset_to_location_wrapping_enabled() {
        let (document, wrapped) = make(SIMPLE_TEXT, 4);
        let cases: &[((isize, isize), Location)] = &[
            ((0, 0), (0, 0)),
            ((1, 0), (0, 1)),
            ((2, 1), (0, 6)),
            ((0, 3), (1, 4)),
            ((1, 3), (1, 5)),
            ((200, 3), (1, 5)),
            ((0, 6), (2, 8)),
            ((0, 7), (3, 0)), // Clicking on the final, empty line.
            ((0, 1000), (3, 0)),
        ];
        for &((x, y), location) in cases {
            assert_eq!(
                wrapped.offset_to_location(&document, x, y),
                location,
                "offset ({x}, {y})"
            );
        }
    }

    #[test]
    fn test_offset_to_location_wrapping_disabled() {
        let (document, wrapped) = make(SIMPLE_TEXT, 0);
        let cases: &[((isize, isize), Location)] = &[
            ((0, 0), (0, 0)),
            ((1, 0), (0, 1)),
            ((2, 1), (1, 2)),
            ((0, 3), (3, 0)),
            ((1, 3), (3, 0)),
            ((200, 3), (3, 0)),
            ((200, 200), (3, 0)), // Clicking below the document.
        ];
        for &((x, y), location) in cases {
            assert_eq!(
                wrapped.offset_to_location(&document, x, y),
                location,
                "offset ({x}, {y})"
            );
        }
    }

    #[test]
    fn test_offset_to_location_invalid_offset_clamps_to_valid_offset() {
        let (document, wrapped) = make(SIMPLE_TEXT, 4);
        assert_eq!(wrapped.offset_to_location(&document, -3, 0), (0, 0));
        assert_eq!(wrapped.offset_to_location(&document, 0, -10), (0, 0));
    }

    #[test]
    fn test_get_offsets() {
        let (_document, wrapped) = make(SIMPLE_TEXT, 4);
        assert_eq!(wrapped.get_offsets(0), Some(&[4usize][..]));
        assert_eq!(wrapped.get_offsets(1), Some(&[4usize][..]));
        assert_eq!(wrapped.get_offsets(2), Some(&[4usize, 8][..]));
    }

    #[test]
    fn test_get_offsets_no_wrapping() {
        let (_document, wrapped) = make("abc", 4);
        assert_eq!(wrapped.get_offsets(0), Some(&[][..]));
    }

    #[test]
    fn test_get_offsets_invalid_line_index() {
        // Python raises ValueError; Rust returns None.
        let (_document, wrapped) = make(SIMPLE_TEXT, 4);
        assert_eq!(wrapped.get_offsets(10_000), None);
    }

    #[test]
    fn location_to_offset_round_trip() {
        let (document, wrapped) = make(SIMPLE_TEXT, 4);
        // "123456789" wrapped as 1234 / 5678 / 9 (its sections are visual
        // rows 4..=6): column 5 sits on the second section at x=1, y=5.
        assert_eq!(wrapped.location_to_offset(&document, (2, 5)), (1, 5));
        // Line 0 "123 4567": column 6 is on section 1 at x=2, y=1.
        assert_eq!(wrapped.location_to_offset(&document, (0, 6)), (2, 1));
        assert_eq!(wrapped.location_to_offset(&document, (0, 0)), (0, 0));
    }

    #[test]
    fn height_counts_sections() {
        let (_document, wrapped) = make(SIMPLE_TEXT, 4);
        assert_eq!(wrapped.height(), 8);
        let (_document, unwrapped) = make(SIMPLE_TEXT, 0);
        assert_eq!(unwrapped.height(), 4);
    }

    #[test]
    fn byte_index_from_cell_x_matches_python_cell_width_to_column_index() {
        // Property pin (spec section 6.4): the Rust hit-test helper agrees
        // with a direct port of Python `cell_width_to_column_index` over
        // mixed ASCII/wide/combining lines (byte-index equivalent).
        use crate::document::graphemes::grapheme_cell_width;
        use unicode_segmentation::UnicodeSegmentation;

        fn python_cell_width_to_column_index(line: &str, cell_width: usize) -> usize {
            // Direct port over grapheme clusters, returning byte index.
            let mut total_cell_offset = 0usize;
            for (byte_index, grapheme) in line.grapheme_indices(true) {
                total_cell_offset += grapheme_cell_width(grapheme);
                if total_cell_offset > cell_width {
                    return byte_index;
                }
            }
            line.len()
        }

        let lines = [
            "hello world",
            "\u{597D}\u{597D}abc", // wide CJK
            "a\u{0301}bc\u{0301}", // combining
            "mixed \u{597D} text a\u{0301}",
            "tab\there",
            "",
        ];
        for line in lines {
            for x in 0..24usize {
                assert_eq!(
                    byte_index_from_cell_x(line, x),
                    python_cell_width_to_column_index(line, x),
                    "line {line:?} x {x}"
                );
            }
        }
    }

    #[test]
    fn tab_document_wraps_hit_tests_and_offsets_consistently() {
        // Degenerate-tab consistency (spec section 6.7): wrap offsets,
        // offset<->location mapping all treat '\t' as 1 cell.
        let document = Document::new("ab\tcd\tef");
        let wrapped = WrappedDocument::new(&document, 4, 4);
        // Tabs are whitespace, so they end word chunks: sections "ab\t" /
        // "cd\t" / "ef" at 3/3/2 cells under the 1-cell tab model.
        assert_eq!(wrapped.get_offsets(0), Some(&[3usize, 6][..]));
        // Hit-testing: x=2 on row 0 is the tab cell itself.
        assert_eq!(wrapped.offset_to_location(&document, 2, 0), (0, 2));
        // Location -> offset round-trips with the 1-cell tab.
        assert_eq!(wrapped.location_to_offset(&document, (0, 3)), (0, 1));
        assert_eq!(wrapped.location_to_offset(&document, (0, 5)), (2, 1));
    }

    #[test]
    fn get_target_document_column_never_splits_a_cluster() {
        // The non-final-section clamp steps back one GRAPHEME, not one byte
        // (spec 3.2, third -1 site).
        let emoji = "\u{1F469}\u{200D}\u{1F680}"; // 2 cells, 11 bytes
        let text = format!("ab{emoji}xy");
        let document = Document::new(&text);
        let wrapped = WrappedDocument::new(&document, 4, 4);
        // Sections: "ab" + emoji (4 cells), then "xy".
        assert_eq!(wrapped.get_offsets(0), Some(&[2 + emoji.len()][..]));
        // A large x on section 0 clamps to the START of the emoji cluster.
        let col = wrapped.get_target_document_column(&document, 0, 100, 0);
        assert_eq!(col, 2);
    }
}
