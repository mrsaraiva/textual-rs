use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use rich_rs::{Console, ConsoleOptions, Segments};
use textual::message::MessageEvent;
use textual::prelude::*;

struct FocusProbe {
    focused: Arc<AtomicBool>,
}

impl FocusProbe {
    fn new(focused: Arc<AtomicBool>) -> Self {
        Self { focused }
    }
}

impl Widget for FocusProbe {
    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        Segments::new()
    }

    fn focusable(&self) -> bool {
        true
    }

    fn set_focus(&mut self, focused: bool) {
        self.focused.store(focused, Ordering::Relaxed);
    }

    fn has_focus(&self) -> bool {
        self.focused.load(Ordering::Relaxed)
    }
}

#[test]
fn command_palette_restores_wrapped_focus_after_close() {
    let child_focus = Arc::new(AtomicBool::new(true));
    let child = FocusProbe::new(child_focus.clone());
    let mut palette = CommandPalette::new(child);

    let mut open_ctx = EventCtx::default();
    palette.on_event(&Event::Action(Action::CommandPalette), &mut open_ctx);
    assert!(palette.is_open());
    assert!(!child_focus.load(Ordering::Relaxed));

    let mut close_ctx = EventCtx::default();
    palette.on_event(&Event::Action(Action::CommandPalette), &mut close_ctx);
    assert!(!palette.is_open());
    assert!(child_focus.load(Ordering::Relaxed));
}

#[test]
fn command_palette_closes_when_overlay_visibility_changes() {
    let mut palette = CommandPalette::new(Label::new("body"));
    let mut open_ctx = EventCtx::default();
    palette.on_event(&Event::Action(Action::CommandPalette), &mut open_ctx);
    assert!(palette.is_open());

    let mut transition_ctx = EventCtx::default();
    palette.on_message(
        &MessageEvent {
            sender: NodeId::default(),
            message: Message::OverlayVisibilityChanged {
                overlay: NodeId::default(),
                visible: true,
            },
        },
        &mut transition_ctx,
    );
    assert!(!palette.is_open());

    assert!(transition_ctx.repaint_requested());
}
