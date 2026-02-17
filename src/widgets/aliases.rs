use std::sync::atomic::{AtomicUsize, Ordering};

use rich_rs::{Console, ConsoleOptions, Segment, Segments};

use crate::event::{Action, Event, EventCtx};
use crate::node_id::NodeId;

use super::helpers::adjust_line_length_no_bg;
use super::helpers::{clamp_with_constraints, crop_line_horizontal, pad_lines_to_width};
use super::{Container, Grid, Node, Row, RowAlign, Widget, WidgetStyles};
use crate::compose::ComposeResult;
use crate::widgets::containers::ScrollView;

fn align_line_horizontal(
    line: &[Segment],
    width: usize,
    child_width: usize,
    offset: usize,
) -> Vec<Segment> {
    let width = width.max(1);
    let child_width = child_width.max(1).min(width);
    let offset = offset.min(width.saturating_sub(child_width));
    let mut out = Vec::new();
    if offset > 0 {
        out.push(Segment::new(" ".repeat(offset)));
    }
    out.extend(adjust_line_length_no_bg(line, child_width));
    let tail = width.saturating_sub(offset + child_width);
    if tail > 0 {
        out.push(Segment::new(" ".repeat(tail)));
    }
    out
}

fn effective_rendered_height(lines: &[Vec<Segment>]) -> usize {
    let last_non_blank = lines.iter().rposition(|line| {
        line.iter()
            .filter(|segment| !segment.is_control())
            .any(|segment| segment.text.chars().any(|ch| ch != ' '))
    });
    last_non_blank.map(|idx| idx + 1).unwrap_or(1)
}

pub struct Horizontal {
    row: Row,
}

impl Horizontal {
    pub fn new() -> Self {
        Self { row: Row::new() }
    }

    pub fn with_child(mut self, child: impl Widget + 'static) -> Self {
        self.row = self.row.with_child(child);
        self
    }

    /// Add multiple children from a `compose![]` result.
    pub fn with_compose(mut self, children: ComposeResult) -> Self {
        self.row = self.row.with_compose(children);
        self
    }

    pub fn push(&mut self, child: impl Widget + 'static) {
        self.row.push(child);
    }

    pub fn align(mut self, align: RowAlign) -> Self {
        self.row = self.row.align(align);
        self
    }
}

impl Widget for Horizontal {
    fn compose(&self) -> ComposeResult {
        self.row.compose()
    }

    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        self.row.take_composed_children()
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        self.row.render(console, options)
    }

    fn render_with_debug(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        debug: &crate::debug::DebugLayout,
    ) -> Segments {
        self.row.render_with_debug(console, options, debug)
    }

    fn on_mount(&mut self) {
        self.row.on_mount();
    }

    fn on_unmount(&mut self) {
        self.row.on_unmount();
    }

    fn on_tick(&mut self, tick: u64) {
        self.row.on_tick(tick);
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        self.row.on_resize(width, height);
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.row.on_layout(width, height);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.row.on_event_capture(event, ctx);
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.row.on_event(event, ctx);
    }

    fn on_mouse_move(&mut self, x: u16, y: u16) -> bool {
        self.row.on_mouse_move(x, y)
    }

    fn layout_height(&self) -> Option<usize> {
        self.row.layout_height()
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        self.row.styles()
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        self.row.styles_mut()
    }
}

pub struct Vertical {
    container: Container,
}

impl Vertical {
    pub fn new() -> Self {
        Self {
            container: Container::new(),
        }
    }

    pub fn with_child(mut self, child: impl Widget + 'static) -> Self {
        self.container = self.container.with_child(child);
        self
    }

    /// Add multiple children from a `compose![]` result.
    pub fn with_compose(mut self, children: ComposeResult) -> Self {
        self.container = self.container.with_compose(children);
        self
    }

    pub fn push(&mut self, child: impl Widget + 'static) {
        self.container.push(child);
    }
}

impl Widget for Vertical {
    fn compose(&self) -> ComposeResult {
        self.container.compose()
    }

    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        self.container.take_composed_children()
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        self.container.render(console, options)
    }

    fn render_with_debug(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        debug: &crate::debug::DebugLayout,
    ) -> Segments {
        self.container.render_with_debug(console, options, debug)
    }

    fn on_mount(&mut self) {
        self.container.on_mount();
    }

    fn on_unmount(&mut self) {
        self.container.on_unmount();
    }

    fn on_tick(&mut self, tick: u64) {
        self.container.on_tick(tick);
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        self.container.on_resize(width, height);
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.container.on_layout(width, height);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.container.on_event_capture(event, ctx);
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.container.on_event(event, ctx);
    }

    fn on_mouse_move(&mut self, x: u16, y: u16) -> bool {
        self.container.on_mouse_move(x, y)
    }

    fn layout_height(&self) -> Option<usize> {
        self.container.layout_height()
    }

    fn content_width(&self) -> Option<usize> {
        self.container.content_width()
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        self.container.styles()
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        self.container.styles_mut()
    }
}

pub struct VerticalGroup {
    inner: Vertical,
}

impl VerticalGroup {
    pub fn new() -> Self {
        Self {
            inner: Vertical::new(),
        }
    }

    pub fn with_child(mut self, child: impl Widget + 'static) -> Self {
        self.inner = self.inner.with_child(child);
        self
    }

    pub fn push(&mut self, child: impl Widget + 'static) {
        self.inner.push(child);
    }
}

impl Widget for VerticalGroup {
    fn compose(&self) -> ComposeResult {
        self.inner.compose()
    }

    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        self.inner.take_composed_children()
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        self.inner.render(console, options)
    }

    fn render_with_debug(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        debug: &crate::debug::DebugLayout,
    ) -> Segments {
        self.inner.render_with_debug(console, options, debug)
    }

    fn on_mount(&mut self) {
        self.inner.on_mount();
    }

    fn on_unmount(&mut self) {
        self.inner.on_unmount();
    }

    fn on_tick(&mut self, tick: u64) {
        self.inner.on_tick(tick);
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        self.inner.on_resize(width, height);
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.inner.on_layout(width, height);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.inner.on_event_capture(event, ctx);
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.inner.on_event(event, ctx);
    }

    fn on_mouse_move(&mut self, x: u16, y: u16) -> bool {
        self.inner.on_mouse_move(x, y)
    }

    fn layout_height(&self) -> Option<usize> {
        self.inner.layout_height()
    }

    fn content_width(&self) -> Option<usize> {
        self.inner.content_width()
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        self.inner.styles()
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        self.inner.styles_mut()
    }
}

pub struct Static {
    label: super::Label,
}

impl Static {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            label: super::Label::new(text),
        }
    }

    pub fn class(self, value: impl Into<String>) -> Node {
        Node::new(self).class(value)
    }

    pub fn id(self, value: impl Into<String>) -> Node {
        Node::new(self).id(value)
    }
}

impl Widget for Static {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        self.label.render(console, options)
    }

    fn layout_height(&self) -> Option<usize> {
        self.label.layout_height()
    }

    fn content_width(&self) -> Option<usize> {
        self.label.content_width()
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        self.label.styles()
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        self.label.styles_mut()
    }
}

pub struct Center {
    child: Container,
    styles: WidgetStyles,
}

impl Center {
    pub fn new() -> Self {
        Self {
            child: Container::new(),
            styles: WidgetStyles::default(),
        }
    }

    pub fn with_child(mut self, child: impl Widget + 'static) -> Self {
        self.child.push(child);
        self
    }

    pub fn push(&mut self, child: impl Widget + 'static) {
        self.child.push(child);
    }
}

impl Widget for Center {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);
        let child_width = self
            .child
            .content_width()
            .unwrap_or(width)
            .max(1)
            .min(width);
        let mut child_options = options.clone();
        child_options.size = (child_width, height);
        child_options.max_width = child_width;
        child_options.max_height = height;

        let segments = self.child.render_styled(console, &child_options);
        let lines = Segment::split_and_crop_lines(segments, child_width, None, true, false);
        let lines = Segment::set_shape(&lines, child_width, None, None, false);
        let offset = width.saturating_sub(child_width) / 2;
        let out_lines: Vec<Vec<Segment>> = lines
            .iter()
            .map(|line| align_line_horizontal(line, width, child_width, offset))
            .collect();

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

    fn on_layout(&mut self, width: u16, height: u16) {
        self.child.on_layout(width, height);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.child.on_event_capture(event, ctx);
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.child.on_event(event, ctx);
    }

    fn layout_height(&self) -> Option<usize> {
        self.child.layout_height()
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

pub struct CenterMiddle {
    child: Container,
    styles: WidgetStyles,
}

impl CenterMiddle {
    pub fn new() -> Self {
        Self {
            child: Container::new(),
            styles: WidgetStyles::default(),
        }
    }

    pub fn with_child(mut self, child: impl Widget + 'static) -> Self {
        self.child.push(child);
        self
    }

    pub fn push(&mut self, child: impl Widget + 'static) {
        self.child.push(child);
    }
}

impl Widget for CenterMiddle {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);
        let child_width = self
            .child
            .content_width()
            .unwrap_or(width)
            .max(1)
            .min(width);
        let intrinsic_height = self
            .child
            .layout_height()
            .unwrap_or(height)
            .max(1)
            .min(height);

        let mut child_options = options.clone();
        child_options.size = (child_width, intrinsic_height);
        child_options.max_width = child_width;
        child_options.max_height = intrinsic_height;

        let segments = self.child.render_styled(console, &child_options);
        let lines = Segment::split_and_crop_lines(segments, child_width, None, true, false);
        let child_height = self
            .child
            .layout_height()
            .unwrap_or_else(|| effective_rendered_height(&lines))
            .max(1)
            .min(height);
        let top = height.saturating_sub(child_height) / 2;
        let left = width.saturating_sub(child_width) / 2;

        let mut out_lines: Vec<Vec<Segment>> = Vec::with_capacity(height);
        for _ in 0..top {
            out_lines.push(vec![Segment::new(" ".repeat(width))]);
        }
        out_lines.extend(
            lines
                .into_iter()
                .take(child_height)
                .map(|line| align_line_horizontal(&line, width, child_width, left)),
        );
        while out_lines.len() < height {
            out_lines.push(vec![Segment::new(" ".repeat(width))]);
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

    fn on_layout(&mut self, width: u16, height: u16) {
        self.child.on_layout(width, height);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.child.on_event_capture(event, ctx);
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.child.on_event(event, ctx);
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

pub struct Right {
    child: Container,
    styles: WidgetStyles,
}

impl Right {
    pub fn new() -> Self {
        Self {
            child: Container::new(),
            styles: WidgetStyles::default(),
        }
    }

    pub fn with_child(mut self, child: impl Widget + 'static) -> Self {
        self.child.push(child);
        self
    }

    pub fn push(&mut self, child: impl Widget + 'static) {
        self.child.push(child);
    }
}

impl Widget for Right {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);
        let child_width = self
            .child
            .content_width()
            .unwrap_or(width)
            .max(1)
            .min(width);
        let mut child_options = options.clone();
        child_options.size = (child_width, height);
        child_options.max_width = child_width;
        child_options.max_height = height;

        let segments = self.child.render_styled(console, &child_options);
        let lines = Segment::split_and_crop_lines(segments, child_width, None, true, false);
        let lines = Segment::set_shape(&lines, child_width, None, None, false);
        let offset = width.saturating_sub(child_width);
        let out_lines: Vec<Vec<Segment>> = lines
            .iter()
            .map(|line| align_line_horizontal(line, width, child_width, offset))
            .collect();

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

    fn on_layout(&mut self, width: u16, height: u16) {
        self.child.on_layout(width, height);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.child.on_event_capture(event, ctx);
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.child.on_event(event, ctx);
    }

    fn layout_height(&self) -> Option<usize> {
        self.child.layout_height()
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

pub struct Middle {
    child: Container,
    styles: WidgetStyles,
}

impl Middle {
    pub fn new() -> Self {
        Self {
            child: Container::new(),
            styles: WidgetStyles::default(),
        }
    }

    pub fn with_child(mut self, child: impl Widget + 'static) -> Self {
        self.child.push(child);
        self
    }

    pub fn push(&mut self, child: impl Widget + 'static) {
        self.child.push(child);
    }
}

impl Widget for Middle {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);
        let intrinsic_height = self
            .child
            .layout_height()
            .unwrap_or(height)
            .max(1)
            .min(height);

        let mut child_options = options.clone();
        child_options.size = (width, intrinsic_height);
        child_options.max_width = width;
        child_options.max_height = intrinsic_height;

        let segments = self.child.render_styled(console, &child_options);
        let lines = Segment::split_and_crop_lines(segments, width, None, true, false);
        let child_height = self
            .child
            .layout_height()
            .unwrap_or_else(|| effective_rendered_height(&lines))
            .max(1)
            .min(height);
        let top = height.saturating_sub(child_height) / 2;

        let mut out_lines: Vec<Vec<Segment>> = Vec::with_capacity(height);
        for _ in 0..top {
            out_lines.push(vec![Segment::new(" ".repeat(width))]);
        }
        out_lines.extend(
            lines
                .into_iter()
                .take(child_height)
                .map(|line| adjust_line_length_no_bg(&line, width)),
        );
        while out_lines.len() < height {
            out_lines.push(vec![Segment::new(" ".repeat(width))]);
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

    fn on_layout(&mut self, width: u16, height: u16) {
        self.child.on_layout(width, height);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.child.on_event_capture(event, ctx);
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.child.on_event(event, ctx);
    }

    fn layout_height(&self) -> Option<usize> {
        super::helpers::fixed_height_from_constraints(self.layout_constraints())
    }

    fn content_width(&self) -> Option<usize> {
        self.child.content_width()
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

pub struct HorizontalGroup {
    inner: Horizontal,
}

impl HorizontalGroup {
    pub fn new() -> Self {
        Self {
            inner: Horizontal::new(),
        }
    }

    pub fn with_child(mut self, child: impl Widget + 'static) -> Self {
        self.inner = self.inner.with_child(child);
        self
    }

    pub fn push(&mut self, child: impl Widget + 'static) {
        self.inner.push(child);
    }

    pub fn align(mut self, align: RowAlign) -> Self {
        self.inner = self.inner.align(align);
        self
    }
}

impl Widget for HorizontalGroup {
    fn compose(&self) -> ComposeResult {
        self.inner.compose()
    }

    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        self.inner.take_composed_children()
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        self.inner.render(console, options)
    }

    fn render_with_debug(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        debug: &crate::debug::DebugLayout,
    ) -> Segments {
        self.inner.render_with_debug(console, options, debug)
    }

    fn on_mount(&mut self) {
        self.inner.on_mount();
    }

    fn on_unmount(&mut self) {
        self.inner.on_unmount();
    }

    fn on_tick(&mut self, tick: u64) {
        self.inner.on_tick(tick);
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        self.inner.on_resize(width, height);
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.inner.on_layout(width, height);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.inner.on_event_capture(event, ctx);
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.inner.on_event(event, ctx);
    }

    fn on_mouse_move(&mut self, x: u16, y: u16) -> bool {
        self.inner.on_mouse_move(x, y)
    }

    fn layout_height(&self) -> Option<usize> {
        self.inner.layout_height()
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        self.inner.styles()
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        self.inner.styles_mut()
    }
}

pub struct VerticalScroll {
    child: Container,
    children_extracted: bool,
    focused: bool,
    height: Option<usize>,
    offset_y: usize,
    scroll_step: usize,
    content_height: AtomicUsize,
    viewport_width: AtomicUsize,
    viewport_height: AtomicUsize,
    styles: WidgetStyles,
}

impl VerticalScroll {
    pub fn new() -> Self {
        Self {
            child: Container::new(),
            children_extracted: false,
            focused: false,
            height: None,
            offset_y: 0,
            scroll_step: 1,
            content_height: AtomicUsize::new(0),
            viewport_width: AtomicUsize::new(1),
            viewport_height: AtomicUsize::new(0),
            styles: WidgetStyles::default(),
        }
    }

    pub fn with_child(mut self, child: impl Widget + 'static) -> Self {
        self.child.push(child);
        self
    }

    /// Add multiple children from a `compose![]` result.
    pub fn with_compose(mut self, children: ComposeResult) -> Self {
        self.child = self.child.with_compose(children);
        self
    }

    pub fn push(&mut self, child: impl Widget + 'static) {
        self.child.push(child);
    }

    pub fn height(mut self, height: usize) -> Self {
        self.height = Some(height.max(1));
        self
    }

    pub fn scroll_by(&mut self, delta: i32) {
        if delta.is_negative() {
            self.offset_y = self.offset_y.saturating_sub(delta.unsigned_abs() as usize);
        } else {
            self.offset_y = self.offset_y.saturating_add(delta as usize);
        }
        self.clamp_offset();
    }

    pub fn scroll_step(mut self, step: usize) -> Self {
        self.scroll_step = step.max(1);
        self
    }

    pub fn set_virtual_content_size(&self, _width: usize, height: usize) {
        self.content_height
            .store(height.max(1), std::sync::atomic::Ordering::Relaxed);
    }

    fn is_tree_mode(&self) -> bool {
        self.children_extracted
    }

    fn max_offset(&self) -> usize {
        let content = self.content_height.load(Ordering::Relaxed);
        let viewport = self.viewport_height.load(Ordering::Relaxed).max(1);
        content.saturating_sub(viewport)
    }

    fn clamp_offset(&mut self) {
        let max_y = self.max_offset();
        if self.offset_y > max_y {
            self.offset_y = max_y;
        }
    }

    fn child_coords(&self, x: u16, y: u16) -> (u16, u16) {
        (x, y.saturating_add(self.offset_y as u16))
    }

    fn sync_child_layout(&mut self) {
        if self.children_extracted {
            return;
        }
        let width = self.viewport_width.load(Ordering::Relaxed).max(1) as u16;
        let height = self.viewport_height.load(Ordering::Relaxed).max(1) as u16;
        self.child.on_layout(width, height);
    }
}

impl Widget for VerticalScroll {
    fn compose(&self) -> ComposeResult {
        self.child.compose()
    }

    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        if self.children_extracted {
            return Vec::new();
        }
        self.children_extracted = true;
        self.child.take_composed_children()
    }

    fn focusable(&self) -> bool {
        true
    }

    fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
        if !self.is_tree_mode() {
            self.child.set_focus(focused);
        }
    }

    fn has_focus(&self) -> bool {
        self.focused
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let viewport_height = self.height.unwrap_or_else(|| options.size.1.max(1)).max(1);
        self.viewport_width.store(width, Ordering::Relaxed);
        self.viewport_height
            .store(viewport_height, Ordering::Relaxed);

        if self.is_tree_mode() {
            // In tree mode, children are managed by the tree. Render scroll
            // chrome (background + scrollbar) based on stored content dimensions.
            let content_h = self.content_height.load(Ordering::Relaxed);
            let show_v = content_h > viewport_height;
            const V_SCROLLBAR_SIZE: usize = 2;
            let content_viewport_w = width
                .saturating_sub(if show_v {
                    V_SCROLLBAR_SIZE.min(width.saturating_sub(1))
                } else {
                    0
                })
                .max(1);

            let mut slice: Vec<Vec<Segment>> = (0..viewport_height)
                .map(|_| vec![Segment::new(" ".repeat(content_viewport_w))])
                .collect();

            if show_v {
                let (track_style, thumb_style, _thumb_active_style) =
                    ScrollView::line_scrollbar_styles();
                let track_len = viewport_height.max(1);
                let offset = self.offset_y.min(self.max_offset());
                let (thumb_start, thumb_len) =
                    ScrollView::line_scrollbar_thumb(track_len, content_h, viewport_height, offset);
                let bar_width = width.saturating_sub(content_viewport_w).max(1);
                for (row, line) in slice.iter_mut().enumerate() {
                    let style =
                        if row < track_len && row >= thumb_start && row < thumb_start + thumb_len {
                            thumb_style
                        } else {
                            track_style
                        };
                    for _ in 0..bar_width {
                        line.push(Segment::styled(" ".to_string(), style));
                    }
                }
            }

            let slice = Segment::set_shape(&slice, width, Some(viewport_height), None, false);
            let line_count = slice.len();
            let mut out = Segments::new();
            for (idx, line) in slice.into_iter().enumerate() {
                out.extend(line);
                if idx + 1 < line_count {
                    out.push(Segment::line());
                }
            }
            return out;
        }

        let constraints = self.child.layout_constraints();
        const V_SCROLLBAR_SIZE: usize = 2;
        let child_layout_height = self.child.layout_height();
        let (track_style, thumb_style, _thumb_active_style) = ScrollView::line_scrollbar_styles();

        let mut show_v = false;
        let mut content_viewport_w = width;
        let mut lines: Vec<Vec<Segment>> = Vec::new();
        let mut content_height = viewport_height;

        for _ in 0..2 {
            let viewport_w = width
                .saturating_sub(if show_v {
                    V_SCROLLBAR_SIZE.min(width.saturating_sub(1))
                } else {
                    0
                })
                .max(1);
            let target_height = child_layout_height
                .unwrap_or_else(|| viewport_height.saturating_add(viewport_height).max(1));
            let render_width = clamp_with_constraints(
                viewport_w,
                constraints.min_width,
                constraints.max_width,
                viewport_w,
            )
            .max(viewport_w);
            let render_height = clamp_with_constraints(
                target_height,
                constraints.min_height,
                constraints.max_height,
                target_height,
            );
            let mut child_options = options.clone();
            child_options.size = (render_width, render_height);
            child_options.max_width = render_width;
            child_options.max_height = render_height;

            let segments = self.child.render_styled(console, &child_options);
            let mut candidate =
                Segment::split_and_crop_lines(segments, render_width, None, true, false);
            let raw_lines_height = candidate.len();
            if let Some(height) = child_layout_height {
                let effective_height = height.max(raw_lines_height).max(1);
                candidate = Segment::set_shape(
                    &candidate,
                    render_width,
                    Some(effective_height),
                    None,
                    false,
                );
            }
            candidate = pad_lines_to_width(candidate, render_width);

            let candidate_height = candidate.len().max(viewport_height);
            let next_show_v = candidate_height > viewport_height;
            lines = candidate;
            content_height = candidate_height;
            content_viewport_w = viewport_w;
            if next_show_v == show_v {
                break;
            }
            show_v = next_show_v;
        }

        self.content_height.store(content_height, Ordering::Relaxed);

        let max_offset = content_height.saturating_sub(viewport_height);
        let offset = self.offset_y.min(max_offset);
        let start = offset.min(lines.len());
        let end = (start + viewport_height).min(lines.len());
        let mut slice = lines[start..end]
            .to_vec()
            .into_iter()
            .map(|line| {
                let cropped = crop_line_horizontal(&line, 0, content_viewport_w);
                adjust_line_length_no_bg(&cropped, content_viewport_w)
            })
            .collect::<Vec<_>>();
        slice = Segment::set_shape(
            &slice,
            content_viewport_w,
            Some(viewport_height),
            None,
            false,
        );

        if show_v {
            let track_len = viewport_height.max(1);
            let (thumb_start, thumb_len) = ScrollView::line_scrollbar_thumb(
                track_len,
                content_height,
                viewport_height,
                offset,
            );
            let bar_width = width.saturating_sub(content_viewport_w).max(1);
            for (row, line) in slice.iter_mut().enumerate() {
                let style =
                    if row < track_len && row >= thumb_start && row < thumb_start + thumb_len {
                        thumb_style
                    } else {
                        track_style
                    };
                for _ in 0..bar_width {
                    line.push(Segment::styled(" ".to_string(), style));
                }
            }
        }

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

    fn on_mount(&mut self) {
        if !self.is_tree_mode() {
            self.child.on_mount();
        }
    }

    fn on_unmount(&mut self) {
        if !self.is_tree_mode() {
            self.child.on_unmount();
        }
    }

    fn on_tick(&mut self, tick: u64) {
        if !self.is_tree_mode() {
            self.child.on_tick(tick);
        }
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        self.viewport_width.store(width as usize, Ordering::Relaxed);
        self.viewport_height
            .store(height as usize, Ordering::Relaxed);
        if !self.is_tree_mode() {
            self.child.on_resize(width, height);
        }
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.viewport_width.store(width as usize, Ordering::Relaxed);
        self.viewport_height
            .store(height as usize, Ordering::Relaxed);
        if !self.is_tree_mode() {
            self.child.on_layout(width, height);
        }
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        if !self.is_tree_mode() {
            self.child.on_event_capture(event, ctx);
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if !self.is_tree_mode() {
            self.sync_child_layout();
        }
        // Scroll actions are handled regardless of tree mode.
        if let Event::Action(action) = event {
            match action {
                Action::ScrollHome => {
                    self.offset_y = 0;
                    ctx.set_handled();
                    return;
                }
                Action::ScrollEnd => {
                    self.offset_y = self.max_offset();
                    ctx.set_handled();
                    return;
                }
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
        // In tree mode, do not dispatch events to extracted children.
        if self.is_tree_mode() {
            return;
        }
        let child_event = match event {
            Event::MouseDown(mouse) => {
                let (child_x, child_y) = self.child_coords(mouse.x, mouse.y);
                Some(Event::MouseDown(crate::event::MouseDownEvent {
                    target: NodeId::default(),
                    screen_x: mouse.screen_x,
                    screen_y: mouse.screen_y,
                    x: child_x,
                    y: child_y,
                }))
            }
            Event::MouseUp(mouse) => {
                let (child_x, child_y) = self.child_coords(mouse.x, mouse.y);
                Some(Event::MouseUp(crate::event::MouseUpEvent {
                    target: Some(NodeId::default()),
                    screen_x: mouse.screen_x,
                    screen_y: mouse.screen_y,
                    x: child_x,
                    y: child_y,
                }))
            }
            Event::MouseScroll(mouse) => {
                let (child_x, child_y) = self.child_coords(mouse.x, mouse.y);
                Some(Event::MouseScroll(crate::event::MouseScrollEvent {
                    target: Some(NodeId::default()),
                    screen_x: mouse.screen_x,
                    screen_y: mouse.screen_y,
                    x: child_x,
                    y: child_y,
                    delta_x: mouse.delta_x,
                    delta_y: mouse.delta_y,
                    modifiers: mouse.modifiers,
                }))
            }
            _ => None,
        };
        if let Some(child_event) = child_event.as_ref() {
            self.child.on_event(child_event, ctx);
        } else {
            self.child.on_event(event, ctx);
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
            ctx.set_handled();
        }
    }

    fn on_mouse_move(&mut self, x: u16, y: u16) -> bool {
        if self.is_tree_mode() {
            return false;
        }
        self.sync_child_layout();
        let (child_x, child_y) = self.child_coords(x, y);
        self.child.on_mouse_move(child_x, child_y)
    }

    fn scroll_offset(&self) -> (usize, usize) {
        (0, self.offset_y)
    }

    fn clips_descendants_to_content(&self) -> bool {
        true
    }

    fn layout_height(&self) -> Option<usize> {
        self.height.or_else(|| {
            if self.is_tree_mode() {
                None
            } else {
                self.child.layout_height()
            }
        })
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

pub struct ScrollableContainer {
    inner: VerticalScroll,
}

impl ScrollableContainer {
    pub fn new() -> Self {
        Self {
            inner: VerticalScroll::new(),
        }
    }

    pub fn with_child(mut self, child: impl Widget + 'static) -> Self {
        self.inner = self.inner.with_child(child);
        self
    }

    pub fn push(&mut self, child: impl Widget + 'static) {
        self.inner.push(child);
    }

    pub fn height(mut self, height: usize) -> Self {
        self.inner = self.inner.height(height);
        self
    }

    pub fn scroll_step(mut self, step: usize) -> Self {
        self.inner = self.inner.scroll_step(step);
        self
    }
}

impl Widget for ScrollableContainer {
    fn compose(&self) -> ComposeResult {
        self.inner.compose()
    }

    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        self.inner.take_composed_children()
    }

    fn focusable(&self) -> bool {
        self.inner.focusable()
    }

    fn set_focus(&mut self, focused: bool) {
        self.inner.set_focus(focused);
    }

    fn has_focus(&self) -> bool {
        self.inner.has_focus()
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        self.inner.render(console, options)
    }

    fn on_mount(&mut self) {
        self.inner.on_mount();
    }

    fn on_unmount(&mut self) {
        self.inner.on_unmount();
    }

    fn on_tick(&mut self, tick: u64) {
        self.inner.on_tick(tick);
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        self.inner.on_resize(width, height);
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.inner.on_layout(width, height);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.inner.on_event_capture(event, ctx);
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.inner.on_event(event, ctx);
    }

    fn on_mouse_scroll(&mut self, delta_x: i32, delta_y: i32, ctx: &mut EventCtx) {
        self.inner.on_mouse_scroll(delta_x, delta_y, ctx);
    }

    fn on_mouse_move(&mut self, x: u16, y: u16) -> bool {
        self.inner.on_mouse_move(x, y)
    }

    fn scroll_offset(&self) -> (usize, usize) {
        self.inner.scroll_offset()
    }

    fn clips_descendants_to_content(&self) -> bool {
        self.inner.clips_descendants_to_content()
    }

    fn layout_height(&self) -> Option<usize> {
        self.inner.layout_height()
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        self.inner.styles()
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        self.inner.styles_mut()
    }
}

pub struct HorizontalScroll {
    child: Container,
    children_extracted: bool,
    focused: bool,
    height: Option<usize>,
    offset_x: usize,
    scroll_step_x: usize,
    content_width: AtomicUsize,
    viewport_width: AtomicUsize,
    styles: WidgetStyles,
}

impl HorizontalScroll {
    pub fn new() -> Self {
        Self {
            child: Container::new(),
            children_extracted: false,
            focused: false,
            height: None,
            offset_x: 0,
            scroll_step_x: 1,
            content_width: AtomicUsize::new(0),
            viewport_width: AtomicUsize::new(0),
            styles: WidgetStyles::default(),
        }
    }

    pub fn with_child(mut self, child: impl Widget + 'static) -> Self {
        self.child.push(child);
        self
    }

    /// Add multiple children from a `compose![]` result.
    pub fn with_compose(mut self, children: ComposeResult) -> Self {
        self.child = self.child.with_compose(children);
        self
    }

    pub fn push(&mut self, child: impl Widget + 'static) {
        self.child.push(child);
    }

    pub fn height(mut self, height: usize) -> Self {
        self.height = Some(height.max(1));
        self
    }

    pub fn scroll_by_x(&mut self, delta: i32) {
        if delta.is_negative() {
            self.offset_x = self.offset_x.saturating_sub(delta.unsigned_abs() as usize);
        } else {
            self.offset_x = self.offset_x.saturating_add(delta as usize);
        }
        self.clamp_offset();
    }

    pub fn scroll_step_x(mut self, step: usize) -> Self {
        self.scroll_step_x = step.max(1);
        self
    }

    pub fn set_virtual_content_size(&self, width: usize, _height: usize) {
        self.content_width
            .store(width.max(1), std::sync::atomic::Ordering::Relaxed);
    }

    fn is_tree_mode(&self) -> bool {
        self.children_extracted
    }

    fn max_offset(&self) -> usize {
        let content = self.content_width.load(Ordering::Relaxed);
        let viewport = self.viewport_width.load(Ordering::Relaxed).max(1);
        content.saturating_sub(viewport)
    }

    fn clamp_offset(&mut self) {
        let max_x = self.max_offset();
        if self.offset_x > max_x {
            self.offset_x = max_x;
        }
    }
}

impl Widget for HorizontalScroll {
    fn compose(&self) -> ComposeResult {
        self.child.compose()
    }

    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        if self.children_extracted {
            return Vec::new();
        }
        self.children_extracted = true;
        self.child.take_composed_children()
    }

    fn focusable(&self) -> bool {
        true
    }

    fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
        if !self.is_tree_mode() {
            self.child.set_focus(focused);
        }
    }

    fn has_focus(&self) -> bool {
        self.focused
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let viewport_width = options.size.0.max(1);
        let viewport_height = self.height.unwrap_or_else(|| options.size.1.max(1)).max(1);
        self.viewport_width.store(viewport_width, Ordering::Relaxed);

        if self.is_tree_mode() {
            // In tree mode, children are managed by the tree. Render scroll
            // chrome (background + scrollbar) based on stored content dimensions.
            let content_w = self.content_width.load(Ordering::Relaxed);
            let show_h = content_w > viewport_width;
            const H_SCROLLBAR_SIZE: usize = 1;
            let content_viewport_h = viewport_height
                .saturating_sub(if show_h { H_SCROLLBAR_SIZE } else { 0 })
                .max(1);

            let mut slice: Vec<Vec<Segment>> = (0..content_viewport_h)
                .map(|_| vec![Segment::new(" ".repeat(viewport_width))])
                .collect();

            if show_h {
                let (track_style, thumb_style, _thumb_active_style) =
                    ScrollView::line_scrollbar_styles();
                let offset = self.offset_x.min(self.max_offset());
                let (thumb_start, thumb_len) = ScrollView::line_scrollbar_thumb(
                    viewport_width,
                    content_w,
                    viewport_width,
                    offset,
                );
                let mut row = Vec::new();
                for col in 0..viewport_width {
                    let style = if col >= thumb_start && col < thumb_start + thumb_len {
                        thumb_style
                    } else {
                        track_style
                    };
                    row.push(Segment::styled(" ".to_string(), style));
                }
                slice.push(row);
            }

            let slice =
                Segment::set_shape(&slice, viewport_width, Some(viewport_height), None, false);
            let line_count = slice.len();
            let mut out = Segments::new();
            for (idx, line) in slice.into_iter().enumerate() {
                out.extend(line);
                if idx + 1 < line_count {
                    out.push(Segment::line());
                }
            }
            return out;
        }

        const H_SCROLLBAR_SIZE: usize = 1;
        let constraints = self.child.layout_constraints();
        let (track_style, thumb_style, _thumb_active_style) = ScrollView::line_scrollbar_styles();

        let mut show_h = false;
        let mut content_viewport_h = viewport_height;
        let mut lines: Vec<Vec<Segment>> = Vec::new();
        let mut content_width = viewport_width;

        for _ in 0..2 {
            let viewport_h = viewport_height
                .saturating_sub(if show_h { H_SCROLLBAR_SIZE } else { 0 })
                .max(1);
            let target_width = self
                .child
                .content_width()
                .unwrap_or(viewport_width)
                .max(viewport_width);
            let render_width = clamp_with_constraints(
                target_width,
                constraints.min_width,
                constraints.max_width,
                target_width,
            )
            .max(viewport_width);
            let target_height = self.child.layout_height().unwrap_or(viewport_h).max(1);
            let render_height = clamp_with_constraints(
                target_height,
                constraints.min_height,
                constraints.max_height,
                target_height,
            );
            let mut child_options = options.clone();
            child_options.size = (render_width, render_height);
            child_options.max_width = render_width;
            child_options.max_height = render_height;

            let segments = self.child.render_styled(console, &child_options);
            let mut candidate =
                Segment::split_and_crop_lines(segments, render_width, None, true, false);
            if let Some(height) = self.child.layout_height() {
                candidate =
                    Segment::set_shape(&candidate, render_width, Some(height.max(1)), None, false);
            }
            candidate = pad_lines_to_width(candidate, render_width);

            let candidate_width = candidate
                .iter()
                .map(|line| Segment::get_line_length(line))
                .max()
                .unwrap_or(viewport_width)
                .max(viewport_width);
            let next_show_h = candidate_width > viewport_width;
            lines = candidate;
            content_width = candidate_width;
            content_viewport_h = viewport_h;
            if next_show_h == show_h {
                break;
            }
            show_h = next_show_h;
        }

        self.content_width.store(content_width, Ordering::Relaxed);

        let max_offset = content_width.saturating_sub(viewport_width);
        let offset = self.offset_x.min(max_offset);
        let slice = lines
            .into_iter()
            .take(content_viewport_h)
            .map(|line| {
                let cropped = crop_line_horizontal(&line, offset, viewport_width);
                adjust_line_length_no_bg(&cropped, viewport_width)
            })
            .collect::<Vec<_>>();
        let mut slice = Segment::set_shape(
            &slice,
            viewport_width,
            Some(content_viewport_h),
            None,
            false,
        );

        if show_h {
            let (thumb_start, thumb_len) = ScrollView::line_scrollbar_thumb(
                viewport_width,
                content_width,
                viewport_width,
                offset,
            );
            let mut row = Vec::new();
            for col in 0..viewport_width {
                let style = if col >= thumb_start && col < thumb_start + thumb_len {
                    thumb_style
                } else {
                    track_style
                };
                row.push(Segment::styled(" ".to_string(), style));
            }
            slice.push(row);
        }

        let slice = Segment::set_shape(&slice, viewport_width, Some(viewport_height), None, false);

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

    fn on_mount(&mut self) {
        if !self.is_tree_mode() {
            self.child.on_mount();
        }
    }

    fn on_unmount(&mut self) {
        if !self.is_tree_mode() {
            self.child.on_unmount();
        }
    }

    fn on_tick(&mut self, tick: u64) {
        if !self.is_tree_mode() {
            self.child.on_tick(tick);
        }
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        if !self.is_tree_mode() {
            self.child.on_resize(width, height);
        }
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        if !self.is_tree_mode() {
            self.child.on_layout(width, height);
        }
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        if !self.is_tree_mode() {
            self.child.on_event_capture(event, ctx);
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        // Scroll actions are handled regardless of tree mode.
        if let Event::Action(action) = event {
            match action {
                Action::ScrollHome => {
                    self.offset_x = 0;
                    ctx.set_handled();
                    return;
                }
                Action::ScrollEnd => {
                    self.offset_x = self.max_offset();
                    ctx.set_handled();
                    return;
                }
                Action::ScrollLeft => {
                    self.scroll_by_x(-(self.scroll_step_x as i32));
                    ctx.set_handled();
                    return;
                }
                Action::ScrollRight => {
                    self.scroll_by_x(self.scroll_step_x as i32);
                    ctx.set_handled();
                    return;
                }
                Action::ScrollPageLeft => {
                    let page = self.viewport_width.load(Ordering::Relaxed).max(1);
                    self.scroll_by_x(-(page as i32));
                    ctx.set_handled();
                    return;
                }
                Action::ScrollPageRight => {
                    let page = self.viewport_width.load(Ordering::Relaxed).max(1);
                    self.scroll_by_x(page as i32);
                    ctx.set_handled();
                    return;
                }
                _ => {}
            }
        }
        // In tree mode, do not dispatch events to extracted children.
        if self.is_tree_mode() {
            return;
        }
        self.child.on_event(event, ctx);
    }

    fn on_mouse_scroll(&mut self, delta_x: i32, delta_y: i32, ctx: &mut EventCtx) {
        let delta = if delta_x != 0 { delta_x } else { delta_y };
        if delta == 0 {
            return;
        }
        let before = self.offset_x;
        self.scroll_by_x(delta.saturating_mul(self.scroll_step_x as i32));
        if self.offset_x != before {
            ctx.request_repaint();
            ctx.set_handled();
        }
    }

    fn on_mouse_move(&mut self, x: u16, y: u16) -> bool {
        if self.is_tree_mode() {
            return false;
        }
        self.child.on_mouse_move(x, y)
    }

    fn scroll_offset(&self) -> (usize, usize) {
        (self.offset_x, 0)
    }

    fn clips_descendants_to_content(&self) -> bool {
        true
    }

    fn layout_height(&self) -> Option<usize> {
        self.height.or_else(|| {
            if self.is_tree_mode() {
                None
            } else {
                self.child.layout_height()
            }
        })
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

impl rich_rs::Renderable for HorizontalScroll {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

pub struct ItemGrid {
    inner: Grid,
}

impl ItemGrid {
    pub fn new(rows: usize, cols: usize) -> Self {
        Self {
            inner: Grid::new(rows, cols),
        }
    }

    pub fn set(&mut self, row: usize, col: usize, child: impl Widget + 'static) {
        self.inner.set(row, col, child);
    }

    pub fn with_cell(mut self, row: usize, col: usize, child: impl Widget + 'static) -> Self {
        self.inner = self.inner.with_cell(row, col, child);
        self
    }

    pub fn row_gap(mut self, gap: usize) -> Self {
        self.inner = self.inner.row_gap(gap);
        self
    }

    pub fn col_gap(mut self, gap: usize) -> Self {
        self.inner = self.inner.col_gap(gap);
        self
    }
}

impl Widget for ItemGrid {
    fn compose(&self) -> ComposeResult {
        self.inner.compose()
    }

    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        self.inner.take_composed_children()
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        self.inner.render(console, options)
    }

    fn render_with_debug(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        debug: &crate::debug::DebugLayout,
    ) -> Segments {
        self.inner.render_with_debug(console, options, debug)
    }

    fn on_mount(&mut self) {
        self.inner.on_mount();
    }

    fn on_unmount(&mut self) {
        self.inner.on_unmount();
    }

    fn on_tick(&mut self, tick: u64) {
        self.inner.on_tick(tick);
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        self.inner.on_resize(width, height);
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.inner.on_layout(width, height);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.inner.on_event_capture(event, ctx);
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.inner.on_event(event, ctx);
    }

    fn layout_height(&self) -> Option<usize> {
        self.inner.layout_height()
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        self.inner.styles()
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        self.inner.styles_mut()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{Action, EventCtx};
    use crate::prelude::Label;
    use rich_rs::{Console, ConsoleOptions, Segment};
    use std::sync::atomic::Ordering;

    // ─── VerticalScroll ──────────────────────────────────────────────

    #[test]
    fn vscroll_tree_mode_render_produces_output() {
        let mut vs = VerticalScroll::new()
            .with_child(Label::new("a"))
            .with_child(Label::new("b"));
        let children = vs.take_composed_children();
        assert!(children.len() >= 2);
        assert!(vs.is_tree_mode());

        // Simulate content taller than viewport so scrollbar appears.
        vs.content_height.store(100, Ordering::Relaxed);

        let console = Console::default();
        let mut opts = ConsoleOptions::default();
        opts.size = (20, 10);
        opts.max_width = 20;
        opts.max_height = 10;

        let segments = Widget::render(&vs, &console, &opts);
        let lines = Segment::split_and_crop_lines(segments, 20, None, true, false);
        assert_eq!(
            lines.len(),
            10,
            "tree-mode render must produce viewport-height lines"
        );
        let has_styled = lines.iter().any(|line| line.len() > 1);
        assert!(
            has_styled,
            "tree-mode render should include scrollbar chrome"
        );
    }

    #[test]
    fn vscroll_scroll_offset_and_clip() {
        let mut vs = VerticalScroll::new().with_child(Label::new("a"));
        vs.offset_y = 7;
        let _ = vs.take_composed_children();
        assert!(vs.is_tree_mode());

        assert_eq!(vs.scroll_offset(), (0, 7));
        assert!(vs.clips_descendants_to_content());
    }

    #[test]
    fn vscroll_tree_mode_scroll_actions() {
        let mut vs = VerticalScroll::new().with_child(Label::new("a"));
        let _ = vs.take_composed_children();
        vs.content_height.store(100, Ordering::Relaxed);
        vs.viewport_height.store(10, Ordering::Relaxed);

        let mut ctx = EventCtx::default();
        vs.on_event(&Event::Action(Action::ScrollDown), &mut ctx);
        assert!(ctx.handled());
        assert!(vs.offset_y > 0, "ScrollDown should increase offset_y");

        let mut ctx2 = EventCtx::default();
        vs.on_event(&Event::Action(Action::ScrollHome), &mut ctx2);
        assert!(ctx2.handled());
        assert_eq!(vs.offset_y, 0, "ScrollHome should reset offset_y");
    }

    #[test]
    fn vscroll_take_composed_children_idempotent() {
        let mut vs = VerticalScroll::new().with_child(Label::new("a"));
        let first = vs.take_composed_children();
        assert_eq!(first.len(), 1);
        let second = vs.take_composed_children();
        assert!(second.is_empty(), "second extraction must return empty");
    }

    // ─── HorizontalScroll ────────────────────────────────────────────

    #[test]
    fn hscroll_tree_mode_render_produces_output() {
        let mut hs = HorizontalScroll::new().with_child(Label::new("wide content here"));
        let children = hs.take_composed_children();
        assert!(!children.is_empty());
        assert!(hs.is_tree_mode());

        // Simulate content wider than viewport so scrollbar appears.
        hs.content_width.store(100, Ordering::Relaxed);

        let console = Console::default();
        let mut opts = ConsoleOptions::default();
        opts.size = (20, 10);
        opts.max_width = 20;
        opts.max_height = 10;

        let segments = Widget::render(&hs, &console, &opts);
        let lines = Segment::split_and_crop_lines(segments, 20, None, true, false);
        assert_eq!(
            lines.len(),
            10,
            "tree-mode render must produce viewport-height lines"
        );
    }

    #[test]
    fn hscroll_scroll_offset_and_clip() {
        let mut hs = HorizontalScroll::new().with_child(Label::new("a"));
        hs.offset_x = 3;
        let _ = hs.take_composed_children();
        assert!(hs.is_tree_mode());

        assert_eq!(hs.scroll_offset(), (3, 0));
        assert!(hs.clips_descendants_to_content());
    }

    #[test]
    fn hscroll_tree_mode_scroll_actions() {
        let mut hs = HorizontalScroll::new().with_child(Label::new("a"));
        let _ = hs.take_composed_children();
        hs.content_width.store(100, Ordering::Relaxed);
        hs.viewport_width.store(20, Ordering::Relaxed);

        let mut ctx = EventCtx::default();
        hs.on_event(&Event::Action(Action::ScrollRight), &mut ctx);
        assert!(ctx.handled());
        assert!(hs.offset_x > 0, "ScrollRight should increase offset_x");

        let mut ctx2 = EventCtx::default();
        hs.on_event(&Event::Action(Action::ScrollHome), &mut ctx2);
        assert!(ctx2.handled());
        assert_eq!(hs.offset_x, 0, "ScrollHome should reset offset_x");
    }

    #[test]
    fn hscroll_take_composed_children_idempotent() {
        let mut hs = HorizontalScroll::new().with_child(Label::new("a"));
        let first = hs.take_composed_children();
        assert_eq!(first.len(), 1);
        let second = hs.take_composed_children();
        assert!(second.is_empty(), "second extraction must return empty");
    }

    // ─── ScrollableContainer ─────────────────────────────────────────

    #[test]
    fn scrollable_container_forwards_scroll_offset() {
        let mut sc = ScrollableContainer::new().with_child(Label::new("a"));
        let _ = sc.take_composed_children();
        assert_eq!(sc.scroll_offset(), (0, 0));
        assert!(sc.clips_descendants_to_content());
    }
}
