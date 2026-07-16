//! The `Document` text storage (port of Python `textual/document/_document.py`).

use super::graphemes::{cell_len, clamp_grapheme_boundary};
use super::{EditResult, Location, detect_newline_style, split_lines};

/// A document which can be opened in a `TextArea`.
///
/// The single mutation primitive is [`Document::replace_range`]; everything
/// else is read-only. Lines are stored without terminators.
///
/// LF-canonical syntax-source invariant: `lines` never contain newline
/// characters, so a syntax source built by joining them with `"\n"` (as
/// `TextArea::recompute_syntax_cache` does) is stable regardless of the
/// document's own [`Document::newline`] style, which affects only
/// [`Document::text`] read-back, [`Document::get_text_range`] joins, and
/// index<->location arithmetic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    /// The lines of the document, excluding newline characters.
    ///
    /// If there is a newline at the end of the text, the final line is an
    /// empty string.
    lines: Vec<String>,
    /// The newline style used by the document (detected from the initial
    /// text, preserved on `text()` read-back).
    newline: &'static str,
}

impl Document {
    /// Build a document from text, detecting the newline style.
    pub fn new(text: &str) -> Self {
        Self {
            newline: detect_newline_style(text),
            lines: split_lines(text),
        }
    }

    /// The text of the document, joined with the document's newline.
    pub fn text(&self) -> String {
        self.lines.join(self.newline)
    }

    /// The newline style used by this document: `"\n"`, `"\r\n"` or `"\r"`.
    pub fn newline(&self) -> &'static str {
        self.newline
    }

    /// The lines of the document (no terminators).
    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    /// The line at `index`, or `""` when out of bounds.
    pub fn line(&self, index: usize) -> &str {
        self.lines.get(index).map(String::as_str).unwrap_or("")
    }

    /// The number of lines in the document (always at least 1).
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// The location of the start of the document: `(0, 0)`.
    pub fn start(&self) -> Location {
        (0, 0)
    }

    /// The location of the end of the document.
    pub fn end(&self) -> Location {
        let last_line = self.lines.last().map(String::as_str).unwrap_or("");
        (self.line_count() - 1, last_line.len())
    }

    /// Replace text at the given range.
    ///
    /// This is the only method by which a document may be updated. Incoming
    /// columns are clamped to grapheme cluster boundaries (a deliberate
    /// strengthening over Python, which allows splitting surrogate-free
    /// codepoint runs anywhere).
    pub fn replace_range(&mut self, start: Location, end: Location, text: &str) -> EditResult {
        let (top, bottom) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };
        let top = self.clamp_column(top);
        let bottom = self.clamp_column(bottom);
        let (top_row, top_column) = top;
        let (bottom_row, bottom_column) = bottom;

        // Split the inserted text on any valid newline; a trailing newline
        // adds a trailing empty line. Empty text yields no lines.
        let mut insert_lines: Vec<String> = if text.is_empty() {
            Vec::new()
        } else {
            split_lines(text)
        };

        let replaced_text = self.get_text_range(top, bottom);
        let after_selection = if bottom_row >= self.lines.len() {
            String::new()
        } else {
            self.lines[bottom_row][bottom_column..].to_string()
        };
        let before_selection = if top_row >= self.lines.len() {
            String::new()
        } else {
            self.lines[top_row][..top_column].to_string()
        };

        let destination_column;
        if !insert_lines.is_empty() {
            insert_lines[0] = format!("{before_selection}{}", insert_lines[0]);
            destination_column = insert_lines.last().map(String::len).unwrap_or(0);
            insert_lines
                .last_mut()
                .expect("insert_lines is non-empty")
                .push_str(&after_selection);
        } else {
            destination_column = before_selection.len();
            insert_lines = vec![format!("{before_selection}{after_selection}")];
        }

        // Python list-slice assignment clamps out-of-range indices.
        let splice_start = top_row.min(self.lines.len());
        let splice_end = (bottom_row + 1).min(self.lines.len()).max(splice_start);
        let inserted_count = insert_lines.len();
        self.lines.splice(splice_start..splice_end, insert_lines);
        let destination_row = top_row + inserted_count - 1;

        EditResult {
            end_location: (destination_row, destination_column),
            replaced_text,
        }
    }

    /// Get the text between `start` (inclusive) and `end` (exclusive),
    /// joining interior lines with the document's own newline.
    pub fn get_text_range(&self, start: Location, end: Location) -> String {
        if start == end {
            return String::new();
        }
        let (top, bottom) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };
        let (top_row, top_column) = self.clamp_column(top);
        let (bottom_row, bottom_column) = self.clamp_column(bottom);
        if top_row >= self.lines.len() {
            return String::new();
        }
        let line_count = self.line_count();
        if top_row == bottom_row {
            let line = &self.lines[top_row];
            return line[top_column..bottom_column.min(line.len())].to_string();
        }
        let mut selected_text = self.lines[top_row][top_column..].to_string();
        for row in top_row + 1..bottom_row.min(line_count) {
            selected_text.push_str(self.newline);
            selected_text.push_str(&self.lines[row]);
        }
        if bottom_row < line_count {
            selected_text.push_str(self.newline);
            selected_text.push_str(&self.lines[bottom_row][..bottom_column]);
        }
        selected_text
    }

    /// Given a location, return the byte index into [`Document::text`]
    /// (newline length counted per row).
    ///
    /// Deviation from Python: this is a byte index, not a codepoint index,
    /// consistent with byte-column locations.
    pub fn get_index_from_location(&self, location: Location) -> usize {
        let (row, column) = location;
        let mut index = row * self.newline.len() + column;
        for line in self.lines.iter().take(row) {
            index += line.len();
        }
        index
    }

    /// Given a byte index into [`Document::text`], return the corresponding
    /// location, or `None` when the index is out of bounds.
    pub fn get_location_from_index(&self, index: usize) -> Option<Location> {
        let newline_length = self.newline.len();
        let text_length = self
            .lines
            .iter()
            .map(String::len)
            .sum::<usize>()
            .saturating_add((self.lines.len() - 1) * newline_length);
        if index > text_length {
            return None;
        }
        let mut column_index = 0usize;
        for (line_index, line) in self.lines.iter().enumerate() {
            let next_column_index = column_index + line.len() + newline_length;
            if index < next_column_index {
                return Some((line_index, index - column_index));
            } else if index == next_column_index {
                return Some((line_index + 1, 0));
            }
            column_index = next_column_index;
        }
        None
    }

    /// The size of the document as `(max cell width, line count)`.
    ///
    /// The pinned degenerate tab model counts `'\t'` as 1 cell regardless of
    /// `tab_width` (consistent with wrap offsets and rendering); the
    /// parameter is kept for Python API parity until full tab expansion
    /// flips all consumers at once.
    pub fn get_size(&self, tab_width: usize) -> (usize, usize) {
        let _ = tab_width;
        let max_cell_length = self
            .lines
            .iter()
            .map(|line| cell_len(line))
            .max()
            .unwrap_or(0);
        (max_cell_length, self.lines.len())
    }

    /// Clamp a location's column to a grapheme boundary within its line.
    /// Rows are left untouched (Python allows out-of-range rows in
    /// `replace_range`, which append at the end of the document).
    fn clamp_column(&self, (row, col): Location) -> Location {
        match self.lines.get(row) {
            Some(line) => (row, clamp_grapheme_boundary(line, col)),
            None => (row, col),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = "I must not fear.\nFear is the mind-killer.";

    fn text_newline() -> String {
        format!("{TEXT}\n")
    }

    fn text_windows() -> String {
        TEXT.replace('\n', "\r\n")
    }

    fn text_windows_newline() -> String {
        text_newline().replace('\n', "\r\n")
    }

    fn variants() -> Vec<String> {
        vec![
            TEXT.to_string(),
            text_newline(),
            text_windows(),
            text_windows_newline(),
        ]
    }

    // ── test_document.py ────────────────────────────────────────────────

    #[test]
    fn test_text() {
        for text in variants() {
            let document = Document(&text);
            assert_eq!(document.text(), text);
        }
    }

    #[allow(non_snake_case)]
    fn Document(text: &str) -> super::Document {
        super::Document::new(text)
    }

    #[test]
    fn test_lines_newline_eof() {
        let document = Document(&text_newline());
        assert_eq!(
            document.lines(),
            ["I must not fear.", "Fear is the mind-killer.", ""]
        );
    }

    #[test]
    fn test_lines_no_newline_eof() {
        let document = Document(TEXT);
        assert_eq!(
            document.lines(),
            ["I must not fear.", "Fear is the mind-killer."]
        );
    }

    #[test]
    fn test_lines_windows() {
        let document = Document(&text_windows());
        assert_eq!(
            document.lines(),
            ["I must not fear.", "Fear is the mind-killer."]
        );
    }

    #[test]
    fn test_lines_windows_newline() {
        let document = Document(&text_windows_newline());
        assert_eq!(
            document.lines(),
            ["I must not fear.", "Fear is the mind-killer.", ""]
        );
    }

    #[test]
    fn test_newline_unix() {
        assert_eq!(Document(TEXT).newline(), "\n");
    }

    #[test]
    fn test_newline_windows() {
        assert_eq!(Document(&text_windows()).newline(), "\r\n");
    }

    #[test]
    fn test_get_selected_text_no_selection() {
        let document = Document(TEXT);
        assert_eq!(document.get_text_range((0, 0), (0, 0)), "");
    }

    #[test]
    fn test_get_selected_text_single_line() {
        let document = Document(&text_windows());
        assert_eq!(document.get_text_range((0, 2), (0, 6)), "must");
    }

    #[test]
    fn test_get_selected_text_multiple_lines_unix() {
        let document = Document(TEXT);
        assert_eq!(
            document.get_text_range((0, 2), (1, 2)),
            "must not fear.\nFe"
        );
    }

    #[test]
    fn test_get_selected_text_multiple_lines_windows() {
        let document = Document(&text_windows());
        assert_eq!(
            document.get_text_range((0, 2), (1, 2)),
            "must not fear.\r\nFe"
        );
    }

    #[test]
    fn test_get_selected_text_including_final_newline_unix() {
        let document = Document(&text_newline());
        assert_eq!(document.get_text_range((0, 0), (2, 0)), text_newline());
    }

    #[test]
    fn test_get_selected_text_including_final_newline_windows() {
        let document = Document(&text_windows_newline());
        assert_eq!(
            document.get_text_range((0, 0), (2, 0)),
            text_windows_newline()
        );
    }

    #[test]
    fn test_get_selected_text_no_newline_at_end_of_file() {
        let document = Document(TEXT);
        assert_eq!(document.get_text_range((0, 0), (2, 0)), TEXT);
    }

    #[test]
    fn test_get_selected_text_no_newline_at_end_of_file_windows() {
        let document = Document(&text_windows());
        assert_eq!(document.get_text_range((0, 0), (2, 0)), text_windows());
    }

    #[test]
    fn test_index_from_location() {
        for text in variants() {
            let document = Document(&text);
            let lines: Vec<&str> = text.split(document.newline()).collect();
            assert_eq!(document.get_index_from_location((0, 0)), 0);
            assert_eq!(
                document.get_index_from_location((0, lines[0].len())),
                lines[0].len()
            );
            assert_eq!(
                document.get_index_from_location((1, 0)),
                lines[0].len() + document.newline().len()
            );
            assert_eq!(
                document.get_index_from_location((lines.len() - 1, lines[lines.len() - 1].len())),
                text.len()
            );
        }
    }

    #[test]
    fn test_location_from_index() {
        for text in variants() {
            let document = Document(&text);
            let lines: Vec<&str> = text.split(document.newline()).collect();
            assert_eq!(document.get_location_from_index(0), Some((0, 0)));
            assert_eq!(
                document.get_location_from_index(lines[0].len()),
                Some((0, lines[0].len()))
            );
            if document.newline().len() > 1 {
                assert_eq!(
                    document.get_location_from_index(lines[0].len() + 1),
                    Some((0, lines[0].len() + 1))
                );
            }
            assert_eq!(
                document.get_location_from_index(lines[0].len() + document.newline().len()),
                Some((1, 0))
            );
            assert_eq!(
                document.get_location_from_index(text.len()),
                Some((lines.len() - 1, lines[lines.len() - 1].len()))
            );
            assert_eq!(document.get_location_from_index(text.len() + 1), None);
        }
    }

    #[test]
    fn test_document_end() {
        for text in variants() {
            let document = Document(&text);
            let line_count = split_lines(&text).len();
            let expected = if text.ends_with('\n') {
                (line_count - 1, 0)
            } else {
                (
                    line_count - 1,
                    split_lines(&text).last().map(String::len).unwrap_or(0),
                )
            };
            assert_eq!(document.end(), expected);
        }
    }

    // ── test_document_insert.py ─────────────────────────────────────────

    #[test]
    fn test_insert_no_newlines() {
        let mut document = Document(TEXT);
        document.replace_range((0, 1), (0, 1), " really");
        assert_eq!(
            document.lines(),
            ["I really must not fear.", "Fear is the mind-killer."]
        );
    }

    #[test]
    fn test_insert_empty_string() {
        let mut document = Document(TEXT);
        document.replace_range((0, 1), (0, 1), "");
        assert_eq!(
            document.lines(),
            ["I must not fear.", "Fear is the mind-killer."]
        );
    }

    #[test]
    fn test_insert_invalid_column() {
        let mut document = Document(TEXT);
        document.replace_range((0, 999), (0, 999), " really");
        assert_eq!(
            document.lines(),
            ["I must not fear. really", "Fear is the mind-killer."]
        );
    }

    #[test]
    fn test_insert_invalid_row_and_column() {
        let mut document = Document(TEXT);
        document.replace_range((999, 0), (999, 0), " really");
        assert_eq!(
            document.lines(),
            ["I must not fear.", "Fear is the mind-killer.", " really"]
        );
    }

    #[test]
    fn test_insert_range_newline_file_start() {
        let mut document = Document(TEXT);
        document.replace_range((0, 0), (0, 0), "\n");
        assert_eq!(
            document.lines(),
            ["", "I must not fear.", "Fear is the mind-killer."]
        );
    }

    #[test]
    fn test_insert_newline_splits_line() {
        let mut document = Document(TEXT);
        document.replace_range((0, 1), (0, 1), "\n");
        assert_eq!(
            document.lines(),
            ["I", " must not fear.", "Fear is the mind-killer."]
        );
    }

    #[test]
    fn test_insert_newline_splits_line_selection() {
        let mut document = Document(TEXT);
        document.replace_range((0, 1), (0, 6), "\n");
        assert_eq!(
            document.lines(),
            ["I", " not fear.", "Fear is the mind-killer."]
        );
    }

    #[test]
    fn test_insert_multiple_lines_ends_with_newline() {
        let mut document = Document(TEXT);
        document.replace_range((0, 1), (0, 1), "Hello,\nworld!\n");
        assert_eq!(
            document.lines(),
            [
                "IHello,",
                "world!",
                " must not fear.",
                "Fear is the mind-killer."
            ]
        );
    }

    #[test]
    fn test_insert_multiple_lines_ends_with_no_newline() {
        let mut document = Document(TEXT);
        document.replace_range((0, 1), (0, 1), "Hello,\nworld!");
        assert_eq!(
            document.lines(),
            [
                "IHello,",
                "world! must not fear.",
                "Fear is the mind-killer."
            ]
        );
    }

    #[test]
    fn test_insert_multiple_lines_starts_with_newline() {
        let mut document = Document(TEXT);
        document.replace_range((0, 1), (0, 1), "\nHello,\nworld!\n");
        assert_eq!(
            document.lines(),
            [
                "I",
                "Hello,",
                "world!",
                " must not fear.",
                "Fear is the mind-killer."
            ]
        );
    }

    #[test]
    fn test_insert_range_text_no_newlines() {
        let mut document = Document(TEXT);
        document.replace_range((0, 2), (0, 6), "MUST");
        assert_eq!(
            document.lines(),
            ["I MUST not fear.", "Fear is the mind-killer."]
        );
    }

    #[test]
    fn test_newline_eof() {
        let document = Document("I must not fear.\nFear is the mind-killer.\n");
        assert_eq!(
            document.lines(),
            ["I must not fear.", "Fear is the mind-killer.", ""]
        );
    }

    // ── test_document_delete.py ─────────────────────────────────────────

    const DELETE_TEXT: &str =
        "I must not fear.\nFear is the mind-killer.\nI forgot the rest of the quote.\nSorry Will.";

    #[test]
    fn test_delete_single_character() {
        let mut document = Document(DELETE_TEXT);
        let result = document.replace_range((0, 0), (0, 1), "");
        assert_eq!(
            result,
            EditResult {
                end_location: (0, 0),
                replaced_text: "I".to_string()
            }
        );
        assert_eq!(
            document.lines(),
            [
                " must not fear.",
                "Fear is the mind-killer.",
                "I forgot the rest of the quote.",
                "Sorry Will."
            ]
        );
    }

    #[test]
    fn test_delete_single_newline() {
        // Deleting a newline from right to left.
        let mut document = Document(DELETE_TEXT);
        let result = document.replace_range((1, 0), (0, 16), "");
        assert_eq!(
            result,
            EditResult {
                end_location: (0, 16),
                replaced_text: "\n".to_string()
            }
        );
        assert_eq!(
            document.lines(),
            [
                "I must not fear.Fear is the mind-killer.",
                "I forgot the rest of the quote.",
                "Sorry Will."
            ]
        );
    }

    #[test]
    fn test_delete_near_end_of_document() {
        let mut document = Document(DELETE_TEXT);
        let result = document.replace_range((1, 0), (3, 11), "");
        assert_eq!(
            result,
            EditResult {
                end_location: (1, 0),
                replaced_text:
                    "Fear is the mind-killer.\nI forgot the rest of the quote.\nSorry Will."
                        .to_string()
            }
        );
        assert_eq!(document.lines(), ["I must not fear.", ""]);
    }

    #[test]
    fn test_delete_clearing_the_document() {
        let mut document = Document(DELETE_TEXT);
        let result = document.replace_range((0, 0), (4, 0), "");
        assert_eq!(
            result,
            EditResult {
                end_location: (0, 0),
                replaced_text: DELETE_TEXT.to_string()
            }
        );
        assert_eq!(document.lines(), [""]);
    }

    #[test]
    fn test_delete_multiple_characters_on_one_line() {
        let mut document = Document(DELETE_TEXT);
        let result = document.replace_range((0, 2), (0, 7), "");
        assert_eq!(
            result,
            EditResult {
                end_location: (0, 2),
                replaced_text: "must ".to_string()
            }
        );
        assert_eq!(
            document.lines(),
            [
                "I not fear.",
                "Fear is the mind-killer.",
                "I forgot the rest of the quote.",
                "Sorry Will."
            ]
        );
    }

    #[test]
    fn test_delete_multiple_lines_partially_spanned() {
        let mut document = Document(DELETE_TEXT);
        let result = document.replace_range((0, 2), (2, 2), "");
        assert_eq!(
            result,
            EditResult {
                end_location: (0, 2),
                replaced_text: "must not fear.\nFear is the mind-killer.\nI ".to_string()
            }
        );
        assert_eq!(
            document.lines(),
            ["I forgot the rest of the quote.", "Sorry Will."]
        );
    }

    #[test]
    fn test_delete_end_of_line() {
        // Deleting a newline from left to right.
        let mut document = Document(DELETE_TEXT);
        let result = document.replace_range((0, 16), (1, 0), "");
        assert_eq!(
            result,
            EditResult {
                end_location: (0, 16),
                replaced_text: "\n".to_string()
            }
        );
        assert_eq!(
            document.lines(),
            [
                "I must not fear.Fear is the mind-killer.",
                "I forgot the rest of the quote.",
                "Sorry Will."
            ]
        );
    }

    #[test]
    fn test_delete_single_line_excluding_newline() {
        let mut document = Document(DELETE_TEXT);
        let result = document.replace_range((2, 0), (2, 31), "");
        assert_eq!(
            result,
            EditResult {
                end_location: (2, 0),
                replaced_text: "I forgot the rest of the quote.".to_string()
            }
        );
        assert_eq!(
            document.lines(),
            [
                "I must not fear.",
                "Fear is the mind-killer.",
                "",
                "Sorry Will."
            ]
        );
    }

    #[test]
    fn test_delete_single_line_including_newline() {
        let mut document = Document(DELETE_TEXT);
        let result = document.replace_range((2, 0), (3, 0), "");
        assert_eq!(
            result,
            EditResult {
                end_location: (2, 0),
                replaced_text: "I forgot the rest of the quote.\n".to_string()
            }
        );
        assert_eq!(
            document.lines(),
            [
                "I must not fear.",
                "Fear is the mind-killer.",
                "Sorry Will."
            ]
        );
    }

    #[test]
    fn test_delete_end_of_file_newline() {
        let mut document = Document("I must not fear.\nFear is the mind-killer.\n");
        let result = document.replace_range((2, 0), (1, 24), "");
        assert_eq!(
            result,
            EditResult {
                end_location: (1, 24),
                replaced_text: "\n".to_string()
            }
        );
        assert_eq!(
            document.lines(),
            ["I must not fear.", "Fear is the mind-killer."]
        );
    }

    // ── Grapheme clamping (Rust-specific: byte columns never split a cluster) ──

    #[test]
    fn test_replace_range_clamps_to_grapheme_boundary() {
        let mut document = Document("a\u{0301}bc");
        // Byte 1 is inside the "a<combining acute>" cluster; clamp to 0.
        let result = document.replace_range((0, 1), (0, 1), "X");
        assert_eq!(result.end_location, (0, 1));
        assert_eq!(document.lines(), ["Xa\u{0301}bc"]);
    }

    #[test]
    fn test_get_size_counts_tabs_as_one_cell() {
        let document = Document("a\tb\nxy");
        assert_eq!(document.get_size(4), (3, 2));
    }
}
