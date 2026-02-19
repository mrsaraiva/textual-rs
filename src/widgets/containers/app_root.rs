use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::compose::ComposeResult;
use crate::css;
use crate::debug::DebugLayout;
use crate::debug::debug_input;
use crate::event::{
    AnimationEase, AnimationLevel, AnimationRequest, AnimationValueEvent, Event, EventCtx,
};
use crate::message::{Message, MessageEvent, ScrollbarAxis, ScrollbarScrollTo};
use crate::node_id::NodeId;
use crate::style::parse_color_like;
use crate::widgets::{
    ScrollBar, ScrollBarCorner, Widget, WidgetStyles, helpers::fixed_height_from_constraints,
    scrollbar_max_offset,
};

pub struct AppRoot {
    children: Vec<Box<dyn Widget>>,
    children_extracted: bool,
    focused: Option<NodeId>,
    styles: WidgetStyles,
    offset_x: f32,
    offset_y: f32,
    scroll_step_x: usize,
    scroll_step_y: usize,
    content_width: AtomicUsize,
    content_height: AtomicUsize,
    viewport_width: AtomicUsize,
    viewport_height: AtomicUsize,
    last_layout_height: u16,
    last_layout_width: u16,
}

#[cfg(test)]
use crate::event::Action;

const APP_ROOT_TYPE_ALIASES: &[&str] = &["AppRoot"];
pub(crate) const APP_ROOT_VSCROLLBAR_ID: &str = "__app_root_vscrollbar";
pub(crate) const APP_ROOT_HSCROLLBAR_ID: &str = "__app_root_hscrollbar";
pub(crate) const APP_ROOT_SCROLLBAR_CORNER_ID: &str = "__app_root_scrollbar_corner";
const APP_ROOT_OFFSET_X_ATTR: &str = "approot.offset_x";
const APP_ROOT_OFFSET_Y_ATTR: &str = "approot.offset_y";
const APP_ROOT_SCROLL_ANIMATION_DURATION: Duration = Duration::from_millis(100);

fn scrollbar_clamp_offset_f32(offset: f32, content_len: usize, viewport_len: usize) -> f32 {
    if !offset.is_finite() {
        return 0.0;
    }
    let max = scrollbar_max_offset(content_len.max(1), viewport_len.max(1)) as f32;
    offset.clamp(0.0, max)
}

fn scrollbar_drag_trace_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("TEXTUAL_DEBUG_SCROLLBAR_DRAG_TRACE")
            .ok()
            .map(|value| {
                let normalized = value.trim().to_ascii_lowercase();
                !(normalized.is_empty()
                    || normalized == "0"
                    || normalized == "false"
                    || normalized == "off"
                    || normalized == "no")
            })
            .unwrap_or(false)
    })
}

impl AppRoot {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            children_extracted: false,
            focused: None,
            styles: WidgetStyles::default(),
            offset_x: 0.0,
            offset_y: 0.0,
            scroll_step_x: 2,
            scroll_step_y: 1,
            content_width: AtomicUsize::new(0),
            content_height: AtomicUsize::new(0),
            viewport_width: AtomicUsize::new(0),
            viewport_height: AtomicUsize::new(0),
            last_layout_height: 0,
            last_layout_width: 0,
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

    pub fn set_virtual_content_size(&self, width: usize, height: usize) {
        self.content_width.store(width.max(1), Ordering::Relaxed);
        self.content_height.store(height.max(1), Ordering::Relaxed);
    }

    fn max_offset_y(&self) -> f32 {
        scrollbar_max_offset(
            self.content_height.load(Ordering::Relaxed).max(1),
            self.viewport_height.load(Ordering::Relaxed).max(1),
        ) as f32
    }

    fn clamp_offsets(&mut self) {
        self.offset_x = scrollbar_clamp_offset_f32(
            self.offset_x,
            self.content_width.load(Ordering::Relaxed).max(1),
            self.viewport_width.load(Ordering::Relaxed).max(1),
        );
        self.offset_y = scrollbar_clamp_offset_f32(
            self.offset_y,
            self.content_height.load(Ordering::Relaxed).max(1),
            self.viewport_height.load(Ordering::Relaxed).max(1),
        );
    }

    fn apply_scrollbar_offset(&mut self, axis: ScrollbarAxis, offset: f32) -> bool {
        let (before_x, before_y) = (self.offset_x, self.offset_y);
        match axis {
            ScrollbarAxis::Horizontal => self.offset_x = offset,
            ScrollbarAxis::Vertical => self.offset_y = offset,
        }
        self.clamp_offsets();
        self.offset_x != before_x || self.offset_y != before_y
    }

    fn clamped_axis_offset(&self, axis: ScrollbarAxis, offset: f32) -> f32 {
        match axis {
            ScrollbarAxis::Horizontal => scrollbar_clamp_offset_f32(
                offset,
                self.content_width.load(Ordering::Relaxed).max(1),
                self.viewport_width.load(Ordering::Relaxed).max(1),
            ),
            ScrollbarAxis::Vertical => scrollbar_clamp_offset_f32(
                offset,
                self.content_height.load(Ordering::Relaxed).max(1),
                self.viewport_height.load(Ordering::Relaxed).max(1),
            ),
        }
    }

    fn axis_offset(&self, axis: ScrollbarAxis) -> f32 {
        match axis {
            ScrollbarAxis::Horizontal => self.offset_x,
            ScrollbarAxis::Vertical => self.offset_y,
        }
    }

    fn request_scroll_animation(
        &mut self,
        axis: ScrollbarAxis,
        to: f32,
        ctx: &mut EventCtx,
    ) -> bool {
        let from = self.axis_offset(axis);
        let to = self.clamped_axis_offset(axis, to);
        if (to - from).abs() <= f32::EPSILON {
            return false;
        }
        let attr = match axis {
            ScrollbarAxis::Horizontal => APP_ROOT_OFFSET_X_ATTR,
            ScrollbarAxis::Vertical => APP_ROOT_OFFSET_Y_ATTR,
        };
        ctx.request_animation(
            AnimationRequest::new(
                self.node_id(),
                attr,
                from,
                to,
                APP_ROOT_SCROLL_ANIMATION_DURATION,
            )
            .with_ease(AnimationEase::OutCubic)
            .with_level(AnimationLevel::Basic),
        );
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
        let mut children = std::mem::take(&mut self.children);

        let mut vbar = ScrollBar::new(true, 2);
        vbar.set_style_id(Some(APP_ROOT_VSCROLLBAR_ID.to_string()));
        children.push(Box::new(vbar));

        let mut hbar = ScrollBar::new(false, 1);
        hbar.set_style_id(Some(APP_ROOT_HSCROLLBAR_ID.to_string()));
        children.push(Box::new(hbar));

        let mut corner = ScrollBarCorner::new();
        corner.set_style_id(Some(APP_ROOT_SCROLLBAR_CORNER_ID.to_string()));
        children.push(Box::new(corner));

        children
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let _ = console;
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);

        let meta = css::selector_meta_generic(self);
        let resolved = css::resolve_style(self, &meta);
        let raw_viewport_w = self.viewport_width.load(Ordering::Relaxed);
        let raw_viewport_h = self.viewport_height.load(Ordering::Relaxed);
        let viewport_w = if raw_viewport_w == 0 {
            width
        } else {
            raw_viewport_w
        }
        .max(1);
        let viewport_h = if raw_viewport_h == 0 {
            height
        } else {
            raw_viewport_h
        }
        .max(1);
        let content_w = self.content_width.load(Ordering::Relaxed).max(1);
        let content_h = self.content_height.load(Ordering::Relaxed).max(1);
        let clamped_offset_x = scrollbar_clamp_offset_f32(self.offset_x, content_w, viewport_w);
        let clamped_offset_y = scrollbar_clamp_offset_f32(self.offset_y, content_h, viewport_h);
        if scrollbar_drag_trace_enabled() {
            debug_input(&format!(
                "[app-root-geom] self=0x{:x} node={} widget={}x{} content={}x{} viewport={}x{} offsets=({:.3}, {:.3})",
                self as *const _ as usize,
                crate::node_id::node_id_to_ffi(self.node_id()),
                width,
                height,
                content_w,
                content_h,
                viewport_w,
                viewport_h,
                clamped_offset_x,
                clamped_offset_y
            ));
        }

        // App/screen baseline surface is a concrete blank renderable using
        // the resolved background.
        let bg = resolved
            .bg
            .or_else(|| parse_color_like("$background"))
            .unwrap_or_else(|| crate::style::Color::rgb(0, 0, 0));
        let base_style = rich_rs::Style::new().with_bgcolor(bg.to_simple_opaque());

        let mut out = Segments::new();
        for row in 0..height {
            out.push(Segment::styled(" ".repeat(width), base_style));

            if row + 1 < height {
                out.push(Segment::line());
            }
        }

        out
    }

    fn render_with_debug(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        _debug: &DebugLayout,
    ) -> Segments {
        Widget::render(self, console, options)
    }

    fn on_mount(&mut self) {}

    fn on_unmount(&mut self) {}

    fn on_tick(&mut self, _tick: u64) {}

    fn on_resize(&mut self, width: u16, height: u16) {
        self.on_layout(width, height);
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.last_layout_height = height.max(1);
        self.last_layout_width = width.max(1);
        self.viewport_width
            .store(self.last_layout_width as usize, Ordering::Relaxed);
        self.viewport_height
            .store(self.last_layout_height as usize, Ordering::Relaxed);
        if scrollbar_drag_trace_enabled() {
            debug_input(&format!(
                "[app-root-layout] self=0x{:x} node={} layout={}x{}",
                self as *const _ as usize,
                crate::node_id::node_id_to_ffi(self.node_id()),
                self.last_layout_width,
                self.last_layout_height
            ));
        }
        self.clamp_offsets();
    }

    fn on_event_capture(&mut self, _event: &Event, _ctx: &mut EventCtx) {}

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if let Event::AnimationValue(AnimationValueEvent {
            target,
            attribute,
            value,
            done,
        }) = event
        {
            if *target == self.node_id() {
                if attribute == APP_ROOT_OFFSET_Y_ATTR {
                    let next = self.clamped_axis_offset(ScrollbarAxis::Vertical, *value);
                    if (next - self.offset_y).abs() > f32::EPSILON {
                        self.offset_y = next;
                        ctx.request_layout_invalidation();
                    }
                    let _ = done;
                    ctx.set_handled();
                    return;
                }
                if attribute == APP_ROOT_OFFSET_X_ATTR {
                    let next = self.clamped_axis_offset(ScrollbarAxis::Horizontal, *value);
                    if (next - self.offset_x).abs() > f32::EPSILON {
                        self.offset_x = next;
                        ctx.request_layout_invalidation();
                    }
                    let _ = done;
                    ctx.set_handled();
                    return;
                }
            }
        }

        let Event::Action(action) = event else {
            return;
        };

        let before_x = self.offset_x;
        let before_y = self.offset_y;
        match action {
            crate::event::Action::ScrollHome => self.offset_y = 0.0,
            crate::event::Action::ScrollEnd => self.offset_y = self.max_offset_y(),
            crate::event::Action::ScrollUp => {
                self.offset_y = (self.offset_y - self.scroll_step_y as f32).max(0.0);
            }
            crate::event::Action::ScrollDown => {
                self.offset_y += self.scroll_step_y as f32;
            }
            crate::event::Action::ScrollPageUp => {
                let page = self.viewport_height.load(Ordering::Relaxed).max(1);
                self.offset_y = (self.offset_y - page as f32).max(0.0);
            }
            crate::event::Action::ScrollPageDown => {
                let page = self.viewport_height.load(Ordering::Relaxed).max(1);
                self.offset_y += page as f32;
            }
            crate::event::Action::ScrollLeft => {
                self.offset_x = (self.offset_x - self.scroll_step_x as f32).max(0.0);
            }
            crate::event::Action::ScrollRight => {
                self.offset_x += self.scroll_step_x as f32;
            }
            crate::event::Action::ScrollPageLeft => {
                let page = self.viewport_width.load(Ordering::Relaxed).max(1);
                self.offset_x = (self.offset_x - page as f32).max(0.0);
            }
            crate::event::Action::ScrollPageRight => {
                let page = self.viewport_width.load(Ordering::Relaxed).max(1);
                self.offset_x += page as f32;
            }
            _ => return,
        }
        self.clamp_offsets();

        if self.offset_x != before_x || self.offset_y != before_y {
            // Root scrolling can move large portions of the composed frame
            // (content + scrollbar thumbs + dock interactions). Request a
            // full-frame invalidation to avoid stale partial-region artifacts.
            ctx.request_layout_invalidation();
            ctx.set_handled();
        }
    }

    fn on_message(&mut self, msg: &MessageEvent, ctx: &mut EventCtx) {
        let Message::ScrollbarScrollTo(ScrollbarScrollTo {
            axis,
            offset,
            animate,
        }) = &msg.message
        else {
            return;
        };
        let changed = if *animate {
            self.request_scroll_animation(*axis, *offset, ctx)
        } else {
            self.apply_scrollbar_offset(*axis, *offset)
        };
        if changed {
            ctx.request_layout_invalidation();
        }
        ctx.set_handled();
    }

    fn on_mouse_scroll(&mut self, delta_x: i32, delta_y: i32, ctx: &mut EventCtx) {
        let before_x = self.offset_x;
        let before_y = self.offset_y;

        if delta_y != 0 {
            self.offset_y += delta_y.saturating_mul(self.scroll_step_y as i32) as f32;
        }
        if delta_x != 0 {
            self.offset_x += delta_x.saturating_mul(self.scroll_step_x as i32) as f32;
        }
        self.clamp_offsets();

        if self.offset_x != before_x || self.offset_y != before_y {
            // Root scrolling can move large portions of the composed frame
            // (content + scrollbar thumbs + dock interactions). Request a
            // full-frame invalidation to avoid stale partial-region artifacts.
            ctx.request_layout_invalidation();
            ctx.set_handled();
        }
    }

    fn on_mouse_move(&mut self, x: u16, y: u16) -> bool {
        let _ = (x, y);
        false
    }

    fn layout_height(&self) -> Option<usize> {
        if let Some(fixed) = fixed_height_from_constraints(self.layout_constraints()) {
            return Some(fixed);
        }
        None
    }

    fn content_width(&self) -> Option<usize> {
        None
    }

    fn scroll_offset(&self) -> (usize, usize) {
        (
            scrollbar_clamp_offset_f32(
                self.offset_x,
                self.content_width.load(Ordering::Relaxed).max(1),
                self.viewport_width.load(Ordering::Relaxed).max(1),
            )
            .round() as usize,
            scrollbar_clamp_offset_f32(
                self.offset_y,
                self.content_height.load(Ordering::Relaxed).max(1),
                self.viewport_height.load(Ordering::Relaxed).max(1),
            )
            .round() as usize,
        )
    }

    fn scroll_offset_f32(&self) -> (f32, f32) {
        (
            scrollbar_clamp_offset_f32(
                self.offset_x,
                self.content_width.load(Ordering::Relaxed).max(1),
                self.viewport_width.load(Ordering::Relaxed).max(1),
            ),
            scrollbar_clamp_offset_f32(
                self.offset_y,
                self.content_height.load(Ordering::Relaxed).max(1),
                self.viewport_height.load(Ordering::Relaxed).max(1),
            ),
        )
    }

    fn scroll_viewport_size(&self) -> Option<(usize, usize)> {
        let viewport_w = self.viewport_width.load(Ordering::Relaxed);
        let viewport_h = self.viewport_height.load(Ordering::Relaxed);
        if viewport_w == 0 || viewport_h == 0 {
            None
        } else {
            Some((viewport_w.max(1), viewport_h.max(1)))
        }
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }

    fn style_type(&self) -> &'static str {
        "Screen"
    }

    fn style_type_aliases(&self) -> &[&'static str] {
        APP_ROOT_TYPE_ALIASES
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
    use crate::widgets::{Button, Horizontal, Input, ListView, VerticalScroll};
    use rich_rs::Console;

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

    #[test]
    fn app_root_matches_screen_selector_type() {
        let root = AppRoot::new();
        assert_eq!(root.style_type(), "Screen");
        assert!(
            root.style_type_aliases().contains(&"AppRoot"),
            "AppRoot alias should remain available for compatibility selectors"
        );
    }

    #[test]
    fn app_root_mouse_scroll_updates_root_offset() {
        let mut root = AppRoot::new();
        root.on_layout(40, 6);
        root.set_virtual_content_size(40, 60);

        let mut ctx = EventCtx::default();
        root.on_mouse_scroll(0, 1, &mut ctx);

        assert!(ctx.handled());
        assert_eq!(root.scroll_offset(), (0, 1));
    }

    #[test]
    fn app_root_scrollbar_message_updates_vertical_offset() {
        let mut root = AppRoot::new();
        root.on_layout(20, 10);
        root.set_virtual_content_size(20, 200);

        let mut ctx = EventCtx::default();
        root.on_message(
            &MessageEvent {
                sender: NodeId::default(),
                message: Message::ScrollbarScrollTo(ScrollbarScrollTo {
                    axis: ScrollbarAxis::Vertical,
                    offset: 24.0,
                    animate: false,
                }),
                control: Some(NodeId::default()),
            },
            &mut ctx,
        );

        assert!(
            ctx.handled(),
            "scrollbar message should be handled by app root"
        );
        assert!(
            ctx.invalidation().layout,
            "scrollbar message should request layout invalidation"
        );
        assert_eq!(
            root.scroll_offset(),
            (0, 24),
            "scrollbar message should set vertical scroll offset"
        );
    }

    #[test]
    fn app_root_scrollbar_message_requests_animation_when_enabled() {
        let mut root = AppRoot::new();
        root.on_layout(20, 10);
        root.set_virtual_content_size(20, 200);

        let mut ctx = EventCtx::default();
        root.on_message(
            &MessageEvent {
                sender: NodeId::default(),
                message: Message::ScrollbarScrollTo(ScrollbarScrollTo {
                    axis: ScrollbarAxis::Vertical,
                    offset: 24.5,
                    animate: true,
                }),
                control: Some(NodeId::default()),
            },
            &mut ctx,
        );

        assert!(ctx.handled());
        assert_eq!(
            root.scroll_offset_f32().1,
            0.0,
            "animated message should not jump offset immediately"
        );
        let requests = ctx.take_animation_requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].target, root.node_id());
        assert_eq!(requests[0].attribute, APP_ROOT_OFFSET_Y_ATTR);
        assert_eq!(requests[0].start, 0.0);
        assert_eq!(requests[0].end, 24.5);
    }

    #[test]
    fn app_root_scrollbar_message_clamps_to_bounds() {
        let mut root = AppRoot::new();
        root.on_layout(20, 10);
        root.set_virtual_content_size(20, 35);

        let mut ctx = EventCtx::default();
        root.on_message(
            &MessageEvent {
                sender: NodeId::default(),
                message: Message::ScrollbarScrollTo(ScrollbarScrollTo {
                    axis: ScrollbarAxis::Vertical,
                    offset: 999.0,
                    animate: false,
                }),
                control: Some(NodeId::default()),
            },
            &mut ctx,
        );

        assert!(ctx.handled());
        assert_eq!(
            root.scroll_offset().1,
            25,
            "offset should clamp to max(content - viewport)"
        );
    }

    #[test]
    fn app_root_scrollbar_message_preserves_fractional_offset() {
        let mut root = AppRoot::new();
        root.on_layout(20, 10);
        root.set_virtual_content_size(20, 200);

        let mut ctx = EventCtx::default();
        root.on_message(
            &MessageEvent {
                sender: NodeId::default(),
                message: Message::ScrollbarScrollTo(ScrollbarScrollTo {
                    axis: ScrollbarAxis::Vertical,
                    offset: 24.5,
                    animate: false,
                }),
                control: Some(NodeId::default()),
            },
            &mut ctx,
        );

        assert!(ctx.handled());
        assert_eq!(root.scroll_offset_f32().1, 24.5);
        assert_eq!(root.scroll_offset().1, 25);
    }
}
