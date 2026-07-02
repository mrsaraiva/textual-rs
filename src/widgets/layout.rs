use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::compose::ComposeResult;
use crate::css;
use crate::debug::{DebugLayout, debug_input, debug_layout};
use crate::event::{
    Action, BlurEvent, Event, EventCtx, FocusEvent, MouseEnterEvent, MouseLeaveEvent,
};
use crate::node_id::NodeId;

use super::{
    LayoutConstraints, NodeSeed, Widget,
    helpers::{
        adjust_line_length_no_bg, apply_debug_box, apply_margin, clamp_with_constraints,
        constraints_from_style, margin_from_style, pad_lines_to_width,
    },
};
use crate::style::{BoxSizing, Dock as StyleDock, Margin, Scalar};

pub struct Row {
    children: Vec<Box<dyn Widget>>,
    children_extracted: bool,
    align: RowAlign,
    last_layout_width: u16,
    seed: NodeSeed,
    /// Index of the currently focused child (non-tree mode only).
    focused_child: Option<usize>,
    /// Index of the currently hovered child (non-tree mode only).
    hovered_child: Option<usize>,
    /// (index into `children`, css_id, classes) recorded by `with_compose` so
    /// `.with_id()`/`.with_classes()` metadata on declared children reaches the
    /// mounted node (mirrors `Container::with_compose`).
    child_decl_meta: Vec<crate::widgets::ChildDeclMeta>,
    /// (index into `children`, sink) recorded by `with_compose` for decls bound
    /// via `HandleSlot::bind`.
    child_handle_sinks: Vec<(usize, crate::handle::HandleSink)>,
}

impl Default for Row {
    fn default() -> Self {
        Self::new()
    }
}

impl Row {
    crate::seed_ident_methods!();

    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            children_extracted: false,
            align: RowAlign::Top,
            last_layout_width: 0,
            seed: NodeSeed::default(),
            focused_child: None,
            hovered_child: None,
            child_decl_meta: Vec::new(),
            child_handle_sinks: Vec::new(),
        }
    }

    pub fn with_child(mut self, child: impl Widget + 'static) -> Self {
        self.children.push(Box::new(child));
        self
    }

    /// Add multiple children from a `compose![]` result.
    ///
    /// Preserves each `ChildDecl`'s `id`/`classes` (so CSS id/class selectors
    /// match the mounted nodes) and any `handle_sink` bound via `HandleSlot::bind`,
    /// mirroring `Container::with_compose`.
    pub fn with_compose(mut self, children: ComposeResult) -> Self {
        for decl in children {
            let crate::compose::ChildDecl {
                builder,
                id,
                classes,
                handle_sink,
                ..
            } = decl;
            let crate::compose::WidgetBuilder::Ready(widget) = builder;
            let index = self.children.len();
            self.children.push(widget);
            if id.is_some() || !classes.is_empty() {
                self.child_decl_meta.push((index, id, classes));
            }
            if let Some(sink) = handle_sink {
                self.child_handle_sinks.push((index, sink));
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

    fn is_tree_mode(&self) -> bool {
        self.children_extracted
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

    /// Cycle focus to the next/prev focusable child (non-tree mode).
    ///
    /// Dispatches `Event::Blur` to the previously focused child and
    /// `Event::Focus` to the next one, updating `self.focused_child`.
    fn cycle_focus(&mut self, action: Action) -> bool {
        let mut focusable: Vec<usize> = Vec::new();
        let mut current_pos: Option<usize> = None;
        for (idx, child) in self.children.iter().enumerate() {
            if child.focusable() {
                if self.focused_child == Some(idx) {
                    current_pos = Some(focusable.len());
                }
                focusable.push(idx);
            }
        }
        if focusable.is_empty() {
            return false;
        }
        let next_pos = match (action, current_pos) {
            (Action::FocusNext, Some(pos)) => (pos + 1) % focusable.len(),
            (Action::FocusPrev, Some(0)) | (Action::FocusPrev, None) => focusable.len() - 1,
            (Action::FocusPrev, Some(pos)) => pos - 1,
            (Action::FocusNext, None) => 0,
            _ => return false,
        };
        let next_idx = focusable[next_pos];
        if self.focused_child == Some(next_idx) {
            return false;
        }
        // Blur the previously focused child.
        if let Some(prev_idx) = self.focused_child {
            let blur = Event::Blur(BlurEvent {
                node: NodeId::default(),
            });
            let mut ctx = EventCtx::default();
            if let Some(child) = self.children.get_mut(prev_idx) {
                child.on_event(&blur, &mut ctx);
            }
        }
        self.focused_child = Some(next_idx);
        // Focus the new child.
        let focus = Event::Focus(FocusEvent {
            node: NodeId::default(),
        });
        let mut ctx = EventCtx::default();
        if let Some(child) = self.children.get_mut(next_idx) {
            child.on_event(&focus, &mut ctx);
        }
        true
    }
}

#[derive(Debug, Clone, Copy)]
pub enum RowAlign {
    Top,
    Center,
    Bottom,
}

impl Widget for Row {
    fn compose(&self) -> ComposeResult {
        Vec::new()
    }

    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        self.children_extracted = true;
        std::mem::take(&mut self.children)
    }

    fn take_child_decl_meta(&mut self) -> Vec<crate::widgets::ChildDeclMeta> {
        std::mem::take(&mut self.child_decl_meta)
    }

    fn take_child_handle_sinks(&mut self) -> Vec<(usize, crate::handle::HandleSink)> {
        std::mem::take(&mut self.child_handle_sinks)
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height_limit = options.size.1.max(1);

        if self.is_tree_mode() {
            let blank = vec![Segment::new(" ".repeat(width))];
            let mut out = Segments::new();
            for row in 0..height_limit {
                out.extend(blank.clone());
                if row + 1 < height_limit {
                    out.push(Segment::line());
                }
            }
            return out;
        }

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
            let constraints = style_constraints;

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
        let base = remaining.checked_div(flex_count).unwrap_or(0);
        let remainder = remaining.checked_rem(flex_count).unwrap_or(0);

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

    fn set_inline_style(&mut self, style: crate::style::Style) {
        self.seed.styles.style = style;
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }

    fn render_with_debug(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        debug: &DebugLayout,
    ) -> Segments {
        if self.is_tree_mode() {
            return Widget::render(self, console, options);
        }

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
            let constraints = style_constraints;

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
        let base = remaining.checked_div(flex_count).unwrap_or(0);
        let remainder = remaining.checked_rem(flex_count).unwrap_or(0);

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
        if !self.is_tree_mode() {
            for child in &mut self.children {
                child.on_mount();
            }
        }
    }

    fn on_unmount(&mut self) {
        if !self.is_tree_mode() {
            for child in &mut self.children {
                child.on_unmount();
            }
        }
    }

    fn on_tick(&mut self, tick: u64) {
        if !self.is_tree_mode() {
            for child in &mut self.children {
                child.on_tick(tick);
            }
        }
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        self.last_layout_width = width;
        if !self.is_tree_mode() {
            for child in &mut self.children {
                child.on_resize(width, height);
            }
        }
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.last_layout_width = width;
        if !self.is_tree_mode() {
            for child in &mut self.children {
                child.on_layout(width, height);
            }
        }
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        if self.is_tree_mode() {
            return;
        }
        for child in &mut self.children {
            child.on_event_capture(event, ctx);
            if ctx.handled() {
                break;
            }
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if self.is_tree_mode() {
            return;
        }
        match event {
            Event::Action(Action::FocusNext) | Event::Action(Action::FocusPrev) => {
                if let Event::Action(action) = event {
                    if self.cycle_focus(*action) {
                        ctx.request_repaint();
                        ctx.set_handled();
                    }
                }
                return;
            }
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

    fn on_mouse_move(&mut self, x: u16, y: u16) -> bool {
        if self.is_tree_mode() {
            return false;
        }
        let hit = self.child_at_x(x);
        let new_hovered = hit.map(|(idx, _)| idx);
        let mut changed = false;
        debug_input(&format!(
            "[hover][row] x={} y={} hit={:?}",
            x,
            y,
            hit
        ));

        // Dispatch Enter/Leave events when the hovered child changes.
        if new_hovered != self.hovered_child {
            if let Some(prev_idx) = self.hovered_child {
                let leave = Event::Leave(MouseLeaveEvent {
                    screen_x: x,
                    screen_y: y,
                    x,
                    y,
                });
                let mut ctx = EventCtx::default();
                if let Some(child) = self.children.get_mut(prev_idx) {
                    child.on_event(&leave, &mut ctx);
                }
                changed = true;
            }
            self.hovered_child = new_hovered;
            if let Some(new_idx) = new_hovered {
                let enter = Event::Enter(MouseEnterEvent {
                    screen_x: x,
                    screen_y: y,
                    x,
                    y,
                });
                let mut ctx = EventCtx::default();
                if let Some(child) = self.children.get_mut(new_idx) {
                    child.on_event(&enter, &mut ctx);
                }
                changed = true;
            }
        }

        if let Some((idx, local_x)) = hit {
            if let Some(child) = self.children.get_mut(idx) {
                changed |= child.on_mouse_move(local_x, y);
            }
        }

        changed
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
    items_extracted: bool,
    fixed_height: Option<usize>,
    last_layout_width: AtomicUsize,
    last_layout_height: AtomicUsize,
    seed: NodeSeed,
}

impl Default for Dock {
    fn default() -> Self {
        Self::new()
    }
}

impl Dock {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            items_extracted: false,
            fixed_height: None,
            last_layout_width: AtomicUsize::new(1),
            last_layout_height: AtomicUsize::new(1),
            seed: NodeSeed::default(),
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

    fn is_tree_mode(&self) -> bool {
        self.items_extracted
    }

    fn apply_item_layout_hints(item: &mut DockItem) {
        let mut style = crate::style::Style::default();
        match item.kind {
            DockKind::Top => {
                style.dock = Some(StyleDock::Top);
                if let Some(height) = item.size {
                    style.height = Some(Scalar::Cells(height as u16));
                    // Dock API sizes are absolute band sizes (including chrome).
                    style.box_sizing = Some(BoxSizing::BorderBox);
                }
            }
            DockKind::Bottom => {
                style.dock = Some(StyleDock::Bottom);
                if let Some(height) = item.size {
                    style.height = Some(Scalar::Cells(height as u16));
                    // Dock API sizes are absolute band sizes (including chrome).
                    style.box_sizing = Some(BoxSizing::BorderBox);
                }
            }
            DockKind::Left => {
                style.dock = Some(StyleDock::Left);
                if let Some(width) = item.size {
                    style.width = Some(Scalar::Cells(width as u16));
                    // Dock API sizes are absolute band sizes (including chrome).
                    style.box_sizing = Some(BoxSizing::BorderBox);
                }
            }
            DockKind::Right => {
                style.dock = Some(StyleDock::Right);
                if let Some(width) = item.size {
                    style.width = Some(Scalar::Cells(width as u16));
                    // Dock API sizes are absolute band sizes (including chrome).
                    style.box_sizing = Some(BoxSizing::BorderBox);
                }
            }
            DockKind::Fill => {
                // No dock style for fill items.
            }
        }
        if style != crate::style::Style::default() {
            item.child.set_inline_style(style);
        }
    }

    fn child_at_xy(&self, x: u16, y: u16) -> Option<(usize, u16, u16, u16, u16)> {
        let mut x0 = 0u16;
        let mut y0 = 0u16;
        let mut width = self.last_layout_width.load(Ordering::Relaxed).max(1) as u16;
        let mut height = self.last_layout_height.load(Ordering::Relaxed).max(1) as u16;
        let mut fill_idx: Option<usize> = None;
        let mut fill_rect: Option<(u16, u16, u16, u16)> = None;

        for (idx, item) in self.items.iter().enumerate() {
            match item.kind {
                DockKind::Top => {
                    let h = item
                        .size
                        .or_else(|| item.child.layout_height())
                        .unwrap_or(1)
                        .max(1)
                        .min(height as usize) as u16;
                    if x >= x0
                        && x < x0.saturating_add(width)
                        && y >= y0
                        && y < y0.saturating_add(h)
                    {
                        return Some((idx, x.saturating_sub(x0), y.saturating_sub(y0), width, h));
                    }
                    y0 = y0.saturating_add(h);
                    height = height.saturating_sub(h);
                }
                DockKind::Bottom => {
                    let h = item
                        .size
                        .or_else(|| item.child.layout_height())
                        .unwrap_or(1)
                        .max(1)
                        .min(height as usize) as u16;
                    let by = y0.saturating_add(height.saturating_sub(h));
                    if x >= x0
                        && x < x0.saturating_add(width)
                        && y >= by
                        && y < by.saturating_add(h)
                    {
                        return Some((idx, x.saturating_sub(x0), y.saturating_sub(by), width, h));
                    }
                    height = height.saturating_sub(h);
                }
                DockKind::Left => {
                    let w = item.size.unwrap_or(1).max(1).min(width as usize) as u16;
                    if x >= x0
                        && x < x0.saturating_add(w)
                        && y >= y0
                        && y < y0.saturating_add(height)
                    {
                        return Some((idx, x.saturating_sub(x0), y.saturating_sub(y0), w, height));
                    }
                    x0 = x0.saturating_add(w);
                    width = width.saturating_sub(w);
                }
                DockKind::Right => {
                    let w = item.size.unwrap_or(1).max(1).min(width as usize) as u16;
                    let bx = x0.saturating_add(width.saturating_sub(w));
                    if x >= bx
                        && x < bx.saturating_add(w)
                        && y >= y0
                        && y < y0.saturating_add(height)
                    {
                        return Some((idx, x.saturating_sub(bx), y.saturating_sub(y0), w, height));
                    }
                    width = width.saturating_sub(w);
                }
                DockKind::Fill => {
                    fill_idx = Some(idx);
                    fill_rect = Some((x0, y0, width.max(1), height.max(1)));
                }
            }
        }

        if let (Some(idx), Some((fx, fy, fw, fh))) = (fill_idx, fill_rect)
            && x >= fx
            && x < fx.saturating_add(fw)
            && y >= fy
            && y < fy.saturating_add(fh)
        {
            return Some((idx, x.saturating_sub(fx), y.saturating_sub(fy), fw, fh));
        }
        None
    }
}

impl Widget for Dock {
    fn compose(&self) -> ComposeResult {
        Vec::new()
    }

    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        self.items_extracted = true;
        let mut children = Vec::with_capacity(self.items.len());
        for mut item in std::mem::take(&mut self.items) {
            Self::apply_item_layout_hints(&mut item);
            children.push(item.child);
        }
        children
    }

    fn focusable(&self) -> bool {
        if self.is_tree_mode() {
            return false;
        }
        self.items.iter().any(|item| item.child.focusable())
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        self.last_layout_width
            .store(options.size.0.max(1), Ordering::Relaxed);
        self.last_layout_height.store(
            self.fixed_height.unwrap_or_else(|| options.size.1.max(1)),
            Ordering::Relaxed,
        );

        if self.is_tree_mode() {
            let width = options.size.0.max(1);
            let height = self.fixed_height.unwrap_or_else(|| options.size.1.max(1));
            let blank = vec![Segment::new(" ".repeat(width))];
            let mut out = Segments::new();
            for row in 0..height {
                out.extend(blank.clone());
                if row + 1 < height {
                    out.push(Segment::line());
                }
            }
            return out;
        }

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
                    let constraints = {
                        let meta = css::selector_meta_generic(item.child.as_ref());
                        let resolved = css::resolve_style(item.child.as_ref(), &meta);
                        constraints_from_style(&resolved)
                    };
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
                    let constraints = {
                        let meta = css::selector_meta_generic(item.child.as_ref());
                        let resolved = css::resolve_style(item.child.as_ref(), &meta);
                        constraints_from_style(&resolved)
                    };
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
                    let constraints = {
                        let meta = css::selector_meta_generic(item.child.as_ref());
                        let resolved = css::resolve_style(item.child.as_ref(), &meta);
                        constraints_from_style(&resolved)
                    };
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
                    let constraints = {
                        let meta = css::selector_meta_generic(item.child.as_ref());
                        let resolved = css::resolve_style(item.child.as_ref(), &meta);
                        constraints_from_style(&resolved)
                    };
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
            let constraints = {
                let meta = css::selector_meta_generic(item.child.as_ref());
                let resolved = css::resolve_style(item.child.as_ref(), &meta);
                constraints_from_style(&resolved)
            };
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

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if self.is_tree_mode() {
            return;
        }
        match event {
            Event::Action(Action::FocusNext) | Event::Action(Action::FocusPrev) => {
                return;
            }
            Event::MouseDown(mouse) => {
                if let Some((idx, local_x, local_y, w, h)) = self.child_at_xy(mouse.x, mouse.y) {
                    if let Some(item) = self.items.get_mut(idx) {
                        item.child.on_layout(w, h);
                    }
                    let child_event = Event::MouseDown(crate::event::MouseDownEvent {
                        target: NodeId::default(),
                        screen_x: mouse.screen_x,
                        screen_y: mouse.screen_y,
                        x: local_x,
                        y: local_y,
                    });
                    if let Some(item) = self.items.get_mut(idx) {
                        item.child.on_event(&child_event, ctx);
                        if ctx.handled() {
                            return;
                        }
                    }
                }
            }
            Event::MouseUp(mouse) => {
                if let Some((idx, local_x, local_y, w, h)) = self.child_at_xy(mouse.x, mouse.y) {
                    if let Some(item) = self.items.get_mut(idx) {
                        item.child.on_layout(w, h);
                    }
                    let child_event = Event::MouseUp(crate::event::MouseUpEvent {
                        target: Some(NodeId::default()),
                        screen_x: mouse.screen_x,
                        screen_y: mouse.screen_y,
                        x: local_x,
                        y: local_y,
                    });
                    if let Some(item) = self.items.get_mut(idx) {
                        item.child.on_event(&child_event, ctx);
                        if ctx.handled() {
                            return;
                        }
                    }
                }
            }
            Event::MouseScroll(mouse) => {
                if let Some((idx, local_x, local_y, w, h)) = self.child_at_xy(mouse.x, mouse.y) {
                    if let Some(item) = self.items.get_mut(idx) {
                        item.child.on_layout(w, h);
                    }
                    let child_event = Event::MouseScroll(crate::event::MouseScrollEvent {
                        target: Some(NodeId::default()),
                        screen_x: mouse.screen_x,
                        screen_y: mouse.screen_y,
                        x: local_x,
                        y: local_y,
                        delta_x: mouse.delta_x,
                        delta_y: mouse.delta_y,
                        modifiers: mouse.modifiers,
                    });
                    if let Some(item) = self.items.get_mut(idx) {
                        item.child.on_event(&child_event, ctx);
                        if ctx.handled() {
                            return;
                        }
                    }
                }
            }
            _ => {}
        }
        for item in &mut self.items {
            item.child.on_event(event, ctx);
            if ctx.handled() {
                break;
            }
        }
    }

    fn on_mouse_move(&mut self, x: u16, y: u16) -> bool {
        if self.is_tree_mode() {
            return false;
        }
        let mut changed = false;
        let hit = self.child_at_xy(x, y);
        if let Some((idx, local_x, local_y, w, h)) = hit
            && let Some(item) = self.items.get_mut(idx)
        {
            item.child.on_layout(w, h);
            changed |= item.child.on_mouse_move(local_x, local_y);
        }
        changed
    }

    fn set_inline_style(&mut self, style: crate::style::Style) {
        self.seed.styles.style = style;
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }

    fn render_with_debug(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        debug: &DebugLayout,
    ) -> Segments {
        if self.is_tree_mode() {
            return Widget::render(self, console, options);
        }

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
                    let constraints = {
                        let meta = css::selector_meta_generic(item.child.as_ref());
                        let resolved = css::resolve_style(item.child.as_ref(), &meta);
                        constraints_from_style(&resolved)
                    };
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
                    let constraints = {
                        let meta = css::selector_meta_generic(item.child.as_ref());
                        let resolved = css::resolve_style(item.child.as_ref(), &meta);
                        constraints_from_style(&resolved)
                    };
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
                    let constraints = {
                        let meta = css::selector_meta_generic(item.child.as_ref());
                        let resolved = css::resolve_style(item.child.as_ref(), &meta);
                        constraints_from_style(&resolved)
                    };
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
                    let constraints = {
                        let meta = css::selector_meta_generic(item.child.as_ref());
                        let resolved = css::resolve_style(item.child.as_ref(), &meta);
                        constraints_from_style(&resolved)
                    };
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
            let constraints = {
                let meta = css::selector_meta_generic(item.child.as_ref());
                let resolved = css::resolve_style(item.child.as_ref(), &meta);
                constraints_from_style(&resolved)
            };
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
        if !self.is_tree_mode() {
            for item in &mut self.items {
                item.child.on_mount();
            }
        }
    }

    fn on_unmount(&mut self) {
        if !self.is_tree_mode() {
            for item in &mut self.items {
                item.child.on_unmount();
            }
        }
    }

    fn on_tick(&mut self, tick: u64) {
        if !self.is_tree_mode() {
            for item in &mut self.items {
                item.child.on_tick(tick);
            }
        }
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        if !self.is_tree_mode() {
            for item in &mut self.items {
                item.child.on_resize(width, height);
            }
        }
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        if self.is_tree_mode() {
            return;
        }
        for item in &mut self.items {
            item.child.on_event_capture(event, ctx);
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

pub struct Grid {
    rows: usize,
    cols: usize,
    cells: Vec<Option<Box<dyn Widget>>>,
    cells_extracted: bool,
    row_gaps: usize,
    col_gaps: usize,
    row_sizes: Option<Vec<usize>>,
    col_sizes: Option<Vec<usize>>,
    seed: NodeSeed,
    /// (index into the `take_composed_children()` extraction order, css_id,
    /// classes) recorded by `with_compose` so `.with_id()`/`.with_classes()`
    /// metadata on declared children reaches the mounted node.
    child_decl_meta: Vec<crate::widgets::ChildDeclMeta>,
    /// (index into the extraction order, sink) recorded by `with_compose` for
    /// decls bound via `HandleSlot::bind`.
    child_handle_sinks: Vec<(usize, crate::handle::HandleSink)>,
}

impl Grid {
    pub fn new(rows: usize, cols: usize) -> Self {
        let rows = rows.max(1);
        let cols = cols.max(1);
        Self {
            rows,
            cols,
            cells: (0..rows * cols).map(|_| None).collect(),
            cells_extracted: false,
            row_gaps: 0,
            col_gaps: 0,
            row_sizes: None,
            col_sizes: None,
            seed: NodeSeed::default(),
            child_decl_meta: Vec::new(),
            child_handle_sinks: Vec::new(),
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

    pub fn with_child(mut self, child: impl Widget + 'static) -> Self {
        self.push(child);
        self
    }

    pub fn with_compose(mut self, children: ComposeResult) -> Self {
        for decl in children {
            let crate::compose::ChildDecl {
                builder,
                id,
                classes,
                handle_sink,
                ..
            } = decl;
            let crate::compose::WidgetBuilder::Ready(widget) = builder;
            // Index within the `take_composed_children()` extraction order, which
            // filters out empty cells. `push_boxed` fills the first empty slot, so
            // the count of occupied cells before insertion equals this child's
            // position in the extracted sequence.
            let index = self.cells.iter().filter(|c| c.is_some()).count();
            self.push_boxed(widget);
            if id.is_some() || !classes.is_empty() {
                self.child_decl_meta.push((index, id, classes));
            }
            if let Some(sink) = handle_sink {
                self.child_handle_sinks.push((index, sink));
            }
        }
        self
    }

    pub fn push(&mut self, child: impl Widget + 'static) {
        self.push_boxed(Box::new(child));
    }

    fn push_boxed(&mut self, child: Box<dyn Widget>) {
        if let Some(idx) = self.cells.iter().position(|cell| cell.is_none()) {
            self.cells[idx] = Some(child);
        } else {
            // Allow overflow so tree-mode grid auto-placement can flow into extra rows.
            self.cells.push(Some(child));
        }
    }

    pub fn id(mut self, value: impl Into<String>) -> Self {
        self.seed.css_id = Some(value.into());
        self
    }

    pub fn class(mut self, value: impl Into<String>) -> Self {
        self.seed.classes.push(value.into());
        self
    }

    pub fn classes(mut self, values: impl IntoIterator<Item = impl Into<String>>) -> Self {
        for value in values {
            self.seed.classes.push(value.into());
        }
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

    fn is_tree_mode(&self) -> bool {
        self.cells_extracted
    }
}

impl Widget for Grid {
    fn compose(&self) -> ComposeResult {
        Vec::new()
    }

    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        self.cells_extracted = true;
        self.cells
            .iter_mut()
            .filter_map(|cell| cell.take())
            .collect()
    }

    fn take_child_decl_meta(&mut self) -> Vec<crate::widgets::ChildDeclMeta> {
        std::mem::take(&mut self.child_decl_meta)
    }

    fn take_child_handle_sinks(&mut self) -> Vec<(usize, crate::handle::HandleSink)> {
        std::mem::take(&mut self.child_handle_sinks)
    }

    #[allow(clippy::needless_range_loop)] // r/c used as 2D indices into row_heights[r]/col_widths[c]
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);

        if self.is_tree_mode() {
            let blank = vec![Segment::new(" ".repeat(width))];
            let mut out = Segments::new();
            for row in 0..height {
                out.extend(blank.clone());
                if row + 1 < height {
                    out.push(Segment::line());
                }
            }
            return out;
        }

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
                    (margin_from_style(&resolved), style_constraints)
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

    #[allow(clippy::needless_range_loop)] // r/c used as 2D indices into row_heights[r]/col_widths[c]
    fn render_with_debug(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        debug: &DebugLayout,
    ) -> Segments {
        if self.is_tree_mode() {
            return Widget::render(self, console, options);
        }

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
                    (margin_from_style(&resolved), style_constraints)
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
        if !self.is_tree_mode() {
            for child in self.cells.iter_mut().flatten() {
                child.on_mount();
            }
        }
    }

    fn on_unmount(&mut self) {
        if !self.is_tree_mode() {
            for child in self.cells.iter_mut().flatten() {
                child.on_unmount();
            }
        }
    }

    fn on_tick(&mut self, tick: u64) {
        if !self.is_tree_mode() {
            for child in self.cells.iter_mut().flatten() {
                child.on_tick(tick);
            }
        }
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        if !self.is_tree_mode() {
            for child in self.cells.iter_mut().flatten() {
                child.on_resize(width, height);
            }
        }
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        if self.is_tree_mode() {
            return;
        }
        for child in self.cells.iter_mut().flatten() {
            child.on_event_capture(event, ctx);
            if ctx.handled() {
                break;
            }
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if self.is_tree_mode() {
            return;
        }
        for child in self.cells.iter_mut().flatten() {
            child.on_event(event, ctx);
            if ctx.handled() {
                break;
            }
        }
    }

    fn set_inline_style(&mut self, style: crate::style::Style) {
        self.seed.styles.style = style;
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }
}

impl Renderable for Grid {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::Label;

    #[test]
    fn row_compose_returns_empty() {
        let r = Row::new().with_child(Label::new("a"));
        assert!(r.compose().is_empty());
    }

    #[test]
    fn row_take_composed_children_extracts_all() {
        let mut r = Row::new()
            .with_child(Label::new("a"))
            .with_child(Label::new("b"));
        let children = r.take_composed_children();
        assert_eq!(children.len(), 2);
        assert!(r.children().is_empty());
    }

    #[test]
    fn row_with_compose_preserves_child_decl_meta() {
        use crate::compose::ChildDecl;
        let decls: Vec<ChildDecl> = vec![
            ChildDecl::from(Label::new("a")).with_id("disp-0"),
            ChildDecl::from(Label::new("b")).with_classes(&["highlight"]),
            ChildDecl::from(Label::new("c")),
        ];
        let mut r = Row::new().with_compose(decls);
        let meta = r.take_child_decl_meta();
        // Only children carrying id/classes are recorded, tagged by child index.
        assert_eq!(meta.len(), 2);
        assert_eq!(meta[0].0, 0);
        assert_eq!(meta[0].1.as_deref(), Some("disp-0"));
        assert_eq!(meta[1].0, 1);
        assert_eq!(meta[1].2, vec!["highlight".to_string()]);
        // Children still extract in declaration order.
        assert_eq!(r.take_composed_children().len(), 3);
    }

    #[test]
    fn grid_with_compose_preserves_child_decl_meta() {
        use crate::compose::ChildDecl;
        let decls: Vec<ChildDecl> = vec![
            ChildDecl::from(Label::new("a")),
            ChildDecl::from(Label::new("b")).with_id("cell-1"),
        ];
        let mut g = Grid::new(2, 2).with_compose(decls);
        let meta = g.take_child_decl_meta();
        assert_eq!(meta.len(), 1);
        // Index reflects position in the (filtered) extraction order.
        assert_eq!(meta[0].0, 1);
        assert_eq!(meta[0].1.as_deref(), Some("cell-1"));
    }

    // ── Row tree-mode regression tests ──────────────────────────────

    #[test]
    fn row_tree_mode_flag_set_after_extraction() {
        let mut r = Row::new()
            .with_child(Label::new("a"))
            .with_child(Label::new("b"));
        assert!(!r.is_tree_mode());
        let _ = r.take_composed_children();
        assert!(r.is_tree_mode());
    }

    #[test]
    fn row_tree_mode_render_returns_chrome() {
        let mut r = Row::new().with_child(Label::new("hello"));
        let _ = r.take_composed_children();

        let console = Console::new();
        let mut options = console.options().clone();
        options.size = (10, 3);
        options.max_width = 10;
        options.max_height = 3;
        let segments = Widget::render(&r, &console, &options);
        assert!(!segments.is_empty());
    }

    #[test]
    fn row_tree_mode_on_event_does_not_panic() {
        let mut r = Row::new().with_child(Label::new("a"));
        let _ = r.take_composed_children();

        let mut ctx = EventCtx::default();
        r.on_event(&Event::Action(Action::FocusNext), &mut ctx);
        assert!(!ctx.handled());
    }

    #[test]
    fn row_tree_mode_mouse_move_returns_false() {
        let mut r = Row::new().with_child(Label::new("a"));
        let _ = r.take_composed_children();
        assert!(!r.on_mouse_move(0, 0));
    }

    // ── Dock tree-mode regression tests ─────────────────────────────

    #[test]
    fn dock_tree_mode_flag_set_after_extraction() {
        let mut d = Dock::new()
            .push_top(Some(1), Label::new("header"))
            .push_fill(Label::new("body"));
        assert!(!d.is_tree_mode());
        let children = d.take_composed_children();
        assert_eq!(children.len(), 2);
        assert!(d.is_tree_mode());
    }

    #[test]
    fn dock_explicit_size_hints_use_border_box_in_tree_mode() {
        let mut d = Dock::new()
            .push_bottom(Some(3), Label::new("footer"))
            .push_right(8, Label::new("side"));
        let children = d.take_composed_children();
        assert_eq!(children.len(), 2);

        let footer_style = children[0]
            .style()
            .expect("footer child should expose style after dock hinting");
        assert_eq!(footer_style.height, Some(Scalar::Cells(3)));
        assert_eq!(footer_style.box_sizing, Some(BoxSizing::BorderBox));

        let side_style = children[1]
            .style()
            .expect("side child should expose style after dock hinting");
        assert_eq!(side_style.width, Some(Scalar::Cells(8)));
        assert_eq!(side_style.box_sizing, Some(BoxSizing::BorderBox));
    }

    #[test]
    fn dock_tree_mode_render_returns_chrome() {
        let mut d = Dock::new()
            .push_top(Some(1), Label::new("header"))
            .push_fill(Label::new("body"));
        let _ = d.take_composed_children();

        let console = Console::new();
        let mut options = console.options().clone();
        options.size = (20, 10);
        options.max_width = 20;
        options.max_height = 10;
        let segments = Widget::render(&d, &console, &options);
        assert!(!segments.is_empty());
    }

    #[test]
    fn dock_tree_mode_on_event_does_not_panic() {
        let mut d = Dock::new().push_fill(Label::new("body"));
        let _ = d.take_composed_children();

        let mut ctx = EventCtx::default();
        d.on_event(&Event::Action(Action::FocusNext), &mut ctx);
        assert!(!ctx.handled());
    }

    #[test]
    fn dock_tree_mode_mouse_move_returns_false() {
        let mut d = Dock::new().push_fill(Label::new("body"));
        let _ = d.take_composed_children();
        assert!(!d.on_mouse_move(0, 0));
    }

    // ── Grid tree-mode regression tests ─────────────────────────────

    #[test]
    fn grid_tree_mode_flag_set_after_extraction() {
        let mut g = Grid::new(2, 2)
            .with_cell(0, 0, Label::new("a"))
            .with_cell(0, 1, Label::new("b"))
            .with_cell(1, 0, Label::new("c"));
        assert!(!g.is_tree_mode());
        let children = g.take_composed_children();
        assert_eq!(children.len(), 3);
        assert!(g.is_tree_mode());
    }

    #[test]
    fn grid_tree_mode_render_returns_chrome() {
        let mut g =
            Grid::new(1, 2)
                .with_cell(0, 0, Label::new("a"))
                .with_cell(0, 1, Label::new("b"));
        let _ = g.take_composed_children();

        let console = Console::new();
        let mut options = console.options().clone();
        options.size = (10, 3);
        options.max_width = 10;
        options.max_height = 3;
        let segments = Widget::render(&g, &console, &options);
        assert!(!segments.is_empty());
    }

    #[test]
    fn grid_tree_mode_on_event_does_not_panic() {
        let mut g = Grid::new(1, 1).with_cell(0, 0, Label::new("a"));
        let _ = g.take_composed_children();

        let mut ctx = EventCtx::default();
        g.on_event(&Event::Action(Action::FocusNext), &mut ctx);
        assert!(!ctx.handled());
    }

    #[test]
    fn row_id_and_class_are_carried_in_seed() {
        let mut r = Row::new().id("row-ident").class("my-row");
        let seed = r.take_node_seed();
        assert_eq!(seed.css_id.as_deref(), Some("row-ident"));
        assert!(seed.classes.iter().any(|c| c == "my-row"));
    }

    #[test]
    fn grid_id_and_class_already_present() {
        let mut g = Grid::new(2, 2).id("grid-ident").class("my-grid");
        let seed = g.take_node_seed();
        assert_eq!(seed.css_id.as_deref(), Some("grid-ident"));
        assert!(seed.classes.iter().any(|c| c == "my-grid"));
    }
}
