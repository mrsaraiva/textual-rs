//! Integration tests for the canonical key model (`textual::keys`).
//!
//! These tests exercise the full round-trip from crossterm `KeyEvent` through
//! `KeyEventData` normalization, and verify display formatting, identifier
//! conversion, alias resolution, Deref compatibility, and edge cases.

use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, MediaKeyCode, ModifierKeyCode,
};
use textual::event::{Action, ActionMap, KeyBind};
use textual::keys::{KeyEventData, format_key_display, key_to_identifier};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_key(code: KeyCode, modifiers: KeyModifiers) -> KeyEventData {
    KeyEventData::from_crossterm(KeyEvent {
        code,
        modifiers,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    })
}

fn make_key_with_kind(code: KeyCode, modifiers: KeyModifiers, kind: KeyEventKind) -> KeyEventData {
    KeyEventData::from_crossterm(KeyEvent {
        code,
        modifiers,
        kind,
        state: KeyEventState::NONE,
    })
}

// ===========================================================================
// 1. Round-trip normalization tests
// ===========================================================================

// -- Printable ASCII: lowercase a-z --

#[test]
fn roundtrip_lowercase_a_through_z() {
    for ch in 'a'..='z' {
        let data = make_key(KeyCode::Char(ch), KeyModifiers::empty());
        assert_eq!(data.key, ch.to_string(), "key name for '{ch}'");
        assert_eq!(data.character, Some(ch), "character for '{ch}'");
        assert!(data.is_printable, "is_printable for '{ch}'");
    }
}

// -- Printable ASCII: uppercase A-Z --

#[test]
fn roundtrip_uppercase_a_through_z() {
    for ch in 'A'..='Z' {
        let data = make_key(KeyCode::Char(ch), KeyModifiers::SHIFT);
        // Shift is consumed by uppercase, canonical name is the uppercase char.
        assert_eq!(data.key, ch.to_string(), "key name for '{ch}'");
        assert_eq!(data.character, Some(ch), "character for '{ch}'");
        assert!(data.is_printable, "is_printable for '{ch}'");
    }
}

// -- Printable ASCII: digits 0-9 --

#[test]
fn roundtrip_digits() {
    for ch in '0'..='9' {
        let data = make_key(KeyCode::Char(ch), KeyModifiers::empty());
        assert_eq!(data.key, ch.to_string(), "key name for '{ch}'");
        assert_eq!(data.character, Some(ch), "character for '{ch}'");
        assert!(data.is_printable, "is_printable for '{ch}'");
    }
}

// -- Common named keys --

#[test]
fn roundtrip_space() {
    let data = make_key(KeyCode::Char(' '), KeyModifiers::empty());
    assert_eq!(data.key, "space");
    assert_eq!(data.character, Some(' '));
    assert!(data.is_printable);
}

#[test]
fn roundtrip_tab() {
    let data = make_key(KeyCode::Tab, KeyModifiers::empty());
    assert_eq!(data.key, "tab");
    assert_eq!(data.character, None);
    assert!(!data.is_printable);
}

#[test]
fn roundtrip_enter() {
    let data = make_key(KeyCode::Enter, KeyModifiers::empty());
    assert_eq!(data.key, "enter");
    assert_eq!(data.character, None);
    assert!(!data.is_printable);
}

#[test]
fn roundtrip_escape() {
    let data = make_key(KeyCode::Esc, KeyModifiers::empty());
    assert_eq!(data.key, "escape");
    assert_eq!(data.character, None);
    assert!(!data.is_printable);
}

#[test]
fn roundtrip_backspace() {
    let data = make_key(KeyCode::Backspace, KeyModifiers::empty());
    assert_eq!(data.key, "backspace");
    assert_eq!(data.character, None);
    assert!(!data.is_printable);
}

#[test]
fn roundtrip_delete() {
    let data = make_key(KeyCode::Delete, KeyModifiers::empty());
    assert_eq!(data.key, "delete");
    assert_eq!(data.character, None);
    assert!(!data.is_printable);
}

// -- Arrow keys with and without modifiers --

#[test]
fn roundtrip_arrow_keys_plain() {
    let cases = [
        (KeyCode::Up, "up"),
        (KeyCode::Down, "down"),
        (KeyCode::Left, "left"),
        (KeyCode::Right, "right"),
    ];
    for (code, expected) in cases {
        let data = make_key(code, KeyModifiers::empty());
        assert_eq!(data.key, expected, "plain arrow {expected}");
        assert_eq!(data.character, None);
        assert!(!data.is_printable);
    }
}

#[test]
fn roundtrip_arrow_keys_with_ctrl() {
    let cases = [
        (KeyCode::Up, "ctrl+up"),
        (KeyCode::Down, "ctrl+down"),
        (KeyCode::Left, "ctrl+left"),
        (KeyCode::Right, "ctrl+right"),
    ];
    for (code, expected) in cases {
        let data = make_key(code, KeyModifiers::CONTROL);
        assert_eq!(data.key, expected, "ctrl+arrow {expected}");
    }
}

#[test]
fn roundtrip_arrow_keys_with_alt() {
    let cases = [
        (KeyCode::Up, "alt+up"),
        (KeyCode::Down, "alt+down"),
        (KeyCode::Left, "alt+left"),
        (KeyCode::Right, "alt+right"),
    ];
    for (code, expected) in cases {
        let data = make_key(code, KeyModifiers::ALT);
        assert_eq!(data.key, expected, "alt+arrow {expected}");
    }
}

#[test]
fn roundtrip_shift_arrow_keys() {
    let cases = [
        (KeyCode::Up, "shift+up"),
        (KeyCode::Down, "shift+down"),
        (KeyCode::Left, "shift+left"),
        (KeyCode::Right, "shift+right"),
    ];
    for (code, expected) in cases {
        let data = make_key(code, KeyModifiers::SHIFT);
        assert_eq!(data.key, expected, "shift+arrow {expected}");
    }
}

// -- F1 through F12 --

#[test]
fn roundtrip_f_keys() {
    for n in 1..=12u8 {
        let data = make_key(KeyCode::F(n), KeyModifiers::empty());
        assert_eq!(data.key, format!("f{n}"), "F{n} key");
        assert_eq!(data.character, None);
        assert!(!data.is_printable);
    }
}

// -- Ctrl+letter combinations --

#[test]
fn roundtrip_ctrl_letter_a_through_z() {
    for ch in 'a'..='z' {
        let data = make_key(KeyCode::Char(ch), KeyModifiers::CONTROL);
        let expected = format!("ctrl+{ch}");
        assert_eq!(data.key, expected, "ctrl+{ch}");
        // Ctrl chords do NOT produce printable characters.
        assert_eq!(data.character, None, "ctrl+{ch} character should be None");
        assert!(!data.is_printable, "ctrl+{ch} should not be printable");
    }
}

// -- Alt+letter combinations --

#[test]
fn roundtrip_alt_letter_a_through_z() {
    for ch in 'a'..='z' {
        let data = make_key(KeyCode::Char(ch), KeyModifiers::ALT);
        let expected = format!("alt+{ch}");
        assert_eq!(data.key, expected, "alt+{ch}");
        assert_eq!(data.character, None, "alt+{ch} character should be None");
        assert!(!data.is_printable, "alt+{ch} should not be printable");
    }
}

// -- Ctrl+Shift+letter --

#[test]
fn roundtrip_ctrl_shift_letter() {
    // Ctrl+Shift+A should produce "ctrl+shift+a", NOT "ctrl+A".
    for ch in 'A'..='Z' {
        let mods = KeyModifiers::CONTROL | KeyModifiers::SHIFT;
        let data = make_key(KeyCode::Char(ch), mods);
        let expected = format!("ctrl+shift+{}", ch.to_ascii_lowercase());
        assert_eq!(data.key, expected, "ctrl+shift+{ch}");
        assert_eq!(data.character, None);
        assert!(!data.is_printable);
    }
}

#[test]
fn roundtrip_alt_shift_letter() {
    for ch in 'A'..='Z' {
        let mods = KeyModifiers::ALT | KeyModifiers::SHIFT;
        let data = make_key(KeyCode::Char(ch), mods);
        let expected = format!("alt+shift+{}", ch.to_ascii_lowercase());
        assert_eq!(data.key, expected, "alt+shift+{ch}");
        assert_eq!(data.character, None);
        assert!(!data.is_printable);
    }
}

// -- BackTab --

#[test]
fn roundtrip_backtab_is_shift_tab() {
    let data = make_key(KeyCode::BackTab, KeyModifiers::SHIFT);
    assert_eq!(data.key, "shift+tab");
    assert_eq!(data.character, None);
    assert!(!data.is_printable);
}

#[test]
fn roundtrip_backtab_without_shift_modifier_still_adds_shift() {
    // crossterm may send BackTab with or without SHIFT already set.
    let data = make_key(KeyCode::BackTab, KeyModifiers::empty());
    assert_eq!(data.key, "shift+tab");
}

// ===========================================================================
// 2. Alias correctness tests
// ===========================================================================

#[test]
fn alias_tab_has_ctrl_i() {
    let data = make_key(KeyCode::Tab, KeyModifiers::empty());
    let aliases = data.aliases();
    assert_eq!(aliases[0], "tab", "canonical name should be first");
    assert!(
        aliases.contains(&"ctrl+i"),
        "tab should have ctrl+i as alias"
    );
}

#[test]
fn alias_enter_has_ctrl_m() {
    let data = make_key(KeyCode::Enter, KeyModifiers::empty());
    let aliases = data.aliases();
    assert_eq!(aliases[0], "enter");
    assert!(
        aliases.contains(&"ctrl+m"),
        "enter should have ctrl+m as alias"
    );
}

#[test]
fn alias_escape_has_ctrl_left_square_bracket() {
    let data = make_key(KeyCode::Esc, KeyModifiers::empty());
    let aliases = data.aliases();
    assert_eq!(aliases[0], "escape");
    assert!(
        aliases.contains(&"ctrl+left_square_bracket"),
        "escape should have ctrl+left_square_bracket as alias"
    );
}

#[test]
fn alias_no_aliases_returns_only_self() {
    let data = make_key(KeyCode::Char('x'), KeyModifiers::empty());
    let aliases = data.aliases();
    assert_eq!(
        aliases,
        vec!["x"],
        "key with no alias should return only itself"
    );
}

#[test]
fn alias_arrow_key_has_no_aliases() {
    let data = make_key(KeyCode::Left, KeyModifiers::empty());
    let aliases = data.aliases();
    assert_eq!(aliases, vec!["left"]);
}

#[test]
fn alias_bidirectional_ctrl_i_resolves_to_tab() {
    // If you type ctrl+i, you should get tab as an alias.
    let data = make_key(KeyCode::Char('i'), KeyModifiers::CONTROL);
    let aliases = data.aliases();
    assert_eq!(aliases[0], "ctrl+i");
    assert!(
        aliases.contains(&"tab"),
        "ctrl+i should resolve to tab alias"
    );
}

#[test]
fn alias_bidirectional_ctrl_m_resolves_to_enter() {
    let data = make_key(KeyCode::Char('m'), KeyModifiers::CONTROL);
    let aliases = data.aliases();
    assert_eq!(aliases[0], "ctrl+m");
    assert!(
        aliases.contains(&"enter"),
        "ctrl+m should resolve to enter alias"
    );
}

#[test]
fn alias_bidirectional_ctrl_left_square_bracket_resolves_to_escape() {
    // Ctrl+[ is typed as Ctrl+Char('['), which normalizes to
    // "ctrl+left_square_bracket". This should have "escape" as an alias.
    let data = make_key(KeyCode::Char('['), KeyModifiers::CONTROL);
    let aliases = data.aliases();
    assert_eq!(aliases[0], "ctrl+left_square_bracket");
    assert!(
        aliases.contains(&"escape"),
        "ctrl+left_square_bracket should resolve to escape alias"
    );
}

// ===========================================================================
// 3. Display formatting tests
// ===========================================================================

#[test]
fn display_plain_letters() {
    assert_eq!(format_key_display("a"), "a");
    assert_eq!(format_key_display("z"), "z");
    assert_eq!(format_key_display("A"), "A");
}

#[test]
fn display_arrow_keys_show_unicode() {
    assert_eq!(format_key_display("up"), "\u{2191}");
    assert_eq!(format_key_display("down"), "\u{2193}");
    assert_eq!(format_key_display("left"), "\u{2190}");
    assert_eq!(format_key_display("right"), "\u{2192}");
}

#[test]
fn display_ctrl_combinations_show_caret() {
    assert_eq!(format_key_display("ctrl+a"), "^a");
    assert_eq!(format_key_display("ctrl+z"), "^z");
    assert_eq!(format_key_display("ctrl+c"), "^c");
}

#[test]
fn display_multi_modifier_keys() {
    assert_eq!(format_key_display("alt+ctrl+x"), "alt+^x");
    assert_eq!(format_key_display("alt+ctrl+left"), "alt+^\u{2190}");
}

#[test]
fn display_named_keys_with_aliases() {
    assert_eq!(format_key_display("tab"), "\u{21e5}");
    assert_eq!(format_key_display("enter"), "\u{23ce}");
    assert_eq!(format_key_display("backspace"), "\u{232b}");
    assert_eq!(format_key_display("escape"), "esc");
    assert_eq!(format_key_display("delete"), "del");
    assert_eq!(format_key_display("space"), "space");
    assert_eq!(format_key_display("pageup"), "pgup");
    assert_eq!(format_key_display("pagedown"), "pgdn");
    assert_eq!(format_key_display("minus"), "-");
}

#[test]
fn display_shift_arrow() {
    assert_eq!(format_key_display("shift+left"), "shift+\u{2190}");
    assert_eq!(format_key_display("shift+right"), "shift+\u{2192}");
    assert_eq!(format_key_display("shift+up"), "shift+\u{2191}");
    assert_eq!(format_key_display("shift+down"), "shift+\u{2193}");
}

#[test]
fn display_ctrl_tab() {
    assert_eq!(format_key_display("ctrl+tab"), "^\u{21e5}");
}

#[test]
fn display_shift_tab() {
    assert_eq!(format_key_display("shift+tab"), "shift+\u{21e5}");
}

#[test]
fn display_f_keys_no_alias() {
    // F-keys have no display alias so they appear as-is.
    assert_eq!(format_key_display("f1"), "f1");
    assert_eq!(format_key_display("f12"), "f12");
}

#[test]
fn display_via_key_event_data_method() {
    let data = make_key(KeyCode::Left, KeyModifiers::empty());
    assert_eq!(data.display(), "\u{2190}");

    let data2 = make_key(KeyCode::Char('p'), KeyModifiers::CONTROL);
    assert_eq!(data2.display(), "^p");
}

// ===========================================================================
// 4. Identifier conversion tests
// ===========================================================================

#[test]
fn identifier_simple_keys() {
    assert_eq!(key_to_identifier("a"), "a");
    assert_eq!(key_to_identifier("x"), "x");
    assert_eq!(key_to_identifier("5"), "5");
}

#[test]
fn identifier_modifier_combinations() {
    assert_eq!(key_to_identifier("ctrl+p"), "ctrl_p");
    assert_eq!(key_to_identifier("alt+x"), "alt_x");
    assert_eq!(key_to_identifier("shift+left"), "shift_left");
    assert_eq!(
        key_to_identifier("alt+ctrl+shift+left"),
        "alt_ctrl_shift_left"
    );
}

#[test]
fn identifier_uppercase_letters_get_upper_prefix() {
    assert_eq!(key_to_identifier("A"), "upper_a");
    assert_eq!(key_to_identifier("Z"), "upper_z");
    assert_eq!(key_to_identifier("M"), "upper_m");
}

#[test]
fn identifier_f_keys() {
    assert_eq!(key_to_identifier("f1"), "f1");
    assert_eq!(key_to_identifier("f12"), "f12");
}

#[test]
fn identifier_named_keys() {
    assert_eq!(key_to_identifier("enter"), "enter");
    assert_eq!(key_to_identifier("escape"), "escape");
    assert_eq!(key_to_identifier("backspace"), "backspace");
    assert_eq!(key_to_identifier("tab"), "tab");
    assert_eq!(key_to_identifier("space"), "space");
}

#[test]
fn identifier_via_key_event_data_method() {
    let data = make_key(KeyCode::Char('A'), KeyModifiers::SHIFT);
    assert_eq!(data.identifier(), "upper_a");

    let data2 = make_key(KeyCode::Char('a'), KeyModifiers::CONTROL);
    assert_eq!(data2.identifier(), "ctrl_a");

    let data3 = make_key(KeyCode::F(5), KeyModifiers::empty());
    assert_eq!(data3.identifier(), "f5");
}

// ===========================================================================
// 5. Deref compatibility tests
// ===========================================================================

#[test]
fn deref_exposes_code() {
    let data = make_key(KeyCode::Char('x'), KeyModifiers::CONTROL);
    assert_eq!(data.code, KeyCode::Char('x'));
}

#[test]
fn deref_exposes_modifiers() {
    let data = make_key(KeyCode::Char('x'), KeyModifiers::CONTROL);
    assert_eq!(data.modifiers, KeyModifiers::CONTROL);
}

#[test]
fn deref_exposes_kind() {
    let data = make_key(KeyCode::Char('x'), KeyModifiers::CONTROL);
    assert_eq!(data.kind, KeyEventKind::Press);
}

#[test]
fn deref_exposes_state() {
    let data = make_key(KeyCode::Char('x'), KeyModifiers::CONTROL);
    assert_eq!(data.state, KeyEventState::NONE);
}

#[test]
fn raw_accessor_matches_deref() {
    let data = make_key(KeyCode::Enter, KeyModifiers::ALT);
    assert_eq!(data.raw().code, data.code);
    assert_eq!(data.raw().modifiers, data.modifiers);
    assert_eq!(data.raw().kind, data.kind);
    assert_eq!(data.raw().state, data.state);
}

#[test]
fn deref_multiple_modifiers() {
    let mods = KeyModifiers::CONTROL | KeyModifiers::SHIFT;
    let data = make_key(KeyCode::Char('A'), mods);
    assert_eq!(data.modifiers, mods);
    assert_eq!(data.code, KeyCode::Char('A'));
}

// ===========================================================================
// 6. Edge cases
// ===========================================================================

#[test]
fn edge_null_key() {
    let data = make_key(KeyCode::Null, KeyModifiers::empty());
    assert_eq!(data.key, "null");
    assert_eq!(data.character, None);
    assert!(!data.is_printable);
}

#[test]
fn edge_media_keys() {
    let cases = [
        (MediaKeyCode::Play, "media_play"),
        (MediaKeyCode::Pause, "media_pause"),
        (MediaKeyCode::PlayPause, "media_play_pause"),
        (MediaKeyCode::Stop, "media_stop"),
        (MediaKeyCode::Reverse, "media_reverse"),
        (MediaKeyCode::FastForward, "media_fast_forward"),
        (MediaKeyCode::Rewind, "media_rewind"),
        (MediaKeyCode::TrackNext, "media_track_next"),
        (MediaKeyCode::TrackPrevious, "media_track_previous"),
        (MediaKeyCode::Record, "media_record"),
        (MediaKeyCode::LowerVolume, "media_lower_volume"),
        (MediaKeyCode::RaiseVolume, "media_raise_volume"),
        (MediaKeyCode::MuteVolume, "media_mute_volume"),
    ];
    for (media_code, expected) in cases {
        let data = make_key(KeyCode::Media(media_code), KeyModifiers::empty());
        assert_eq!(data.key, expected, "media key {expected}");
        assert_eq!(data.character, None);
        assert!(!data.is_printable);
    }
}

#[test]
fn edge_modifier_keys() {
    let cases = [
        (ModifierKeyCode::LeftShift, "left_shift"),
        (ModifierKeyCode::RightShift, "right_shift"),
        (ModifierKeyCode::LeftControl, "left_control"),
        (ModifierKeyCode::RightControl, "right_control"),
        (ModifierKeyCode::LeftAlt, "left_alt"),
        (ModifierKeyCode::RightAlt, "right_alt"),
        (ModifierKeyCode::LeftSuper, "left_super"),
        (ModifierKeyCode::RightSuper, "right_super"),
        (ModifierKeyCode::LeftHyper, "left_hyper"),
        (ModifierKeyCode::RightHyper, "right_hyper"),
        (ModifierKeyCode::LeftMeta, "left_meta"),
        (ModifierKeyCode::RightMeta, "right_meta"),
        (ModifierKeyCode::IsoLevel3Shift, "iso_level3_shift"),
        (ModifierKeyCode::IsoLevel5Shift, "iso_level5_shift"),
    ];
    for (mod_code, expected) in cases {
        let data = make_key(KeyCode::Modifier(mod_code), KeyModifiers::empty());
        assert_eq!(data.key, expected, "modifier key {expected}");
        assert_eq!(data.character, None);
        assert!(!data.is_printable);
    }
}

#[test]
fn edge_empty_modifiers_vs_none() {
    // KeyModifiers::empty() and KeyModifiers::NONE should behave identically.
    let data1 = make_key(KeyCode::Char('a'), KeyModifiers::empty());
    let data2 = make_key(KeyCode::Char('a'), KeyModifiers::NONE);
    assert_eq!(data1.key, data2.key);
    assert_eq!(data1.character, data2.character);
    assert_eq!(data1.is_printable, data2.is_printable);
}

#[test]
fn edge_key_repeat_kind() {
    // KeyEventKind::Repeat should still produce correct normalization.
    let data = make_key_with_kind(
        KeyCode::Char('a'),
        KeyModifiers::empty(),
        KeyEventKind::Repeat,
    );
    assert_eq!(data.key, "a");
    assert_eq!(data.character, Some('a'));
    assert!(data.is_printable);
    assert_eq!(data.kind, KeyEventKind::Repeat);
}

#[test]
fn edge_key_release_kind() {
    let data = make_key_with_kind(
        KeyCode::Char('a'),
        KeyModifiers::empty(),
        KeyEventKind::Release,
    );
    assert_eq!(data.key, "a");
    assert_eq!(data.kind, KeyEventKind::Release);
}

#[test]
fn edge_control_char_not_printable() {
    // A literal control character (e.g. \x01) is not printable.
    let data = make_key(KeyCode::Char('\x01'), KeyModifiers::empty());
    assert!(
        !data.is_printable,
        "control char \\x01 should not be printable"
    );
}

#[test]
fn edge_control_char_null_byte() {
    let data = make_key(KeyCode::Char('\0'), KeyModifiers::empty());
    assert!(!data.is_printable, "null byte should not be printable");
}

#[test]
fn edge_lock_and_special_keys() {
    let cases = [
        (KeyCode::CapsLock, "capslock"),
        (KeyCode::ScrollLock, "scrolllock"),
        (KeyCode::NumLock, "numlock"),
        (KeyCode::PrintScreen, "printscreen"),
        (KeyCode::Pause, "pause"),
        (KeyCode::Menu, "menu"),
        (KeyCode::KeypadBegin, "keypad_begin"),
        (KeyCode::Home, "home"),
        (KeyCode::End, "end"),
        (KeyCode::PageUp, "pageup"),
        (KeyCode::PageDown, "pagedown"),
        (KeyCode::Insert, "insert"),
    ];
    for (code, expected) in cases {
        let data = make_key(code, KeyModifiers::empty());
        assert_eq!(data.key, expected, "special key {expected}");
        assert_eq!(data.character, None);
        assert!(!data.is_printable);
    }
}

#[test]
fn edge_symbol_shift_consumed() {
    // Shift+1 produces '!' on US layout -- Shift should be stripped from
    // the canonical name since the character itself encodes the shift.
    let data = make_key(KeyCode::Char('!'), KeyModifiers::SHIFT);
    assert_eq!(data.key, "exclamation_mark");
    // No "shift+" prefix since Shift is consumed.
    assert!(!data.key.contains("shift+"));
}

#[test]
fn edge_all_modifier_ordering() {
    // Modifiers must appear in alphabetical order:
    // alt+ctrl+hyper+meta+shift+super.
    let mods = KeyModifiers::SHIFT
        | KeyModifiers::CONTROL
        | KeyModifiers::ALT
        | KeyModifiers::SUPER
        | KeyModifiers::HYPER
        | KeyModifiers::META;
    let data = make_key(KeyCode::Up, mods);
    assert_eq!(data.key, "alt+ctrl+hyper+meta+shift+super+up");
}

#[test]
fn edge_name_method_matches_key_field() {
    let data = make_key(KeyCode::Char('q'), KeyModifiers::CONTROL);
    assert_eq!(data.name(), &data.key);
    assert_eq!(data.name(), "ctrl+q");
}

#[test]
fn edge_super_char_is_not_printable() {
    let data = make_key(KeyCode::Char('a'), KeyModifiers::SUPER);
    assert_eq!(data.key, "super+a");
    assert_eq!(data.character, None);
    assert!(!data.is_printable);
}

#[test]
fn edge_hyper_char_is_not_printable() {
    let data = make_key(KeyCode::Char('a'), KeyModifiers::HYPER);
    assert_eq!(data.key, "hyper+a");
    assert_eq!(data.character, None);
    assert!(!data.is_printable);
}

#[test]
fn edge_meta_char_is_not_printable() {
    let data = make_key(KeyCode::Char('a'), KeyModifiers::META);
    assert_eq!(data.key, "meta+a");
    assert_eq!(data.character, None);
    assert!(!data.is_printable);
}

// ===========================================================================
// 7. ActionMap integration
// ===========================================================================

#[test]
fn action_map_keybind_from_event_simple() {
    let data = make_key(KeyCode::Tab, KeyModifiers::empty());
    let bind = KeyBind::from_event(&data);
    assert_eq!(bind.code, KeyCode::Tab);
    assert_eq!(bind.modifiers, KeyModifiers::empty());
}

#[test]
fn action_map_keybind_from_event_with_modifiers() {
    let mods = KeyModifiers::CONTROL | KeyModifiers::SHIFT;
    let data = make_key(KeyCode::Char('A'), mods);
    let bind = KeyBind::from_event(&data);
    assert_eq!(bind.code, KeyCode::Char('A'));
    assert_eq!(bind.modifiers, mods);
}

#[test]
fn action_map_bind_and_lookup() {
    let mut map = ActionMap::new();
    let bind = KeyBind::new(KeyCode::Tab, KeyModifiers::empty());
    map.bind(bind, Action::FocusNext);

    assert_eq!(map.lookup(&bind), Some(Action::FocusNext));
}

#[test]
fn action_map_lookup_miss() {
    let map = ActionMap::new();
    let bind = KeyBind::new(KeyCode::Char('z'), KeyModifiers::empty());
    assert_eq!(map.lookup(&bind), None);
}

#[test]
fn action_map_keybind_from_event_roundtrip() {
    // Create a KeyEventData, derive a KeyBind, bind an action, then verify
    // that a subsequent event produces a matching KeyBind that retrieves the
    // action.
    let mut map = ActionMap::new();

    let data = make_key(KeyCode::Char('n'), KeyModifiers::CONTROL);
    let bind = KeyBind::from_event(&data);
    map.bind(bind, Action::FocusNext);

    // Simulate the same key press again.
    let data2 = make_key(KeyCode::Char('n'), KeyModifiers::CONTROL);
    let bind2 = KeyBind::from_event(&data2);
    assert_eq!(
        map.lookup(&bind2),
        Some(Action::FocusNext),
        "same key press should match bound action"
    );
}

#[test]
fn action_map_multiple_bindings() {
    let mut map = ActionMap::new();

    map.bind(
        KeyBind::new(KeyCode::Tab, KeyModifiers::empty()),
        Action::FocusNext,
    );
    map.bind(
        KeyBind::new(KeyCode::BackTab, KeyModifiers::SHIFT),
        Action::FocusPrev,
    );
    map.bind(
        KeyBind::new(KeyCode::Up, KeyModifiers::empty()),
        Action::ScrollUp,
    );
    map.bind(
        KeyBind::new(KeyCode::Down, KeyModifiers::empty()),
        Action::ScrollDown,
    );

    assert_eq!(
        map.lookup(&KeyBind::new(KeyCode::Tab, KeyModifiers::empty())),
        Some(Action::FocusNext)
    );
    assert_eq!(
        map.lookup(&KeyBind::new(KeyCode::BackTab, KeyModifiers::SHIFT)),
        Some(Action::FocusPrev)
    );
    assert_eq!(
        map.lookup(&KeyBind::new(KeyCode::Up, KeyModifiers::empty())),
        Some(Action::ScrollUp)
    );
    assert_eq!(
        map.lookup(&KeyBind::new(KeyCode::Down, KeyModifiers::empty())),
        Some(Action::ScrollDown)
    );
}

// ===========================================================================
// Additional comprehensive round-trip tests for symbols
// ===========================================================================

#[test]
fn roundtrip_common_symbols() {
    let cases: &[(char, &str)] = &[
        ('!', "exclamation_mark"),
        ('@', "at"),
        ('#', "number_sign"),
        ('$', "dollar_sign"),
        ('%', "percent_sign"),
        ('^', "circumflex_accent"),
        ('&', "ampersand"),
        ('*', "asterisk"),
        ('(', "left_parenthesis"),
        (')', "right_parenthesis"),
        ('-', "minus"),
        ('_', "underscore"),
        ('+', "plus"),
        ('=', "equals_sign"),
        ('[', "left_square_bracket"),
        (']', "right_square_bracket"),
        ('{', "left_curly_bracket"),
        ('}', "right_curly_bracket"),
        ('\\', "backslash"),
        ('|', "vertical_line"),
        (';', "semicolon"),
        (':', "colon"),
        ('\'', "apostrophe"),
        ('"', "quotation_mark"),
        (',', "comma"),
        ('.', "full_stop"),
        ('<', "less_than_sign"),
        ('>', "greater_than_sign"),
        ('/', "slash"),
        ('?', "question_mark"),
        ('`', "grave_accent"),
        ('~', "tilde"),
        (' ', "space"),
    ];
    for &(ch, expected_name) in cases {
        let data = make_key(KeyCode::Char(ch), KeyModifiers::empty());
        assert_eq!(
            data.key, expected_name,
            "symbol '{ch}' should have canonical name '{expected_name}'"
        );
        assert_eq!(data.character, Some(ch));
        // Space and printable symbols are printable, control chars are not.
        if !ch.is_control() {
            assert!(data.is_printable, "'{ch}' should be printable");
        }
    }
}

#[test]
fn roundtrip_ctrl_with_symbol_key() {
    // Ctrl + symbol should apply ctrl+ prefix.
    let data = make_key(KeyCode::Char('/'), KeyModifiers::CONTROL);
    assert_eq!(data.key, "ctrl+slash");
    assert_eq!(data.character, None);
    assert!(!data.is_printable);
}

#[test]
fn roundtrip_alt_with_digit() {
    let data = make_key(KeyCode::Char('3'), KeyModifiers::ALT);
    assert_eq!(data.key, "alt+3");
    assert_eq!(data.character, None);
    assert!(!data.is_printable);
}
