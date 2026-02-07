use rich_rs::{Console, ConsoleOptions, Renderable, Segments};

use crate::event::{Action, Event, EventCtx};
use crate::message::{Message, MessageEvent};
use crate::render::{Cell, FrameBuffer};

use super::{Input, KeyPanel, ListView, Widget, WidgetId, WidgetRenderable, WidgetStyles};

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
    show_key_panel: bool,
    query: Input,
    list: ListView,
    key_panel: KeyPanel,
    commands: Vec<PaletteCommand>,
    filtered: Vec<usize>,
    layout_width: usize,
    layout_height: usize,
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
            show_key_panel: false,
            query: Input::new().with_placeholder("Search for commands..."),
            list: ListView::new(Vec::new()).scroll_step(2),
            key_panel: KeyPanel::new(),
            commands,
            filtered: Vec::new(),
            layout_width: 1,
            layout_height: 1,
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

    fn key_panel_width(&self, width: usize) -> usize {
        if !self.show_key_panel || width < 56 {
            return 0;
        }
        let preferred = ((width as f32) * 0.30).round() as usize;
        preferred.clamp(28, 40).min(width.saturating_sub(20))
    }

    fn palette_geometry(&self, width: usize, height: usize) -> (usize, usize, usize, usize) {
        let panel_x = 0usize;
        let panel_y = 2usize.min(height.saturating_sub(1));
        let panel_width = width.max(1);
        let max_panel_height = height.saturating_sub(panel_y).max(1);
        let panel_height = max_panel_height.min(14).max(8);
        (panel_x, panel_y, panel_width, panel_height)
    }

    fn palette_content_width(panel_width: usize) -> usize {
        panel_width.saturating_sub(2).max(1)
    }

    fn palette_results_geometry(
        &self,
        panel_x: usize,
        panel_y: usize,
        panel_width: usize,
        panel_height: usize,
    ) -> (usize, usize, usize, usize) {
        let content_x = panel_x.saturating_add(1);
        let content_y = panel_y.saturating_add(2);
        let content_width = Self::palette_content_width(panel_width);
        let content_height = panel_height.saturating_sub(3).max(1);
        (content_x, content_y, content_width, content_height)
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
            .map(|index| self.commands[*index].title.clone())
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
        match command.id.as_str() {
            "quit" => ctx.request_stop(),
            "keys" => {
                self.show_key_panel = !self.show_key_panel;
                ctx.request_repaint();
            }
            _ => {
                ctx.post_message(
                    self.id,
                    Message::CommandPaletteCommandSelected {
                        id: command.id.clone(),
                        title: command.title.clone(),
                    },
                );
            }
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
        let (width, height) = options.size;

        if !self.open {
            let key_width = self.key_panel_width(width);
            if key_width == 0 {
                return self.child.render_styled(console, options);
            }

            let body_width = width.saturating_sub(key_width).max(1);
            let mut body_options = options.clone();
            body_options.size = (body_width, height);
            body_options.max_width = body_width;
            body_options.max_height = height;

            let mut panel_options = options.clone();
            panel_options.size = (key_width, height);
            panel_options.max_width = key_width;
            panel_options.max_height = height;

            let body_buffer = FrameBuffer::from_renderable(
                console,
                &body_options,
                &WidgetRenderable::new(self.child.as_ref()),
                None,
            );
            let panel_buffer = FrameBuffer::from_renderable(
                console,
                &panel_options,
                &WidgetRenderable::new(&self.key_panel),
                None,
            );

            let mut merged = FrameBuffer::new(width, height, None);
            for y in 0..height {
                for x in 0..body_buffer.width.min(width) {
                    *merged.get_mut(x, y) = body_buffer.get(x, y).clone();
                }
                for x in 0..panel_buffer.width.min(width.saturating_sub(body_width)) {
                    let tx = body_width.saturating_add(x);
                    if tx >= width {
                        break;
                    }
                    *merged.get_mut(tx, y) = panel_buffer.get(x, y).clone();
                }
            }
            return merged.to_segments();
        }

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
        let search_icon_style =
            crate::css::resolve_component_style(self, &["command-palette--search-icon"])
                .to_rich()
                .unwrap_or(panel_style);
        let title_style =
            crate::css::resolve_component_style(self, &["command-palette--item-title"])
                .to_rich()
                .unwrap_or(panel_style);
        let help_style = crate::css::resolve_component_style(self, &["command-palette--item-help"])
            .to_rich()
            .unwrap_or(panel_style);
        let selected_style =
            crate::css::resolve_component_style(self, &["command-palette--item-selected"])
                .to_rich()
                .unwrap_or(panel_style);

        for y in panel_y..panel_y.saturating_add(panel_height).min(height) {
            for x in panel_x..panel_x.saturating_add(panel_width).min(width) {
                *merged.get_mut(x, y) = Cell::blank(Some(panel_style));
            }
        }

        let search_width = Self::palette_content_width(panel_width)
            .saturating_sub(2)
            .max(1);
        let mut search_options = options.clone();
        search_options.size = (search_width, 1);
        search_options.max_width = search_width;
        search_options.max_height = 1;
        let search_buffer =
            FrameBuffer::from_renderable(console, &search_options, &self.query, None);

        let search_y = panel_y;
        let search_icon_x = panel_x.saturating_add(1);
        if search_y < height && search_icon_x < width {
            *merged.get_mut(search_icon_x, search_y) = Cell {
                text: "🔎".to_string(),
                style: Some(search_icon_style),
                meta: None,
                continuation: false,
            };
        }
        if search_y < height {
            for sx in 0..search_buffer.width.min(search_width) {
                let tx = panel_x.saturating_add(3).saturating_add(sx);
                if tx >= width {
                    break;
                }
                *merged.get_mut(tx, search_y) = search_buffer.get(sx, 0).clone();
            }
        }

        let (results_x, results_y, results_w, results_h) =
            self.palette_results_geometry(panel_x, panel_y, panel_width, panel_height);
        let visible_items = (results_h / 2).max(1);
        let selected = self
            .list
            .selected()
            .min(self.filtered.len().saturating_sub(1));
        let start = self
            .list
            .offset()
            .min(self.filtered.len().saturating_sub(visible_items));
        for row in 0..visible_items {
            let index = start.saturating_add(row);
            let ty_title = results_y.saturating_add(row.saturating_mul(2));
            let ty_help = ty_title.saturating_add(1);
            if index >= self.filtered.len() || ty_title >= height {
                break;
            }
            let command = &self.commands[self.filtered[index]];
            let active = index == selected;
            let title_line = rich_rs::set_cell_size(&command.title, results_w);
            let help_line = rich_rs::set_cell_size(&command.help, results_w);
            let title_cell_style = if active { selected_style } else { title_style };
            let help_cell_style = if active { selected_style } else { help_style };

            for (col, ch) in title_line.chars().enumerate() {
                let tx = results_x.saturating_add(col);
                if tx >= width {
                    break;
                }
                *merged.get_mut(tx, ty_title) = Cell {
                    text: ch.to_string(),
                    style: Some(title_cell_style),
                    meta: None,
                    continuation: false,
                };
            }
            if ty_help < height {
                for (col, ch) in help_line.chars().enumerate() {
                    let tx = results_x.saturating_add(col);
                    if tx >= width {
                        break;
                    }
                    *merged.get_mut(tx, ty_help) = Cell {
                        text: ch.to_string(),
                        style: Some(help_cell_style),
                        meta: None,
                        continuation: false,
                    };
                }
            }
        }
        for ty in results_y..results_y.saturating_add(results_h).min(height) {
            for tx in results_x..results_x.saturating_add(results_w).min(width) {
                if merged.get(tx, ty).text.is_empty() {
                    *merged.get_mut(tx, ty) = Cell::blank(Some(panel_style));
                }
            }
        }

        if panel_y.saturating_add(panel_height) < height {
            let border_style =
                crate::css::resolve_component_style(self, &["command-palette--border"])
                    .to_rich()
                    .unwrap_or(panel_style);
            let border_y = panel_y.saturating_add(panel_height);
            for x in panel_x..panel_x.saturating_add(panel_width).min(width) {
                *merged.get_mut(x, border_y) = Cell {
                    text: "─".to_string(),
                    style: Some(border_style),
                    meta: None,
                    continuation: false,
                };
            }
        }

        // Ensure search input metadata remains addressable (for click/focus/cursor behavior).
        for sy in 0..search_buffer.height.min(1) {
            let ty = search_y.saturating_add(sy);
            if ty >= height {
                break;
            }
            for sx in 0..search_buffer.width.min(search_width) {
                let tx = panel_x.saturating_add(3).saturating_add(sx);
                if tx >= width {
                    break;
                }
                let cell = search_buffer.get(sx, sy).clone();
                if cell.meta.is_some() {
                    *merged.get_mut(tx, ty) = cell;
                }
            }
        }

        merged.to_segments()
    }

    fn on_mount(&mut self) {
        self.child.on_mount();
        self.query.on_mount();
        self.list.on_mount();
        self.key_panel.on_mount();
    }

    fn on_unmount(&mut self) {
        self.child.on_unmount();
        self.query.on_unmount();
        self.list.on_unmount();
        self.key_panel.on_unmount();
    }

    fn on_tick(&mut self, tick: u64) {
        self.child.on_tick(tick);
        self.key_panel.on_tick(tick);
        if self.open {
            self.query.on_tick(tick);
            self.list.on_tick(tick);
        }
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        let total_width = usize::from(width);
        let total_height = usize::from(height);
        self.layout_width = total_width.max(1);
        self.layout_height = total_height.max(1);
        let key_width = self.key_panel_width(total_width);
        let child_width = total_width.saturating_sub(key_width).max(1) as u16;
        self.child.on_resize(child_width, height);
        if key_width > 0 {
            self.key_panel.on_resize(key_width as u16, height);
        }

        let (_x, _y, panel_w, panel_h) = self.palette_geometry(total_width, total_height);
        let query_width = Self::palette_content_width(panel_w)
            .saturating_sub(2)
            .max(1);
        self.query.on_resize(query_width as u16, 1);
        let result_rows = panel_h.saturating_sub(3).max(1);
        let visible_items = (result_rows / 2).max(1);
        self.list.on_resize(1, visible_items as u16);
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        let total_width = usize::from(width);
        let total_height = usize::from(height);
        self.layout_width = total_width.max(1);
        self.layout_height = total_height.max(1);
        let key_width = self.key_panel_width(total_width);
        let child_width = total_width.saturating_sub(key_width).max(1) as u16;
        self.child.on_layout(child_width, height);
        if key_width > 0 {
            self.key_panel.on_layout(key_width as u16, height);
        }

        let (_x, _y, panel_w, panel_h) = self.palette_geometry(total_width, total_height);
        let query_width = Self::palette_content_width(panel_w)
            .saturating_sub(2)
            .max(1);
        self.query.on_layout(query_width as u16, 1);
        let result_rows = panel_h.saturating_sub(3).max(1);
        let visible_items = (result_rows / 2).max(1);
        self.list.on_layout(1, visible_items as u16);
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
        if matches!(event, Event::BindingsChanged(_)) {
            self.key_panel.on_event(event, ctx);
        }

        if let Event::Action(Action::CommandPalette) = event {
            self.set_open(!self.open, ctx);
            ctx.set_handled();
            return;
        }

        if !self.open {
            if self.show_key_panel {
                match event {
                    Event::MouseDown(mouse) if mouse.target == self.key_panel.id() => {
                        self.key_panel.on_event(event, ctx);
                        if ctx.handled() {
                            return;
                        }
                    }
                    Event::MouseUp(mouse) if mouse.target == Some(self.key_panel.id()) => {
                        self.key_panel.on_event(event, ctx);
                        if ctx.handled() {
                            return;
                        }
                    }
                    Event::MouseScroll(mouse) if mouse.target == Some(self.key_panel.id()) => {
                        self.key_panel.on_event(event, ctx);
                        if ctx.handled() {
                            return;
                        }
                    }
                    _ => {}
                }
            }
            self.child.on_event(event, ctx);
            return;
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

        if let Event::MouseDown(mouse) = event {
            let (panel_x, panel_y, panel_w, panel_h) =
                self.palette_geometry(self.layout_width, self.layout_height);
            let x = mouse.x as usize;
            let y = mouse.y as usize;
            let inside_panel = x >= panel_x
                && x < panel_x.saturating_add(panel_w)
                && y >= panel_y
                && y < panel_y.saturating_add(panel_h);

            if !inside_panel {
                self.set_open(false, ctx);
                ctx.set_handled();
                return;
            }

            if mouse.target == self.query.id() {
                // Let Input handle cursor placement/focus details.
            } else {
                let (results_x, results_y, results_w, results_h) =
                    self.palette_results_geometry(panel_x, panel_y, panel_w, panel_h);
                if x >= results_x
                    && x < results_x.saturating_add(results_w)
                    && y >= results_y
                    && y < results_y.saturating_add(results_h)
                {
                    let row = y.saturating_sub(results_y) / 2;
                    let visible_items = (results_h / 2).max(1);
                    let start = self
                        .list
                        .offset()
                        .min(self.filtered.len().saturating_sub(visible_items));
                    let index = start.saturating_add(row);
                    if index < self.filtered.len() {
                        self.list.set_selected(index);
                        self.execute_selected(ctx);
                        ctx.set_handled();
                        return;
                    }
                }
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
        self.key_panel.on_message(message, ctx);
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
            if self.show_key_panel {
                self.key_panel.on_mouse_scroll(delta_x, delta_y, ctx);
                if ctx.handled() {
                    return;
                }
            }
            self.child.on_mouse_scroll(delta_x, delta_y, ctx);
        }
    }

    fn visit_children_mut(&mut self, f: &mut dyn FnMut(&mut dyn Widget)) {
        f(self.child.as_mut());
        f(&mut self.query);
        f(&mut self.list);
        f(&mut self.key_panel);
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

        let down = crate::keys::KeyEventData::from_crossterm(KeyEvent::new(
            KeyCode::Down,
            KeyModifiers::NONE,
        ));
        let mut nav_ctx = EventCtx::default();
        palette.on_event(&Event::Key(down), &mut nav_ctx);

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

    #[test]
    fn command_palette_keys_command_toggles_key_panel() {
        let mut palette = CommandPalette::new(Label::new("body"));
        let mut ctx = EventCtx::default();
        palette.on_event(&Event::Action(Action::CommandPalette), &mut ctx);
        assert!(palette.is_open());

        let enter = crate::keys::KeyEventData::from_crossterm(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        ));
        let mut execute_ctx = EventCtx::default();
        palette.on_event(&Event::Key(enter), &mut execute_ctx);

        assert!(!palette.is_open());
        assert!(palette.show_key_panel);
    }
}
