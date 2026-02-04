use crate::widget::WidgetId;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub enum Event {
    Key(KeyEvent),
    Action(Action),
    MouseDown(WidgetId),
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

    pub fn from_event(key: &KeyEvent) -> Self {
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
}

impl EventCtx {
    pub fn handled(&self) -> bool {
        self.handled
    }

    pub fn set_handled(&mut self) {
        self.handled = true;
    }
}
