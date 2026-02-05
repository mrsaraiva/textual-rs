use crossterm::event::KeyCode;
use rich_rs::{Console, ConsoleOptions, Renderable, Segments, Text};

use crate::event::{Action, Event, EventCtx};

use super::{
    Widget, WidgetId, WidgetStyles,
    helpers::{empty_classes, focused_classes},
};

#[derive(Debug, Clone)]
pub struct ListView {
    id: WidgetId,
    items: Vec<String>,
    selected: usize,
    offset: usize,
    focused: bool,
    styles: WidgetStyles,
}

impl ListView {
    pub fn new(items: Vec<String>) -> Self {
        Self {
            id: WidgetId::new(),
            items,
            selected: 0,
            offset: 0,
            focused: false,
            styles: WidgetStyles::default(),
        }
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn set_selected(&mut self, index: usize) {
        if self.items.is_empty() {
            self.selected = 0;
            self.offset = 0;
            return;
        }
        self.selected = index.min(self.items.len() - 1);
    }

    fn ensure_visible(&mut self, height: usize) {
        if self.items.is_empty() {
            self.offset = 0;
            return;
        }
        if self.selected < self.offset {
            self.offset = self.selected;
        } else if self.selected >= self.offset + height {
            self.offset = self.selected + 1 - height;
        }
    }
}

impl Widget for ListView {
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
        let mut handled = false;
        match event {
            Event::Action(Action::ScrollUp) => {
                if self.selected > 0 {
                    self.selected -= 1;
                }
                handled = true;
            }
            Event::Action(Action::ScrollDown) => {
                if self.selected + 1 < self.items.len() {
                    self.selected += 1;
                }
                handled = true;
            }
            Event::Action(Action::ScrollPageUp) => {
                if self.selected > 0 {
                    let step = 5.min(self.selected);
                    self.selected -= step;
                }
                handled = true;
            }
            Event::Action(Action::ScrollPageDown) => {
                if self.selected + 1 < self.items.len() {
                    let step = 5.min(self.items.len().saturating_sub(1) - self.selected);
                    self.selected += step;
                }
                handled = true;
            }
            Event::Key(key) => match key.code {
                KeyCode::Up => {
                    if self.selected > 0 {
                        self.selected -= 1;
                    }
                    handled = true;
                }
                KeyCode::Down => {
                    if self.selected + 1 < self.items.len() {
                        self.selected += 1;
                    }
                    handled = true;
                }
                KeyCode::PageUp => {
                    if self.selected > 0 {
                        let step = 5.min(self.selected);
                        self.selected -= step;
                    }
                    handled = true;
                }
                KeyCode::PageDown => {
                    if self.selected + 1 < self.items.len() {
                        let step = 5.min(self.items.len().saturating_sub(1) - self.selected);
                        self.selected += step;
                    }
                    handled = true;
                }
                _ => {}
            },
            _ => {}
        }
        if handled {
            ctx.set_handled();
        }
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let height = options.size.1.max(1);
        let mut view = self.clone();
        view.ensure_visible(height);

        let mut lines: Vec<String> = Vec::new();
        for (idx, item) in view.items.iter().enumerate() {
            if idx < view.offset {
                continue;
            }
            if lines.len() >= height {
                break;
            }
            let marker = if self.focused && idx == view.selected {
                "> "
            } else if idx == view.selected {
                "* "
            } else {
                "  "
            };
            lines.push(format!("{marker}{item}"));
        }
        if lines.is_empty() {
            lines.push(String::new());
        }
        let text = Text::plain(lines.join("\n"));
        text.render(console, options)
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

impl Renderable for ListView {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}
