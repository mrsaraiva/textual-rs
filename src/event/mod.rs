use crate::debug::debug_message;
use crate::keys::KeyEventData;
use crate::message::{Message, MessageEvent};
use crate::widgets::WidgetId;
use crossterm::event::{KeyCode, KeyModifiers};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseDownEvent {
    pub target: WidgetId,
    pub screen_x: u16,
    pub screen_y: u16,
    /// Content-local coordinates (origin at widget content top-left).
    pub x: u16,
    pub y: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseUpEvent {
    pub target: Option<WidgetId>,
    pub screen_x: u16,
    pub screen_y: u16,
    /// Content-local coordinates (origin at widget content top-left of `target`, if any).
    pub x: u16,
    pub y: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseScrollEvent {
    pub target: Option<WidgetId>,
    pub screen_x: u16,
    pub screen_y: u16,
    /// Content-local coordinates (origin at widget content top-left of `target`, if any).
    pub x: u16,
    pub y: u16,
    pub delta_x: i32,
    pub delta_y: i32,
    pub modifiers: KeyModifiers,
}

#[derive(Debug, Clone)]
pub enum Event {
    Key(KeyEventData),
    Action(Action),
    MouseDown(MouseDownEvent),
    MouseUp(MouseUpEvent),
    MouseScroll(MouseScrollEvent),
    AppFocus(bool),
    Tick(u64),
    Resize(u16, u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    FocusNext,
    FocusPrev,
    ScrollUp,
    ScrollDown,
    ScrollPageUp,
    ScrollPageDown,
    ScrollLeft,
    ScrollRight,
    ScrollPageLeft,
    ScrollPageRight,
    Toggle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyBind {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}

impl KeyBind {
    pub fn new(code: KeyCode, modifiers: KeyModifiers) -> Self {
        Self { code, modifiers }
    }

    pub fn from_event(key: &KeyEventData) -> Self {
        Self {
            code: key.code,
            modifiers: key.modifiers,
        }
    }
}

#[derive(Debug, Default)]
pub struct ActionMap {
    bindings: HashMap<KeyBind, Action>,
}

impl ActionMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bind(&mut self, key: KeyBind, action: Action) {
        self.bindings.insert(key, action);
    }

    pub fn lookup(&self, key: &KeyBind) -> Option<Action> {
        self.bindings.get(key).copied()
    }
}

#[derive(Debug, Default)]
pub struct EventCtx {
    handled: bool,
    repaint_requested: bool,
    messages: Vec<MessageEvent>,
}

impl EventCtx {
    pub fn handled(&self) -> bool {
        self.handled
    }

    pub fn set_handled(&mut self) {
        self.handled = true;
    }

    /// Request a repaint after this event dispatch finishes.
    ///
    /// This is useful when a widget updates visual state but does not (or should not)
    /// mark the event as handled.
    pub fn request_repaint(&mut self) {
        self.repaint_requested = true;
    }

    pub fn repaint_requested(&self) -> bool {
        self.repaint_requested
    }

    pub fn post_message(&mut self, sender: WidgetId, message: Message) {
        debug_message(&format!(
            "[post_message] sender={} payload={message:?}",
            sender.as_u64()
        ));
        self.messages.push(MessageEvent { sender, message });
    }

    pub(crate) fn merge_from(&mut self, mut other: EventCtx) {
        if other.handled {
            self.handled = true;
        }
        if other.repaint_requested {
            self.repaint_requested = true;
        }
        self.messages.append(&mut other.messages);
    }

    pub(crate) fn take_messages(&mut self) -> Vec<MessageEvent> {
        std::mem::take(&mut self.messages)
    }
}
