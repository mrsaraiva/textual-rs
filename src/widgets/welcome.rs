use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::event::{Event, EventCtx, MouseDownEvent, MouseUpEvent};
use crate::message::*;
use crate::render::FrameBuffer;

use crate::node_id::NodeId;
use crate::runtime::dispatch_ctx::set_dispatch_recipient;
use crate::widgets::NodeState;

use super::{Button, ButtonVariant, Markdown, NodeSeed, Widget};

const WELCOME_MD: &str = r#"# Welcome!

Textual is a TUI, or *Text User Interface*, framework for Python inspired by modern web development. **We hope you enjoy using Textual!**

## Dune quote

> "I must not fear.
Fear is the mind-killer.
Fear is the little-death that brings total obliteration.
I will face my fear.
I will permit it to pass over me and through me.
And when it has gone past, I will turn the inner eye to see its path.
Where the fear has gone there will be nothing. Only I will remain.""#;

#[derive(Clone)]
pub struct Welcome {
    markdown: Markdown,
    close: Button,
    last_width: u16,
    last_height: u16,
    seed: NodeSeed,
}

impl std::fmt::Debug for Welcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Welcome")
            .field("last_width", &self.last_width)
            .field("last_height", &self.last_height)
            .finish()
    }
}

impl Default for Welcome {
    fn default() -> Self {
        Self::new()
    }
}

impl Welcome {
    pub fn new() -> Self {
        let mut seed = NodeSeed::default();
        seed.classes.push("welcome".to_string());
        Self {
            markdown: Markdown::new(WELCOME_MD),
            close: Button::new("OK").variant(ButtonVariant::Success),
            last_width: 1,
            last_height: 1,
            seed,
        }
    }

    pub fn markdown(&self) -> &str {
        WELCOME_MD
    }

    pub fn close_button_id(&self) -> NodeId {
        self.node_id()
    }

    fn body_height(&self) -> u16 {
        self.last_height.saturating_sub(1).max(1)
    }

    fn translate_mouse_down(&self, mouse: MouseDownEvent) -> Event {
        let on_close_row = self.last_height <= 1 || mouse.y + 1 >= self.last_height;
        if mouse.target == self.node_id() && on_close_row {
            Event::MouseDown(MouseDownEvent {
                target: self.node_id(),
                screen_x: mouse.screen_x,
                screen_y: mouse.screen_y,
                x: mouse.x,
                y: 0,
            })
        } else {
            Event::MouseDown(mouse)
        }
    }

    fn translate_mouse_up(&self, mouse: MouseUpEvent) -> Event {
        let on_close_row = self.last_height <= 1 || mouse.y + 1 >= self.last_height;
        if mouse.target.is_some_and(|t| t == self.node_id()) && on_close_row {
            Event::MouseUp(MouseUpEvent {
                target: Some(self.node_id()),
                screen_x: mouse.screen_x,
                screen_y: mouse.screen_y,
                x: mouse.x,
                y: 0,
            })
        } else {
            Event::MouseUp(mouse)
        }
    }
}

impl Widget for Welcome {
    fn focusable(&self) -> bool {
        true
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.last_width = width.max(1);
        self.last_height = height.max(1);
        let body_height = self.body_height();
        self.markdown.on_layout(self.last_width, body_height);
        self.close.on_layout(self.last_width, 1);
    }

    fn on_mount(&mut self) {
        self.markdown.on_mount();
        self.close.on_mount();
    }

    fn on_unmount(&mut self) {
        self.markdown.on_unmount();
        self.close.on_unmount();
    }

    fn on_tick(&mut self, tick: u64) {
        self.markdown.on_tick(tick);
        self.close.on_tick(tick);
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        self.last_width = width.max(1);
        self.last_height = height.max(1);
        let body_height = self.body_height();
        self.markdown.on_resize(self.last_width, body_height);
        self.close.on_resize(self.last_width, 1);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.markdown.on_event_capture(event, ctx);
        if !ctx.handled() {
            self.close.on_event_capture(event, ctx);
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        let focused = self.node_state().focused;
        match event {
            Event::MouseDown(mouse) => {
                let translated = self.translate_mouse_down(*mouse);
                self.close.on_event(&translated, ctx);
            }
            Event::MouseUp(mouse) => {
                let translated = self.translate_mouse_up(*mouse);
                self.close.on_event(&translated, ctx);
            }
            Event::Action(_) | Event::Key(_) if focused => {
                // Welcome acts as a proxy for the close button when focused.
                // Temporarily promote the close button to focused via dispatch context
                // so it handles keyboard events correctly (Button.on_event checks node_state().focused).
                let _guard = set_dispatch_recipient(
                    NodeId::default(),
                    NodeState {
                        focused: true,
                        ..Default::default()
                    },
                );
                self.close.on_event(event, ctx);
            }
            _ => {}
        }
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut EventCtx) {
        if message.sender != self.node_id() {
            self.markdown.on_message(message, ctx);
            if !ctx.handled() {
                self.close.on_message(message, ctx);
            }
            return;
        }

        if message.is::<ButtonPressed>() {
            ctx.post_message(ButtonPressed {
                description: "Welcome.close".to_string(),
                button_id: None,
            });
            ctx.post_message(OverlayDismissRequested { overlay: None });
            ctx.set_handled();
        }
    }

    fn on_mouse_move(&mut self, x: u16, y: u16) -> bool {
        if self.last_height > 1 && y + 1 >= self.last_height {
            self.close.on_mouse_move(x, 0)
        } else {
            false
        }
    }

    fn on_mouse_scroll(&mut self, delta_x: i32, delta_y: i32, ctx: &mut EventCtx) {
        self.close.on_mouse_scroll(delta_x, delta_y, ctx);
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);

        if height == 1 {
            let mut button_options = options.clone();
            button_options.size = (width, 1);
            button_options.max_width = width;
            button_options.max_height = 1;
            return self.close.render_styled(console, &button_options);
        }

        let body_height = height - 1;

        let mut body_options = options.clone();
        body_options.size = (width, body_height);
        body_options.max_width = width;
        body_options.max_height = body_height;
        // Render the welcome body through the rich-rs markdown renderer directly. The
        // textual `Markdown` widget is compose-only (its `render()` returns empty segments;
        // content is rendered by the tree engine via composed children), so `render_styled`
        // here yields nothing. `self.markdown` is retained for layout/lifecycle/messages.
        let body_segments =
            rich_rs::markdown::Markdown::new(WELCOME_MD.to_string()).render(console, &body_options);
        let body_lines = Segment::split_and_crop_lines(body_segments, width, None, true, false);
        let body_buf = FrameBuffer::from_lines(&body_lines, width, body_height, None);

        let mut button_options = options.clone();
        button_options.size = (width, 1);
        button_options.max_width = width;
        button_options.max_height = 1;
        let button_segments = self.close.render_styled(console, &button_options);
        let button_lines = Segment::split_and_crop_lines(button_segments, width, None, true, false);
        let button_buf = FrameBuffer::from_lines(&button_lines, width, 1, None);

        let mut merged = FrameBuffer::new(width, height, None);
        for y in 0..body_height {
            for x in 0..width {
                merged.set_cell(x, y, body_buf.get(x, y).clone());
            }
        }
        for x in 0..width {
            merged.set_cell(x, body_height, button_buf.get(x, 0).clone());
        }

        merged.to_segments()
    }

    fn layout_height(&self) -> Option<usize> {
        None
    }

    fn content_width(&self) -> Option<usize> {
        Some(rich_rs::cell_len("Welcome!").max(8))
    }

    fn style_type(&self) -> &'static str {
        "Welcome"
    }

    fn set_inline_style(&mut self, style: crate::style::Style) {
        self.seed.styles.style = style;
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }
}

impl Renderable for Welcome {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn welcome_close_re_emits_button_press_and_overlay_dismiss() {
        let mut welcome = Welcome::new();
        welcome.on_layout(48, 10);

        let mut ctx = EventCtx::default();
        welcome.on_message(
            &MessageEvent::new(
                welcome.close_button_id(),
                ButtonPressed {
                    description: "Button(classes='button', variant='success')".to_string(),
                    button_id: None,
                },
            ),
            &mut ctx,
        );

        assert!(ctx.handled());
        let emitted = ctx.take_messages();
        assert!(emitted.iter().any(|event| {
            event
                .downcast_ref::<ButtonPressed>()
                .is_some_and(|bp| bp.description == "Welcome.close")
        }));
        assert!(emitted.iter().any(|event| {
            event
                .downcast_ref::<OverlayDismissRequested>()
                .is_some_and(|odr| odr.overlay.is_none())
        }));
    }
}
