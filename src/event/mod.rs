use crossterm::event::KeyEvent;

#[derive(Debug, Clone)]
pub enum Event {
    Key(KeyEvent),
    Action(Action),
    Tick(u64),
    Resize(u16, u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    FocusNext,
    FocusPrev,
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
