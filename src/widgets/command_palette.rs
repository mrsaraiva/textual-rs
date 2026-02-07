use rich_rs::{Console, ConsoleOptions, Renderable, Segments};

use crate::event::{Action, Event, EventCtx};
use crate::message::{Message, MessageEvent};
use crate::render::{Cell, FrameBuffer};

use super::{Input, ListView, Widget, WidgetId, WidgetRenderable, WidgetStyles};

#[derive(Debug, Clone)]
pub struct PaletteCommand {
    pub id: String,
    pub title: String,
    pub help: String,
}

impl PaletteCommand {
    pub fn new(id: impl Into<String>, title: impl Into<String>, help: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            help: help.into(),
        }
    }
}

pub struct CommandPalette {
    id: WidgetId,
    child: Box<dyn Widget>,
    open: bool,
    query: Input,
    list: ListView,
    commands: Vec<PaletteCommand>,
    filtered: Vec<usize>,
    styles: WidgetStyles,
}

impl CommandPalette {
    pub fn new(child: impl Widget + 'static) -> Self {
        let commands = vec![
            PaletteCommand::new(
                "keys",
                "Keys",
                "Show help for the focused widget and available keys",
            ),
            PaletteCommand::new("maximize", "Maximize", "Maximize the focused widget"),
            PaletteCommand::new("quit", "Quit", "Quit the application as soon as possible"),
            PaletteCommand::new(
                "screenshot",
                "Screenshot",
                "Save an SVG screenshot of the current screen",
            ),
            PaletteCommand::new("theme", "Theme", "Change the current theme"),
        ];
        let mut out = Self {
            id: WidgetId::new(),
            child: Box::new(child),
            open: false,
            query: Input::new().with_placeholder("Search for commands..."),
            list: ListView::new(Vec::new()).scroll_step(2),
            commands,
            filtered: Vec::new(),
            styles: WidgetStyles::default(),
        };
        out.rebuild_results();
        out
    }

    pub fn with_commands(mut self, commands: Vec<PaletteCommand>) -> Self {
        self.commands = commands;
        self.rebuild_results();
        self
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    fn palette_geometry(&self, width: usize, height: usize) -> (usize, usize, usize, usize) {
        let panel_x = 0usize;
        let panel_y = 2usize.min(height.saturating_sub(1));
        let panel_width = width.max(1);
        let max_panel_height = height.saturating_sub(panel_y).max(1);
        let panel_height = max_panel_height.min(12).max(6);
        (panel_x, panel_y, panel_width, panel_height)
    }

    fn rebuild_results(&mut self) {
        let needle = self.query.text().trim().to_lowercase();
        self.filtered = self
            .commands
            .iter()
            .enumerate()
            .filter_map(|(index, command)| {
                if needle.is_empty()
                    || command.id.to_lowercase().contains(&needle)
                    || command.title.to_lowercase().contains(&needle)
                    || command.help.to_lowercase().contains(&needle)
                {
                    Some(index)
                } else {
                    None
                }
            })
            .collect();
        let list_items = self
            .filtered
            .iter()
            .map(|index| {
                let command = &self.commands[*index];
                format!("{} - {}", command.title, command.help)
            })
            .collect::<Vec<_>>();
        self.list.set_items(list_items);
        self.list.set_selected(0);
    }

    fn set_open(&mut self, open: bool, ctx: &mut EventCtx) {
        if self.open == open {
            return;
        }
        self.open = open;
        if self.open {
            self.query.set_text("");
            self.query.set_focus(true);
            self.list.set_focus(true);
            self.rebuild_results();
            ctx.post_message(self.id, Message::CommandPaletteOpened);
        } else {
            self.query.set_focus(false);
            self.list.set_focus(false);
            ctx.post_message(self.id, Message::CommandPaletteClosed);
        }
        ctx.request_repaint();
    }

    fn execute_selected(&mut self, ctx: &mut EventCtx) {
        if self.filtered.is_empty() {
            self.set_open(false, ctx);
            return;
        }
        let selected = self.list.selected().min(self.filtered.len() - 1);
        let command = &self.commands[self.filtered[selected]];
        if command.id == "quit" {
            ctx.request_stop();
        } else {
            ctx.post_message(
                self.id,
                Message::CommandPaletteCommandSelected {
                    id: command.id.clone(),
                    title: command.title.clone(),
                },
            );
        }
        self.set_open(false, ctx);
    }
}

impl Widget for CommandPalette {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn style_type(&self) -> &'static str {
        "CommandPalette"
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        if !self.open {
            return self.child.render_styled(console, options);
        }

        let (width, height) = options.size;
        let base = FrameBuffer::from_renderable(
            console,
            options,
            &WidgetRenderable::new(self.child.as_ref()),
            None,
        );
        let mut merged = base.clone();
        let (panel_x, panel_y, panel_width, panel_height) = self.palette_geometry(width, height);
        let panel_style = crate::css::resolve_component_style(self, &["command-palette--panel"])
            .to_rich()
            .unwrap_or_else(rich_rs::Style::new);
        let border_style = crate::css::resolve_component_style(self, &["command-palette--border"])
            .to_rich()
            .unwrap_or_else(rich_rs::Style::new);

        for y in panel_y..panel_y.saturating_add(panel_height).min(height) {
            for x in panel_x..panel_x.saturating_add(panel_width).min(width) {
                *merged.get_mut(x, y) = Cell::blank(Some(panel_style));
            }
        }
        if panel_y < height {
            for x in panel_x..panel_x.saturating_add(panel_width).min(width) {
                *merged.get_mut(x, panel_y) = Cell {
                    text: "─".to_string(),
                    style: Some(border_style),
                    meta: None,
                    continuation: false,
                };
            }
        }
        let search_width = panel_width.saturating_sub(2).max(1);
        let mut search_options = options.clone();
        search_options.size = (search_width, 1);
        search_options.max_width = search_width;
        search_options.max_height = 1;
        let search_buffer =
            FrameBuffer::from_renderable(console, &search_options, &self.query, None);
        let search_y = panel_y.saturating_add(1);
        if search_y < height {
            for sx in 0..search_buffer.width.min(search_width) {
                let tx = panel_x.saturating_add(1).saturating_add(sx);
                if tx >= width {
                    break;
                }
                *merged.get_mut(tx, search_y) = search_buffer.get(sx, 0).clone();
            }
        }

        let list_height = panel_height.saturating_sub(3).max(1);
        let mut list_options = options.clone();
        list_options.size = (search_width, list_height);
        list_options.max_width = search_width;
        list_options.max_height = list_height;
        let list_buffer = FrameBuffer::from_renderable(console, &list_options, &self.list, None);
        let list_y = panel_y.saturating_add(2);
        for sy in 0..list_buffer.height.min(list_height) {
            let ty = list_y.saturating_add(sy);
            if ty >= height {
                break;
            }
            for sx in 0..list_buffer.width.min(search_width) {
                let tx = panel_x.saturating_add(1).saturating_add(sx);
                if tx >= width {
                    break;
                }
                *merged.get_mut(tx, ty) = list_buffer.get(sx, sy).clone();
            }
        }

        merged.to_segments()
    }

    fn on_mount(&mut self) {
        self.child.on_mount();
        self.query.on_mount();
        self.list.on_mount();
    }

    fn on_unmount(&mut self) {
        self.child.on_unmount();
        self.query.on_unmount();
        self.list.on_unmount();
    }

    fn on_tick(&mut self, tick: u64) {
        self.child.on_tick(tick);
        if self.open {
            self.query.on_tick(tick);
            self.list.on_tick(tick);
        }
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        self.child.on_resize(width, height);
        self.query.on_resize(width, 1);
        self.list.on_resize(width, height.saturating_sub(3));
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.child.on_layout(width, height);
        let (_x, _y, panel_w, panel_h) =
            self.palette_geometry(usize::from(width), usize::from(height));
        let content_w = panel_w.saturating_sub(2).max(1) as u16;
        self.query.on_layout(content_w, 1);
        self.list
            .on_layout(content_w, panel_h.saturating_sub(3) as u16);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        if self.open {
            self.query.on_event_capture(event, ctx);
            if !ctx.handled() {
                self.list.on_event_capture(event, ctx);
            }
        } else {
            self.child.on_event_capture(event, ctx);
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if let Event::Action(Action::CommandPalette) = event {
            self.set_open(!self.open, ctx);
            ctx.set_handled();
            return;
        }

        if !self.open {
            self.child.on_event(event, ctx);
            return;
        }

        if let Event::MouseDown(mouse) = event {
            if mouse.target != self.query.id() && mouse.target != self.list.id() {
                self.set_open(false, ctx);
                ctx.set_handled();
                return;
            }
        }

        if let Event::Key(key) = event {
            if key.code == crossterm::event::KeyCode::Esc {
                self.set_open(false, ctx);
                ctx.set_handled();
                return;
            }
            if key.code == crossterm::event::KeyCode::Enter {
                self.execute_selected(ctx);
                ctx.set_handled();
                return;
            }
        }

        self.query.on_event(event, ctx);
        if !ctx.handled() {
            self.list.on_event(event, ctx);
        }
        if !ctx.handled() {
            ctx.set_handled();
        }
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut EventCtx) {
        self.query.on_message(message, ctx);
        self.list.on_message(message, ctx);
        if message.sender == self.query.id() {
            if let Message::InputChanged { .. } = &message.message {
                self.rebuild_results();
                ctx.request_repaint();
                ctx.set_handled();
                return;
            }
        }
        self.child.on_message(message, ctx);
    }

    fn on_mouse_scroll(&mut self, delta_x: i32, delta_y: i32, ctx: &mut EventCtx) {
        if self.open {
            self.list.on_mouse_scroll(delta_x, delta_y, ctx);
            if !ctx.handled() {
                ctx.set_handled();
            }
        } else {
            self.child.on_mouse_scroll(delta_x, delta_y, ctx);
        }
    }

    fn visit_children_mut(&mut self, f: &mut dyn FnMut(&mut dyn Widget)) {
        f(self.child.as_mut());
        f(&mut self.query);
        f(&mut self.list);
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

impl Renderable for CommandPalette {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{Action, Event, EventCtx};
    use crate::message::Message;
    use crate::widgets::Label;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn command_palette_toggles_from_action() {
        let mut palette = CommandPalette::new(Label::new("body"));
        let mut ctx = EventCtx::default();

        palette.on_event(&Event::Action(Action::CommandPalette), &mut ctx);
        assert!(palette.is_open());
        assert!(ctx.handled());
    }

    #[test]
    fn command_palette_emits_selection_message_on_enter() {
        let mut palette = CommandPalette::new(Label::new("body"));
        let mut ctx = EventCtx::default();
        palette.on_event(&Event::Action(Action::CommandPalette), &mut ctx);

        let key = crate::keys::KeyEventData::from_crossterm(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        ));
        let mut execute_ctx = EventCtx::default();
        palette.on_event(&Event::Key(key), &mut execute_ctx);

        let messages = execute_ctx.take_messages();
        assert!(
            messages.iter().any(|event| matches!(
                event.message,
                Message::CommandPaletteCommandSelected { .. }
            ))
        );
        assert!(!palette.is_open());
    }
}
