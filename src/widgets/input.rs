use crossterm::event::KeyCode;
use rich_rs::{Console, ConsoleOptions, Renderable, Segments, Text};

use crate::event::{Event, EventCtx};

use super::{
    Widget, WidgetId, WidgetStyles,
    helpers::{empty_classes, focused_classes, fixed_height_from_constraints},
};

pub struct Input {
    id: WidgetId,
    text: String,
    cursor: usize,
    focused: bool,
    placeholder: Option<String>,
    styles: WidgetStyles,
}

impl Input {
    pub fn new() -> Self {
        Self {
            id: WidgetId::new(),
            text: String::new(),
            cursor: 0,
            focused: false,
            placeholder: None,
            styles: WidgetStyles::default(),
        }
    }

    pub fn with_placeholder(mut self, value: impl Into<String>) -> Self {
        self.placeholder = Some(value.into());
        self
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn set_text(&mut self, value: impl Into<String>) {
        self.text = value.into();
        if self.cursor > self.text.len() {
            self.cursor = self.text.len();
        }
    }
}

impl Widget for Input {
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

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if !self.focused {
            return;
        }
        if let Event::Key(key) = event {
            match key.code {
                KeyCode::Char(ch) => {
                    self.text.insert(self.cursor, ch);
                    self.cursor += 1;
                    ctx.set_handled();
                }
                KeyCode::Backspace => {
                    if self.cursor > 0 {
                        self.cursor -= 1;
                        self.text.remove(self.cursor);
                        ctx.set_handled();
                    }
                }
                KeyCode::Delete => {
                    if self.cursor < self.text.len() {
                        self.text.remove(self.cursor);
                        ctx.set_handled();
                    }
                }
                KeyCode::Left => {
                    if self.cursor > 0 {
                        self.cursor -= 1;
                        ctx.set_handled();
                    }
                }
                KeyCode::Right => {
                    if self.cursor < self.text.len() {
                        self.cursor += 1;
                        ctx.set_handled();
                    }
                }
                KeyCode::Home => {
                    self.cursor = 0;
                    ctx.set_handled();
                }
                KeyCode::End => {
                    self.cursor = self.text.len();
                    ctx.set_handled();
                }
                _ => {}
            }
        }
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let marker = if self.focused { "> " } else { "  " };
        let content = if self.text.is_empty() {
            self.placeholder.clone().unwrap_or_default()
        } else {
            self.text.clone()
        };
        let text = Text::plain(format!("{marker}{content}"));
        text.render(console, options)
    }

    fn layout_height(&self) -> Option<usize> {
        fixed_height_from_constraints(self.layout_constraints()).or(Some(1))
    }

    fn style_classes(&self) -> &[String] {
        if self.focused {
            focused_classes()
        } else {
            empty_classes()
        }
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

impl Renderable for Input {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}
