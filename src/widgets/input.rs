use rich_rs::{Console, ConsoleOptions, Renderable, Segments};
use std::time::Instant;
use unicode_segmentation::UnicodeSegmentation;

use crate::event::{Event, EventCtx};
use crate::message::{Message, MessageEvent};
use crate::style::{Color, parse_color_like};
use crate::validation::{ValidationResult, ValidatorRef};

use super::{
    Widget, WidgetId, WidgetStyles,
    helpers::{empty_classes, fixed_height_from_constraints},
    input_chrome::InputChrome,
    text_edit::{
        EditCommand, MoveUnit, byte_index_from_cell_x, clamp_grapheme_boundary,
        edit_command_from_key, grapheme_cell_width, next_grapheme_boundary, next_word_boundary,
        prev_grapheme_boundary, prev_word_boundary,
    },
};

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
    id: WidgetId,
    text: String,
    cursor: usize,
    selection: Selection,
    placeholder: Option<String>,
    input_type: InputType,
    validators: Vec<ValidatorRef>,
    validation_result: ValidationResult,
    chrome: InputChrome,
    styles: WidgetStyles,
}

impl Input {
    pub fn new() -> Self {
        Self {
            id: WidgetId::new(),
            text: String::new(),
            cursor: 0,
            selection: Selection::cursor(0),
            placeholder: None,
            input_type: InputType::Text,
            validators: Vec::new(),
            validation_result: ValidationResult::success(),
            chrome: InputChrome::new(),
            styles: WidgetStyles::default(),
        }
    }

    pub fn with_placeholder(mut self, value: impl Into<String>) -> Self {
        self.placeholder = Some(value.into());
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
        self.chrome.add_user_class(class.into());
        self
    }

    pub fn set_class(&mut self, class: &str, enabled: bool) {
        self.chrome.set_class(class, enabled);
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
        self.revalidate();
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

    fn post_changed(&mut self, ctx: &mut EventCtx) {
        ctx.post_message(
            self.id,
            Message::InputChanged {
                value: self.text.clone(),
                validation: self.validation_result.clone(),
            },
        );
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

    fn selected_text(&self) -> Option<String> {
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

    fn insert_text_from_clipboard(&mut self, text: &str) -> bool {
        if text.is_empty() {
            return false;
        }
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
}

impl Widget for Input {
    fn id(&self) -> WidgetId {
        self.id
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
        // Groundwork for selection: update selection end (and cursor) while dragging.
        let next = self.cursor_from_x(x);
        if next == self.selection.end && next == self.cursor {
            return false;
        }
        self.selection.end = next;
        self.cursor = next;
        true
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        match event {
            Event::AppFocus(active) => {
                self.chrome.handle_app_focus(*active);
                ctx.request_repaint();
            }
            Event::MouseDown(mouse) if mouse.target == self.id => {
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
            Event::MouseUp(_) => {
                if self.chrome.is_mouse_down() {
                    self.chrome.set_mouse_down(false);
                    ctx.request_repaint();
                }
            }
            Event::Tick(tick) => {
                let _ = tick;
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
                    EditCommand::InsertChar(ch) => {
                        if self.is_allowed_char(ch) {
                            self.delete_selection_if_any();
                            self.text.insert(self.cursor, ch);
                            self.cursor += ch.len_utf8();
                            self.cursor = clamp_grapheme_boundary(&self.text, self.cursor);
                            self.selection = Selection::cursor(self.cursor);
                            changed = true;
                            value_changed = true;
                        }
                    }
                    EditCommand::Submit => {
                        ctx.post_message(
                            self.id,
                            Message::InputSubmitted {
                                value: self.text.clone(),
                            },
                        );
                    }
                    EditCommand::Copy => {
                        if let Some(text) = self.selected_text() {
                            ctx.post_message(
                                self.id,
                                Message::TextEditClipboardCopyRequested { text, cut: false },
                            );
                        }
                    }
                    EditCommand::Cut => {
                        if let Some(text) = self.selected_text() {
                            ctx.post_message(
                                self.id,
                                Message::TextEditClipboardCopyRequested { text, cut: true },
                            );
                            if self.delete_selection_if_any() {
                                changed = true;
                                value_changed = true;
                            }
                        }
                    }
                    EditCommand::Paste => {
                        ctx.post_message(
                            self.id,
                            Message::TextEditClipboardPasteRequested { target: self.id },
                        );
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
                    EditCommand::MoveHome { select } => {
                        changed = self.move_cursor_to(0, select);
                    }
                    EditCommand::MoveEnd { select } => {
                        changed = self.move_cursor_to(self.text.len(), select);
                    }
                    EditCommand::InsertNewline
                    | EditCommand::MoveUp { .. }
                    | EditCommand::MoveDown { .. } => {}
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
            if *target != self.id {
                return;
            }
            if self.insert_text_from_clipboard(text) {
                self.revalidate();
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

        // Resolve base widget style so we can composite component colors (which may include alpha).
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
        let selection_style = resolve_component_rich("input--selection");
        let placeholder_style = resolve_component_rich("input--placeholder");

        if self.text.is_empty() {
            let placeholder = self.placeholder.clone().unwrap_or_default();
            let line = rich_rs::set_cell_size(&placeholder, width);
            if self.chrome.has_focus() && self.chrome.cursor_visible() {
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
            if self.chrome.has_focus() && self.selection.start != self.selection.end {
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
            let style = pending_style.take().unwrap_or_else(rich_rs::Style::new);
            out.push(rich_rs::Segment::styled(
                std::mem::take(pending_text),
                style,
            ));
        };

        for (byte_idx, grapheme) in self.text.grapheme_indices(true) {
            let w = grapheme_cell_width(grapheme);
            if cells_used.saturating_add(w) > width {
                break;
            }

            let is_cursor =
                self.chrome.has_focus() && self.chrome.cursor_visible() && byte_idx == self.cursor;
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
            pending_text.push_str(grapheme);
            cells_used = cells_used.saturating_add(w);
        }

        flush(&mut out, &mut pending_style, &mut pending_text);

        if self.chrome.has_focus()
            && self.chrome.cursor_visible()
            && self.cursor == self.text.len()
            && cells_used < width
        {
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
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn mouse_click_positions_cursor_in_text() {
        let mut input = Input::new();
        input.set_text("hello");
        input.set_focus(true);
        let id = input.id();
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
        input.set_focus(true);
        let id = input.id();
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
        input.set_focus(true);
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
        assert!(matches!(
            messages[0].message,
            Message::InputChanged { ref value, .. } if value == "a"
        ));
    }

    #[test]
    fn enter_emits_input_submitted_message() {
        let mut input = Input::new();
        input.set_focus(true);
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
        assert!(matches!(
            messages[0].message,
            Message::InputSubmitted { ref value } if value == "done"
        ));
    }

    #[test]
    fn left_and_backspace_respect_grapheme_clusters() {
        let mut input = Input::new();
        input.set_focus(true);
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
        input.set_focus(true);
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
        input.set_focus(true);
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
        input.set_focus(true);
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
        assert!(matches!(
            copy_messages.first().map(|m| &m.message),
            Some(Message::TextEditClipboardCopyRequested { text, cut }) if text == "hello" && !cut
        ));

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
                Message::TextEditClipboardCopyRequested { ref text, cut: true } if text == "hello"
            )
        }));
        assert_eq!(input.text(), " world");
    }

    #[test]
    fn paste_request_and_message_updates_input_value() {
        let mut input = Input::new();
        input.set_focus(true);
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
        assert!(matches!(
            messages.first().map(|m| &m.message),
            Some(Message::TextEditClipboardPasteRequested { target }) if *target == input.id()
        ));

        let mut ctx = EventCtx::default();
        input.on_message(
            &MessageEvent {
                sender: input.id(),
                message: Message::TextEditClipboardPaste {
                    target: input.id(),
                    text: "XYZ".to_string(),
                },
            },
            &mut ctx,
        );
        assert_eq!(input.text(), "aXYZbc");
        assert!(ctx.handled());
    }
}
