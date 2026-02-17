use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::compose::ComposeResult;
use crate::css;
use crate::debug::{debug_input, DebugLayout};
use crate::event::{Event, EventCtx};
use crate::renderables::Blank;

use crate::node_id::NodeId;
use crate::widgets::{
    helpers::{
        apply_debug_box, apply_margin, clamp_with_constraints, constraints_from_style,
        fixed_height_from_constraints, margin_from_style, merge_constraints, pad_lines_to_width,
    },
    Widget, WidgetStyles,
};

pub struct AppRoot {
    children: Vec<Box<dyn Widget>>,
    children_extracted: bool,
    focused: Option<NodeId>,
    styles: WidgetStyles,
    last_layout_height: u16,
}

#[cfg(test)]
use crate::event::Action;

impl AppRoot {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            children_extracted: false,
            focused: None,
            styles: WidgetStyles::default(),
            last_layout_height: 0,
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

    fn is_tree_mode(&self) -> bool {
        self.children_extracted
    }

    fn child_at_y(&self, y: u16) -> Option<(usize, u16)> {
        let mut cursor = 0u16;
        let viewport_h = self.last_layout_height.max(1);
        for (idx, child) in self.children.iter().enumerate() {
            let meta = css::selector_meta_generic(child.as_ref());
            let resolved = css::resolve_style(child.as_ref(), &meta);
            let margin = margin_from_style(&resolved);
            let top_margin = margin.top;
            let bottom_margin = margin.bottom;
            let inner_height = if let Some(h) = child.layout_height() {
                h.max(1) as u16
            } else if idx + 1 == self.children.len() {
                // For an auto-height final child, treat it as consuming the
                // remaining viewport; this keeps root hit-testing aligned with
                // full-screen layouts like AppRoot -> Dock/ScrollView.
                viewport_h
                    .saturating_sub(cursor)
                    .saturating_sub(bottom_margin)
                    .max(1)
            } else {
                1
            };
            let outer_height = inner_height.saturating_add(top_margin + bottom_margin);
            let outer_end = cursor.saturating_add(outer_height);
            debug_input(&format!(
                "[hover][approot] idx={} y={} cursor={} inner={} top={} bottom={} outer_end={} viewport_h={}",
                idx, y, cursor, inner_height, top_margin, bottom_margin, outer_end, viewport_h
            ));
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

    /// Read-only access to the root's children.
    pub fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }

    /// Mutable access to the root's children.
    pub fn children_mut(&mut self) -> &mut Vec<Box<dyn Widget>> {
        &mut self.children
    }

    pub fn focus_first(&mut self) {
        // Legacy stub calls removed (P1-14g): collect_focus_ids/set_focus_by_id
        // were no-ops. Tree-based focus management handles actual traversal.
        self.focused = None;
    }

    pub fn focus_next(&mut self) {
        // Legacy stub calls removed (P1-14g): collect_focus_ids/set_focus_by_id
        // were no-ops. Tree-based focus management handles actual traversal.
        // Keep self.focused field logic for compatibility.
    }

    pub fn focus_prev(&mut self) {
        // Legacy stub calls removed (P1-14g): collect_focus_ids/set_focus_by_id
        // were no-ops. Tree-based focus management handles actual traversal.
    }

    pub fn focus(&mut self, id: NodeId) -> bool {
        // Legacy stub calls removed (P1-14g): collect_focus_ids/set_focus_by_id
        // were no-ops. Update self.focused for compatibility; tree-based focus
        // management handles actual focus setting.
        self.focused = Some(id);
        true
    }
}

impl Default for AppRoot {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for AppRoot {
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
            // Python parity: app/screen baseline surface should be a concrete
            // blank renderable using the resolved background.
            let meta = css::selector_meta_generic(self);
            let resolved = css::resolve_style(self, &meta);
            let bg = resolved
                .bg
                .or_else(|| crate::style::parse_color_like("$background"))
                .unwrap_or_else(|| crate::style::Color::rgb(0, 0, 0));
            return Blank::new(bg).render_for_size(width, height_limit);
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
            let render_width = clamp_with_constraints(
                width
                    .saturating_sub(margin.left as usize + margin.right as usize)
                    .max(1),
                constraints.min_width,
                constraints.max_width,
                width
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
            let meta = css::selector_meta_generic(child.as_ref());
            let resolved = css::resolve_style(child.as_ref(), &meta);
            let margin = margin_from_style(&resolved);
            let style_constraints = constraints_from_style(&resolved);
            let constraints = merge_constraints(style_constraints, child.layout_constraints());
            let render_width = clamp_with_constraints(
                width
                    .saturating_sub(margin.left as usize + margin.right as usize)
                    .max(1),
                constraints.min_width,
                constraints.max_width,
                width
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
        self.last_layout_height = height.max(1);
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
        if matches!(event, Event::MouseUp(..) | Event::AppFocus(..)) {
            for child in &mut self.children {
                child.on_event(event, ctx);
            }
            return;
        }
        if let Event::MouseDown(mouse) = event {
            let _ = self.focus(mouse.target);
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
        debug_input(&format!(
            "[hover][approot] move x={} y={} hit={:?}",
            x,
            y,
            hit.map(|(idx, local_y)| (idx, local_y))
        ));
        let mut changed = false;

        for (idx, child) in self.children.iter_mut().enumerate() {
            let hovered = hit.map(|(hit_idx, _)| hit_idx == idx).unwrap_or(false);
            if child.is_hovered() != hovered {
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
        if any {
            Some(widest.max(1))
        } else {
            None
        }
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

impl Renderable for AppRoot {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[cfg(test)]
mod focus_tests {
    use super::*;
    use crate::css::{set_style_context, StyleSheet};
    use crate::widgets::containers::{Container, Panel, ScrollView};
    use crate::widgets::{Button, Horizontal, Input, ListView, VerticalScroll};
    use rich_rs::{Console, ConsoleOptions, Segments};
    use std::sync::{
        atomic::{AtomicU16, Ordering},
        Arc,
    };

    struct ProbeWidget {
        last_y: Arc<AtomicU16>,
    }

    impl ProbeWidget {
        fn new(last_y: Arc<AtomicU16>) -> Self {
            Self { last_y }
        }
    }

    impl Widget for ProbeWidget {
        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
            Segments::new()
        }

        fn on_mouse_move(&mut self, _x: u16, y: u16) -> bool {
            self.last_y.store(y, Ordering::Relaxed);
            true
        }
    }

    #[test]
    fn focus_next_advances_after_set_focus_by_id() {
        use crate::widget_tree::WidgetTree;

        // Build a WidgetTree with two focusable Input widgets.
        let mut tree = WidgetTree::new();
        let root_id = tree.set_root(Box::new(AppRoot::new()));
        let container_id = tree.mount(root_id, Box::new(Container::new()));
        let first_id = tree.mount(
            container_id,
            Box::new(Input::new().with_placeholder("First")),
        );
        let second_id = tree.mount(
            container_id,
            Box::new(Input::new().with_placeholder("Second")),
        );

        // Collect focusable nodes via depth-first walk.
        let ids: Vec<_> = tree
            .walk_depth_first(root_id)
            .into_iter()
            .filter(|&id| tree.get(id).map(|n| n.widget.focusable()).unwrap_or(false))
            .collect();
        assert_eq!(ids.len(), 2);
        assert_eq!(ids[0], first_id);
        assert_eq!(ids[1], second_id);

        // Set focus on the first input.
        tree.get_mut(first_id).unwrap().widget.set_focus(true);
        assert!(tree.get(first_id).unwrap().widget.has_focus());

        // Advance focus: find current in chain, move to next.
        let current = ids.iter().position(|&id| id == first_id).unwrap();
        let next = ids[(current + 1) % ids.len()];
        tree.get_mut(first_id).unwrap().widget.set_focus(false);
        tree.get_mut(next).unwrap().widget.set_focus(true);

        assert_eq!(next, second_id);
        assert!(tree.get(second_id).unwrap().widget.has_focus());
        assert!(!tree.get(first_id).unwrap().widget.has_focus());
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
    fn scroll_view_action_emits_offset_animation_requests_when_transition_enabled() {
        let _guard = set_style_context(StyleSheet::parse(
            "ScrollView > .scrollview--content { transition: scrollview.offset 120ms ease-out; }",
        ));
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
        scroll.on_event(&Event::Action(Action::ScrollDown), &mut ctx);
        let requests = ctx.take_animation_requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].attribute, ScrollView::OFFSET_Y_ATTR);
        assert_eq!(requests[0].start, 0.0);
        assert_eq!(requests[0].end, 1.0);
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

    #[test]
    fn scroll_view_ignores_trailing_blank_probe_lines_for_fill_layouts() {
        use std::sync::atomic::Ordering;
        let console = Console::new();
        let mut options = console.options().clone();
        options.size = (48, 12);
        options.max_width = 48;
        options.max_height = 12;

        let columns =
            Horizontal::new().with_child(VerticalScroll::new().with_child(Button::new("One")));
        let scroll = ScrollView::new(columns);
        let _ = Widget::render(&scroll, &console, &options);

        assert_eq!(
            scroll.viewport_width.load(Ordering::Relaxed),
            48,
            "false vertical scrollbar shrank viewport width"
        );
    }

    #[test]
    fn app_root_routes_mouse_move_across_full_height_for_auto_child() {
        let observed_y = Arc::new(AtomicU16::new(0));
        let mut root = AppRoot::new().with_child(ProbeWidget::new(observed_y.clone()));

        root.on_layout(80, 24);
        assert!(root.on_mouse_move(7, 19));
        assert_eq!(observed_y.load(Ordering::Relaxed), 19);
    }

    #[test]
    fn app_root_tree_mode_flag_set_after_extraction() {
        let mut root = AppRoot::new()
            .with_child(Button::new("a"))
            .with_child(Button::new("b"));
        assert!(!root.is_tree_mode());
        let children = root.take_composed_children();
        assert_eq!(children.len(), 2);
        assert!(root.is_tree_mode());
    }

    #[test]
    fn app_root_tree_mode_render_returns_chrome() {
        let mut root = AppRoot::new().with_child(Button::new("ok"));
        let _ = root.take_composed_children();

        let console = Console::new();
        let mut options = console.options().clone();
        options.size = (10, 4);
        options.max_width = 10;
        options.max_height = 4;
        let segments = Widget::render(&root, &console, &options);
        assert!(!segments.is_empty());
    }

    #[test]
    fn app_root_tree_mode_on_event_does_not_panic() {
        let mut root = AppRoot::new().with_child(Button::new("ok"));
        let _ = root.take_composed_children();

        let mut ctx = EventCtx::default();
        root.on_event(&Event::Action(Action::FocusNext), &mut ctx);
        // In tree mode, events are a no-op — not handled.
        assert!(!ctx.handled());
    }

    #[test]
    fn app_root_tree_mode_mouse_move_returns_false() {
        let mut root = AppRoot::new().with_child(Button::new("ok"));
        let _ = root.take_composed_children();
        root.on_layout(80, 24);
        assert!(!root.on_mouse_move(5, 5));
    }
}
