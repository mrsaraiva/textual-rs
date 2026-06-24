use std::sync::Arc;

use crossterm::event::KeyCode;
use rich_rs::{Renderable, Text};

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

/// Rich content for an `OptionItem` option row.
///
/// An option can hold either a [`Text`] value (pre-rendered, cheap to clone,
/// line count computable from `plain_text()`) or an arbitrary [`Renderable`]
/// (for tables, diagrams, etc.) that renders live at the runtime widget width.
///
/// Matches Python's `OptionListContent = Option | VisualType | None` model where
/// `VisualType` includes any Rich renderable (tables, panels, …).
pub enum OptionContent {
    /// Rich-text content (pre-formed, line count is `plain_text().split('\n').count()`).
    Text(Text),
    /// Arbitrary renderable (table, panel, …). Rendered live at runtime width.
    /// Height is not known until rendered; `OptionList` re-measures on layout.
    Renderable(Arc<dyn Renderable>),
}

impl Clone for OptionContent {
    fn clone(&self) -> Self {
        match self {
            Self::Text(t) => Self::Text(t.clone()),
            Self::Renderable(r) => Self::Renderable(Arc::clone(r)),
        }
    }
}

impl std::fmt::Debug for OptionContent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Text(t) => f.debug_tuple("Text").field(&t.plain_text()).finish(),
            Self::Renderable(_) => f.write_str("Renderable(..)"),
        }
    }
}

/// Shared option row model used by `OptionList`, `Select`, and `SelectionList`.
///
/// Items can hold either a plain text `prompt` or rich [`OptionContent`].
/// When content is present, it takes precedence over `prompt` during rendering
/// while the plain prompt serves as a fallback / accessibility label.
#[derive(Debug, Clone)]
pub enum OptionItem {
    Option {
        prompt: String,
        /// Rich content (Text or arbitrary Renderable).
        /// When set, takes precedence over `prompt` during rendering.
        content: Option<OptionContent>,
        id: Option<OptionId>,
        disabled: bool,
    },
    Separator,
}

/// Equality considers prompt/id/disabled only — rich `content` is a rendering detail.
impl PartialEq for OptionItem {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::Option {
                    prompt: p1,
                    id: id1,
                    disabled: d1,
                    ..
                },
                Self::Option {
                    prompt: p2,
                    id: id2,
                    disabled: d2,
                    ..
                },
            ) => p1 == p2 && id1 == id2 && d1 == d2,
            (Self::Separator, Self::Separator) => true,
            _ => false,
        }
    }
}

impl Eq for OptionItem {}

impl OptionItem {
    pub fn new(prompt: impl Into<String>) -> Self {
        Self::Option {
            prompt: prompt.into(),
            content: None,
            id: None,
            disabled: false,
        }
    }

    pub fn with_id(prompt: impl Into<String>, id: impl Into<OptionId>) -> Self {
        Self::Option {
            prompt: prompt.into(),
            content: None,
            id: Some(id.into()),
            disabled: false,
        }
    }

    pub fn disabled(prompt: impl Into<String>) -> Self {
        Self::Option {
            prompt: prompt.into(),
            content: None,
            id: None,
            disabled: true,
        }
    }

    pub fn disabled_with_id(prompt: impl Into<String>, id: impl Into<OptionId>) -> Self {
        Self::Option {
            prompt: prompt.into(),
            content: None,
            id: Some(id.into()),
            disabled: true,
        }
    }

    /// Create an option with rich [`Text`] content.
    ///
    /// The `label` is stored as the plain-text fallback; `content` is used for rendering.
    pub fn rich(label: impl Into<String>, content: Text) -> Self {
        Self::Option {
            prompt: label.into(),
            content: Some(OptionContent::Text(content)),
            id: None,
            disabled: false,
        }
    }

    /// Create a rich option with a typed id.
    pub fn rich_with_id(label: impl Into<String>, content: Text, id: impl Into<OptionId>) -> Self {
        Self::Option {
            prompt: label.into(),
            content: Some(OptionContent::Text(content)),
            id: Some(id.into()),
            disabled: false,
        }
    }

    /// Create an option with an arbitrary [`Renderable`] content.
    ///
    /// The renderable is rendered live at the runtime widget width (unlike
    /// `rich()` which uses a fixed-width pre-rendered `Text`). This is the
    /// correct path for `rich_rs::Table` and other dynamic renderables that
    /// need to reflow at the actual content width.
    ///
    /// Mirrors Python `OptionList(*[Option(table)])` where `table` is a Rich
    /// `Table` renderable.
    pub fn renderable(
        label: impl Into<String>,
        renderable: impl Renderable + 'static,
    ) -> Self {
        Self::Option {
            prompt: label.into(),
            content: Some(OptionContent::Renderable(Arc::new(renderable))),
            id: None,
            disabled: false,
        }
    }

    /// Create a renderable option with a typed id.
    pub fn renderable_with_id(
        label: impl Into<String>,
        renderable: impl Renderable + 'static,
        id: impl Into<OptionId>,
    ) -> Self {
        Self::Option {
            prompt: label.into(),
            content: Some(OptionContent::Renderable(Arc::new(renderable))),
            id: Some(id.into()),
            disabled: false,
        }
    }

    /// Builder: attach rich [`Text`] content to this option.
    pub fn with_content(mut self, content: Text) -> Self {
        if let Self::Option {
            content: ref mut c, ..
        } = self
        {
            *c = Some(OptionContent::Text(content));
        }
        self
    }

    /// Builder: attach a renderable to this option.
    ///
    /// The renderable is rendered live at the runtime widget width.
    pub fn with_renderable(mut self, renderable: impl Renderable + 'static) -> Self {
        if let Self::Option {
            content: ref mut c, ..
        } = self
        {
            *c = Some(OptionContent::Renderable(Arc::new(renderable)));
        }
        self
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

    /// Rich content, if any.
    pub fn content(&self) -> Option<&OptionContent> {
        match self {
            Self::Option { content, .. } => content.as_ref(),
            Self::Separator => None,
        }
    }

    /// Access the content as `Text`, if it is a `Text` variant.
    pub fn text_content(&self) -> Option<&Text> {
        match self.content() {
            Some(OptionContent::Text(t)) => Some(t),
            _ => None,
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
            Event::MouseUp(mouse)
                if self.pressed => {
                    self.pressed = false;
                    outcome.repaint = true;
                    if mouse.target == Some(id) {
                        self.toggle();
                        outcome.toggled = true;
                        outcome.handled = true;
                    }
                }
            Event::AppFocus(false)
                if self.pressed => {
                    self.pressed = false;
                    outcome.repaint = true;
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
