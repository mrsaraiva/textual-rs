use rich_rs::{Console, ConsoleOptions, Renderable, Segments};
use std::collections::HashSet;
use std::time::Instant;

use crate::event::{Event, EventCtx};
use crate::message::{Message, MessageEvent};
use crate::style::{Color, parse_color_like};
use crate::validation::{ValidationResult, ValidatorRef};

use crate::node_id::NodeId;

use super::{
    Widget, WidgetStyles,
    helpers::{adjust_line_length_no_bg, empty_classes, fixed_height_from_constraints},
    input_chrome::InputChrome,
    text_edit::{
        EditCommand, MoveUnit, byte_index_from_cell_x, edit_command_from_key, first_clipboard_line,
    },
};

// ---------------------------------------------------------------------------
// CharFlags — simple bitmask (no external crate needed)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CharFlags(u8);

impl CharFlags {
    const NONE: Self = Self(0);
    const REQUIRED: Self = Self(1 << 0);
    const SEPARATOR: Self = Self(1 << 1);
    const UPPERCASE: Self = Self(1 << 2);
    const LOWERCASE: Self = Self(1 << 3);

    fn contains(self, flag: Self) -> bool {
        (self.0 & flag.0) == flag.0
    }

    fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

// ---------------------------------------------------------------------------
// Pattern matching (no regex crate — hand-rolled matchers)
// ---------------------------------------------------------------------------

/// A simple character class matcher, replacing regex patterns.
#[derive(Debug, Clone, Copy)]
enum CharPattern {
    Alpha,        // [A-Za-z]
    AlphaNum,     // [A-Za-z0-9]
    NonSpace,     // [^ ]
    Digit,        // [0-9]
    NonZeroDigit, // [1-9]
    DigitOrSign,  // [0-9+\-]
    HexDigit,     // [A-Fa-f0-9]
    BinaryDigit,  // [0-1]
    Literal(char),
}

impl CharPattern {
    fn matches(self, ch: char) -> bool {
        match self {
            CharPattern::Alpha => ch.is_ascii_alphabetic(),
            CharPattern::AlphaNum => ch.is_ascii_alphanumeric(),
            CharPattern::NonSpace => ch != ' ',
            CharPattern::Digit => ch.is_ascii_digit(),
            CharPattern::NonZeroDigit => ch.is_ascii_digit() && ch != '0',
            CharPattern::DigitOrSign => ch.is_ascii_digit() || ch == '+' || ch == '-',
            CharPattern::HexDigit => ch.is_ascii_hexdigit(),
            CharPattern::BinaryDigit => ch == '0' || ch == '1',
            CharPattern::Literal(expected) => ch == expected,
        }
    }
}

// ---------------------------------------------------------------------------
// CharDefinition
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct CharDefinition {
    pattern: CharPattern,
    flags: CharFlags,
    /// For separators: the literal char. For non-separators: the placeholder char.
    char: char,
}

impl CharDefinition {
    fn is_separator(self) -> bool {
        self.flags.contains(CharFlags::SEPARATOR)
    }

    fn is_required(self) -> bool {
        self.flags.contains(CharFlags::REQUIRED)
    }

    fn matches(self, ch: char) -> bool {
        self.pattern.matches(ch)
    }
}

// ---------------------------------------------------------------------------
// Template character definitions
// ---------------------------------------------------------------------------

fn template_char_def(ch: char) -> Option<(CharPattern, bool)> {
    match ch {
        'A' => Some((CharPattern::Alpha, true)),
        'a' => Some((CharPattern::Alpha, false)),
        'N' => Some((CharPattern::AlphaNum, true)),
        'n' => Some((CharPattern::AlphaNum, false)),
        'X' => Some((CharPattern::NonSpace, true)),
        'x' => Some((CharPattern::NonSpace, false)),
        '9' => Some((CharPattern::Digit, true)),
        '0' => Some((CharPattern::Digit, false)),
        'D' => Some((CharPattern::NonZeroDigit, true)),
        'd' => Some((CharPattern::NonZeroDigit, false)),
        '#' => Some((CharPattern::DigitOrSign, false)),
        'H' => Some((CharPattern::HexDigit, true)),
        'h' => Some((CharPattern::HexDigit, false)),
        'B' => Some((CharPattern::BinaryDigit, true)),
        'b' => Some((CharPattern::BinaryDigit, false)),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Template
// ---------------------------------------------------------------------------

/// Parsed template mask that enforces character-level rules.
#[derive(Debug, Clone)]
struct Template {
    defs: Vec<CharDefinition>,
    blank: char,
}

impl Template {
    fn parse(template_str: &str) -> Self {
        let mut defs = Vec::new();
        let mut blank = ' ';
        let mut escaped = false;
        let mut case_flags = CharFlags::NONE;
        let mut chars = template_str.chars();

        while let Some(ch) = chars.next() {
            if escaped {
                let mut flags = CharFlags::SEPARATOR;
                flags = flags.union(case_flags);
                defs.push(CharDefinition {
                    pattern: CharPattern::Literal(ch),
                    flags,
                    char: ch,
                });
                escaped = false;
                continue;
            }

            match ch {
                '\\' => {
                    escaped = true;
                    continue;
                }
                ';' => {
                    if let Some(b) = chars.next() {
                        blank = b;
                    }
                    break;
                }
                '>' => {
                    case_flags = CharFlags::UPPERCASE;
                    continue;
                }
                '<' => {
                    case_flags = CharFlags::LOWERCASE;
                    continue;
                }
                '!' => {
                    case_flags = CharFlags::NONE;
                    continue;
                }
                _ => {}
            }

            if let Some((pattern, required)) = template_char_def(ch) {
                let mut flags = if required {
                    CharFlags::REQUIRED
                } else {
                    CharFlags::NONE
                };
                flags = flags.union(case_flags);
                defs.push(CharDefinition {
                    pattern,
                    flags,
                    char: blank,
                });
            } else {
                // Unknown character → treated as separator.
                let mut flags = CharFlags::SEPARATOR;
                flags = flags.union(case_flags);
                defs.push(CharDefinition {
                    pattern: CharPattern::Literal(ch),
                    flags,
                    char: ch,
                });
            }
        }

        assert!(
            defs.iter().any(|d| !d.is_separator()),
            "Template must contain at least one non-separator character"
        );

        // Update non-separator placeholder chars to the resolved blank (which
        // may have been set by a trailing `;X` clause after the defs were
        // already created with the initial default blank).
        for def in &mut defs {
            if !def.is_separator() {
                def.char = blank;
            }
        }

        Template { defs, blank }
    }

    fn len(&self) -> usize {
        self.defs.len()
    }

    // --- validation --------------------------------------------------------

    fn check(&self, value: &[char], allow_space: bool) -> bool {
        for (i, def) in self.defs.iter().enumerate() {
            let ch = if i < value.len() { value[i] } else { '\0' };
            if def.is_required() && !def.matches(ch) && (!allow_space || ch != ' ') {
                return false;
            }
        }
        true
    }

    fn validate(&self, value: &str) -> ValidationResult {
        let mut padded: Vec<char> = value.chars().collect();
        while padded.len() < self.defs.len() {
            padded.push('\0');
        }
        if self.check(&padded, false) {
            ValidationResult::success()
        } else {
            ValidationResult::failure("Value does not match template!")
        }
    }

    // --- separator helpers -------------------------------------------------

    fn at_separator(&self, position: usize) -> bool {
        position < self.defs.len() && self.defs[position].is_separator()
    }

    fn prev_separator_position(&self, position: usize) -> Option<usize> {
        if position == 0 {
            return None;
        }
        for i in (0..position).rev() {
            if self.defs[i].is_separator() {
                return Some(i);
            }
        }
        None
    }

    fn next_separator_position(&self, position: usize) -> Option<usize> {
        for i in (position + 1)..self.defs.len() {
            if self.defs[i].is_separator() {
                return Some(i);
            }
        }
        None
    }

    fn next_separator_char(&self, position: usize) -> Option<char> {
        self.next_separator_position(position)
            .map(|i| self.defs[i].char)
    }

    // --- insert_separators -------------------------------------------------

    fn insert_separators(&self, value: &[char], cursor: usize) -> (Vec<char>, usize) {
        let mut chars = value.to_vec();
        let mut pos = cursor;
        while pos < self.defs.len() && self.defs[pos].is_separator() {
            let sep_ch = self.defs[pos].char;
            if pos < chars.len() {
                chars[pos] = sep_ch;
            } else {
                chars.push(sep_ch);
            }
            pos += 1;
        }
        (chars, pos)
    }

    // --- insert text -------------------------------------------------------

    fn insert_text_at(
        &self,
        value: &[char],
        cursor: usize,
        text: &str,
    ) -> Option<(Vec<char>, usize)> {
        let mut chars = value.to_vec();
        let mut pos = cursor;

        let separators: HashSet<char> = self
            .defs
            .iter()
            .filter(|d| d.is_separator())
            .map(|d| d.char)
            .collect();

        for ch in text.chars() {
            if separators.contains(&ch) {
                if Some(ch) == self.next_separator_char(pos) {
                    let prev_pos = self.prev_separator_position(pos);
                    let prev_is_adjacent = prev_pos.is_none_or(|p| p == pos.wrapping_sub(1));
                    if pos > 0 && !prev_is_adjacent {
                        let next_pos = self.next_separator_position(pos).unwrap_or(self.defs.len());
                        while pos < next_pos + 1 {
                            let fill = if pos < self.defs.len() && self.defs[pos].is_separator() {
                                self.defs[pos].char
                            } else {
                                ' '
                            };
                            if pos < chars.len() {
                                chars[pos] = fill;
                            } else {
                                chars.push(fill);
                            }
                            pos += 1;
                        }
                    }
                }
                continue;
            }

            if pos >= self.defs.len() {
                break;
            }

            let def = &self.defs[pos];
            debug_assert!(!def.is_separator());

            if !def.matches(ch) {
                return None;
            }

            let ch = if def.flags.contains(CharFlags::LOWERCASE) {
                ch.to_lowercase().next().unwrap_or(ch)
            } else if def.flags.contains(CharFlags::UPPERCASE) {
                ch.to_uppercase().next().unwrap_or(ch)
            } else {
                ch
            };

            if pos < chars.len() {
                chars[pos] = ch;
            } else {
                chars.push(ch);
            }
            pos += 1;

            let (new_chars, new_pos) = self.insert_separators(&chars, pos);
            chars = new_chars;
            pos = new_pos;
        }

        Some((chars, pos))
    }

    // --- move cursor -------------------------------------------------------

    fn move_cursor(&self, cursor: usize, delta: i32) -> usize {
        if delta < 0 {
            let all_seps = (0..cursor).all(|i| self.defs[i].is_separator());
            if all_seps {
                return cursor;
            }
        }

        let mut pos = cursor as i32 + delta;
        while pos >= 0 && (pos as usize) < self.defs.len() && self.defs[pos as usize].is_separator()
        {
            pos += delta;
        }
        (pos.max(0) as usize).min(self.defs.len())
    }

    // --- delete at position ------------------------------------------------

    fn delete_at(&self, value: &[char], position: usize) -> (Vec<char>, usize) {
        let mut chars = value.to_vec();
        let pos = position;

        if pos < self.defs.len() {
            debug_assert!(!self.defs[pos].is_separator());
            if pos == chars.len().saturating_sub(1) {
                chars.truncate(pos);
            } else if pos < chars.len() {
                chars[pos] = ' ';
            }
        }

        // Trim trailing spaces and separators.
        let mut trim_pos = chars.len();
        while trim_pos > 0 {
            let def = &self.defs[trim_pos - 1];
            if !def.is_separator() && chars[trim_pos - 1] != ' ' {
                break;
            }
            trim_pos -= 1;
        }
        chars.truncate(trim_pos);

        let new_pos = if pos > chars.len() { chars.len() } else { pos };
        self.insert_separators(&chars, new_pos)
    }

    // --- display -----------------------------------------------------------

    fn display(&self, value: &[char]) -> Vec<char> {
        let mut result = Vec::with_capacity(value.len());
        for (i, &ch) in value.iter().enumerate() {
            if ch == ' ' && i < self.defs.len() {
                result.push(self.defs[i].char);
            } else {
                result.push(ch);
            }
        }
        result
    }

    fn mask(&self) -> Vec<char> {
        self.defs.iter().map(|d| d.char).collect()
    }

    fn empty_mask(&self) -> Vec<char> {
        self.defs
            .iter()
            .map(|d| if d.is_separator() { d.char } else { ' ' })
            .collect()
    }

    fn update_mask(&mut self, placeholder: &str) {
        let ph_chars: Vec<char> = placeholder.chars().collect();
        for (i, def) in self.defs.iter_mut().enumerate() {
            if !def.is_separator() {
                def.char = if i < ph_chars.len() {
                    ph_chars[i]
                } else {
                    self.blank
                };
            }
        }
    }
}

// ---------------------------------------------------------------------------
// MaskedInput
// ---------------------------------------------------------------------------

pub struct MaskedInput {
    template: Template,
    /// Current value as a char vec (positions correspond 1:1 with template defs).
    value: Vec<char>,
    cursor: usize,
    placeholder: String,
    validators: Vec<ValidatorRef>,
    validation_result: ValidationResult,
    chrome: InputChrome,
    styles: WidgetStyles,
}

impl MaskedInput {
    pub fn new(template_str: impl Into<String>) -> Self {
        let template_str = template_str.into();
        let mut template = Template::parse(&template_str);
        template.update_mask("");
        let (value, cursor) = template.insert_separators(&[], 0);
        let mut out = Self {
            template,
            value,
            cursor,
            placeholder: String::new(),
            validators: Vec::new(),
            validation_result: ValidationResult::success(),
            chrome: InputChrome::new(),
            styles: WidgetStyles::default(),
        };
        out.revalidate();
        out
    }

    pub fn with_value(mut self, value: impl Into<String>) -> Self {
        let v: Vec<char> = value.into().chars().collect();
        if !v.is_empty() {
            let (v, _) = self.template.insert_separators(&v, 0);
            self.value = v;
        }
        self.revalidate();
        self
    }

    pub fn with_placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self.template.update_mask(&self.placeholder);
        self
    }

    pub fn with_validators(mut self, validators: Vec<ValidatorRef>) -> Self {
        self.validators = validators;
        self.revalidate();
        self
    }

    pub fn class(mut self, class: impl Into<String>) -> Self {
        self.chrome.add_user_class(class.into());
        self
    }

    pub fn set_class(&mut self, class: &str, enabled: bool) {
        self.chrome.set_class(class, enabled);
    }

    /// Returns the current value as a string.
    pub fn text(&self) -> String {
        self.value.iter().collect()
    }

    pub fn validation_result(&self) -> &ValidationResult {
        &self.validation_result
    }

    pub fn set_text(&mut self, value: impl Into<String>) {
        let v: Vec<char> = value.into().chars().collect();
        if self.template.check(&v, true) {
            self.value = v;
            if self.cursor > self.value.len() {
                self.cursor = self.value.len();
            }
            self.revalidate();
        }
    }

    pub fn clear(&mut self) {
        let (value, cursor) = self.template.insert_separators(&[], 0);
        self.value = value;
        self.cursor = cursor;
        self.revalidate();
    }

    /// Replace the template at runtime, re-parsing and resetting content/cursor state.
    ///
    /// Returns `Err` if the template string contains no non-separator characters.
    pub fn set_template(&mut self, template_str: &str) -> Result<(), String> {
        // Validate before modifying state: template must have at least one editable slot.
        let has_editable = {
            let mut escaped = false;
            let mut found = false;
            for ch in template_str.chars() {
                if escaped {
                    escaped = false;
                    continue;
                }
                match ch {
                    '\\' => {
                        escaped = true;
                        continue;
                    }
                    ';' => break,
                    '>' | '<' | '!' => continue,
                    _ => {
                        if template_char_def(ch).is_some() {
                            found = true;
                            break;
                        }
                    }
                }
            }
            found
        };
        if !has_editable {
            return Err(
                "Template must contain at least one non-separator character".to_string(),
            );
        }

        let mut template = Template::parse(template_str);
        let placeholder = self.placeholder.clone();
        template.update_mask(&placeholder);
        let (value, cursor) = template.insert_separators(&[], 0);
        self.template = template;
        self.value = value;
        self.cursor = cursor;
        self.revalidate();
        Ok(())
    }

    // --- internal helpers --------------------------------------------------

    fn value_str(&self) -> String {
        self.value.iter().collect()
    }

    fn post_changed(&mut self, ctx: &mut EventCtx) {
        ctx.post_message(Message::InputChanged {
            value: self.value_str(),
            validation: self.validation_result.clone(),
        });
    }

    fn copy_text(&self) -> Option<String> {
        let text = self.value_str();
        if text.trim().is_empty() {
            None
        } else {
            Some(text)
        }
    }

    fn revalidate(&mut self) {
        let value_str = self.value_str();

        let template_result = self.template.validate(&value_str);

        let mut failures: Vec<String> = Vec::new();
        if !template_result.is_valid {
            failures.extend(template_result.failure_descriptions);
        }
        for validator in &self.validators {
            let result = validator.validate(&value_str);
            if !result.is_valid {
                failures.extend(result.failure_descriptions);
            }
        }

        self.validation_result = if failures.is_empty() {
            ValidationResult::success()
        } else {
            ValidationResult {
                is_valid: false,
                failure_descriptions: failures,
            }
        };

        let trimmed = value_str.trim().is_empty();
        if trimmed {
            self.set_class("-valid", false);
            self.set_class("-invalid", false);
        } else if self.validation_result.is_valid {
            self.set_class("-valid", true);
            self.set_class("-invalid", false);
        } else {
            self.set_class("-valid", false);
            self.set_class("-invalid", true);
        }
    }

    fn cursor_from_x(&self, x: u16) -> usize {
        let slots = self.display_slots();
        let display: String = slots.iter().collect();
        let byte_idx = byte_index_from_cell_x(&display, x as usize);
        display[..byte_idx].chars().count().min(self.template.len())
    }

    fn display_slots(&self) -> Vec<char> {
        let mut slots = self.template.mask();
        let display_chars = self.template.display(&self.value);
        for (idx, ch) in display_chars.into_iter().enumerate() {
            if idx < slots.len() {
                slots[idx] = ch;
            } else {
                slots.push(ch);
            }
        }
        slots
    }

    // --- masked actions (ported from Python _masked_input.py) ---------------

    fn action_insert_text(&mut self, text: &str) -> bool {
        if let Some((new_val, new_cursor)) =
            self.template.insert_text_at(&self.value, self.cursor, text)
        {
            self.value = new_val;
            self.cursor = new_cursor;
            true
        } else {
            false
        }
    }

    fn action_cursor_left(&mut self) {
        self.cursor = self.template.move_cursor(self.cursor, -1);
    }

    fn action_cursor_right(&mut self) {
        self.cursor = self.template.move_cursor(self.cursor, 1);
    }

    fn action_home(&mut self) {
        self.cursor = self
            .template
            .move_cursor(self.cursor, -(self.template.len() as i32));
        // If position 0 is a separator, skip forward to first editable slot.
        if self.cursor < self.template.len() && self.template.at_separator(self.cursor) {
            self.cursor = self.template.move_cursor(self.cursor, 1);
        }
    }

    fn action_end(&mut self) {
        self.cursor = self.template.mask().len();
    }

    fn action_cursor_left_word(&mut self) {
        let pos = if self.cursor > 0 && self.template.at_separator(self.cursor - 1) {
            self.template.prev_separator_position(self.cursor - 1)
        } else {
            self.template.prev_separator_position(self.cursor)
        };
        self.cursor = pos.map(|p| p + 1).unwrap_or(0);
    }

    fn action_cursor_right_word(&mut self) {
        let pos = self.template.next_separator_position(self.cursor);
        self.cursor = pos
            .map(|p| p + 1)
            .unwrap_or_else(|| self.template.mask().len());
    }

    fn action_delete_right(&mut self) {
        if self.cursor < self.template.len() && !self.template.at_separator(self.cursor) {
            let (new_val, new_cursor) = self.template.delete_at(&self.value, self.cursor);
            self.value = new_val;
            self.cursor = new_cursor;
        }
    }

    fn action_delete_left(&mut self) {
        if self.cursor == 0 {
            return;
        }
        self.cursor = self.template.move_cursor(self.cursor, -1);
        let (new_val, new_cursor) = self.template.delete_at(&self.value, self.cursor);
        self.value = new_val;
        self.cursor = new_cursor;
    }

    fn action_delete_right_word(&mut self) {
        let end = self
            .template
            .next_separator_position(self.cursor)
            .map(|p| p + 1)
            .unwrap_or(self.value.len());
        let start = self.cursor;
        // Delete non-separator chars from start..end. Since delete shifts values,
        // we repeatedly delete at `start` for each non-separator position.
        for i in start..end {
            if !self.template.at_separator(i) {
                let (new_val, _) = self.template.delete_at(&self.value, start);
                self.value = new_val;
            }
        }
        let (new_val, new_cursor) = self.template.insert_separators(&self.value, self.cursor);
        self.value = new_val;
        self.cursor = new_cursor;
    }

    fn action_delete_left_word(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let target = if self.cursor > 0 && self.template.at_separator(self.cursor - 1) {
            self.template
                .prev_separator_position(self.cursor - 1)
                .map(|p| p + 1)
                .unwrap_or(0)
        } else {
            self.template
                .prev_separator_position(self.cursor)
                .map(|p| p + 1)
                .unwrap_or(0)
        };

        let original_cursor = self.cursor;
        for i in target..original_cursor {
            if !self.template.at_separator(i) {
                let (new_val, _) = self.template.delete_at(&self.value, target);
                self.value = new_val;
            }
        }
        self.cursor = target;
    }

    fn action_delete_left_all(&mut self) {
        if self.cursor > 0 {
            let cursor_pos = self.cursor;
            if cursor_pos >= self.value.len() {
                self.value.clear();
            } else {
                let empty_mask = self.template.empty_mask();
                for i in 0..cursor_pos.min(self.value.len()) {
                    if i < empty_mask.len() {
                        self.value[i] = empty_mask[i];
                    } else {
                        self.value[i] = ' ';
                    }
                }
            }
            self.cursor = 0;
        }
    }
}

impl Widget for MaskedInput {
    fn style_type(&self) -> &'static str {
        "MaskedInput"
    }

    fn focusable(&self) -> bool {
        true
    }

    fn set_focus(&mut self, focused: bool) {
        self.chrome.set_focus(focused);
    }

    fn has_focus(&self) -> bool {
        self.chrome.has_focus()
    }

    fn is_active(&self) -> bool {
        self.chrome.is_active()
    }

    fn on_mouse_move(&mut self, x: u16, _y: u16) -> bool {
        if !self.chrome.is_mouse_down() {
            return false;
        }
        let mut next = self.cursor_from_x(x);
        if next < self.template.len() && self.template.at_separator(next) {
            next = self.template.move_cursor(next, 1);
        }
        if next == self.cursor {
            return false;
        }
        self.cursor = next;
        true
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        match event {
            Event::AppFocus(active) => {
                self.chrome.handle_app_focus(*active);
                ctx.request_repaint();
            }
            // TODO(P1-14 integration): wire tree-based NodeId comparison
            Event::MouseDown(mouse) if mouse.target == NodeId::default() => {
                let pos = self.cursor_from_x(mouse.x);
                self.cursor = pos;
                if self.template.at_separator(self.cursor) {
                    self.cursor = self.template.move_cursor(self.cursor, 1);
                }
                self.chrome.set_mouse_down(true);
                self.chrome.reset_blink();
                ctx.request_repaint();
                ctx.set_handled();
            }
            Event::MouseUp(_) => {
                if self.chrome.is_mouse_down() {
                    self.chrome.set_mouse_down(false);
                    ctx.request_repaint();
                }
            }
            Event::Tick(_) => {
                if self.chrome.handle_tick(Instant::now()) {
                    ctx.request_repaint();
                }
            }
            Event::Key(key) if self.chrome.has_focus() => {
                let Some(cmd) = edit_command_from_key(key, false) else {
                    return;
                };
                let mut changed = false;
                let mut value_changed = false;
                match cmd {
                    EditCommand::DeleteToStart => {
                        self.action_delete_left_all();
                        changed = true;
                        value_changed = true;
                    }
                    EditCommand::InsertChar(ch) => {
                        if self.action_insert_text(&ch.to_string()) {
                            changed = true;
                            value_changed = true;
                        }
                    }
                    EditCommand::Submit => {
                        ctx.post_message(Message::InputSubmitted {
                            value: self.value_str(),
                        });
                    }
                    EditCommand::Copy => {
                        if let Some(text) = self.copy_text() {
                            ctx.post_message(Message::TextEditClipboardCopyRequested {
                                text,
                                cut: false,
                            });
                        }
                    }
                    EditCommand::Cut => {
                        if let Some(text) = self.copy_text() {
                            ctx.post_message(Message::TextEditClipboardCopyRequested {
                                text,
                                cut: true,
                            });
                            self.clear();
                            changed = true;
                            value_changed = true;
                        }
                    }
                    EditCommand::Paste => {
                        // TODO(P1-14 integration): wire tree-based NodeId comparison
                        ctx.post_message(Message::TextEditClipboardPasteRequested {
                            target: NodeId::default(),
                        });
                    }
                    EditCommand::Backspace { unit } => {
                        match unit {
                            MoveUnit::Grapheme => self.action_delete_left(),
                            MoveUnit::Word => self.action_delete_left_word(),
                        }
                        changed = true;
                        value_changed = true;
                    }
                    EditCommand::Delete { unit } => {
                        match unit {
                            MoveUnit::Grapheme => self.action_delete_right(),
                            MoveUnit::Word => self.action_delete_right_word(),
                        }
                        changed = true;
                        value_changed = true;
                    }
                    EditCommand::MoveLeft { unit, .. } => {
                        match unit {
                            MoveUnit::Grapheme => self.action_cursor_left(),
                            MoveUnit::Word => self.action_cursor_left_word(),
                        }
                        changed = true;
                    }
                    EditCommand::MoveRight { unit, .. } => {
                        match unit {
                            MoveUnit::Grapheme => self.action_cursor_right(),
                            MoveUnit::Word => self.action_cursor_right_word(),
                        }
                        changed = true;
                    }
                    EditCommand::MoveHome { .. } => {
                        self.action_home();
                        changed = true;
                    }
                    EditCommand::MoveEnd { .. } => {
                        self.action_end();
                        changed = true;
                    }
                    EditCommand::InsertNewline
                    | EditCommand::MoveUp { .. }
                    | EditCommand::MoveDown { .. }
                    | EditCommand::DeleteToEnd
                    | EditCommand::DeleteLine
                    | EditCommand::SelectAll
                    | EditCommand::SelectLine
                    | EditCommand::Undo
                    | EditCommand::Redo => {}
                }

                if value_changed {
                    self.revalidate();
                    self.post_changed(ctx);
                }
                if changed || value_changed {
                    self.chrome.reset_blink();
                    ctx.request_repaint();
                }
                ctx.set_handled();
            }
            _ => {}
        }
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut EventCtx) {
        if let Message::TextEditClipboardPaste { target, text } = &message.message {
            // TODO(P1-14 integration): wire tree-based NodeId comparison
            if *target != NodeId::default() {
                return;
            }
            if let Some(line) = first_clipboard_line(text) {
                if self.action_insert_text(line) {
                    self.revalidate();
                    self.post_changed(ctx);
                    self.chrome.reset_blink();
                    ctx.request_repaint();
                    ctx.set_handled();
                }
            }
        }
    }

    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);

        let base_meta = crate::css::selector_meta_generic(self);
        let base_style = crate::css::resolve_style(self, &base_meta);
        let fallback_bg = parse_color_like("$background").unwrap_or(Color::rgb(0, 0, 0));
        let base_bg = base_style.bg.unwrap_or(fallback_bg);

        let resolve_component_rich = |class: &str| -> rich_rs::Style {
            let meta = crate::css::selector_meta_component(self.style_type(), &[class]);
            let style = crate::css::resolve_style_for_meta(&meta);
            let mut rich = style
                .to_rich_without_colors()
                .unwrap_or_else(rich_rs::Style::new);
            let mut under_bg = base_bg;

            if let Some(bg) = style.bg {
                let flat = bg.flatten_over(under_bg);
                under_bg = flat;
                rich = rich.with_bgcolor(flat.to_simple_opaque());
            }
            if let Some(fg) = style.fg {
                let flat = fg.flatten_over(under_bg);
                rich = rich.with_color(flat.to_simple_opaque());
            }
            rich
        };

        let cursor_style = resolve_component_rich("input--cursor");
        let placeholder_style = resolve_component_rich("input--placeholder");

        #[derive(Clone, Copy, PartialEq, Eq)]
        enum SlotVisual {
            Normal,
            Placeholder,
            Cursor,
        }

        let mut runs: Vec<(SlotVisual, String)> = Vec::new();
        let mut push_char = |visual: SlotVisual, ch: char| {
            if let Some((last_visual, text)) = runs.last_mut()
                && *last_visual == visual
            {
                text.push(ch);
                return;
            }
            runs.push((visual, ch.to_string()));
        };

        for (idx, ch) in self.display_slots().into_iter().enumerate() {
            let is_cursor =
                self.chrome.has_focus() && self.chrome.cursor_visible() && idx == self.cursor;
            let original_is_space = idx < self.value.len() && self.value[idx] == ' ';
            let visual = if is_cursor {
                SlotVisual::Cursor
            } else if original_is_space || idx >= self.value.len() {
                SlotVisual::Placeholder
            } else {
                SlotVisual::Normal
            };
            push_char(visual, ch);
        }

        if self.chrome.has_focus()
            && self.chrome.cursor_visible()
            && self.cursor >= self.template.len()
        {
            push_char(SlotVisual::Cursor, ' ');
        }

        let mut out: Vec<rich_rs::Segment> = Vec::new();
        for (visual, text) in runs {
            let style = match visual {
                SlotVisual::Normal => rich_rs::Style::new(),
                SlotVisual::Placeholder => placeholder_style,
                SlotVisual::Cursor => cursor_style,
            };
            out.push(rich_rs::Segment::styled(text, style));
        }
        adjust_line_length_no_bg(&out, width).into()
    }

    fn layout_height(&self) -> Option<usize> {
        let meta = crate::css::selector_meta_generic(self);
        let base_style = crate::css::resolve_style(self, &meta);
        let default_height = 1 + super::helpers::border_vertical_padding(&base_style);
        fixed_height_from_constraints(self.layout_constraints()).or(Some(default_height))
    }

    fn style_classes(&self) -> &[String] {
        let classes = self.chrome.style_classes();
        if classes.is_empty() {
            empty_classes()
        } else {
            classes
        }
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

impl Renderable for MaskedInput {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::KeyEventData;
    use crate::node_id::NodeId;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use rich_rs::Console;

    #[test]
    fn template_parse_phone() {
        let t = Template::parse(r"\(999\) 999\-9999");
        assert_eq!(t.len(), 14);
        assert!(t.defs[0].is_separator()); // (
        assert!(!t.defs[1].is_separator()); // first digit
    }

    #[test]
    fn template_insert_separators() {
        let t = Template::parse(r"\(999\) 999\-9999");
        let (val, cursor) = t.insert_separators(&[], 0);
        assert_eq!(val, vec!['(']);
        assert_eq!(cursor, 1);
    }

    #[test]
    fn template_insert_text() {
        let t = Template::parse(r"\(999\) 999\-9999");
        let (val, cursor) = t.insert_separators(&[], 0);
        let result = t.insert_text_at(&val, cursor, "555");
        assert!(result.is_some());
        let (val, cursor) = result.unwrap();
        let s: String = val.iter().collect();
        assert_eq!(&s, "(555) ");
        assert_eq!(cursor, 6);
    }

    #[test]
    fn template_insert_full_phone() {
        let t = Template::parse(r"\(999\) 999\-9999");
        let (val, cursor) = t.insert_separators(&[], 0);
        let result = t.insert_text_at(&val, cursor, "5551234567");
        assert!(result.is_some());
        let (val, _cursor) = result.unwrap();
        let s: String = val.iter().collect();
        assert_eq!(&s, "(555) 123-4567");
    }

    #[test]
    fn template_rejects_invalid_char() {
        let t = Template::parse("999");
        let result = t.insert_text_at(&[], 0, "a");
        assert!(result.is_none());
    }

    #[test]
    fn template_case_forcing_upper() {
        let t = Template::parse(">AAAA");
        let result = t.insert_text_at(&[], 0, "abcd");
        assert!(result.is_some());
        let (val, _) = result.unwrap();
        let s: String = val.iter().collect();
        assert_eq!(&s, "ABCD");
    }

    #[test]
    fn template_case_forcing_lower() {
        let t = Template::parse("<AAAA");
        let result = t.insert_text_at(&[], 0, "ABCD");
        assert!(result.is_some());
        let (val, _) = result.unwrap();
        let s: String = val.iter().collect();
        assert_eq!(&s, "abcd");
    }

    #[test]
    fn template_validation_pass() {
        let t = Template::parse("999");
        assert!(t.validate("123").is_valid);
    }

    #[test]
    fn template_validation_fail_incomplete() {
        let t = Template::parse("999");
        assert!(!t.validate("12").is_valid);
    }

    #[test]
    fn template_validation_fail_wrong_char() {
        let t = Template::parse("999");
        assert!(!t.validate("abc").is_valid);
    }

    #[test]
    fn template_optional_not_required() {
        let t = Template::parse("990");
        // First two digits required, third optional.
        assert!(t.validate("12").is_valid);
        assert!(!t.validate("1").is_valid);
    }

    #[test]
    fn template_display() {
        let mut t = Template::parse(r"\(999\) 999\-9999");
        t.update_mask("______________");
        let val: Vec<char> = "(555) ".chars().collect();
        let display: String = t.display(&val).into_iter().collect();
        assert_eq!(display, "(555) ");
    }

    #[test]
    fn template_mask_chars() {
        let mut t = Template::parse(r"\(999\) 999\-9999");
        t.update_mask("______________");
        let mask: String = t.mask().into_iter().collect();
        assert_eq!(mask.len(), 14);
        assert_eq!(&mask[0..1], "(");
    }

    #[test]
    fn template_move_cursor_skips_separator() {
        let t = Template::parse(r"\(999\) 999\-9999");
        // Position 3 → next position 4 is ')' sep, 5 is ' ' sep → skip to 6.
        let pos = t.move_cursor(3, 1);
        assert_eq!(pos, 6);
    }

    #[test]
    fn template_delete_at() {
        let t = Template::parse("999");
        let val: Vec<char> = "123".chars().collect();
        let (new_val, cursor) = t.delete_at(&val, 1);
        let s: String = new_val.into_iter().collect();
        assert_eq!(s, "1 3");
        assert_eq!(cursor, 1);
    }

    #[test]
    fn masked_input_new_starts_with_separator() {
        let mi = MaskedInput::new(r"\(999\) 999\-9999");
        assert_eq!(mi.text(), "(");
    }

    #[test]
    fn template_blank_placeholder() {
        let t = Template::parse("999;_");
        assert_eq!(t.blank, '_');
        let mask: String = t.mask().into_iter().collect();
        assert_eq!(mask, "___");
    }

    #[test]
    fn template_hex_valid() {
        let t = Template::parse("HHHH");
        let result = t.insert_text_at(&[], 0, "A1f0");
        assert!(result.is_some());
        let (val, _) = result.unwrap();
        let s: String = val.iter().collect();
        assert_eq!(&s, "A1f0");
    }

    #[test]
    fn template_hex_invalid() {
        let t = Template::parse("HHHH");
        assert!(t.insert_text_at(&[], 0, "GHIJ").is_none());
    }

    #[test]
    fn template_binary_valid() {
        let t = Template::parse("BBBB");
        let result = t.insert_text_at(&[], 0, "1010");
        assert!(result.is_some());
    }

    #[test]
    fn template_binary_invalid() {
        let t = Template::parse("BBBB");
        assert!(t.insert_text_at(&[], 0, "1234").is_none());
    }

    #[test]
    fn template_mixed_case_modes() {
        // >AA forces upper, ! resets, AA normal, <AA forces lower
        let t = Template::parse(">AA!AA<AA");
        let result = t.insert_text_at(&[], 0, "abCDef");
        assert!(result.is_some());
        let (val, _) = result.unwrap();
        let s: String = val.iter().collect();
        assert_eq!(&s, "ABCDef");
    }

    #[test]
    fn masked_input_style_type() {
        let mi = MaskedInput::new("999");
        assert_eq!(mi.style_type(), "MaskedInput");
    }

    #[test]
    fn masked_input_typing_emits_input_changed_message() {
        let mut input = MaskedInput::new("999");
        input.set_focus(true);
        let mut ctx = EventCtx::default();

        input.on_event(
            &Event::Key(KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Char('1'),
                KeyModifiers::NONE,
            ))),
            &mut ctx,
        );

        let messages = ctx.take_messages();
        assert!(messages.iter().any(|m| matches!(
            m.message,
            Message::InputChanged { ref value, .. } if value.starts_with('1')
        )));
    }

    #[test]
    fn ctrl_u_clears_to_start_via_shared_command_map() {
        let mut input = MaskedInput::new("9999");
        input.set_focus(true);
        input.set_text("1234");
        input.cursor = 4;
        let mut ctx = EventCtx::default();

        input.on_event(
            &Event::Key(KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Char('u'),
                KeyModifiers::CONTROL,
            ))),
            &mut ctx,
        );

        assert_eq!(input.text(), "");
        assert!(ctx.handled());
    }

    #[test]
    fn masked_input_copy_cut_and_paste_hooks() {
        let mut input = MaskedInput::new("9999");
        input.set_focus(true);
        input.set_text("1234");

        let mut ctx = EventCtx::default();
        input.on_event(
            &Event::Key(KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Char('c'),
                KeyModifiers::CONTROL,
            ))),
            &mut ctx,
        );
        let copy_messages = ctx.take_messages();
        assert!(copy_messages.iter().any(|m| {
            matches!(
                m.message,
                Message::TextEditClipboardCopyRequested { ref text, cut: false } if text == "1234"
            )
        }));

        let mut ctx = EventCtx::default();
        input.on_event(
            &Event::Key(KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Char('x'),
                KeyModifiers::CONTROL,
            ))),
            &mut ctx,
        );
        let cut_messages = ctx.take_messages();
        assert!(cut_messages.iter().any(|m| {
            matches!(
                m.message,
                Message::TextEditClipboardCopyRequested { ref text, cut: true } if text == "1234"
            )
        }));
        assert_eq!(input.text(), "");

        let mut ctx = EventCtx::default();
        input.on_message(
            &MessageEvent {
                sender: NodeId::default(),
                message: Message::TextEditClipboardPaste {
                    target: NodeId::default(),
                    text: "9876".to_string(),
                },
            },
            &mut ctx,
        );
        assert_eq!(input.text(), "9876");
        assert!(ctx.handled());
    }

    #[test]
    fn masked_input_paste_uses_first_clipboard_line_only() {
        let mut input = MaskedInput::new("9999");
        input.set_focus(true);

        let mut ctx = EventCtx::default();
        input.on_message(
            &MessageEvent {
                sender: NodeId::default(),
                message: Message::TextEditClipboardPaste {
                    target: NodeId::default(),
                    text: "9876\n1234".to_string(),
                },
            },
            &mut ctx,
        );

        assert_eq!(input.text(), "9876");
        assert!(ctx.handled());
    }

    #[test]
    fn cursor_from_x_handles_zwj_and_combining_clusters() {
        let zwj_input = MaskedInput::new("👩‍🚀9");
        assert_eq!(zwj_input.cursor_from_x(0), 0);
        assert_eq!(zwj_input.cursor_from_x(1), 0);
        assert_eq!(zwj_input.cursor_from_x(2), 3);

        let combining_input = MaskedInput::new("e\u{0301}9");
        assert_eq!(combining_input.cursor_from_x(0), 0);
        assert_eq!(combining_input.cursor_from_x(1), 2);
    }

    #[test]
    fn render_clamps_wide_cells_to_viewport_width() {
        let input = MaskedInput::new("中9");
        let console = Console::new();
        let mut options = console.options().clone();
        options.size = (1, 1);
        options.max_width = 1;
        options.max_height = 1;

        let rendered = Widget::render(&input, &console, &options);
        assert_eq!(rendered.cell_len(), 1);
    }
}
