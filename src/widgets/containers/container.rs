use std::sync::atomic::{AtomicUsize, Ordering};

use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::compose::ComposeResult;
use crate::css;
use crate::debug::DebugLayout;
use crate::event::{Event, EventCtx};
use crate::message::{MessageEvent, ScrollbarAxis, ScrollbarScrollTo};
use crate::style::Overflow;
use crate::widgets::{NodeSeed, Widget, helpers::apply_debug_box, scrollbar_max_offset};

/// Synthetic css-ids for the scrollbar lane children a plain container injects
/// when its resolved `overflow` is `auto`/`scroll`. The runtime layout pass
/// (`apply_host_scrollbar_layout`) recognises these ids to reserve the gutter
/// and position the bars, exactly as it does for `ScrollView`/`AppRoot`.
pub(crate) const CONTAINER_VSCROLLBAR_ID: &str = "__container_vscrollbar";
pub(crate) const CONTAINER_HSCROLLBAR_ID: &str = "__container_hscrollbar";
pub(crate) const CONTAINER_SCROLLBAR_CORNER_ID: &str = "__container_scrollbar_corner";

fn clamp_offset_f32(offset: f32, content_len: usize, viewport_len: usize) -> f32 {
    if !offset.is_finite() {
        return 0.0;
    }
    let max = scrollbar_max_offset(content_len.max(1), viewport_len.max(1)) as f32;
    offset.clamp(0.0, max)
}

pub struct Container {
    children: Vec<Box<dyn Widget>>,
    children_extracted: bool,
    seed: NodeSeed,
    /// (index into `children`, css_id, classes) recorded by `with_compose` so
    /// `.with_id()`/`.with_classes()` metadata on declared children reaches the
    /// mounted node.
    child_decl_meta: Vec<crate::widgets::ChildDeclMeta>,
    /// (index into `children`, sink) recorded by `with_compose` for decls bound
    /// via `HandleSlot::bind`.
    child_handle_sinks: Vec<(usize, crate::handle::HandleSink)>,
    // --- Scroll-host state (Python parity: every container can scroll) ---
    //
    // A plain container reserves a scrollbar gutter and scrolls its overflowing
    // children when its resolved `overflow-x`/`overflow-y` is `auto`/`scroll`.
    // This mirrors how `AppRoot` (a multi-child arena container) already works:
    // scrollbar lane children are injected at compose time and the runtime's
    // `apply_host_scrollbar_layout` reserves the gutter + drives the bars.
    offset_x: f32,
    offset_y: f32,
    scroll_step_x: usize,
    scroll_step_y: usize,
    /// Virtual content extent (`set_virtual_content_size`, runtime-driven).
    content_width: AtomicUsize,
    content_height: AtomicUsize,
    /// Visible viewport size after the gutter is reserved (`on_layout`).
    viewport_width: AtomicUsize,
    viewport_height: AtomicUsize,
    /// Resolved overflow per axis, cached during `on_layout` so the scroll-host
    /// trait methods don't depend on a per-node CSS context being active.
    overflow_x: Overflow,
    overflow_y: Overflow,
    /// When `true`, `take_composed_children` does NOT inject scrollbar lanes and
    /// the scroll-host trait methods stay inert. Set for the inner content
    /// holder of `ScrollView`/`ScrollableContainer`, which manage their own
    /// scrollbar lanes — without this the inner container would inject a second,
    /// conflicting set of bars into the same host node.
    suppress_scrollbars: bool,
}

impl Default for Container {
    fn default() -> Self {
        Self::new()
    }
}

impl Container {
    crate::seed_ident_methods!();

    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            children_extracted: false,
            seed: NodeSeed::default(),
            child_decl_meta: Vec::new(),
            child_handle_sinks: Vec::new(),
            offset_x: 0.0,
            offset_y: 0.0,
            scroll_step_x: 2,
            scroll_step_y: 1,
            content_width: AtomicUsize::new(0),
            content_height: AtomicUsize::new(0),
            viewport_width: AtomicUsize::new(0),
            viewport_height: AtomicUsize::new(0),
            // Plain-container default (Python `Container`/`Horizontal`/`Vertical`
            // default CSS is `overflow: hidden hidden`). Overwritten in
            // `on_layout` once the real CSS-resolved overflow is known.
            overflow_x: Overflow::Hidden,
            overflow_y: Overflow::Hidden,
            suppress_scrollbars: false,
        }
    }

    /// Mark this Container as the inner content holder of a scroll host
    /// (`ScrollView`/`ScrollableContainer`): it will not host scrollbar lanes
    /// and stays inert as a scroll host.
    pub(crate) fn suppress_scrollbars(&mut self) {
        self.suppress_scrollbars = true;
    }

    /// Whether scrollbar-lane hosting is suppressed (content-holder role).
    pub(crate) fn is_scrollbar_suppressed(&self) -> bool {
        self.suppress_scrollbars
    }

    /// Whether the container's resolved overflow allows scrolling on an axis.
    fn scrollable_x(&self) -> bool {
        matches!(self.overflow_x, Overflow::Auto | Overflow::Scroll)
    }

    fn scrollable_y(&self) -> bool {
        matches!(self.overflow_y, Overflow::Auto | Overflow::Scroll)
    }

    /// Whether the container is currently acting as a scroll host: its overflow
    /// allows scrolling AND a viewport has been measured. Content/viewport are
    /// compared in the per-axis accessors below.
    fn is_scroll_host(&self) -> bool {
        !self.suppress_scrollbars
            && (self.scrollable_x() || self.scrollable_y())
            && self.viewport_width.load(Ordering::Relaxed) > 0
            && self.viewport_height.load(Ordering::Relaxed) > 0
    }

    fn clamp_offsets(&mut self) {
        self.offset_x = clamp_offset_f32(
            self.offset_x,
            self.content_width.load(Ordering::Relaxed).max(1),
            self.viewport_width.load(Ordering::Relaxed).max(1),
        );
        self.offset_y = clamp_offset_f32(
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

    pub fn with_child(mut self, child: impl Widget + 'static) -> Self {
        self.children.push(Box::new(child));
        self
    }

    /// Add multiple children from a `compose![]` result.
    ///
    /// Preserves each `ChildDecl`'s `id`/`classes` (so CSS id/class selectors
    /// match the mounted nodes) and any `handle_sink` bound via
    /// `HandleSlot::bind`, mirroring `App::mount_declarations`.
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

    /// Read-only access to the container's children.
    pub fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }

    /// Mutable access to the container's children.
    pub fn children_mut(&mut self) -> &mut Vec<Box<dyn Widget>> {
        &mut self.children
    }

    /// Mutable access to the pre-mount `NodeSeed` (css_id, classes, inline styles).
    ///
    /// Valid until the widget is mounted into the arena tree; after mount the
    /// node record is the single source of truth and seed changes have no effect.
    pub fn seed_mut(&mut self) -> &mut NodeSeed {
        &mut self.seed
    }
}

impl Widget for Container {
    fn compose(&mut self) -> ComposeResult {
        Vec::new()
    }

    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        // Idempotent: only the first extraction drains declared children.
        // Re-extraction yields nothing (the arena tree owns the mounted nodes
        // after the first pass).
        //
        // NOTE: scrollbar lanes are NOT injected here. Whether a plain container
        // scrolls depends on its CSS-resolved `overflow`, which is unknown at
        // compose time (it can be set by an external stylesheet). The runtime
        // lazily mounts the lanes during the layout pass once overflow resolves
        // (`ensure_container_scrollbar_lanes` in `runtime::render`), so a
        // non-scrolling `overflow: hidden` container keeps a clean child list.
        if self.children_extracted {
            return Vec::new();
        }
        self.children_extracted = true;
        std::mem::take(&mut self.children)
    }

    fn take_child_handle_sinks(&mut self) -> Vec<(usize, crate::handle::HandleSink)> {
        std::mem::take(&mut self.child_handle_sinks)
    }

    fn take_child_decl_meta(&mut self) -> Vec<crate::widgets::ChildDeclMeta> {
        std::mem::take(&mut self.child_decl_meta)
    }

    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height_limit = options.size.1.max(1);

        // Chrome-only render. Children are rendered through the arena tree.
        let meta = css::selector_meta_generic(self);
        let resolved = css::resolve_style(self, &meta);
        let paints_surface = resolved.bg.is_some()
            || resolved.hatch.is_some()
            || resolved.border_top.is_set()
            || resolved.border_right.is_set()
            || resolved.border_bottom.is_set()
            || resolved.border_left.is_set()
            || resolved.outline_top.is_set()
            || resolved.outline_right.is_set()
            || resolved.outline_bottom.is_set()
            || resolved.outline_left.is_set();
        if !paints_surface {
            return Segments::new();
        }

        let lines = vec![vec![Segment::new(" ".repeat(width))]; height_limit];
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
        let rendered = Widget::render(self, console, options);
        let width = options.size.0.max(1);
        let height_limit = options.size.1.max(1);
        let mut lines = Segment::split_and_crop_lines(rendered, width, None, true, false);
        lines = Segment::set_shape(&lines, width, Some(height_limit), None, false);
        let label = if debug.show_sizes {
            Some(format!("{width}x{height_limit}"))
        } else {
            None
        };
        let boxed = apply_debug_box(
            lines,
            width,
            height_limit,
            label.as_deref(),
            debug.style_for(0),
        );
        let line_count = boxed.len();
        let mut out = Segments::new();
        for (idx, line) in boxed.into_iter().enumerate() {
            out.extend(line);
            if idx + 1 < line_count {
                out.push(Segment::line());
            }
        }
        out
    }

    fn on_mount(&mut self) {}

    fn on_unmount(&mut self) {}

    fn on_tick(&mut self, _tick: u64) {}

    fn on_resize(&mut self, _width: u16, _height: u16) {}

    fn on_layout(&mut self, width: u16, height: u16) {
        // `width`/`height` are the post-gutter CONTENT box size (the scroll
        // viewport) — `apply_layout_info_tree_from_layout_rects` calls this
        // AFTER `apply_host_scrollbar_layout` has reserved any scrollbar lane.
        self.viewport_width
            .store(width.max(1) as usize, Ordering::Relaxed);
        self.viewport_height
            .store(height.max(1) as usize, Ordering::Relaxed);

        // Cache the CSS-resolved overflow so the scroll-host trait methods (which
        // run without a guaranteed per-node style context) can read it cheaply.
        // The layout/render pass guarantees the global stylesheet context is set.
        let meta = css::selector_meta_generic(self);
        let resolved = css::resolve_style(self, &meta);
        let fallback = resolved.overflow.unwrap_or(Overflow::Hidden);
        self.overflow_x = resolved.overflow_x.unwrap_or(fallback);
        self.overflow_y = resolved.overflow_y.unwrap_or(fallback);

        self.clamp_offsets();
    }

    fn on_event_capture(&mut self, _event: &Event, _ctx: &mut EventCtx) {}

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if !self.is_scroll_host() {
            return;
        }
        let Event::Action(action) = event else {
            return;
        };
        let before_x = self.offset_x;
        let before_y = self.offset_y;
        match action {
            crate::event::Action::ScrollHome => self.offset_y = 0.0,
            crate::event::Action::ScrollEnd => {
                self.offset_y = scrollbar_max_offset(
                    self.content_height.load(Ordering::Relaxed).max(1),
                    self.viewport_height.load(Ordering::Relaxed).max(1),
                ) as f32;
            }
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
            ctx.request_layout_invalidation();
            ctx.set_handled();
        }
    }

    fn on_message(&mut self, msg: &MessageEvent, ctx: &mut EventCtx) {
        let Some(ScrollbarScrollTo { axis, offset, .. }) = msg.downcast_ref::<ScrollbarScrollTo>()
        else {
            return;
        };
        if !self.is_scroll_host() {
            return;
        }
        if self.apply_scrollbar_offset(*axis, *offset) {
            ctx.request_layout_invalidation();
        }
        ctx.set_handled();
    }

    fn on_mouse_scroll(&mut self, delta_x: i32, delta_y: i32, ctx: &mut EventCtx) {
        if !self.is_scroll_host() {
            return;
        }
        let before_x = self.offset_x;
        let before_y = self.offset_y;
        if delta_y != 0 && self.scrollable_y() {
            self.offset_y += delta_y.saturating_mul(self.scroll_step_y as i32) as f32;
        }
        if delta_x != 0 && self.scrollable_x() {
            self.offset_x += delta_x.saturating_mul(self.scroll_step_x as i32) as f32;
        }
        self.clamp_offsets();
        if self.offset_x != before_x || self.offset_y != before_y {
            ctx.request_layout_invalidation();
            ctx.set_handled();
        }
    }

    fn on_mouse_move(&mut self, _x: u16, _y: u16) -> bool {
        false
    }

    fn layout_height(&self) -> Option<usize> {
        if self.children_extracted {
            return None;
        }

        let mut total = 0usize;
        for child in &self.children {
            let child_height = child.layout_height()?;
            total = total.saturating_add(child_height.max(1));
        }
        if total == 0 { None } else { Some(total) }
    }

    fn content_width(&self) -> Option<usize> {
        if self.children_extracted {
            return None;
        }
        let mut max_width = 0usize;
        let mut saw = false;
        for child in &self.children {
            if let Some(width) = child.content_width() {
                max_width = max_width.max(width.max(1));
                saw = true;
            }
        }
        if saw { Some(max_width.max(1)) } else { None }
    }

    fn set_virtual_content_size(&mut self, width: usize, height: usize) {
        self.content_width.store(width.max(1), Ordering::Relaxed);
        self.content_height.store(height.max(1), Ordering::Relaxed);
    }

    fn scroll_offset(&self) -> (usize, usize) {
        let (x, y) = self.scroll_offset_f32();
        (x.round() as usize, y.round() as usize)
    }

    fn scroll_offset_f32(&self) -> (f32, f32) {
        if !self.is_scroll_host() {
            return (0.0, 0.0);
        }
        (
            clamp_offset_f32(
                self.offset_x,
                self.content_width.load(Ordering::Relaxed).max(1),
                self.viewport_width.load(Ordering::Relaxed).max(1),
            ),
            clamp_offset_f32(
                self.offset_y,
                self.content_height.load(Ordering::Relaxed).max(1),
                self.viewport_height.load(Ordering::Relaxed).max(1),
            ),
        )
    }

    fn scroll_viewport_size(&self) -> Option<(usize, usize)> {
        if !self.is_scroll_host() {
            return None;
        }
        let vw = self.viewport_width.load(Ordering::Relaxed);
        let vh = self.viewport_height.load(Ordering::Relaxed);
        Some((vw.max(1), vh.max(1)))
    }

    fn scroll_virtual_content_size(&self) -> Option<(usize, usize)> {
        if !self.is_scroll_host() {
            return None;
        }
        let cw = self.content_width.load(Ordering::Relaxed);
        let ch = self.content_height.load(Ordering::Relaxed);
        if cw == 0 || ch == 0 {
            None
        } else {
            Some((cw, ch))
        }
    }

    fn clips_descendants_to_content(&self) -> bool {
        // Only clip when actually scrolling; a non-scrolling plain container
        // keeps its historical (non-clipping) behavior so the `node_has_gutter`
        // / border-padding clip path in the runtime stays the sole authority.
        self.is_scroll_host()
    }

    fn style(&self) -> Option<crate::style::Style> {
        if self.seed.styles.style != Default::default() {
            Some(self.seed.styles.style.clone())
        } else {
            None
        }
    }

    fn set_inline_style(&mut self, style: crate::style::Style) {
        self.seed.styles.style = style;
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
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

    #[test]
    fn compose_returns_empty() {
        let mut c = Container::new().with_child(Label::new("a"));
        assert!(c.compose().is_empty());
    }

    #[test]
    fn take_composed_children_extracts_all() {
        let mut c = Container::new()
            .with_child(Label::new("a"))
            .with_child(Label::new("b"));
        let children = c.take_composed_children();
        // Scrollbar lanes are NOT injected at compose time (they are lazily
        // mounted by the runtime once overflow resolves), so extraction yields
        // exactly the declared children.
        assert_eq!(children.len(), 2);
        // After extraction, internal declared-children Vec is empty.
        assert!(c.children().is_empty());
    }

    #[test]
    fn extraction_is_idempotent() {
        let mut c = Container::new()
            .with_child(Label::new("a"))
            .with_child(Label::new("b"));
        let _ = c.take_composed_children();
        assert!(c.take_composed_children().is_empty());
    }

    #[test]
    fn tree_mode_render_transparent_container_is_empty() {
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
        assert!(segments.is_empty());
    }

    #[test]
    fn tree_mode_render_styled_container_produces_surface_fill() {
        let mut c = Container::new()
            .with_child(Label::new("hello"))
            .with_child(Label::new("world"));
        c.seed.styles.style.bg = Some(crate::style::Color::rgb(10, 20, 30));
        let _ = c.take_composed_children();

        let console = Console::new();
        let mut options = console.options().clone();
        options.size = (10, 4);
        options.max_width = 10;
        options.max_height = 4;
        let segments = Widget::render(&c, &console, &options);
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
    fn tree_mode_layout_height_returns_none() {
        let mut c = Container::new().with_child(Label::new("a"));
        let _ = c.take_composed_children();
        // Without fixed constraints, tree mode returns None.
        assert!(c.layout_height().is_none());
    }

    #[test]
    fn overflow_x_auto_container_reserves_horizontal_scrollbar_gutter() {
        // A PLAIN `Vertical` with `overflow-x: auto` whose auto-width child
        // overflows the viewport must reserve a 1-row horizontal scrollbar
        // gutter, shrinking the content box — exactly like Python. (The layout
        // solver already lets auto-width children overflow a horizontally
        // scrollable parent, so this exercises the full Container scroll-host
        // path end-to-end.)
        use crate::css::StyleSheet;
        use crate::runtime::run_layout_pass;
        use crate::widget_tree::WidgetTree;
        use crate::widgets::{CONTAINER_HSCROLLBAR_ID, Label, Vertical};

        let mut sheet = crate::css::default_widget_stylesheet();
        sheet.extend(&StyleSheet::parse(
            "Vertical { width: 100%; height: 100%; overflow-x: auto; overflow-y: hidden; } \
             Label { width: auto; height: auto; }",
        ));
        let _guard = crate::css::set_style_context(sheet);

        let mut tree = WidgetTree::new();
        // An 80-cell-wide line inside a 40-col viewport overflows horizontally.
        let root_id = tree.set_root(Box::new(
            Vertical::new().with_child(Label::new("x".repeat(80))),
        ));

        let children = {
            let root = tree.get_mut(root_id).expect("root exists");
            root.widget.take_composed_children()
        };
        for child in children {
            tree.mount(root_id, child);
        }

        run_layout_pass(&mut tree, (40, 10));

        // The horizontal scrollbar lane must be displayed and occupy the bottom
        // row of the host.
        let hbar_id = tree
            .walk_depth_first(root_id)
            .into_iter()
            .find(|&n| {
                tree.get(n).and_then(|n| n.css_id.as_deref()) == Some(CONTAINER_HSCROLLBAR_ID)
            })
            .expect("horizontal scrollbar child must exist");
        let hbar = tree.get(hbar_id).expect("hbar node");
        assert!(
            hbar.display,
            "horizontal scrollbar must be shown on overflow"
        );
        let lane_h = hbar.layout_rect.y1.saturating_sub(hbar.layout_rect.y0);
        assert_eq!(lane_h, 1, "horizontal scrollbar lane is 1 row tall");
        assert_eq!(hbar.layout_rect.y1, 10, "lane sits at the bottom edge");

        // The host's content box must shrink by the gutter (9 rows).
        let host = tree.get(root_id).expect("host node");
        let content_h = host.content_rect.y1.saturating_sub(host.content_rect.y0);
        assert_eq!(content_h, 9, "content box shrinks by the 1-row gutter");
    }

    #[test]
    fn overflow_y_auto_horizontal_reserves_vertical_scrollbar_gutter() {
        use crate::css::StyleSheet;
        use crate::runtime::run_layout_pass;
        use crate::widget_tree::WidgetTree;
        use crate::widgets::{CONTAINER_VSCROLLBAR_ID, Horizontal, Label};

        let mut sheet = crate::css::default_widget_stylesheet();
        sheet.extend(&StyleSheet::parse(
            "Horizontal { width: 100%; height: 100%; overflow-y: auto; } \
             Label { width: 1fr; height: 60; }",
        ));
        let _guard = crate::css::set_style_context(sheet);

        let mut tree = WidgetTree::new();
        let root_id = tree.set_root(Box::new(
            Horizontal::new().with_child(Label::new("tall\n".repeat(60))),
        ));
        let children = {
            let root = tree.get_mut(root_id).expect("root exists");
            root.widget.take_composed_children()
        };
        for child in children {
            tree.mount(root_id, child);
        }

        run_layout_pass(&mut tree, (120, 30));

        let vbar_id = tree
            .walk_depth_first(root_id)
            .into_iter()
            .find(|&n| {
                tree.get(n).and_then(|n| n.css_id.as_deref()) == Some(CONTAINER_VSCROLLBAR_ID)
            })
            .expect("vertical scrollbar child must exist");
        let vbar = tree.get(vbar_id).expect("vbar node");
        assert!(vbar.display, "vertical scrollbar must be shown on overflow");
        let lane_w = vbar.layout_rect.x1.saturating_sub(vbar.layout_rect.x0);
        assert_eq!(lane_w, 2, "vertical scrollbar lane is 2 columns wide");
        assert_eq!(vbar.layout_rect.x1, 120, "lane sits at the right edge");

        let host = tree.get(root_id).expect("host node");
        let content_w = host.content_rect.x1.saturating_sub(host.content_rect.x0);
        assert_eq!(content_w, 118, "content box shrinks by the 2-col gutter");
    }

    #[test]
    fn overflow_hidden_container_injects_no_scrollbar_lanes() {
        // The default `overflow: hidden` must NOT inject scrollbar lanes at all
        // (lazy injection is gated on resolved overflow) and must reserve no
        // gutter — the container keeps a clean child list.
        use crate::css::StyleSheet;
        use crate::runtime::run_layout_pass;
        use crate::widget_tree::WidgetTree;
        use crate::widgets::{CONTAINER_VSCROLLBAR_ID, Horizontal, Label};

        let mut sheet = crate::css::default_widget_stylesheet();
        sheet.extend(&StyleSheet::parse(
            "Horizontal { width: 100%; height: 100%; } Label { width: 1fr; height: 60; }",
        ));
        let _guard = crate::css::set_style_context(sheet);

        let mut tree = WidgetTree::new();
        let root_id = tree.set_root(Box::new(
            Horizontal::new().with_child(Label::new("tall\n".repeat(60))),
        ));
        let children = {
            let root = tree.get_mut(root_id).expect("root exists");
            root.widget.take_composed_children()
        };
        for child in children {
            tree.mount(root_id, child);
        }

        run_layout_pass(&mut tree, (120, 30));

        // No scrollbar lane node should have been injected.
        let has_vbar = tree.walk_depth_first(root_id).into_iter().any(|n| {
            tree.get(n).and_then(|n| n.css_id.as_deref()) == Some(CONTAINER_VSCROLLBAR_ID)
        });
        assert!(
            !has_vbar,
            "overflow:hidden container must not inject a scrollbar lane"
        );
        // The host has exactly its declared child (the Label) — no lanes.
        assert_eq!(tree.children(root_id).len(), 1, "only the declared child");

        let host = tree.get(root_id).expect("host node");
        let content_w = host.content_rect.x1.saturating_sub(host.content_rect.x0);
        assert_eq!(content_w, 120, "overflow:hidden reserves no gutter");
    }
}
