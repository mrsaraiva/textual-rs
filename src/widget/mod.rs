use std::sync::atomic::{AtomicU64, Ordering};

use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments, Text};
use rich_rs::markdown::Markdown as RichMarkdown;

use crate::debug::DebugLayout;
use crate::event::{Action, Event, EventCtx};
use crossterm::event::KeyCode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct WidgetId(u64);

impl WidgetId {
    pub fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

pub trait Widget: Send + Sync {
    fn id(&self) -> WidgetId;
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments;
    fn render_with_debug(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        _debug: &DebugLayout,
    ) -> Segments {
        self.render(console, options)
    }
    fn on_mount(&mut self) {}
    fn on_unmount(&mut self) {}
    fn on_tick(&mut self, _tick: u64) {}
    fn on_resize(&mut self, _width: u16, _height: u16) {}
    fn on_event_capture(&mut self, _event: &Event, _ctx: &mut EventCtx) {}
    fn on_event(&mut self, _event: &Event, _ctx: &mut EventCtx) {}
    fn focusable(&self) -> bool {
        false
    }
    fn set_focus(&mut self, _focused: bool) {}
    fn layout_height(&self) -> Option<usize> {
        None
    }
}

pub struct WidgetRenderable<'a> {
    widget: &'a dyn Widget,
}

#[derive(Default)]
pub struct Container {
    id: WidgetId,
    children: Vec<Box<dyn Widget>>,
}

impl Container {
    pub fn new() -> Self {
        Self {
            id: WidgetId::new(),
            children: Vec::new(),
        }
    }

    pub fn with_child(mut self, child: impl Widget + 'static) -> Self {
        self.children.push(Box::new(child));
        self
    }

    pub fn push(&mut self, child: impl Widget + 'static) {
        self.children.push(Box::new(child));
    }
}

impl Widget for Container {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height_limit = options.size.1.max(1);
        let bounds = rich_rs::Region::from_size(width as u32, height_limit as u32);

        let mut lines: Vec<Vec<Segment>> = Vec::new();
        let mut cursor_y: i32 = 0;

        for child in &self.children {
            let segments = child.render(console, options);
            let mut child_lines =
                Segment::split_and_crop_lines(segments, width, None, true, false);
            if let Some(height) = child.layout_height() {
                child_lines = Segment::set_shape(&child_lines, width, Some(height), None, false);
            }
            let child_height = child_lines.len();
            let child_region =
                rich_rs::Region::new(0, cursor_y, width as u32, child_height as u32);
            if let Some(visible) = child_region.intersection(&bounds) {
                let start = (visible.y - child_region.y).max(0) as usize;
                let end = (start + visible.height as usize).min(child_lines.len());
                for line in child_lines.into_iter().skip(start).take(end - start) {
                    if lines.len() >= height_limit {
                        break;
                    }
                    lines.push(line);
                }
            }
            cursor_y += child_height as i32;
            if cursor_y as usize >= height_limit {
                break;
            }
        }

        let line_count = lines.len();
        let mut out = Segments::new();
        for (idx, line) in lines.into_iter().enumerate() {
            out.extend(line);
            if idx + 1 < line_count {
                out.push(Segment::line());
            }
        }
        out
    }

    fn render_with_debug(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        debug: &DebugLayout,
    ) -> Segments {
        let width = options.size.0.max(1);
        let height_limit = options.size.1.max(1);
        let bounds = rich_rs::Region::from_size(width as u32, height_limit as u32);

        let mut lines: Vec<Vec<Segment>> = Vec::new();
        let mut cursor_y: i32 = 0;

        for (idx, child) in self.children.iter().enumerate() {
            let segments = child.render(console, options);
            let mut child_lines =
                Segment::split_and_crop_lines(segments, width, None, true, false);
            if let Some(height) = child.layout_height() {
                child_lines = Segment::set_shape(&child_lines, width, Some(height), None, false);
            }
            let child_height = child_lines.len().max(1);
            let debug_height = (child_height + 2).max(3);
            let child_region =
                rich_rs::Region::new(0, cursor_y, width as u32, debug_height as u32);
            if let Some(visible) = child_region.intersection(&bounds) {
                let start = (visible.y - child_region.y).max(0) as usize;
                let end = (start + visible.height as usize).min(debug_height);
                let label = if debug.show_sizes {
                    Some(format!("{width}x{debug_height}"))
                } else {
                    None
                };
                let wrapped = apply_debug_box(
                    child_lines,
                    width,
                    debug_height,
                    label.as_deref(),
                    debug.style_for(idx),
                );
                for line in wrapped.into_iter().skip(start).take(end - start) {
                    if lines.len() >= height_limit {
                        break;
                    }
                    lines.push(line);
                }
            }
            cursor_y += debug_height as i32;
            if cursor_y as usize >= height_limit {
                break;
            }
        }

        let line_count = lines.len();
        let mut out = Segments::new();
        for (idx, line) in lines.into_iter().enumerate() {
            out.extend(line);
            if idx + 1 < line_count {
                out.push(Segment::line());
            }
        }
        out
    }

    fn on_mount(&mut self) {
        for child in &mut self.children {
            child.on_mount();
        }
    }

    fn on_unmount(&mut self) {
        for child in &mut self.children {
            child.on_unmount();
        }
    }

    fn on_tick(&mut self, tick: u64) {
        for child in &mut self.children {
            child.on_tick(tick);
        }
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        for child in &mut self.children {
            child.on_resize(width, height);
        }
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        for child in &mut self.children {
            child.on_event_capture(event, ctx);
            if ctx.handled() {
                break;
            }
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        for child in &mut self.children {
            child.on_event(event, ctx);
            if ctx.handled() {
                break;
            }
        }
    }
}

impl Renderable for Container {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

impl<'a> WidgetRenderable<'a> {
    pub fn new(widget: &'a dyn Widget) -> Self {
        Self { widget }
    }
}

impl Renderable for WidgetRenderable<'_> {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        self.widget.render(console, options)
    }
}

#[derive(Debug, Clone)]
pub struct Label {
    id: WidgetId,
    text: String,
}

impl Label {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            id: WidgetId::new(),
            text: text.into(),
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
    }
}

impl Widget for Label {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let text = Text::plain(&self.text);
        text.render(console, options)
    }

    fn layout_height(&self) -> Option<usize> {
        Some(1)
    }
}

impl Renderable for Label {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

pub struct Row {
    id: WidgetId,
    children: Vec<Box<dyn Widget>>,
}

impl Row {
    pub fn new() -> Self {
        Self {
            id: WidgetId::new(),
            children: Vec::new(),
        }
    }

    pub fn with_child(mut self, child: impl Widget + 'static) -> Self {
        self.children.push(Box::new(child));
        self
    }

    pub fn push(&mut self, child: impl Widget + 'static) {
        self.children.push(Box::new(child));
    }
}

impl Widget for Row {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height_limit = options.size.1.max(1);

        let count = self.children.len().max(1);
        let base = width / count;
        let remainder = width % count;

        let widths: Vec<usize> = (0..count)
            .map(|idx| base + if idx < remainder { 1 } else { 0 })
            .collect();

        let mut child_lines: Vec<Vec<Vec<Segment>>> = Vec::new();

        for (idx, child) in self.children.iter().enumerate() {
            let child_width = widths[idx].max(1);
            let mut child_options = options.clone();
            child_options.size = (child_width, height_limit);
            child_options.max_width = child_width;
            child_options.max_height = height_limit;

            let segments = child.render(console, &child_options);
            let mut lines = Segment::split_and_crop_lines(segments, child_width, None, true, false);
            if let Some(height) = child.layout_height() {
                let capped = height.min(height_limit);
                lines = Segment::set_shape(&lines, child_width, Some(capped), None, false);
            }
            child_lines.push(lines);
        }

        let max_child_height = child_lines
            .iter()
            .map(|lines| lines.len())
            .max()
            .unwrap_or(1)
            .max(1)
            .min(height_limit);

        let mut out_lines: Vec<Vec<Segment>> = Vec::new();
        for row in 0..max_child_height {
            let mut line: Vec<Segment> = Vec::new();
            for (idx, lines) in child_lines.iter().enumerate() {
                let child_width = widths.get(idx).copied().unwrap_or(1).max(1);
                let child_line = lines.get(row).cloned().unwrap_or_else(|| {
                    vec![Segment::new(" ".repeat(child_width))]
                });
                let adjusted = Segment::adjust_line_length(&child_line, child_width, None, true);
                line.extend(adjusted);
            }
            out_lines.push(line);
        }

        let line_count = out_lines.len();
        let mut out = Segments::new();
        for (idx, line) in out_lines.into_iter().enumerate() {
            out.extend(line);
            if idx + 1 < line_count {
                out.push(Segment::line());
            }
        }
        out
    }

    fn render_with_debug(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        debug: &DebugLayout,
    ) -> Segments {
        let width = options.size.0.max(1);
        let height_limit = options.size.1.max(1);

        let count = self.children.len().max(1);
        let base = width / count;
        let remainder = width % count;

        let widths: Vec<usize> = (0..count)
            .map(|idx| base + if idx < remainder { 1 } else { 0 })
            .collect();

        let mut child_lines: Vec<Vec<Vec<Segment>>> = Vec::new();

        for (idx, child) in self.children.iter().enumerate() {
            let child_width = widths[idx].max(1);
            let mut child_options = options.clone();
            child_options.size = (child_width, height_limit);
            child_options.max_width = child_width;
            child_options.max_height = height_limit;

            let segments = child.render(console, &child_options);
            let mut lines = Segment::split_and_crop_lines(segments, child_width, None, true, false);
            if let Some(height) = child.layout_height() {
                let capped = height.min(height_limit);
                lines = Segment::set_shape(&lines, child_width, Some(capped), None, false);
            }
            let child_height = lines.len().max(1);
            let debug_height = (child_height + 2).max(3);
            let label = if debug.show_sizes {
                Some(format!("{child_width}x{debug_height}"))
            } else {
                None
            };
            let wrapped = apply_debug_box(
                lines,
                child_width,
                debug_height,
                label.as_deref(),
                debug.style_for(idx),
            );
            child_lines.push(wrapped);
        }

        let max_child_height = child_lines
            .iter()
            .map(|lines| lines.len())
            .max()
            .unwrap_or(1)
            .max(1)
            .min(height_limit);

        let mut out_lines: Vec<Vec<Segment>> = Vec::new();
        for row in 0..max_child_height {
            let mut line: Vec<Segment> = Vec::new();
            for (idx, lines) in child_lines.iter().enumerate() {
                let child_width = widths.get(idx).copied().unwrap_or(1).max(1);
                let child_line = lines.get(row).cloned().unwrap_or_else(|| {
                    vec![Segment::new(" ".repeat(child_width))]
                });
                let adjusted = Segment::adjust_line_length(&child_line, child_width, None, true);
                line.extend(adjusted);
            }
            out_lines.push(line);
        }

        let line_count = out_lines.len();
        let mut out = Segments::new();
        for (idx, line) in out_lines.into_iter().enumerate() {
            out.extend(line);
            if idx + 1 < line_count {
                out.push(Segment::line());
            }
        }
        out
    }

    fn on_mount(&mut self) {
        for child in &mut self.children {
            child.on_mount();
        }
    }

    fn on_unmount(&mut self) {
        for child in &mut self.children {
            child.on_unmount();
        }
    }

    fn on_tick(&mut self, tick: u64) {
        for child in &mut self.children {
            child.on_tick(tick);
        }
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        for child in &mut self.children {
            child.on_resize(width, height);
        }
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        for child in &mut self.children {
            child.on_event_capture(event, ctx);
            if ctx.handled() {
                break;
            }
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        for child in &mut self.children {
            child.on_event(event, ctx);
            if ctx.handled() {
                break;
            }
        }
    }
}

impl Renderable for Row {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockKind {
    Top,
    Bottom,
    Left,
    Right,
    Fill,
}

pub struct DockItem {
    kind: DockKind,
    size: Option<usize>,
    child: Box<dyn Widget>,
}

pub struct Dock {
    id: WidgetId,
    items: Vec<DockItem>,
    fixed_height: Option<usize>,
}

impl Dock {
    pub fn new() -> Self {
        Self {
            id: WidgetId::new(),
            items: Vec::new(),
            fixed_height: None,
        }
    }

    pub fn height(mut self, height: usize) -> Self {
        self.fixed_height = Some(height.max(1));
        self
    }

    pub fn push_top(mut self, height: Option<usize>, child: impl Widget + 'static) -> Self {
        self.items.push(DockItem {
            kind: DockKind::Top,
            size: height,
            child: Box::new(child),
        });
        self
    }

    pub fn push_bottom(mut self, height: Option<usize>, child: impl Widget + 'static) -> Self {
        self.items.push(DockItem {
            kind: DockKind::Bottom,
            size: height,
            child: Box::new(child),
        });
        self
    }

    pub fn push_left(mut self, width: usize, child: impl Widget + 'static) -> Self {
        self.items.push(DockItem {
            kind: DockKind::Left,
            size: Some(width),
            child: Box::new(child),
        });
        self
    }

    pub fn push_right(mut self, width: usize, child: impl Widget + 'static) -> Self {
        self.items.push(DockItem {
            kind: DockKind::Right,
            size: Some(width),
            child: Box::new(child),
        });
        self
    }

    pub fn push_fill(mut self, child: impl Widget + 'static) -> Self {
        self.items.push(DockItem {
            kind: DockKind::Fill,
            size: None,
            child: Box::new(child),
        });
        self
    }
}

impl Widget for Dock {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let mut remaining_width = options.size.0.max(1);
        let mut remaining_height = self
            .fixed_height
            .unwrap_or_else(|| options.size.1.max(1));

        let mut top_lines: Vec<Vec<Segment>> = Vec::new();
        let mut bottom_lines: Vec<Vec<Segment>> = Vec::new();

        let mut left_columns: Vec<(usize, Vec<Vec<Segment>>)> = Vec::new();
        let mut right_columns: Vec<(usize, Vec<Vec<Segment>>)> = Vec::new();
        let mut fill_lines: Option<Vec<Vec<Segment>>> = None;

        for item in &self.items {
            match item.kind {
                DockKind::Top => {
                    let height = item
                        .size
                        .or_else(|| item.child.layout_height())
                        .unwrap_or(1)
                        .min(remaining_height);
                    let mut child_options = options.clone();
                    child_options.size = (remaining_width, height);
                    child_options.max_width = remaining_width;
                    child_options.max_height = height;
                    let segments = item.child.render(console, &child_options);
                    let mut lines =
                        Segment::split_and_crop_lines(segments, remaining_width, None, true, false);
                    lines = Segment::set_shape(&lines, remaining_width, Some(height), None, false);
                    top_lines.extend(lines);
                    remaining_height = remaining_height.saturating_sub(height);
                }
                DockKind::Bottom => {
                    let height = item
                        .size
                        .or_else(|| item.child.layout_height())
                        .unwrap_or(1)
                        .min(remaining_height);
                    let mut child_options = options.clone();
                    child_options.size = (remaining_width, height);
                    child_options.max_width = remaining_width;
                    child_options.max_height = height;
                    let segments = item.child.render(console, &child_options);
                    let mut lines =
                        Segment::split_and_crop_lines(segments, remaining_width, None, true, false);
                    lines = Segment::set_shape(&lines, remaining_width, Some(height), None, false);
                    bottom_lines.extend(lines);
                    remaining_height = remaining_height.saturating_sub(height);
                }
                DockKind::Left => {
                    let width = item.size.unwrap_or(1).min(remaining_width);
                    let mut child_options = options.clone();
                    child_options.size = (width, remaining_height);
                    child_options.max_width = width;
                    child_options.max_height = remaining_height;
                    let segments = item.child.render(console, &child_options);
                    let mut lines =
                        Segment::split_and_crop_lines(segments, width, None, true, false);
                    lines =
                        Segment::set_shape(&lines, width, Some(remaining_height), None, false);
                    left_columns.push((width, lines));
                    remaining_width = remaining_width.saturating_sub(width);
                }
                DockKind::Right => {
                    let width = item.size.unwrap_or(1).min(remaining_width);
                    let mut child_options = options.clone();
                    child_options.size = (width, remaining_height);
                    child_options.max_width = width;
                    child_options.max_height = remaining_height;
                    let segments = item.child.render(console, &child_options);
                    let mut lines =
                        Segment::split_and_crop_lines(segments, width, None, true, false);
                    lines =
                        Segment::set_shape(&lines, width, Some(remaining_height), None, false);
                    right_columns.push((width, lines));
                    remaining_width = remaining_width.saturating_sub(width);
                }
                DockKind::Fill => {
                    let mut child_options = options.clone();
                    child_options.size = (remaining_width, remaining_height);
                    child_options.max_width = remaining_width;
                    child_options.max_height = remaining_height;
                    let segments = item.child.render(console, &child_options);
                    let mut lines =
                        Segment::split_and_crop_lines(segments, remaining_width, None, true, false);
                    lines =
                        Segment::set_shape(&lines, remaining_width, Some(remaining_height), None, false);
                    fill_lines = Some(lines);
                }
            }
        }

        let mut middle_lines: Vec<Vec<Segment>> = Vec::new();
        for row in 0..remaining_height {
            let mut line: Vec<Segment> = Vec::new();

            for (col_width, column) in &left_columns {
                let col_line = column.get(row).cloned().unwrap_or_else(|| {
                    vec![Segment::new(" ".repeat(*col_width))]
                });
                let adjusted = Segment::adjust_line_length(&col_line, *col_width, None, true);
                line.extend(adjusted);
            }

            let remaining_mid_width = remaining_width;
            if let Some(lines) = &fill_lines {
                let fill_line = lines.get(row).cloned().unwrap_or_else(|| {
                    vec![Segment::new(" ".repeat(remaining_mid_width))]
                });
                let adjusted =
                    Segment::adjust_line_length(&fill_line, remaining_mid_width, None, true);
                line.extend(adjusted);
            } else {
                line.extend(vec![Segment::new(" ".repeat(remaining_mid_width))]);
            }

            for (col_width, column) in &right_columns {
                let col_line = column.get(row).cloned().unwrap_or_else(|| {
                    vec![Segment::new(" ".repeat(*col_width))]
                });
                let adjusted = Segment::adjust_line_length(&col_line, *col_width, None, true);
                line.extend(adjusted);
            }

            middle_lines.push(line);
        }

        let mut out_lines: Vec<Vec<Segment>> = Vec::new();
        out_lines.extend(top_lines);
        out_lines.extend(middle_lines);
        out_lines.extend(bottom_lines);

        let line_count = out_lines.len();
        let mut out = Segments::new();
        for (idx, line) in out_lines.into_iter().enumerate() {
            out.extend(line);
            if idx + 1 < line_count {
                out.push(Segment::line());
            }
        }
        out
    }

    fn render_with_debug(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        debug: &DebugLayout,
    ) -> Segments {
        let mut remaining_width = options.size.0.max(1);
        let mut remaining_height = self
            .fixed_height
            .unwrap_or_else(|| options.size.1.max(1));

        let mut top_lines: Vec<Vec<Segment>> = Vec::new();
        let mut bottom_lines: Vec<Vec<Segment>> = Vec::new();

        let mut left_columns: Vec<(usize, Vec<Vec<Segment>>)> = Vec::new();
        let mut right_columns: Vec<(usize, Vec<Vec<Segment>>)> = Vec::new();
        let mut fill_lines: Option<Vec<Vec<Segment>>> = None;

        for (idx, item) in self.items.iter().enumerate() {
            match item.kind {
                DockKind::Top => {
                    let height = item
                        .size
                        .or_else(|| item.child.layout_height())
                        .unwrap_or(1)
                        .min(remaining_height);
                    let mut child_options = options.clone();
                    child_options.size = (remaining_width, height);
                    child_options.max_width = remaining_width;
                    child_options.max_height = height;
                    let segments = item.child.render(console, &child_options);
                    let mut lines =
                        Segment::split_and_crop_lines(segments, remaining_width, None, true, false);
                    lines = Segment::set_shape(&lines, remaining_width, Some(height), None, false);
                    let debug_height = (height + 2).max(3);
                    let label = if debug.show_sizes {
                        Some(format!("{remaining_width}x{debug_height}"))
                    } else {
                        None
                    };
                    let wrapped = apply_debug_box(
                        lines,
                        remaining_width,
                        debug_height,
                        label.as_deref(),
                        debug.style_for(idx),
                    );
                    top_lines.extend(wrapped);
                    remaining_height = remaining_height.saturating_sub(height);
                }
                DockKind::Bottom => {
                    let height = item
                        .size
                        .or_else(|| item.child.layout_height())
                        .unwrap_or(1)
                        .min(remaining_height);
                    let mut child_options = options.clone();
                    child_options.size = (remaining_width, height);
                    child_options.max_width = remaining_width;
                    child_options.max_height = height;
                    let segments = item.child.render(console, &child_options);
                    let mut lines =
                        Segment::split_and_crop_lines(segments, remaining_width, None, true, false);
                    lines = Segment::set_shape(&lines, remaining_width, Some(height), None, false);
                    let debug_height = (height + 2).max(3);
                    let label = if debug.show_sizes {
                        Some(format!("{remaining_width}x{debug_height}"))
                    } else {
                        None
                    };
                    let wrapped = apply_debug_box(
                        lines,
                        remaining_width,
                        debug_height,
                        label.as_deref(),
                        debug.style_for(idx),
                    );
                    bottom_lines.extend(wrapped);
                    remaining_height = remaining_height.saturating_sub(height);
                }
                DockKind::Left => {
                    let width = item.size.unwrap_or(1).min(remaining_width);
                    let mut child_options = options.clone();
                    child_options.size = (width, remaining_height);
                    child_options.max_width = width;
                    child_options.max_height = remaining_height;
                    let segments = item.child.render(console, &child_options);
                    let mut lines =
                        Segment::split_and_crop_lines(segments, width, None, true, false);
                    lines =
                        Segment::set_shape(&lines, width, Some(remaining_height), None, false);
                    let debug_height = (remaining_height + 2).max(3);
                    let label = if debug.show_sizes {
                        Some(format!("{width}x{debug_height}"))
                    } else {
                        None
                    };
                    let wrapped = apply_debug_box(
                        lines,
                        width,
                        debug_height,
                        label.as_deref(),
                        debug.style_for(idx),
                    );
                    left_columns.push((width, wrapped));
                    remaining_width = remaining_width.saturating_sub(width);
                }
                DockKind::Right => {
                    let width = item.size.unwrap_or(1).min(remaining_width);
                    let mut child_options = options.clone();
                    child_options.size = (width, remaining_height);
                    child_options.max_width = width;
                    child_options.max_height = remaining_height;
                    let segments = item.child.render(console, &child_options);
                    let mut lines =
                        Segment::split_and_crop_lines(segments, width, None, true, false);
                    lines =
                        Segment::set_shape(&lines, width, Some(remaining_height), None, false);
                    let debug_height = (remaining_height + 2).max(3);
                    let label = if debug.show_sizes {
                        Some(format!("{width}x{debug_height}"))
                    } else {
                        None
                    };
                    let wrapped = apply_debug_box(
                        lines,
                        width,
                        debug_height,
                        label.as_deref(),
                        debug.style_for(idx),
                    );
                    right_columns.push((width, wrapped));
                    remaining_width = remaining_width.saturating_sub(width);
                }
                DockKind::Fill => {
                    let mut child_options = options.clone();
                    child_options.size = (remaining_width, remaining_height);
                    child_options.max_width = remaining_width;
                    child_options.max_height = remaining_height;
                    let segments = item.child.render(console, &child_options);
                    let mut lines =
                        Segment::split_and_crop_lines(segments, remaining_width, None, true, false);
                    lines =
                        Segment::set_shape(&lines, remaining_width, Some(remaining_height), None, false);
                    let debug_height = (remaining_height + 2).max(3);
                    let label = if debug.show_sizes {
                        Some(format!("{remaining_width}x{debug_height}"))
                    } else {
                        None
                    };
                    let wrapped = apply_debug_box(
                        lines,
                        remaining_width,
                        debug_height,
                        label.as_deref(),
                        debug.style_for(idx),
                    );
                    fill_lines = Some(wrapped);
                }
            }
        }

        let mut middle_lines: Vec<Vec<Segment>> = Vec::new();
        for row in 0..remaining_height {
            let mut line: Vec<Segment> = Vec::new();

            for (col_width, column) in &left_columns {
                let col_line = column.get(row).cloned().unwrap_or_else(|| {
                    vec![Segment::new(" ".repeat(*col_width))]
                });
                let adjusted = Segment::adjust_line_length(&col_line, *col_width, None, true);
                line.extend(adjusted);
            }

            let remaining_mid_width = remaining_width;
            if let Some(lines) = &fill_lines {
                let fill_line = lines.get(row).cloned().unwrap_or_else(|| {
                    vec![Segment::new(" ".repeat(remaining_mid_width))]
                });
                let adjusted =
                    Segment::adjust_line_length(&fill_line, remaining_mid_width, None, true);
                line.extend(adjusted);
            } else {
                line.extend(vec![Segment::new(" ".repeat(remaining_mid_width))]);
            }

            for (col_width, column) in &right_columns {
                let col_line = column.get(row).cloned().unwrap_or_else(|| {
                    vec![Segment::new(" ".repeat(*col_width))]
                });
                let adjusted = Segment::adjust_line_length(&col_line, *col_width, None, true);
                line.extend(adjusted);
            }

            middle_lines.push(line);
        }

        let mut out_lines: Vec<Vec<Segment>> = Vec::new();
        out_lines.extend(top_lines);
        out_lines.extend(middle_lines);
        out_lines.extend(bottom_lines);

        let line_count = out_lines.len();
        let mut out = Segments::new();
        for (idx, line) in out_lines.into_iter().enumerate() {
            out.extend(line);
            if idx + 1 < line_count {
                out.push(Segment::line());
            }
        }
        out
    }

    fn on_mount(&mut self) {
        for item in &mut self.items {
            item.child.on_mount();
        }
    }

    fn on_unmount(&mut self) {
        for item in &mut self.items {
            item.child.on_unmount();
        }
    }

    fn on_tick(&mut self, tick: u64) {
        for item in &mut self.items {
            item.child.on_tick(tick);
        }
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        for item in &mut self.items {
            item.child.on_resize(width, height);
        }
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        for item in &mut self.items {
            item.child.on_event_capture(event, ctx);
            if ctx.handled() {
                break;
            }
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        for item in &mut self.items {
            item.child.on_event(event, ctx);
            if ctx.handled() {
                break;
            }
        }
    }

    fn layout_height(&self) -> Option<usize> {
        self.fixed_height
    }
}

impl Renderable for Dock {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[derive(Debug, Clone)]
pub struct Button {
    id: WidgetId,
    label: String,
    focused: bool,
    pressed: bool,
}

impl Button {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            id: WidgetId::new(),
            label: label.into(),
            focused: false,
            pressed: false,
        }
    }

    pub fn pressed(&self) -> bool {
        self.pressed
    }
}

impl Widget for Button {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn focusable(&self) -> bool {
        true
    }

    fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if !self.focused {
            return;
        }
        if let Event::Action(Action::Toggle) = event {
            self.pressed = !self.pressed;
            ctx.set_handled();
            return;
        }
        if let Event::Key(key) = event {
            match key.code {
                KeyCode::Enter | KeyCode::Char(' ') => {
                    self.pressed = !self.pressed;
                    ctx.set_handled();
                }
                _ => {}
            }
        }
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let marker = if self.focused { "> " } else { "  " };
        let state = if self.pressed { "[x]" } else { "[ ]" };
        let text = Text::plain(format!("{marker}{state} {}", self.label));
        text.render(console, options)
    }

    fn layout_height(&self) -> Option<usize> {
        Some(1)
    }
}

impl Renderable for Button {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[derive(Debug, Clone)]
pub struct ListView {
    id: WidgetId,
    items: Vec<String>,
    selected: usize,
    offset: usize,
    focused: bool,
}

impl ListView {
    pub fn new(items: Vec<String>) -> Self {
        Self {
            id: WidgetId::new(),
            items,
            selected: 0,
            offset: 0,
            focused: false,
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
}

impl Renderable for ListView {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[derive(Debug, Clone)]
pub struct DataTable {
    id: WidgetId,
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    selected: usize,
    offset: usize,
    focused: bool,
}

impl DataTable {
    pub fn new(headers: Vec<String>, rows: Vec<Vec<String>>) -> Self {
        Self {
            id: WidgetId::new(),
            headers,
            rows,
            selected: 0,
            offset: 0,
            focused: false,
        }
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn set_selected(&mut self, index: usize) {
        if self.rows.is_empty() {
            self.selected = 0;
            self.offset = 0;
            return;
        }
        self.selected = index.min(self.rows.len() - 1);
    }

    fn ensure_visible(&mut self, height: usize) {
        if self.rows.is_empty() || height == 0 {
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

impl Widget for DataTable {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn focusable(&self) -> bool {
        true
    }

    fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
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
                if self.selected + 1 < self.rows.len() {
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
                if self.selected + 1 < self.rows.len() {
                    let step = 5.min(self.rows.len().saturating_sub(1) - self.selected);
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
                    if self.selected + 1 < self.rows.len() {
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
                    if self.selected + 1 < self.rows.len() {
                        let step = 5.min(self.rows.len().saturating_sub(1) - self.selected);
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
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);
        let cols = self.headers.len().max(1);

        let sep_width = 3usize; // " | "
        let total_sep = sep_width.saturating_mul(cols.saturating_sub(1));
        let inner_width = width.saturating_sub(total_sep).max(1);
        let base = inner_width / cols;
        let rem = inner_width % cols;
        let col_widths: Vec<usize> = (0..cols)
            .map(|idx| base + if idx < rem { 1 } else { 0 })
            .collect();

        let mut lines: Vec<String> = Vec::new();
        let mut header_cells: Vec<String> = Vec::new();
        for (idx, header) in self.headers.iter().enumerate() {
            let w = col_widths.get(idx).copied().unwrap_or(1).max(1);
            header_cells.push(rich_rs::set_cell_size(header, w));
        }
        if !header_cells.is_empty() {
            lines.push(header_cells.join(" | "));
            let sep = header_cells
                .iter()
                .map(|h| "-".repeat(rich_rs::cell_len(h)))
                .collect::<Vec<_>>()
                .join("-+-");
            lines.push(sep);
        }

        let mut view = self.clone();
        let rows_available = height.saturating_sub(lines.len()).max(0);
        view.ensure_visible(rows_available);

        for (row_idx, row) in view.rows.iter().enumerate() {
            if row_idx < view.offset {
                continue;
            }
            if lines.len() >= height {
                break;
            }
            let mut cells: Vec<String> = Vec::new();
            for (idx, w) in col_widths.iter().enumerate() {
                let mut cell = row.get(idx).cloned().unwrap_or_default();
                if idx == 0 && row_idx == view.selected {
                    if *w >= 2 {
                        let marker = if self.focused { "> " } else { "* " };
                        cell = format!("{marker}{cell}");
                    }
                }
                cells.push(rich_rs::set_cell_size(&cell, *w));
            }
            lines.push(cells.join(" | "));
        }

        if lines.is_empty() {
            lines.push(String::new());
        }
        let text = Text::plain(lines.join("\n"));
        text.render(console, options)
    }
}

impl Renderable for DataTable {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[derive(Debug, Clone)]
pub struct TreeNode {
    label: String,
    children: Vec<TreeNode>,
    expanded: bool,
}

impl TreeNode {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            children: Vec::new(),
            expanded: true,
        }
    }

    pub fn with_child(mut self, child: TreeNode) -> Self {
        self.children.push(child);
        self
    }

    pub fn push(&mut self, child: TreeNode) {
        self.children.push(child);
    }

    pub fn expanded(mut self, expanded: bool) -> Self {
        self.expanded = expanded;
        self
    }
}

#[derive(Debug, Clone)]
pub struct Tree {
    id: WidgetId,
    roots: Vec<TreeNode>,
    selected: usize,
    offset: usize,
    focused: bool,
}

impl Tree {
    pub fn new(roots: Vec<TreeNode>) -> Self {
        Self {
            id: WidgetId::new(),
            roots,
            selected: 0,
            offset: 0,
            focused: false,
        }
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn set_selected(&mut self, index: usize) {
        let total = self.visible_count();
        if total == 0 {
            self.selected = 0;
            self.offset = 0;
            return;
        }
        self.selected = index.min(total - 1);
    }

    fn visible_count(&self) -> usize {
        fn walk(nodes: &[TreeNode], count: &mut usize) {
            for node in nodes {
                *count += 1;
                if node.expanded {
                    walk(&node.children, count);
                }
            }
        }
        let mut count = 0;
        walk(&self.roots, &mut count);
        count
    }

    fn ensure_visible(&mut self, height: usize) {
        if height == 0 {
            self.offset = 0;
            return;
        }
        let total = self.visible_count();
        if total == 0 {
            self.offset = 0;
            return;
        }
        if self.selected < self.offset {
            self.offset = self.selected;
        } else if self.selected >= self.offset + height {
            self.offset = self.selected + 1 - height;
        }
    }

    fn toggle_selected(&mut self) {
        let mut index = 0usize;
        if let Some(node) = node_mut_by_visible_index(&mut self.roots, self.selected, &mut index) {
            if !node.children.is_empty() {
                node.expanded = !node.expanded;
            }
        }
    }
}

impl Widget for Tree {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn focusable(&self) -> bool {
        true
    }

    fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
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
                let total = self.visible_count();
                if self.selected + 1 < total {
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
                let total = self.visible_count();
                if self.selected + 1 < total {
                    let step = 5.min(total.saturating_sub(1) - self.selected);
                    self.selected += step;
                }
                handled = true;
            }
            Event::Action(Action::Toggle) => {
                self.toggle_selected();
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
                    let total = self.visible_count();
                    if self.selected + 1 < total {
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
                    let total = self.visible_count();
                    if self.selected + 1 < total {
                        let step = 5.min(total.saturating_sub(1) - self.selected);
                        self.selected += step;
                    }
                    handled = true;
                }
                KeyCode::Left => {
                    let mut index = 0usize;
                    if let Some(node) =
                        node_mut_by_visible_index(&mut self.roots, self.selected, &mut index)
                    {
                        if node.expanded {
                            node.expanded = false;
                        }
                    }
                    handled = true;
                }
                KeyCode::Right => {
                    let mut index = 0usize;
                    if let Some(node) =
                        node_mut_by_visible_index(&mut self.roots, self.selected, &mut index)
                    {
                        if !node.children.is_empty() {
                            node.expanded = true;
                        }
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
        let mut index = 0usize;
        render_tree_lines(
            &view.roots,
            0,
            &mut index,
            view.selected,
            view.offset,
            height,
            view.focused,
            &mut lines,
        );

        if lines.is_empty() {
            lines.push(String::new());
        }
        let text = Text::plain(lines.join("\n"));
        text.render(console, options)
    }
}

impl Renderable for Tree {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

fn node_mut_by_visible_index<'a>(
    nodes: &'a mut [TreeNode],
    target: usize,
    index: &mut usize,
) -> Option<&'a mut TreeNode> {
    for node in nodes {
        if *index == target {
            return Some(node);
        }
        *index += 1;
        if node.expanded {
            if let Some(found) = node_mut_by_visible_index(&mut node.children, target, index) {
                return Some(found);
            }
        }
    }
    None
}

fn render_tree_lines(
    nodes: &[TreeNode],
    depth: usize,
    index: &mut usize,
    selected: usize,
    offset: usize,
    height: usize,
    focused: bool,
    lines: &mut Vec<String>,
) {
    for node in nodes {
        if lines.len() >= height {
            return;
        }
        if *index >= offset && lines.len() < height {
            let marker = if *index == selected {
                if focused { "> " } else { "* " }
            } else {
                "  "
            };
            let twist = if node.children.is_empty() {
                " "
            } else if node.expanded {
                "v"
            } else {
                ">"
            };
            let indent = "  ".repeat(depth);
            lines.push(format!("{marker}{indent}{twist} {}", node.label));
        }
        *index += 1;
        if node.expanded {
            render_tree_lines(
                &node.children,
                depth + 1,
                index,
                selected,
                offset,
                height,
                focused,
                lines,
            );
        }
    }
}

pub struct Tabs {
    id: WidgetId,
    tabs: Vec<Tab>,
    active: usize,
    focused: bool,
}

pub struct Tab {
    title: String,
    child: Box<dyn Widget>,
}

impl Tabs {
    pub fn new() -> Self {
        Self {
            id: WidgetId::new(),
            tabs: Vec::new(),
            active: 0,
            focused: false,
        }
    }

    pub fn with_tab(mut self, title: impl Into<String>, child: impl Widget + 'static) -> Self {
        self.tabs.push(Tab {
            title: title.into(),
            child: Box::new(child),
        });
        self
    }

    pub fn push(&mut self, title: impl Into<String>, child: impl Widget + 'static) {
        self.tabs.push(Tab {
            title: title.into(),
            child: Box::new(child),
        });
    }

    pub fn active(&self) -> usize {
        self.active
    }

    pub fn set_active(&mut self, index: usize) {
        if self.tabs.is_empty() {
            self.active = 0;
            return;
        }
        let next = index.min(self.tabs.len() - 1);
        if self.focused {
            if let Some(tab) = self.tabs.get_mut(self.active) {
                tab.child.set_focus(false);
            }
        }
        self.active = next;
        if self.focused {
            if let Some(tab) = self.tabs.get_mut(self.active) {
                tab.child.set_focus(true);
            }
        }
    }

    fn activate_prev(&mut self) {
        if self.tabs.is_empty() {
            return;
        }
        let next = if self.active == 0 {
            self.tabs.len() - 1
        } else {
            self.active - 1
        };
        self.set_active(next);
    }

    fn activate_next(&mut self) {
        if self.tabs.is_empty() {
            return;
        }
        let next = (self.active + 1) % self.tabs.len();
        self.set_active(next);
    }
}

impl Widget for Tabs {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn focusable(&self) -> bool {
        true
    }

    fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.child.set_focus(focused);
        }
    }

    fn on_mount(&mut self) {
        for tab in &mut self.tabs {
            tab.child.on_mount();
        }
    }

    fn on_unmount(&mut self) {
        for tab in &mut self.tabs {
            tab.child.on_unmount();
        }
    }

    fn on_tick(&mut self, tick: u64) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.child.on_tick(tick);
        }
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.child.on_resize(width, height);
        }
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.child.on_event_capture(event, ctx);
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if self.focused {
            if let Event::Key(key) = event {
                match key.code {
                    KeyCode::Left => {
                        self.activate_prev();
                        ctx.set_handled();
                        return;
                    }
                    KeyCode::Right => {
                        self.activate_next();
                        ctx.set_handled();
                        return;
                    }
                    KeyCode::Char('h') => {
                        self.activate_prev();
                        ctx.set_handled();
                        return;
                    }
                    KeyCode::Char('l') => {
                        self.activate_next();
                        ctx.set_handled();
                        return;
                    }
                    _ => {}
                }
            }
        }
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.child.on_event(event, ctx);
        }
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);

        let header = if self.tabs.is_empty() {
            "no tabs".to_string()
        } else {
            let mut parts = Vec::new();
            for (idx, tab) in self.tabs.iter().enumerate() {
                if idx == self.active {
                    parts.push(format!("[{}]", tab.title));
                } else {
                    parts.push(format!(" {} ", tab.title));
                }
            }
            parts.join(" ")
        };
        let header_line = rich_rs::set_cell_size(&header, width);
        let header_segments = Text::plain(header_line).render(console, options);
        let mut lines = Segment::split_and_crop_lines(header_segments, width, None, true, false);
        lines = Segment::set_shape(&lines, width, Some(1), None, false);

        if height > 1 {
            if let Some(tab) = self.tabs.get(self.active) {
                let mut child_options = options.clone();
                child_options.size = (width, height - 1);
                child_options.max_width = width;
                child_options.max_height = height - 1;
                let child_segments = tab.child.render(console, &child_options);
                let mut child_lines =
                    Segment::split_and_crop_lines(child_segments, width, None, true, false);
                child_lines =
                    Segment::set_shape(&child_lines, width, Some(height - 1), None, false);
                lines.extend(child_lines);
            }
        }

        let line_count = lines.len();
        let mut out = Segments::new();
        for (idx, line) in lines.into_iter().enumerate() {
            out.extend(line);
            if idx + 1 < line_count {
                out.push(Segment::line());
            }
        }
        out
    }

    fn layout_height(&self) -> Option<usize> {
        let child_height = self
            .tabs
            .get(self.active)
            .and_then(|tab| tab.child.layout_height());
        child_height.map(|height| height + 1)
    }
}

impl Renderable for Tabs {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[derive(Debug, Clone)]
pub struct Markdown {
    id: WidgetId,
    markup: String,
}

impl Markdown {
    pub fn new(markup: impl Into<String>) -> Self {
        Self {
            id: WidgetId::new(),
            markup: markup.into(),
        }
    }

    pub fn set_markup(&mut self, markup: impl Into<String>) {
        self.markup = markup.into();
    }
}

impl Widget for Markdown {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        RichMarkdown::new(self.markup.clone()).render(console, options)
    }
}

impl Renderable for Markdown {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[derive(Debug, Clone)]
pub struct Checkbox {
    id: WidgetId,
    label: String,
    checked: bool,
    focused: bool,
}

impl Checkbox {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            id: WidgetId::new(),
            label: label.into(),
            checked: false,
            focused: false,
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
        Some(1)
    }
}

impl Renderable for Checkbox {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[derive(Debug, Clone)]
pub struct Input {
    id: WidgetId,
    text: String,
    cursor: usize,
    focused: bool,
    placeholder: Option<String>,
}

impl Input {
    pub fn new() -> Self {
        Self {
            id: WidgetId::new(),
            text: String::new(),
            cursor: 0,
            focused: false,
            placeholder: None,
        }
    }

    pub fn with_placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    pub fn value(&self) -> &str {
        &self.text
    }

    pub fn set_value(&mut self, value: impl Into<String>) {
        self.text = value.into();
        self.cursor = self.text.len();
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
        Some(1)
    }
}

impl Renderable for Input {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

pub struct AppRoot {
    id: WidgetId,
    children: Vec<Box<dyn Widget>>,
    focused: Option<usize>,
}

impl AppRoot {
    pub fn new() -> Self {
        Self {
            id: WidgetId::new(),
            children: Vec::new(),
            focused: None,
        }
    }

    pub fn with_child(mut self, child: impl Widget + 'static) -> Self {
        self.children.push(Box::new(child));
        self
    }

    pub fn push(&mut self, child: impl Widget + 'static) {
        self.children.push(Box::new(child));
    }

    pub fn focus_first(&mut self) {
        self.focused = None;
        for (idx, child) in self.children.iter_mut().enumerate() {
            if child.focusable() {
                child.set_focus(true);
                self.focused = Some(idx);
                break;
            }
        }
    }

    pub fn focus_next(&mut self) {
        if self.children.is_empty() {
            return;
        }
        let start = self.focused.unwrap_or(usize::MAX);
        if let Some(idx) = self.focused {
            self.children[idx].set_focus(false);
        }
        let mut i = if start == usize::MAX { 0 } else { (start + 1) % self.children.len() };
        let mut visited = 0;
        while visited < self.children.len() {
            if self.children[i].focusable() {
                self.children[i].set_focus(true);
                self.focused = Some(i);
                return;
            }
            i = (i + 1) % self.children.len();
            visited += 1;
        }
        self.focused = None;
    }

    pub fn focus_prev(&mut self) {
        if self.children.is_empty() {
            return;
        }
        let start = self.focused.unwrap_or(0);
        if let Some(idx) = self.focused {
            self.children[idx].set_focus(false);
        }
        let mut i = if start == 0 { self.children.len() - 1 } else { start - 1 };
        let mut visited = 0;
        while visited < self.children.len() {
            if self.children[i].focusable() {
                self.children[i].set_focus(true);
                self.focused = Some(i);
                return;
            }
            if i == 0 {
                i = self.children.len() - 1;
            } else {
                i -= 1;
            }
            visited += 1;
        }
        self.focused = None;
    }

    pub fn focus(&mut self, id: WidgetId) -> bool {
        let target = self
            .children
            .iter()
            .enumerate()
            .find(|(_, child)| child.id() == id && child.focusable())
            .map(|(idx, _)| idx);

        if let Some(idx) = target {
            if let Some(cur) = self.focused {
                self.children[cur].set_focus(false);
            }
            self.children[idx].set_focus(true);
            self.focused = Some(idx);
            return true;
        }

        false
    }
}

impl Default for AppRoot {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for AppRoot {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height_limit = options.size.1.max(1);
        let bounds = rich_rs::Region::from_size(width as u32, height_limit as u32);

        let mut lines: Vec<Vec<Segment>> = Vec::new();
        let mut cursor_y: i32 = 0;

        for child in &self.children {
            let segments = child.render(console, options);
            let mut child_lines =
                Segment::split_and_crop_lines(segments, width, None, true, false);
            if let Some(height) = child.layout_height() {
                child_lines = Segment::set_shape(&child_lines, width, Some(height), None, false);
            }
            let child_height = child_lines.len();
            let child_region =
                rich_rs::Region::new(0, cursor_y, width as u32, child_height as u32);
            if let Some(visible) = child_region.intersection(&bounds) {
                let start = (visible.y - child_region.y).max(0) as usize;
                let end = (start + visible.height as usize).min(child_lines.len());
                for line in child_lines.into_iter().skip(start).take(end - start) {
                    if lines.len() >= height_limit {
                        break;
                    }
                    lines.push(line);
                }
            }
            cursor_y += child_height as i32;
            if cursor_y as usize >= height_limit {
                break;
            }
        }

        let line_count = lines.len();
        let mut out = Segments::new();
        for (idx, line) in lines.into_iter().enumerate() {
            out.extend(line);
            if idx + 1 < line_count {
                out.push(Segment::line());
            }
        }
        out
    }

    fn render_with_debug(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        debug: &DebugLayout,
    ) -> Segments {
        let width = options.size.0.max(1);
        let height_limit = options.size.1.max(1);
        let bounds = rich_rs::Region::from_size(width as u32, height_limit as u32);

        let mut lines: Vec<Vec<Segment>> = Vec::new();
        let mut cursor_y: i32 = 0;

        for (idx, child) in self.children.iter().enumerate() {
            let segments = child.render(console, options);
            let mut child_lines =
                Segment::split_and_crop_lines(segments, width, None, true, false);
            if let Some(height) = child.layout_height() {
                child_lines = Segment::set_shape(&child_lines, width, Some(height), None, false);
            }
            let child_height = child_lines.len().max(1);
            let debug_height = (child_height + 2).max(3);
            let child_region =
                rich_rs::Region::new(0, cursor_y, width as u32, debug_height as u32);
            if let Some(visible) = child_region.intersection(&bounds) {
                let start = (visible.y - child_region.y).max(0) as usize;
                let end = (start + visible.height as usize).min(debug_height);
                let label = if debug.show_sizes {
                    Some(format!("{width}x{debug_height}"))
                } else {
                    None
                };
                let wrapped = apply_debug_box(
                    child_lines,
                    width,
                    debug_height,
                    label.as_deref(),
                    debug.style_for(idx),
                );
                for line in wrapped.into_iter().skip(start).take(end - start) {
                    if lines.len() >= height_limit {
                        break;
                    }
                    lines.push(line);
                }
            }
            cursor_y += debug_height as i32;
            if cursor_y as usize >= height_limit {
                break;
            }
        }

        let line_count = lines.len();
        let mut out = Segments::new();
        for (idx, line) in lines.into_iter().enumerate() {
            out.extend(line);
            if idx + 1 < line_count {
                out.push(Segment::line());
            }
        }
        out
    }

    fn on_mount(&mut self) {
        for child in &mut self.children {
            child.on_mount();
        }
        self.focus_first();
    }

    fn on_unmount(&mut self) {
        for child in &mut self.children {
            child.on_unmount();
        }
    }

    fn on_tick(&mut self, tick: u64) {
        for child in &mut self.children {
            child.on_tick(tick);
        }
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        for child in &mut self.children {
            child.on_resize(width, height);
        }
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        for child in &mut self.children {
            child.on_event_capture(event, ctx);
            if ctx.handled() {
                break;
            }
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if let Event::Action(action) = event {
            match action {
                Action::FocusNext => {
                    self.focus_next();
                    ctx.set_handled();
                    return;
                }
                Action::FocusPrev => {
                    self.focus_prev();
                    ctx.set_handled();
                    return;
                }
                _ => {}
            }
        }
        if let Event::Key(key) = event {
            if key.code == KeyCode::Tab {
                self.focus_next();
                ctx.set_handled();
                return;
            }
        }

        if let Some(idx) = self.focused {
            self.children[idx].on_event(event, ctx);
            if ctx.handled() {
                return;
            }
        }

        for child in &mut self.children {
            child.on_event(event, ctx);
            if ctx.handled() {
                break;
            }
        }
    }
}

impl Renderable for AppRoot {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

pub struct Frame {
    id: WidgetId,
    child: Box<dyn Widget>,
    padding: usize,
    border: bool,
}

impl Frame {
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            id: WidgetId::new(),
            child: Box::new(child),
            padding: 1,
            border: true,
        }
    }

    pub fn padding(mut self, padding: usize) -> Self {
        self.padding = padding;
        self
    }

    pub fn border(mut self, border: bool) -> Self {
        self.border = border;
        self
    }
}

impl Widget for Frame {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let border_width = if self.border { 1 } else { 0 };
        let total_padding = self.padding * 2;

        let width = options.size.0.max(1);
        let height = options.size.1.max(1);

        let inner_width = width.saturating_sub(border_width * 2 + total_padding).max(1);
        let mut inner_height = height.saturating_sub(border_width * 2 + total_padding).max(1);
        if let Some(child_height) = self.child.layout_height() {
            inner_height = inner_height.min(child_height.max(1));
        }

        let mut child_options = options.clone();
        child_options.size = (inner_width, inner_height);
        child_options.max_width = inner_width;
        child_options.max_height = inner_height;

        let child_segments = self.child.render(console, &child_options);
        let mut child_lines =
            Segment::split_and_crop_lines(child_segments, inner_width, None, true, false);
        if let Some(height) = self.child.layout_height() {
            let capped = height.min(inner_height);
            child_lines = Segment::set_shape(&child_lines, inner_width, Some(capped), None, false);
        }

        let padding_line = vec![Segment::new(" ".repeat(inner_width))];
        let mut content_lines: Vec<Vec<Segment>> = Vec::new();
        for _ in 0..self.padding {
            content_lines.push(padding_line.clone());
        }
        content_lines.extend(child_lines.into_iter());
        for _ in 0..self.padding {
            content_lines.push(padding_line.clone());
        }
        content_lines =
            Segment::set_shape(&content_lines, inner_width, Some(inner_height + total_padding), None, false);

        let inner_total = inner_width + total_padding;
        let mut out = Segments::new();
        let line_count = content_lines.len();

        if self.border {
            let b = rich_rs::r#box::SQUARE;
            let top = format!(
                "{}{}{}",
                b.top_left,
                std::iter::repeat(b.top).take(inner_total).collect::<String>(),
                b.top_right
            );
            out.push(Segment::new(top));
            out.push(Segment::line());

            for (idx, line) in content_lines.into_iter().enumerate() {
                out.push(Segment::new(b.mid_left.to_string()));
                if self.padding > 0 {
                    out.push(Segment::new(" ".repeat(self.padding)));
                }
                let adjusted = Segment::adjust_line_length(&line, inner_width, None, true);
                out.extend(adjusted);
                if self.padding > 0 {
                    out.push(Segment::new(" ".repeat(self.padding)));
                }
                out.push(Segment::new(b.mid_right.to_string()));
                if idx + 1 < line_count {
                    out.push(Segment::line());
                }
            }

            let bottom = format!(
                "{}{}{}",
                b.bottom_left,
                std::iter::repeat(b.bottom).take(inner_total).collect::<String>(),
                b.bottom_right
            );
            out.push(Segment::line());
            out.push(Segment::new(bottom));
        } else {
            for (idx, line) in content_lines.into_iter().enumerate() {
                let adjusted = Segment::adjust_line_length(&line, inner_total, None, true);
                out.extend(adjusted);
                if idx + 1 < line_count {
                    out.push(Segment::line());
                }
            }
        }

        out
    }

    fn render_with_debug(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        debug: &DebugLayout,
    ) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);
        let segments = Widget::render(self, console, options);
        let mut lines = Segment::split_and_crop_lines(segments, width, None, true, false);
        let label = if debug.show_sizes {
            Some(format!("{width}x{height}"))
        } else {
            None
        };
        lines = apply_debug_box(lines, width, height, label.as_deref(), debug.style_for(0));
        let line_count = lines.len();
        let mut out = Segments::new();
        for (idx, line) in lines.into_iter().enumerate() {
            out.extend(line);
            if idx + 1 < line_count {
                out.push(Segment::line());
            }
        }
        out
    }

    fn on_mount(&mut self) {
        self.child.on_mount();
    }

    fn on_unmount(&mut self) {
        self.child.on_unmount();
    }

    fn on_tick(&mut self, tick: u64) {
        self.child.on_tick(tick);
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        self.child.on_resize(width, height);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.child.on_event_capture(event, ctx);
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.child.on_event(event, ctx);
    }

    fn layout_height(&self) -> Option<usize> {
        self.child
            .layout_height()
            .map(|h| h + self.padding * 2 + if self.border { 2 } else { 0 })
    }

    fn focusable(&self) -> bool {
        self.child.focusable()
    }

    fn set_focus(&mut self, focused: bool) {
        self.child.set_focus(focused);
    }
}

impl Renderable for Frame {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

pub struct ScrollView {
    id: WidgetId,
    child: Box<dyn Widget>,
    height: Option<usize>,
    offset_y: usize,
    scroll_step: usize,
}

impl ScrollView {
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            id: WidgetId::new(),
            child: Box::new(child),
            height: None,
            offset_y: 0,
            scroll_step: 1,
        }
    }

    pub fn height(mut self, height: usize) -> Self {
        self.height = Some(height.max(1));
        self
    }

    pub fn scroll_to(&mut self, offset_y: usize) {
        self.offset_y = offset_y;
    }

    pub fn scroll_by(&mut self, delta: i32) {
        if delta.is_negative() {
            self.offset_y = self.offset_y.saturating_sub(delta.unsigned_abs() as usize);
        } else {
            self.offset_y = self.offset_y.saturating_add(delta as usize);
        }
    }

    pub fn scroll_step(mut self, step: usize) -> Self {
        self.scroll_step = step.max(1);
        self
    }
}

impl Widget for ScrollView {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let viewport_height = self.height.unwrap_or_else(|| options.size.1.max(1));

        let mut child_options = options.clone();
        child_options.size = (width, viewport_height.saturating_add(self.offset_y).max(1));
        child_options.max_width = width;
        child_options.max_height = child_options.size.1;

        let segments = self.child.render(console, &child_options);
        let mut lines = Segment::split_and_crop_lines(segments, width, None, true, false);
        if let Some(height) = self.child.layout_height() {
            let capped = height.max(viewport_height + self.offset_y);
            lines = Segment::set_shape(&lines, width, Some(capped), None, false);
        }

        let start = self.offset_y.min(lines.len());
        let end = (start + viewport_height).min(lines.len());
        let slice = lines[start..end].to_vec();
        let slice = Segment::set_shape(&slice, width, Some(viewport_height), None, false);

        let line_count = slice.len();
        let mut out = Segments::new();
        for (idx, line) in slice.into_iter().enumerate() {
            out.extend(line);
            if idx + 1 < line_count {
                out.push(Segment::line());
            }
        }
        out
    }

    fn render_with_debug(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        debug: &DebugLayout,
    ) -> Segments {
        let width = options.size.0.max(1);
        let height = self.height.unwrap_or_else(|| options.size.1.max(1));
        let segments = Widget::render(self, console, options);
        let mut lines = Segment::split_and_crop_lines(segments, width, None, true, false);
        let label = if debug.show_sizes {
            Some(format!("{width}x{height}"))
        } else {
            None
        };
        lines = apply_debug_box(lines, width, height.max(3), label.as_deref(), debug.style_for(0));
        let line_count = lines.len();
        let mut out = Segments::new();
        for (idx, line) in lines.into_iter().enumerate() {
            out.extend(line);
            if idx + 1 < line_count {
                out.push(Segment::line());
            }
        }
        out
    }

    fn on_mount(&mut self) {
        self.child.on_mount();
    }

    fn on_unmount(&mut self) {
        self.child.on_unmount();
    }

    fn on_tick(&mut self, tick: u64) {
        self.child.on_tick(tick);
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        self.child.on_resize(width, height);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.child.on_event_capture(event, ctx);
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if let Event::Action(action) = event {
            match action {
                Action::ScrollUp => {
                    self.scroll_by(-(self.scroll_step as i32));
                    ctx.set_handled();
                    return;
                }
                Action::ScrollDown => {
                    self.scroll_by(self.scroll_step as i32);
                    ctx.set_handled();
                    return;
                }
                Action::ScrollPageUp => {
                    let page = self.height.unwrap_or(1).max(1);
                    self.scroll_by(-(page as i32));
                    ctx.set_handled();
                    return;
                }
                Action::ScrollPageDown => {
                    let page = self.height.unwrap_or(1).max(1);
                    self.scroll_by(page as i32);
                    ctx.set_handled();
                    return;
                }
                _ => {}
            }
        }
        self.child.on_event(event, ctx);
    }

    fn layout_height(&self) -> Option<usize> {
        self.height
    }
}

impl Renderable for ScrollView {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

pub struct Grid {
    id: WidgetId,
    rows: usize,
    cols: usize,
    cells: Vec<Option<Box<dyn Widget>>>,
    row_gaps: usize,
    col_gaps: usize,
    row_sizes: Option<Vec<usize>>,
    col_sizes: Option<Vec<usize>>,
}

impl Grid {
    pub fn new(rows: usize, cols: usize) -> Self {
        let rows = rows.max(1);
        let cols = cols.max(1);
        Self {
            id: WidgetId::new(),
            rows,
            cols,
            cells: (0..rows * cols).map(|_| None).collect(),
            row_gaps: 0,
            col_gaps: 0,
            row_sizes: None,
            col_sizes: None,
        }
    }

    pub fn set(&mut self, row: usize, col: usize, child: impl Widget + 'static) {
        if row >= self.rows || col >= self.cols {
            return;
        }
        let idx = row * self.cols + col;
        self.cells[idx] = Some(Box::new(child));
    }

    pub fn with_cell(mut self, row: usize, col: usize, child: impl Widget + 'static) -> Self {
        self.set(row, col, child);
        self
    }

    pub fn row_gap(mut self, gap: usize) -> Self {
        self.row_gaps = gap;
        self
    }

    pub fn col_gap(mut self, gap: usize) -> Self {
        self.col_gaps = gap;
        self
    }

    pub fn row_sizes(mut self, sizes: Vec<usize>) -> Self {
        if sizes.len() == self.rows {
            self.row_sizes = Some(sizes);
        }
        self
    }

    pub fn col_sizes(mut self, sizes: Vec<usize>) -> Self {
        if sizes.len() == self.cols {
            self.col_sizes = Some(sizes);
        }
        self
    }
}

impl Widget for Grid {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);

        let total_col_gaps = self.col_gaps.saturating_mul(self.cols.saturating_sub(1));
        let total_row_gaps = self.row_gaps.saturating_mul(self.rows.saturating_sub(1));
        let inner_width = width.saturating_sub(total_col_gaps).max(1);
        let inner_height = height.saturating_sub(total_row_gaps).max(1);

        let col_widths: Vec<usize> = if let Some(sizes) = &self.col_sizes {
            sizes.clone()
        } else {
            let base_w = inner_width / self.cols;
            let rem_w = inner_width % self.cols;
            (0..self.cols)
                .map(|c| base_w + if c < rem_w { 1 } else { 0 })
                .collect()
        };

        let row_heights: Vec<usize> = if let Some(sizes) = &self.row_sizes {
            sizes.clone()
        } else {
            let base_h = inner_height / self.rows;
            let rem_h = inner_height % self.rows;
            (0..self.rows)
                .map(|r| base_h + if r < rem_h { 1 } else { 0 })
                .collect()
        };

        let mut cell_lines: Vec<Vec<Vec<Vec<Segment>>>> = Vec::new();
        for r in 0..self.rows {
            let mut row_cells = Vec::new();
            for c in 0..self.cols {
                let idx = r * self.cols + c;
                let cell_width = col_widths[c].max(1);
                let cell_height = row_heights[r].max(1);
                let mut child_options = options.clone();
                child_options.size = (cell_width, cell_height);
                child_options.max_width = cell_width;
                child_options.max_height = cell_height;
                let lines = if let Some(child) = &self.cells[idx] {
                    let segments = child.render(console, &child_options);
                    let mut lines =
                        Segment::split_and_crop_lines(segments, cell_width, None, true, false);
                    lines = Segment::set_shape(&lines, cell_width, Some(cell_height), None, false);
                    lines
                } else {
                    Segment::set_shape(&[], cell_width, Some(cell_height), None, false)
                };
                row_cells.push(lines);
            }
            cell_lines.push(row_cells);
        }

        let mut out_lines: Vec<Vec<Segment>> = Vec::new();
        for r in 0..self.rows {
            let cell_height = row_heights[r].max(1);
            for row in 0..cell_height {
                let mut line: Vec<Segment> = Vec::new();
                for c in 0..self.cols {
                    let cell_width = col_widths[c].max(1);
                    let lines = &cell_lines[r][c];
                    let cell_line = lines.get(row).cloned().unwrap_or_else(|| {
                        vec![Segment::new(" ".repeat(cell_width))]
                    });
                    let adjusted = Segment::adjust_line_length(&cell_line, cell_width, None, true);
                    line.extend(adjusted);
                    if c + 1 < self.cols && self.col_gaps > 0 {
                        line.push(Segment::new(" ".repeat(self.col_gaps)));
                    }
                }
                out_lines.push(line);
            }
            if r + 1 < self.rows && self.row_gaps > 0 {
                let gap_line = vec![Segment::new(" ".repeat(width))];
                for _ in 0..self.row_gaps {
                    out_lines.push(gap_line.clone());
                }
            }
        }

        let line_count = out_lines.len();
        let mut out = Segments::new();
        for (idx, line) in out_lines.into_iter().enumerate() {
            out.extend(line);
            if idx + 1 < line_count {
                out.push(Segment::line());
            }
        }
        out
    }

    fn render_with_debug(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        debug: &DebugLayout,
    ) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);

        let total_col_gaps = self.col_gaps.saturating_mul(self.cols.saturating_sub(1));
        let total_row_gaps = self.row_gaps.saturating_mul(self.rows.saturating_sub(1));
        let inner_width = width.saturating_sub(total_col_gaps).max(1);
        let inner_height = height.saturating_sub(total_row_gaps).max(1);

        let col_widths: Vec<usize> = if let Some(sizes) = &self.col_sizes {
            sizes.clone()
        } else {
            let base_w = inner_width / self.cols;
            let rem_w = inner_width % self.cols;
            (0..self.cols)
                .map(|c| base_w + if c < rem_w { 1 } else { 0 })
                .collect()
        };

        let row_heights: Vec<usize> = if let Some(sizes) = &self.row_sizes {
            sizes.clone()
        } else {
            let base_h = inner_height / self.rows;
            let rem_h = inner_height % self.rows;
            (0..self.rows)
                .map(|r| base_h + if r < rem_h { 1 } else { 0 })
                .collect()
        };

        let mut cell_lines: Vec<Vec<Vec<Vec<Segment>>>> = Vec::new();
        let mut cell_index = 0;
        for r in 0..self.rows {
            let mut row_cells = Vec::new();
            for c in 0..self.cols {
                let idx = r * self.cols + c;
                let cell_width = col_widths[c].max(1);
                let cell_height = row_heights[r].max(1);
                let mut child_options = options.clone();
                child_options.size = (cell_width, cell_height);
                child_options.max_width = cell_width;
                child_options.max_height = cell_height;
                let lines = if let Some(child) = &self.cells[idx] {
                    let segments = child.render(console, &child_options);
                    let mut lines =
                        Segment::split_and_crop_lines(segments, cell_width, None, true, false);
                    lines = Segment::set_shape(&lines, cell_width, Some(cell_height), None, false);
                    let label = if debug.show_sizes {
                        Some(format!("{cell_width}x{cell_height}"))
                    } else {
                        None
                    };
                    apply_debug_box(
                        lines,
                        cell_width,
                        (cell_height + 2).max(3),
                        label.as_deref(),
                        debug.style_for(cell_index),
                    )
                } else {
                    Segment::set_shape(&[], cell_width, Some(cell_height), None, false)
                };
                row_cells.push(lines);
                cell_index += 1;
            }
            cell_lines.push(row_cells);
        }

        let mut out_lines: Vec<Vec<Segment>> = Vec::new();
        for r in 0..self.rows {
            let cell_height = row_heights[r].max(1);
            for row in 0..cell_height {
                let mut line: Vec<Segment> = Vec::new();
                for c in 0..self.cols {
                    let cell_width = col_widths[c].max(1);
                    let lines = &cell_lines[r][c];
                    let cell_line = lines.get(row).cloned().unwrap_or_else(|| {
                        vec![Segment::new(" ".repeat(cell_width))]
                    });
                    let adjusted = Segment::adjust_line_length(&cell_line, cell_width, None, true);
                    line.extend(adjusted);
                    if c + 1 < self.cols && self.col_gaps > 0 {
                        line.push(Segment::new(" ".repeat(self.col_gaps)));
                    }
                }
                out_lines.push(line);
            }
            if r + 1 < self.rows && self.row_gaps > 0 {
                let gap_line = vec![Segment::new(" ".repeat(width))];
                for _ in 0..self.row_gaps {
                    out_lines.push(gap_line.clone());
                }
            }
        }

        let line_count = out_lines.len();
        let mut out = Segments::new();
        for (idx, line) in out_lines.into_iter().enumerate() {
            out.extend(line);
            if idx + 1 < line_count {
                out.push(Segment::line());
            }
        }
        out
    }

    fn on_mount(&mut self) {
        for cell in &mut self.cells {
            if let Some(child) = cell {
                child.on_mount();
            }
        }
    }

    fn on_unmount(&mut self) {
        for cell in &mut self.cells {
            if let Some(child) = cell {
                child.on_unmount();
            }
        }
    }

    fn on_tick(&mut self, tick: u64) {
        for cell in &mut self.cells {
            if let Some(child) = cell {
                child.on_tick(tick);
            }
        }
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        for cell in &mut self.cells {
            if let Some(child) = cell {
                child.on_resize(width, height);
            }
        }
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        for cell in &mut self.cells {
            if let Some(child) = cell {
                child.on_event_capture(event, ctx);
                if ctx.handled() {
                    break;
                }
            }
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        for cell in &mut self.cells {
            if let Some(child) = cell {
                child.on_event(event, ctx);
                if ctx.handled() {
                    break;
                }
            }
        }
    }
}

impl Renderable for Grid {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

pub struct Overlay {
    id: WidgetId,
    base: Box<dyn Widget>,
    modal: Box<dyn Widget>,
    visible: bool,
}

impl Overlay {
    pub fn new(base: impl Widget + 'static, modal: impl Widget + 'static) -> Self {
        Self {
            id: WidgetId::new(),
            base: Box::new(base),
            modal: Box::new(modal),
            visible: true,
        }
    }

    pub fn visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }
}

impl Widget for Overlay {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        if !self.visible {
            return self.base.render(console, options);
        }
        let base_renderable = WidgetRenderable::new(self.base.as_ref());
        let modal_renderable = WidgetRenderable::new(self.modal.as_ref());
        let base = crate::render::FrameBuffer::from_renderable(
            console,
            options,
            &base_renderable,
            None,
        );
        let top = crate::render::FrameBuffer::from_renderable(
            console,
            options,
            &modal_renderable,
            None,
        );
        let mut merged = base.clone();
        for y in 0..base.height {
            for x in 0..base.width {
                let cell = top.get(x, y);
                if !cell.continuation && !cell.text.is_empty() && cell.text != " " {
                    let out = merged.get_mut(x, y);
                    *out = cell.clone();
                }
            }
        }
        let lines = merged.as_plain_lines().join("\n");
        Text::plain(lines).render(console, options)
    }

    fn on_mount(&mut self) {
        self.base.on_mount();
        self.modal.on_mount();
    }

    fn on_unmount(&mut self) {
        self.base.on_unmount();
        self.modal.on_unmount();
    }

    fn on_tick(&mut self, tick: u64) {
        self.base.on_tick(tick);
        self.modal.on_tick(tick);
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        self.base.on_resize(width, height);
        self.modal.on_resize(width, height);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.modal.on_event_capture(event, ctx);
        if !ctx.handled() {
            self.base.on_event_capture(event, ctx);
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.modal.on_event(event, ctx);
        if !ctx.handled() {
            self.base.on_event(event, ctx);
        }
    }
}

impl Renderable for Overlay {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

fn apply_debug_box(
    lines: Vec<Vec<Segment>>,
    width: usize,
    height: usize,
    label: Option<&str>,
    style: rich_rs::Style,
) -> Vec<Vec<Segment>> {
    if width < 3 || height < 3 {
        return lines;
    }

    let b = rich_rs::r#box::SQUARE;
    let mut out: Vec<Vec<Segment>> = Vec::new();

    let mut top = String::new();
    top.push(b.top_left);
    let mut label_text = String::new();
    if let Some(text) = label {
        for ch in text.chars() {
            label_text.push(ch);
            if rich_rs::cell_len(&label_text) > width - 2 {
                label_text.pop();
                break;
            }
        }
    }
    let label_width = rich_rs::cell_len(&label_text);
    let fill_width = (width - 2).saturating_sub(label_width);
    top.push_str(&label_text);
    top.push_str(&std::iter::repeat(b.top).take(fill_width).collect::<String>());
    top.push(b.top_right);
    out.push(vec![Segment::styled(top, style)]);

    let mut content = lines;
    content = Segment::set_shape(&content, width - 2, Some(height - 2), None, false);

    for line in content.into_iter().take(height - 2) {
        let mut row: Vec<Segment> = Vec::new();
        row.push(Segment::styled(b.mid_left.to_string(), style));
        let inner = Segment::adjust_line_length(&line, width - 2, None, true);
        row.extend(inner);
        row.push(Segment::styled(b.mid_right.to_string(), style));
        out.push(row);
    }

    let mut bottom = String::new();
    bottom.push(b.bottom_left);
    bottom.push_str(&std::iter::repeat(b.bottom).take(width - 2).collect::<String>());
    bottom.push(b.bottom_right);
    out.push(vec![Segment::styled(bottom, style)]);

    out
}
