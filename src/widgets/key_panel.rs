use std::sync::atomic::{AtomicUsize, Ordering};

use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::event::{Action, BindingHint, Event, EventCtx};

use super::footer::FooterBinding;
use super::helpers::{
    adjust_line_length_no_bg, empty_classes, fixed_height_from_constraints, pad_lines_to_width,
};
use super::{Widget, WidgetId, WidgetStyles};

#[derive(Debug, Clone)]
pub struct BindingsTable {
    id: WidgetId,
    bindings: Vec<FooterBinding>,
    classes: Vec<String>,
    styles: WidgetStyles,
}

impl BindingsTable {
    pub fn new() -> Self {
        Self {
            id: WidgetId::new(),
            bindings: Vec::new(),
            classes: Vec::new(),
            styles: WidgetStyles::default(),
        }
    }

    pub fn with_bindings(mut self, bindings: Vec<FooterBinding>) -> Self {
        self.bindings = bindings;
        self
    }

    pub fn set_bindings(&mut self, bindings: Vec<FooterBinding>) {
        self.bindings = bindings;
    }

    fn lines(&self, width: usize) -> Vec<Vec<Segment>> {
        if self.bindings.is_empty() {
            return vec![adjust_line_length_no_bg(
                &[Segment::new("(no bindings)".to_string())],
                width,
            )];
        }

        let key_column_width = self
            .bindings
            .iter()
            .map(|binding| rich_rs::cell_len(&binding.key))
            .max()
            .unwrap_or(0)
            .min(24)
            .max(1);

        let mut out = Vec::new();
        let key_head = rich_rs::set_cell_size("Key", key_column_width);
        out.push(adjust_line_length_no_bg(
            &[Segment::new(format!(" {}  Description", key_head))],
            width,
        ));
        out.push(adjust_line_length_no_bg(
            &[Segment::new("-".repeat(width))],
            width,
        ));
        for binding in &self.bindings {
            let key = rich_rs::set_cell_size(&binding.key, key_column_width);
            let line = format!(" {}  {}", key, binding.description);
            out.push(adjust_line_length_no_bg(&[Segment::new(line)], width));
        }
        out
    }
}

impl Widget for BindingsTable {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let mut lines = self.lines(width);
        lines = pad_lines_to_width(lines, width);
        let line_count = lines.len();
        let mut out = Segments::new();
        for (index, line) in lines.into_iter().enumerate() {
            out.extend(line);
            if index + 1 < line_count {
                out.push(Segment::line());
            }
        }
        out
    }

    fn layout_height(&self) -> Option<usize> {
        fixed_height_from_constraints(self.layout_constraints())
            .or(Some(self.bindings.len().max(1)))
    }

    fn style_classes(&self) -> &[String] {
        if self.classes.is_empty() {
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

impl Renderable for BindingsTable {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[derive(Debug)]
pub struct KeyPanel {
    id: WidgetId,
    title: String,
    table: BindingsTable,
    offset_y: usize,
    scroll_step: usize,
    content_height: AtomicUsize,
    viewport_height: AtomicUsize,
    classes: Vec<String>,
    styles: WidgetStyles,
}

impl KeyPanel {
    pub fn new() -> Self {
        Self {
            id: WidgetId::new(),
            title: "Key Bindings".to_string(),
            table: BindingsTable::new(),
            offset_y: 0,
            scroll_step: 1,
            content_height: AtomicUsize::new(1),
            viewport_height: AtomicUsize::new(1),
            classes: Vec::new(),
            styles: WidgetStyles::default(),
        }
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    pub fn with_bindings(mut self, bindings: Vec<FooterBinding>) -> Self {
        self.table.set_bindings(bindings);
        self
    }

    pub fn set_bindings(&mut self, bindings: Vec<FooterBinding>) {
        self.table.set_bindings(bindings);
        self.clamp_offset();
    }

    pub fn set_binding_hints(&mut self, bindings: &[BindingHint]) {
        let mapped = bindings
            .iter()
            .map(|hint| FooterBinding::new(hint.key.clone(), hint.description.clone()))
            .collect::<Vec<_>>();
        self.set_bindings(mapped);
    }

    pub fn scroll_step(mut self, step: usize) -> Self {
        self.scroll_step = step.max(1);
        self
    }

    fn max_offset(&self) -> usize {
        let content = self.content_height.load(Ordering::Relaxed).max(1);
        let viewport = self.viewport_height.load(Ordering::Relaxed).max(1);
        content.saturating_sub(viewport)
    }

    fn clamp_offset(&mut self) {
        let max = self.max_offset();
        if self.offset_y > max {
            self.offset_y = max;
        }
    }

    fn scroll_by(&mut self, delta: i32) {
        if delta.is_negative() {
            self.offset_y = self.offset_y.saturating_sub(delta.unsigned_abs() as usize);
        } else {
            self.offset_y = self.offset_y.saturating_add(delta as usize);
        }
        self.clamp_offset();
    }
}

impl Widget for KeyPanel {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);

        let title_line =
            adjust_line_length_no_bg(&[Segment::new(format!(" {} ", self.title))], width);

        let body_viewport = height.saturating_sub(1).max(1);
        self.viewport_height.store(body_viewport, Ordering::Relaxed);
        let table_lines = self.table.lines(width);
        self.content_height
            .store(table_lines.len().max(1), Ordering::Relaxed);

        let max_offset = table_lines.len().saturating_sub(body_viewport);
        let offset = self.offset_y.min(max_offset);
        let start = offset.min(table_lines.len());
        let end = (start + body_viewport).min(table_lines.len());
        let mut body = table_lines[start..end].to_vec();
        body = pad_lines_to_width(body, width);
        while body.len() < body_viewport {
            body.push(vec![Segment::new(" ".repeat(width))]);
        }

        let mut out = Segments::new();
        out.extend(title_line);
        if height > 1 {
            out.push(Segment::line());
            for (index, line) in body.into_iter().enumerate() {
                out.extend(line);
                if index + 1 < body_viewport {
                    out.push(Segment::line());
                }
            }
        }
        out
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if let Event::BindingsChanged(bindings) = event {
            self.set_binding_hints(bindings);
            ctx.request_repaint();
            return;
        }
        if let Event::Action(action) = event {
            let before = self.offset_y;
            match action {
                Action::ScrollUp => self.scroll_by(-(self.scroll_step as i32)),
                Action::ScrollDown => self.scroll_by(self.scroll_step as i32),
                Action::ScrollPageUp => {
                    let page = self.viewport_height.load(Ordering::Relaxed).max(1);
                    self.scroll_by(-(page as i32));
                }
                Action::ScrollPageDown => {
                    let page = self.viewport_height.load(Ordering::Relaxed).max(1);
                    self.scroll_by(page as i32);
                }
                _ => return,
            }
            if self.offset_y != before {
                ctx.request_repaint();
            }
            ctx.set_handled();
        }
    }

    fn on_mouse_scroll(&mut self, _delta_x: i32, delta_y: i32, ctx: &mut EventCtx) {
        if delta_y == 0 {
            return;
        }
        let before = self.offset_y;
        self.scroll_by(delta_y.saturating_mul(self.scroll_step as i32));
        if self.offset_y != before {
            ctx.request_repaint();
        }
        ctx.set_handled();
    }

    fn layout_height(&self) -> Option<usize> {
        fixed_height_from_constraints(self.layout_constraints())
    }

    fn style_classes(&self) -> &[String] {
        if self.classes.is_empty() {
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

impl Renderable for KeyPanel {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}
