use std::io::Write;

use crossterm::event::KeyCode;
use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::css;
use crate::debug::DebugLayout;
use crate::event::{Action, Event, EventCtx};

use crate::node_id::{NodeId, node_id_to_ffi};
use crate::widgets::{
    Widget, WidgetStyles,
    helpers::{
        apply_debug_box, apply_margin, clamp_with_constraints, collect_focus_ids,
        constraints_from_style, dispatch_event_to_focus, fixed_height_from_constraints,
        margin_from_style, merge_constraints, pad_lines_to_width, set_focus_by_id,
    },
};

pub struct AppRoot {
    children: Vec<Box<dyn Widget>>,
    focused: Option<NodeId>,
    styles: WidgetStyles,
}

impl AppRoot {
    pub fn new() -> Self {
        Self {
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
                ids.iter().map(|id| node_id_to_ffi(*id)).collect::<Vec<_>>()
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
                self.focused.map(|id| node_id_to_ffi(id)),
                node_id_to_ffi(next)
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

    pub fn focus(&mut self, id: NodeId) -> bool {
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
    use crate::css::{StyleSheet, set_style_context};
    use crate::widgets::containers::{Container, Panel, ScrollView};
    use crate::widgets::{
        Button, Horizontal, Input, ListView, VerticalScroll, collect_focus_ids, set_focus_by_id,
    };
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
}
