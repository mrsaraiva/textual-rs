//! Canonical key model and normalization helpers.
//!
//! This module provides a canonical key representation aligned with Python
//! Textual's key model.  It wraps crossterm's [`KeyEvent`] and adds normalized
//! key names, printability info, and display/identifier helpers.
//!
//! The module is purely computational -- no I/O or terminal interaction.
//!
//! # Normalization rules
//!
//! - **Modifier ordering:** alphabetical prefix —
//!   `alt+ctrl+hyper+meta+shift+super+key`.
//! - **Shift consumption:** for plain uppercase letters (`Shift+A`), SHIFT is
//!   consumed and the canonical name is the uppercase char (`"A"`).  When CTRL
//!   is also present (`Ctrl+Shift+A`), SHIFT is preserved and the letter is
//!   lowercased → `"ctrl+shift+a"`.
//! - **Ctrl/Alt/Super/Hyper/Meta chords:** do **not** produce printable
//!   characters.
//!   `character` is `None` and `is_printable` is `false`.
//! - **Symbols:** shift is consumed (the symbol itself encodes the shift).
//!   Named via a built-in ASCII table (e.g. `'!'` → `"exclamation_mark"`).
//! - **BackTab:** treated as `shift+tab` regardless of whether crossterm sets
//!   the SHIFT modifier.
//!
//! # Aliases
//!
//! Certain keys have traditional alias relationships:
//!
//! | Canonical   | Alias                    |
//! |-------------|--------------------------|
//! | `tab`       | `ctrl+i`                 |
//! | `enter`     | `ctrl+m`                 |
//! | `escape`    | `ctrl+left_square_bracket` |
//!
//! Aliases are computed lazily via [`KeyEventData::aliases()`].
//!
//! # Kitty keyboard protocol
//!
//! The terminal driver supports the Kitty
//! keyboard protocol (mode 1: `DISAMBIGUATE_ESCAPE_CODES`) via a tri-state
//! [`KeyboardProtocol`](crate::driver::KeyboardProtocol) setting:
//!
//! - **Off** (default): legacy terminal input.  Tab and Ctrl+I are
//!   indistinguishable at the crossterm level.
//! - **Auto**: probe-first behavior. The driver attempts to enable keyboard
//!   enhancements and falls back if unsupported. Override with
//!   `TEXTUAL_KEYBOARD_PROTOCOL=on|off`.
//! - **On**: unconditionally push the enhancement flag.
//!
//! When the protocol is active, crossterm reports distinct key codes for
//! Tab vs Ctrl+I, Enter vs Ctrl+M, etc., so the alias table above becomes
//! informational rather than ambiguous.
//!
//! # Known terminal / environment limitations
//!
//! | Environment | Issue | Workaround |
//! |-------------|-------|------------|
//! | **tmux** | Kitty keyboard protocol not forwarded (tmux ≤ 3.4) | Use `KeyboardProtocol::Off` or run outside tmux |
//! | **screen** | No Kitty protocol; limited modifier reporting | Legacy mode only |
//! | **macOS Terminal.app** | No Kitty protocol; limited Alt modifier reporting (sends ESC prefix) | Avoid Alt bindings or use a modern terminal (kitty, WezTerm, iTerm2, Ghostty) |
//! | **PuTTY / Windows Console** | Partial modifier support; no Kitty protocol | Legacy mode; crossterm handles translation |
//! | **SSH** | Protocol support depends on the local terminal, not the remote shell | Enable on the local terminal |
//! | **Ctrl+Shift+letter** | Some terminals report `KeyCode::Char(uppercase)` with CTRL+SHIFT; we normalize to `ctrl+shift+lowercase` | Covered by normalization |
//! | **BackTab** | Crossterm may or may not set the SHIFT modifier alongside `KeyCode::BackTab` | We unconditionally add SHIFT, producing `shift+tab` |
//!
//! The `examples/keys.rs` diagnostic harness is the recommended tool for
//! verifying input behavior in any terminal environment.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::ops::Deref;

// ---------------------------------------------------------------------------
// Static tables
// ---------------------------------------------------------------------------

/// Key alias pairs.  First element is the canonical name, second is the list
/// of aliases that should also match.
const KEY_ALIASES: &[(&str, &[&str])] = &[
    ("tab", &["ctrl+i"]),
    ("enter", &["ctrl+m"]),
    ("escape", &["ctrl+left_square_bracket"]),
];

/// Human-friendly display replacements for key names.
const KEY_DISPLAY_ALIASES: &[(&str, &str)] = &[
    ("up", "\u{2191}"),        // ↑
    ("down", "\u{2193}"),      // ↓
    ("left", "\u{2190}"),      // ←
    ("right", "\u{2192}"),     // →
    ("backspace", "\u{232b}"), // ⌫
    ("escape", "esc"),
    ("enter", "\u{23ce}"), // ⏎
    ("minus", "-"),
    ("space", "space"),
    ("pagedown", "pgdn"),
    ("pageup", "pgup"),
    ("delete", "del"),
    ("tab", "\u{21e5}"), // ⇥
];

/// Replacements applied to Unicode-derived key names so they align with
/// Python Textual conventions.
const KEY_NAME_REPLACEMENTS: &[(&str, &str)] = &[
    ("solidus", "slash"),
    ("reverse_solidus", "backslash"),
    ("commercial_at", "at"),
    ("hyphen_minus", "minus"),
    ("plus_sign", "plus"),
    ("low_line", "underscore"),
];

// ---------------------------------------------------------------------------
// KeyEventData
// ---------------------------------------------------------------------------

/// Canonical key event data.
///
/// Wraps crossterm's [`KeyEvent`] (accessible via [`Deref`]) and adds
/// normalized canonical fields aligned with Python Textual's key model.
#[derive(Debug, Clone)]
pub struct KeyEventData {
    raw: KeyEvent,
    /// Canonical key name, e.g. `"tab"`, `"ctrl+a"`, `"shift+left"`,
    /// `"exclamation_mark"`.  Modifier ordering is alphabetical:
    /// `alt+ctrl+shift+key`.
    pub key: String,
    /// Character produced if printable, `None` otherwise.
    pub character: Option<char>,
    /// Whether the character is printable (i.e. not a control character).
    pub is_printable: bool,
}

impl Deref for KeyEventData {
    type Target = KeyEvent;
    fn deref(&self) -> &Self::Target {
        &self.raw
    }
}

impl KeyEventData {
    /// Create a [`KeyEventData`] by normalizing a crossterm [`KeyEvent`].
    pub fn from_crossterm(raw: KeyEvent) -> Self {
        let (key, character, is_printable) = normalize_key_code(raw.code, raw.modifiers);
        Self {
            raw,
            key,
            character,
            is_printable,
        }
    }

    /// Returns the canonical key name (borrows from `self.key`).
    pub fn name(&self) -> &str {
        &self.key
    }

    /// Returns a Python-identifier-friendly form of the key name.
    ///
    /// Examples: `"ctrl+p"` -> `"ctrl_p"`, `"A"` -> `"upper_a"`.
    pub fn identifier(&self) -> String {
        key_to_identifier(&self.key)
    }

    /// Returns aliases for this key, computed on demand from the
    /// [`KEY_ALIASES`] table.  The canonical name is always the first element.
    pub fn aliases(&self) -> Vec<&str> {
        let mut result = vec![self.key.as_str()];
        for &(canonical, aliases) in KEY_ALIASES {
            if canonical == self.key {
                result.extend_from_slice(aliases);
            } else {
                for &alias in aliases {
                    if alias == self.key {
                        result.push(canonical);
                    }
                }
            }
        }
        result
    }

    /// Human-friendly display form of the key.
    ///
    /// Examples: `"ctrl+p"` -> `"^p"`, `"left"` -> `"←"`.
    pub fn display(&self) -> String {
        format_key_display(&self.key)
    }

    /// Explicit accessor for the raw crossterm [`KeyEvent`].
    pub fn raw(&self) -> &KeyEvent {
        &self.raw
    }
}

// ---------------------------------------------------------------------------
// Normalization
// ---------------------------------------------------------------------------

/// Core normalization logic.  Maps a crossterm [`KeyCode`] + [`KeyModifiers`]
/// to a canonical `(key_name, character, is_printable)` triple.
fn normalize_key_code(code: KeyCode, modifiers: KeyModifiers) -> (String, Option<char>, bool) {
    // We will strip SHIFT from the modifier set when the character already
    // encodes it (e.g. uppercase letter, or symbol produced by Shift).
    let mut mods = modifiers;

    let has_ctrl = mods.contains(KeyModifiers::CONTROL);
    let has_alt = mods.contains(KeyModifiers::ALT);
    let has_super = mods.contains(KeyModifiers::SUPER);
    let has_hyper = mods.contains(KeyModifiers::HYPER);
    let has_meta = mods.contains(KeyModifiers::META);
    let has_non_shift_modifier = has_alt || has_ctrl || has_hyper || has_meta || has_super;

    let (base_name, raw_char) = match code {
        KeyCode::Char(ch) => {
            if ch.is_ascii_alphabetic() {
                if ch.is_ascii_uppercase() {
                    if has_non_shift_modifier {
                        // Non-shift modifier + uppercase keeps explicit Shift
                        // in the canonical form, and normalizes the base key to lowercase.
                        (ch.to_ascii_lowercase().to_string(), Some(ch))
                    } else {
                        // Shift+A -> "A" (Shift consumed by uppercase)
                        mods.remove(KeyModifiers::SHIFT);
                        (ch.to_string(), Some(ch))
                    }
                } else {
                    (ch.to_lowercase().to_string(), Some(ch))
                }
            } else if ch.is_ascii_digit() {
                (ch.to_string(), Some(ch))
            } else {
                // Symbol / punctuation -- Shift already produced the
                // character, so strip the modifier.
                mods.remove(KeyModifiers::SHIFT);
                let name = character_to_key_name(ch);
                (name, Some(ch))
            }
        }
        KeyCode::Enter => ("enter".into(), None),
        KeyCode::Tab => ("tab".into(), None),
        KeyCode::Backspace => ("backspace".into(), None),
        KeyCode::Delete => ("delete".into(), None),
        KeyCode::Esc => ("escape".into(), None),
        KeyCode::Up => ("up".into(), None),
        KeyCode::Down => ("down".into(), None),
        KeyCode::Left => ("left".into(), None),
        KeyCode::Right => ("right".into(), None),
        KeyCode::Home => ("home".into(), None),
        KeyCode::End => ("end".into(), None),
        KeyCode::PageUp => ("pageup".into(), None),
        KeyCode::PageDown => ("pagedown".into(), None),
        KeyCode::Insert => ("insert".into(), None),
        KeyCode::F(n) => (format!("f{n}"), None),
        KeyCode::BackTab => {
            // BackTab is conceptually Shift+Tab.
            mods.insert(KeyModifiers::SHIFT);
            ("tab".into(), None)
        }
        KeyCode::Null => ("null".into(), None),
        KeyCode::CapsLock => ("capslock".into(), None),
        KeyCode::ScrollLock => ("scrolllock".into(), None),
        KeyCode::NumLock => ("numlock".into(), None),
        KeyCode::PrintScreen => ("printscreen".into(), None),
        KeyCode::Pause => ("pause".into(), None),
        KeyCode::Menu => ("menu".into(), None),
        KeyCode::KeypadBegin => ("keypad_begin".into(), None),
        KeyCode::Media(media) => {
            let name = match media {
                crossterm::event::MediaKeyCode::Play => "media_play",
                crossterm::event::MediaKeyCode::Pause => "media_pause",
                crossterm::event::MediaKeyCode::PlayPause => "media_play_pause",
                crossterm::event::MediaKeyCode::Reverse => "media_reverse",
                crossterm::event::MediaKeyCode::Stop => "media_stop",
                crossterm::event::MediaKeyCode::FastForward => "media_fast_forward",
                crossterm::event::MediaKeyCode::Rewind => "media_rewind",
                crossterm::event::MediaKeyCode::TrackNext => "media_track_next",
                crossterm::event::MediaKeyCode::TrackPrevious => "media_track_previous",
                crossterm::event::MediaKeyCode::Record => "media_record",
                crossterm::event::MediaKeyCode::LowerVolume => "media_lower_volume",
                crossterm::event::MediaKeyCode::RaiseVolume => "media_raise_volume",
                crossterm::event::MediaKeyCode::MuteVolume => "media_mute_volume",
            };
            (name.into(), None)
        }
        KeyCode::Modifier(modifier) => {
            let name = match modifier {
                crossterm::event::ModifierKeyCode::LeftShift => "left_shift",
                crossterm::event::ModifierKeyCode::LeftControl => "left_control",
                crossterm::event::ModifierKeyCode::LeftAlt => "left_alt",
                crossterm::event::ModifierKeyCode::LeftSuper => "left_super",
                crossterm::event::ModifierKeyCode::LeftHyper => "left_hyper",
                crossterm::event::ModifierKeyCode::LeftMeta => "left_meta",
                crossterm::event::ModifierKeyCode::RightShift => "right_shift",
                crossterm::event::ModifierKeyCode::RightControl => "right_control",
                crossterm::event::ModifierKeyCode::RightAlt => "right_alt",
                crossterm::event::ModifierKeyCode::RightSuper => "right_super",
                crossterm::event::ModifierKeyCode::RightHyper => "right_hyper",
                crossterm::event::ModifierKeyCode::RightMeta => "right_meta",
                crossterm::event::ModifierKeyCode::IsoLevel3Shift => "iso_level3_shift",
                crossterm::event::ModifierKeyCode::IsoLevel5Shift => "iso_level5_shift",
            };
            (name.into(), None)
        }
    };

    // Determine character and printability.
    // Chords with non-shift modifiers do NOT produce printable characters.
    let (character, is_printable) = if has_non_shift_modifier {
        (None, false)
    } else {
        match raw_char {
            Some(ch) => (Some(ch), !ch.is_control()),
            None => (None, false),
        }
    };

    // Build canonical name with modifier prefixes in alphabetical order.
    let mut prefix = String::new();
    if mods.contains(KeyModifiers::ALT) {
        prefix.push_str("alt+");
    }
    if mods.contains(KeyModifiers::CONTROL) {
        prefix.push_str("ctrl+");
    }
    if mods.contains(KeyModifiers::HYPER) {
        prefix.push_str("hyper+");
    }
    if mods.contains(KeyModifiers::META) {
        prefix.push_str("meta+");
    }
    if mods.contains(KeyModifiers::SHIFT) {
        prefix.push_str("shift+");
    }
    if mods.contains(KeyModifiers::SUPER) {
        prefix.push_str("super+");
    }

    let canonical = format!("{prefix}{base_name}");
    (canonical, character, is_printable)
}

/// Derive a human-readable key name for non-alphanumeric characters.
///
/// Uses a built-in lookup table for ASCII symbols.  Characters not in the
/// table fall back to `"u+XXXX"` hex representation.
fn character_to_key_name(ch: char) -> String {
    let raw_name = match ch {
        ' ' => "space",
        '!' => "exclamation_mark",
        '"' => "quotation_mark",
        '#' => "number_sign",
        '$' => "dollar_sign",
        '%' => "percent_sign",
        '&' => "ampersand",
        '\'' => "apostrophe",
        '(' => "left_parenthesis",
        ')' => "right_parenthesis",
        '*' => "asterisk",
        '+' => "plus_sign",
        ',' => "comma",
        '-' => "hyphen_minus",
        '.' => "full_stop",
        '/' => "solidus",
        ':' => "colon",
        ';' => "semicolon",
        '<' => "less_than_sign",
        '=' => "equals_sign",
        '>' => "greater_than_sign",
        '?' => "question_mark",
        '@' => "commercial_at",
        '[' => "left_square_bracket",
        '\\' => "reverse_solidus",
        ']' => "right_square_bracket",
        '^' => "circumflex_accent",
        '_' => "low_line",
        '`' => "grave_accent",
        '{' => "left_curly_bracket",
        '|' => "vertical_line",
        '}' => "right_curly_bracket",
        '~' => "tilde",
        _ => {
            return format!("u+{:04x}", ch as u32);
        }
    };

    // Apply name replacements to align with Python Textual conventions.
    apply_key_name_replacements(raw_name)
}

/// Apply [`KEY_NAME_REPLACEMENTS`] to a raw key name.
fn apply_key_name_replacements(name: &str) -> String {
    for &(from, to) in KEY_NAME_REPLACEMENTS {
        if name == from {
            return to.to_string();
        }
    }
    name.to_string()
}

// ---------------------------------------------------------------------------
// Public helpers
// ---------------------------------------------------------------------------

/// Convert a canonical key name to a Python-identifier-friendly form.
///
/// - Replaces `'+'` with `'_'`
/// - Lowercases everything
/// - Single uppercase characters are prefixed with `"upper_"`
///
/// # Examples
///
/// ```
/// use textual::keys::key_to_identifier;
/// assert_eq!(key_to_identifier("ctrl+p"), "ctrl_p");
/// assert_eq!(key_to_identifier("A"), "upper_a");
/// assert_eq!(key_to_identifier("shift+left"), "shift_left");
/// ```
pub fn key_to_identifier(key: &str) -> String {
    // Single uppercase character (e.g. "A", "Z").
    if key.len() == 1 {
        let ch = key.chars().next().unwrap();
        if ch.is_ascii_uppercase() {
            return format!("upper_{}", ch.to_ascii_lowercase());
        }
    }
    key.replace('+', "_").to_lowercase()
}

/// Format a canonical key name for human-friendly display.
///
/// - Looks up display aliases for the key part (e.g. `"left"` -> `"←"`)
/// - Replaces `"ctrl"` modifier with `"^"` prefix on the key part
/// - Joins remaining modifiers with `"+"`
///
/// # Examples
///
/// ```
/// use textual::keys::format_key_display;
/// assert_eq!(format_key_display("ctrl+p"), "^p");
/// assert_eq!(format_key_display("left"), "\u{2190}");
/// assert_eq!(format_key_display("shift+left"), "shift+\u{2190}");
/// assert_eq!(format_key_display("alt+ctrl+x"), "alt+^x");
/// ```
/// Inverse of `character_to_key_name` for the punctuation set: maps a canonical
/// key identifier (e.g. "question_mark") back to its display character ("?").
/// Used by `format_key_display` so punctuation bindings render as their symbol
/// in footers/hints, matching Python Textual.
fn punctuation_name_to_char(name: &str) -> Option<&'static str> {
    Some(match name {
        "exclamation_mark" => "!",
        "quotation_mark" => "\"",
        "number_sign" => "#",
        "dollar_sign" => "$",
        "percent_sign" => "%",
        "ampersand" => "&",
        "apostrophe" => "'",
        "left_parenthesis" => "(",
        "right_parenthesis" => ")",
        "asterisk" => "*",
        "plus_sign" => "+",
        "comma" => ",",
        "hyphen_minus" => "-",
        "full_stop" => ".",
        "solidus" => "/",
        "colon" => ":",
        "semicolon" => ";",
        "less_than_sign" => "<",
        "equals_sign" => "=",
        "greater_than_sign" => ">",
        "question_mark" => "?",
        "commercial_at" => "@",
        "left_square_bracket" => "[",
        "reverse_solidus" => "\\",
        _ => return None,
    })
}

pub fn format_key_display(key: &str) -> String {
    let parts: Vec<&str> = key.split('+').collect();
    if parts.is_empty() {
        return key.to_string();
    }

    let key_part = parts[parts.len() - 1];
    let modifier_parts = &parts[..parts.len() - 1];

    // Look up display alias for the key part. Punctuation keys carry their
    // canonical identifier (e.g. "question_mark") as the binding key; display
    // them as their source character (e.g. "?"), mirroring Python Textual.
    let displayed_key = KEY_DISPLAY_ALIASES
        .iter()
        .find(|&&(name, _)| name == key_part)
        .map(|&(_, display)| display)
        .or_else(|| punctuation_name_to_char(key_part))
        .unwrap_or(key_part);

    let has_ctrl = modifier_parts.contains(&"ctrl");

    // Collect non-ctrl modifiers.
    let other_mods: Vec<&str> = modifier_parts
        .iter()
        .copied()
        .filter(|&m| m != "ctrl")
        .collect();

    if has_ctrl {
        // "ctrl" becomes "^" prefix on the key.
        let ctrl_key = format!("^{displayed_key}");
        if other_mods.is_empty() {
            ctrl_key
        } else {
            format!("{}+{}", other_mods.join("+"), ctrl_key)
        }
    } else if other_mods.is_empty() && modifier_parts.is_empty() {
        // No modifiers at all.
        displayed_key.to_string()
    } else {
        // Non-ctrl modifiers only.
        let mods = modifier_parts.join("+");
        format!("{mods}+{displayed_key}")
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    /// Helper to create a KeyEvent for testing.
    fn key_event(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    // -- normalize_key_code: common keys --

    #[test]
    fn normalize_lowercase_letter() {
        let (name, ch, printable) = normalize_key_code(KeyCode::Char('a'), KeyModifiers::empty());
        assert_eq!(name, "a");
        assert_eq!(ch, Some('a'));
        assert!(printable);
    }

    #[test]
    fn normalize_uppercase_letter() {
        let (name, ch, _) = normalize_key_code(KeyCode::Char('A'), KeyModifiers::SHIFT);
        assert_eq!(name, "A");
        assert_eq!(ch, Some('A'));
        // Shift should NOT appear in the name (consumed by uppercase).
    }

    #[test]
    fn normalize_ctrl_shift_a() {
        // Ctrl+Shift+A should preserve SHIFT and use lowercase.
        let mods = KeyModifiers::CONTROL | KeyModifiers::SHIFT;
        let (name, ch, printable) = normalize_key_code(KeyCode::Char('A'), mods);
        assert_eq!(name, "ctrl+shift+a");
        // Ctrl chord does not produce a printable character.
        assert_eq!(ch, None);
        assert!(!printable);
    }

    #[test]
    fn normalize_alt_shift_a() {
        let mods = KeyModifiers::ALT | KeyModifiers::SHIFT;
        let (name, ch, printable) = normalize_key_code(KeyCode::Char('A'), mods);
        assert_eq!(name, "alt+shift+a");
        assert_eq!(ch, None);
        assert!(!printable);
    }

    #[test]
    fn normalize_digit() {
        let (name, ch, printable) = normalize_key_code(KeyCode::Char('5'), KeyModifiers::empty());
        assert_eq!(name, "5");
        assert_eq!(ch, Some('5'));
        assert!(printable);
    }

    #[test]
    fn normalize_enter() {
        let (name, ch, printable) = normalize_key_code(KeyCode::Enter, KeyModifiers::empty());
        assert_eq!(name, "enter");
        assert_eq!(ch, None);
        assert!(!printable);
    }

    #[test]
    fn normalize_tab() {
        let (name, _, _) = normalize_key_code(KeyCode::Tab, KeyModifiers::empty());
        assert_eq!(name, "tab");
    }

    #[test]
    fn normalize_escape() {
        let (name, _, _) = normalize_key_code(KeyCode::Esc, KeyModifiers::empty());
        assert_eq!(name, "escape");
    }

    #[test]
    fn normalize_arrows() {
        assert_eq!(
            normalize_key_code(KeyCode::Up, KeyModifiers::empty()).0,
            "up"
        );
        assert_eq!(
            normalize_key_code(KeyCode::Down, KeyModifiers::empty()).0,
            "down"
        );
        assert_eq!(
            normalize_key_code(KeyCode::Left, KeyModifiers::empty()).0,
            "left"
        );
        assert_eq!(
            normalize_key_code(KeyCode::Right, KeyModifiers::empty()).0,
            "right"
        );
    }

    #[test]
    fn normalize_f_keys() {
        assert_eq!(
            normalize_key_code(KeyCode::F(1), KeyModifiers::empty()).0,
            "f1"
        );
        assert_eq!(
            normalize_key_code(KeyCode::F(12), KeyModifiers::empty()).0,
            "f12"
        );
    }

    #[test]
    fn normalize_backtab() {
        let (name, _, _) = normalize_key_code(KeyCode::BackTab, KeyModifiers::SHIFT);
        assert_eq!(name, "shift+tab");
    }

    // -- normalize_key_code: symbols --

    #[test]
    fn normalize_exclamation() {
        let (name, ch, printable) = normalize_key_code(KeyCode::Char('!'), KeyModifiers::SHIFT);
        assert_eq!(name, "exclamation_mark");
        assert_eq!(ch, Some('!'));
        assert!(printable);
    }

    #[test]
    fn normalize_space() {
        let (name, ch, printable) = normalize_key_code(KeyCode::Char(' '), KeyModifiers::empty());
        assert_eq!(name, "space");
        assert_eq!(ch, Some(' '));
        assert!(printable);
    }

    #[test]
    fn normalize_slash() {
        let (name, _, _) = normalize_key_code(KeyCode::Char('/'), KeyModifiers::empty());
        assert_eq!(name, "slash");
    }

    #[test]
    fn normalize_at() {
        let (name, _, _) = normalize_key_code(KeyCode::Char('@'), KeyModifiers::empty());
        assert_eq!(name, "at");
    }

    #[test]
    fn normalize_minus() {
        let (name, _, _) = normalize_key_code(KeyCode::Char('-'), KeyModifiers::empty());
        assert_eq!(name, "minus");
    }

    #[test]
    fn normalize_plus() {
        let (name, _, _) = normalize_key_code(KeyCode::Char('+'), KeyModifiers::SHIFT);
        assert_eq!(name, "plus");
    }

    #[test]
    fn normalize_underscore() {
        let (name, _, _) = normalize_key_code(KeyCode::Char('_'), KeyModifiers::SHIFT);
        assert_eq!(name, "underscore");
    }

    // -- normalize_key_code: with modifiers --

    #[test]
    fn normalize_ctrl_a() {
        let (name, ch, printable) = normalize_key_code(KeyCode::Char('a'), KeyModifiers::CONTROL);
        assert_eq!(name, "ctrl+a");
        // Ctrl chord does not produce a printable character.
        assert_eq!(ch, None);
        assert!(!printable);
    }

    #[test]
    fn normalize_alt_ctrl_shift_left() {
        let mods = KeyModifiers::ALT | KeyModifiers::CONTROL | KeyModifiers::SHIFT;
        let (name, _, _) = normalize_key_code(KeyCode::Left, mods);
        assert_eq!(name, "alt+ctrl+shift+left");
    }

    #[test]
    fn normalize_all_modifiers_ordering() {
        let mods = KeyModifiers::ALT
            | KeyModifiers::CONTROL
            | KeyModifiers::HYPER
            | KeyModifiers::META
            | KeyModifiers::SHIFT
            | KeyModifiers::SUPER;
        let (name, _, _) = normalize_key_code(KeyCode::Left, mods);
        assert_eq!(name, "alt+ctrl+hyper+meta+shift+super+left");
    }

    #[test]
    fn normalize_shift_f1() {
        let (name, _, _) = normalize_key_code(KeyCode::F(1), KeyModifiers::SHIFT);
        assert_eq!(name, "shift+f1");
    }

    #[test]
    fn normalize_alt_enter() {
        let (name, _, _) = normalize_key_code(KeyCode::Enter, KeyModifiers::ALT);
        assert_eq!(name, "alt+enter");
    }

    // -- character_to_key_name --

    #[test]
    fn char_name_period() {
        assert_eq!(character_to_key_name('.'), "full_stop");
    }

    #[test]
    fn char_name_backslash() {
        assert_eq!(character_to_key_name('\\'), "backslash");
    }

    #[test]
    fn char_name_unknown_unicode() {
        // A character not in the ASCII table falls back to u+XXXX.
        assert_eq!(character_to_key_name('\u{00e9}'), "u+00e9");
    }

    #[test]
    fn char_name_tilde() {
        assert_eq!(character_to_key_name('~'), "tilde");
    }

    #[test]
    fn char_name_left_curly() {
        assert_eq!(character_to_key_name('{'), "left_curly_bracket");
    }

    // -- key_to_identifier --

    #[test]
    fn identifier_ctrl_p() {
        assert_eq!(key_to_identifier("ctrl+p"), "ctrl_p");
    }

    #[test]
    fn identifier_uppercase_a() {
        assert_eq!(key_to_identifier("A"), "upper_a");
    }

    #[test]
    fn identifier_shift_left() {
        assert_eq!(key_to_identifier("shift+left"), "shift_left");
    }

    #[test]
    fn identifier_lowercase_a() {
        assert_eq!(key_to_identifier("a"), "a");
    }

    #[test]
    fn identifier_f1() {
        assert_eq!(key_to_identifier("f1"), "f1");
    }

    // -- format_key_display --

    #[test]
    fn display_ctrl_p() {
        assert_eq!(format_key_display("ctrl+p"), "^p");
    }

    #[test]
    fn display_left_arrow() {
        assert_eq!(format_key_display("left"), "\u{2190}");
    }

    #[test]
    fn display_shift_left() {
        assert_eq!(format_key_display("shift+left"), "shift+\u{2190}");
    }

    #[test]
    fn display_alt_ctrl_x() {
        assert_eq!(format_key_display("alt+ctrl+x"), "alt+^x");
    }

    #[test]
    fn display_enter() {
        assert_eq!(format_key_display("enter"), "\u{23ce}");
    }

    #[test]
    fn display_tab() {
        assert_eq!(format_key_display("tab"), "\u{21e5}");
    }

    #[test]
    fn display_plain_key() {
        // No alias, no modifier -- returned as-is.
        assert_eq!(format_key_display("a"), "a");
    }

    // -- aliases --

    #[test]
    fn aliases_tab() {
        let data = KeyEventData::from_crossterm(key_event(KeyCode::Tab, KeyModifiers::empty()));
        let aliases = data.aliases();
        assert_eq!(aliases[0], "tab");
        assert!(aliases.contains(&"ctrl+i"));
    }

    #[test]
    fn aliases_enter() {
        let data = KeyEventData::from_crossterm(key_event(KeyCode::Enter, KeyModifiers::empty()));
        let aliases = data.aliases();
        assert_eq!(aliases[0], "enter");
        assert!(aliases.contains(&"ctrl+m"));
    }

    #[test]
    fn aliases_no_alias() {
        let data =
            KeyEventData::from_crossterm(key_event(KeyCode::Char('x'), KeyModifiers::empty()));
        let aliases = data.aliases();
        assert_eq!(aliases, vec!["x"]);
    }

    // -- KeyEventData round-trip --

    #[test]
    fn key_event_data_from_crossterm() {
        let raw = key_event(KeyCode::Char('a'), KeyModifiers::CONTROL);
        let data = KeyEventData::from_crossterm(raw);
        assert_eq!(data.name(), "ctrl+a");
        // Ctrl chord does not produce a printable character.
        assert_eq!(data.character, None);
        assert!(!data.is_printable);
        assert_eq!(data.identifier(), "ctrl_a");
        assert_eq!(data.display(), "^a");
        // Deref should give us the raw KeyEvent fields.
        assert_eq!(data.code, KeyCode::Char('a'));
        assert_eq!(data.modifiers, KeyModifiers::CONTROL);
    }

    #[test]
    fn key_event_data_raw_accessor() {
        let raw = key_event(KeyCode::Enter, KeyModifiers::empty());
        let data = KeyEventData::from_crossterm(raw);
        assert_eq!(data.raw().code, KeyCode::Enter);
    }

    // -- Media / Modifier keys --

    #[test]
    fn normalize_media_key() {
        let (name, _, _) = normalize_key_code(
            KeyCode::Media(crossterm::event::MediaKeyCode::Play),
            KeyModifiers::empty(),
        );
        assert_eq!(name, "media_play");
    }

    #[test]
    fn normalize_modifier_key() {
        let (name, _, _) = normalize_key_code(
            KeyCode::Modifier(crossterm::event::ModifierKeyCode::LeftShift),
            KeyModifiers::empty(),
        );
        assert_eq!(name, "left_shift");
    }

    #[test]
    fn normalize_alt_char_not_printable() {
        let (name, ch, printable) = normalize_key_code(KeyCode::Char('x'), KeyModifiers::ALT);
        assert_eq!(name, "alt+x");
        // Alt chords do not produce a printable character.
        assert_eq!(ch, None);
        assert!(!printable);
    }
}
