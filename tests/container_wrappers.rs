use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use rich_rs::{Console, ConsoleOptions, Segment, Segments};
use textual::message::MessageEvent;
use textual::event::EventCtx;
use textual::prelude::*;

#[derive(Clone)]
struct ProbeHandles {
    layout_calls: Arc<AtomicUsize>,
    message_calls: Arc<AtomicUsize>,
    scroll_calls: Arc<AtomicUsize>,
    last_width: Arc<AtomicUsize>,
    last_height: Arc<AtomicUsize>,
}

impl ProbeHandles {
    fn new() -> Self {
        Self {
            layout_calls: Arc::new(AtomicUsize::new(0)),
            message_calls: Arc::new(AtomicUsize::new(0)),
            scroll_calls: Arc::new(AtomicUsize::new(0)),
            last_width: Arc::new(AtomicUsize::new(0)),
            last_height: Arc::new(AtomicUsize::new(0)),
        }
    }
}

struct ProbeWidget {
    handles: ProbeHandles,
}

impl ProbeWidget {
    fn new(handles: ProbeHandles) -> Self {
        Self { handles }
    }
}

impl Widget for ProbeWidget {
    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let mut out = Segments::new();
        out.push(Segment::new(" ".repeat(options.size.0.max(1))));
        out
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.handles.layout_calls.fetch_add(1, Ordering::Relaxed);
        self.handles
            .last_width
            .store(width as usize, Ordering::Relaxed);
        self.handles
            .last_height
            .store(height as usize, Ordering::Relaxed);
    }

    fn on_message(&mut self, _message: &MessageEvent, _ctx: &mut textual::event::WidgetCtx) {
        self.handles.message_calls.fetch_add(1, Ordering::Relaxed);
    }

    fn on_mouse_scroll(&mut self, _delta_x: i32, _delta_y: i32, _ctx: &mut textual::event::WidgetCtx) {
        self.handles.scroll_calls.fetch_add(1, Ordering::Relaxed);
    }
}

#[test]
fn panel_forwards_layout_and_messages() {
    let handles = ProbeHandles::new();
    let mut panel = Panel::new(ProbeWidget::new(handles.clone()))
        .padding(1)
        .border(true);

    panel.on_layout(20, 10);
    assert_eq!(handles.layout_calls.load(Ordering::Relaxed), 1);
    assert_eq!(handles.last_width.load(Ordering::Relaxed), 16);
    assert_eq!(handles.last_height.load(Ordering::Relaxed), 6);

    let message = MessageEvent::new(NodeId::default(), ClearRequested);
    let mut ctx = EventCtx::default();
    { let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut ctx); panel.on_message(&message, &mut __w) };
    assert_eq!(handles.message_calls.load(Ordering::Relaxed), 1);
}

#[test]
fn frame_forwards_layout_messages_and_scroll() {
    let handles = ProbeHandles::new();
    let mut frame = Frame::new(ProbeWidget::new(handles.clone()))
        .padding(1)
        .border(true);

    frame.on_layout(20, 10);
    assert_eq!(handles.layout_calls.load(Ordering::Relaxed), 1);
    assert_eq!(handles.last_width.load(Ordering::Relaxed), 16);
    assert_eq!(handles.last_height.load(Ordering::Relaxed), 6);

    let message = MessageEvent::new(NodeId::default(), ClearRequested);
    let mut ctx = EventCtx::default();
    { let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut ctx); frame.on_message(&message, &mut __w) };
    assert_eq!(handles.message_calls.load(Ordering::Relaxed), 1);

    { let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut ctx); frame.on_mouse_scroll(0, 1, &mut __w) };
    assert_eq!(handles.scroll_calls.load(Ordering::Relaxed), 1);
}
