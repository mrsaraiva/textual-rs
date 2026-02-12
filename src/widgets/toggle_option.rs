use crossterm::event::KeyCode;

use crate::event::{Action, Event};
use crate::node_id::NodeId;

/// Strongly-typed option identifier used by option-oriented widgets.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OptionId(String);

impl OptionId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for OptionId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for OptionId {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

/// Shared option row model used by `OptionList`, `Select`, and `SelectionList`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OptionItem {
    Option {
        prompt: String,
        id: Option<OptionId>,
        disabled: bool,
    },
    Separator,
}

impl OptionItem {
    pub fn new(prompt: impl Into<String>) -> Self {
        Self::Option {
            prompt: prompt.into(),
            id: None,
            disabled: false,
        }
    }

    pub fn with_id(prompt: impl Into<String>, id: impl Into<OptionId>) -> Self {
        Self::Option {
            prompt: prompt.into(),
            id: Some(id.into()),
            disabled: false,
        }
    }

    pub fn disabled(prompt: impl Into<String>) -> Self {
        Self::Option {
            prompt: prompt.into(),
            id: None,
            disabled: true,
        }
    }

    pub fn disabled_with_id(prompt: impl Into<String>, id: impl Into<OptionId>) -> Self {
        Self::Option {
            prompt: prompt.into(),
            id: Some(id.into()),
            disabled: true,
        }
    }

    pub fn is_separator(&self) -> bool {
        matches!(self, Self::Separator)
    }

    pub fn is_disabled(&self) -> bool {
        matches!(self, Self::Option { disabled: true, .. })
    }

    pub fn is_selectable(&self) -> bool {
        !self.is_separator() && !self.is_disabled()
    }

    pub fn prompt(&self) -> Option<&str> {
        match self {
            Self::Option { prompt, .. } => Some(prompt),
            Self::Separator => None,
        }
    }

    pub fn id(&self) -> Option<&OptionId> {
        match self {
            Self::Option { id, .. } => id.as_ref(),
            Self::Separator => None,
        }
    }

    pub fn string_id(&self) -> Option<&str> {
        self.id().map(OptionId::as_str)
    }
}

/// Shared highlighted/selected cursor state for option-oriented widgets.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OptionCursorState {
    highlighted: Option<usize>,
    selected: Option<usize>,
}

impl OptionCursorState {
    pub fn highlighted(&self) -> Option<usize> {
        self.highlighted
    }

    pub fn set_highlighted(&mut self, highlighted: Option<usize>) {
        self.highlighted = highlighted;
    }

    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    pub fn set_selected(&mut self, selected: Option<usize>) {
        self.selected = selected;
    }

    pub fn clear(&mut self) {
        self.highlighted = None;
        self.selected = None;
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ToggleEventOutcome {
    pub handled: bool,
    pub repaint: bool,
    pub toggled: bool,
}

/// Shared interaction state and input handling for binary toggle widgets.
#[derive(Debug, Clone)]
pub(crate) struct BinaryToggleState {
    value: bool,
    focused: bool,
    hovered: bool,
    pressed: bool,
    disabled: bool,
}

impl BinaryToggleState {
    pub fn new(value: bool) -> Self {
        Self {
            value,
            focused: false,
            hovered: false,
            pressed: false,
            disabled: false,
        }
    }

    pub fn value(&self) -> bool {
        self.value
    }

    pub fn set_value(&mut self, value: bool) {
        self.value = value;
    }

    pub fn focused(&self) -> bool {
        self.focused
    }

    pub fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }

    pub fn hovered(&self) -> bool {
        self.hovered
    }

    pub fn set_hovered(&mut self, hovered: bool) {
        self.hovered = hovered;
    }

    #[cfg(test)]
    pub fn pressed(&self) -> bool {
        self.pressed
    }

    pub fn disabled(&self) -> bool {
        self.disabled
    }

    pub fn set_disabled(&mut self, disabled: bool) {
        self.disabled = disabled;
    }

    pub fn focusable(&self) -> bool {
        !self.disabled
    }

    pub fn is_active(&self) -> bool {
        self.pressed && self.hovered
    }

    pub fn toggle(&mut self) {
        self.value = !self.value;
    }

    pub fn handle_event(&mut self, event: &Event, id: NodeId) -> ToggleEventOutcome {
        let mut outcome = ToggleEventOutcome::default();
        if self.disabled {
            return outcome;
        }

        match event {
            Event::MouseDown(mouse) if mouse.target == id => {
                self.pressed = true;
                outcome.repaint = true;
                outcome.handled = true;
            }
            Event::MouseUp(mouse) => {
                if self.pressed {
                    self.pressed = false;
                    outcome.repaint = true;
                    if mouse.target == Some(id) {
                        self.toggle();
                        outcome.toggled = true;
                        outcome.handled = true;
                    }
                }
            }
            Event::AppFocus(false) => {
                if self.pressed {
                    self.pressed = false;
                    outcome.repaint = true;
                }
            }
            Event::Action(Action::Toggle) if self.focused => {
                self.toggle();
                outcome.toggled = true;
                outcome.repaint = true;
                outcome.handled = true;
            }
            Event::Key(key) if self.focused => match key.code {
                KeyCode::Enter | KeyCode::Char(' ') => {
                    self.toggle();
                    outcome.toggled = true;
                    outcome.repaint = true;
                    outcome.handled = true;
                }
                _ => {}
            },
            _ => {}
        }

        outcome
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{MouseDownEvent, MouseUpEvent};
    use crate::keys::KeyEventData;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn option_item_exposes_typed_id() {
        let item = OptionItem::with_id("Alpha", OptionId::new("alpha"));
        assert_eq!(item.string_id(), Some("alpha"));
        assert!(item.is_selectable());
    }

    #[test]
    fn option_cursor_keeps_highlight_and_selection_separate() {
        let mut cursor = OptionCursorState::default();
        cursor.set_highlighted(Some(1));
        cursor.set_selected(Some(4));
        assert_eq!(cursor.highlighted(), Some(1));
        assert_eq!(cursor.selected(), Some(4));
        cursor.clear();
        assert_eq!(cursor.highlighted(), None);
        assert_eq!(cursor.selected(), None);
    }

    #[test]
    fn binary_toggle_handles_pointer_and_keyboard_activation() {
        let id = NodeId::default();
        let mut state = BinaryToggleState::new(false);
        state.set_focused(true);
        state.set_hovered(true);

        let down = state.handle_event(
            &Event::MouseDown(MouseDownEvent {
                target: id,
                screen_x: 0,
                screen_y: 0,
                x: 0,
                y: 0,
            }),
            id,
        );
        assert!(down.handled);
        assert!(down.repaint);
        assert!(state.pressed());
        assert!(!state.value());

        let up = state.handle_event(
            &Event::MouseUp(MouseUpEvent {
                target: Some(id),
                screen_x: 0,
                screen_y: 0,
                x: 0,
                y: 0,
            }),
            id,
        );
        assert!(up.handled);
        assert!(up.repaint);
        assert!(up.toggled);
        assert!(state.value());

        let key =
            KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        let keyboard = state.handle_event(&Event::Key(key), id);
        assert!(keyboard.toggled);
        assert!(!state.value());
    }
}
