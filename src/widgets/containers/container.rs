use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::compose::ComposeResult;
use crate::css;
use crate::debug::{DebugLayout, debug_input};
use crate::event::{Action, Event, EventCtx};
use crate::node_id::NodeId;

use crate::widgets::{
    Widget, WidgetStyles,
    helpers::{
        apply_debug_box, apply_margin, clamp_with_constraints, constraints_from_style,
        fixed_height_from_constraints, margin_from_style, merge_constraints, pad_lines_to_width,
    },
};

pub struct Container {
    children: Vec<Box<dyn Widget>>,
    children_extracted: bool,
    styles: WidgetStyles,
}

impl Container {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            children_extracted: false,
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

    /// Read-only access to the container's children.
    pub fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }

    /// Mutable access to the container's children.
    pub fn children_mut(&mut self) -> &mut Vec<Box<dyn Widget>> {
        &mut self.children
    }

    fn is_tree_mode(&self) -> bool {
        self.children_extracted
    }

    fn child_at_y(&self, y: u16) -> Option<(usize, u16)> {
        let mut cursor = 0u16;
        for (idx, child) in self.children.iter().enumerate() {
            let meta = css::selector_meta_generic(child.as_ref());
            let resolved = css::resolve_style(child.as_ref(), &meta);
            let margin = margin_from_style(&resolved);
            let top_margin = margin.top;
            let bottom_margin = margin.bottom;
            let inner_height = child.layout_height().unwrap_or(1).max(1) as u16;
            let outer_height = inner_height.saturating_add(top_margin + bottom_margin);
            let outer_end = cursor.saturating_add(outer_height);
            if y < outer_end {
                let inner_start = cursor.saturating_add(top_margin);
                let inner_end = inner_start.saturating_add(inner_height);
                if y >= inner_start && y < inner_end {
                    return Some((idx, y.saturating_sub(inner_start)));
                }
                return None;
            }
            cursor = outer_end;
        }
        None
    }

    fn focus_child(&mut self, index: usize) -> bool {
        let mut changed = false;
        for (idx, child) in self.children.iter_mut().enumerate() {
            let should_focus = idx == index && child.focusable() && !child.is_disabled();
            if child.has_focus() != should_focus {
                child.set_focus(should_focus);
                changed = true;
            }
        }
        changed
    }

    fn cycle_focus(&mut self, action: Action) -> bool {
        let mut focusable = Vec::new();
        let mut current = None;
        for (idx, child) in self.children.iter().enumerate() {
            if child.focusable() && !child.is_disabled() {
                if child.has_focus() {
                    current = Some(focusable.len());
                }
                focusable.push(idx);
            }
        }
        if focusable.is_empty() {
            return false;
        }
        let next_pos = match (action, current) {
            (Action::FocusNext, Some(pos)) => (pos + 1) % focusable.len(),
            (Action::FocusPrev, Some(0)) => focusable.len() - 1,
            (Action::FocusPrev, Some(pos)) => pos - 1,
            (Action::FocusNext, None) => 0,
            (Action::FocusPrev, None) => focusable.len() - 1,
            _ => return false,
        };
        self.focus_child(focusable[next_pos])
    }
}

impl Widget for Container {
    fn has_focus(&self) -> bool {
        if self.is_tree_mode() {
            return false;
        }
        self.children.iter().any(|child| child.has_focus())
    }

    fn set_focus(&mut self, focused: bool) {
        if self.is_tree_mode() || focused {
            return;
        }
        for child in &mut self.children {
            if child.has_focus() {
                child.set_focus(false);
            }
        }
    }

    fn compose(&self) -> ComposeResult {
        Vec::new()
    }

    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        self.children_extracted = true;
        std::mem::take(&mut self.children)
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height_limit = options.size.1.max(1);

        if self.is_tree_mode() {
            // Chrome-only render: produce blank fill for the tree pipeline to
            // composite children onto.
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

        let bounds = rich_rs::Region::from_size(width as u32, height_limit as u32);

        let mut lines: Vec<Vec<Segment>> = Vec::new();
        let mut cursor_y: i32 = 0;

        for child in &self.children {
            let meta = css::selector_meta_generic(child.as_ref());
            let resolved = css::resolve_style(child.as_ref(), &meta);
            let margin = margin_from_style(&resolved);
            let style_constraints = constraints_from_style(&resolved);
            let constraints = merge_constraints(style_constraints, child.layout_constraints());
            let available_width = width
                .saturating_sub(margin.left as usize + margin.right as usize)
                .max(1);
            let mut render_width = clamp_with_constraints(
                available_width,
                constraints.min_width,
                constraints.max_width,
                available_width,
            );
            if matches!(resolved.width, Some(crate::style::Scalar::Auto)) {
                let pad = resolved
                    .padding
                    .map(|s| s.left as usize)
                    .unwrap_or(0)
                    .saturating_mul(2);
                let (_, _, border_left, border_right) =
                    crate::widgets::helpers::border_spacing_from_style(&resolved);
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
        if self.is_tree_mode() {
            return Widget::render(self, console, options);
        }

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
        if !self.is_tree_mode() {
            for child in &mut self.children {
                child.on_resize(width, height);
            }
        }
    }

    fn on_layout(&mut self, width: u16, height: u16) {
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
                if let Some((idx, local_y)) = self.child_at_y(mouse.y) {
                    let _ = self.focus_child(idx);
                    let child_event = Event::MouseDown(crate::event::MouseDownEvent {
                        target: NodeId::default(),
                        screen_x: mouse.screen_x,
                        screen_y: mouse.screen_y,
                        x: mouse.x,
                        y: local_y,
                    });
                    if let Some(child) = self.children.get_mut(idx) {
                        child.on_event(&child_event, ctx);
                    }
                }
                return;
            }
            Event::MouseUp(mouse) => {
                if let Some((idx, local_y)) = self.child_at_y(mouse.y) {
                    let child_event = Event::MouseUp(crate::event::MouseUpEvent {
                        target: Some(NodeId::default()),
                        screen_x: mouse.screen_x,
                        screen_y: mouse.screen_y,
                        x: mouse.x,
                        y: local_y,
                    });
                    if let Some(child) = self.children.get_mut(idx) {
                        child.on_event(&child_event, ctx);
                    }
                }
                return;
            }
            Event::MouseScroll(mouse) => {
                if let Some((idx, local_y)) = self.child_at_y(mouse.y) {
                    let child_event = Event::MouseScroll(crate::event::MouseScrollEvent {
                        target: Some(NodeId::default()),
                        screen_x: mouse.screen_x,
                        screen_y: mouse.screen_y,
                        x: mouse.x,
                        y: local_y,
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
        let hit = self.child_at_y(y);
        let mut changed = false;
        debug_input(&format!(
            "[hover][container] x={} y={} hit={:?}",
            x,
            y,
            hit.map(|(idx, local_y)| (idx, local_y))
        ));

        for (idx, child) in self.children.iter_mut().enumerate() {
            let hovered = hit.map(|(hit_idx, _)| hit_idx == idx).unwrap_or(false);
            if child.is_hovered() != hovered {
                debug_input(&format!(
                    "[hover][container] set child={} hovered={}",
                    idx, hovered
                ));
                child.set_hovered(hovered);
                changed = true;
            }
        }

        if let Some((idx, local_y)) = hit {
            if let Some(child) = self.children.get_mut(idx) {
                changed |= child.on_mouse_move(x, local_y);
            }
        }

        changed
    }

    fn layout_height(&self) -> Option<usize> {
        if let Some(fixed) = fixed_height_from_constraints(self.layout_constraints()) {
            return Some(fixed);
        }
        if self.is_tree_mode() {
            return None;
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
                        .saturating_add(margin.top as usize + margin.bottom as usize);
                }
                None => return None,
            }
        }
        Some(total.max(1))
    }

    fn content_width(&self) -> Option<usize> {
        if self.is_tree_mode() {
            return None;
        }
        let mut widest = 0usize;
        let mut any = false;
        for child in &self.children {
            let meta = css::selector_meta_generic(child.as_ref());
            let resolved = css::resolve_style(child.as_ref(), &meta);
            let margin = margin_from_style(&resolved);
            if let Some(width) = child.content_width() {
                widest =
                    widest.max(width.saturating_add(margin.left as usize + margin.right as usize));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::Label;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Clone)]
    struct LayoutProbe {
        layout_hits: Arc<AtomicUsize>,
    }

    impl LayoutProbe {
        fn new(layout_hits: Arc<AtomicUsize>) -> Self {
            Self { layout_hits }
        }
    }

    impl Widget for LayoutProbe {
        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
            Segments::new()
        }

        fn on_layout(&mut self, _width: u16, _height: u16) {
            self.layout_hits.fetch_add(1, Ordering::Relaxed);
        }
    }

    #[test]
    fn compose_returns_empty() {
        let c = Container::new().with_child(Label::new("a"));
        assert!(c.compose().is_empty());
    }

    #[test]
    fn take_composed_children_extracts_all() {
        let mut c = Container::new()
            .with_child(Label::new("a"))
            .with_child(Label::new("b"));
        let children = c.take_composed_children();
        assert_eq!(children.len(), 2);
        // After extraction, internal Vec is empty.
        assert!(c.children().is_empty());
    }

    #[test]
    fn tree_mode_flag_set_after_extraction() {
        let mut c = Container::new()
            .with_child(Label::new("a"))
            .with_child(Label::new("b"));
        assert!(!c.is_tree_mode());
        let _ = c.take_composed_children();
        assert!(c.is_tree_mode());
    }

    #[test]
    fn tree_mode_render_returns_chrome_not_blank() {
        let mut c = Container::new()
            .with_child(Label::new("hello"))
            .with_child(Label::new("world"));
        let _ = c.take_composed_children();

        let console = Console::new();
        let mut options = console.options().clone();
        options.size = (10, 4);
        options.max_width = 10;
        options.max_height = 4;
        let segments = Widget::render(&c, &console, &options);
        // Should produce non-empty segments (blank fill chrome).
        assert!(!segments.is_empty());
    }

    #[test]
    fn tree_mode_on_event_does_not_panic() {
        let mut c = Container::new().with_child(Label::new("a"));
        let _ = c.take_composed_children();

        let mut ctx = EventCtx::default();
        // Key event should not panic even though children are gone.
        c.on_event(&Event::Action(crate::event::Action::FocusNext), &mut ctx);
        assert!(!ctx.handled());
    }

    #[test]
    fn tree_mode_on_mouse_move_returns_false() {
        let mut c = Container::new().with_child(Label::new("a"));
        let _ = c.take_composed_children();
        assert!(!c.on_mouse_move(0, 0));
    }

    #[test]
    fn non_tree_on_layout_forwards_to_children() {
        let hits = Arc::new(AtomicUsize::new(0));
        let probe = LayoutProbe::new(hits.clone());
        let mut c = Container::new().with_child(probe);

        c.on_layout(50, 10);

        assert_eq!(hits.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn tree_mode_layout_height_returns_none() {
        let mut c = Container::new().with_child(Label::new("a"));
        let _ = c.take_composed_children();
        // Without fixed constraints, tree mode returns None.
        assert!(c.layout_height().is_none());
    }
}
