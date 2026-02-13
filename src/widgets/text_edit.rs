use crossterm::event::{KeyCode, KeyModifiers};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::keys::KeyEventData;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MoveUnit {
    Grapheme,
    Word,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditCommand {
    InsertChar(char),
    InsertNewline,
    Submit,
    Copy,
    Cut,
    Paste,
    MoveLeft { select: bool, unit: MoveUnit },
    MoveRight { select: bool, unit: MoveUnit },
    MoveUp { select: bool },
    MoveDown { select: bool },
    MoveHome { select: bool },
    MoveEnd { select: bool },
    Backspace { unit: MoveUnit },
    Delete { unit: MoveUnit },
    DeleteToStart,
    DeleteToEnd,
    DeleteLine,
    SelectAll,
    /// Used in text_area pattern matching for line selection.
    #[allow(dead_code)]
    SelectLine,
    Undo,
    Redo,
}

pub(crate) fn edit_command_from_key(key: &KeyEventData, multiline: bool) -> Option<EditCommand> {
    let mut mods_without_shift = key.modifiers;
    mods_without_shift.remove(KeyModifiers::SHIFT);

    let has_extra_modifiers =
        mods_without_shift.intersects(KeyModifiers::ALT | KeyModifiers::HYPER | KeyModifiers::META);
    let ctrl_shortcut = !has_extra_modifiers && mods_without_shift == KeyModifiers::CONTROL;
    let super_shortcut = !has_extra_modifiers && mods_without_shift == KeyModifiers::SUPER;
    let plain_or_shift = !has_extra_modifiers && mods_without_shift.is_empty();
    let has_text_blocking_modifier = key.modifiers.intersects(
        KeyModifiers::CONTROL
            | KeyModifiers::SUPER
            | KeyModifiers::ALT
            | KeyModifiers::HYPER
            | KeyModifiers::META,
    );
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);

    match key.code {
        KeyCode::Char('u') if ctrl_shortcut => Some(EditCommand::DeleteToStart),
        KeyCode::Char('d') if ctrl_shortcut => Some(EditCommand::Delete {
            unit: MoveUnit::Grapheme,
        }),
        KeyCode::Char('k') if ctrl_shortcut && !shift => Some(EditCommand::DeleteToEnd),
        KeyCode::Char('k') if ctrl_shortcut && shift => Some(EditCommand::DeleteLine),
        KeyCode::Char('f') if ctrl_shortcut => Some(EditCommand::MoveRight {
            select: shift,
            unit: MoveUnit::Grapheme,
        }),
        KeyCode::Char('a') if ctrl_shortcut => Some(EditCommand::SelectAll),
        KeyCode::Char('z') if ctrl_shortcut && !shift => Some(EditCommand::Undo),
        KeyCode::Char('z') if ctrl_shortcut && shift => Some(EditCommand::Redo),
        KeyCode::Char('y') if ctrl_shortcut => Some(EditCommand::Redo),
        KeyCode::Char(ch) if (ctrl_shortcut || super_shortcut) && ch.eq_ignore_ascii_case(&'x') => {
            Some(EditCommand::Cut)
        }
        KeyCode::Char(ch) if (ctrl_shortcut || super_shortcut) && ch.eq_ignore_ascii_case(&'c') => {
            Some(EditCommand::Copy)
        }
        KeyCode::Char(ch) if (ctrl_shortcut || super_shortcut) && ch.eq_ignore_ascii_case(&'v') => {
            Some(EditCommand::Paste)
        }
        KeyCode::Char(ch) if super_shortcut && ch.eq_ignore_ascii_case(&'a') => {
            Some(EditCommand::MoveHome { select: shift })
        }
        KeyCode::Char(ch) if super_shortcut && ch.eq_ignore_ascii_case(&'e') => {
            Some(EditCommand::MoveEnd { select: shift })
        }
        KeyCode::Char(_) if !has_text_blocking_modifier => key
            .character
            .filter(|_| key.is_printable)
            .map(EditCommand::InsertChar),
        KeyCode::Enter if plain_or_shift && multiline => Some(EditCommand::InsertNewline),
        KeyCode::Enter if plain_or_shift => Some(EditCommand::Submit),
        KeyCode::Insert if ctrl_shortcut => Some(EditCommand::Copy),
        KeyCode::Insert if shift && plain_or_shift => Some(EditCommand::Paste),
        KeyCode::Delete if shift && plain_or_shift => Some(EditCommand::Cut),
        KeyCode::Backspace if super_shortcut && !multiline => Some(EditCommand::DeleteToStart),
        KeyCode::Backspace if ctrl_shortcut => Some(EditCommand::Backspace {
            unit: MoveUnit::Word,
        }),
        KeyCode::Backspace
            if key.modifiers == KeyModifiers::ALT
                || key.modifiers == (KeyModifiers::ALT | KeyModifiers::SHIFT) =>
        {
            Some(EditCommand::Backspace {
                unit: MoveUnit::Word,
            })
        }
        KeyCode::Backspace if plain_or_shift => Some(EditCommand::Backspace {
            unit: MoveUnit::Grapheme,
        }),
        KeyCode::Delete if ctrl_shortcut => Some(EditCommand::Delete {
            unit: MoveUnit::Word,
        }),
        KeyCode::Delete
            if key.modifiers == KeyModifiers::ALT
                || key.modifiers == (KeyModifiers::ALT | KeyModifiers::SHIFT) =>
        {
            Some(EditCommand::Delete {
                unit: MoveUnit::Word,
            })
        }
        KeyCode::Delete if plain_or_shift => Some(EditCommand::Delete {
            unit: MoveUnit::Grapheme,
        }),
        KeyCode::Left if ctrl_shortcut => Some(EditCommand::MoveLeft {
            select: shift,
            unit: MoveUnit::Word,
        }),
        KeyCode::Left if plain_or_shift => Some(EditCommand::MoveLeft {
            select: shift,
            unit: MoveUnit::Grapheme,
        }),
        KeyCode::Left if super_shortcut => Some(EditCommand::MoveHome { select: shift }),
        KeyCode::Left
            if key.modifiers == KeyModifiers::ALT
                || key.modifiers == (KeyModifiers::ALT | KeyModifiers::SHIFT) =>
        {
            Some(EditCommand::MoveLeft {
                select: shift,
                unit: MoveUnit::Word,
            })
        }
        KeyCode::Right if ctrl_shortcut => Some(EditCommand::MoveRight {
            select: shift,
            unit: MoveUnit::Word,
        }),
        KeyCode::Right if plain_or_shift => Some(EditCommand::MoveRight {
            select: shift,
            unit: MoveUnit::Grapheme,
        }),
        KeyCode::Right if super_shortcut => Some(EditCommand::MoveEnd { select: shift }),
        KeyCode::Right
            if key.modifiers == KeyModifiers::ALT
                || key.modifiers == (KeyModifiers::ALT | KeyModifiers::SHIFT) =>
        {
            Some(EditCommand::MoveRight {
                select: shift,
                unit: MoveUnit::Word,
            })
        }
        KeyCode::Up if plain_or_shift => Some(EditCommand::MoveUp { select: shift }),
        KeyCode::Down if plain_or_shift => Some(EditCommand::MoveDown { select: shift }),
        KeyCode::Home if plain_or_shift => Some(EditCommand::MoveHome { select: shift }),
        KeyCode::End if plain_or_shift => Some(EditCommand::MoveEnd { select: shift }),
        _ => None,
    }
}

pub(crate) fn first_clipboard_line(text: &str) -> Option<&str> {
    let line_end = text.find(['\n', '\r']).unwrap_or(text.len());
    if line_end == 0 {
        return None;
    }
    text.get(..line_end)
}

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

pub(crate) fn prev_word_boundary(s: &str, idx: usize) -> usize {
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

pub(crate) fn next_word_boundary(s: &str, idx: usize) -> usize {
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
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

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
    fn key_mapping_handles_word_and_selection_commands() {
        let left_word = edit_command_from_key(
            &crate::keys::KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Left,
                KeyModifiers::CONTROL | KeyModifiers::SHIFT,
            )),
            false,
        );
        assert_eq!(
            left_word,
            Some(EditCommand::MoveLeft {
                select: true,
                unit: MoveUnit::Word
            })
        );
        let ctrl_u = edit_command_from_key(
            &crate::keys::KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Char('u'),
                KeyModifiers::CONTROL,
            )),
            false,
        );
        assert_eq!(ctrl_u, Some(EditCommand::DeleteToStart));
    }

    #[test]
    fn key_mapping_includes_clipboard_commands() {
        let copy = edit_command_from_key(
            &crate::keys::KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Char('c'),
                KeyModifiers::CONTROL,
            )),
            false,
        );
        assert_eq!(copy, Some(EditCommand::Copy));

        let cut = edit_command_from_key(
            &crate::keys::KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Char('x'),
                KeyModifiers::CONTROL,
            )),
            false,
        );
        assert_eq!(cut, Some(EditCommand::Cut));

        let paste = edit_command_from_key(
            &crate::keys::KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Char('v'),
                KeyModifiers::CONTROL,
            )),
            false,
        );
        assert_eq!(paste, Some(EditCommand::Paste));

        let cut_super = edit_command_from_key(
            &crate::keys::KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Char('x'),
                KeyModifiers::SUPER,
            )),
            false,
        );
        assert_eq!(cut_super, Some(EditCommand::Cut));

        let paste_super = edit_command_from_key(
            &crate::keys::KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Char('v'),
                KeyModifiers::SUPER,
            )),
            false,
        );
        assert_eq!(paste_super, Some(EditCommand::Paste));
    }

    #[test]
    fn key_mapping_ignores_clipboard_chords_with_extra_modifiers() {
        let alt_ctrl_v = edit_command_from_key(
            &crate::keys::KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Char('v'),
                KeyModifiers::CONTROL | KeyModifiers::ALT,
            )),
            false,
        );
        assert_eq!(alt_ctrl_v, None);

        let ctrl_super_c = edit_command_from_key(
            &crate::keys::KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Char('c'),
                KeyModifiers::CONTROL | KeyModifiers::SUPER,
            )),
            false,
        );
        assert_eq!(ctrl_super_c, None);
    }

    #[test]
    fn first_clipboard_line_handles_newline_variants() {
        assert_eq!(first_clipboard_line("hello\nworld"), Some("hello"));
        assert_eq!(first_clipboard_line("hello\r\nworld"), Some("hello"));
        assert_eq!(first_clipboard_line("hello\rworld"), Some("hello"));
        assert_eq!(first_clipboard_line("\nworld"), None);
        assert_eq!(first_clipboard_line(""), None);
    }

    #[test]
    fn key_mapping_supports_insert_delete_clipboard_chords() {
        let copy = edit_command_from_key(
            &crate::keys::KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Insert,
                KeyModifiers::CONTROL,
            )),
            false,
        );
        assert_eq!(copy, Some(EditCommand::Copy));

        let paste = edit_command_from_key(
            &crate::keys::KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Insert,
                KeyModifiers::SHIFT,
            )),
            false,
        );
        assert_eq!(paste, Some(EditCommand::Paste));

        let cut = edit_command_from_key(
            &crate::keys::KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Delete,
                KeyModifiers::SHIFT,
            )),
            false,
        );
        assert_eq!(cut, Some(EditCommand::Cut));
    }

    #[test]
    fn key_mapping_supports_alt_and_super_navigation_shortcuts() {
        let alt_left = edit_command_from_key(
            &crate::keys::KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Left,
                KeyModifiers::ALT,
            )),
            false,
        );
        assert_eq!(
            alt_left,
            Some(EditCommand::MoveLeft {
                select: false,
                unit: MoveUnit::Word
            })
        );

        let super_left = edit_command_from_key(
            &crate::keys::KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Left,
                KeyModifiers::SUPER,
            )),
            false,
        );
        assert_eq!(super_left, Some(EditCommand::MoveHome { select: false }));

        let super_backspace = edit_command_from_key(
            &crate::keys::KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Backspace,
                KeyModifiers::SUPER,
            )),
            false,
        );
        assert_eq!(super_backspace, Some(EditCommand::DeleteToStart));
    }
}
