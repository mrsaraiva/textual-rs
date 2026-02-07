use crossterm::event::KeyCode;
use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::event::{Action, Event, EventCtx};
use crate::message::Message;

use super::{
    Widget, WidgetId, WidgetStyles,
    helpers::{empty_classes, fixed_height_from_constraints},
};

/// A simple clickable text widget that posts a message with a URL when activated.
///
/// Renders as a single line of text; CSS provides underline/color styling.
/// Activated via click or Enter key when focused.
#[derive(Debug, Clone)]
pub struct Link {
    id: WidgetId,
    text: String,
    url: String,
    focused: bool,
    hovered: bool,
    pressed: bool,
    classes: Vec<String>,
    focused_classes: Vec<String>,
    styles: WidgetStyles,
}

impl Link {
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let url_str = text.clone();
        Self {
            id: WidgetId::new(),
            text,
            url: url_str,
            focused: false,
            hovered: false,
            pressed: false,
            classes: vec!["link".to_string()],
            focused_classes: vec!["link".to_string(), "focused".to_string()],
            styles: WidgetStyles::default(),
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
    }

    pub fn set_url(&mut self, url: impl Into<String>) {
        self.url = url.into();
    }

    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.url = url.into();
        self
    }

    fn activate(&mut self, ctx: &mut EventCtx) {
        if !self.url.is_empty() {
            ctx.post_message(
                self.id,
                Message::LinkClicked {
                    url: self.url.clone(),
                },
            );
        }
        ctx.request_repaint();
        ctx.set_handled();
    }
}

impl Widget for Link {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn focusable(&self) -> bool {
        true
    }

    fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
    }

    fn has_focus(&self) -> bool {
        self.focused
    }

    fn is_hovered(&self) -> bool {
        self.hovered
    }

    fn set_hovered(&mut self, hovered: bool) {
        self.hovered = hovered;
    }

    fn is_active(&self) -> bool {
        self.pressed && self.hovered
    }

    fn content_width(&self) -> Option<usize> {
        Some(rich_rs::cell_len(&self.text).max(1))
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        match event {
            Event::MouseDown(mouse) if mouse.target == self.id => {
                self.pressed = true;
                ctx.request_repaint();
                ctx.set_handled();
            }
            Event::MouseUp(mouse) => {
                if self.pressed {
                    self.pressed = false;
                    ctx.request_repaint();
                    if mouse.target == Some(self.id) {
                        self.activate(ctx);
                        return;
                    }
                }
            }
            Event::AppFocus(false) => {
                if self.pressed {
                    self.pressed = false;
                    ctx.request_repaint();
                }
            }
            Event::Action(Action::Toggle) if self.focused => {
                self.activate(ctx);
                return;
            }
            Event::Key(key) if self.focused => match key.code {
                KeyCode::Enter | KeyCode::Char(' ') => {
                    self.activate(ctx);
                    return;
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let line = rich_rs::set_cell_size(&self.text, width);
        let mut out = Segments::new();
        out.push(Segment::styled(
            line,
            crate::css::resolve_component_style(self, &["link"])
                .to_rich()
                .unwrap_or_else(rich_rs::Style::new),
        ));
        out
    }

    fn layout_height(&self) -> Option<usize> {
        fixed_height_from_constraints(self.layout_constraints()).or(Some(1))
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

    fn style_type(&self) -> &'static str {
        "Link"
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

impl Renderable for Link {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}
