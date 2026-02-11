use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use rich_rs::{Console, ConsoleOptions, Segment, Segments, Style};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use textual::css::set_style_context;
use textual::event::MouseScrollEvent;
use textual::message::MessageEvent;
use textual::prelude::*;
use textual::render::FrameBuffer;

fn options_for(console: &Console, width: usize, height: usize) -> rich_rs::ConsoleOptions {
    let mut options = console.options().clone();
    options.size = (width, height);
    options.max_width = width;
    options.max_height = height;
    options
}

struct SpyWidget {
    id: WidgetId,
    dismiss_messages: Arc<AtomicUsize>,
    mouse_moves: Arc<AtomicUsize>,
    mouse_scrolls: Arc<AtomicUsize>,
}

impl SpyWidget {
    fn new(
        dismiss_messages: Arc<AtomicUsize>,
        mouse_moves: Arc<AtomicUsize>,
        mouse_scrolls: Arc<AtomicUsize>,
    ) -> Self {
        Self {
            id: WidgetId::new(),
            dismiss_messages,
            mouse_moves,
            mouse_scrolls,
        }
    }
}

impl Widget for SpyWidget {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        vec![Segment::styled("spy", Style::new())].into()
    }

    fn on_message(&mut self, message: &MessageEvent, _ctx: &mut EventCtx) {
        if matches!(message.message, Message::OverlayDismissRequested { .. }) {
            self.dismiss_messages.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn on_mouse_move(&mut self, _x: u16, _y: u16) -> bool {
        self.mouse_moves.fetch_add(1, Ordering::Relaxed);
        true
    }

    fn on_mouse_scroll(&mut self, _delta_x: i32, _delta_y: i32, _ctx: &mut EventCtx) {
        self.mouse_scrolls.fetch_add(1, Ordering::Relaxed);
    }
}

#[test]
fn tooltip_renders_overlay_text_when_visible() {
    let console = Console::new();
    let options = options_for(&console, 30, 6);
    let tooltip = Tooltip::new(Label::new("base content"), "Tooltip text").visible(true);

    let buf = FrameBuffer::from_renderable(&console, &options, &tooltip, None);
    let lines = buf.as_plain_lines();

    assert!(lines.iter().any(|line| line.contains("Tooltip text")));
    assert!(lines.iter().any(|line| line.contains("base content")));
}

#[test]
fn tooltip_hides_on_escape_key() {
    let console = Console::new();
    let options = options_for(&console, 30, 6);
    let mut tooltip = Tooltip::new(Label::new("base"), "tip").visible(true);

    let key = KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    tooltip.on_event(&Event::Key(key), &mut EventCtx::default());

    let buf = FrameBuffer::from_renderable(&console, &options, &tooltip, None);
    let lines = buf.as_plain_lines();
    assert!(lines.iter().all(|line| !line.contains("tip")));
    assert!(lines.iter().any(|line| line.contains("base")));
}

#[test]
fn tooltip_visibility_can_be_driven_via_overlay_messages() {
    let console = Console::new();
    let options = options_for(&console, 30, 6);
    let mut tooltip = Tooltip::new(Label::new("base"), "tip").visible(true);

    tooltip.on_message(
        &MessageEvent {
            sender: WidgetId::new(),
            message: Message::OverlaySetVisible {
                overlay: tooltip.id(),
                visible: false,
            },
        },
        &mut EventCtx::default(),
    );

    let buf = FrameBuffer::from_renderable(&console, &options, &tooltip, None);
    let lines = buf.as_plain_lines();
    assert!(lines.iter().all(|line| !line.contains("tip")));
}

#[test]
fn tooltip_positions_above_anchor_when_bottom_space_is_insufficient() {
    let console = Console::new();
    let options = options_for(&console, 28, 6);
    let tooltip = Tooltip::new(Label::new("base"), "anchored")
        .visible(true)
        .with_anchor(14, 5);

    let buf = FrameBuffer::from_renderable(&console, &options, &tooltip, None);
    let lines = buf.as_plain_lines();
    let line_idx = lines
        .iter()
        .position(|line| line.contains("anchored"))
        .expect("tooltip line");

    assert_eq!(line_idx, 2);
}

#[test]
fn tooltip_clamps_horizontally_when_anchor_is_left_of_viewport() {
    let console = Console::new();
    let options = options_for(&console, 20, 6);
    let tooltip = Tooltip::new(Label::new("base"), "left-edge")
        .visible(true)
        .with_anchor(0, 1);

    let buf = FrameBuffer::from_renderable(&console, &options, &tooltip, None);
    let lines = buf.as_plain_lines();
    let line = lines
        .iter()
        .find(|line| line.contains("left-edge"))
        .expect("tooltip line");
    let x = line.find("left-edge").expect("x position");

    assert_eq!(x, 2);
}

#[test]
fn tooltip_updates_anchor_from_runtime_mouse_events() {
    let console = Console::new();
    let options = options_for(&console, 30, 8);
    let mut tooltip = Tooltip::new(Label::new("base"), "tip")
        .visible(true)
        .with_anchor(2, 0);

    let before = FrameBuffer::from_renderable(&console, &options, &tooltip, None);
    let before_lines = before.as_plain_lines();
    let before_line = before_lines
        .iter()
        .find(|line| line.contains("tip"))
        .expect("tip line before");
    let before_x = before_line.find("tip").expect("tip x before");

    let target = tooltip.anchor_target_id();
    tooltip.on_event(
        &Event::MouseScroll(MouseScrollEvent {
            target: Some(target),
            screen_x: 22,
            screen_y: 1,
            x: 22,
            y: 1,
            delta_x: 0,
            delta_y: 1,
            modifiers: KeyModifiers::empty(),
        }),
        &mut EventCtx::default(),
    );

    let after = FrameBuffer::from_renderable(&console, &options, &tooltip, None);
    let after_lines = after.as_plain_lines();
    let after_line = after_lines
        .iter()
        .find(|line| line.contains("tip"))
        .expect("tip line after");
    let after_x = after_line.find("tip").expect("tip x after");

    assert!(after_x > before_x);
}

#[test]
fn tooltip_anchor_can_be_driven_by_overlay_anchor_messages() {
    let console = Console::new();
    let options = options_for(&console, 30, 8);
    let mut tooltip = Tooltip::new(Label::new("base"), "tip")
        .visible(true)
        .with_anchor(2, 0);

    tooltip.on_message(
        &MessageEvent {
            sender: WidgetId::new(),
            message: Message::OverlaySetAnchor {
                overlay: tooltip.id(),
                x: 22,
                y: 1,
            },
        },
        &mut EventCtx::default(),
    );

    let after_set = FrameBuffer::from_renderable(&console, &options, &tooltip, None);
    let set_lines = after_set.as_plain_lines();
    let set_line = set_lines
        .iter()
        .find(|line| line.contains("tip"))
        .expect("tip line after set-anchor");
    let set_x = set_line.find("tip").expect("tip x after set-anchor");

    tooltip.on_message(
        &MessageEvent {
            sender: WidgetId::new(),
            message: Message::OverlayClearAnchor {
                overlay: tooltip.id(),
            },
        },
        &mut EventCtx::default(),
    );

    let after_clear = FrameBuffer::from_renderable(&console, &options, &tooltip, None);
    let clear_lines = after_clear.as_plain_lines();
    let clear_line = clear_lines
        .iter()
        .find(|line| line.contains("tip"))
        .expect("tip line after clear-anchor");
    let clear_x = clear_line.find("tip").expect("tip x after clear-anchor");

    assert!(set_x > clear_x);
}

#[test]
fn tooltip_forwards_non_matching_overlay_dismiss_to_child() {
    let dismiss_messages = Arc::new(AtomicUsize::new(0));
    let mouse_moves = Arc::new(AtomicUsize::new(0));
    let mouse_scrolls = Arc::new(AtomicUsize::new(0));
    let spy = SpyWidget::new(Arc::clone(&dismiss_messages), mouse_moves, mouse_scrolls);
    let mut tooltip = Tooltip::new(spy, "tip").visible(true);

    tooltip.on_message(
        &MessageEvent {
            sender: WidgetId::new(),
            message: Message::OverlayDismissRequested {
                overlay: Some(WidgetId::new()),
            },
        },
        &mut EventCtx::default(),
    );

    assert_eq!(dismiss_messages.load(Ordering::Relaxed), 1);
}

#[test]
fn tooltip_delegates_mouse_hooks_to_child() {
    let dismiss_messages = Arc::new(AtomicUsize::new(0));
    let mouse_moves = Arc::new(AtomicUsize::new(0));
    let mouse_scrolls = Arc::new(AtomicUsize::new(0));
    let spy = SpyWidget::new(
        dismiss_messages,
        Arc::clone(&mouse_moves),
        Arc::clone(&mouse_scrolls),
    );
    let mut tooltip = Tooltip::new(spy, "tip").visible(true);

    assert!(tooltip.on_mouse_move(3, 2));
    let mut ctx = EventCtx::default();
    tooltip.on_mouse_scroll(0, 1, &mut ctx);

    assert_eq!(mouse_moves.load(Ordering::Relaxed), 1);
    assert_eq!(mouse_scrolls.load(Ordering::Relaxed), 1);
}

#[test]
fn tooltip_unmount_resets_visibility_and_anchor_state() {
    let console = Console::new();
    let options = options_for(&console, 30, 8);
    let mut tooltip = Tooltip::new(Label::new("base"), "tip")
        .visible(true)
        .with_anchor(22, 1);

    let before = FrameBuffer::from_renderable(&console, &options, &tooltip, None);
    let before_lines = before.as_plain_lines();
    let before_line = before_lines
        .iter()
        .find(|line| line.contains("tip"))
        .expect("tip line before unmount");
    let anchored_x = before_line.find("tip").expect("anchored x");

    tooltip.on_unmount();
    tooltip.on_message(
        &MessageEvent {
            sender: WidgetId::new(),
            message: Message::OverlaySetVisible {
                overlay: tooltip.id(),
                visible: true,
            },
        },
        &mut EventCtx::default(),
    );

    let after = FrameBuffer::from_renderable(&console, &options, &tooltip, None);
    let after_lines = after.as_plain_lines();
    let after_line = after_lines
        .iter()
        .find(|line| line.contains("tip"))
        .expect("tip line after remount lifecycle");
    let reset_x = after_line.find("tip").expect("reset x");

    assert!(anchored_x > reset_x);
}

#[test]
fn tooltip_accepts_vkey_border_styles_from_css() {
    let console = Console::new();
    let options = options_for(&console, 20, 4);
    let _guard = set_style_context(StyleSheet::parse(
        "Tooltip { border-left: vkey $foreground; }",
    ));
    let tooltip = Tooltip::new(Label::new("base"), "tip").visible(true);
    let renderable = WidgetRenderable::new(&tooltip);
    let buf = FrameBuffer::from_renderable(&console, &options, &renderable, None);
    let lines = buf.as_plain_lines();

    assert!(
        lines.iter().any(|line| line.starts_with('\u{258f}')),
        "expected vkey left border glyph on tooltip, got {lines:?}"
    );
}
