use crossterm::event::{KeyCode, KeyModifiers};
use regex::Regex;
use rich_rs::{Console, ConsoleOptions, Renderable, Segments};
use std::time::Instant;
use unicode_segmentation::UnicodeSegmentation;

use crate::event::{Event, EventCtx};
use crate::message::*;
use crate::reactive::{ReactiveChange, ReactiveCtx, ReactiveFlags, ReactiveWidget};
use crate::style::{Color, parse_color_like};
use crate::validation::{ValidationResult, ValidatorRef};

use crate::action::ParsedAction;

use super::{
    BindingDecl, NodeSeed, NodeState, Widget,
    input_chrome::InputChrome,
    text_edit::{
        EditCommand, MoveUnit, byte_index_from_cell_x, clamp_grapheme_boundary,
        edit_command_from_key, first_clipboard_line, grapheme_cell_width, next_grapheme_boundary,
        next_word_boundary, prev_grapheme_boundary, prev_word_boundary,
    },
};

// ---------------------------------------------------------------------------
// Suggester trait + built-in implementations
// ---------------------------------------------------------------------------

/// Provides auto-completion suggestions for an [`Input`] widget.
///
/// Implement this trait to supply custom suggestion logic.  The built-in
/// [`SuggestFromList`] covers the common case of matching against a fixed
/// list of strings.
pub trait Suggester: Send + Sync {
    /// Return a completion suggestion for the current input `value`, or
    /// `None` if no suggestion applies.
    ///
    /// The returned string should be the **full** replacement value (not just
    /// the suffix).  For example, if the user typed `"Por"` and the
    /// suggestion is `"Portugal"`, return `Some("Portugal")`.
    fn suggest(&self, value: &str) -> Option<String>;
}

/// A [`Suggester`] that matches against a fixed list of strings by prefix.
///
/// By default matching is **case-insensitive**.  The canonical casing of the
/// suggestion list is preserved in the returned value.
///
/// ```rust,ignore
/// use textual_rs::widgets::input::{Input, SuggestFromList};
///
/// let countries = vec!["England", "Scotland", "Portugal", "Spain", "France"];
/// let input = Input::new()
///     .with_suggester(SuggestFromList::new(countries, false));
/// ```
pub struct SuggestFromList {
    suggestions: Vec<String>,
    folded: Vec<String>,
    case_sensitive: bool,
}

impl SuggestFromList {
    /// Create a new prefix-matching suggester.
    ///
    /// * `suggestions` – the valid completions, ordered by priority
    ///   (first match wins).
    /// * `case_sensitive` – when `false` (the default), incoming values are
    ///   case-folded before comparison.
    pub fn new(
        suggestions: impl IntoIterator<Item = impl Into<String>>,
        case_sensitive: bool,
    ) -> Self {
        let suggestions: Vec<String> = suggestions.into_iter().map(Into::into).collect();
        let folded = if case_sensitive {
            suggestions.clone()
        } else {
            suggestions.iter().map(|s| s.to_lowercase()).collect()
        };
        Self {
            suggestions,
            folded,
            case_sensitive,
        }
    }
}

impl Suggester for SuggestFromList {
    fn suggest(&self, value: &str) -> Option<String> {
        if value.is_empty() {
            return None;
        }
        let needle = if self.case_sensitive {
            value.to_string()
        } else {
            value.to_lowercase()
        };
        for (idx, candidate) in self.folded.iter().enumerate() {
            if candidate.starts_with(&needle) {
                return Some(self.suggestions[idx].clone());
            }
        }
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputType {
    Text,
    Integer,
    Number,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Selection {
    start: usize,
    end: usize,
}

impl Selection {
    fn cursor(pos: usize) -> Self {
        Self {
            start: pos,
            end: pos,
        }
    }

    #[cfg(test)]
    fn normalized(self) -> (usize, usize) {
        if self.start <= self.end {
            (self.start, self.end)
        } else {
            (self.end, self.start)
        }
    }
}

pub struct Input {
    text: String,
    cursor: usize,
    selection: Selection,
    placeholder: Option<String>,
    input_type: InputType,
    password: bool,
    restrict: Option<Regex>,
    max_length: Option<usize>,
    pending_blur: bool,
    validators: Vec<ValidatorRef>,
    validation_result: ValidationResult,
    chrome: InputChrome,
    seed: NodeSeed,
    style_type_name: &'static str,
    style_type_aliases: Vec<&'static str>,
    suggester: Option<Box<dyn Suggester>>,
    suggestion: String,
}

impl Default for Input {
    fn default() -> Self {
        Self::new()
    }
}

impl Input {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            cursor: 0,
            selection: Selection::cursor(0),
            placeholder: None,
            input_type: InputType::Text,
            password: false,
            restrict: None,
            max_length: None,
            pending_blur: false,
            validators: Vec::new(),
            validation_result: ValidationResult::success(),
            chrome: InputChrome::new(),
            seed: {
                let mut s = NodeSeed::default();
                s.classes.push("input".to_string());
                s
            },
            style_type_name: "Input",
            style_type_aliases: Vec::new(),
            suggester: None,
            suggestion: String::new(),
        }
    }

    /// Override the CSS style type and optional base-type aliases.
    ///
    /// This enables Python-style subclass selector behavior. For example,
    /// `CommandInput` can use `style_type_name="CommandInput"` while still
    /// inheriting `Input` selector rules via aliases.
    pub fn with_style_type(
        mut self,
        style_type_name: &'static str,
        aliases: impl IntoIterator<Item = &'static str>,
    ) -> Self {
        self.style_type_name = style_type_name;
        self.style_type_aliases = aliases.into_iter().collect();
        self
    }

    pub fn with_placeholder(mut self, value: impl Into<String>) -> Self {
        self.placeholder = Some(value.into());
        self
    }

    /// Set an initial value for the input (Python `Input(value, ...)`).
    ///
    /// A non-empty initial value is rendered as the input's text — not the
    /// placeholder. The cursor is placed at the end of the value, matching the
    /// post-construction `set_text` behaviour.
    pub fn with_value(mut self, value: impl Into<String>) -> Self {
        self.set_text(value);
        self
    }

    pub fn with_type(mut self, input_type: InputType) -> Self {
        self.input_type = input_type;
        self
    }

    pub fn with_validators(mut self, validators: Vec<ValidatorRef>) -> Self {
        self.validators = validators;
        self.revalidate();
        self
    }

    pub fn class(mut self, class: impl Into<String>) -> Self {
        self.seed.classes.push(class.into());
        self
    }

    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.seed.css_id = Some(id.into());
        self
    }

    pub fn with_password(mut self, password: bool) -> Self {
        self.password = password;
        self
    }

    pub fn with_restrict(mut self, pattern: &str) -> Self {
        self.restrict = Regex::new(pattern).ok();
        self
    }

    pub fn with_max_length(mut self, max_length: usize) -> Self {
        self.max_length = Some(max_length);
        self
    }

    /// Attach a [`Suggester`] that provides auto-completion ghost text.
    pub fn with_suggester(mut self, suggester: impl Suggester + 'static) -> Self {
        self.suggester = Some(Box::new(suggester));
        self
    }

    pub fn set_class(&mut self, class: &str, enabled: bool) {
        if enabled {
            if !self.seed.classes.iter().any(|c| c == class) {
                self.seed.classes.push(class.to_string());
            }
        } else {
            self.seed.classes.retain(|c| c != class);
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn validation_result(&self) -> &ValidationResult {
        &self.validation_result
    }

    pub fn set_text(&mut self, value: impl Into<String>) {
        self.text = value.into();
        if self.cursor > self.text.len() {
            self.cursor = self.text.len();
        }
        self.cursor = clamp_grapheme_boundary(&self.text, self.cursor);
        self.selection = Selection::cursor(self.cursor);
        self.suggestion.clear();
        self.revalidate();
    }

    /// Clear the input text.
    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
        self.selection = Selection::cursor(0);
        self.suggestion.clear();
        self.revalidate();
    }

    /// Insert text at the current cursor position.
    pub fn insert_text_at_cursor(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.delete_selection_if_any();
        let mut filtered = String::new();
        for ch in text.chars() {
            if !self.is_allowed_char(ch) {
                continue;
            }
            if self
                .max_length
                .is_some_and(|max| self.text.len() + filtered.len() + ch.len_utf8() > max)
            {
                break;
            }
            if let Some(ref re) = self.restrict {
                let mut candidate = self.text.clone();
                candidate.insert_str(self.cursor + filtered.len(), &filtered);
                candidate.insert(self.cursor + filtered.len(), ch);
                if !re.is_match(&candidate) {
                    continue;
                }
            }
            filtered.push(ch);
        }
        if filtered.is_empty() {
            return;
        }
        self.text.insert_str(self.cursor, &filtered);
        self.cursor += filtered.len();
        self.cursor = clamp_grapheme_boundary(&self.text, self.cursor);
        self.selection = Selection::cursor(self.cursor);
        self.revalidate();
        self.update_suggestion();
    }

    /// Delete the selection if any, otherwise delete the character after the cursor.
    pub fn delete(&mut self) {
        if self.delete_selection_if_any() {
            self.revalidate();
            self.update_suggestion();
            return;
        }
        if self.cursor < self.text.len() {
            let end = next_grapheme_boundary(&self.text, self.cursor);
            self.text.drain(self.cursor..end);
            self.selection = Selection::cursor(self.cursor);
            self.revalidate();
            self.update_suggestion();
        }
    }

    /// Replace the current selection with the given text. If nothing is selected, inserts at cursor.
    pub fn replace(&mut self, text: &str) {
        self.delete_selection_if_any();
        self.insert_text_at_cursor(text);
    }

    /// Select all text.
    pub fn select_all(&mut self) {
        if self.text.is_empty() {
            return;
        }
        self.selection = Selection {
            start: 0,
            end: self.text.len(),
        };
        self.cursor = self.text.len();
    }

    /// Return the currently selected text, or None if no selection.
    pub fn selected_text(&self) -> Option<String> {
        if self.selection.start == self.selection.end {
            return None;
        }
        let (start, end) = if self.selection.start <= self.selection.end {
            (self.selection.start, self.selection.end)
        } else {
            (self.selection.end, self.selection.start)
        };
        Some(self.text[start..end].to_string())
    }

    fn cursor_from_x(&self, x: u16) -> usize {
        // Mouse coordinates are content-local, so `x` maps to the rendered value text
        // (without borders / line-pad).
        byte_index_from_cell_x(&self.text, x as usize)
    }

    fn is_allowed_char(&self, ch: char) -> bool {
        match self.input_type {
            InputType::Text => true,
            InputType::Integer => {
                if ch.is_ascii_digit() {
                    return true;
                }
                if (ch == '-' || ch == '+') && self.cursor == 0 {
                    return !self.text.starts_with(['-', '+']);
                }
                false
            }
            InputType::Number => {
                if ch.is_ascii_digit() {
                    return true;
                }
                if (ch == '-' || ch == '+') && self.cursor == 0 {
                    return !self.text.starts_with(['-', '+']);
                }
                if ch == '.' {
                    return !self.text.contains('.');
                }
                if ch == 'e' || ch == 'E' {
                    return !(self.text.contains('e') || self.text.contains('E'));
                }
                false
            }
        }
    }

    /// Check if the proposed new value passes restrict and max_length checks.
    fn is_value_allowed(&self, value: &str) -> bool {
        if self.max_length.is_some_and(|max| value.len() > max) {
            return false;
        }
        if self.restrict.as_ref().is_some_and(|re| !re.is_match(value)) {
            return false;
        }
        true
    }

    fn post_changed(&mut self, ctx: &mut EventCtx) {
        ctx.post_message(InputChanged {
            value: self.text.clone(),
            validation: self.validation_result.clone(),
        });
    }

    fn delete_selection_if_any(&mut self) -> bool {
        if self.selection.start == self.selection.end {
            return false;
        }
        let (start, end) = if self.selection.start <= self.selection.end {
            (self.selection.start, self.selection.end)
        } else {
            (self.selection.end, self.selection.start)
        };
        self.text.drain(start..end);
        self.cursor = start;
        self.selection = Selection::cursor(start);
        true
    }

    fn insert_text_from_clipboard(&mut self, text: &str) -> bool {
        let Some(text) = first_clipboard_line(text) else {
            return false;
        };
        let mut inserted = String::new();
        for ch in text.chars() {
            if self.is_allowed_char(ch) {
                inserted.push(ch);
            }
        }
        if inserted.is_empty() {
            return false;
        }
        self.delete_selection_if_any();
        // Apply max_length: truncate the insert to fit
        if let Some(max) = self.max_length {
            let remaining = max.saturating_sub(self.text.len());
            if remaining == 0 {
                return false;
            }
            // Truncate to remaining chars
            let truncated: String = inserted.chars().take(remaining).collect();
            inserted = truncated;
        }
        // Build proposed value and check restrict
        let mut proposed = self.text.clone();
        proposed.insert_str(self.cursor, &inserted);
        if self
            .restrict
            .as_ref()
            .is_some_and(|re| !re.is_match(&proposed))
        {
            return false;
        }
        self.text.insert_str(self.cursor, &inserted);
        self.cursor += inserted.len();
        self.cursor = clamp_grapheme_boundary(&self.text, self.cursor);
        self.selection = Selection::cursor(self.cursor);
        true
    }

    fn move_cursor_to(&mut self, next: usize, select: bool) -> bool {
        let next = clamp_grapheme_boundary(&self.text, next);
        if select {
            if self.selection.start == self.selection.end {
                self.selection.start = self.cursor;
            }
            if next == self.cursor {
                return false;
            }
            self.cursor = next;
            self.selection.end = next;
            return true;
        }
        if next == self.cursor && self.selection.start == self.selection.end {
            return false;
        }
        self.cursor = next;
        self.selection = Selection::cursor(next);
        true
    }

    /// Whether the cursor is at the end of the text.
    fn cursor_at_end(&self) -> bool {
        self.cursor >= self.text.len()
    }

    /// Query the attached suggester and update `self.suggestion`.
    fn update_suggestion(&mut self) {
        self.suggestion.clear();
        if let Some(ref suggester) = self.suggester {
            if !self.text.is_empty() {
                if let Some(s) = suggester.suggest(&self.text) {
                    // Only accept suggestions that are longer than the current value
                    // (matching Python Textual behaviour).
                    if s.len() > self.text.len() {
                        self.suggestion = s;
                    }
                }
            }
        }
    }

    /// Accept the current suggestion: replace the input value with the
    /// suggestion and move the cursor to the end.  Returns `true` if a
    /// suggestion was accepted.
    fn accept_suggestion(&mut self) -> bool {
        if self.suggestion.is_empty() || !self.cursor_at_end() {
            return false;
        }
        // Validate the suggestion against restrict/max_length before accepting.
        if !self.is_value_allowed(&self.suggestion) {
            self.suggestion.clear();
            return false;
        }
        self.text = std::mem::take(&mut self.suggestion);
        self.cursor = self.text.len();
        self.selection = Selection::cursor(self.cursor);
        self.revalidate();
        true
    }

    fn revalidate(&mut self) {
        if self.validators.is_empty() {
            self.validation_result = ValidationResult::success();
            self.set_class("-valid", false);
            self.set_class("-invalid", false);
            return;
        }

        let mut failures: Vec<String> = Vec::new();
        for validator in &self.validators {
            let result = validator.validate(&self.text);
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

        if self.text.trim().is_empty() {
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

    // ── Reactive getters ─────────────────────────────────────────────────

    /// Reactive getter for the input value (Python-aligned name for `text`).
    pub fn value(&self) -> &str {
        &self.text
    }

    /// Reactive getter for the placeholder text.
    pub fn placeholder(&self) -> Option<&str> {
        self.placeholder.as_deref()
    }

    // ── Reactive setters ─────────────────────────────────────────────────

    /// Reactive setter for the input value. Records the change in the
    /// provided [`ReactiveCtx`] if the value actually changed.
    /// The watcher handles cursor clamping, revalidation, and suggestion clearing.
    pub fn set_value(&mut self, value: String, ctx: &mut ReactiveCtx) {
        if self.text != value {
            let old = self.text.clone();
            self.text = value;
            let new = self.text.clone();
            ctx.record_change(
                "value",
                ReactiveFlags::reactive(),
                Box::new(old),
                Box::new(new),
            );
        }
    }

    /// Reactive setter for the placeholder. Records the change in the
    /// provided [`ReactiveCtx`] if the value actually changed.
    pub fn set_placeholder(&mut self, value: Option<String>, ctx: &mut ReactiveCtx) {
        if self.placeholder != value {
            let old = self.placeholder.clone();
            self.placeholder = value;
            let new = self.placeholder.clone();
            ctx.record_change(
                "placeholder",
                ReactiveFlags::reactive(),
                Box::new(old),
                Box::new(new),
            );
        }
    }

    // ── Watchers ─────────────────────────────────────────────────────────

    fn watch_value(&mut self, _old: &String, _new: &String, _ctx: &mut ReactiveCtx) {
        if self.cursor > self.text.len() {
            self.cursor = self.text.len();
        }
        self.cursor = clamp_grapheme_boundary(&self.text, self.cursor);
        self.selection = Selection::cursor(self.cursor);
        self.suggestion.clear();
        self.revalidate();
    }
}

impl ReactiveWidget for Input {
    fn reactive_dispatch(&mut self, changes: &[ReactiveChange], ctx: &mut ReactiveCtx) {
        for change in changes {
            if change.field_name == "value" {
                if let (Some(old), Some(new)) = (
                    change.old_value.downcast_ref::<String>(),
                    change.new_value.downcast_ref::<String>(),
                ) {
                    self.watch_value(old, new, ctx);
                }
            }
        }
    }
}

impl Widget for Input {
    fn style_type(&self) -> &'static str {
        self.style_type_name
    }

    fn style_type_aliases(&self) -> &[&'static str] {
        self.style_type_aliases.as_slice()
    }

    fn focusable(&self) -> bool {
        true
    }

    fn on_node_state_changed(&mut self, old: NodeState, new: NodeState) {
        if old.focused != new.focused {
            let was_focused = old.focused;
            self.chrome.set_focus(new.focused);
            if was_focused && !new.focused {
                self.pending_blur = true;
            }
            // Clear suggestion on focus change (matches Python Textual).
            self.suggestion.clear();
        }
    }

    fn is_active(&self) -> bool {
        self.chrome.is_active()
    }

    fn style_classes(&self) -> &[String] {
        &self.seed.classes
    }

    fn on_mouse_move(&mut self, x: u16, _y: u16) -> bool {
        if !self.chrome.is_mouse_down() {
            return false;
        }
        // Groundwork for selection: update selection end (and cursor) while dragging.
        let next = self.cursor_from_x(x);
        if next == self.selection.end && next == self.cursor {
            return false;
        }
        self.selection.end = next;
        self.cursor = next;
        true
    }

    fn action_namespace(&self) -> &str {
        "input"
    }

    fn bindings(&self) -> Vec<BindingDecl> {
        vec![BindingDecl::new("enter", "submit", "Submit")]
    }

    fn execute_action(&mut self, action: &ParsedAction, ctx: &mut EventCtx) -> bool {
        match action.name.as_str() {
            "submit" => {
                ctx.post_message(InputSubmitted {
                    value: self.text.clone(),
                });
                ctx.set_handled();
                true
            }
            _ => false,
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        match event {
            Event::AppFocus(active) => {
                self.chrome.handle_app_focus(*active);
                ctx.request_repaint();
            }
            Event::MouseDown(mouse) if mouse.target == self.node_id() => {
                if self.text.is_empty() {
                    self.cursor = 0;
                } else {
                    self.cursor = self.cursor_from_x(mouse.x);
                }
                self.selection = Selection::cursor(self.cursor);
                self.chrome.set_mouse_down(true);
                self.chrome.reset_blink();
                ctx.request_repaint();
                ctx.set_handled();
            }
            Event::MouseUp(_)
                if self.chrome.is_mouse_down() => {
                    self.chrome.set_mouse_down(false);
                    ctx.request_repaint();
                }
            Event::Tick(tick) => {
                let _ = tick;
                if self.pending_blur {
                    self.pending_blur = false;
                    ctx.post_message(InputBlurred {
                        value: self.text.clone(),
                    });
                }
                if self.chrome.handle_tick(Instant::now()) {
                    ctx.request_repaint();
                }
            }
            Event::Key(key) if self.node_state().focused => {
                // Tab accepts the current suggestion (intercept before edit_command_from_key,
                // which returns None for Tab).  When no suggestion is active, Tab falls
                // through (not handled) so the runtime can use it for focus navigation.
                if key.code == KeyCode::Tab
                    && key.modifiers == KeyModifiers::NONE
                    && !self.suggestion.is_empty()
                    && self.cursor_at_end()
                {
                    if self.accept_suggestion() {
                        self.post_changed(ctx);
                        self.chrome.reset_blink();
                        ctx.request_repaint();
                        ctx.set_handled();
                    }
                    return;
                }

                let Some(cmd) = edit_command_from_key(key, false) else {
                    return;
                };
                let mut changed = false;
                let mut value_changed = false;
                match cmd {
                    EditCommand::InsertChar(ch) => {
                        if self.is_allowed_char(ch) {
                            // Build the proposed new value
                            let mut proposed = self.text.clone();
                            let mut pos = self.cursor;
                            if self.selection.start != self.selection.end {
                                let (s, e) = if self.selection.start <= self.selection.end {
                                    (self.selection.start, self.selection.end)
                                } else {
                                    (self.selection.end, self.selection.start)
                                };
                                proposed.drain(s..e);
                                pos = s;
                            }
                            proposed.insert(pos, ch);
                            if self.is_value_allowed(&proposed) {
                                self.delete_selection_if_any();
                                self.text.insert(self.cursor, ch);
                                self.cursor += ch.len_utf8();
                                self.cursor = clamp_grapheme_boundary(&self.text, self.cursor);
                                self.selection = Selection::cursor(self.cursor);
                                changed = true;
                                value_changed = true;
                            }
                        }
                    }
                    EditCommand::Submit => {
                        ctx.post_message(InputSubmitted {
                            value: self.text.clone(),
                        });
                    }
                    EditCommand::Copy => {
                        if let Some(text) = self.selected_text() {
                            ctx.post_message(TextEditClipboardCopyRequested { text, cut: false });
                        }
                    }
                    EditCommand::Cut => {
                        if let Some(text) = self.selected_text() {
                            ctx.post_message(TextEditClipboardCopyRequested { text, cut: true });
                            if self.delete_selection_if_any() {
                                changed = true;
                                value_changed = true;
                            }
                        }
                    }
                    EditCommand::Paste => {
                        ctx.post_message(TextEditClipboardPasteRequested {
                            target: self.node_id(),
                        });
                    }
                    EditCommand::Backspace { unit } => {
                        if self.delete_selection_if_any() {
                            changed = true;
                            value_changed = true;
                        } else if self.cursor > 0 {
                            let start = match unit {
                                MoveUnit::Grapheme => {
                                    prev_grapheme_boundary(&self.text, self.cursor)
                                }
                                MoveUnit::Word => prev_word_boundary(&self.text, self.cursor),
                            };
                            self.text.drain(start..self.cursor);
                            self.cursor = start;
                            self.selection = Selection::cursor(self.cursor);
                            changed = true;
                            value_changed = true;
                        }
                    }
                    EditCommand::Delete { unit } => {
                        if self.delete_selection_if_any() {
                            changed = true;
                            value_changed = true;
                        } else if self.cursor < self.text.len() {
                            let end = match unit {
                                MoveUnit::Grapheme => {
                                    next_grapheme_boundary(&self.text, self.cursor)
                                }
                                MoveUnit::Word => next_word_boundary(&self.text, self.cursor),
                            };
                            self.text.drain(self.cursor..end);
                            self.selection = Selection::cursor(self.cursor);
                            changed = true;
                            value_changed = true;
                        }
                    }
                    EditCommand::DeleteToStart => {
                        if self.delete_selection_if_any() {
                            changed = true;
                            value_changed = true;
                        } else if self.cursor > 0 {
                            self.text.drain(0..self.cursor);
                            self.cursor = 0;
                            self.selection = Selection::cursor(0);
                            changed = true;
                            value_changed = true;
                        }
                    }
                    EditCommand::MoveLeft { select, unit } => {
                        let next = if self.selection.start != self.selection.end && !select {
                            self.selection.start.min(self.selection.end)
                        } else {
                            match unit {
                                MoveUnit::Grapheme => {
                                    prev_grapheme_boundary(&self.text, self.cursor)
                                }
                                MoveUnit::Word => prev_word_boundary(&self.text, self.cursor),
                            }
                        };
                        changed = self.move_cursor_to(next, select);
                    }
                    EditCommand::MoveRight { select, unit } => {
                        // Accept suggestion on Right at end of text (Python Textual parity).
                        if !select
                            && self.cursor_at_end()
                            && self.selection.start == self.selection.end
                            && !self.suggestion.is_empty()
                        {
                            if self.accept_suggestion() {
                                changed = true;
                                value_changed = true;
                            }
                        } else {
                            let next = if self.selection.start != self.selection.end && !select {
                                self.selection.start.max(self.selection.end)
                            } else {
                                match unit {
                                    MoveUnit::Grapheme => {
                                        next_grapheme_boundary(&self.text, self.cursor)
                                    }
                                    MoveUnit::Word => next_word_boundary(&self.text, self.cursor),
                                }
                            };
                            changed = self.move_cursor_to(next, select);
                        }
                    }
                    EditCommand::MoveHome { select } => {
                        changed = self.move_cursor_to(0, select);
                    }
                    EditCommand::MoveEnd { select } => {
                        changed = self.move_cursor_to(self.text.len(), select);
                    }
                    EditCommand::DeleteToEnd => {
                        if self.delete_selection_if_any() {
                            changed = true;
                            value_changed = true;
                        } else if self.cursor < self.text.len() {
                            self.text.truncate(self.cursor);
                            self.selection = Selection::cursor(self.cursor);
                            changed = true;
                            value_changed = true;
                        }
                    }
                    EditCommand::SelectAll => {
                        if !self.text.is_empty() {
                            self.selection = Selection {
                                start: 0,
                                end: self.text.len(),
                            };
                            self.cursor = self.text.len();
                            changed = true;
                        }
                    }
                    EditCommand::InsertNewline
                    | EditCommand::MoveUp { .. }
                    | EditCommand::MoveDown { .. }
                    | EditCommand::DeleteLine
                    | EditCommand::SelectLine
                    | EditCommand::Undo
                    | EditCommand::Redo => {}
                }

                if value_changed {
                    self.revalidate();
                    self.update_suggestion();
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
        if let Some(m) = message.downcast_ref::<TextEditClipboardPaste>() {
            if m.target != self.node_id() {
                return;
            }
            if self.insert_text_from_clipboard(&m.text) {
                self.revalidate();
                self.update_suggestion();
                self.post_changed(ctx);
                self.chrome.reset_blink();
                ctx.request_repaint();
                ctx.set_handled();
            }
        }
    }

    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let mut out = Segments::new();

        // Compute the Input's painted surface so component colours (which may include alpha or an
        // `auto <pct>%` contrast token) composite over the correct background. During a live render
        // the composited background reflects the widget's own state-aware surface — including the
        // `:focus` `background-tint: $foreground 5%` — so a focused Input's placeholder contrasts
        // against `#272727`, not the untinted `$surface`. This mirrors Python `dom.py`:
        // `background += styles.background.tint(styles.background_tint)`.
        let fallback_bg = parse_color_like("$background").unwrap_or(Color::rgb(0, 0, 0));
        let base_bg = crate::css::current_composited_background().unwrap_or_else(|| {
            // Off-tree callers (unit tests without a live style stack): resolve statelessly and
            // apply any `background-tint` by hand.
            let base_meta = crate::css::selector_meta_generic(self);
            let base_style = crate::css::resolve_style(self, &base_meta);
            let mut bg = match base_style.bg {
                Some(bg) if bg.a <= 0.0 => fallback_bg,
                Some(bg) => bg,
                None => fallback_bg,
            };
            if let Some(tint) = base_style.background_tint {
                bg = crate::renderables::Tint::<()>::blend_color_with_percent(
                    bg, tint.color, tint.percent,
                );
            }
            bg
        });
        // The widget's OWN (untinted) background. The base `Input { background: $surface }` rule
        // also matches the component selector meta (which is typed `Input`), so component styles
        // that don't set their own background inherit `$surface`. That leaked background must NOT
        // be treated as a genuine component surface: the placeholder/suggestion text is painted on
        // the widget's real (tinted) surface, so we keep `base_bg` for compositing in that case.
        let widget_own_bg = crate::css::current_self_style().and_then(|s| s.bg);

        let resolve_component_rich = |class: &str| -> rich_rs::Style {
            let style = crate::css::resolve_component_style(self, &[class]);
            let mut rich = style
                .to_rich_without_colors()
                .unwrap_or_default();
            let mut under_bg = base_bg;

            if let Some(bg) = style.bg {
                if bg.a <= 0.0 {
                    return rich;
                }
                // Only a component background that DIFFERS from the widget's own surface is a real
                // override (e.g. `input--cursor`/`input--selection`); a background equal to the
                // widget surface is the leaked base rule — leave the segment transparent so the
                // compositor paints (and tints) it against the real surface.
                if Some(bg) != widget_own_bg {
                    let flat = bg.flatten_over(under_bg);
                    under_bg = flat;
                    rich = rich.with_bgcolor(flat.to_simple_opaque());
                }
            }
            if let Some(fg) = style.fg {
                let flat = fg.flatten_over(under_bg);
                rich = rich.with_color(flat.to_simple_opaque());
            } else if let Some(auto) = style.fg_auto {
                // `auto <pct>%` foreground (e.g. `$text-disabled` = `auto 38%` used by
                // `input--placeholder`/`input--suggestion`). Resolve the contrast colour of the
                // under-background and composite it at the fractional alpha, matching Python's
                // `background.get_contrast_text(alpha)` / the segment compositor's fg_auto path.
                let contrast = crate::style::contrast_text(under_bg);
                let flat = contrast.blend_over_float(under_bg, auto.alpha());
                rich = rich.with_color(flat.to_simple_opaque());
            }
            rich
        };

        let cursor_style = resolve_component_rich("input--cursor");
        let selection_style = resolve_component_rich("input--selection");
        let placeholder_style = resolve_component_rich("input--placeholder");
        let suggestion_style = resolve_component_rich("input--suggestion");

        if self.text.is_empty() {
            let placeholder = self.placeholder.clone().unwrap_or_default();
            let line = rich_rs::set_cell_size(&placeholder, width);
            if self.node_state().focused && self.chrome.cursor_visible() {
                // Match Python Textual: when empty and focused, render a cursor in the first cell
                // (even over placeholder text).
                let mut chars = line.chars();
                let first = chars.next().unwrap_or(' ');
                let rest: String = chars.collect();
                out.push(rich_rs::Segment::styled(first.to_string(), cursor_style));
                if !rest.is_empty() {
                    out.push(rich_rs::Segment::styled(rest, placeholder_style));
                }
            } else {
                out.push(rich_rs::Segment::styled(line, placeholder_style));
            }
            return out;
        }

        let (sel_start, sel_end) =
            if self.node_state().focused && self.selection.start != self.selection.end {
                (
                    self.selection.start.min(self.text.len()),
                    self.selection.end.min(self.text.len()),
                )
            } else {
                (
                    self.cursor.min(self.text.len()),
                    self.cursor.min(self.text.len()),
                )
            };
        let (sel_lo, sel_hi) = if sel_start <= sel_end {
            (sel_start, sel_end)
        } else {
            (sel_end, sel_start)
        };

        let mut cells_used: usize = 0;
        let mut pending_style: Option<rich_rs::Style> = None;
        let mut pending_text = String::new();

        let flush = |out: &mut Segments,
                     pending_style: &mut Option<rich_rs::Style>,
                     pending_text: &mut String| {
            if pending_text.is_empty() {
                return;
            }
            let style = pending_style.take().unwrap_or_default();
            out.push(rich_rs::Segment::styled(
                std::mem::take(pending_text),
                style,
            ));
        };

        // Iterate over original text for byte indices (cursor/selection use these),
        // but display bullet character in password mode.
        let bullet = "\u{2022}";
        for (byte_idx, grapheme) in self.text.grapheme_indices(true) {
            let display = if self.password { bullet } else { grapheme };
            let w = grapheme_cell_width(display);
            if cells_used.saturating_add(w) > width {
                break;
            }

            let is_cursor = self.node_state().focused
                && self.chrome.cursor_visible()
                && byte_idx == self.cursor;
            let in_sel = byte_idx >= sel_lo && byte_idx < sel_hi;
            let style = if is_cursor {
                Some(cursor_style)
            } else if in_sel {
                Some(selection_style)
            } else {
                None
            };

            let style_changed = match (&pending_style, &style) {
                (None, None) => false,
                (Some(a), Some(b)) => a != b,
                _ => true,
            };
            if style_changed {
                flush(&mut out, &mut pending_style, &mut pending_text);
                pending_style = style;
            }
            pending_text.push_str(display);
            cells_used = cells_used.saturating_add(w);
        }

        flush(&mut out, &mut pending_style, &mut pending_text);

        // Show suggestion ghost text (the suffix beyond what the user typed).
        let show_suggestion = self.node_state().focused
            && !self.suggestion.is_empty()
            && self.suggestion.len() > self.text.len()
            && self.suggestion.is_char_boundary(self.text.len());
        if show_suggestion {
            let ghost = &self.suggestion[self.text.len()..];
            // When cursor is at end, the first ghost character gets cursor style
            // (Python Textual renders the cursor over the first ghost char).
            if self.cursor == self.text.len() && self.chrome.cursor_visible() {
                let mut ghost_graphemes = ghost.grapheme_indices(true);
                if let Some((_idx, first_g)) = ghost_graphemes.next() {
                    let first_w = grapheme_cell_width(first_g);
                    if cells_used.saturating_add(first_w) <= width {
                        out.push(rich_rs::Segment::styled(first_g.to_string(), cursor_style));
                        cells_used = cells_used.saturating_add(first_w);
                    }
                }
                // Remaining ghost text in suggestion style
                let rest_start = ghost
                    .grapheme_indices(true)
                    .nth(1)
                    .map(|(i, _)| i)
                    .unwrap_or(ghost.len());
                let rest = &ghost[rest_start..];
                if !rest.is_empty() && cells_used < width {
                    let mut ghost_text = String::new();
                    for grapheme in rest.graphemes(true) {
                        let w = grapheme_cell_width(grapheme);
                        if cells_used.saturating_add(w) > width {
                            break;
                        }
                        ghost_text.push_str(grapheme);
                        cells_used = cells_used.saturating_add(w);
                    }
                    if !ghost_text.is_empty() {
                        out.push(rich_rs::Segment::styled(ghost_text, suggestion_style));
                    }
                }
            } else {
                // Cursor not at end — just show ghost text after typed text
                let mut ghost_text = String::new();
                for grapheme in ghost.graphemes(true) {
                    let w = grapheme_cell_width(grapheme);
                    if cells_used.saturating_add(w) > width {
                        break;
                    }
                    ghost_text.push_str(grapheme);
                    cells_used = cells_used.saturating_add(w);
                }
                if !ghost_text.is_empty() {
                    out.push(rich_rs::Segment::styled(ghost_text, suggestion_style));
                }
            }
        } else if self.node_state().focused
            && self.chrome.cursor_visible()
            && self.cursor == self.text.len()
            && cells_used < width
        {
            // No suggestion — render trailing cursor space (original behaviour).
            out.push(rich_rs::Segment::styled(" ".to_string(), cursor_style));
            cells_used += 1;
        }

        if cells_used < width {
            out.push(rich_rs::Segment::new(" ".repeat(width - cells_used)));
        }

        out
    }

    fn layout_height(&self) -> Option<usize> {
        let meta = crate::css::selector_meta_generic(self);
        let base_style = crate::css::resolve_style(self, &meta);
        let default_height = 1 + super::helpers::border_vertical_padding(&base_style);
        Some(default_height)
    }

    fn set_inline_style(&mut self, style: crate::style::Style) {
        self.seed.styles.style = style;
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }

    fn get_selection(&self) -> Option<String> {
        self.selected_text()
    }
}

impl Renderable for Input {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::MouseDownEvent;
    use crate::keys::KeyEventData;
    use crate::node_id::NodeId;
    use crate::runtime::dispatch_ctx::set_dispatch_recipient;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn make_node_id() -> NodeId {
        use slotmap::SlotMap;
        let mut sm: SlotMap<NodeId, ()> = SlotMap::new();
        sm.insert(())
    }

    fn focused_state() -> NodeState {
        NodeState {
            focused: true,
            ..Default::default()
        }
    }

    fn render_plain(input: &Input, width: usize) -> String {
        let console = Console::new();
        let options = ConsoleOptions {
            size: (width, 1),
            max_width: width,
            ..Default::default()
        };
        Widget::render(input, &console, &options)
            .iter()
            .map(|s| s.text.as_ref())
            .collect::<String>()
    }

    #[test]
    fn with_value_renders_value_not_placeholder() {
        let _guard = crate::css::set_style_context(crate::css::default_widget_stylesheet());
        let input = Input::new()
            .with_value("0")
            .with_placeholder("Enter red 0-255");
        assert_eq!(input.text(), "0");
        let rendered = render_plain(&input, 20);
        assert!(
            rendered.trim_end().ends_with('0') || rendered.contains('0'),
            "a non-empty initial value must render as text, got {rendered:?}"
        );
        assert!(
            !rendered.contains("Enter red"),
            "placeholder must not render when an initial value is set, got {rendered:?}"
        );
    }

    #[test]
    fn empty_input_renders_placeholder() {
        let _guard = crate::css::set_style_context(crate::css::default_widget_stylesheet());
        let input = Input::new().with_placeholder("Enter red 0-255");
        let rendered = render_plain(&input, 20);
        assert!(
            rendered.contains("Enter red"),
            "an empty input must render its placeholder, got {rendered:?}"
        );
    }

    #[test]
    fn placeholder_foreground_resolves_text_disabled_auto() {
        // Regression: `input--placeholder` resolves `color: $text-disabled` as an
        // `auto 38%` (fg_auto) contrast colour, not a concrete `fg`. `resolve_component_rich`
        // must composite the auto colour over the widget surface instead of leaving the
        // placeholder foreground unset (which then inherited the opaque `$foreground`).
        let _guard = crate::css::set_style_context(crate::css::default_widget_stylesheet());
        let input = Input::new().with_placeholder("Enter a color");
        let console = Console::new();
        let options = ConsoleOptions {
            size: (20, 1),
            max_width: 20,
            ..Default::default()
        };
        let segs = Widget::render(&input, &console, &options);
        // The first non-space glyph segment carries the placeholder colour.
        let placeholder_fg = segs
            .iter()
            .find(|s| s.text.trim() != "")
            .and_then(|s| s.style.as_ref())
            .and_then(|st| st.color.clone());
        let fg = placeholder_fg.expect("placeholder segment must carry an explicit foreground");
        // $text-disabled over $surface (#1e1e1e) ≈ #747474 (blurred, untinted); it must NOT be
        // the opaque foreground #e0e0e0.
        let rgb = format!("{fg:?}");
        assert!(
            !rgb.contains("224"),
            "placeholder fg must be the dimmed $text-disabled, not opaque $foreground (#e0e0e0); got {rgb}"
        );
    }

    #[test]
    fn mouse_click_positions_cursor_in_text() {
        let mut input = Input::new();
        input.set_text("hello");
        let id = make_node_id();
        let _guard = set_dispatch_recipient(id, NodeState::default());
        let mut ctx = EventCtx::default();

        input.on_event(
            &Event::MouseDown(MouseDownEvent {
                target: id,
                screen_x: 0,
                screen_y: 0,
                x: 0,
                y: 0,
            }),
            &mut ctx,
        );
        assert!(ctx.handled());
        assert_eq!(input.cursor, 0);

        let mut ctx = EventCtx::default();
        input.on_event(
            &Event::MouseDown(MouseDownEvent {
                target: id,
                screen_x: 0,
                screen_y: 0,
                x: 5,
                y: 0,
            }),
            &mut ctx,
        );
        assert_eq!(input.cursor, input.text.len());
    }

    #[test]
    fn mouse_drag_updates_selection_end() {
        let mut input = Input::new();
        input.set_text("hello");
        let id = make_node_id();
        let _guard = set_dispatch_recipient(id, NodeState::default());
        let mut ctx = EventCtx::default();
        input.on_event(
            &Event::MouseDown(MouseDownEvent {
                target: id,
                screen_x: 0,
                screen_y: 0,
                x: 1,
                y: 0,
            }),
            &mut ctx,
        );
        assert!(input.is_active());
        let changed = input.on_mouse_move(4, 0);
        assert!(changed);
        let (a, b) = input.selection.normalized();
        assert_eq!((a, b), (1, 4));
    }

    #[test]
    fn typing_emits_input_changed_message() {
        let mut input = Input::new();
        let _guard = set_dispatch_recipient(make_node_id(), focused_state());
        let mut ctx = EventCtx::default();
        input.on_event(
            &Event::Key(KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Char('a'),
                KeyModifiers::NONE,
            ))),
            &mut ctx,
        );
        let messages = ctx.take_messages();
        assert_eq!(messages.len(), 1);
        assert!(
            messages[0]
                .downcast_ref::<InputChanged>()
                .is_some_and(|m| m.value == "a")
        );
    }

    #[test]
    fn enter_emits_input_submitted_message() {
        let mut input = Input::new();
        let _guard = set_dispatch_recipient(make_node_id(), focused_state());
        input.set_text("done");
        let mut ctx = EventCtx::default();
        input.on_event(
            &Event::Key(KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Enter,
                KeyModifiers::NONE,
            ))),
            &mut ctx,
        );
        let messages = ctx.take_messages();
        assert_eq!(messages.len(), 1);
        assert!(
            messages[0]
                .downcast_ref::<InputSubmitted>()
                .is_some_and(|m| m.value == "done")
        );
    }

    #[test]
    fn left_and_backspace_respect_grapheme_clusters() {
        let mut input = Input::new();
        let _guard = set_dispatch_recipient(make_node_id(), focused_state());
        input.set_text("a\u{0301}👩‍🚀z");
        input.cursor = input.text.len();
        input.selection = Selection::cursor(input.cursor);

        let mut ctx = EventCtx::default();
        input.on_event(
            &Event::Key(KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Left,
                KeyModifiers::NONE,
            ))),
            &mut ctx,
        );
        let cursor_after_left = input.cursor;
        assert_eq!(&input.text[cursor_after_left..], "z");

        let mut ctx = EventCtx::default();
        input.on_event(
            &Event::Key(KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Backspace,
                KeyModifiers::NONE,
            ))),
            &mut ctx,
        );
        assert_eq!(input.text, "a\u{0301}z");
    }

    #[test]
    fn shift_navigation_expands_selection_and_backspace_deletes_it() {
        let mut input = Input::new();
        let _guard = set_dispatch_recipient(make_node_id(), focused_state());
        input.set_text("hello world");
        input.cursor = 5;
        input.selection = Selection::cursor(5);

        let mut ctx = EventCtx::default();
        input.on_event(
            &Event::Key(KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Right,
                KeyModifiers::SHIFT,
            ))),
            &mut ctx,
        );
        assert_eq!(input.selection.normalized(), (5, 6));

        let mut ctx = EventCtx::default();
        input.on_event(
            &Event::Key(KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Backspace,
                KeyModifiers::NONE,
            ))),
            &mut ctx,
        );
        assert_eq!(input.text, "helloworld");
        assert_eq!(input.cursor, 5);
    }

    #[test]
    fn ctrl_backspace_deletes_previous_word() {
        let mut input = Input::new();
        let _guard = set_dispatch_recipient(make_node_id(), focused_state());
        input.set_text("alpha beta");
        input.cursor = input.text.len();
        input.selection = Selection::cursor(input.cursor);

        let mut ctx = EventCtx::default();
        input.on_event(
            &Event::Key(KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Backspace,
                KeyModifiers::CONTROL,
            ))),
            &mut ctx,
        );

        assert_eq!(input.text, "alpha ");
    }

    #[test]
    fn copy_and_cut_emit_clipboard_messages() {
        let mut input = Input::new();
        let _guard = set_dispatch_recipient(make_node_id(), focused_state());
        input.set_text("hello world");
        input.cursor = 5;
        input.selection = Selection { start: 0, end: 5 };

        let mut ctx = EventCtx::default();
        input.on_event(
            &Event::Key(KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Char('c'),
                KeyModifiers::CONTROL,
            ))),
            &mut ctx,
        );
        let copy_messages = ctx.take_messages();
        assert!(
            copy_messages
                .first()
                .and_then(|m| m.downcast_ref::<TextEditClipboardCopyRequested>())
                .is_some_and(|m| m.text == "hello" && !m.cut)
        );

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
            m.downcast_ref::<TextEditClipboardCopyRequested>()
                .is_some_and(|r| r.text == "hello" && r.cut)
        }));
        assert_eq!(input.text(), " world");
    }

    #[test]
    fn paste_request_and_message_updates_input_value() {
        let mut input = Input::new();
        let id = make_node_id();
        let _guard = set_dispatch_recipient(id, focused_state());
        input.set_text("abc");
        input.cursor = 1;
        input.selection = Selection::cursor(1);

        let mut ctx = EventCtx::default();
        input.on_event(
            &Event::Key(KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Char('v'),
                KeyModifiers::CONTROL,
            ))),
            &mut ctx,
        );
        let messages = ctx.take_messages();
        assert!(
            messages
                .first()
                .and_then(|m| m.downcast_ref::<TextEditClipboardPasteRequested>())
                .is_some_and(|m| m.target == id)
        );

        let mut ctx = EventCtx::default();
        input.on_message(
            &MessageEvent::new(
                NodeId::default(),
                TextEditClipboardPaste {
                    target: id,
                    text: "XYZ".to_string(),
                },
            ),
            &mut ctx,
        );
        assert_eq!(input.text(), "aXYZbc");
        assert!(ctx.handled());
    }

    #[test]
    fn paste_message_uses_first_clipboard_line_only() {
        let mut input = Input::new();
        let id = make_node_id();
        let _guard = set_dispatch_recipient(id, focused_state());
        input.set_text("abc");
        input.cursor = 1;
        input.selection = Selection::cursor(1);

        let mut ctx = EventCtx::default();
        input.on_message(
            &MessageEvent::new(
                NodeId::default(),
                TextEditClipboardPaste {
                    target: id,
                    text: "XYZ\r\n123".to_string(),
                },
            ),
            &mut ctx,
        );
        assert_eq!(input.text(), "aXYZbc");
        assert!(ctx.handled());
    }

    #[test]
    fn bindings_are_declared() {
        let input = Input::new();
        let bindings = input.bindings();
        assert!(!bindings.is_empty());
        assert!(bindings.iter().any(|b| b.action == "submit"));
    }

    #[test]
    fn execute_action_handles_submit() {
        use crate::action::ParsedAction;
        let mut input = Input::new();
        let _guard = set_dispatch_recipient(make_node_id(), focused_state());
        input.set_text("hello");
        let mut ctx = EventCtx::default();
        let action = ParsedAction {
            namespace: None,
            name: "submit".to_string(),
            arguments: vec![],
        };
        assert!(input.execute_action(&action, &mut ctx));
        let messages = ctx.take_messages();
        assert!(messages.iter().any(|m| {
            m.downcast_ref::<InputSubmitted>()
                .is_some_and(|s| s.value == "hello")
        }));
    }

    // -----------------------------------------------------------------------
    // Suggester tests
    // -----------------------------------------------------------------------

    #[test]
    fn suggest_from_list_case_insensitive_prefix() {
        let suggester = SuggestFromList::new(vec!["Portugal", "Poland", "Spain"], false);
        assert_eq!(suggester.suggest("por"), Some("Portugal".to_string()));
        assert_eq!(suggester.suggest("POR"), Some("Portugal".to_string()));
        assert_eq!(suggester.suggest("pol"), Some("Poland".to_string()));
        assert_eq!(suggester.suggest("sp"), Some("Spain".to_string()));
        assert_eq!(suggester.suggest("fr"), None);
        assert_eq!(suggester.suggest(""), None);
    }

    #[test]
    fn suggest_from_list_case_sensitive() {
        let suggester = SuggestFromList::new(vec!["Portugal", "Poland"], true);
        assert_eq!(suggester.suggest("Por"), Some("Portugal".to_string()));
        assert_eq!(suggester.suggest("por"), None); // case-sensitive: no match
    }

    #[test]
    fn suggest_from_list_returns_first_match() {
        let suggester = SuggestFromList::new(vec!["Apple", "Application", "Banana"], false);
        // First match wins (ordered by priority).
        assert_eq!(suggester.suggest("app"), Some("Apple".to_string()));
    }

    #[test]
    fn typing_updates_suggestion() {
        let mut input =
            Input::new().with_suggester(SuggestFromList::new(vec!["Portugal", "Spain"], false));
        let _guard = set_dispatch_recipient(make_node_id(), focused_state());

        // Type 'p' => should suggest "Portugal"
        let mut ctx = EventCtx::default();
        input.on_event(
            &Event::Key(KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Char('p'),
                KeyModifiers::NONE,
            ))),
            &mut ctx,
        );
        assert_eq!(input.text(), "p");
        assert_eq!(input.suggestion, "Portugal");
    }

    #[test]
    fn typing_clears_stale_suggestion() {
        let mut input = Input::new().with_suggester(SuggestFromList::new(vec!["Portugal"], false));
        let _guard = set_dispatch_recipient(make_node_id(), focused_state());

        // Type 'p' => "Portugal"
        let mut ctx = EventCtx::default();
        input.on_event(
            &Event::Key(KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Char('p'),
                KeyModifiers::NONE,
            ))),
            &mut ctx,
        );
        assert_eq!(input.suggestion, "Portugal");

        // Type 'x' => no match, suggestion cleared
        let mut ctx = EventCtx::default();
        input.on_event(
            &Event::Key(KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Char('x'),
                KeyModifiers::NONE,
            ))),
            &mut ctx,
        );
        assert_eq!(input.text(), "px");
        assert!(input.suggestion.is_empty());
    }

    #[test]
    fn tab_accepts_suggestion() {
        let mut input = Input::new().with_suggester(SuggestFromList::new(vec!["Portugal"], false));
        let _guard = set_dispatch_recipient(make_node_id(), focused_state());

        // Type 'p'
        let mut ctx = EventCtx::default();
        input.on_event(
            &Event::Key(KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Char('p'),
                KeyModifiers::NONE,
            ))),
            &mut ctx,
        );
        assert_eq!(input.suggestion, "Portugal");

        // Tab accepts suggestion
        let mut ctx = EventCtx::default();
        input.on_event(
            &Event::Key(KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Tab,
                KeyModifiers::NONE,
            ))),
            &mut ctx,
        );
        assert_eq!(input.text(), "Portugal");
        assert!(input.suggestion.is_empty());
        assert!(ctx.handled());
    }

    #[test]
    fn tab_without_suggestion_is_not_handled() {
        let mut input = Input::new();
        let _guard = set_dispatch_recipient(make_node_id(), focused_state());
        input.set_text("hello");
        input.cursor = input.text.len();
        input.selection = Selection::cursor(input.cursor);

        let mut ctx = EventCtx::default();
        input.on_event(
            &Event::Key(KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Tab,
                KeyModifiers::NONE,
            ))),
            &mut ctx,
        );
        // Tab with no suggestion should not be handled (allows focus navigation).
        assert!(!ctx.handled());
        assert_eq!(input.text(), "hello");
    }

    #[test]
    fn right_arrow_at_end_accepts_suggestion() {
        let mut input = Input::new().with_suggester(SuggestFromList::new(vec!["Spain"], false));
        let _guard = set_dispatch_recipient(make_node_id(), focused_state());

        // Type 's'
        let mut ctx = EventCtx::default();
        input.on_event(
            &Event::Key(KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Char('s'),
                KeyModifiers::NONE,
            ))),
            &mut ctx,
        );
        assert_eq!(input.suggestion, "Spain");
        assert!(input.cursor_at_end());

        // Right arrow at end accepts
        let mut ctx = EventCtx::default();
        input.on_event(
            &Event::Key(KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Right,
                KeyModifiers::NONE,
            ))),
            &mut ctx,
        );
        assert_eq!(input.text(), "Spain");
        assert!(input.suggestion.is_empty());
    }

    #[test]
    fn right_arrow_not_at_end_does_not_accept_suggestion() {
        let mut input = Input::new().with_suggester(SuggestFromList::new(vec!["Portugal"], false));
        let _guard = set_dispatch_recipient(make_node_id(), focused_state());
        input.set_text("po");
        input.cursor = 0; // cursor NOT at end
        input.selection = Selection::cursor(0);
        input.suggestion = "Portugal".to_string();

        let mut ctx = EventCtx::default();
        input.on_event(
            &Event::Key(KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Right,
                KeyModifiers::NONE,
            ))),
            &mut ctx,
        );
        // Should just move cursor, not accept suggestion
        assert_eq!(input.text(), "po");
        assert_eq!(input.cursor, 1);
    }

    #[test]
    fn set_text_clears_suggestion() {
        let mut input = Input::new().with_suggester(SuggestFromList::new(vec!["Portugal"], false));
        input.suggestion = "Portugal".to_string();
        input.set_text("new value");
        assert!(input.suggestion.is_empty());
    }

    #[test]
    fn clear_clears_suggestion() {
        let mut input = Input::new().with_suggester(SuggestFromList::new(vec!["Portugal"], false));
        input.set_text("po");
        input.suggestion = "Portugal".to_string();
        input.clear();
        assert!(input.suggestion.is_empty());
    }

    #[test]
    fn focus_change_clears_suggestion() {
        let mut input = Input::new().with_suggester(SuggestFromList::new(vec!["Portugal"], false));
        input.on_node_state_changed(
            NodeState::default(),
            NodeState {
                focused: true,
                ..Default::default()
            },
        );
        input.suggestion = "Portugal".to_string();
        input.on_node_state_changed(
            NodeState {
                focused: true,
                ..Default::default()
            },
            NodeState::default(),
        );
        assert!(input.suggestion.is_empty());
    }

    #[test]
    fn suggestion_not_shown_when_matches_value_exactly() {
        let suggester = SuggestFromList::new(vec!["abc"], false);
        let mut input = Input::new().with_suggester(suggester);
        let _guard = set_dispatch_recipient(make_node_id(), focused_state());

        // Type the full value "abc"
        for ch in ['a', 'b', 'c'] {
            let mut ctx = EventCtx::default();
            input.on_event(
                &Event::Key(KeyEventData::from_crossterm(KeyEvent::new(
                    KeyCode::Char(ch),
                    KeyModifiers::NONE,
                ))),
                &mut ctx,
            );
        }
        // Suggestion should be empty because "abc" matches exactly (no ghost text to show).
        assert!(input.suggestion.is_empty());
    }

    #[test]
    fn tab_accept_emits_input_changed() {
        let mut input = Input::new().with_suggester(SuggestFromList::new(vec!["Portugal"], false));
        let _guard = set_dispatch_recipient(make_node_id(), focused_state());

        // Type 'p'
        let mut ctx = EventCtx::default();
        input.on_event(
            &Event::Key(KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Char('p'),
                KeyModifiers::NONE,
            ))),
            &mut ctx,
        );
        let _ = ctx.take_messages(); // discard the first InputChanged

        // Tab accepts
        let mut ctx = EventCtx::default();
        input.on_event(
            &Event::Key(KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Tab,
                KeyModifiers::NONE,
            ))),
            &mut ctx,
        );
        let messages = ctx.take_messages();
        assert_eq!(messages.len(), 1);
        assert!(
            messages[0]
                .downcast_ref::<InputChanged>()
                .is_some_and(|m| m.value == "Portugal")
        );
    }

    // ── Reactive field tests ────────────────────────────────────────────

    #[test]
    fn reactive_value_getter() {
        let mut input = Input::new();
        input.set_text("hello");
        assert_eq!(input.value(), "hello");
    }

    #[test]
    fn reactive_set_value_records_change() {
        let mut input = Input::new();
        let id = make_node_id();
        let mut ctx = ReactiveCtx::new(id);
        input.set_value("hello".to_string(), &mut ctx);
        assert_eq!(input.value(), "hello");
        assert!(ctx.has_changes());
        assert!(ctx.needs_repaint());
        assert_eq!(ctx.changes()[0].field_name, "value");
    }

    #[test]
    fn reactive_set_value_noop_when_same() {
        let mut input = Input::new();
        input.set_text("same");
        let id = make_node_id();
        let mut ctx = ReactiveCtx::new(id);
        input.set_value("same".to_string(), &mut ctx);
        assert!(!ctx.has_changes());
    }

    #[test]
    fn reactive_dispatch_clamps_cursor() {
        let mut input = Input::new();
        input.set_text("hello world");
        input.cursor = 11; // at end
        input.selection = Selection::cursor(11);
        let id = make_node_id();
        let mut ctx = ReactiveCtx::new(id);
        input.set_value("hi".to_string(), &mut ctx);
        let changes = ctx.take_changes();
        input.reactive_dispatch(&changes, &mut ctx);
        // Cursor should be clamped to the new text length
        assert!(input.cursor <= input.text.len());
        assert!(input.suggestion.is_empty());
    }

    #[test]
    fn reactive_set_placeholder_records_change() {
        let mut input = Input::new();
        let id = make_node_id();
        let mut ctx = ReactiveCtx::new(id);
        input.set_placeholder(Some("Enter name".to_string()), &mut ctx);
        assert_eq!(input.placeholder(), Some("Enter name"));
        assert!(ctx.has_changes());
        assert_eq!(ctx.changes()[0].field_name, "placeholder");
    }

    #[test]
    fn reactive_set_placeholder_noop_when_same() {
        let mut input = Input::new().with_placeholder("Enter name");
        let id = make_node_id();
        let mut ctx = ReactiveCtx::new(id);
        input.set_placeholder(Some("Enter name".to_string()), &mut ctx);
        assert!(!ctx.has_changes());
    }

    // ── P1-14 dispatch-context regression tests ─────────────────────────

    #[test]
    fn mouse_click_with_dispatch_context_is_handled() {
        let mut input = Input::new();
        input.set_text("hello");

        let id = make_node_id();
        let _guard = set_dispatch_recipient(id, NodeState::default());

        let mut ctx = EventCtx::default();
        input.on_event(
            &Event::MouseDown(MouseDownEvent {
                target: id,
                screen_x: 0,
                screen_y: 0,
                x: 2,
                y: 0,
            }),
            &mut ctx,
        );
        assert!(ctx.handled());
        assert_eq!(input.cursor, 2);
    }

    #[test]
    fn mouse_click_with_wrong_target_is_ignored() {
        use slotmap::SlotMap;

        let mut input = Input::new();
        input.set_text("hello");

        let mut sm: SlotMap<NodeId, ()> = SlotMap::new();
        let my_id = sm.insert(());
        let other_id = sm.insert(());
        let _guard = set_dispatch_recipient(my_id, NodeState::default());

        let mut ctx = EventCtx::default();
        input.on_event(
            &Event::MouseDown(MouseDownEvent {
                target: other_id,
                screen_x: 0,
                screen_y: 0,
                x: 2,
                y: 0,
            }),
            &mut ctx,
        );
        assert!(!ctx.handled());
    }

    #[test]
    fn paste_message_with_wrong_target_is_ignored() {
        use slotmap::SlotMap;

        let mut input = Input::new();
        input.set_text("abc");

        let mut sm: SlotMap<NodeId, ()> = SlotMap::new();
        let my_id = sm.insert(());
        let other_id = sm.insert(());
        let _guard = set_dispatch_recipient(my_id, NodeState::default());

        let mut ctx = EventCtx::default();
        input.on_message(
            &MessageEvent::new(
                NodeId::default(),
                TextEditClipboardPaste {
                    target: other_id,
                    text: "XYZ".to_string(),
                },
            ),
            &mut ctx,
        );
        assert!(!ctx.handled());
        assert_eq!(input.text(), "abc");
    }
}
