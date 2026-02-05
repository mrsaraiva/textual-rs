use std::io::Write;
use std::sync::atomic::{AtomicUsize, Ordering};

use crossterm::event::KeyCode;
use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments, Text};

use crate::css;
use crate::debug::{DebugLayout, debug_input, debug_layout};
use crate::event::{Action, Event, EventCtx};
use crate::style::Style;

use super::{
    LayoutConstraints, Widget, WidgetId, WidgetRenderable, WidgetStyles,
    helpers::{
        apply_debug_box, apply_margin, clamp_with_constraints, collect_focus_ids,
        constraints_from_style, crop_line_horizontal, dispatch_event_to_focus,
        fixed_height_from_constraints, margin_from_style, merge_constraints, pad_lines_to_width,
        set_focus_by_id,
    },
};

pub struct Container {
    id: WidgetId,
    children: Vec<Box<dyn Widget>>,
    styles: WidgetStyles,
}

impl Container {
    pub fn new() -> Self {
        Self {
            id: WidgetId::new(),
            children: Vec::new(),
            styles: WidgetStyles::default(),
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
            let meta = css::selector_meta_generic(child.as_ref());
            let resolved = css::resolve_style(child.as_ref(), &meta);
            let margin = margin_from_style(&resolved);
            let style_constraints = constraints_from_style(&resolved);
            let constraints = merge_constraints(style_constraints, child.layout_constraints());
            let available_width = width.saturating_sub(margin.left + margin.right).max(1);
            let mut render_width = clamp_with_constraints(
                available_width,
                constraints.min_width,
                constraints.max_width,
                available_width,
            );
            if resolved.width_auto == Some(true) {
                let pad = resolved.line_pad.unwrap_or(0).saturating_mul(2);
                let (_, _, border_left, border_right) =
                    super::helpers::border_spacing_from_style(&resolved);
                let intrinsic = child
                    .content_width()
                    .unwrap_or(render_width)
                    .saturating_add(pad + border_left + border_right)
                    .max(1);
                render_width = clamp_with_constraints(
                    intrinsic,
                    constraints.min_width,
                    constraints.max_width,
                    available_width,
                );
            }
            let render_height = clamp_with_constraints(
                height_limit
                    .saturating_sub(margin.top + margin.bottom)
                    .max(1),
                constraints.min_height,
                constraints.max_height,
                height_limit
                    .saturating_sub(margin.top + margin.bottom)
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
            let mut child_lines =
                Segment::split_and_crop_lines(segments, render_width, None, true, false);
            let mut target_height = child.layout_height().unwrap_or(child_lines.len().max(1));
            target_height = clamp_with_constraints(
                target_height,
                constraints.min_height,
                constraints.max_height,
                target_height,
            );
            child_lines =
                Segment::set_shape(&child_lines, render_width, Some(target_height), None, false);
            child_lines = pad_lines_to_width(child_lines, render_width);
            child_lines = apply_margin(child_lines, width, margin);
            let child_height = child_lines.len();
            let child_region = rich_rs::Region::new(0, cursor_y, width as u32, child_height as u32);
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
            let constraints = child.layout_constraints();
            let render_width =
                clamp_with_constraints(width, constraints.min_width, constraints.max_width, width);
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
            let mut child_lines =
                Segment::split_and_crop_lines(segments, render_width, None, true, false);
            let mut target_height = child.layout_height().unwrap_or(child_lines.len().max(1));
            target_height = clamp_with_constraints(
                target_height,
                constraints.min_height,
                constraints.max_height,
                target_height,
            );
            child_lines =
                Segment::set_shape(&child_lines, render_width, Some(target_height), None, false);
            child_lines = pad_lines_to_width(child_lines, width);
            let child_height = child_lines.len().max(1);
            let debug_height = (child_height + 2).max(3);
            let child_region = rich_rs::Region::new(0, cursor_y, width as u32, debug_height as u32);
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

    fn visit_children_mut(&mut self, f: &mut dyn FnMut(&mut dyn Widget)) {
        for child in &mut self.children {
            f(child.as_mut());
        }
    }

    fn layout_height(&self) -> Option<usize> {
        if let Some(fixed) = fixed_height_from_constraints(self.layout_constraints()) {
            return Some(fixed);
        }
        let mut total = 0usize;
        for child in &self.children {
            let meta = css::selector_meta_generic(child.as_ref());
            let resolved = css::resolve_style(child.as_ref(), &meta);
            let margin = margin_from_style(&resolved);
            match child.layout_height() {
                Some(height) => {
                    total = total
                        .saturating_add(height)
                        .saturating_add(margin.top + margin.bottom);
                }
                None => return None,
            }
        }
        Some(total.max(1))
    }

    fn content_width(&self) -> Option<usize> {
        let mut widest = 0usize;
        let mut any = false;
        for child in &self.children {
            let meta = css::selector_meta_generic(child.as_ref());
            let resolved = css::resolve_style(child.as_ref(), &meta);
            let margin = margin_from_style(&resolved);
            if let Some(width) = child.content_width() {
                widest = widest.max(width.saturating_add(margin.left + margin.right));
                any = true;
            }
        }
        if any { Some(widest.max(1)) } else { None }
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

impl Renderable for Container {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

pub struct Constrained {
    id: WidgetId,
    child: Box<dyn Widget>,
    constraints: LayoutConstraints,
    styles: WidgetStyles,
}

impl Constrained {
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            id: WidgetId::new(),
            child: Box::new(child),
            constraints: LayoutConstraints::default(),
            styles: WidgetStyles::default(),
        }
    }

    pub fn min_width(mut self, value: usize) -> Self {
        self.constraints = self.constraints.min_width(value);
        self
    }

    pub fn max_width(mut self, value: usize) -> Self {
        self.constraints = self.constraints.max_width(value);
        self
    }

    pub fn min_height(mut self, value: usize) -> Self {
        self.constraints = self.constraints.min_height(value);
        self
    }

    pub fn max_height(mut self, value: usize) -> Self {
        self.constraints = self.constraints.max_height(value);
        self
    }
}

impl Widget for Constrained {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        self.child.render_styled(console, options)
    }

    fn render_with_debug(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        debug: &DebugLayout,
    ) -> Segments {
        self.child.render_styled_with_debug(console, options, debug)
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

    fn focusable(&self) -> bool {
        self.child.focusable()
    }

    fn set_focus(&mut self, focused: bool) {
        self.child.set_focus(focused);
    }

    fn layout_height(&self) -> Option<usize> {
        let constraints = self.layout_constraints();
        if let (Some(min), Some(max)) = (constraints.min_height, constraints.max_height) {
            if min == max {
                return Some(min);
            }
        }
        self.child.layout_height()
    }

    fn layout_constraints(&self) -> LayoutConstraints {
        merge_constraints(self.styles.layout, self.constraints)
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }

    fn visit_children_mut(&mut self, f: &mut dyn FnMut(&mut dyn Widget)) {
        f(self.child.as_mut());
    }
}

impl Renderable for Constrained {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

pub struct Styled {
    id: WidgetId,
    child: Box<dyn Widget>,
    styles: WidgetStyles,
}

impl Styled {
    pub fn new(child: impl Widget + 'static, style: Style) -> Self {
        let mut styles = WidgetStyles::default();
        styles.style = style;
        Self {
            id: WidgetId::new(),
            child: Box::new(child),
            styles,
        }
    }

    pub fn style(mut self, style: Style) -> Self {
        self.styles.style = style;
        self
    }
}

impl Widget for Styled {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        self.child.render_styled(console, options)
    }

    fn render_with_debug(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        debug: &DebugLayout,
    ) -> Segments {
        self.child.render_styled_with_debug(console, options, debug)
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

    fn focusable(&self) -> bool {
        self.child.focusable()
    }

    fn set_focus(&mut self, focused: bool) {
        self.child.set_focus(focused);
    }

    fn layout_height(&self) -> Option<usize> {
        if let Some(fixed) = fixed_height_from_constraints(self.layout_constraints()) {
            return Some(fixed);
        }
        self.child.layout_height()
    }

    fn layout_constraints(&self) -> LayoutConstraints {
        merge_constraints(self.styles.layout, self.child.layout_constraints())
    }

    fn style(&self) -> Option<Style> {
        Some(self.styles.style)
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }

    fn style_type(&self) -> &'static str {
        self.child.style_type()
    }

    fn visit_children_mut(&mut self, f: &mut dyn FnMut(&mut dyn Widget)) {
        f(self.child.as_mut());
    }
}

impl Renderable for Styled {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

pub struct Node {
    id: WidgetId,
    child: Box<dyn Widget>,
    style_id: Option<String>,
    classes: Vec<String>,
    styles: WidgetStyles,
}

impl Node {
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            id: WidgetId::new(),
            child: Box::new(child),
            style_id: None,
            classes: Vec::new(),
            styles: WidgetStyles::default(),
        }
    }

    pub fn id(mut self, value: impl Into<String>) -> Self {
        self.style_id = Some(value.into());
        self
    }

    pub fn class(mut self, value: impl Into<String>) -> Self {
        self.classes.push(value.into());
        self
    }

    pub fn classes(mut self, values: impl IntoIterator<Item = impl Into<String>>) -> Self {
        for value in values {
            self.classes.push(value.into());
        }
        self
    }
}

impl Widget for Node {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        self.child.render_styled(console, options)
    }

    fn render_with_debug(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        debug: &DebugLayout,
    ) -> Segments {
        self.child.render_styled_with_debug(console, options, debug)
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

    fn focusable(&self) -> bool {
        self.child.focusable()
    }

    fn set_focus(&mut self, focused: bool) {
        self.child.set_focus(focused);
    }

    fn layout_height(&self) -> Option<usize> {
        if let Some(fixed) = fixed_height_from_constraints(self.layout_constraints()) {
            return Some(fixed);
        }
        self.child.layout_height()
    }

    fn layout_constraints(&self) -> LayoutConstraints {
        merge_constraints(self.styles.layout, self.child.layout_constraints())
    }

    fn style(&self) -> Option<Style> {
        Some(self.styles.style)
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }

    fn style_type(&self) -> &'static str {
        self.child.style_type()
    }

    fn style_id(&self) -> Option<&str> {
        self.style_id.as_deref()
    }

    fn style_classes(&self) -> &[String] {
        &self.classes
    }

    fn visit_children_mut(&mut self, f: &mut dyn FnMut(&mut dyn Widget)) {
        f(self.child.as_mut());
    }
}

impl Renderable for Node {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

pub struct AppRoot {
    id: WidgetId,
    children: Vec<Box<dyn Widget>>,
    focused: Option<WidgetId>,
    styles: WidgetStyles,
}

impl AppRoot {
    pub fn new() -> Self {
        Self {
            id: WidgetId::new(),
            children: Vec::new(),
            focused: None,
            styles: WidgetStyles::default(),
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
        let mut ids = Vec::new();
        for child in &mut self.children {
            collect_focus_ids(child.as_mut(), &mut ids);
        }
        let target = ids.first().copied();
        for child in &mut self.children {
            set_focus_by_id(child.as_mut(), target);
        }
        self.focused = target;
    }

    pub fn focus_next(&mut self) {
        let mut ids = Vec::new();
        for child in &mut self.children {
            collect_focus_ids(child.as_mut(), &mut ids);
        }
        if std::env::var("TEXTUAL_DEBUG_FOCUS").ok().as_deref() == Some("1") {
            let line = format!(
                "[focus] chain (len={}): {:?}",
                ids.len(),
                ids.iter().map(|id| id.as_u64()).collect::<Vec<_>>()
            );
            if let Ok(path) = std::env::var("TEXTUAL_DEBUG_FOCUS_FILE") {
                if let Ok(mut file) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                {
                    let _ = writeln!(file, "{line}");
                }
            } else {
                eprintln!("{line}");
            }
        }
        if ids.is_empty() {
            self.focused = None;
            return;
        }
        let next = if let Some(current) = self.focused {
            if let Some(idx) = ids.iter().position(|id| *id == current) {
                ids[(idx + 1) % ids.len()]
            } else {
                ids[0]
            }
        } else {
            ids[0]
        };
        if std::env::var("TEXTUAL_DEBUG_FOCUS").ok().as_deref() == Some("1") {
            let line = format!(
                "[focus] current={:?} -> next={:?}",
                self.focused.map(|id| id.as_u64()),
                next.as_u64()
            );
            if let Ok(path) = std::env::var("TEXTUAL_DEBUG_FOCUS_FILE") {
                if let Ok(mut file) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                {
                    let _ = writeln!(file, "{line}");
                }
            } else {
                eprintln!("{line}");
            }
        }
        for child in &mut self.children {
            set_focus_by_id(child.as_mut(), Some(next));
        }
        self.focused = Some(next);
    }

    pub fn focus_prev(&mut self) {
        let mut ids = Vec::new();
        for child in &mut self.children {
            collect_focus_ids(child.as_mut(), &mut ids);
        }
        if ids.is_empty() {
            self.focused = None;
            return;
        }
        let prev = if let Some(current) = self.focused {
            if let Some(idx) = ids.iter().position(|id| *id == current) {
                if idx == 0 {
                    ids[ids.len() - 1]
                } else {
                    ids[idx - 1]
                }
            } else {
                ids[0]
            }
        } else {
            ids[0]
        };
        for child in &mut self.children {
            set_focus_by_id(child.as_mut(), Some(prev));
        }
        self.focused = Some(prev);
    }

    pub fn focus(&mut self, id: WidgetId) -> bool {
        let mut ids = Vec::new();
        for child in &mut self.children {
            collect_focus_ids(child.as_mut(), &mut ids);
        }
        if ids.iter().any(|target| *target == id) {
            for child in &mut self.children {
                set_focus_by_id(child.as_mut(), Some(id));
            }
            self.focused = Some(id);
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

    fn set_focus_target(&mut self, target: Option<WidgetId>) {
        self.focused = target;
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height_limit = options.size.1.max(1);
        let bounds = rich_rs::Region::from_size(width as u32, height_limit as u32);

        let mut lines: Vec<Vec<Segment>> = Vec::new();
        let mut cursor_y: i32 = 0;

        for child in &self.children {
            let meta = css::selector_meta_generic(child.as_ref());
            let resolved = css::resolve_style(child.as_ref(), &meta);
            let margin = margin_from_style(&resolved);
            let style_constraints = constraints_from_style(&resolved);
            let constraints = merge_constraints(style_constraints, child.layout_constraints());
            let render_width = clamp_with_constraints(
                width.saturating_sub(margin.left + margin.right).max(1),
                constraints.min_width,
                constraints.max_width,
                width.saturating_sub(margin.left + margin.right).max(1),
            );
            let render_height = clamp_with_constraints(
                height_limit
                    .saturating_sub(margin.top + margin.bottom)
                    .max(1),
                constraints.min_height,
                constraints.max_height,
                height_limit
                    .saturating_sub(margin.top + margin.bottom)
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
            let mut child_lines =
                Segment::split_and_crop_lines(segments, render_width, None, true, false);
            let mut target_height = child.layout_height().unwrap_or(child_lines.len().max(1));
            target_height = clamp_with_constraints(
                target_height,
                constraints.min_height,
                constraints.max_height,
                target_height,
            );
            child_lines =
                Segment::set_shape(&child_lines, render_width, Some(target_height), None, false);
            child_lines = pad_lines_to_width(child_lines, render_width);
            child_lines = apply_margin(child_lines, width, margin);
            let child_height = child_lines.len();
            let child_region = rich_rs::Region::new(0, cursor_y, width as u32, child_height as u32);
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
            let meta = css::selector_meta_generic(child.as_ref());
            let resolved = css::resolve_style(child.as_ref(), &meta);
            let margin = margin_from_style(&resolved);
            let style_constraints = constraints_from_style(&resolved);
            let constraints = merge_constraints(style_constraints, child.layout_constraints());
            let render_width = clamp_with_constraints(
                width.saturating_sub(margin.left + margin.right).max(1),
                constraints.min_width,
                constraints.max_width,
                width.saturating_sub(margin.left + margin.right).max(1),
            );
            let render_height = clamp_with_constraints(
                height_limit
                    .saturating_sub(margin.top + margin.bottom)
                    .max(1),
                constraints.min_height,
                constraints.max_height,
                height_limit
                    .saturating_sub(margin.top + margin.bottom)
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
            let mut child_lines =
                Segment::split_and_crop_lines(segments, render_width, None, true, false);
            let mut target_height = child.layout_height().unwrap_or(child_lines.len().max(1));
            target_height = clamp_with_constraints(
                target_height,
                constraints.min_height,
                constraints.max_height,
                target_height,
            );
            child_lines =
                Segment::set_shape(&child_lines, render_width, Some(target_height), None, false);
            child_lines = pad_lines_to_width(child_lines, render_width);
            child_lines = apply_margin(child_lines, width, margin);
            let child_height = child_lines.len().max(1);
            let debug_height = (child_height + 2).max(3);
            let child_region = rich_rs::Region::new(0, cursor_y, width as u32, debug_height as u32);
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
        if matches!(event, Event::MouseUp(..) | Event::AppFocus(..)) {
            // Mouse release is a global state transition (e.g. clearing `:active`).
            // Broadcast it to all children regardless of focus or handled state.
            for child in &mut self.children {
                child.on_event(event, ctx);
            }
            return;
        }
        if let Event::MouseDown(mouse) = event {
            let _ = self.focus(mouse.target);
        }
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

        if let Some(id) = self.focused {
            for child in &mut self.children {
                dispatch_event_to_focus(child.as_mut(), id, event, ctx);
                if ctx.handled() {
                    return;
                }
            }
        }

        for child in &mut self.children {
            child.on_event(event, ctx);
            if ctx.handled() {
                break;
            }
        }
    }

    fn layout_height(&self) -> Option<usize> {
        if let Some(fixed) = fixed_height_from_constraints(self.layout_constraints()) {
            return Some(fixed);
        }
        let mut total = 0usize;
        for child in &self.children {
            let meta = css::selector_meta_generic(child.as_ref());
            let resolved = css::resolve_style(child.as_ref(), &meta);
            let margin = margin_from_style(&resolved);
            match child.layout_height() {
                Some(height) => {
                    total = total
                        .saturating_add(height)
                        .saturating_add(margin.top + margin.bottom);
                }
                None => return None,
            }
        }
        Some(total.max(1))
    }

    fn content_width(&self) -> Option<usize> {
        let mut widest = 0usize;
        let mut any = false;
        for child in &self.children {
            let meta = css::selector_meta_generic(child.as_ref());
            let resolved = css::resolve_style(child.as_ref(), &meta);
            let margin = margin_from_style(&resolved);
            if let Some(width) = child.content_width() {
                widest = widest.max(width.saturating_add(margin.left + margin.right));
                any = true;
            }
        }
        if any { Some(widest.max(1)) } else { None }
    }

    fn visit_children_mut(&mut self, f: &mut dyn FnMut(&mut dyn Widget)) {
        for child in &mut self.children {
            f(child.as_mut());
        }
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

#[cfg(test)]
mod focus_tests {
    use super::*;
    use crate::widgets::{Input, ListView, collect_focus_ids, set_focus_by_id};
    use rich_rs::Console;

    #[test]
    fn focus_next_advances_after_set_focus_by_id() {
        let mut root = AppRoot::new().with_child(
            Container::new()
                .with_child(Input::new().with_placeholder("First"))
                .with_child(Input::new().with_placeholder("Second")),
        );

        let mut ids = Vec::new();
        collect_focus_ids(&mut root, &mut ids);
        assert_eq!(ids.len(), 2);
        let first = ids[0];
        let second = ids[1];

        set_focus_by_id(&mut root, Some(first));
        assert_eq!(root.focused, Some(first));

        root.focus_next();
        assert_eq!(root.focused, Some(second));
    }

    #[test]
    fn scroll_view_handles_mouse_scroll_without_focus() {
        let console = Console::new();
        let mut options = console.options().clone();
        options.size = (12, 3);
        options.max_width = 12;
        options.max_height = 3;

        let list = ListView::new(vec![
            "item 1".to_string(),
            "item 2".to_string(),
            "item 3".to_string(),
            "item 4".to_string(),
            "item 5".to_string(),
        ]);
        let mut scroll = ScrollView::new(list).height(3);
        let _ = Widget::render(&scroll, &console, &options);

        let mut ctx = EventCtx::default();
        scroll.on_mouse_scroll(0, 1, &mut ctx);
        assert!(ctx.handled());
        assert_eq!(scroll.offset_y, 1);
    }

    #[test]
    fn panel_forwards_action_to_scrollview_child() {
        let console = Console::new();
        let mut options = console.options().clone();
        options.size = (14, 6);
        options.max_width = 14;
        options.max_height = 6;

        let list = ListView::new(vec![
            "item 1".to_string(),
            "item 2".to_string(),
            "item 3".to_string(),
            "item 4".to_string(),
            "item 5".to_string(),
        ]);
        let mut panel = Panel::new(ScrollView::new(list).height(3)).padding(1);
        let _ = Widget::render(&panel, &console, &options);

        let mut ctx = EventCtx::default();
        panel.on_event(&Event::Action(Action::ScrollDown), &mut ctx);
        assert!(ctx.handled());
    }

    #[test]
    fn panel_forwards_mouse_scroll_to_scrollview_child() {
        let console = Console::new();
        let mut options = console.options().clone();
        options.size = (14, 6);
        options.max_width = 14;
        options.max_height = 6;

        let list = ListView::new(vec![
            "item 1".to_string(),
            "item 2".to_string(),
            "item 3".to_string(),
            "item 4".to_string(),
            "item 5".to_string(),
        ]);
        let mut panel = Panel::new(ScrollView::new(list).height(3)).padding(1);
        let _ = Widget::render(&panel, &console, &options);

        let mut ctx = EventCtx::default();
        panel.on_mouse_scroll(0, 1, &mut ctx);
        assert!(ctx.handled());
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
    styles: WidgetStyles,
}

impl Frame {
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            id: WidgetId::new(),
            child: Box::new(child),
            padding: 1,
            border: true,
            styles: WidgetStyles::default(),
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

pub struct Panel {
    id: WidgetId,
    child: Box<dyn Widget>,
    title: Option<String>,
    padding: usize,
    border: bool,
    styles: WidgetStyles,
}

impl Panel {
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            id: WidgetId::new(),
            child: Box::new(child),
            title: None,
            padding: 0,
            border: true,
            styles: WidgetStyles::default(),
        }
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
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

impl Widget for Panel {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let border_width = if self.border { 1 } else { 0 };
        let total_padding = self.padding * 2;
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);

        let inner_width = width
            .saturating_sub(border_width * 2 + total_padding)
            .max(1);
        let inner_height = height
            .saturating_sub(border_width * 2 + total_padding)
            .max(1);

        let mut child_options = options.clone();
        child_options.size = (inner_width, inner_height);
        child_options.max_width = inner_width;
        child_options.max_height = inner_height;

        let child_segments = self.child.render_styled(console, &child_options);
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

        let content_height = content_lines.len().max(1);
        let content_height = content_height.min(height.saturating_sub(border_width * 2).max(1));
        let mut content_lines = Segment::set_shape(
            &content_lines,
            inner_width,
            Some(content_height),
            None,
            false,
        );

        if !self.border {
            let line_count = content_lines.len();
            let mut out = Segments::new();
            for (idx, line) in content_lines.into_iter().enumerate() {
                out.extend(line);
                if idx + 1 < line_count {
                    out.push(Segment::line());
                }
            }
            return out;
        }

        let box_chars = rich_rs::r#box::SQUARE;
        let mut out_lines: Vec<Vec<Segment>> = Vec::new();

        let mut top = String::new();
        top.push(box_chars.top_left);
        let mut title = self.title.clone().unwrap_or_default();
        if !title.is_empty() && inner_width >= 2 {
            title = format!(" {title} ");
        }
        let title_width = rich_rs::cell_len(&title);
        if title_width >= inner_width {
            top.push_str(&rich_rs::set_cell_size(&title, inner_width));
        } else {
            let remaining = inner_width.saturating_sub(title_width);
            let left = remaining / 2;
            let right = remaining - left;
            top.push_str(&box_chars.top.to_string().repeat(left));
            top.push_str(&title);
            top.push_str(&box_chars.top.to_string().repeat(right));
        }
        top.push(box_chars.top_right);
        out_lines.push(vec![Segment::new(top)]);

        for line in content_lines.drain(..) {
            let mut middle = Vec::new();
            middle.push(Segment::new(box_chars.mid_left.to_string()));
            middle.extend(line);
            middle.push(Segment::new(box_chars.mid_right.to_string()));
            out_lines.push(middle);
        }

        let mut bottom = String::new();
        bottom.push(box_chars.bottom_left);
        bottom.push_str(&box_chars.bottom.to_string().repeat(inner_width));
        bottom.push(box_chars.bottom_right);
        out_lines.push(vec![Segment::new(bottom)]);

        let out_lines = Segment::set_shape(&out_lines, width, Some(height), None, false);
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

    fn layout_height(&self) -> Option<usize> {
        if let Some(fixed) = fixed_height_from_constraints(self.layout_constraints()) {
            return Some(fixed);
        }
        self.child.layout_height().map(|child| {
            let border = if self.border { 2 } else { 0 };
            child + self.padding * 2 + border
        })
    }

    fn content_width(&self) -> Option<usize> {
        self.child.content_width().map(|child| {
            let border = if self.border { 2 } else { 0 };
            child + self.padding * 2 + border
        })
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

    fn on_mouse_scroll(&mut self, delta_x: i32, delta_y: i32, ctx: &mut EventCtx) {
        self.child.on_mouse_scroll(delta_x, delta_y, ctx);
    }

    fn focusable(&self) -> bool {
        self.child.focusable()
    }

    fn set_focus(&mut self, focused: bool) {
        self.child.set_focus(focused);
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }

    fn visit_children_mut(&mut self, f: &mut dyn FnMut(&mut dyn Widget)) {
        f(self.child.as_mut());
    }
}

impl Renderable for Panel {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
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

        let inner_width = width
            .saturating_sub(border_width * 2 + total_padding)
            .max(1);
        let mut inner_height = height
            .saturating_sub(border_width * 2 + total_padding)
            .max(1);
        if let Some(child_height) = self.child.layout_height() {
            inner_height = inner_height.min(child_height.max(1));
        }

        let mut child_options = options.clone();
        child_options.size = (inner_width, inner_height);
        child_options.max_width = inner_width;
        child_options.max_height = inner_height;

        let child_segments = self.child.render_styled(console, &child_options);
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
        content_lines = Segment::set_shape(
            &content_lines,
            inner_width,
            Some(inner_height + total_padding),
            None,
            false,
        );

        let inner_total = inner_width + total_padding;
        let mut out = Segments::new();
        let line_count = content_lines.len();

        if self.border {
            let b = rich_rs::r#box::SQUARE;
            let top = format!(
                "{}{}{}",
                b.top_left,
                std::iter::repeat(b.top)
                    .take(inner_total)
                    .collect::<String>(),
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
                std::iter::repeat(b.bottom)
                    .take(inner_total)
                    .collect::<String>(),
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
        if let Some(fixed) = fixed_height_from_constraints(self.layout_constraints()) {
            return Some(fixed);
        }
        self.child
            .layout_height()
            .map(|h| h + self.padding * 2 + if self.border { 2 } else { 0 })
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }

    fn focusable(&self) -> bool {
        self.child.focusable()
    }

    fn set_focus(&mut self, focused: bool) {
        self.child.set_focus(focused);
    }

    fn visit_children_mut(&mut self, f: &mut dyn FnMut(&mut dyn Widget)) {
        f(self.child.as_mut());
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
    content_height: AtomicUsize,
    viewport_height: AtomicUsize,
    offset_x: usize,
    scroll_step_x: usize,
    content_width: AtomicUsize,
    viewport_width: AtomicUsize,
    styles: WidgetStyles,
}

impl ScrollView {
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            id: WidgetId::new(),
            child: Box::new(child),
            height: None,
            offset_y: 0,
            scroll_step: 1,
            content_height: AtomicUsize::new(0),
            viewport_height: AtomicUsize::new(0),
            offset_x: 0,
            scroll_step_x: 2,
            content_width: AtomicUsize::new(0),
            viewport_width: AtomicUsize::new(0),
            styles: WidgetStyles::default(),
        }
    }

    pub fn height(mut self, height: usize) -> Self {
        self.height = Some(height.max(1));
        self
    }

    pub fn scroll_to(&mut self, offset_y: usize) {
        self.offset_y = offset_y;
        self.clamp_offset();
    }

    pub fn scroll_to_x(&mut self, offset_x: usize) {
        self.offset_x = offset_x;
        self.clamp_offset();
    }

    pub fn scroll_by(&mut self, delta: i32) {
        if delta.is_negative() {
            self.offset_y = self.offset_y.saturating_sub(delta.unsigned_abs() as usize);
        } else {
            self.offset_y = self.offset_y.saturating_add(delta as usize);
        }
        self.clamp_offset();
    }

    pub fn scroll_by_x(&mut self, delta: i32) {
        if delta.is_negative() {
            self.offset_x = self.offset_x.saturating_sub(delta.unsigned_abs() as usize);
        } else {
            self.offset_x = self.offset_x.saturating_add(delta as usize);
        }
        self.clamp_offset();
    }

    pub fn scroll_step(mut self, step: usize) -> Self {
        self.scroll_step = step.max(1);
        self
    }

    pub fn scroll_step_x(mut self, step: usize) -> Self {
        self.scroll_step_x = step.max(1);
        self
    }

    pub fn offset_y(&self) -> usize {
        self.offset_y
    }

    pub fn offset_x(&self) -> usize {
        self.offset_x
    }

    fn max_offset(&self) -> usize {
        let content = self.content_height.load(Ordering::Relaxed);
        let viewport = self.viewport_height.load(Ordering::Relaxed).max(1);
        content.saturating_sub(viewport)
    }

    fn max_offset_x(&self) -> usize {
        let content = self.content_width.load(Ordering::Relaxed);
        let viewport = self.viewport_width.load(Ordering::Relaxed).max(1);
        content.saturating_sub(viewport)
    }

    fn clamp_offset(&mut self) {
        let max_y = self.max_offset();
        if self.offset_y > max_y {
            self.offset_y = max_y;
        }
        let max_x = self.max_offset_x();
        if self.offset_x > max_x {
            self.offset_x = max_x;
        }
    }
}

impl Widget for ScrollView {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let viewport_height = self.height.unwrap_or_else(|| options.size.1.max(1));
        if std::env::var("TEXTUAL_DEBUG_LAYOUT_FILE").is_ok() {
            debug_layout(&format!(
                "[scroll] id={} viewport=({}, {}) offset=({}, {})",
                self.id.as_u64(),
                width,
                viewport_height,
                self.offset_x,
                self.offset_y
            ));
        }
        self.viewport_height
            .store(viewport_height, Ordering::Relaxed);
        self.viewport_width.store(width, Ordering::Relaxed);

        let constraints = self.child.layout_constraints();
        let target_height = self.child.layout_height().unwrap_or_else(|| {
            // For children without an intrinsic height, probe at least one extra viewport
            // so scrolling can start from offset 0, without letting probe height
            // grow with scroll offset.
            viewport_height.saturating_add(viewport_height).max(1)
        });
        let target_width = self.child.content_width().unwrap_or(width).max(width);
        let render_width = clamp_with_constraints(
            target_width,
            constraints.min_width,
            constraints.max_width,
            target_width,
        )
        .max(width);
        if std::env::var("TEXTUAL_DEBUG_LAYOUT_FILE").is_ok() {
            debug_layout(&format!(
                "[scroll] id={} child render_width={} constraints=({:?},{:?})",
                self.id.as_u64(),
                render_width,
                constraints.min_width,
                constraints.max_width
            ));
        }
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
        let mut lines = Segment::split_and_crop_lines(segments, render_width, None, true, false);
        if let Some(height) = self.child.layout_height() {
            lines = Segment::set_shape(&lines, render_width, Some(height.max(1)), None, false);
        }
        lines = pad_lines_to_width(lines, render_width);

        let content_height = lines.len().max(viewport_height);
        self.content_height.store(content_height, Ordering::Relaxed);
        let content_width = lines
            .iter()
            .map(|line| Segment::get_line_length(line))
            .max()
            .unwrap_or(width)
            .max(width);
        self.content_width.store(content_width, Ordering::Relaxed);

        let max_offset = content_height.saturating_sub(viewport_height);
        let offset = self.offset_y.min(max_offset);
        let max_offset_x = content_width.saturating_sub(width);
        let offset_x = self.offset_x.min(max_offset_x);
        let start = offset.min(lines.len());
        let end = (start + viewport_height).min(lines.len());
        let slice = lines[start..end]
            .to_vec()
            .into_iter()
            .map(|line| {
                let cropped = crop_line_horizontal(&line, offset_x, width);
                Segment::adjust_line_length(&cropped, width, None, true)
            })
            .collect::<Vec<_>>();
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
        let height = self.height.unwrap_or_else(|| options.size.1.max(1));
        let segments = Widget::render(self, console, options);
        let mut lines = Segment::split_and_crop_lines(segments, width, None, true, false);
        let label = if debug.show_sizes {
            Some(format!("{width}x{height}"))
        } else {
            None
        };
        lines = apply_debug_box(
            lines,
            width,
            height.max(3),
            label.as_deref(),
            debug.style_for(0),
        );
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
        let mut child_ctx = EventCtx::default();
        self.child.on_event(event, &mut child_ctx);
        let child_handled = child_ctx.handled();
        ctx.merge_from(child_ctx);
        if child_handled {
            return;
        }
        if let Event::Action(action) = event {
            match action {
                Action::ScrollUp => {
                    let before = self.offset_y;
                    self.scroll_by(-(self.scroll_step as i32));
                    debug_input(&format!(
                        "[scrollview] action=ScrollUp before_y={} after_y={} max_y={}",
                        before,
                        self.offset_y,
                        self.max_offset()
                    ));
                    ctx.set_handled();
                    return;
                }
                Action::ScrollDown => {
                    let before = self.offset_y;
                    self.scroll_by(self.scroll_step as i32);
                    debug_input(&format!(
                        "[scrollview] action=ScrollDown before_y={} after_y={} max_y={}",
                        before,
                        self.offset_y,
                        self.max_offset()
                    ));
                    ctx.set_handled();
                    return;
                }
                Action::ScrollPageUp => {
                    let before = self.offset_y;
                    let page = self.height.unwrap_or(1).max(1);
                    self.scroll_by(-(page as i32));
                    debug_input(&format!(
                        "[scrollview] action=ScrollPageUp page={} before_y={} after_y={} max_y={}",
                        page,
                        before,
                        self.offset_y,
                        self.max_offset()
                    ));
                    ctx.set_handled();
                    return;
                }
                Action::ScrollPageDown => {
                    let before = self.offset_y;
                    let page = self.height.unwrap_or(1).max(1);
                    self.scroll_by(page as i32);
                    debug_input(&format!(
                        "[scrollview] action=ScrollPageDown page={} before_y={} after_y={} max_y={}",
                        page,
                        before,
                        self.offset_y,
                        self.max_offset()
                    ));
                    ctx.set_handled();
                    return;
                }
                Action::ScrollLeft => {
                    let before = self.offset_x;
                    self.scroll_by_x(-(self.scroll_step_x as i32));
                    debug_input(&format!(
                        "[scrollview] action=ScrollLeft before_x={} after_x={} max_x={}",
                        before,
                        self.offset_x,
                        self.max_offset_x()
                    ));
                    ctx.set_handled();
                    return;
                }
                Action::ScrollRight => {
                    let before = self.offset_x;
                    self.scroll_by_x(self.scroll_step_x as i32);
                    debug_input(&format!(
                        "[scrollview] action=ScrollRight before_x={} after_x={} max_x={}",
                        before,
                        self.offset_x,
                        self.max_offset_x()
                    ));
                    ctx.set_handled();
                    return;
                }
                Action::ScrollPageLeft => {
                    let before = self.offset_x;
                    let page = self.viewport_width.load(Ordering::Relaxed).max(1);
                    self.scroll_by_x(-(page as i32));
                    debug_input(&format!(
                        "[scrollview] action=ScrollPageLeft page={} before_x={} after_x={} max_x={}",
                        page,
                        before,
                        self.offset_x,
                        self.max_offset_x()
                    ));
                    ctx.set_handled();
                    return;
                }
                Action::ScrollPageRight => {
                    let before = self.offset_x;
                    let page = self.viewport_width.load(Ordering::Relaxed).max(1);
                    self.scroll_by_x(page as i32);
                    debug_input(&format!(
                        "[scrollview] action=ScrollPageRight page={} before_x={} after_x={} max_x={}",
                        page,
                        before,
                        self.offset_x,
                        self.max_offset_x()
                    ));
                    ctx.set_handled();
                    return;
                }
                _ => {}
            }
        }
    }

    fn on_mouse_scroll(&mut self, delta_x: i32, delta_y: i32, ctx: &mut EventCtx) {
        let before_x = self.offset_x;
        let before_y = self.offset_y;

        if delta_y != 0 {
            self.scroll_by(delta_y.saturating_mul(self.scroll_step as i32));
        }
        if delta_x != 0 {
            self.scroll_by_x(delta_x.saturating_mul(self.scroll_step_x as i32));
        }
        debug_input(&format!(
            "[scrollview] mouse dx={} dy={} before=({}, {}) after=({}, {}) max=({}, {})",
            delta_x,
            delta_y,
            before_x,
            before_y,
            self.offset_x,
            self.offset_y,
            self.max_offset_x(),
            self.max_offset()
        ));

        if self.offset_x != before_x || self.offset_y != before_y {
            ctx.request_repaint();
            ctx.set_handled();
        }
    }

    fn visit_children_mut(&mut self, f: &mut dyn FnMut(&mut dyn Widget)) {
        f(self.child.as_mut());
    }

    fn layout_height(&self) -> Option<usize> {
        if let Some(fixed) = fixed_height_from_constraints(self.layout_constraints()) {
            return Some(fixed);
        }
        self.height
    }
}

impl Renderable for ScrollView {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

pub struct Overlay {
    id: WidgetId,
    base: Box<dyn Widget>,
    modal: Box<dyn Widget>,
    visible: bool,
    styles: WidgetStyles,
}

impl Overlay {
    pub fn new(base: impl Widget + 'static, modal: impl Widget + 'static) -> Self {
        Self {
            id: WidgetId::new(),
            base: Box::new(base),
            modal: Box::new(modal),
            visible: true,
            styles: WidgetStyles::default(),
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
            return self.base.render_styled(console, options);
        }
        let base_renderable = WidgetRenderable::new(self.base.as_ref());
        let modal_renderable = WidgetRenderable::new(self.modal.as_ref());
        let base =
            crate::render::FrameBuffer::from_renderable(console, options, &base_renderable, None);
        let top =
            crate::render::FrameBuffer::from_renderable(console, options, &modal_renderable, None);
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

    fn visit_children_mut(&mut self, f: &mut dyn FnMut(&mut dyn Widget)) {
        f(self.modal.as_mut());
        f(self.base.as_mut());
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }

    fn layout_height(&self) -> Option<usize> {
        if let Some(fixed) = fixed_height_from_constraints(self.layout_constraints()) {
            return Some(fixed);
        }
        if self.visible {
            match (self.base.layout_height(), self.modal.layout_height()) {
                (Some(base), Some(modal)) => Some(base.max(modal)),
                (Some(base), None) => Some(base),
                (None, Some(modal)) => Some(modal),
                (None, None) => None,
            }
        } else {
            self.base.layout_height()
        }
    }
}

impl Renderable for Overlay {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}
