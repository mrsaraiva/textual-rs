use crossterm::event::KeyCode;
use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::event::{Action, Event, EventCtx};
use crate::message::Message;

use super::{
    Widget, WidgetId, WidgetStyles,
    helpers::{adjust_line_length_no_bg, empty_classes, fixed_height_from_constraints},
};

#[derive(Debug, Clone)]
pub struct ListView {
    id: WidgetId,
    items: Vec<String>,
    selected: usize,
    offset: usize,
    focused: bool,
    hovered: bool,
    hovered_index: Option<usize>,
    viewport_height: usize,
    scroll_step: usize,
    classes: Vec<String>,
    focused_classes: Vec<String>,
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
            hovered: false,
            hovered_index: None,
            viewport_height: 1,
            scroll_step: 1,
            classes: vec!["list-view".to_string()],
            focused_classes: vec!["list-view".to_string(), "focused".to_string()],
            styles: WidgetStyles::default(),
        }
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn selected_item(&self) -> Option<&str> {
        self.items.get(self.selected).map(String::as_str)
    }

    pub fn offset(&self) -> usize {
        self.offset
    }

    pub fn items(&self) -> &[String] {
        &self.items
    }

    pub fn set_selected(&mut self, index: usize) {
        if self.items.is_empty() {
            self.selected = 0;
            self.offset = 0;
            return;
        }
        self.selected = index.min(self.items.len() - 1);
        self.ensure_visible();
    }

    pub fn set_items(&mut self, items: Vec<String>) {
        self.items = items;
        self.clamp_offsets();
        self.ensure_visible();
    }

    pub fn scroll_step(mut self, step: usize) -> Self {
        self.scroll_step = step.max(1);
        self
    }

    fn max_offset(&self) -> usize {
        self.items.len().saturating_sub(self.viewport_height.max(1))
    }

    fn clamp_offsets(&mut self) {
        if self.items.is_empty() {
            self.selected = 0;
            self.offset = 0;
            self.hovered_index = None;
            return;
        }
        self.selected = self.selected.min(self.items.len() - 1);
        self.offset = self.offset.min(self.max_offset());
        if let Some(index) = self.hovered_index {
            if index >= self.items.len() {
                self.hovered_index = None;
            }
        }
    }

    fn ensure_visible(&mut self) {
        self.clamp_offsets();
        if self.items.is_empty() {
            return;
        }
        let viewport = self.viewport_height.max(1);
        if self.selected < self.offset {
            self.offset = self.selected;
        } else if self.selected >= self.offset + viewport {
            self.offset = self.selected + 1 - viewport;
        }
        self.offset = self.offset.min(self.max_offset());
    }

    fn emit_selection_changed(&self, ctx: &mut EventCtx) {
        if let Some(item) = self.items.get(self.selected) {
            ctx.post_message(
                self.id,
                Message::ListViewSelectionChanged {
                    index: self.selected,
                    item: item.clone(),
                },
            );
        }
    }

    fn select_index(&mut self, index: usize, ctx: &mut EventCtx) {
        if self.items.is_empty() {
            return;
        }
        let next = index.min(self.items.len() - 1);
        if next != self.selected {
            self.selected = next;
            self.ensure_visible();
            self.emit_selection_changed(ctx);
            ctx.request_repaint();
        }
    }

    fn move_selection(&mut self, delta: isize, ctx: &mut EventCtx) {
        if self.items.is_empty() {
            return;
        }
        let current = self.selected as isize;
        let max = (self.items.len() - 1) as isize;
        let next = (current + delta).clamp(0, max) as usize;
        self.select_index(next, ctx);
    }

    fn page_step(&self) -> usize {
        self.viewport_height.saturating_sub(1).max(1)
    }

    fn scroll_offset(&mut self, delta_rows: isize, ctx: &mut EventCtx) {
        let before = self.offset;
        if delta_rows.is_negative() {
            self.offset = self.offset.saturating_sub(delta_rows.unsigned_abs());
        } else {
            self.offset = self.offset.saturating_add(delta_rows as usize);
        }
        self.offset = self.offset.min(self.max_offset());
        if self.offset != before {
            ctx.request_repaint();
            ctx.set_handled();
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

    fn is_hovered(&self) -> bool {
        self.hovered
    }

    fn set_hovered(&mut self, hovered: bool) {
        self.hovered = hovered;
        if !hovered {
            self.hovered_index = None;
        }
    }

    fn on_layout(&mut self, _width: u16, height: u16) {
        self.viewport_height = usize::from(height).max(1);
        self.ensure_visible();
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        match event {
            Event::MouseDown(mouse) if mouse.target == self.id => {
                let index = self.offset.saturating_add(mouse.y as usize);
                if index < self.items.len() {
                    self.select_index(index, ctx);
                    ctx.set_handled();
                }
            }
            Event::Action(action) if self.focused => match action {
                Action::ScrollUp => {
                    self.move_selection(-1, ctx);
                    ctx.set_handled();
                }
                Action::ScrollDown => {
                    self.move_selection(1, ctx);
                    ctx.set_handled();
                }
                Action::ScrollPageUp => {
                    self.move_selection(-(self.page_step() as isize), ctx);
                    ctx.set_handled();
                }
                Action::ScrollPageDown => {
                    self.move_selection(self.page_step() as isize, ctx);
                    ctx.set_handled();
                }
                _ => {}
            },
            Event::Key(key) if self.focused => match key.code {
                KeyCode::Up => {
                    self.move_selection(-1, ctx);
                    ctx.set_handled();
                }
                KeyCode::Down => {
                    self.move_selection(1, ctx);
                    ctx.set_handled();
                }
                KeyCode::PageUp => {
                    self.move_selection(-(self.page_step() as isize), ctx);
                    ctx.set_handled();
                }
                KeyCode::PageDown => {
                    self.move_selection(self.page_step() as isize, ctx);
                    ctx.set_handled();
                }
                KeyCode::Home => {
                    self.select_index(0, ctx);
                    ctx.set_handled();
                }
                KeyCode::End => {
                    if !self.items.is_empty() {
                        self.select_index(self.items.len() - 1, ctx);
                    }
                    ctx.set_handled();
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn on_mouse_move(&mut self, _x: u16, y: u16) -> bool {
        if self.items.is_empty() {
            return false;
        }
        let index = self.offset.saturating_add(y as usize);
        let hovered = (index < self.items.len()).then_some(index);
        if hovered != self.hovered_index {
            self.hovered_index = hovered;
            return true;
        }
        false
    }

    fn on_mouse_scroll(&mut self, _delta_x: i32, delta_y: i32, ctx: &mut EventCtx) {
        if delta_y == 0 {
            return;
        }
        self.scroll_offset(
            delta_y.saturating_mul(self.scroll_step as i32) as isize,
            ctx,
        );
    }

    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);
        let mut out = Segments::new();

        let base_style = crate::css::resolve_component_style(self, &["list-view--item"])
            .to_rich()
            .unwrap_or_else(rich_rs::Style::new);

        for row in 0..height {
            let index = self.offset + row;
            let mut text = String::new();
            let mut style = base_style;
            if let Some(item) = self.items.get(index) {
                let selected = index == self.selected;
                let hovered = self.hovered_index == Some(index);
                let mut classes = vec!["list-view--item"];
                if selected {
                    classes.push("-selected");
                }
                if hovered {
                    classes.push("-hover");
                }
                if selected && self.focused {
                    classes.push("-focus");
                }
                style = crate::css::resolve_component_style(self, &classes)
                    .to_rich()
                    .unwrap_or(style);
                let marker = if selected { "› " } else { "  " };
                text = format!("{marker}{item}");
            }

            let line = adjust_line_length_no_bg(&[Segment::styled(text, style)], width);
            out.extend(line);
            if row + 1 < height {
                out.push(Segment::line());
            }
        }

        out
    }

    fn layout_height(&self) -> Option<usize> {
        fixed_height_from_constraints(self.layout_constraints()).or(Some(self.items.len().max(1)))
    }

    fn content_width(&self) -> Option<usize> {
        let width = self
            .items
            .iter()
            .map(|item| rich_rs::cell_len(item).saturating_add(2))
            .max()
            .unwrap_or(2)
            .max(1);
        Some(width)
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
}

impl Renderable for ListView {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}
