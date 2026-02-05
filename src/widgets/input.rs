use crossterm::event::KeyCode;
use rich_rs::{Console, ConsoleOptions, Renderable, Segments};
use std::sync::Arc;
use std::time::{Duration, Instant};
use unicode_width::UnicodeWidthChar;

use crate::event::{Event, EventCtx};
use crate::message::Message;
use crate::style::{Color, parse_color_like};
use crate::validation::{ValidationResult, ValidatorRef};

use super::{
    Widget, WidgetId, WidgetStyles,
    helpers::{empty_classes, fixed_height_from_constraints},
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
        Self { start: pos, end: pos }
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
    focused: bool,
    placeholder: Option<String>,
    input_type: InputType,
    validators: Vec<ValidatorRef>,
    validation_result: ValidationResult,
    on_change: Option<Arc<dyn Fn(&mut Input) + Send + Sync>>,
    mouse_down: bool,
    cursor_visible: bool,
    cursor_blink_next_at: Option<Instant>,
    app_active: bool,
    user_classes: Vec<String>,
    classes: Vec<String>,
    focused_classes: Vec<String>,
    styles: WidgetStyles,
}

impl Input {
    // Python Textual uses `set_interval(0.5, ...)` for cursor blink.
    const CURSOR_BLINK_PERIOD: Duration = Duration::from_millis(500);

    fn next_blink_deadline() -> Instant {
        let now = Instant::now();
        now.checked_add(Self::CURSOR_BLINK_PERIOD).unwrap_or(now)
    }

    pub fn new() -> Self {
        let mut out = Self {
            id: WidgetId::new(),
            text: String::new(),
            cursor: 0,
            selection: Selection::cursor(0),
            focused: false,
            placeholder: None,
            input_type: InputType::Text,
            validators: Vec::new(),
            validation_result: ValidationResult::success(),
            on_change: None,
            mouse_down: false,
            cursor_visible: false,
            cursor_blink_next_at: None,
            app_active: true,
            user_classes: Vec::new(),
            classes: Vec::new(),
            focused_classes: Vec::new(),
            styles: WidgetStyles::default(),
        };
        out.rebuild_classes();
        out
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
        self.user_classes.push(class.into());
        self.rebuild_classes();
        self
    }

    pub fn set_class(&mut self, class: &str, enabled: bool) {
        if enabled {
            if !self.user_classes.iter().any(|c| c == class) {
                self.user_classes.push(class.to_string());
            }
        } else {
            self.user_classes.retain(|c| c != class);
        }
        self.rebuild_classes();
    }

    pub fn on_change(mut self, handler: impl Fn(&mut Input) + Send + Sync + 'static) -> Self {
        self.on_change = Some(Arc::new(handler));
        self
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
        self.selection = Selection::cursor(self.cursor);
        self.revalidate();
    }

    fn cursor_from_x(&self, x: u16) -> usize {
        // Mouse coordinates are content-local, so `x` maps to the rendered value text
        // (without borders / line-pad).
        let mut cell_x: u16 = 0;
        let mut last_boundary: usize = 0;
        for (byte_idx, ch) in self.text.char_indices() {
            let w = UnicodeWidthChar::width(ch).unwrap_or(0) as u16;
            let w = w.max(1);
            let mid = cell_x.saturating_add(w / 2);
            if x <= mid {
                return byte_idx;
            }
            cell_x = cell_x.saturating_add(w);
            last_boundary = byte_idx + ch.len_utf8();
            if x < cell_x {
                return last_boundary;
            }
        }
        last_boundary
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

    fn notify_changed(&mut self) {
        if let Some(handler) = self.on_change.clone() {
            handler(self);
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
        self.notify_changed();
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

    fn reset_blink(&mut self) {
        if !self.focused || !self.app_active {
            return;
        }
        self.cursor_visible = true;
        self.cursor_blink_next_at = Some(Self::next_blink_deadline());
    }

    fn rebuild_classes(&mut self) {
        // Textual-ish conventions: `Input` type name in selector; keep a stable "input" class.
        let mut classes = vec!["input".to_string()];
        classes.extend(self.user_classes.iter().cloned());
        let mut focused_classes = classes.clone();
        focused_classes.push("focused".to_string());
        self.classes = classes;
        self.focused_classes = focused_classes;
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
        self.focused = focused;
        if !focused {
            self.mouse_down = false;
            self.cursor_visible = false;
            self.cursor_blink_next_at = None;
            return;
        }
        self.reset_blink();
    }

    fn has_focus(&self) -> bool {
        self.focused
    }

    fn is_active(&self) -> bool {
        self.mouse_down
    }

    fn on_mouse_move(&mut self, x: u16, _y: u16) -> bool {
        if !self.mouse_down {
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
                self.app_active = *active;
                if !*active {
                    self.cursor_visible = false;
                    self.cursor_blink_next_at = None;
                } else {
                    self.reset_blink();
                }
                ctx.request_repaint();
            }
            Event::MouseDown(mouse) if mouse.target == self.id => {
                if self.text.is_empty() {
                    self.cursor = 0;
                } else {
                    self.cursor = self.cursor_from_x(mouse.x);
                }
                self.selection = Selection::cursor(self.cursor);
                self.mouse_down = true;
                self.reset_blink();
                ctx.request_repaint();
                ctx.set_handled();
            }
            Event::MouseUp(_) => {
                if self.mouse_down {
                    self.mouse_down = false;
                    ctx.request_repaint();
                }
            }
            Event::Tick(tick) => {
                let _ = tick;
                if !self.focused || !self.app_active {
                    return;
                }
                let Some(next_at) = self.cursor_blink_next_at else {
                    return;
                };
                let now = Instant::now();
                if now >= next_at {
                    self.cursor_visible = !self.cursor_visible;
                    self.cursor_blink_next_at =
                        now.checked_add(Self::CURSOR_BLINK_PERIOD).or(Some(now));
                    ctx.request_repaint();
                }
            }
            Event::Key(key) if self.focused => match key.code {
                KeyCode::Char(ch) => {
                    if self.is_allowed_char(ch) {
                        self.text.insert(self.cursor, ch);
                        self.cursor += ch.len_utf8();
                        self.selection = Selection::cursor(self.cursor);
                        self.revalidate();
                        self.post_changed(ctx);
                        self.reset_blink();
                        ctx.request_repaint();
                    }
                    ctx.set_handled();
                }
                KeyCode::Enter => {
                    ctx.post_message(
                        self.id,
                        Message::InputSubmitted {
                            value: self.text.clone(),
                        },
                    );
                    ctx.set_handled();
                }
                KeyCode::Backspace => {
                    if self.cursor > 0 {
                        // Move to previous UTF-8 boundary.
                        let prev = self.text[..self.cursor]
                            .char_indices()
                            .last()
                            .map(|(i, _)| i)
                            .unwrap_or(0);
                        self.text.drain(prev..self.cursor);
                        self.cursor = prev;
                        self.selection = Selection::cursor(self.cursor);
                        self.revalidate();
                        self.post_changed(ctx);
                        self.reset_blink();
                        ctx.request_repaint();
                        ctx.set_handled();
                    }
                }
                KeyCode::Delete => {
                    if self.cursor < self.text.len() {
                        let next = self.text[self.cursor..]
                            .char_indices()
                            .nth(1)
                            .map(|(i, _)| self.cursor + i)
                            .unwrap_or(self.text.len());
                        self.text.drain(self.cursor..next);
                        self.selection = Selection::cursor(self.cursor);
                        self.revalidate();
                        self.post_changed(ctx);
                        self.reset_blink();
                        ctx.request_repaint();
                        ctx.set_handled();
                    }
                }
                KeyCode::Left => {
                    if self.cursor > 0 {
                        self.cursor = self.text[..self.cursor]
                            .char_indices()
                            .last()
                            .map(|(i, _)| i)
                            .unwrap_or(0);
                        self.selection = Selection::cursor(self.cursor);
                        self.reset_blink();
                        ctx.request_repaint();
                        ctx.set_handled();
                    }
                }
                KeyCode::Right => {
                    if self.cursor < self.text.len() {
                        self.cursor = self.text[self.cursor..]
                            .char_indices()
                            .nth(1)
                            .map(|(i, _)| self.cursor + i)
                            .unwrap_or(self.text.len());
                        self.selection = Selection::cursor(self.cursor);
                        self.reset_blink();
                        ctx.request_repaint();
                        ctx.set_handled();
                    }
                }
                KeyCode::Home => {
                    self.cursor = 0;
                    self.selection = Selection::cursor(self.cursor);
                    self.reset_blink();
                    ctx.request_repaint();
                    ctx.set_handled();
                }
                KeyCode::End => {
                    self.cursor = self.text.len();
                    self.selection = Selection::cursor(self.cursor);
                    self.reset_blink();
                    ctx.request_repaint();
                    ctx.set_handled();
                }
                _ => {}
            },
            _ => {}
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
            let mut rich = style.to_rich_without_colors().unwrap_or_else(rich_rs::Style::new);
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
            if self.focused && self.cursor_visible {
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

        let (sel_start, sel_end) = if self.focused && self.mouse_down {
            // Selection only exists while dragging for now.
            (self.selection.start.min(self.text.len()), self.selection.end.min(self.text.len()))
        } else {
            (self.cursor.min(self.text.len()), self.cursor.min(self.text.len()))
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
            out.push(rich_rs::Segment::styled(std::mem::take(pending_text), style));
        };

        for (byte_idx, ch) in self.text.char_indices() {
            let w = UnicodeWidthChar::width(ch).unwrap_or(0).max(1);
            if cells_used.saturating_add(w) > width {
                break;
            }

            let is_cursor = self.focused && self.cursor_visible && byte_idx == self.cursor;
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
            pending_text.push(ch);
            cells_used = cells_used.saturating_add(w);
        }

        flush(&mut out, &mut pending_style, &mut pending_text);

        if self.focused && self.cursor_visible && self.cursor == self.text.len() && cells_used < width {
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
        if self.focused {
            &self.focused_classes
        } else if self.classes.is_empty() {
            empty_classes()
        } else {
            &self.classes
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
        assert!(input.mouse_down);
        let changed = input.on_mouse_move(4, 0);
        assert!(changed);
        let (a, b) = input.selection.normalized();
        assert_eq!((a, b), (1, 4));
    }
}
