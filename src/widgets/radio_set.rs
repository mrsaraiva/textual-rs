use crossterm::event::KeyCode;
use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::event::{Event, EventCtx};
use crate::message::{Message, MessageEvent};

use super::{
    helpers::{adjust_line_length_no_bg, empty_classes, fixed_height_from_constraints},
    option_list::toggle_option::OptionCursorState,
    radio_button::RadioButton,
    Widget, WidgetId, WidgetStyles,
};

/// A container widget that groups `RadioButton` children for mutual exclusion.
///
/// When one radio button is toggled on, all others are automatically deselected.
/// The set itself is focusable and handles keyboard navigation (Up/Down) between
/// its children. Individual RadioButtons inside a set do not receive independent
/// focus — the set manages focus delegation visually via the selected index.
#[derive(Debug, Clone)]
pub struct RadioSet {
    id: WidgetId,
    buttons: Vec<RadioButton>,
    cursor: OptionCursorState,
    focused: bool,
    hovered: bool,
    hovered_index: Option<usize>,
    classes: Vec<String>,
    focused_classes: Vec<String>,
    styles: WidgetStyles,
}

impl RadioSet {
    /// Create a new empty RadioSet.
    pub fn new() -> Self {
        Self {
            id: WidgetId::new(),
            buttons: Vec::new(),
            cursor: OptionCursorState::default(),
            focused: false,
            hovered: false,
            hovered_index: None,
            classes: vec!["radio-set".to_string()],
            focused_classes: vec!["radio-set".to_string(), "focused".to_string()],
            styles: WidgetStyles::default(),
        }
    }

    /// Create a RadioSet from string labels. Each label becomes a RadioButton.
    pub fn from_labels(labels: &[&str]) -> Self {
        let mut set = Self::new();
        for label in labels {
            set.buttons.push(RadioButton::new(*label));
        }
        // Select the first button by default if any exist.
        if !set.buttons.is_empty() {
            set.cursor.set_highlighted(Some(0));
        }
        set
    }

    /// Builder: add a RadioButton to the set.
    /// If the button is pre-selected (value=true), it becomes the pressed button
    /// and any previously pressed button is deselected.
    pub fn with_button(mut self, button: RadioButton) -> Self {
        self.add_button(button);
        self
    }

    /// Add a RadioButton after construction.
    /// If the button is pre-selected (value=true), it becomes the pressed button
    /// and any previously pressed button is deselected.
    pub fn add_button(&mut self, button: RadioButton) {
        let index = self.buttons.len();
        if button.value() {
            // Enforce mutual exclusion: deselect any previously pressed button.
            if let Some(prev) = self.cursor.selected() {
                if let Some(btn) = self.buttons.get_mut(prev) {
                    btn.set_value_silent(false);
                }
            }
            self.cursor.set_selected(Some(index));
        }
        self.buttons.push(button);
        if self.cursor.highlighted().is_none() {
            self.cursor.set_highlighted(Some(0));
        }
    }

    /// Returns the index of the currently pressed (on) button, or `None`.
    pub fn pressed_index(&self) -> Option<usize> {
        self.cursor.selected()
    }

    /// Returns the currently selected (highlighted) index.
    pub fn selected_index(&self) -> usize {
        self.cursor.highlighted().unwrap_or(0)
    }

    /// Returns a reference to the button at `index`, if it exists.
    pub fn button(&self, index: usize) -> Option<&RadioButton> {
        self.buttons.get(index)
    }

    /// Returns a mutable reference to the button at `index`.
    pub fn button_mut(&mut self, index: usize) -> Option<&mut RadioButton> {
        self.buttons.get_mut(index)
    }

    /// Returns the number of buttons in the set.
    pub fn len(&self) -> usize {
        self.buttons.len()
    }

    /// Returns `true` if the set contains no buttons.
    pub fn is_empty(&self) -> bool {
        self.buttons.is_empty()
    }

    /// Move the selection cursor by `delta` (-1 for up, +1 for down), wrapping.
    fn move_selection(&mut self, delta: isize) {
        if self.buttons.is_empty() {
            return;
        }
        let len = self.buttons.len() as isize;
        let current = self.cursor.highlighted().unwrap_or(0) as isize;
        let next = ((current + delta) % len + len) % len;
        self.cursor.set_highlighted(Some(next as usize));
    }

    /// Toggle the currently selected button. Enforces mutual exclusion:
    /// if the selected button is being turned on, turn off the previously pressed one.
    fn toggle_selected(&mut self, ctx: &mut EventCtx) {
        if self.buttons.is_empty() {
            return;
        }
        let index = self.cursor.highlighted().unwrap_or(0);
        let already_pressed = self.cursor.selected() == Some(index);

        if already_pressed {
            // In a radio set, clicking the already-on button should keep it on
            // (same as Python Textual: prevents deselecting).
            ctx.set_handled();
            return;
        }

        // Turn off the previously pressed button.
        if let Some(prev) = self.cursor.selected() {
            if let Some(btn) = self.buttons.get_mut(prev) {
                btn.set_value_silent(false);
            }
        }

        // Turn on the newly selected button.
        if let Some(btn) = self.buttons.get_mut(index) {
            btn.set_value_silent(true);
        }
        self.cursor.set_selected(Some(index));

        let button_id = self.buttons[index].id();
        ctx.post_message(self.id, Message::RadioSetChanged { index, button_id });
        ctx.request_repaint();
        ctx.set_handled();
    }
}

impl Widget for RadioSet {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn focusable(&self) -> bool {
        !self.buttons.is_empty()
    }

    fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
    }

    fn has_focus(&self) -> bool {
        self.focused
    }

    fn is_hovered(&self) -> bool {
        self.hovered
    }

    fn set_hovered(&mut self, hovered: bool) {
        self.hovered = hovered;
        if !hovered {
            self.hovered_index = None;
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if self.buttons.is_empty() {
            return;
        }
        match event {
            Event::MouseDown(mouse) if mouse.target == self.id => {
                // Determine which button was clicked by y coordinate.
                let index = mouse.y as usize;
                if index < self.buttons.len() {
                    self.cursor.set_highlighted(Some(index));
                    self.toggle_selected(ctx);
                }
            }
            Event::Key(key) if self.focused => match key.code {
                KeyCode::Up | KeyCode::Left => {
                    self.move_selection(-1);
                    ctx.request_repaint();
                    ctx.set_handled();
                }
                KeyCode::Down | KeyCode::Right => {
                    self.move_selection(1);
                    ctx.request_repaint();
                    ctx.set_handled();
                }
                KeyCode::Enter | KeyCode::Char(' ') => {
                    self.toggle_selected(ctx);
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut EventCtx) {
        // Intercept RadioButtonChanged messages from child buttons.
        // This handles the case where a child button is toggled directly
        // (e.g. via its own event handler if it ever receives one).
        if let Message::RadioButtonChanged { value } = &message.message {
            // Find which button sent this message.
            if let Some(index) = self.buttons.iter().position(|b| b.id() == message.sender) {
                if *value {
                    // A button was turned on — enforce mutual exclusion.
                    if let Some(prev) = self.cursor.selected() {
                        if prev != index {
                            if let Some(btn) = self.buttons.get_mut(prev) {
                                btn.set_value_silent(false);
                            }
                        }
                    }
                    self.cursor.set_selected(Some(index));
                    self.cursor.set_highlighted(Some(index));

                    let button_id = self.buttons[index].id();
                    ctx.post_message(self.id, Message::RadioSetChanged { index, button_id });
                    ctx.request_repaint();
                } else {
                    // A button was turned off — in a radio set, prevent deselection.
                    // Re-enable the button silently.
                    if let Some(btn) = self.buttons.get_mut(index) {
                        btn.set_value_silent(true);
                    }
                }
                ctx.set_handled();
            }
        }
    }

    fn on_mouse_move(&mut self, _x: u16, y: u16) -> bool {
        if self.buttons.is_empty() {
            return false;
        }
        let index = y as usize;
        let hovered = (index < self.buttons.len()).then_some(index);
        if hovered != self.hovered_index {
            self.hovered_index = hovered;
            return true;
        }
        false
    }

    // NOTE: RadioSet intentionally does NOT implement visit_children_mut.
    // The set manages focus delegation internally — individual RadioButtons
    // should not appear in the global focus traversal.

    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);
        let mut out = Segments::new();

        let base_style = crate::css::resolve_component_style(self, &["radio-button--label"])
            .to_rich()
            .unwrap_or_else(rich_rs::Style::new);

        for row in 0..height {
            if row >= self.buttons.len() {
                // Empty row padding.
                out.push(Segment::styled(" ".repeat(width), base_style));
            } else {
                let button = &self.buttons[row];
                let is_selected = self.cursor.highlighted() == Some(row);
                let is_pressed = self.cursor.selected() == Some(row);
                let is_hovered_row = self.hovered_index == Some(row);

                let glyph = if is_pressed || button.value() {
                    "●"
                } else {
                    "○"
                };

                // Build component-style classes for the glyph and label.
                let mut glyph_classes = vec!["radio-button--button"];
                let mut label_classes = vec!["radio-button--label"];
                if is_pressed || button.value() {
                    glyph_classes.push("-on");
                    label_classes.push("-on");
                }
                if is_selected && self.focused {
                    glyph_classes.push("-focus");
                    label_classes.push("-focus");
                }
                if is_hovered_row {
                    glyph_classes.push("-hover");
                    label_classes.push("-hover");
                }
                if is_selected {
                    label_classes.push("-selected");
                }

                let glyph_style = crate::css::resolve_component_style(self, &glyph_classes)
                    .to_rich()
                    .unwrap_or_else(rich_rs::Style::new);
                let label_style = crate::css::resolve_component_style(self, &label_classes)
                    .to_rich()
                    .unwrap_or_else(rich_rs::Style::new);

                // Render the half-block framed glyph: "▐●▌ label"
                let glyph_bg = glyph_style.bgcolor;
                let outer_bg = crate::style::parse_color_like("$surface")
                    .unwrap_or(crate::style::Color::rgb(0, 0, 0))
                    .to_simple_opaque();
                let left_style = rich_rs::Style::new()
                    .with_color(glyph_bg.unwrap_or(outer_bg))
                    .with_bgcolor(outer_bg);
                let right_style = left_style;

                let segments = vec![
                    Segment::styled("▐".to_string(), left_style),
                    Segment::styled(glyph.to_string(), glyph_style),
                    Segment::styled("▌".to_string(), right_style),
                    Segment::styled(format!(" {}", button.label()), label_style),
                ];

                let line = adjust_line_length_no_bg(&segments, width);
                out.extend(line);
            }

            if row + 1 < height {
                out.push(Segment::line());
            }
        }

        out
    }

    fn layout_height(&self) -> Option<usize> {
        fixed_height_from_constraints(self.layout_constraints()).or(Some(self.buttons.len().max(1)))
    }

    fn content_width(&self) -> Option<usize> {
        let width = self
            .buttons
            .iter()
            .map(|b| {
                // "▐●▌ " + label = 4 + label width
                rich_rs::cell_len(b.label()).saturating_add(4)
            })
            .max()
            .unwrap_or(4)
            .max(1);
        Some(width)
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
        "RadioSet"
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

impl Renderable for RadioSet {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::KeyEventData;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn radio_set_space_changes_selection_and_emits_message() {
        let mut set = RadioSet::from_labels(&["A", "B", "C"]);
        set.set_focus(true);
        let down = KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        let mut ctx1 = EventCtx::default();
        set.on_event(&Event::Key(down), &mut ctx1);
        assert_eq!(set.selected_index(), 1);

        let space =
            KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        let mut ctx2 = EventCtx::default();
        set.on_event(&Event::Key(space), &mut ctx2);
        assert_eq!(set.pressed_index(), Some(1));
        let messages = ctx2.take_messages();
        assert!(messages
            .iter()
            .any(|m| matches!(m.message, Message::RadioSetChanged { index: 1, .. })));
    }

    #[test]
    fn radio_set_cannot_deselect_active_button() {
        let mut set = RadioSet::new().with_button(RadioButton::new("A").with_value(true));
        set.set_focus(true);
        let space =
            KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        let mut ctx = EventCtx::default();
        set.on_event(&Event::Key(space), &mut ctx);
        assert_eq!(set.pressed_index(), Some(0));
    }
}
