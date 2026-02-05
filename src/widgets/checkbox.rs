use crossterm::event::KeyCode;
use rich_rs::{Console, ConsoleOptions, Renderable, Segments, Text};

use crate::event::{Action, Event, EventCtx};

use super::{
    Widget, WidgetId, WidgetStyles,
    helpers::{empty_classes, focused_classes, fixed_height_from_constraints},
};

#[derive(Debug, Clone)]
pub struct Checkbox {
    id: WidgetId,
    label: String,
    checked: bool,
    focused: bool,
    styles: WidgetStyles,
}

impl Checkbox {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            id: WidgetId::new(),
            label: label.into(),
            checked: false,
            focused: false,
            styles: WidgetStyles::default(),
        }
    }

    pub fn checked(&self) -> bool {
        self.checked
    }

    pub fn set_checked(&mut self, checked: bool) {
        self.checked = checked;
    }
}

impl Widget for Checkbox {
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
        if let Event::Action(Action::Toggle) = event {
            self.checked = !self.checked;
            ctx.set_handled();
            return;
        }
        if let Event::Key(key) = event {
            match key.code {
                KeyCode::Enter | KeyCode::Char(' ') => {
                    self.checked = !self.checked;
                    ctx.set_handled();
                }
                _ => {}
            }
        }
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let marker = if self.focused { "> " } else { "  " };
        let state = if self.checked { "[x]" } else { "[ ]" };
        let text = Text::plain(format!("{marker}{state} {}", self.label));
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

impl Renderable for Checkbox {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}
