//! The `Edit` delta (port of Python `textual/document/_edit.py`).

use super::{Document, EditResult, Location, Selection};

/// A single undoable replacement of text at some range within a document.
///
/// Borrow shape (deviation from Python, which passes the whole `TextArea`):
/// [`Edit::apply`] and [`Edit::undo`] take `(&mut Document, Selection)` and
/// record the selection intent in [`Edit::updated_selection`]; the `TextArea`
/// edit funnel owns applying that selection after re-wrap, preserving the
/// Python ordering (edit, wrap_range, then selection restore).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    /// The text to insert. An empty string is equivalent to deletion.
    pub text: String,
    /// The start location of the insert.
    pub from_location: Location,
    /// The end location of the insert.
    pub to_location: Location,
    /// If true, the selection will maintain its offset to the replacement
    /// range; otherwise the selection collapses to a cursor at the edit's
    /// end location.
    pub maintain_selection_offset: bool,
    /// The selection when the edit was originally performed, restored on
    /// undo.
    original_selection: Option<Selection>,
    /// Where the selection should move to after the replace happens.
    updated_selection: Option<Selection>,
    /// The result of performing the edit (from the original `apply`; never
    /// overwritten by `undo`).
    edit_result: Option<EditResult>,
}

impl Edit {
    /// Create an edit replacing `from_location..to_location` with `text`.
    pub fn new(
        text: impl Into<String>,
        from_location: Location,
        to_location: Location,
        maintain_selection_offset: bool,
    ) -> Self {
        Self {
            text: text.into(),
            from_location,
            to_location,
            maintain_selection_offset,
            original_selection: None,
            updated_selection: None,
            edit_result: None,
        }
    }

    /// The location impacted by this edit nearest the document start.
    pub fn top(&self) -> Location {
        self.from_location.min(self.to_location)
    }

    /// The location impacted by this edit nearest the document end.
    pub fn bottom(&self) -> Location {
        self.from_location.max(self.to_location)
    }

    /// The result of the original [`Edit::apply`], if performed.
    pub fn edit_result(&self) -> Option<&EditResult> {
        self.edit_result.as_ref()
    }

    /// Where the selection should move to after this edit (or after undo).
    pub fn updated_selection(&self) -> Option<Selection> {
        self.updated_selection
    }

    /// The selection recorded when the edit was originally performed.
    pub fn original_selection(&self) -> Option<Selection> {
        self.original_selection
    }

    /// Perform the edit operation (Python `Edit.do`).
    ///
    /// `selection` is the widget's current selection; with
    /// `record_selection` it is recorded for restoration on undo.
    pub fn apply(
        &mut self,
        document: &mut Document,
        selection: Selection,
        record_selection: bool,
    ) -> EditResult {
        if record_selection {
            self.original_selection = Some(selection);
        }

        // This code is mostly handling how we adjust the selection when an
        // edit is made to the document programmatically. We want a user who
        // is typing away to maintain their relative position in the document
        // even if an insert happens before their cursor position.
        let (edit_bottom_row, edit_bottom_column) = self.bottom();
        let (selection_start_row, selection_start_column) = selection.start.location();
        let (selection_end_row, selection_end_column) = selection.end.location();

        let edit_result = document.replace_range(self.top(), self.bottom(), &self.text);
        let (new_edit_to_row, new_edit_to_column) = edit_result.end_location;

        let column_offset = new_edit_to_column as isize - edit_bottom_column as isize;
        let target_selection_start_column = if edit_bottom_row == selection_start_row
            && edit_bottom_column <= selection_start_column
        {
            (selection_start_column as isize + column_offset).max(0) as usize
        } else {
            selection_start_column
        };
        let target_selection_end_column =
            if edit_bottom_row == selection_end_row && edit_bottom_column <= selection_end_column {
                (selection_end_column as isize + column_offset).max(0) as usize
            } else {
                selection_end_column
            };

        let row_offset = new_edit_to_row as isize - edit_bottom_row as isize;
        let target_selection_start_row = if edit_bottom_row <= selection_start_row {
            (selection_start_row as isize + row_offset).max(0) as usize
        } else {
            selection_start_row
        };
        let target_selection_end_row = if edit_bottom_row <= selection_end_row {
            (selection_end_row as isize + row_offset).max(0) as usize
        } else {
            selection_end_row
        };

        if self.maintain_selection_offset {
            self.updated_selection = Some(Selection {
                start: (target_selection_start_row, target_selection_start_column).into(),
                end: (target_selection_end_row, target_selection_end_column).into(),
            });
        } else {
            self.updated_selection = Some(Selection::cursor(edit_result.end_location.into()));
        }

        self.edit_result = Some(edit_result.clone());
        edit_result
    }

    /// Undo the edit operation: performs the inverse replace and returns a
    /// fresh `EditResult` WITHOUT overwriting the stored [`Edit::edit_result`]
    /// (batch replay reads the original end location; Python parity pin).
    ///
    /// # Panics
    ///
    /// Panics if the edit has not been performed via [`Edit::apply`] yet
    /// (Python raises `HistoryException` on the equivalent misuse).
    pub fn undo(&mut self, document: &mut Document) -> EditResult {
        let stored = self
            .edit_result
            .as_ref()
            .expect("Cannot undo an Edit before it has been performed via `Edit::apply`");
        let replaced_text = stored.replaced_text.clone();
        let edit_end = stored.end_location;

        // Replace the span of the edit with the text that was originally there.
        let undo_edit_result = document.replace_range(self.top(), edit_end, &replaced_text);
        self.updated_selection = self.original_selection;

        undo_edit_result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_records_result_and_collapsed_selection() {
        let mut document = Document::new("hello");
        let mut edit = Edit::new("XY", (0, 1), (0, 3), false);
        let selection = Selection::cursor((0, 5).into());
        let result = edit.apply(&mut document, selection, true);
        assert_eq!(document.text(), "hXYlo");
        assert_eq!(result.end_location, (0, 3));
        assert_eq!(result.replaced_text, "el");
        assert_eq!(edit.original_selection(), Some(selection));
        assert_eq!(
            edit.updated_selection(),
            Some(Selection::cursor((0, 3).into()))
        );
    }

    #[test]
    fn apply_maintains_selection_offset() {
        let mut document = Document::new("hello");
        // Insert before the cursor; cursor should shift right by the insert width.
        let mut edit = Edit::new("AB", (0, 0), (0, 0), true);
        let selection = Selection::cursor((0, 2).into());
        edit.apply(&mut document, selection, true);
        assert_eq!(document.text(), "ABhello");
        assert_eq!(
            edit.updated_selection(),
            Some(Selection::cursor((0, 4).into()))
        );
    }

    #[test]
    fn undo_restores_text_and_does_not_overwrite_edit_result() {
        let mut document = Document::new("hello\nworld");
        let mut edit = Edit::new("", (0, 3), (1, 2), false);
        let selection = Selection {
            start: (0, 3).into(),
            end: (1, 2).into(),
        };
        edit.apply(&mut document, selection, true);
        assert_eq!(document.text(), "helrld");
        let stored = edit.edit_result().cloned().unwrap();

        let undo_result = edit.undo(&mut document);
        assert_eq!(document.text(), "hello\nworld");
        assert_eq!(undo_result.end_location, (1, 2));
        // The stored result from the original apply is untouched.
        assert_eq!(edit.edit_result(), Some(&stored));
        // Undo restores the original selection intent.
        assert_eq!(edit.updated_selection(), Some(selection));
    }
}
