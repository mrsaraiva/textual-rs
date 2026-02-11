use crossterm::event::KeyCode;
use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::event::{Event, EventCtx, MouseDownEvent};
use crate::message::{Message, MessageEvent};
use crate::render::{Cell, FrameBuffer};

use super::helpers::{adjust_line_length_no_bg, empty_classes};
use super::option_list::toggle_option::OptionCursorState;
use super::option_list::{OptionItem, OptionList};
use super::{Widget, WidgetId, WidgetStyles};

/// A dropdown select control.
///
/// Shows the current selection (or a placeholder prompt) with a dropdown arrow.
/// On activation (Enter/Space/click), opens an [`OptionList`] overlay for choosing a value.
///
/// Generic over the value type `T`.
pub struct Select<T: Clone + PartialEq + Send + Sync + 'static> {
    id: WidgetId,
    options: Vec<(String, T)>,
    cursor: OptionCursorState,
    prompt: String,
    disabled: bool,
    open: bool,
    list: OptionList,
    focused: bool,
    hovered: bool,
    viewport_width: usize,
    viewport_height: usize,
    classes: Vec<String>,
    focused_classes: Vec<String>,
    styles: WidgetStyles,
}

impl<T: Clone + PartialEq + Send + Sync + 'static> Select<T> {
    /// Create a new `Select` widget.
    ///
    /// `options` is a list of `(label, value)` pairs.
    /// `prompt` is shown when nothing is selected.
    pub fn new(options: Vec<(String, T)>, prompt: impl Into<String>) -> Self {
        let list_items: Vec<OptionItem> = options
            .iter()
            .map(|(label, _)| OptionItem::new(label.as_str()))
            .collect();
        let mut list = OptionList::with_items(list_items);
        list.set_focus(true);

        Self {
            id: WidgetId::new(),
            options,
            cursor: OptionCursorState::default(),
            prompt: prompt.into(),
            disabled: false,
            open: false,
            list,
            focused: false,
            hovered: false,
            viewport_width: 20,
            viewport_height: 10,
            classes: vec!["select".to_string()],
            focused_classes: vec!["select".to_string(), "focused".to_string()],
            styles: WidgetStyles::default(),
        }
    }

    // ── Public API ──────────────────────────────────────────────────

    /// The currently selected value, or `None`.
    pub fn value(&self) -> Option<&T> {
        self.cursor
            .selected()
            .and_then(|i| self.options.get(i).map(|(_, v)| v))
    }

    /// Programmatically set the value. If the value is not found, selection is cleared.
    pub fn set_value(&mut self, value: &T) {
        let selected = self.options.iter().position(|(_, v)| v == value);
        self.cursor.set_selected(selected);
        self.cursor.set_highlighted(selected);
        if let Some(index) = selected {
            self.list.set_highlighted(index);
        } else {
            self.list.clear_highlighted();
        }
    }

    /// Clear the current selection (revert to prompt state).
    pub fn clear(&mut self) {
        self.cursor.clear();
        self.list.clear_highlighted();
    }

    /// Whether the dropdown overlay is currently open.
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Builder: set disabled state for the entire select.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        if disabled {
            self.focused = false;
            self.open = false;
            self.list.set_focus(false);
        }
        self
    }

    /// Replace all options. Clears the current selection.
    pub fn set_options(&mut self, options: Vec<(String, T)>) {
        let list_items: Vec<OptionItem> = options
            .iter()
            .map(|(label, _)| OptionItem::new(label.as_str()))
            .collect();
        self.options = options;
        self.cursor.clear();
        self.list.set_items(list_items);
    }

    // ── Internals ───────────────────────────────────────────────────

    fn set_open(&mut self, open: bool, ctx: &mut EventCtx) {
        if self.open == open {
            return;
        }
        self.open = open;
        if self.open {
            // Sync list highlight with current selection.
            if let Some(selected) = self.cursor.selected() {
                self.list.set_highlighted(selected);
                self.cursor.set_highlighted(Some(selected));
            } else if let Some(first) = self.list.first_selectable_index() {
                self.list.set_highlighted(first);
                self.cursor.set_highlighted(Some(first));
            } else {
                self.list.clear_highlighted();
                self.cursor.set_highlighted(None);
            }
            self.list.set_focus(true);
        } else {
            self.list.set_focus(false);
        }
        ctx.request_repaint();
    }

    fn apply_selection(&mut self, index: usize, ctx: &mut EventCtx) {
        if index >= self.options.len() {
            return;
        }
        let changed = self.cursor.selected() != Some(index);
        self.cursor.set_selected(Some(index));
        self.cursor.set_highlighted(Some(index));
        self.set_open(false, ctx);
        if changed {
            let label = self.options[index].0.clone();
            ctx.post_message(self.id, Message::SelectChanged { index, label });
        }
    }

    /// Geometry for the dropdown overlay panel.
    fn dropdown_geometry(&self) -> (usize, usize, usize, usize) {
        let panel_x = 0usize;
        let panel_y = 1usize; // directly below the closed-state line
        let panel_width = self.viewport_width.max(1);
        let available_height = self.viewport_height.saturating_sub(panel_y).max(1);
        let desired = self.options.len().max(1);
        let panel_height = desired.min(available_height).min(12).max(1);
        (panel_x, panel_y, panel_width, panel_height)
    }

    /// Render the closed state: "  Selected Label   ▼" or "  Prompt...   ▼".
    fn render_closed(&self, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let mut classes = vec!["select--current-value"];
        if self.focused {
            classes.push("-focus");
        }
        if self.hovered {
            classes.push("-hover");
        }
        let label_style = crate::css::resolve_component_style(self, &classes)
            .to_rich()
            .unwrap_or_else(rich_rs::Style::new);

        let arrow_classes = if self.open {
            vec!["select--arrow", "-open"]
        } else {
            vec!["select--arrow"]
        };
        let arrow_style = crate::css::resolve_component_style(self, &arrow_classes)
            .to_rich()
            .unwrap_or(label_style);

        let label_text = if let Some(index) = self.cursor.selected() {
            self.options[index].0.as_str()
        } else {
            &self.prompt
        };

        let arrow = if self.open { "▲" } else { "▼" };
        // Reserve 2 cells for the arrow (space + arrow char).
        let label_width = width.saturating_sub(2).max(1);
        let label_seg = Segment::styled(
            rich_rs::set_cell_size(&format!(" {label_text}"), label_width),
            label_style,
        );
        let arrow_seg = Segment::styled(format!(" {arrow}"), arrow_style);

        let line = adjust_line_length_no_bg(&[label_seg, arrow_seg], width);
        let mut out = Segments::new();
        out.extend(line);
        out
    }
}

impl<T: Clone + PartialEq + Send + Sync + 'static> Widget for Select<T> {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn focusable(&self) -> bool {
        !self.disabled
    }

    fn set_focus(&mut self, focused: bool) {
        self.focused = focused && !self.disabled;
        if !focused && self.open {
            // Close dropdown when focus is lost.
            self.open = false;
            self.list.set_focus(false);
        }
    }

    fn has_focus(&self) -> bool {
        self.focused
    }

    fn is_disabled(&self) -> bool {
        self.disabled
    }

    fn is_hovered(&self) -> bool {
        self.hovered
    }

    fn set_hovered(&mut self, hovered: bool) {
        self.hovered = hovered;
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.viewport_width = usize::from(width).max(1);
        self.viewport_height = usize::from(height).max(1);
        if self.open {
            let (_, _, pw, ph) = self.dropdown_geometry();
            self.list.on_layout(pw as u16, ph as u16);
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if self.disabled {
            return;
        }
        if self.open {
            // When the overlay is open, handle its events first.
            match event {
                Event::Key(key) => match key.code {
                    KeyCode::Esc => {
                        self.set_open(false, ctx);
                        ctx.set_handled();
                        return;
                    }
                    KeyCode::Enter => {
                        if self.list.highlighted().is_none() {
                            self.set_open(false, ctx);
                        } else {
                            // Route selection through OptionList message flow.
                            self.list.on_event(event, ctx);
                            self.cursor.set_highlighted(self.list.highlighted());
                        }
                        ctx.set_handled();
                        return;
                    }
                    _ => {}
                },
                Event::MouseDown(mouse) => {
                    if mouse.target != self.id && mouse.target != self.list.id() {
                        // Click outside the Select widget — close dropdown.
                        self.set_open(false, ctx);
                        ctx.set_handled();
                        return;
                    }
                    if mouse.target == self.list.id() {
                        // Click inside dropdown list coordinates.
                        self.list.on_event(event, ctx);
                        self.cursor.set_highlighted(self.list.highlighted());
                    } else {
                        // Click within Select — check if it's in the dropdown area.
                        let (_, panel_y, _, panel_h) = self.dropdown_geometry();
                        let click_y = mouse.y as usize;
                        if click_y >= panel_y && click_y < panel_y + panel_h {
                            // Translate click to OptionList coordinates and route through its
                            // message flow.
                            let list_y = click_y - panel_y;
                            self.list.on_event(
                                &Event::MouseDown(MouseDownEvent {
                                    target: self.list.id(),
                                    screen_x: mouse.screen_x,
                                    screen_y: mouse.screen_y,
                                    x: mouse.x,
                                    y: list_y as u16,
                                }),
                                ctx,
                            );
                            self.cursor.set_highlighted(self.list.highlighted());
                        } else {
                            // Click on the closed-state bar area — toggle closed.
                            self.set_open(false, ctx);
                        }
                    }
                    ctx.set_handled();
                    return;
                }
                _ => {}
            }
            // Delegate navigation keys to the inner OptionList.
            self.list.on_event(event, ctx);
            self.cursor.set_highlighted(self.list.highlighted());
            if !ctx.handled() {
                // Absorb all events when overlay is open.
                ctx.set_handled();
            }
        } else {
            // Closed state: open on Enter/Space/click.
            match event {
                Event::Key(key) if self.focused => match key.code {
                    KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Down | KeyCode::Up => {
                        self.set_open(true, ctx);
                        ctx.set_handled();
                    }
                    _ => {}
                },
                Event::MouseDown(mouse) if mouse.target == self.id => {
                    self.set_open(true, ctx);
                    ctx.set_handled();
                }
                _ => {}
            }
        }
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut EventCtx) {
        // Handle OptionSelected from inner list.
        if message.sender == self.list.id() {
            if let Message::OptionSelected { index } = &message.message {
                self.apply_selection(*index, ctx);
                ctx.set_handled();
                return;
            }
        }
    }

    fn on_mouse_move(&mut self, x: u16, y: u16) -> bool {
        if self.disabled {
            return false;
        }
        if self.open {
            // Forward to the list if the mouse is within the dropdown area.
            let (_, panel_y, _, panel_h) = self.dropdown_geometry();
            let y_usize = y as usize;
            if y_usize >= panel_y && y_usize < panel_y + panel_h {
                return self.list.on_mouse_move(x, (y_usize - panel_y) as u16);
            }
        }
        false
    }

    fn on_mouse_scroll(&mut self, delta_x: i32, delta_y: i32, ctx: &mut EventCtx) {
        if self.disabled {
            return;
        }
        if self.open {
            self.list.on_mouse_scroll(delta_x, delta_y, ctx);
            if !ctx.handled() {
                ctx.set_handled();
            }
        }
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        if !self.open {
            return self.render_closed(options);
        }

        // Open state: render closed bar + dropdown overlay below it.
        let (width, height) = options.size;
        let width = width.max(1);
        let height = height.max(1);

        // Render the closed-state line into the top row of a full-height buffer.
        let mut closed_options = options.clone();
        closed_options.size = (width, 1);
        closed_options.max_width = width;
        closed_options.max_height = 1;
        let closed_segments = self.render_closed(&closed_options);
        let closed_lines =
            Segment::split_and_crop_lines(closed_segments, width, None, false, false);
        let closed_buf = FrameBuffer::from_lines(&closed_lines, width, 1, None);
        let mut merged = FrameBuffer::new(width, height, None);
        for x in 0..width.min(closed_buf.width) {
            *merged.get_mut(x, 0) = closed_buf.get(x, 0).clone();
        }

        // Render the dropdown OptionList.
        let (panel_x, panel_y, panel_width, panel_height) = self.dropdown_geometry();
        let panel_width = panel_width.min(width);
        let panel_height = panel_height.min(height.saturating_sub(panel_y));
        if panel_height == 0 {
            return merged.to_segments();
        }

        let panel_style = crate::css::resolve_component_style(self, &["select--dropdown"])
            .to_rich()
            .unwrap_or_else(rich_rs::Style::new);

        // Clear the dropdown area.
        for y in panel_y..panel_y.saturating_add(panel_height).min(height) {
            for x in panel_x..panel_x.saturating_add(panel_width).min(width) {
                *merged.get_mut(x, y) = Cell::blank(Some(panel_style));
            }
        }

        // Render the OptionList into a sub-buffer.
        let mut list_options = options.clone();
        list_options.size = (panel_width, panel_height);
        list_options.max_width = panel_width;
        list_options.max_height = panel_height;
        let list_buffer = FrameBuffer::from_renderable(console, &list_options, &self.list, None);

        for sy in 0..list_buffer.height.min(panel_height) {
            let ty = panel_y.saturating_add(sy);
            if ty >= height {
                break;
            }
            for sx in 0..list_buffer.width.min(panel_width) {
                let tx = panel_x.saturating_add(sx);
                if tx >= width {
                    break;
                }
                *merged.get_mut(tx, ty) = list_buffer.get(sx, sy).clone();
            }
        }

        merged.to_segments()
    }

    fn layout_height(&self) -> Option<usize> {
        // When closed, 1 line. When open, 1 + dropdown height.
        if self.open {
            let (_, _, _, ph) = self.dropdown_geometry();
            Some(1 + ph)
        } else {
            Some(1)
        }
    }

    fn content_width(&self) -> Option<usize> {
        let label_width = self
            .options
            .iter()
            .map(|(label, _)| rich_rs::cell_len(label))
            .max()
            .unwrap_or(0)
            .max(rich_rs::cell_len(&self.prompt));
        // label + space padding + arrow
        Some(label_width.saturating_add(3).max(1))
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

    fn style_type(&self) -> &'static str {
        "Select"
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }

    // NOTE: Select intentionally does NOT implement visit_children_mut.
    // The inner OptionList is a private implementation detail and should not
    // appear in the global focus traversal — Select manages it internally.
}

impl<T: Clone + PartialEq + Send + Sync + 'static> Renderable for Select<T> {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{Event, EventCtx, MouseDownEvent};
    use crate::keys::KeyEventData;
    use crate::message::Message;
    use crate::message::MessageEvent;
    use crossterm::event::{KeyEvent, KeyModifiers};

    fn make_select() -> Select<i32> {
        Select::new(
            vec![
                ("Alpha".to_string(), 1),
                ("Beta".to_string(), 2),
                ("Gamma".to_string(), 3),
            ],
            "Pick one...",
        )
    }

    fn dispatch_messages(sel: &mut Select<i32>, ctx: &mut EventCtx) -> Vec<MessageEvent> {
        let mut delivered = Vec::new();
        loop {
            let batch = ctx.take_messages();
            if batch.is_empty() {
                break;
            }
            delivered.extend(batch.clone());
            for message in batch {
                sel.on_message(&message, ctx);
            }
        }
        delivered
    }

    #[test]
    fn select_starts_closed_with_no_value() {
        let sel = make_select();
        assert!(!sel.is_open());
        assert!(sel.value().is_none());
    }

    #[test]
    fn select_opens_on_enter() {
        let mut sel = make_select();
        sel.set_focus(true);
        sel.on_layout(30, 20);

        let key = KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let mut ctx = EventCtx::default();
        sel.on_event(&Event::Key(key), &mut ctx);
        assert!(sel.is_open());
        assert!(ctx.handled());
    }

    #[test]
    fn select_closes_on_escape() {
        let mut sel = make_select();
        sel.set_focus(true);
        sel.on_layout(30, 20);

        // Open
        let key = KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let mut ctx = EventCtx::default();
        sel.on_event(&Event::Key(key), &mut ctx);
        assert!(sel.is_open());

        // Close
        let esc = KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        let mut ctx2 = EventCtx::default();
        sel.on_event(&Event::Key(esc), &mut ctx2);
        assert!(!sel.is_open());
    }

    #[test]
    fn select_enter_selects_highlighted_option() {
        let mut sel = make_select();
        sel.set_focus(true);
        sel.on_layout(30, 20);

        // Open
        let enter = KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let mut ctx = EventCtx::default();
        sel.on_event(&Event::Key(enter.clone()), &mut ctx);
        assert!(sel.is_open());

        // Move down once
        let down = KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        let mut ctx2 = EventCtx::default();
        sel.on_event(&Event::Key(down), &mut ctx2);

        // Confirm with Enter
        let mut ctx3 = EventCtx::default();
        sel.on_event(&Event::Key(enter), &mut ctx3);
        let delivered = dispatch_messages(&mut sel, &mut ctx3);
        assert!(!sel.is_open());
        assert_eq!(sel.value(), Some(&2)); // Beta

        let option_selected_pos = delivered
            .iter()
            .position(|m| matches!(m.message, Message::OptionSelected { index: 1 }));
        let select_changed_pos = delivered
            .iter()
            .position(|m| matches!(m.message, Message::SelectChanged { index: 1, label: _ }));
        assert!(
            option_selected_pos.is_some()
                && select_changed_pos.is_some()
                && option_selected_pos < select_changed_pos
        );
    }

    #[test]
    fn select_mouse_click_inside_dropdown_selects_item() {
        let mut sel = make_select();
        sel.set_focus(true);
        sel.on_layout(30, 20);

        let open_key =
            KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let mut open_ctx = EventCtx::default();
        sel.on_event(&Event::Key(open_key), &mut open_ctx);
        assert!(sel.is_open());

        let mut click_ctx = EventCtx::default();
        sel.on_event(
            &Event::MouseDown(MouseDownEvent {
                target: sel.list.id(),
                screen_x: 1,
                screen_y: 2,
                x: 1,
                y: 1,
            }),
            &mut click_ctx,
        );
        let delivered = dispatch_messages(&mut sel, &mut click_ctx);

        assert!(!sel.is_open());
        assert_eq!(sel.value(), Some(&2));
        assert!(click_ctx.handled());
        let option_selected_pos = delivered
            .iter()
            .position(|m| matches!(m.message, Message::OptionSelected { index: 1 }));
        let select_changed_pos = delivered
            .iter()
            .position(|m| matches!(m.message, Message::SelectChanged { index: 1, label: _ }));
        assert!(
            option_selected_pos.is_some()
                && select_changed_pos.is_some()
                && option_selected_pos < select_changed_pos
        );
    }

    #[test]
    fn select_set_value_programmatic() {
        let mut sel = make_select();
        sel.set_value(&3);
        assert_eq!(sel.value(), Some(&3));
    }

    #[test]
    fn select_clear_resets() {
        let mut sel = make_select();
        sel.set_value(&2);
        sel.clear();
        assert!(sel.value().is_none());
    }

    #[test]
    fn select_clear_then_reopen_highlights_first_selectable() {
        let mut sel = make_select();
        sel.set_value(&3);
        sel.clear();
        sel.set_focus(true);
        sel.on_layout(30, 20);

        let open = KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let mut ctx = EventCtx::default();
        sel.on_event(&Event::Key(open), &mut ctx);

        assert!(sel.is_open());
        assert_eq!(sel.list.highlighted(), Some(0));
    }

    #[test]
    fn select_ignores_disabled_option_click() {
        let mut sel = Select::new(
            vec![("Alpha".to_string(), 1), ("Beta".to_string(), 2)],
            "Pick one...",
        );
        sel.list
            .set_items(vec![OptionItem::new("Alpha"), OptionItem::disabled("Beta")]);
        sel.set_focus(true);
        sel.on_layout(30, 20);

        let open = KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let mut open_ctx = EventCtx::default();
        sel.on_event(&Event::Key(open), &mut open_ctx);
        assert!(sel.is_open());

        let mut click_ctx = EventCtx::default();
        sel.on_event(
            &Event::MouseDown(MouseDownEvent {
                target: sel.list.id(),
                screen_x: 1,
                screen_y: 2,
                x: 1,
                y: 1,
            }),
            &mut click_ctx,
        );

        assert!(sel.value().is_none());
        assert!(sel.is_open());
    }

    #[test]
    fn select_disabled_ignores_open_input() {
        let mut sel = make_select().disabled(true);
        sel.set_focus(true);
        sel.on_layout(30, 20);

        let key = KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let mut key_ctx = EventCtx::default();
        sel.on_event(&Event::Key(key), &mut key_ctx);
        assert!(!sel.is_open());
        assert!(!key_ctx.handled());

        let mut click_ctx = EventCtx::default();
        sel.on_event(
            &Event::MouseDown(MouseDownEvent {
                target: sel.id(),
                screen_x: 0,
                screen_y: 0,
                x: 0,
                y: 0,
            }),
            &mut click_ctx,
        );
        assert!(!sel.is_open());
        assert!(!click_ctx.handled());
        assert!(!sel.focusable());
    }
}
