use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::compose::ComposeResult;
use crate::debug::{DebugLayout, debug_input, debug_layout};
use crate::event::{
    Action, AnimationEase, AnimationLevel, AnimationRequest, AnimationValueEvent, Event, EventCtx,
};
use crate::message::{MessageEvent, ScrollbarAxis, ScrollbarScrollTo};
use crate::style::{Overflow, ScrollbarGutter, ScrollbarVisibility, parse_color_like};

use crate::action::ParsedAction;
use crate::node_id::NodeId;
use crate::renderables::Blank;
use crate::widgets::scrollbar;
use crate::widgets::{
    BindingDecl, Container, NodeSeed, ScrollBar, ScrollBarCorner, Spacer, Widget, WidgetStyles,
    helpers::{
        adjust_line_length_no_bg, apply_debug_box, clamp_with_constraints, crop_line_horizontal,
        fixed_height_from_constraints, pad_lines_to_width,
    },
};

pub(crate) const SCROLL_VIEW_VSCROLLBAR_ID: &str = "__scrollview_vscrollbar";
pub(crate) const SCROLL_VIEW_HSCROLLBAR_ID: &str = "__scrollview_hscrollbar";
pub(crate) const SCROLL_VIEW_SCROLLBAR_CORNER_ID: &str = "__scrollview_scrollbar_corner";

pub struct ScrollView {
    child: Box<dyn Widget>,
    child_extracted: bool,
    height: Option<usize>,
    pub(crate) offset_y: usize,
    render_offset_y: f32,
    scroll_step: usize,
    pub(crate) content_height: AtomicUsize,
    pub(crate) viewport_height: AtomicUsize,
    offset_x: usize,
    render_offset_x: f32,
    scroll_step_x: usize,
    pub(crate) content_width: AtomicUsize,
    pub(crate) viewport_width: AtomicUsize,
    widget_width: AtomicUsize,
    widget_height: AtomicUsize,
    drag_v: Option<usize>,
    drag_h: Option<usize>,
    hover_v_thumb: bool,
    hover_v_track: bool,
    hover_h_thumb: bool,
    hover_h_track: bool,
    seed: NodeSeed,
}

impl ScrollView {
    pub(crate) const OFFSET_Y_ATTR: &'static str = "scrollview.offset_y";
    const OFFSET_X_ATTR: &'static str = "scrollview.offset_x";

    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            child: Box::new(child),
            child_extracted: false,
            height: None,
            offset_y: 0,
            render_offset_y: 0.0,
            scroll_step: 1,
            content_height: AtomicUsize::new(0),
            viewport_height: AtomicUsize::new(0),
            offset_x: 0,
            render_offset_x: 0.0,
            scroll_step_x: 2,
            content_width: AtomicUsize::new(0),
            viewport_width: AtomicUsize::new(0),
            widget_width: AtomicUsize::new(0),
            widget_height: AtomicUsize::new(0),
            drag_v: None,
            drag_h: None,
            hover_v_thumb: false,
            hover_v_track: false,
            hover_h_thumb: false,
            hover_h_track: false,
            seed: NodeSeed::default(),
        }
    }

    fn child_container_mut(&mut self) -> Option<&mut Container> {
        let child_any = &mut *self.child as &mut dyn std::any::Any;
        child_any.downcast_mut::<Container>()
    }

    pub fn with_child(mut self, child: impl Widget + 'static) -> Self {
        if let Some(container) = self.child_container_mut() {
            container.push(child);
        }
        self
    }

    pub fn with_compose(mut self, children: ComposeResult) -> Self {
        if let Some(container) = self.child_container_mut() {
            let existing = std::mem::replace(container, Container::new());
            *container = existing.with_compose(children);
        }
        self
    }

    pub fn push(&mut self, child: impl Widget + 'static) {
        if let Some(container) = self.child_container_mut() {
            container.push(child);
        }
    }

    pub fn height(mut self, height: usize) -> Self {
        self.height = Some(height.max(1));
        self
    }

    pub fn scroll_to(&mut self, offset_y: usize) {
        self.offset_y = offset_y;
        self.clamp_offset();
        self.render_offset_y = self.offset_y as f32;
    }

    /// Scroll to the top of the content (offset 0).
    ///
    /// Mirrors Python `VerticalScroll.scroll_home(animate=False)`.
    pub fn scroll_home(&mut self) {
        self.scroll_to(0);
    }

    /// Scroll to the end of the content.
    ///
    /// Mirrors Python `VerticalScroll.scroll_end(animate=False)`.
    pub fn scroll_end(&mut self) {
        self.scroll_to(self.max_offset());
    }

    pub fn scroll_to_x(&mut self, offset_x: usize) {
        self.offset_x = offset_x;
        self.clamp_offset();
        self.render_offset_x = self.offset_x as f32;
    }

    pub fn scroll_by(&mut self, delta: i32) {
        if delta.is_negative() {
            self.offset_y = self.offset_y.saturating_sub(delta.unsigned_abs() as usize);
        } else {
            self.offset_y = self.offset_y.saturating_add(delta as usize);
        }
        self.clamp_offset();
        self.render_offset_y = self.offset_y as f32;
    }

    pub fn scroll_by_x(&mut self, delta: i32) {
        if delta.is_negative() {
            self.offset_x = self.offset_x.saturating_sub(delta.unsigned_abs() as usize);
        } else {
            self.offset_x = self.offset_x.saturating_add(delta as usize);
        }
        self.clamp_offset();
        self.render_offset_x = self.offset_x as f32;
    }

    pub fn scroll_step(mut self, step: usize) -> Self {
        self.scroll_step = step.max(1);
        self
    }

    pub fn set_scroll_step(&mut self, step: usize) {
        self.scroll_step = step.max(1);
    }

    pub fn scroll_step_x(mut self, step: usize) -> Self {
        self.scroll_step_x = step.max(1);
        self
    }

    pub fn set_scroll_step_x(&mut self, step: usize) {
        self.scroll_step_x = step.max(1);
    }

    pub fn offset_y(&self) -> usize {
        self.offset_y
    }

    pub fn offset_x(&self) -> usize {
        self.offset_x
    }

    /// Set the virtual content dimensions (for testing / programmatic use).
    pub fn set_virtual_content_size(&self, width: usize, height: usize) {
        self.content_width
            .store(width, std::sync::atomic::Ordering::Relaxed);
        self.content_height
            .store(height, std::sync::atomic::Ordering::Relaxed);
    }

    pub(crate) fn line_max_offset(content_len: usize, viewport_len: usize) -> usize {
        scrollbar::max_offset(content_len, viewport_len)
    }

    pub(crate) fn line_clamp_offset(
        offset: usize,
        content_len: usize,
        viewport_len: usize,
    ) -> usize {
        scrollbar::clamp_offset(offset, content_len, viewport_len)
    }

    pub(crate) fn line_scroll_by(
        offset: usize,
        delta: i32,
        content_len: usize,
        viewport_len: usize,
    ) -> usize {
        scrollbar::scroll_by(offset, delta, content_len, viewport_len)
    }

    pub(crate) fn line_scroll_end(content_len: usize, viewport_len: usize) -> usize {
        scrollbar::scroll_end(content_len, viewport_len)
    }

    pub(crate) fn line_scrollbar_thumb(
        track_len: usize,
        content_len: usize,
        viewport_len: usize,
        offset: usize,
    ) -> (usize, usize) {
        scrollbar::thumb_range(track_len, content_len, viewport_len, offset)
    }

    pub(crate) fn line_drag_offset(
        pointer: usize,
        grab_offset: usize,
        track_len: usize,
        content_len: usize,
        viewport_len: usize,
        current_offset: usize,
    ) -> usize {
        scrollbar::drag_to_offset(
            pointer,
            grab_offset,
            track_len,
            content_len,
            viewport_len,
            current_offset,
        )
    }

    pub(crate) fn line_scrollbar_styles() -> (rich_rs::Style, rich_rs::Style, rich_rs::Style) {
        let track_bg = parse_color_like("$scrollbar-background")
            .or_else(|| parse_color_like("$background-darken-1"))
            .or_else(|| parse_color_like("$surface-darken-1"))
            .unwrap_or_else(|| crate::style::Color::rgb(30, 30, 30));
        let thumb_bg = parse_color_like("$scrollbar")
            .or_else(|| parse_color_like("$primary-muted"))
            .or_else(|| parse_color_like("$primary"))
            .unwrap_or_else(|| crate::style::Color::rgb(48, 156, 255));
        let thumb_active_bg = parse_color_like("$scrollbar-active")
            .or_else(|| parse_color_like("$primary"))
            .unwrap_or_else(|| crate::style::Color::rgb(1, 120, 212));

        let track_style = rich_rs::Style::new().with_bgcolor(track_bg.to_simple_opaque());
        let thumb_style = rich_rs::Style::new().with_bgcolor(thumb_bg.to_simple_opaque());
        let thumb_active_style =
            rich_rs::Style::new().with_bgcolor(thumb_active_bg.to_simple_opaque());
        (track_style, thumb_style, thumb_active_style)
    }

    fn blank_run(width: usize, style: rich_rs::Style) -> Vec<Segment> {
        let width = width.max(1);
        let blank = if let Some(bg) = style.bgcolor {
            Blank::new(crate::style::color_from_simple(bg))
        } else {
            Blank::transparent()
        };
        let mut line = blank.line_for_width(width);
        for seg in &mut line {
            if seg.control.is_none() {
                seg.style = Some(style);
            }
        }
        line
    }

    /// Compute visual content height while ignoring probe-introduced trailing blank lines.
    ///
    /// Some auto-height/fill containers render into an oversized probe height and emit
    /// whitespace-only tail rows. Those rows should not trigger vertical scrollbar visibility.
    fn effective_content_height(lines: &[Vec<Segment>]) -> usize {
        let last_non_blank = lines.iter().rposition(|line| {
            line.iter()
                .filter(|segment| !segment.is_control())
                .any(|segment| segment.text.chars().any(|ch| ch != ' '))
        });
        last_non_blank.map(|idx| idx + 1).unwrap_or(1)
    }

    fn max_offset(&self) -> usize {
        Self::line_max_offset(
            self.content_height.load(Ordering::Relaxed),
            self.viewport_height.load(Ordering::Relaxed),
        )
    }

    fn max_offset_x(&self) -> usize {
        Self::line_max_offset(
            self.content_width.load(Ordering::Relaxed),
            self.viewport_width.load(Ordering::Relaxed),
        )
    }

    fn clamp_offset(&mut self) {
        let max_y = self.max_offset();
        if self.offset_y > max_y {
            self.offset_y = max_y;
        }
        self.render_offset_y = self.render_offset_y.clamp(0.0, max_y as f32);
        let max_x = self.max_offset_x();
        if self.offset_x > max_x {
            self.offset_x = max_x;
        }
        self.render_offset_x = self.render_offset_x.clamp(0.0, max_x as f32);
    }

    // transition_timing_to_animation_ease removed — delegated to
    // crate::runtime::event_loop::resolve_transition_for_property.

    fn request_offset_y_animation_with_duration(
        &mut self,
        from: usize,
        to: usize,
        duration_override: Option<Duration>,
        ctx: &mut EventCtx,
    ) {
        if from == to {
            return;
        }
        if let Some(duration) = duration_override
            && !duration.is_zero()
        {
            self.render_offset_y = from as f32;
            ctx.request_animation(
                AnimationRequest::new(
                    self.node_id(),
                    Self::OFFSET_Y_ATTR,
                    from as f32,
                    to as f32,
                    duration,
                )
                .with_ease(AnimationEase::OutCubic)
                .with_level(AnimationLevel::Basic),
            );
        } else if let Some((duration, delay, ease)) =
            self.animation_params_for_property(Self::OFFSET_Y_ATTR)
        {
            self.render_offset_y = from as f32;
            ctx.request_animation(
                AnimationRequest::new(
                    self.node_id(),
                    Self::OFFSET_Y_ATTR,
                    from as f32,
                    to as f32,
                    duration,
                )
                .with_delay(delay)
                .with_ease(ease)
                .with_level(AnimationLevel::Basic),
            );
        } else {
            self.render_offset_y = to as f32;
        }
        ctx.request_repaint();
    }

    fn request_offset_y_animation(&mut self, from: usize, to: usize, ctx: &mut EventCtx) {
        self.request_offset_y_animation_with_duration(from, to, None, ctx);
    }

    fn request_offset_x_animation_with_duration(
        &mut self,
        from: usize,
        to: usize,
        duration_override: Option<Duration>,
        ctx: &mut EventCtx,
    ) {
        if from == to {
            return;
        }
        if let Some(duration) = duration_override
            && !duration.is_zero()
        {
            self.render_offset_x = from as f32;
            ctx.request_animation(
                AnimationRequest::new(
                    self.node_id(),
                    Self::OFFSET_X_ATTR,
                    from as f32,
                    to as f32,
                    duration,
                )
                .with_ease(AnimationEase::OutCubic)
                .with_level(AnimationLevel::Basic),
            );
        } else if let Some((duration, delay, ease)) =
            self.animation_params_for_property(Self::OFFSET_X_ATTR)
        {
            self.render_offset_x = from as f32;
            ctx.request_animation(
                AnimationRequest::new(
                    self.node_id(),
                    Self::OFFSET_X_ATTR,
                    from as f32,
                    to as f32,
                    duration,
                )
                .with_delay(delay)
                .with_ease(ease)
                .with_level(AnimationLevel::Basic),
            );
        } else {
            self.render_offset_x = to as f32;
        }
        ctx.request_repaint();
    }

    fn request_offset_x_animation(&mut self, from: usize, to: usize, ctx: &mut EventCtx) {
        self.request_offset_x_animation_with_duration(from, to, None, ctx);
    }

    fn child_coords(&self, x: u16, y: u16) -> (u16, u16) {
        (
            x.saturating_add(self.offset_x as u16),
            y.saturating_add(self.offset_y as u16),
        )
    }

    fn sync_child_layout(&mut self) {
        if self.child_extracted {
            return;
        }
        let width = self.viewport_width.load(Ordering::Relaxed).max(1) as u16;
        let height = self.viewport_height.load(Ordering::Relaxed).max(1) as u16;
        self.child.on_layout(width, height);
    }

    fn update_scrollbar_hover_state(&mut self, x: u16, y: u16) -> bool {
        let widget_width = self.widget_width.load(Ordering::Relaxed).max(1);
        let widget_height = self.widget_height.load(Ordering::Relaxed).max(1);
        let viewport_w = self.viewport_width.load(Ordering::Relaxed).max(1);
        let viewport_h = self.viewport_height.load(Ordering::Relaxed).max(1);
        let content_w = self.content_width.load(Ordering::Relaxed);
        let content_h = self.content_height.load(Ordering::Relaxed);
        let show_v = content_h > viewport_h;
        let show_h = content_w > viewport_w;
        let v_scrollbar_size = widget_width.saturating_sub(viewport_w).max(1);
        let h_scrollbar_size = widget_height.saturating_sub(viewport_h).max(1);
        let local_x = x as usize;
        let local_y = y as usize;

        let mut next_v_track = false;
        let mut next_v_thumb = false;
        if show_v
            && local_x >= widget_width.saturating_sub(v_scrollbar_size)
            && local_y < viewport_h
        {
            next_v_track = true;
            let offset = self
                .render_offset_y
                .clamp(0.0, self.max_offset() as f32)
                .round() as usize;
            let (thumb_start, thumb_len) =
                Self::line_scrollbar_thumb(viewport_h, content_h, viewport_h, offset);
            next_v_thumb =
                local_y >= thumb_start && local_y < thumb_start.saturating_add(thumb_len);
        }

        let mut next_h_track = false;
        let mut next_h_thumb = false;
        if show_h
            && local_y >= widget_height.saturating_sub(h_scrollbar_size)
            && local_x < viewport_w
        {
            next_h_track = true;
            let offset = self
                .render_offset_x
                .clamp(0.0, self.max_offset_x() as f32)
                .round() as usize;
            let (thumb_start, thumb_len) =
                Self::line_scrollbar_thumb(viewport_w, content_w, viewport_w, offset);
            next_h_thumb =
                local_x >= thumb_start && local_x < thumb_start.saturating_add(thumb_len);
        }

        let changed = self.hover_v_thumb != next_v_thumb
            || self.hover_v_track != next_v_track
            || self.hover_h_thumb != next_h_thumb
            || self.hover_h_track != next_h_track;
        self.hover_v_thumb = next_v_thumb;
        self.hover_v_track = next_v_track;
        self.hover_h_thumb = next_h_thumb;
        self.hover_h_track = next_h_track;
        changed
    }

    /// Resolve all scrollbar-related CSS properties for a single render pass.
    ///
    /// CSS fields take priority; falls back to theme tokens matching the
    /// existing `line_scrollbar_styles()` defaults.
    fn resolve_scrollbar_css(&self) -> ResolvedScrollbar {
        let meta = crate::css::selector_meta_generic(self);
        let style = crate::css::resolve_style(self, &meta);
        let fallback_overflow = style.overflow.unwrap_or(crate::style::Overflow::Auto);

        // Track background: CSS → theme token.
        let track_bg = style.scrollbar_background.unwrap_or_else(|| {
            parse_color_like("$scrollbar-background")
                .or_else(|| parse_color_like("$background-darken-1"))
                .or_else(|| parse_color_like("$surface-darken-1"))
                .unwrap_or_else(|| crate::style::Color::rgb(30, 30, 30))
        });

        // Thumb idle: CSS scrollbar_color → $scrollbar token.
        let thumb_bg = style.scrollbar_color.unwrap_or_else(|| {
            parse_color_like("$scrollbar")
                .or_else(|| parse_color_like("$primary-muted"))
                .or_else(|| parse_color_like("$primary"))
                .unwrap_or_else(|| crate::style::Color::rgb(48, 156, 255))
        });
        let thumb_hover_bg = style.scrollbar_color_hover.unwrap_or(thumb_bg);

        // Thumb active: CSS scrollbar_color_active → $scrollbar-active token.
        let thumb_active_bg = style.scrollbar_color_active.unwrap_or_else(|| {
            parse_color_like("$scrollbar-active")
                .or_else(|| parse_color_like("$primary"))
                .unwrap_or_else(|| crate::style::Color::rgb(1, 120, 212))
        });

        // Corner color.
        let corner_bg = style.scrollbar_corner_color.unwrap_or_else(|| {
            parse_color_like("$scrollbar-corner-color")
                .or_else(|| parse_color_like("$scrollbar-background"))
                .or_else(|| parse_color_like("$background-darken-1"))
                .or_else(|| parse_color_like("$surface-darken-1"))
                .unwrap_or_else(|| crate::style::Color::rgb(30, 30, 30))
        });
        let track_hover_bg = style.scrollbar_background_hover.unwrap_or(track_bg);
        let track_active_bg = style.scrollbar_background_active.unwrap_or(track_bg);

        // Scrollbar sizes: per-axis CSS → shorthand CSS → defaults (2 vertical, 1 horizontal).
        let v_size = style
            .scrollbar_size_vertical
            .or(style.scrollbar_size)
            .map(|s| s as usize)
            .unwrap_or(2);
        let h_size = style
            .scrollbar_size_horizontal
            .or(style.scrollbar_size)
            .map(|s| s as usize)
            .unwrap_or(1);

        ResolvedScrollbar {
            overflow_x: style.overflow_x.unwrap_or(fallback_overflow),
            overflow_y: style.overflow_y.unwrap_or(fallback_overflow),
            track_style: rich_rs::Style::new().with_bgcolor(track_bg.to_simple_opaque()),
            track_hover_style: rich_rs::Style::new()
                .with_bgcolor(track_hover_bg.to_simple_opaque()),
            track_active_style: rich_rs::Style::new()
                .with_bgcolor(track_active_bg.to_simple_opaque()),
            thumb_style: rich_rs::Style::new().with_bgcolor(thumb_bg.to_simple_opaque()),
            thumb_hover_style: rich_rs::Style::new()
                .with_bgcolor(thumb_hover_bg.to_simple_opaque()),
            thumb_active_style: rich_rs::Style::new()
                .with_bgcolor(thumb_active_bg.to_simple_opaque()),
            corner_style: rich_rs::Style::new().with_bgcolor(corner_bg.to_simple_opaque()),
            v_size,
            h_size,
            visibility: style
                .scrollbar_visibility
                .unwrap_or(ScrollbarVisibility::Auto),
            gutter: style.scrollbar_gutter.unwrap_or(ScrollbarGutter::Auto),
        }
    }

    /// Look up per-property transition parameters for a given attribute name.
    ///
    /// Delegates to the shared `resolve_transition_for_property()` helper which
    /// checks the `transitions` CSS vec first, then falls back to the generic
    /// `transition-duration / delay / timing` properties.
    fn animation_params_for_property(
        &self,
        property: &str,
    ) -> Option<(Duration, Duration, AnimationEase)> {
        let style = crate::css::resolve_component_style(self, &["scrollview--content"]);
        crate::runtime::resolve_transition_for_property(&style, property)
    }
}

/// CSS-resolved scrollbar configuration for a single render pass.
struct ResolvedScrollbar {
    overflow_x: crate::style::Overflow,
    overflow_y: crate::style::Overflow,
    track_style: rich_rs::Style,
    track_hover_style: rich_rs::Style,
    track_active_style: rich_rs::Style,
    thumb_style: rich_rs::Style,
    thumb_hover_style: rich_rs::Style,
    thumb_active_style: rich_rs::Style,
    corner_style: rich_rs::Style,
    v_size: usize,
    h_size: usize,
    visibility: ScrollbarVisibility,
    gutter: ScrollbarGutter,
}

impl Widget for ScrollView {
    fn compose(&self) -> crate::compose::ComposeResult {
        Vec::new()
    }

    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        if self.child_extracted {
            return Vec::new();
        }
        self.child_extracted = true;
        let mut children = Vec::with_capacity(4);
        let child = std::mem::replace(&mut self.child, Box::new(Spacer::new(1)));
        children.push(child);

        let mut vbar = ScrollBar::new(true, 2);
        vbar.set_style_id(Some(SCROLL_VIEW_VSCROLLBAR_ID.to_string()));
        children.push(Box::new(vbar));

        let mut hbar = ScrollBar::new(false, 1);
        hbar.set_style_id(Some(SCROLL_VIEW_HSCROLLBAR_ID.to_string()));
        children.push(Box::new(hbar));

        let mut corner = ScrollBarCorner::new();
        corner.set_style_id(Some(SCROLL_VIEW_SCROLLBAR_CORNER_ID.to_string()));
        children.push(Box::new(corner));

        children
    }

    fn focusable(&self) -> bool {
        true
    }

    fn set_hovered(&mut self, hovered: bool) {
        if !hovered {
            self.hover_v_thumb = false;
            self.hover_v_track = false;
            self.hover_h_thumb = false;
            self.hover_h_track = false;
        }
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let viewport_height = self.height.unwrap_or_else(|| options.size.1.max(1));
        self.widget_width.store(width, Ordering::Relaxed);
        self.widget_height.store(viewport_height, Ordering::Relaxed);

        // Resolve CSS scrollbar config once for this render pass.
        let sb = self.resolve_scrollbar_css();

        if self.child_extracted {
            // Do NOT overwrite content_height/content_width — preserve values
            // set by the tree layout system so scrollbars reflect real content.

            let allow_scrollbars_h = !matches!(sb.visibility, ScrollbarVisibility::Hidden)
                && !matches!(sb.overflow_x, crate::style::Overflow::Hidden);
            let allow_scrollbars_v = !matches!(sb.visibility, ScrollbarVisibility::Hidden)
                && !matches!(sb.overflow_y, crate::style::Overflow::Hidden);

            let content_h = self.content_height.load(Ordering::Relaxed);
            let content_w = self.content_width.load(Ordering::Relaxed);

            // Iterative scrollbar resolution using CSS-resolved sizes.
            let v_scrollbar_size = sb.v_size;
            let h_scrollbar_size = sb.h_size;
            let force_gutter = matches!(sb.gutter, ScrollbarGutter::Stable);
            let force_visible = matches!(sb.visibility, ScrollbarVisibility::Visible);
            let mut show_v = false;
            let mut show_h = false;
            let mut content_viewport_w = width;
            let mut content_viewport_h = viewport_height;
            for _ in 0..3 {
                let reserve_v = show_v || force_gutter;
                let reserve_h = show_h || (force_gutter && allow_scrollbars_h);
                let vp_w = width
                    .saturating_sub(if reserve_v {
                        v_scrollbar_size.min(width.saturating_sub(1))
                    } else {
                        0
                    })
                    .max(1);
                let vp_h = viewport_height
                    .saturating_sub(if reserve_h { h_scrollbar_size } else { 0 })
                    .max(1);
                let next_show_v = allow_scrollbars_v && (content_h > vp_h || force_visible);
                let next_show_h = allow_scrollbars_h && (content_w > vp_w || force_visible);
                content_viewport_w = vp_w;
                content_viewport_h = vp_h;
                if next_show_v == show_v && next_show_h == show_h {
                    break;
                }
                show_v = next_show_v;
                show_h = next_show_h;
            }

            // Store reduced viewport dimensions.
            self.viewport_height
                .store(content_viewport_h, Ordering::Relaxed);
            self.viewport_width
                .store(content_viewport_w, Ordering::Relaxed);

            // Background fill for viewport area.
            let mut slice: Vec<Vec<Segment>> = (0..content_viewport_h)
                .map(|_| vec![Segment::new(" ".repeat(content_viewport_w))])
                .collect();

            let track_style = sb.track_style;
            let track_hover_style = sb.track_hover_style;
            let track_active_style = sb.track_active_style;
            let thumb_style = sb.thumb_style;
            let thumb_hover_style = sb.thumb_hover_style;
            let thumb_active_style = sb.thumb_active_style;
            let corner_style = sb.corner_style;
            // Tree runtime uses dedicated scrollbar child widgets.
            // Keep these values referenced here to avoid unused warnings; the
            // pre-extraction widget-local render path below still performs
            // inline scrollbar painting.
            let _ = (
                track_style,
                track_hover_style,
                track_active_style,
                thumb_style,
                thumb_hover_style,
                thumb_active_style,
                corner_style,
                show_v,
                show_h,
                content_h,
                content_w,
            );

            slice = Segment::set_shape(&slice, width, Some(viewport_height), None, false);
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
        if std::env::var("TEXTUAL_DEBUG_LAYOUT_FILE").is_ok() {
            debug_layout(&format!(
                "[scroll] id={} viewport=({}, {}) offset=({}, {})",
                0u64, width, viewport_height, self.offset_x, self.offset_y
            ));
        }
        // Use resolved CSS scrollbar config (already computed above).
        let allow_scrollbars_h = !matches!(sb.visibility, ScrollbarVisibility::Hidden)
            && !matches!(sb.overflow_x, crate::style::Overflow::Hidden);
        let allow_scrollbars_v = !matches!(sb.visibility, ScrollbarVisibility::Hidden)
            && !matches!(sb.overflow_y, crate::style::Overflow::Hidden);

        let constraints = self.child.layout_constraints();
        let v_scrollbar_size = sb.v_size;
        let h_scrollbar_size = sb.h_size;
        let force_gutter = matches!(sb.gutter, ScrollbarGutter::Stable);
        let force_visible = matches!(sb.visibility, ScrollbarVisibility::Visible);
        let mut show_v = false;
        let mut show_h = false;
        let mut content_viewport_w = width;
        let mut content_viewport_h = viewport_height;
        let mut lines: Vec<Vec<Segment>> = Vec::new();
        let mut content_width = width;
        let mut content_height = viewport_height;

        for _ in 0..3 {
            let reserve_v = show_v || force_gutter;
            let reserve_h = show_h || (force_gutter && allow_scrollbars_h);
            let viewport_w = width
                .saturating_sub(if reserve_v {
                    v_scrollbar_size.min(width.saturating_sub(1))
                } else {
                    0
                })
                .max(1);
            let viewport_h = viewport_height
                .saturating_sub(if reserve_h { h_scrollbar_size } else { 0 })
                .max(1);

            let target_height = self
                .child
                .layout_height()
                .unwrap_or_else(|| viewport_h.saturating_add(viewport_h).max(1));
            let target_width = self
                .child
                .content_width()
                .unwrap_or(viewport_w)
                .max(viewport_w);
            let render_width = clamp_with_constraints(
                target_width,
                constraints.min_width,
                constraints.max_width,
                target_width,
            )
            .max(viewport_w);
            if std::env::var("TEXTUAL_DEBUG_LAYOUT_FILE").is_ok() {
                debug_layout(&format!(
                    "[scroll] id={} child render_width={} constraints=({:?},{:?})",
                    0u64, render_width, constraints.min_width, constraints.max_width
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
            let mut candidate =
                Segment::split_and_crop_lines(segments, render_width, None, true, false);
            let fixed_height = self.child.layout_height();
            if let Some(height) = fixed_height {
                candidate =
                    Segment::set_shape(&candidate, render_width, Some(height.max(1)), None, false);
            }
            candidate = pad_lines_to_width(candidate, render_width);

            let candidate_height = if fixed_height.is_some() {
                candidate.len().max(1)
            } else {
                Self::effective_content_height(&candidate)
            };
            let candidate_width = candidate
                .iter()
                .map(|line| Segment::get_line_length(line))
                .max()
                .unwrap_or(viewport_w)
                .max(viewport_w);
            let next_show_v =
                allow_scrollbars_v && (candidate_height > viewport_h || force_visible);
            let next_show_h = allow_scrollbars_h && (candidate_width > viewport_w || force_visible);

            lines = candidate;
            content_width = candidate_width;
            content_height = candidate_height;
            content_viewport_w = viewport_w;
            content_viewport_h = viewport_h;

            if next_show_v == show_v && next_show_h == show_h {
                break;
            }
            show_v = next_show_v;
            show_h = next_show_h;
        }

        self.viewport_height
            .store(content_viewport_h, Ordering::Relaxed);
        self.viewport_width
            .store(content_viewport_w, Ordering::Relaxed);
        self.content_height.store(content_height, Ordering::Relaxed);
        self.content_width.store(content_width, Ordering::Relaxed);

        let max_offset = content_height.saturating_sub(content_viewport_h);
        let offset = self.render_offset_y.clamp(0.0, max_offset as f32).round() as usize;
        let max_offset_x = content_width.saturating_sub(content_viewport_w);
        let offset_x = self.render_offset_x.clamp(0.0, max_offset_x as f32).round() as usize;
        let start = offset.min(lines.len());
        let end = (start + content_viewport_h).min(lines.len());
        let mut slice = lines[start..end]
            .to_vec()
            .into_iter()
            .map(|line| {
                let cropped = crop_line_horizontal(&line, offset_x, content_viewport_w);
                adjust_line_length_no_bg(&cropped, content_viewport_w)
            })
            .collect::<Vec<_>>();
        slice = Segment::set_shape(
            &slice,
            content_viewport_w,
            Some(content_viewport_h),
            None,
            false,
        );

        let track_style = sb.track_style;
        let track_hover_style = sb.track_hover_style;
        let track_active_style = sb.track_active_style;
        let thumb_style = sb.thumb_style;
        let thumb_hover_style = sb.thumb_hover_style;
        let thumb_active_style = sb.thumb_active_style;
        let corner_style = sb.corner_style;
        let v_scrollbar_size = if show_v {
            width.saturating_sub(content_viewport_w)
        } else {
            0
        };
        if show_v {
            let track_len = content_viewport_h.max(1);
            let (thumb_start, thumb_len) =
                Self::line_scrollbar_thumb(track_len, content_height, content_viewport_h, offset);
            let mut thumb_drawn = false;
            for (row, line) in slice.iter_mut().enumerate() {
                let in_track = row < track_len;
                let style = if in_track && row >= thumb_start && row < thumb_start + thumb_len {
                    if self.drag_v.is_some() {
                        thumb_active_style
                    } else if self.hover_v_thumb {
                        thumb_hover_style
                    } else {
                        thumb_style
                    }
                } else if self.drag_v.is_some() {
                    track_active_style
                } else if self.hover_v_track {
                    track_hover_style
                } else {
                    track_style
                };
                line.extend(Self::blank_run(v_scrollbar_size.max(1), style));
                thumb_drawn |= in_track && row >= thumb_start && row < thumb_start + thumb_len;
            }
            if !thumb_drawn && !slice.is_empty() {
                let row = track_len.saturating_sub(1).min(slice.len() - 1);
                let line = &mut slice[row];
                for _ in 0..v_scrollbar_size.max(1) {
                    if !line.is_empty() {
                        line.pop();
                    }
                }
                let active_style = if self.drag_v.is_some() {
                    thumb_active_style
                } else if self.hover_v_thumb {
                    thumb_hover_style
                } else {
                    thumb_style
                };
                line.extend(Self::blank_run(v_scrollbar_size.max(1), active_style));
            }
        }
        if show_h {
            let (thumb_start, thumb_len) = Self::line_scrollbar_thumb(
                content_viewport_w,
                content_width,
                content_viewport_w,
                offset_x,
            );
            let mut row = Vec::new();
            for col in 0..content_viewport_w {
                let style = if col >= thumb_start && col < thumb_start + thumb_len {
                    if self.drag_h.is_some() {
                        thumb_active_style
                    } else if self.hover_h_thumb {
                        thumb_hover_style
                    } else {
                        thumb_style
                    }
                } else if self.drag_h.is_some() {
                    track_active_style
                } else if self.hover_h_track {
                    track_hover_style
                } else {
                    track_style
                };
                row.extend(Self::blank_run(1, style));
            }
            if show_v {
                row.extend(Self::blank_run(v_scrollbar_size.max(1), corner_style));
            }
            slice.push(row);
        }

        slice = Segment::set_shape(&slice, width, Some(viewport_height), None, false);
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
        Some(&self.seed.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.seed.styles)
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        let seed = std::mem::take(&mut self.seed);
        self.seed.styles = seed.styles.clone();
        seed
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
        if !self.child_extracted {
            self.child.on_mount();
        }
    }

    fn on_unmount(&mut self) {
        if !self.child_extracted {
            self.child.on_unmount();
        }
    }

    fn on_tick(&mut self, tick: u64) {
        if !self.child_extracted {
            self.child.on_tick(tick);
        }
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        if !self.child_extracted {
            self.child.on_resize(width, height);
        }
    }

    fn set_virtual_content_size(&mut self, width: usize, height: usize) {
        ScrollView::set_virtual_content_size(self, width, height);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        if !self.child_extracted {
            self.child.on_event_capture(event, ctx);
        }
    }

    fn action_namespace(&self) -> &str {
        "scroll-view"
    }

    fn bindings(&self) -> Vec<BindingDecl> {
        vec![
            BindingDecl::new("up", "scroll_up", "Scroll up").hidden(),
            BindingDecl::new("down", "scroll_down", "Scroll down").hidden(),
            BindingDecl::new("pageup", "page_up", "Page up").hidden(),
            BindingDecl::new("pagedown", "page_down", "Page down").hidden(),
            BindingDecl::new("home", "scroll_home", "Scroll to top").hidden(),
            BindingDecl::new("end", "scroll_end", "Scroll to bottom").hidden(),
            BindingDecl::new("left", "scroll_left", "Scroll left").hidden(),
            BindingDecl::new("right", "scroll_right", "Scroll right").hidden(),
        ]
    }

    fn execute_action(&mut self, action: &ParsedAction, ctx: &mut EventCtx) -> bool {
        match action.name.as_str() {
            "scroll_up" => {
                let before = self.offset_y;
                self.scroll_by(-(self.scroll_step as i32));
                self.request_offset_y_animation(before, self.offset_y, ctx);
                ctx.set_handled();
                true
            }
            "scroll_down" => {
                let before = self.offset_y;
                self.scroll_by(self.scroll_step as i32);
                self.request_offset_y_animation(before, self.offset_y, ctx);
                ctx.set_handled();
                true
            }
            "page_up" => {
                let before = self.offset_y;
                let viewport_h = self.viewport_height.load(Ordering::Relaxed);
                self.scroll_by(-(viewport_h as i32));
                self.request_offset_y_animation(before, self.offset_y, ctx);
                ctx.set_handled();
                true
            }
            "page_down" => {
                let before = self.offset_y;
                let viewport_h = self.viewport_height.load(Ordering::Relaxed);
                self.scroll_by(viewport_h as i32);
                self.request_offset_y_animation(before, self.offset_y, ctx);
                ctx.set_handled();
                true
            }
            "scroll_home" => {
                let before_x = self.offset_x;
                let before_y = self.offset_y;
                self.scroll_to(0);
                self.scroll_to_x(0);
                self.request_offset_x_animation(before_x, self.offset_x, ctx);
                self.request_offset_y_animation(before_y, self.offset_y, ctx);
                ctx.set_handled();
                true
            }
            "scroll_end" => {
                let before_y = self.offset_y;
                self.scroll_to(self.max_offset());
                self.request_offset_y_animation(before_y, self.offset_y, ctx);
                ctx.set_handled();
                true
            }
            "scroll_left" => {
                let before = self.offset_x;
                self.scroll_by_x(-(self.scroll_step as i32));
                self.request_offset_x_animation(before, self.offset_x, ctx);
                ctx.set_handled();
                true
            }
            "scroll_right" => {
                let before = self.offset_x;
                self.scroll_by_x(self.scroll_step as i32);
                self.request_offset_x_animation(before, self.offset_x, ctx);
                ctx.set_handled();
                true
            }
            _ => false,
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.sync_child_layout();
        if let Event::AnimationValue(AnimationValueEvent {
            target,
            attribute,
            value,
            done,
        }) = event
        {
            if *target == self.node_id() {
                if attribute == Self::OFFSET_Y_ATTR {
                    if self.drag_v.is_none() {
                        self.render_offset_y = if *done { self.offset_y as f32 } else { *value };
                        ctx.request_repaint();
                    }
                    ctx.set_handled();
                    return;
                }
                if attribute == Self::OFFSET_X_ATTR {
                    if self.drag_h.is_none() {
                        self.render_offset_x = if *done { self.offset_x as f32 } else { *value };
                        ctx.request_repaint();
                    }
                    ctx.set_handled();
                    return;
                }
            }
        }
        if let Event::MouseDown(mouse) = event {
            if mouse.target == self.node_id() {
                let hover_changed = self.update_scrollbar_hover_state(mouse.x, mouse.y);
                let widget_width = self.widget_width.load(Ordering::Relaxed).max(1);
                let widget_height = self.widget_height.load(Ordering::Relaxed).max(1);
                let viewport_w = self.viewport_width.load(Ordering::Relaxed).max(1);
                let viewport_h = self.viewport_height.load(Ordering::Relaxed).max(1);
                let content_w = self.content_width.load(Ordering::Relaxed);
                let content_h = self.content_height.load(Ordering::Relaxed);
                let show_v = content_h > viewport_h;
                let show_h = content_w > viewport_w;
                let v_scrollbar_size = widget_width.saturating_sub(viewport_w).max(1);
                let h_scrollbar_size = widget_height.saturating_sub(viewport_h).max(1);
                let local_x = mouse.x as usize;
                let local_y = mouse.y as usize;

                if show_v
                    && local_x >= widget_width.saturating_sub(v_scrollbar_size)
                    && local_y < viewport_h
                {
                    let (thumb_start, thumb_len) = Self::line_scrollbar_thumb(
                        viewport_h,
                        content_h,
                        viewport_h,
                        self.offset_y,
                    );
                    if local_y >= thumb_start && local_y < thumb_start.saturating_add(thumb_len) {
                        self.drag_v = Some(local_y.saturating_sub(thumb_start));
                        self.drag_h = None;
                        if hover_changed {
                            ctx.request_repaint();
                        }
                        ctx.set_handled();
                        return;
                    }
                    let before = self.offset_y;
                    if local_y < thumb_start {
                        self.scroll_by(-(viewport_h as i32));
                    } else if local_y >= thumb_start.saturating_add(thumb_len) {
                        self.scroll_by(viewport_h as i32);
                    }
                    if self.offset_y != before {
                        self.request_offset_y_animation(before, self.offset_y, ctx);
                        ctx.request_repaint();
                    }
                    ctx.set_handled();
                    return;
                }

                if show_h
                    && local_y >= widget_height.saturating_sub(h_scrollbar_size)
                    && local_x < viewport_w
                {
                    let (thumb_start, thumb_len) = Self::line_scrollbar_thumb(
                        viewport_w,
                        content_w,
                        viewport_w,
                        self.offset_x,
                    );
                    if local_x >= thumb_start && local_x < thumb_start.saturating_add(thumb_len) {
                        self.drag_h = Some(local_x.saturating_sub(thumb_start));
                        self.drag_v = None;
                        if hover_changed {
                            ctx.request_repaint();
                        }
                        ctx.set_handled();
                        return;
                    }
                    let before = self.offset_x;
                    if local_x < thumb_start {
                        self.scroll_by_x(-(viewport_w as i32));
                    } else if local_x >= thumb_start.saturating_add(thumb_len) {
                        self.scroll_by_x(viewport_w as i32);
                    }
                    if self.offset_x != before {
                        self.request_offset_x_animation(before, self.offset_x, ctx);
                        ctx.request_repaint();
                    }
                    ctx.set_handled();
                    return;
                }
            }
        }
        if matches!(event, Event::MouseUp(_) | Event::AppFocus(false)) {
            let was_dragging = self.drag_v.take().is_some() || self.drag_h.take().is_some();
            if was_dragging {
                ctx.set_handled();
            }
        }

        if !self.child_extracted {
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
            let mut child_ctx = EventCtx::default();
            if let Some(child_event) = child_event.as_ref() {
                self.child.on_event(child_event, &mut child_ctx);
            } else {
                self.child.on_event(event, &mut child_ctx);
            }
            let child_handled = child_ctx.handled();
            ctx.merge_from(child_ctx);
            if child_handled {
                return;
            }
        }
        if let Event::Action(action) = event {
            match action {
                Action::ScrollHome => {
                    let before_x = self.offset_x;
                    let before_y = self.offset_y;
                    self.scroll_to(0);
                    self.scroll_to_x(0);
                    self.request_offset_x_animation(before_x, self.offset_x, ctx);
                    self.request_offset_y_animation(before_y, self.offset_y, ctx);
                    ctx.set_handled();
                    return;
                }
                Action::ScrollEnd => {
                    let before_x = self.offset_x;
                    let before_y = self.offset_y;
                    self.scroll_to(self.max_offset());
                    self.scroll_to_x(self.max_offset_x());
                    self.request_offset_x_animation(before_x, self.offset_x, ctx);
                    self.request_offset_y_animation(before_y, self.offset_y, ctx);
                    debug_input(&format!(
                        "[scrollview] action=ScrollEnd before=({}, {}) after=({}, {}) max=({}, {})",
                        before_x,
                        before_y,
                        self.offset_x,
                        self.offset_y,
                        self.max_offset_x(),
                        self.max_offset()
                    ));
                    ctx.set_handled();
                    return;
                }
                Action::ScrollUp => {
                    let before = self.offset_y;
                    self.scroll_by(-(self.scroll_step as i32));
                    self.request_offset_y_animation(before, self.offset_y, ctx);
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
                    self.request_offset_y_animation(before, self.offset_y, ctx);
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
                    self.request_offset_y_animation(before, self.offset_y, ctx);
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
                    self.request_offset_y_animation(before, self.offset_y, ctx);
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
                    self.request_offset_x_animation(before, self.offset_x, ctx);
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
                    self.request_offset_x_animation(before, self.offset_x, ctx);
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
                    self.request_offset_x_animation(before, self.offset_x, ctx);
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
                    self.request_offset_x_animation(before, self.offset_x, ctx);
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

    fn on_message(&mut self, msg: &MessageEvent, ctx: &mut EventCtx) {
        let Some(ScrollbarScrollTo {
            axis,
            offset,
            animate,
            scroll_duration,
        }) = msg.downcast_ref::<ScrollbarScrollTo>()
        else {
            return;
        };

        match axis {
            ScrollbarAxis::Vertical => {
                let before = self.offset_y;
                let next = Self::line_clamp_offset(
                    offset.max(0.0).round() as usize,
                    self.content_height.load(Ordering::Relaxed).max(1),
                    self.viewport_height.load(Ordering::Relaxed).max(1),
                );
                self.offset_y = next;
                if *animate {
                    self.request_offset_y_animation_with_duration(
                        before,
                        self.offset_y,
                        *scroll_duration,
                        ctx,
                    );
                } else {
                    self.render_offset_y = self.offset_y as f32;
                    ctx.request_repaint();
                }
            }
            ScrollbarAxis::Horizontal => {
                let before = self.offset_x;
                let next = Self::line_clamp_offset(
                    offset.max(0.0).round() as usize,
                    self.content_width.load(Ordering::Relaxed).max(1),
                    self.viewport_width.load(Ordering::Relaxed).max(1),
                );
                self.offset_x = next;
                if *animate {
                    self.request_offset_x_animation_with_duration(
                        before,
                        self.offset_x,
                        *scroll_duration,
                        ctx,
                    );
                } else {
                    self.render_offset_x = self.offset_x as f32;
                    ctx.request_repaint();
                }
            }
        }
        ctx.set_handled();
    }

    fn on_mouse_scroll(&mut self, delta_x: i32, delta_y: i32, ctx: &mut EventCtx) {
        // Horizontal-only scroll containers use wheel Y deltas to scroll X.
        let mut resolved_dx = delta_x;
        let mut resolved_dy = delta_y;
        let overflow_x = self.seed.styles.style.overflow_x.unwrap_or(Overflow::Auto);
        let overflow_y = self.seed.styles.style.overflow_y.unwrap_or(Overflow::Auto);
        if resolved_dx == 0
            && resolved_dy != 0
            && matches!(overflow_y, Overflow::Hidden)
            && !matches!(overflow_x, Overflow::Hidden)
        {
            resolved_dx = resolved_dy;
            resolved_dy = 0;
        }

        let before_x = self.offset_x;
        let before_y = self.offset_y;

        if resolved_dy != 0 {
            self.scroll_by(resolved_dy.saturating_mul(self.scroll_step as i32));
        }
        if resolved_dx != 0 {
            self.scroll_by_x(resolved_dx.saturating_mul(self.scroll_step_x as i32));
        }
        debug_input(&format!(
            "[scrollview] mouse dx={} dy={} before=({}, {}) after=({}, {}) max=({}, {})",
            resolved_dx,
            resolved_dy,
            before_x,
            before_y,
            self.offset_x,
            self.offset_y,
            self.max_offset_x(),
            self.max_offset()
        ));

        if self.offset_x != before_x || self.offset_y != before_y {
            // Python parity: wheel scrolling is immediate (non-animated).
            self.render_offset_x = self.offset_x as f32;
            self.render_offset_y = self.offset_y as f32;
            ctx.request_repaint();
            ctx.set_handled();
        }
    }

    fn on_mouse_move(&mut self, x: u16, y: u16) -> bool {
        self.sync_child_layout();
        let mut changed = self.update_scrollbar_hover_state(x, y);
        if let Some(grab_offset) = self.drag_v {
            let viewport_h = self.viewport_height.load(Ordering::Relaxed).max(1);
            let content_h = self.content_height.load(Ordering::Relaxed).max(1);
            if content_h > viewport_h {
                let new_offset = Self::line_drag_offset(
                    y as usize,
                    grab_offset,
                    viewport_h,
                    content_h,
                    viewport_h,
                    self.offset_y,
                );
                if new_offset != self.offset_y {
                    self.offset_y = new_offset;
                    self.render_offset_y = new_offset as f32;
                    changed = true;
                }
            }
        } else if let Some(grab_offset) = self.drag_h {
            let viewport_w = self.viewport_width.load(Ordering::Relaxed).max(1);
            let content_w = self.content_width.load(Ordering::Relaxed).max(1);
            if content_w > viewport_w {
                let new_offset = Self::line_drag_offset(
                    x as usize,
                    grab_offset,
                    viewport_w,
                    content_w,
                    viewport_w,
                    self.offset_x,
                );
                if new_offset != self.offset_x {
                    self.offset_x = new_offset;
                    self.render_offset_x = new_offset as f32;
                    changed = true;
                }
            }
        } else {
            if !self.child_extracted {
                let (child_x, child_y) = self.child_coords(x, y);
                debug_input(&format!(
                    "[hover][scrollview] x={} y={} child=({}, {})",
                    x, y, child_x, child_y
                ));
                changed |= self.child.on_mouse_move(child_x, child_y);
            }
        }
        changed
    }

    fn scroll_offset(&self) -> (usize, usize) {
        (self.offset_x, self.offset_y)
    }

    fn scroll_offset_f32(&self) -> (f32, f32) {
        let max_x = self
            .content_width
            .load(Ordering::Relaxed)
            .saturating_sub(self.viewport_width.load(Ordering::Relaxed).max(1))
            as f32;
        let max_y = self
            .content_height
            .load(Ordering::Relaxed)
            .saturating_sub(self.viewport_height.load(Ordering::Relaxed).max(1))
            as f32;
        (
            self.render_offset_x.clamp(0.0, max_x),
            self.render_offset_y.clamp(0.0, max_y),
        )
    }

    fn clips_descendants_to_content(&self) -> bool {
        true
    }

    fn scroll_viewport_size(&self) -> Option<(usize, usize)> {
        let vw = self.viewport_width.load(Ordering::Relaxed);
        let vh = self.viewport_height.load(Ordering::Relaxed);
        if vw == 0 || vh == 0 {
            None
        } else {
            Some((vw, vh))
        }
    }

    fn scroll_virtual_content_size(&self) -> Option<(usize, usize)> {
        let cw = self.content_width.load(Ordering::Relaxed);
        let ch = self.content_height.load(Ordering::Relaxed);
        if cw == 0 || ch == 0 {
            None
        } else {
            Some((cw, ch))
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::ParsedAction;
    use crate::event::{AnimationValueEvent, EventCtx};
    use crate::node_id::NodeId;
    use crate::prelude::Label;
    use crate::runtime::dispatch_ctx::set_dispatch_recipient;

    fn make_node_id() -> NodeId {
        let mut sm: slotmap::SlotMap<NodeId, ()> = slotmap::SlotMap::new();
        sm.insert(())
    }

    #[test]
    fn bindings_are_declared() {
        let sv = ScrollView::new(Label::new("content"));
        let bindings = sv.bindings();
        assert!(!bindings.is_empty());
        assert!(bindings.iter().any(|b| b.action == "scroll_up"));
        assert!(bindings.iter().any(|b| b.action == "scroll_down"));
        assert!(bindings.iter().any(|b| b.action == "scroll_home"));
        assert!(bindings.iter().any(|b| b.action == "scroll_end"));
    }

    #[test]
    fn execute_action_handles_scroll_down() {
        let mut sv = ScrollView::new(Label::new("content"));
        let mut ctx = EventCtx::default();
        let action = ParsedAction {
            namespace: None,
            name: "scroll_down".to_string(),
            arguments: vec![],
        };
        assert!(sv.execute_action(&action, &mut ctx));
        assert!(ctx.handled());
    }

    #[test]
    fn animation_value_matches_real_node_id() {
        let id = make_node_id();
        let _guard = set_dispatch_recipient(id, crate::widgets::NodeState::default());

        let mut sv = ScrollView::new(Label::new("content"));
        let mut ctx = EventCtx::default();

        // Event targeting our real NodeId should be handled.
        let event = Event::AnimationValue(AnimationValueEvent {
            target: id,
            attribute: ScrollView::OFFSET_Y_ATTR.to_string(),
            value: 2.0,
            done: false,
        });
        sv.on_event(&event, &mut ctx);
        assert!(
            ctx.handled(),
            "AnimationValueEvent targeting real NodeId must be handled"
        );
    }

    #[test]
    fn animation_value_ignores_foreign_node_id() {
        let mut sm: slotmap::SlotMap<NodeId, ()> = slotmap::SlotMap::new();
        let id = sm.insert(());
        let other = sm.insert(());
        assert_ne!(id, other);
        let _guard = set_dispatch_recipient(id, crate::widgets::NodeState::default());

        let mut sv = ScrollView::new(Label::new("content"));
        let mut ctx = EventCtx::default();

        // Event targeting a different NodeId should NOT be handled by this widget.
        let event = Event::AnimationValue(AnimationValueEvent {
            target: other,
            attribute: ScrollView::OFFSET_Y_ATTR.to_string(),
            value: 2.0,
            done: false,
        });
        sv.on_event(&event, &mut ctx);
        assert!(
            !ctx.handled(),
            "AnimationValueEvent targeting a foreign NodeId must not be handled"
        );
    }

    #[test]
    fn animation_tick_in_tree_mode_requests_repaint_not_layout() {
        let id = make_node_id();
        let _guard = set_dispatch_recipient(id, crate::widgets::NodeState::default());

        let mut sv = ScrollView::new(Label::new("content"));
        let _ = sv.take_composed_children();
        assert!(sv.child_extracted);
        let mut ctx = EventCtx::default();

        let event = Event::AnimationValue(AnimationValueEvent {
            target: id,
            attribute: ScrollView::OFFSET_Y_ATTR.to_string(),
            value: 3.5,
            done: false,
        });
        sv.on_event(&event, &mut ctx);

        assert!(ctx.handled());
        assert!(ctx.repaint_requested());
        assert!(
            !ctx.invalidation().layout,
            "animation ticks should avoid forcing full layout"
        );
    }

    #[test]
    fn wheel_scroll_is_immediate_and_does_not_enqueue_animation() {
        let mut sv = ScrollView::new(Label::new("content"));
        sv.content_height.store(200, Ordering::Relaxed);
        sv.viewport_height.store(20, Ordering::Relaxed);
        let before = sv.offset_y;
        let mut ctx = EventCtx::default();

        sv.on_mouse_scroll(0, 1, &mut ctx);

        assert!(ctx.handled());
        assert!(ctx.repaint_requested());
        assert!(
            !ctx.invalidation().layout,
            "wheel scrolling should not trigger layout invalidation"
        );
        assert!(ctx.take_animation_requests().is_empty());
        assert!(sv.offset_y > before);
        assert_eq!(sv.render_offset_y, sv.offset_y as f32);
    }

    #[test]
    fn mouse_down_skips_scrollbar_for_foreign_node_id() {
        let mut sm: slotmap::SlotMap<NodeId, ()> = slotmap::SlotMap::new();
        let id = sm.insert(());
        let other = sm.insert(());
        assert_ne!(id, other);
        let _guard = set_dispatch_recipient(id, crate::widgets::NodeState::default());

        let mut sv = ScrollView::new(Label::new("content"));
        let mut ctx = EventCtx::default();

        // MouseDown targeting a foreign NodeId should NOT enter the scrollbar
        // hit-test branch (the `mouse.target == self.node_id()` guard must
        // reject it), so scrollbar-specific handling is skipped.
        let event = Event::MouseDown(crate::event::MouseDownEvent {
            target: other,
            screen_x: 0,
            screen_y: 0,
            x: 0,
            y: 0,
        });
        sv.on_event(&event, &mut ctx);
        // The scrollbar branch sets handled+returns on any scrollbar hit.
        // Since the target doesn't match, the branch is skipped entirely
        // and the event falls through to child dispatch (Label, which
        // ignores mouse events). So handled must be false.
        assert!(
            !ctx.handled(),
            "MouseDown with foreign NodeId must skip scrollbar logic"
        );
    }

    #[test]
    fn tree_mode_render_produces_chrome_not_blank() {
        let mut sv = ScrollView::new(Label::new("content"));
        // Extract child to enter tree mode.
        let children = sv.take_composed_children();
        // Tree mode exposes content + dedicated scrollbar children.
        assert_eq!(children.len(), 4);
        assert!(sv.child_extracted);

        // Simulate content larger than viewport so scrollbar appears.
        sv.content_height.store(100, Ordering::Relaxed);
        sv.content_width.store(10, Ordering::Relaxed);

        let console = Console::default();
        let mut opts = ConsoleOptions::default();
        opts.size = (20, 10);
        opts.max_width = 20;
        opts.max_height = 10;

        let segments = Widget::render(&sv, &console, &opts);
        let lines = Segment::split_and_crop_lines(segments, 20, None, true, false);
        assert_eq!(
            lines.len(),
            10,
            "tree-mode render must produce viewport-height lines"
        );
        // At least one line should have more than one segment (scrollbar chrome).
        let has_styled = lines.iter().any(|line| line.len() > 1);
        assert!(
            has_styled,
            "tree-mode render should include scrollbar chrome"
        );
    }

    #[test]
    fn tree_mode_scroll_offset_and_clip() {
        let mut sv = ScrollView::new(Label::new("content"));
        sv.offset_x = 5;
        sv.offset_y = 10;
        let _ = sv.take_composed_children();
        assert!(sv.child_extracted);

        assert_eq!(sv.scroll_offset(), (5, 10));
        assert!(sv.clips_descendants_to_content());
    }

    #[test]
    fn tree_mode_scroll_actions_still_work() {
        let mut sv = ScrollView::new(Label::new("content"));
        let _ = sv.take_composed_children();
        assert!(sv.child_extracted);

        // Set content larger than viewport.
        sv.content_height.store(100, Ordering::Relaxed);
        sv.viewport_height.store(10, Ordering::Relaxed);

        let mut ctx = EventCtx::default();
        let event = Event::Action(Action::ScrollDown);
        sv.on_event(&event, &mut ctx);
        assert!(
            ctx.handled(),
            "ScrollDown action should be handled in tree mode"
        );
        assert!(sv.offset_y > 0, "offset_y should increase after ScrollDown");

        let mut ctx2 = EventCtx::default();
        let event2 = Event::Action(Action::ScrollHome);
        sv.on_event(&event2, &mut ctx2);
        assert!(ctx2.handled(), "ScrollHome should be handled in tree mode");
        assert_eq!(sv.offset_y, 0, "offset_y should be 0 after ScrollHome");
    }

    #[test]
    fn tree_mode_preserves_content_dimensions() {
        let mut sv = ScrollView::new(Label::new("content"));
        // Pre-set content dimensions as if tree layout had done so.
        sv.content_height.store(200, Ordering::Relaxed);
        sv.content_width.store(50, Ordering::Relaxed);

        let _ = sv.take_composed_children();

        let console = Console::default();
        let mut opts = ConsoleOptions::default();
        opts.size = (20, 10);
        opts.max_width = 20;
        opts.max_height = 10;

        let _ = Widget::render(&sv, &console, &opts);
        // render() must NOT overwrite content dimensions to viewport values.
        assert_eq!(
            sv.content_height.load(Ordering::Relaxed),
            200,
            "content_height must be preserved in tree-mode render"
        );
        assert_eq!(
            sv.content_width.load(Ordering::Relaxed),
            50,
            "content_width must be preserved in tree-mode render"
        );
    }

    #[test]
    fn line_drag_offset_is_stable_for_same_pointer() {
        let pointer = 13;
        let grab_offset = 4;
        let track_len = 32;
        let content_len = 50;
        let viewport_len = 32;

        let a = ScrollView::line_drag_offset(
            pointer,
            grab_offset,
            track_len,
            content_len,
            viewport_len,
            0,
        );
        let b = ScrollView::line_drag_offset(
            pointer,
            grab_offset,
            track_len,
            content_len,
            viewport_len,
            6,
        );
        let c = ScrollView::line_drag_offset(
            pointer,
            grab_offset,
            track_len,
            content_len,
            viewport_len,
            18,
        );

        assert_eq!(a, b, "drag mapping must not depend on current_offset");
        assert_eq!(b, c, "drag mapping must be stable for same pointer");
    }

    #[test]
    fn line_drag_offset_is_monotonic() {
        let grab_offset = 0;
        let track_len = 32;
        let content_len = 50;
        let viewport_len = 32;

        let mut previous = 0usize;
        for pointer in 0..track_len {
            let offset = ScrollView::line_drag_offset(
                pointer,
                grab_offset,
                track_len,
                content_len,
                viewport_len,
                0,
            );
            assert!(
                offset >= previous,
                "offset must not decrease as pointer increases: pointer={} prev={} now={}",
                pointer,
                previous,
                offset
            );
            previous = offset;
        }
    }

    #[test]
    fn scroll_home_resets_offset_to_zero() {
        use crate::widgets::Label;
        let mut view = ScrollView::new(Label::new(""));
        // Manually set offset_y to bypass clamping (no layout available in unit tests).
        view.offset_y = 42;
        view.render_offset_y = 42.0;
        view.scroll_home();
        assert_eq!(view.offset_y, 0);
        assert_eq!(view.render_offset_y, 0.0);
    }

    #[test]
    fn scroll_end_does_not_panic_without_layout() {
        use crate::widgets::Label;
        let mut view = ScrollView::new(Label::new(""));
        // Without real layout content_height is 0, so max_offset() == 0 too.
        // Just verify scroll_end() doesn't panic and leaves a clamped result.
        view.scroll_end();
        assert_eq!(view.offset_y, 0);
    }

    #[test]
    fn scroll_offset_f32_uses_render_offsets_for_animation() {
        use crate::widgets::Label;
        let mut view = ScrollView::new(Label::new(""));
        view.content_width.store(200, Ordering::Relaxed);
        view.viewport_width.store(40, Ordering::Relaxed);
        view.content_height.store(300, Ordering::Relaxed);
        view.viewport_height.store(30, Ordering::Relaxed);
        view.offset_x = 80;
        view.offset_y = 120;
        view.render_offset_x = 23.5;
        view.render_offset_y = 77.25;

        let (x, y) = view.scroll_offset_f32();
        assert_eq!(x, 23.5);
        assert_eq!(y, 77.25);
    }

    #[test]
    fn scroll_virtual_content_size_reports_runtime_metrics() {
        use crate::widgets::Label;
        let view = ScrollView::new(Label::new(""));
        assert_eq!(view.scroll_virtual_content_size(), None);
        view.content_width.store(120, Ordering::Relaxed);
        view.content_height.store(80, Ordering::Relaxed);
        assert_eq!(view.scroll_virtual_content_size(), Some((120, 80)));
    }
}
