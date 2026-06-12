use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub(super) struct InputChrome {
    focused: bool,
    mouse_down: bool,
    cursor_visible: bool,
    cursor_blink_next_at: Option<Instant>,
    app_active: bool,
}

impl InputChrome {
    const CURSOR_BLINK_PERIOD: Duration = Duration::from_millis(500);

    pub(super) fn new() -> Self {
        Self {
            focused: false,
            mouse_down: false,
            cursor_visible: false,
            cursor_blink_next_at: None,
            app_active: true,
        }
    }

    fn next_blink_deadline() -> Instant {
        let now = Instant::now();
        now.checked_add(Self::CURSOR_BLINK_PERIOD).unwrap_or(now)
    }

    pub(super) fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
        if !focused {
            self.mouse_down = false;
            self.cursor_visible = false;
            self.cursor_blink_next_at = None;
            return;
        }
        self.reset_blink();
    }

    pub(super) fn set_mouse_down(&mut self, down: bool) {
        self.mouse_down = down;
    }

    pub(super) fn is_mouse_down(&self) -> bool {
        self.mouse_down
    }

    pub(super) fn is_active(&self) -> bool {
        self.mouse_down
    }

    pub(super) fn cursor_visible(&self) -> bool {
        self.cursor_visible
    }

    pub(super) fn reset_blink(&mut self) {
        if !self.focused || !self.app_active {
            return;
        }
        self.cursor_visible = true;
        self.cursor_blink_next_at = Some(Self::next_blink_deadline());
    }

    pub(super) fn handle_app_focus(&mut self, active: bool) {
        self.app_active = active;
        if !active {
            self.cursor_visible = false;
            self.cursor_blink_next_at = None;
            return;
        }
        self.reset_blink();
    }

    pub(super) fn handle_tick(&mut self, now: Instant) -> bool {
        if !self.focused || !self.app_active {
            return false;
        }
        let Some(next_at) = self.cursor_blink_next_at else {
            return false;
        };
        if now < next_at {
            return false;
        }
        self.cursor_visible = !self.cursor_visible;
        self.cursor_blink_next_at = now.checked_add(Self::CURSOR_BLINK_PERIOD).or(Some(now));
        true
    }
}
