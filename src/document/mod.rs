//! Document model for text-editing widgets.
//!
//! Rust port of Python Textual's `textual.document` package: the [`Document`]
//! text storage with its single [`Document::replace_range`] mutation
//! primitive, plus the shared location/selection types used by `TextArea`.
//!
//! This module is a framework primitive and must not depend on
//! `crate::widgets`. `TextArea` re-exports the types its public API needs.
//!
//! # Location semantics (deviation from Python)
//!
//! Python Textual locations are `(row, codepoint column)`. Rust locations are
//! `(row, byte column within the line)`, kept on grapheme cluster boundaries
//! by clamping at every [`Document`] boundary. Byte columns give O(1) line
//! slicing and are the native currency of the grapheme helpers in
//! [`graphemes`]. For pure-ASCII text the two coincide.

#[allow(clippy::module_inception)]
mod document;
mod edit;
pub mod graphemes;
mod history;
mod navigator;
pub mod wrap;
mod wrapped;

pub use document::Document;
pub use edit::Edit;
pub use history::{EditHistory, HistoryClock, MockClock, MonotonicClock};
pub use navigator::DocumentNavigator;
pub use wrapped::WrappedDocument;

/// A location `(row, byte column)` within a document. Indexing starts at 0.
///
/// Deviation from Python Textual: the column is a byte offset within the
/// line (not a codepoint index), clamped to grapheme cluster boundaries by
/// all [`Document`] methods. For ASCII content the values coincide with
/// Python's.
pub type Location = (usize, usize);

/// Contains information about an edit that has occurred.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditResult {
    /// The new end location after the edit is complete.
    pub end_location: Location,
    /// The text that was replaced.
    pub replaced_text: String,
}

/// A cursor position within a document: `(row, byte column)`.
///
/// Representation-identical to [`Location`]; kept as a named struct for the
/// `TextArea` public API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, PartialOrd, Ord)]
pub struct Cursor {
    pub row: usize,
    /// Byte index within the line, always on a grapheme cluster boundary.
    pub col: usize,
}

impl Cursor {
    /// Convert to a `(row, column)` document location.
    pub fn location(self) -> Location {
        (self.row, self.col)
    }
}

impl From<Location> for Cursor {
    fn from((row, col): Location) -> Self {
        Self { row, col }
    }
}

impl From<Cursor> for Location {
    fn from(cursor: Cursor) -> Self {
        (cursor.row, cursor.col)
    }
}

/// A range of characters within a document from a start point to the end
/// point. The location of the cursor is always considered to be the `end`
/// point of the selection. The selection is inclusive of the minimum point
/// and exclusive of the maximum point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Selection {
    /// Where the selection started (anchor point).
    pub start: Cursor,
    /// Where the selection ends (the cursor location).
    pub end: Cursor,
}

impl Selection {
    /// Create a Selection with the same start and end point: a "cursor".
    pub fn cursor(pos: Cursor) -> Self {
        Self {
            start: pos,
            end: pos,
        }
    }

    /// True if the selection has zero width, i.e. it is just a cursor.
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
}

/// Detect the document's newline style (Python `_detect_newline_style`):
/// `"\r\n"` (Windows) wins over `"\n"` (Unix), then `"\r"` (old MacOS),
/// defaulting to `"\n"`.
pub fn detect_newline_style(text: &str) -> &'static str {
    if text.contains("\r\n") {
        "\r\n"
    } else if text.contains('\n') {
        "\n"
    } else if text.contains('\r') {
        "\r"
    } else {
        "\n"
    }
}

/// Split text into lines on any of `\r\n`, `\n`, `\r` (Python
/// `str.splitlines` over `VALID_NEWLINES`). A trailing newline yields a
/// trailing empty line, mirroring Python's `Document.__init__`; empty text
/// yields a single empty line.
pub fn split_lines(text: &str) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\n' => lines.push(std::mem::take(&mut current)),
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                lines.push(std::mem::take(&mut current));
            }
            _ => current.push(ch),
        }
    }
    lines.push(current);
    lines
}
