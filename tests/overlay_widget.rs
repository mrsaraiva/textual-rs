use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use rich_rs::Console;
use rich_rs::{ConsoleOptions, Segment, Segments};
use textual::event::MouseDownEvent;
use textual::message::MessageEvent;
use textual::prelude::*;
use textual::render::FrameBuffer;

struct EventProbe {
    events: Arc<AtomicUsize>,
}

impl EventProbe {
    fn new(events: Arc<AtomicUsize>) -> Self {
        Self { events }
    }
}

impl Widget for EventProbe {
    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let mut out = Segments::new();
        out.push(Segment::new(" ".repeat(options.size.0.max(1))));
        out
    }

    fn on_event(&mut self, _event: &Event, _ctx: &mut EventCtx) {
        self.events.fetch_add(1, Ordering::Relaxed);
    }
}

#[test]
fn overlay_shows_modal_over_base() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (12, 3);
    options.max_width = 12;
    options.max_height = 3;

    let base = Label::new("base content");
    let modal = Frame::new(Label::new("modal"));
    let overlay = Overlay::new(base, modal);

    let buf = FrameBuffer::from_renderable(&console, &options, &overlay, None);
    insta::assert_snapshot!(buf.debug_dump());
}

#[test]
fn overlay_traps_base_events_when_visible() {
    let base_events = Arc::new(AtomicUsize::new(0));
    let base = EventProbe::new(base_events.clone());
    let modal = Label::new("modal");
    let mut overlay = Overlay::new(base, modal);

    let key = KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
    let mut ctx = EventCtx::default();
    overlay.on_event(&Event::Key(key), &mut ctx);

    assert_eq!(base_events.load(Ordering::Relaxed), 0);
}

#[test]
fn overlay_escape_hides_modal() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (12, 3);
    options.max_width = 12;
    options.max_height = 3;

    let base = Label::new("base");
    let modal = Label::new("modal");
    let mut overlay = Overlay::new(base, modal);

    let key = KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    let mut ctx = EventCtx::default();
    overlay.on_event(&Event::Key(key), &mut ctx);

    let buf = FrameBuffer::from_renderable(&console, &options, &overlay, None);
    assert!(buf.debug_dump().contains("base"));
}

#[test]
fn overlay_dismiss_message_hides_modal() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (12, 3);
    options.max_width = 12;
    options.max_height = 3;

    let base = Label::new("base");
    let modal = Label::new("modal");
    let mut overlay = Overlay::new(base, modal);

    let mut ctx = EventCtx::default();
    overlay.on_message(
        &MessageEvent {
            sender: NodeId::default(),
            message: Message::OverlayDismissRequested(OverlayDismissRequested { overlay: None }),
            control: None,
        },
        &mut ctx,
    );

    let buf = FrameBuffer::from_renderable(&console, &options, &overlay, None);
    assert!(buf.debug_dump().contains("base"));
}

#[test]
fn toast_with_zero_timeout_dismisses_on_first_tick() {
    let mut toast = Toast::new("hello", ToastSeverity::Information).with_timeout(0);
    let mut ctx = EventCtx::default();
    toast.on_event(&Event::Tick(1), &mut ctx);

    assert!(ctx.handled());
    assert!(ctx.repaint_requested());
}

#[test]
fn toast_click_dismisses_and_posts_message() {
    let mut toast = Toast::new("click me", ToastSeverity::Warning);
    let mut ctx = EventCtx::default();
    toast.on_event(
        &Event::MouseDown(MouseDownEvent {
            target: NodeId::default(),
            screen_x: 0,
            screen_y: 0,
            x: 0,
            y: 0,
        }),
        &mut ctx,
    );

    assert!(ctx.handled());
    assert!(ctx.repaint_requested());
}
