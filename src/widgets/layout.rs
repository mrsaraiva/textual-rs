use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::compose::ComposeResult;
use crate::css;
use crate::debug::{DebugLayout, debug_layout};
use crate::event::{Event, EventCtx};
use crate::node_id::NodeId;

use super::{
    LayoutConstraints, Widget, WidgetStyles,
    helpers::{
        adjust_line_length_no_bg, apply_debug_box, apply_margin, clamp_with_constraints,
        constraints_from_style, fixed_height_from_constraints, margin_from_style,
        merge_constraints, pad_lines_to_width,
    },
};
use crate::style::{Margin, Scalar};

pub struct Row {
    children: Vec<Box<dyn Widget>>,
    align: RowAlign,
    last_layout_width: u16,
    styles: WidgetStyles,
}

impl Row {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            align: RowAlign::Top,
            last_layout_width: 0,
            styles: WidgetStyles::default(),
        }
    }

    pub fn with_child(mut self, child: impl Widget + 'static) -> Self {
        self.children.push(Box::new(child));
        self
    }

    /// Add multiple children from a `compose![]` result.
    pub fn with_compose(mut self, children: ComposeResult) -> Self {
        for decl in children {
            match decl.builder {
                crate::compose::WidgetBuilder::Ready(widget) => self.children.push(widget),
            }
        }
        self
    }

    pub fn push(&mut self, child: impl Widget + 'static) {
        self.children.push(Box::new(child));
    }

    pub fn align(mut self, align: RowAlign) -> Self {
        self.align = align;
        self
    }

    /// Read-only access to the row's children.
    pub fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }

    /// Mutable access to the row's children.
    pub fn children_mut(&mut self) -> &mut Vec<Box<dyn Widget>> {
        &mut self.children
    }

    fn child_at_x(&self, x: u16) -> Option<(usize, u16)> {
        let count = self.children.len();
        if count == 0 {
            return None;
        }
        let total_width = self.last_layout_width.max(x.saturating_add(1)).max(1) as usize;
        let base = total_width / count;
        let remainder = total_width % count;

        let mut cursor = 0usize;
        for idx in 0..count {
            let width = (base + usize::from(idx < remainder)).max(1);
            let end = cursor + width;
            let xu = x as usize;
            if xu < end {
                return Some((idx, (xu - cursor) as u16));
            }
            cursor = end;
        }
        Some((count - 1, 0))
    }
}

#[derive(Debug, Clone, Copy)]
pub enum RowAlign {
    Top,
    Center,
    Bottom,
}

impl Widget for Row {
    /// Declare children for tree-based mounting.
    ///
    /// TODO(P1-15): Row stores children via `with_child()`/`push()` as
    /// owned `Box<dyn Widget>`. Because `compose()` is `&self`, we cannot move
    /// them into `ChildDecl` entries. Once the runtime supports extracting
    /// children from containers during mount (via `take_composed_children()`),
    /// this will return proper declarations. Until then, render/event methods
    /// continue iterating `self.children` directly.
    fn compose(&self) -> ComposeResult {
        Vec::new()
    }

    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        std::mem::take(&mut self.children)
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height_limit = options.size.1.max(1);

        let count = self.children.len().max(1);
        let mut fixed_widths: Vec<Option<usize>> = vec![None; count];
        let mut margins: Vec<Margin> = vec![Margin::default(); count];
        let mut constraints_list: Vec<LayoutConstraints> = Vec::with_capacity(count);
        let mut resolved_list: Vec<crate::style::Style> = Vec::with_capacity(count);

        for (idx, child) in self.children.iter().enumerate() {
            let meta = css::selector_meta_generic(child.as_ref());
            let resolved = css::resolve_style(child.as_ref(), &meta);
            let margin = margin_from_style(&resolved);
            let style_constraints = constraints_from_style(&resolved);
            let constraints = merge_constraints(style_constraints, child.layout_constraints());

            let fixed =
                if let (Some(min), Some(max)) = (constraints.min_width, constraints.max_width) {
                    if min == max { Some(min) } else { None }
                } else if matches!(resolved.width, Some(Scalar::Auto)) {
                    let pad = resolved
                        .padding
                        .map(|s| s.left as usize)
                        .unwrap_or(0)
                        .saturating_mul(2);
                    let (_, _, border_left, border_right) =
                        super::helpers::border_spacing_from_style(&resolved);
                    child
                        .content_width()
                        .map(|w| w.saturating_add(pad + border_left + border_right).max(1))
                } else {
                    None
                };

            fixed_widths[idx] = fixed;
            margins[idx] = margin;
            constraints_list.push(constraints);
            resolved_list.push(resolved);
        }

        let mut fixed_total = 0usize;
        let mut flex_count = 0usize;
        for (idx, fixed) in fixed_widths.iter().enumerate() {
            if let Some(width) = fixed {
                let margin = margins[idx];
                fixed_total = fixed_total
                    .saturating_add(width + margin.left as usize + margin.right as usize);
            } else {
                flex_count += 1;
            }
        }

        if std::env::var("TEXTUAL_DEBUG_LAYOUT_FILE").is_ok() {
            debug_layout(&format!(
                "[row] id={} viewport=({}, {}) children={} fixed_total={}",
                0u64, width, height_limit, count, fixed_total
            ));
            for (idx, fixed) in fixed_widths.iter().enumerate() {
                debug_layout(&format!(
                    "[row] child={} fixed={:?} margin=({}, {}) constraints=({:?},{:?}) width={:?}",
                    idx,
                    fixed,
                    margins[idx].left,
                    margins[idx].right,
                    constraints_list[idx].min_width,
                    constraints_list[idx].max_width,
                    resolved_list[idx].width
                ));
            }
        }

        let remaining = width.saturating_sub(fixed_total);
        let base = if flex_count > 0 {
            remaining / flex_count
        } else {
            0
        };
        let remainder = if flex_count > 0 {
            remaining % flex_count
        } else {
            0
        };

        let mut flex_seen = 0usize;
        let widths: Vec<usize> = (0..count)
            .map(|idx| {
                if let Some(fixed) = fixed_widths[idx] {
                    let margin = margins[idx];
                    (fixed + margin.left as usize + margin.right as usize).max(1)
                } else {
                    let extra = if flex_seen < remainder { 1 } else { 0 };
                    flex_seen += 1;
                    (base + extra).max(1)
                }
            })
            .collect();

        if std::env::var("TEXTUAL_DEBUG_LAYOUT_FILE").is_ok() {
            debug_layout(&format!(
                "[row] id={} widths={:?} remaining={} flex_count={} base={} remainder={}",
                0u64, widths, remaining, flex_count, base, remainder
            ));
        }

        let mut child_lines: Vec<Vec<Vec<Segment>>> = Vec::new();

        for (idx, child) in self.children.iter().enumerate() {
            let _resolved = &resolved_list[idx];
            let margin = margins[idx];
            let child_width = widths[idx].max(1);
            let constraints = constraints_list[idx];
            let render_width = clamp_with_constraints(
                child_width
                    .saturating_sub(margin.left as usize + margin.right as usize)
                    .max(1),
                constraints.min_width,
                constraints.max_width,
                child_width
                    .saturating_sub(margin.left as usize + margin.right as usize)
                    .max(1),
            );
            let render_height = clamp_with_constraints(
                height_limit
                    .saturating_sub(margin.top as usize + margin.bottom as usize)
                    .max(1),
                constraints.min_height,
                constraints.max_height,
                height_limit
                    .saturating_sub(margin.top as usize + margin.bottom as usize)
                    .max(1),
            );
            let render_height = if let Some(fixed_total) = child.layout_height() {
                render_height.min(fixed_total.max(1))
            } else {
                render_height
            };
            let mut child_options = options.clone();
            child_options.size = (render_width, render_height);
            child_options.max_width = render_width;
            child_options.max_height = render_height;

            let segments = child.render_styled(console, &child_options);
            let mut lines =
                Segment::split_and_crop_lines(segments, render_width, None, true, false);
            let mut target_height = child.layout_height().unwrap_or(lines.len().max(1));
            target_height = clamp_with_constraints(
                target_height,
                constraints.min_height,
                constraints.max_height,
                height_limit,
            );
            lines = Segment::set_shape(&lines, render_width, Some(target_height), None, false);
            lines = pad_lines_to_width(lines, render_width);
            lines = apply_margin(lines, child_width, margin);
            child_lines.push(lines);
        }

        let max_child_height = child_lines
            .iter()
            .map(|lines| lines.len())
            .max()
            .unwrap_or(1)
            .max(1)
            .min(height_limit);

        let mut normalized_lines: Vec<Vec<Vec<Segment>>> = Vec::new();
        for lines in child_lines {
            let height = lines.len().max(1);
            let (pad_top, pad_bottom) = match self.align {
                RowAlign::Top => (0, max_child_height.saturating_sub(height)),
                RowAlign::Center => {
                    let total = max_child_height.saturating_sub(height);
                    (total / 2, total - total / 2)
                }
                RowAlign::Bottom => (max_child_height.saturating_sub(height), 0),
            };
            let mut padded = Vec::new();
            for _ in 0..pad_top {
                padded.push(Vec::new());
            }
            padded.extend(lines);
            for _ in 0..pad_bottom {
                padded.push(Vec::new());
            }
            normalized_lines.push(padded);
        }

        let mut out_lines: Vec<Vec<Segment>> = Vec::new();
        for row in 0..max_child_height {
            let mut line: Vec<Segment> = Vec::new();
            for (idx, lines) in normalized_lines.iter().enumerate() {
                let child_width = widths.get(idx).copied().unwrap_or(1).max(1);
                let child_line = lines
                    .get(row)
                    .cloned()
                    .unwrap_or_else(|| vec![Segment::new(" ".repeat(child_width))]);
                let adjusted = adjust_line_length_no_bg(&child_line, child_width);
                line.extend(adjusted);
            }
            out_lines.push(line);
        }

        let mut out_lines = out_lines;
        out_lines.truncate(max_child_height);
        while out_lines.len() < max_child_height {
            out_lines.push(Vec::new());
        }
        let out_lines = pad_lines_to_width(out_lines, width);
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

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
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
        let mut fixed_widths: Vec<Option<usize>> = vec![None; count];
        let mut margins: Vec<Margin> = vec![Margin::default(); count];
        let mut constraints_list: Vec<LayoutConstraints> = Vec::with_capacity(count);
        let mut resolved_list: Vec<crate::style::Style> = Vec::with_capacity(count);

        for (idx, child) in self.children.iter().enumerate() {
            let meta = css::selector_meta_generic(child.as_ref());
            let resolved = css::resolve_style(child.as_ref(), &meta);
            let margin = margin_from_style(&resolved);
            let style_constraints = constraints_from_style(&resolved);
            let constraints = merge_constraints(style_constraints, child.layout_constraints());

            let fixed =
                if let (Some(min), Some(max)) = (constraints.min_width, constraints.max_width) {
                    if min == max { Some(min) } else { None }
                } else if matches!(resolved.width, Some(Scalar::Auto)) {
                    let pad = resolved
                        .padding
                        .map(|s| s.left as usize)
                        .unwrap_or(0)
                        .saturating_mul(2);
                    let (_, _, border_left, border_right) =
                        super::helpers::border_spacing_from_style(&resolved);
                    child
                        .content_width()
                        .map(|w| w.saturating_add(pad + border_left + border_right).max(1))
                } else {
                    None
                };

            fixed_widths[idx] = fixed;
            margins[idx] = margin;
            constraints_list.push(constraints);
            resolved_list.push(resolved);
        }

        let mut fixed_total = 0usize;
        let mut flex_count = 0usize;
        for (idx, fixed) in fixed_widths.iter().enumerate() {
            if let Some(width) = fixed {
                let margin = margins[idx];
                fixed_total = fixed_total
                    .saturating_add(width + margin.left as usize + margin.right as usize);
            } else {
                flex_count += 1;
            }
        }

        let remaining = width.saturating_sub(fixed_total);
        let base = if flex_count > 0 {
            remaining / flex_count
        } else {
            0
        };
        let remainder = if flex_count > 0 {
            remaining % flex_count
        } else {
            0
        };

        let mut flex_seen = 0usize;
        let widths: Vec<usize> = (0..count)
            .map(|idx| {
                if let Some(fixed) = fixed_widths[idx] {
                    let margin = margins[idx];
                    (fixed + margin.left as usize + margin.right as usize).max(1)
                } else {
                    let extra = if flex_seen < remainder { 1 } else { 0 };
                    flex_seen += 1;
                    (base + extra).max(1)
                }
            })
            .collect();

        let mut child_lines: Vec<Vec<Vec<Segment>>> = Vec::new();

        for (idx, child) in self.children.iter().enumerate() {
            let child_width = widths[idx].max(1);
            let constraints = constraints_list[idx];
            let margin = margins[idx];
            let render_width = clamp_with_constraints(
                child_width
                    .saturating_sub(margin.left as usize + margin.right as usize)
                    .max(1),
                constraints.min_width,
                constraints.max_width,
                child_width
                    .saturating_sub(margin.left as usize + margin.right as usize)
                    .max(1),
            );
            let render_height = clamp_with_constraints(
                height_limit,
                constraints.min_height,
                constraints.max_height,
                height_limit,
            );
            let mut child_options = options.clone();
            child_options.size = (render_width, render_height);
            child_options.max_width = render_width;
            child_options.max_height = render_height;

            let segments = child.render_styled(console, &child_options);
            let mut lines =
                Segment::split_and_crop_lines(segments, render_width, None, true, false);
            let mut target_height = child.layout_height().unwrap_or(lines.len().max(1));
            target_height = clamp_with_constraints(
                target_height,
                constraints.min_height,
                constraints.max_height,
                height_limit,
            );
            lines = Segment::set_shape(&lines, render_width, Some(target_height), None, false);
            lines = pad_lines_to_width(lines, render_width);
            lines = apply_margin(lines, child_width, margin);
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

        let mut normalized_lines: Vec<Vec<Vec<Segment>>> = Vec::new();
        for lines in child_lines {
            let height = lines.len().max(1);
            let (pad_top, pad_bottom) = match self.align {
                RowAlign::Top => (0, max_child_height.saturating_sub(height)),
                RowAlign::Center => {
                    let total = max_child_height.saturating_sub(height);
                    (total / 2, total - total / 2)
                }
                RowAlign::Bottom => (max_child_height.saturating_sub(height), 0),
            };
            let mut padded = Vec::new();
            for _ in 0..pad_top {
                padded.push(Vec::new());
            }
            padded.extend(lines);
            for _ in 0..pad_bottom {
                padded.push(Vec::new());
            }
            normalized_lines.push(padded);
        }

        let mut out_lines: Vec<Vec<Segment>> = Vec::new();
        for row in 0..max_child_height {
            let mut line: Vec<Segment> = Vec::new();
            for (idx, lines) in normalized_lines.iter().enumerate() {
                let child_width = widths.get(idx).copied().unwrap_or(1).max(1);
                let child_line = lines
                    .get(row)
                    .cloned()
                    .unwrap_or_else(|| vec![Segment::new(" ".repeat(child_width))]);
                let adjusted = adjust_line_length_no_bg(&child_line, child_width);
                line.extend(adjusted);
            }
            out_lines.push(line);
        }

        let mut out_lines = out_lines;
        out_lines.truncate(max_child_height);
        while out_lines.len() < max_child_height {
            out_lines.push(Vec::new());
        }
        let out_lines = pad_lines_to_width(out_lines, width);
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
        self.last_layout_width = width;
        for child in &mut self.children {
            child.on_resize(width, height);
        }
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.last_layout_width = width;
        for child in &mut self.children {
            child.on_layout(width, height);
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
        match event {
            Event::MouseDown(mouse) => {
                if let Some((idx, local_x)) = self.child_at_x(mouse.x) {
                    let child_event = Event::MouseDown(crate::event::MouseDownEvent {
                        target: NodeId::default(),
                        screen_x: mouse.screen_x,
                        screen_y: mouse.screen_y,
                        x: local_x,
                        y: mouse.y,
                    });
                    if let Some(child) = self.children.get_mut(idx) {
                        child.on_event(&child_event, ctx);
                    }
                }
                return;
            }
            Event::MouseUp(mouse) => {
                if let Some((idx, local_x)) = self.child_at_x(mouse.x) {
                    let child_event = Event::MouseUp(crate::event::MouseUpEvent {
                        target: Some(NodeId::default()),
                        screen_x: mouse.screen_x,
                        screen_y: mouse.screen_y,
                        x: local_x,
                        y: mouse.y,
                    });
                    if let Some(child) = self.children.get_mut(idx) {
                        child.on_event(&child_event, ctx);
                    }
                }
                return;
            }
            Event::MouseScroll(mouse) => {
                if let Some((idx, local_x)) = self.child_at_x(mouse.x) {
                    let child_event = Event::MouseScroll(crate::event::MouseScrollEvent {
                        target: Some(NodeId::default()),
                        screen_x: mouse.screen_x,
                        screen_y: mouse.screen_y,
                        x: local_x,
                        y: mouse.y,
                        delta_x: mouse.delta_x,
                        delta_y: mouse.delta_y,
                        modifiers: mouse.modifiers,
                    });
                    if let Some(child) = self.children.get_mut(idx) {
                        child.on_event(&child_event, ctx);
                    }
                }
                return;
            }
            _ => {}
        }
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
    items: Vec<DockItem>,
    fixed_height: Option<usize>,
    styles: WidgetStyles,
}

impl Dock {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            fixed_height: None,
            styles: WidgetStyles::default(),
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
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let mut remaining_width = options.size.0.max(1);
        let mut remaining_height = self.fixed_height.unwrap_or_else(|| options.size.1.max(1));

        let mut top_lines: Vec<Vec<Segment>> = Vec::new();
        let mut bottom_lines: Vec<Vec<Segment>> = Vec::new();

        let mut left_columns: Vec<(usize, Vec<Vec<Segment>>)> = Vec::new();
        let mut right_columns: Vec<(usize, Vec<Vec<Segment>>)> = Vec::new();
        let mut fill_lines: Option<Vec<Vec<Segment>>> = None;
        let mut fill_index: Option<usize> = None;

        for (idx, item) in self.items.iter().enumerate() {
            match item.kind {
                DockKind::Top => {
                    let height = item
                        .size
                        .or_else(|| item.child.layout_height())
                        .unwrap_or(1)
                        .min(remaining_height);
                    let constraints = item.child.layout_constraints();
                    let render_height = clamp_with_constraints(
                        height,
                        constraints.min_height,
                        constraints.max_height,
                        height,
                    );
                    let render_width = clamp_with_constraints(
                        remaining_width,
                        constraints.min_width,
                        constraints.max_width,
                        remaining_width,
                    );
                    let mut child_options = options.clone();
                    child_options.size = (render_width, render_height);
                    child_options.max_width = render_width;
                    child_options.max_height = render_height;
                    let segments = item.child.render_styled(console, &child_options);
                    let mut lines =
                        Segment::split_and_crop_lines(segments, render_width, None, true, false);
                    lines =
                        Segment::set_shape(&lines, render_width, Some(render_height), None, false);
                    lines = pad_lines_to_width(lines, remaining_width);
                    top_lines.extend(lines);
                    remaining_height = remaining_height.saturating_sub(height);
                }
                DockKind::Bottom => {
                    let height = item
                        .size
                        .or_else(|| item.child.layout_height())
                        .unwrap_or(1)
                        .min(remaining_height);
                    let constraints = item.child.layout_constraints();
                    let render_height = clamp_with_constraints(
                        height,
                        constraints.min_height,
                        constraints.max_height,
                        height,
                    );
                    let render_width = clamp_with_constraints(
                        remaining_width,
                        constraints.min_width,
                        constraints.max_width,
                        remaining_width,
                    );
                    let mut child_options = options.clone();
                    child_options.size = (render_width, render_height);
                    child_options.max_width = render_width;
                    child_options.max_height = render_height;
                    let segments = item.child.render_styled(console, &child_options);
                    let mut lines =
                        Segment::split_and_crop_lines(segments, render_width, None, true, false);
                    lines =
                        Segment::set_shape(&lines, render_width, Some(render_height), None, false);
                    lines = pad_lines_to_width(lines, remaining_width);
                    bottom_lines.extend(lines);
                    remaining_height = remaining_height.saturating_sub(height);
                }
                DockKind::Left => {
                    let width = item.size.unwrap_or(1).min(remaining_width);
                    let constraints = item.child.layout_constraints();
                    let render_width = clamp_with_constraints(
                        width,
                        constraints.min_width,
                        constraints.max_width,
                        width,
                    );
                    let render_height = clamp_with_constraints(
                        remaining_height,
                        constraints.min_height,
                        constraints.max_height,
                        remaining_height,
                    );
                    let mut child_options = options.clone();
                    child_options.size = (render_width, render_height);
                    child_options.max_width = render_width;
                    child_options.max_height = render_height;
                    let segments = item.child.render_styled(console, &child_options);
                    let mut lines =
                        Segment::split_and_crop_lines(segments, render_width, None, true, false);
                    lines =
                        Segment::set_shape(&lines, render_width, Some(render_height), None, false);
                    lines = pad_lines_to_width(lines, width);
                    left_columns.push((width, lines));
                    remaining_width = remaining_width.saturating_sub(width);
                }
                DockKind::Right => {
                    let width = item.size.unwrap_or(1).min(remaining_width);
                    let constraints = item.child.layout_constraints();
                    let render_width = clamp_with_constraints(
                        width,
                        constraints.min_width,
                        constraints.max_width,
                        width,
                    );
                    let render_height = clamp_with_constraints(
                        remaining_height,
                        constraints.min_height,
                        constraints.max_height,
                        remaining_height,
                    );
                    let mut child_options = options.clone();
                    child_options.size = (render_width, render_height);
                    child_options.max_width = render_width;
                    child_options.max_height = render_height;
                    let segments = item.child.render_styled(console, &child_options);
                    let mut lines =
                        Segment::split_and_crop_lines(segments, render_width, None, true, false);
                    lines =
                        Segment::set_shape(&lines, render_width, Some(render_height), None, false);
                    lines = pad_lines_to_width(lines, width);
                    right_columns.push((width, lines));
                    remaining_width = remaining_width.saturating_sub(width);
                }
                DockKind::Fill => {
                    fill_index = Some(idx);
                }
            }
        }

        if let Some(idx) = fill_index {
            let item = &self.items[idx];
            let constraints = item.child.layout_constraints();
            let render_width = clamp_with_constraints(
                remaining_width,
                constraints.min_width,
                constraints.max_width,
                remaining_width,
            );
            let render_height = clamp_with_constraints(
                remaining_height,
                constraints.min_height,
                constraints.max_height,
                remaining_height,
            );
            let mut child_options = options.clone();
            child_options.size = (render_width, render_height);
            child_options.max_width = render_width;
            child_options.max_height = render_height;
            let segments = item.child.render_styled(console, &child_options);
            let mut lines =
                Segment::split_and_crop_lines(segments, render_width, None, true, false);
            lines = Segment::set_shape(&lines, render_width, Some(render_height), None, false);
            lines = pad_lines_to_width(lines, remaining_width);
            fill_lines = Some(lines);
        }

        let mut middle_lines: Vec<Vec<Segment>> = Vec::new();
        for row in 0..remaining_height {
            let mut line: Vec<Segment> = Vec::new();

            for (col_width, column) in &left_columns {
                let col_line = column
                    .get(row)
                    .cloned()
                    .unwrap_or_else(|| vec![Segment::new(" ".repeat(*col_width))]);
                let adjusted = Segment::adjust_line_length(&col_line, *col_width, None, true);
                line.extend(adjusted);
            }

            let remaining_mid_width = remaining_width;
            if let Some(lines) = &fill_lines {
                let fill_line = lines
                    .get(row)
                    .cloned()
                    .unwrap_or_else(|| vec![Segment::new(" ".repeat(remaining_mid_width))]);
                let adjusted =
                    Segment::adjust_line_length(&fill_line, remaining_mid_width, None, true);
                line.extend(adjusted);
            } else {
                line.extend(vec![Segment::new(" ".repeat(remaining_mid_width))]);
            }

            for (col_width, column) in &right_columns {
                let col_line = column
                    .get(row)
                    .cloned()
                    .unwrap_or_else(|| vec![Segment::new(" ".repeat(*col_width))]);
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

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }

    fn render_with_debug(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        debug: &DebugLayout,
    ) -> Segments {
        let mut remaining_width = options.size.0.max(1);
        let mut remaining_height = self.fixed_height.unwrap_or_else(|| options.size.1.max(1));

        let mut top_lines: Vec<Vec<Segment>> = Vec::new();
        let mut bottom_lines: Vec<Vec<Segment>> = Vec::new();

        let mut left_columns: Vec<(usize, Vec<Vec<Segment>>)> = Vec::new();
        let mut right_columns: Vec<(usize, Vec<Vec<Segment>>)> = Vec::new();
        let mut fill_lines: Option<Vec<Vec<Segment>>> = None;
        let mut fill_index: Option<usize> = None;

        for (idx, item) in self.items.iter().enumerate() {
            match item.kind {
                DockKind::Top => {
                    let height = item
                        .size
                        .or_else(|| item.child.layout_height())
                        .unwrap_or(1)
                        .min(remaining_height);
                    let constraints = item.child.layout_constraints();
                    let render_height = clamp_with_constraints(
                        height,
                        constraints.min_height,
                        constraints.max_height,
                        height,
                    );
                    let render_width = clamp_with_constraints(
                        remaining_width,
                        constraints.min_width,
                        constraints.max_width,
                        remaining_width,
                    );
                    let mut child_options = options.clone();
                    child_options.size = (render_width, render_height);
                    child_options.max_width = render_width;
                    child_options.max_height = render_height;
                    let segments = item.child.render_styled(console, &child_options);
                    let mut lines =
                        Segment::split_and_crop_lines(segments, render_width, None, true, false);
                    lines =
                        Segment::set_shape(&lines, render_width, Some(render_height), None, false);
                    lines = pad_lines_to_width(lines, remaining_width);
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
                    let constraints = item.child.layout_constraints();
                    let render_height = clamp_with_constraints(
                        height,
                        constraints.min_height,
                        constraints.max_height,
                        height,
                    );
                    let render_width = clamp_with_constraints(
                        remaining_width,
                        constraints.min_width,
                        constraints.max_width,
                        remaining_width,
                    );
                    let mut child_options = options.clone();
                    child_options.size = (render_width, render_height);
                    child_options.max_width = render_width;
                    child_options.max_height = render_height;
                    let segments = item.child.render_styled(console, &child_options);
                    let mut lines =
                        Segment::split_and_crop_lines(segments, render_width, None, true, false);
                    lines =
                        Segment::set_shape(&lines, render_width, Some(render_height), None, false);
                    lines = pad_lines_to_width(lines, remaining_width);
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
                    let constraints = item.child.layout_constraints();
                    let render_width = clamp_with_constraints(
                        width,
                        constraints.min_width,
                        constraints.max_width,
                        width,
                    );
                    let render_height = clamp_with_constraints(
                        remaining_height,
                        constraints.min_height,
                        constraints.max_height,
                        remaining_height,
                    );
                    let mut child_options = options.clone();
                    child_options.size = (render_width, render_height);
                    child_options.max_width = render_width;
                    child_options.max_height = render_height;
                    let segments = item.child.render_styled(console, &child_options);
                    let mut lines =
                        Segment::split_and_crop_lines(segments, render_width, None, true, false);
                    lines =
                        Segment::set_shape(&lines, render_width, Some(render_height), None, false);
                    lines = pad_lines_to_width(lines, width);
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
                    let constraints = item.child.layout_constraints();
                    let render_width = clamp_with_constraints(
                        width,
                        constraints.min_width,
                        constraints.max_width,
                        width,
                    );
                    let render_height = clamp_with_constraints(
                        remaining_height,
                        constraints.min_height,
                        constraints.max_height,
                        remaining_height,
                    );
                    let mut child_options = options.clone();
                    child_options.size = (render_width, render_height);
                    child_options.max_width = render_width;
                    child_options.max_height = render_height;
                    let segments = item.child.render_styled(console, &child_options);
                    let mut lines =
                        Segment::split_and_crop_lines(segments, render_width, None, true, false);
                    lines =
                        Segment::set_shape(&lines, render_width, Some(render_height), None, false);
                    lines = pad_lines_to_width(lines, width);
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
                    fill_index = Some(idx);
                }
            }
        }

        if let Some(idx) = fill_index {
            let item = &self.items[idx];
            let constraints = item.child.layout_constraints();
            let render_width = clamp_with_constraints(
                remaining_width,
                constraints.min_width,
                constraints.max_width,
                remaining_width,
            );
            let render_height = clamp_with_constraints(
                remaining_height,
                constraints.min_height,
                constraints.max_height,
                remaining_height,
            );
            let mut child_options = options.clone();
            child_options.size = (render_width, render_height);
            child_options.max_width = render_width;
            child_options.max_height = render_height;
            let segments = item.child.render_styled(console, &child_options);
            let mut lines =
                Segment::split_and_crop_lines(segments, render_width, None, true, false);
            lines = Segment::set_shape(&lines, render_width, Some(render_height), None, false);
            lines = pad_lines_to_width(lines, remaining_width);
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

        let mut middle_lines: Vec<Vec<Segment>> = Vec::new();
        for row in 0..remaining_height {
            let mut line: Vec<Segment> = Vec::new();

            for (col_width, column) in &left_columns {
                let col_line = column
                    .get(row)
                    .cloned()
                    .unwrap_or_else(|| vec![Segment::new(" ".repeat(*col_width))]);
                let adjusted = Segment::adjust_line_length(&col_line, *col_width, None, true);
                line.extend(adjusted);
            }

            let remaining_mid_width = remaining_width;
            if let Some(lines) = &fill_lines {
                let fill_line = lines
                    .get(row)
                    .cloned()
                    .unwrap_or_else(|| vec![Segment::new(" ".repeat(remaining_mid_width))]);
                let adjusted =
                    Segment::adjust_line_length(&fill_line, remaining_mid_width, None, true);
                line.extend(adjusted);
            } else {
                line.extend(vec![Segment::new(" ".repeat(remaining_mid_width))]);
            }

            for (col_width, column) in &right_columns {
                let col_line = column
                    .get(row)
                    .cloned()
                    .unwrap_or_else(|| vec![Segment::new(" ".repeat(*col_width))]);
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
        if let Some(fixed) = fixed_height_from_constraints(self.layout_constraints()) {
            return Some(fixed);
        }
        self.fixed_height
    }
}

impl Renderable for Dock {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

pub struct Grid {
    rows: usize,
    cols: usize,
    cells: Vec<Option<Box<dyn Widget>>>,
    row_gaps: usize,
    col_gaps: usize,
    row_sizes: Option<Vec<usize>>,
    col_sizes: Option<Vec<usize>>,
    styles: WidgetStyles,
}

impl Grid {
    pub fn new(rows: usize, cols: usize) -> Self {
        let rows = rows.max(1);
        let cols = cols.max(1);
        Self {
            rows,
            cols,
            cells: (0..rows * cols).map(|_| None).collect(),
            row_gaps: 0,
            col_gaps: 0,
            row_sizes: None,
            col_sizes: None,
            styles: WidgetStyles::default(),
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
                let (margin, constraints) = if let Some(child) = &self.cells[idx] {
                    let meta = css::selector_meta_generic(child.as_ref());
                    let resolved = css::resolve_style(child.as_ref(), &meta);
                    let style_constraints = constraints_from_style(&resolved);
                    (
                        margin_from_style(&resolved),
                        merge_constraints(style_constraints, child.layout_constraints()),
                    )
                } else {
                    (Margin::default(), LayoutConstraints::default())
                };
                let render_width = clamp_with_constraints(
                    cell_width
                        .saturating_sub(margin.left as usize + margin.right as usize)
                        .max(1),
                    constraints.min_width,
                    constraints.max_width,
                    cell_width
                        .saturating_sub(margin.left as usize + margin.right as usize)
                        .max(1),
                );
                let render_height = clamp_with_constraints(
                    cell_height
                        .saturating_sub(margin.top as usize + margin.bottom as usize)
                        .max(1),
                    constraints.min_height,
                    constraints.max_height,
                    cell_height
                        .saturating_sub(margin.top as usize + margin.bottom as usize)
                        .max(1),
                );
                let mut child_options = options.clone();
                child_options.size = (render_width, render_height);
                child_options.max_width = render_width;
                child_options.max_height = render_height;
                let lines = if let Some(child) = &self.cells[idx] {
                    let segments = child.render_styled(console, &child_options);
                    let mut lines =
                        Segment::split_and_crop_lines(segments, render_width, None, true, false);
                    lines =
                        Segment::set_shape(&lines, render_width, Some(render_height), None, false);
                    lines = pad_lines_to_width(lines, render_width);
                    lines = apply_margin(lines, cell_width, margin);
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
                    let cell_line = lines
                        .get(row)
                        .cloned()
                        .unwrap_or_else(|| vec![Segment::new(" ".repeat(cell_width))]);
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
                let (margin, constraints) = if let Some(child) = &self.cells[idx] {
                    let meta = css::selector_meta_generic(child.as_ref());
                    let resolved = css::resolve_style(child.as_ref(), &meta);
                    let style_constraints = constraints_from_style(&resolved);
                    (
                        margin_from_style(&resolved),
                        merge_constraints(style_constraints, child.layout_constraints()),
                    )
                } else {
                    (Margin::default(), LayoutConstraints::default())
                };
                let render_width = clamp_with_constraints(
                    cell_width
                        .saturating_sub(margin.left as usize + margin.right as usize)
                        .max(1),
                    constraints.min_width,
                    constraints.max_width,
                    cell_width
                        .saturating_sub(margin.left as usize + margin.right as usize)
                        .max(1),
                );
                let render_height = clamp_with_constraints(
                    cell_height
                        .saturating_sub(margin.top as usize + margin.bottom as usize)
                        .max(1),
                    constraints.min_height,
                    constraints.max_height,
                    cell_height
                        .saturating_sub(margin.top as usize + margin.bottom as usize)
                        .max(1),
                );
                let mut child_options = options.clone();
                child_options.size = (render_width, render_height);
                child_options.max_width = render_width;
                child_options.max_height = render_height;
                let lines = if let Some(child) = &self.cells[idx] {
                    let segments = child.render_styled(console, &child_options);
                    let mut lines =
                        Segment::split_and_crop_lines(segments, render_width, None, true, false);
                    lines =
                        Segment::set_shape(&lines, render_width, Some(render_height), None, false);
                    lines = pad_lines_to_width(lines, render_width);
                    lines = apply_margin(lines, cell_width, margin);
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
                    let cell_line = lines
                        .get(row)
                        .cloned()
                        .unwrap_or_else(|| vec![Segment::new(" ".repeat(cell_width))]);
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

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

impl Renderable for Grid {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}
