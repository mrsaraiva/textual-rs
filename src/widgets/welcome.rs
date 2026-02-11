use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::event::{Event, EventCtx, MouseDownEvent, MouseUpEvent};
use crate::message::{Message, MessageEvent};
use crate::render::FrameBuffer;

use super::{
    Button, ButtonVariant, Markdown, Widget, WidgetId, WidgetStyles,
    helpers::{empty_classes, fixed_height_from_constraints},
};

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
    id: WidgetId,
    markdown: Markdown,
    close: Button,
    focused: bool,
    hovered: bool,
    last_width: u16,
    last_height: u16,
    classes: Vec<String>,
    focused_classes: Vec<String>,
    styles: WidgetStyles,
}

impl std::fmt::Debug for Welcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Welcome")
            .field("id", &self.id)
            .field("focused", &self.focused)
            .field("hovered", &self.hovered)
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
        Self {
            id: WidgetId::new(),
            markdown: Markdown::new(WELCOME_MD),
            close: Button::new("OK").variant(ButtonVariant::Success),
            focused: false,
            hovered: false,
            last_width: 1,
            last_height: 1,
            classes: vec!["welcome".to_string()],
            focused_classes: vec!["welcome".to_string(), "focused".to_string()],
            styles: WidgetStyles::default(),
        }
    }

    pub fn markdown(&self) -> &str {
        WELCOME_MD
    }

    pub fn close_button_id(&self) -> WidgetId {
        self.close.id()
    }

    fn body_height(&self) -> u16 {
        self.last_height.saturating_sub(1).max(1)
    }

    fn translate_mouse_down(&self, mouse: MouseDownEvent) -> Event {
        let on_close_row = self.last_height <= 1 || mouse.y + 1 >= self.last_height;
        if mouse.target == self.id && on_close_row {
            Event::MouseDown(MouseDownEvent {
                target: self.close.id(),
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
        if mouse.target == Some(self.id) && on_close_row {
            Event::MouseUp(MouseUpEvent {
                target: Some(self.close.id()),
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
    fn id(&self) -> WidgetId {
        self.id
    }

    fn focusable(&self) -> bool {
        true
    }

    fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
        self.close.set_focus(focused);
    }

    fn has_focus(&self) -> bool {
        self.focused
    }

    fn is_hovered(&self) -> bool {
        self.hovered
    }

    fn set_hovered(&mut self, hovered: bool) {
        self.hovered = hovered;
        if !hovered {
            self.close.set_hovered(false);
        }
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
        self.focused = false;
        self.hovered = false;
        self.close.set_focus(false);
        self.close.set_hovered(false);
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
        match event {
            Event::MouseDown(mouse) => {
                let translated = self.translate_mouse_down(*mouse);
                self.close.on_event(&translated, ctx);
            }
            Event::MouseUp(mouse) => {
                let translated = self.translate_mouse_up(*mouse);
                self.close.on_event(&translated, ctx);
            }
            Event::Action(_) | Event::Key(_) if self.focused => {
                self.close.on_event(event, ctx);
            }
            _ => {}
        }
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut EventCtx) {
        if message.sender != self.close.id() {
            self.markdown.on_message(message, ctx);
            if !ctx.handled() {
                self.close.on_message(message, ctx);
            }
            return;
        }

        if let Message::ButtonPressed { .. } = &message.message {
            ctx.post_message(
                self.id,
                Message::ButtonPressed {
                    description: "Welcome.close".to_string(),
                },
            );
            ctx.post_message(self.id, Message::OverlayDismissRequested { overlay: None });
            ctx.set_handled();
        }
    }

    fn on_mouse_move(&mut self, x: u16, y: u16) -> bool {
        if self.last_height > 1 && y + 1 >= self.last_height {
            self.close.on_mouse_move(x, 0)
        } else {
            let was_hovered = self.close.is_hovered();
            if was_hovered {
                self.close.set_hovered(false);
            }
            was_hovered
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
        let body_segments = self.markdown.render_styled(console, &body_options);
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
                *merged.get_mut(x, y) = body_buf.get(x, y).clone();
            }
        }
        for x in 0..width {
            *merged.get_mut(x, body_height) = button_buf.get(x, 0).clone();
        }

        merged.to_segments()
    }

    fn layout_height(&self) -> Option<usize> {
        fixed_height_from_constraints(self.layout_constraints())
    }

    fn content_width(&self) -> Option<usize> {
        Some(rich_rs::cell_len("Welcome!").max(8))
    }

    fn style_type(&self) -> &'static str {
        "Welcome"
    }

    fn style_classes(&self) -> &[String] {
        if self.focused {
            &self.focused_classes
        } else if self.classes.is_empty() {
            empty_classes()
        } else {
            &self.classes
        }
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }

    fn visit_children_mut(&mut self, f: &mut dyn FnMut(&mut dyn Widget)) {
        f(&mut self.markdown);
        f(&mut self.close);
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
            &MessageEvent {
                sender: welcome.close_button_id(),
                message: Message::ButtonPressed {
                    description: "Button(classes='button', variant='success')".to_string(),
                },
            },
            &mut ctx,
        );

        assert!(ctx.handled());
        let emitted = ctx.take_messages();
        assert!(emitted.iter().any(|event| {
            matches!(
                event.message,
                Message::ButtonPressed {
                    ref description
                } if description == "Welcome.close"
            )
        }));
        assert!(emitted.iter().any(|event| {
            matches!(
                event.message,
                Message::OverlayDismissRequested { overlay: None }
            )
        }));
    }
}
