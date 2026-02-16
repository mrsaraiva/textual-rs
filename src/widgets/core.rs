use rich_rs::{Console, ConsoleOptions, MetaValue, Segments, StyleMeta};
use std::any::Any;

use crate::action::{ActionDecl, ParsedAction};
use crate::compose::ComposeResult;
use crate::debug::DebugLayout;
use crate::event::{Action, BindingHint, Event, EventCtx};
use crate::message::MessageEvent;
use crate::node_id::{self, NodeId};
use crate::reactive::ReactiveWidget;
use crate::style::{Color, Position, Style};

use super::helpers;

const META_WIDGET_ID: &str = "textual:widget_id";

// ── Style invalidation classification ──────────────────────────────

/// Classification of style property changes for invalidation.
///
/// Used to decide whether a style mutation requires a full relayout
/// (layout-affecting properties) or just a repaint (visual-only properties).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyleChangeKind {
    /// No properties changed.
    None,
    /// Only visual properties changed (color, bg, border appearance, opacity,
    /// tint, text formatting, pointer, layer ordering, transitions).
    /// Triggers repaint without relayout.
    Visual,
    /// Layout-affecting properties changed (display, visibility, overflow,
    /// layout, dock, width/height/min/max, margin, padding, alignment,
    /// offset, constrain, grid configuration).
    /// Triggers full relayout + repaint.
    Layout,
}

/// Compare two styles and classify the kind of change for invalidation.
///
/// **Layout-affecting properties:** display, visibility, overflow, layout, dock,
/// width, height, min_width, max_width, min_height, max_height, margin,
/// padding, align, content_align, offset, constrain, grid_*.
///
/// **Visual-only properties:** fg, bg, opacity, bold, dim, italic, underline,
/// reverse, border edges, tint, text_align, pointer, layer, layers,
/// transition parameters.
pub fn classify_style_change(old: &Style, new: &Style) -> StyleChangeKind {
    if old == new {
        return StyleChangeKind::None;
    }

    // Check layout-affecting fields first.
    if old.display != new.display
        || old.visibility != new.visibility
        || old.overflow != new.overflow
        || old.layout != new.layout
        || old.dock != new.dock
        || old.width != new.width
        || old.height != new.height
        || old.min_width != new.min_width
        || old.max_width != new.max_width
        || old.min_height != new.min_height
        || old.max_height != new.max_height
        || old.margin != new.margin
        || old.padding != new.padding
        || old.align != new.align
        || old.content_align != new.content_align
        || old.offset != new.offset
        || old.constrain != new.constrain
        || old.grid_size_columns != new.grid_size_columns
        || old.grid_size_rows != new.grid_size_rows
        || old.grid_columns != new.grid_columns
        || old.grid_rows != new.grid_rows
        || old.grid_gutter_horizontal != new.grid_gutter_horizontal
        || old.grid_gutter_vertical != new.grid_gutter_vertical
        || old.border != new.border
        || old.border_top != new.border_top
        || old.border_right != new.border_right
        || old.border_bottom != new.border_bottom
        || old.border_left != new.border_left
    {
        return StyleChangeKind::Layout;
    }

    // Styles differ but no layout-affecting field changed → visual only.
    StyleChangeKind::Visual
}

/// A declarative key-binding declaration, analogous to Python Textual's `Binding`.
///
/// Widgets return these from [`Widget::bindings()`] to declare key→action mappings.
/// The runtime collects bindings along the focused widget chain and dispatches
/// matching actions before falling through to raw `on_event` handling.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BindingDecl {
    /// Key specification (e.g. `"enter"`, `"j,down"`). Comma-separated alternatives.
    pub key: String,
    /// Action string to dispatch (e.g. `"select_cursor"`, `"app.quit"`).
    pub action: String,
    /// Human-readable description shown in footer/help.
    pub description: String,
    /// Whether this binding is displayed in footer/help panels.
    pub show: bool,
    /// Priority bindings are checked before normal bindings across the whole chain.
    pub priority: bool,
}

impl BindingDecl {
    pub fn new(key: &str, action: &str, description: &str) -> Self {
        Self {
            key: key.to_string(),
            action: action.to_string(),
            description: description.to_string(),
            show: true,
            priority: false,
        }
    }

    /// Mark this binding as hidden (not shown in footer/help).
    pub fn hidden(mut self) -> Self {
        self.show = false;
        self
    }

    /// Mark this binding as priority (dispatched before normal bindings).
    pub fn priority(mut self) -> Self {
        self.priority = true;
        self
    }
}

/// Behavior-only widget trait.
///
/// Identity (NodeId) comes from the arena-based `WidgetTree`, not from the
/// widget itself. Structural concerns (parent/child links, CSS classes, display
/// state) also live on the tree, not here.
///
/// The `render_styled_dyn_obj` method receives a `NodeId` from the runtime so
/// it can tag rendered segments with the correct arena identity. The convenience
/// wrappers `render_styled` / `render_styled_with_debug` use a null sentinel
/// NodeId for backward-compatible widget-to-widget rendering during migration.
pub trait Widget: Send + Sync + Any {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments;

    /// Declare child widgets for this widget.
    ///
    /// The runtime materializes these declarations into arena nodes during
    /// mount. The default implementation returns an empty list (leaf widget).
    fn compose(&self) -> ComposeResult {
        Vec::new()
    }

    /// Move children out of this widget for tree mounting.
    ///
    /// Containers override this to drain their owned children vec.
    /// After calling, the container's children list is empty — the tree
    /// takes ownership and tree-driven rendering handles child layout.
    /// Leaf widgets return an empty vec (the default).
    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        Vec::new()
    }

    /// Return this widget's arena-assigned NodeId.
    ///
    /// During event/message dispatch and rendering, the runtime sets a
    /// thread-local dispatch context to the current node, so this returns
    /// the real tree-assigned identity. Outside dispatch (e.g. standalone
    /// tests without context setup) it returns `NodeId::default()`.
    ///
    /// Aligns with B-06 Identity Principle: widgets receive NodeId through
    /// context, they don't own it.
    fn node_id(&self) -> NodeId {
        crate::runtime::dispatch_ctx::dispatch_recipient().unwrap_or_default()
    }

    /// Render with full CSS styling, border composition, and segment tagging.
    ///
    /// `_node_id` is the arena-assigned identity used for metadata tagging so
    /// hit-test lookups remain compatible with `HitTestMap` and `NodeHitTestMap`.
    fn render_styled_dyn_obj(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        debug: Option<&DebugLayout>,
        _node_id: NodeId,
    ) -> Segments {
        // Set dispatch context so self.node_id() returns the correct arena
        // NodeId during render(). The guard restores the previous recipient
        // on drop, so nested/sibling renders don't leak context.
        let _dispatch_guard = crate::runtime::dispatch_ctx::set_dispatch_recipient(_node_id);

        // Use the arena NodeId for metadata tagging — `apply_style_to_segments`
        // checks this value, so it must match the tag used here.
        let debug_widget_label = {
            let mut label = self.style_type().to_string();
            if let Some(id) = self.style_id() {
                label.push('#');
                label.push_str(id);
            }
            for class in self.style_classes() {
                label.push('.');
                label.push_str(class);
            }
            if self.is_disabled() {
                label.push_str(":disabled");
            }
            if self.has_focus() {
                label.push_str(":focus");
            }
            if self.is_hovered() {
                label.push_str(":hover");
            }
            if self.is_active() {
                label.push_str(":active");
            }
            label
        };
        let meta = crate::css::selector_meta_generic(self);
        let resolved = crate::css::resolve_style(self, &meta);
        render_widget_with_meta(
            self,
            console,
            options,
            debug,
            _node_id,
            &meta,
            &resolved,
            &debug_widget_label,
        )
    }
    fn render_with_debug(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        _debug: &DebugLayout,
    ) -> Segments {
        self.render(console, options)
    }
    fn on_mount(&mut self) {}
    fn on_unmount(&mut self) {}
    fn on_tick(&mut self, _tick: u64) {}
    fn on_resize(&mut self, _width: u16, _height: u16) {}
    fn on_layout(&mut self, _width: u16, _height: u16) {}
    fn on_event_capture(&mut self, _event: &Event, _ctx: &mut EventCtx) {}
    fn on_event(&mut self, _event: &Event, _ctx: &mut EventCtx) {}
    fn on_message(&mut self, _message: &MessageEvent, _ctx: &mut EventCtx) {}
    /// Optional runtime-level app key hook.
    ///
    /// The runtime calls this before normal widget key dispatch, passing the
    /// active [`crate::App`] handle so app wrappers can query/mutate tree state.
    fn on_app_key(
        &mut self,
        _app: &mut crate::App,
        _key: &crate::keys::KeyEventData,
        _ctx: &mut EventCtx,
    ) {
    }
    /// Optional runtime-level app action hook.
    ///
    /// Called for unhandled actions so app wrappers can run selector/query-based
    /// behavior with mutable runtime access.
    fn on_app_action(&mut self, _app: &mut crate::App, _action: Action, _ctx: &mut EventCtx) {}
    /// Optional runtime-level app message hook.
    ///
    /// Called after `on_message` when the message remains unhandled.
    fn on_app_message(
        &mut self,
        _app: &mut crate::App,
        _message: &MessageEvent,
        _ctx: &mut EventCtx,
    ) {
    }
    /// Optional runtime-level app tick hook.
    ///
    /// Runs once per runtime tick after `on_tick` and before `Event::Tick`
    /// dispatch, allowing app wrappers to use query/mutation APIs.
    fn on_app_tick(&mut self, _app: &mut crate::App, _tick: u64, _ctx: &mut EventCtx) {}
    /// Optional visibility override for tree child nodes by child index.
    ///
    /// Tree mode can query this every frame and mirror it to child node
    /// `display` flags. Returning `None` leaves display unchanged.
    fn child_display_for_tree(&self, _child_index: usize) -> Option<bool> {
        None
    }
    /// Extra insets reserved by this widget before laying out tree children.
    ///
    /// Return `(top, right, bottom, left)` in cells. This is useful for
    /// widgets that draw internal chrome (for example tab bars) and need
    /// their children to start within the remaining content area.
    fn tree_child_content_inset(&self) -> (u16, u16, u16, u16) {
        (0, 0, 0, 0)
    }
    /// Optional hook exposing this widget's reactive dispatch implementation.
    ///
    /// Widgets with `ReactiveWidget` implementations should return `Some(self)`
    /// so the runtime can run queued reactive work in deterministic node order.
    fn reactive_widget(&mut self) -> Option<&mut dyn ReactiveWidget> {
        None
    }
    /// Optional key-binding hints exposed by this widget.
    ///
    /// Runtime dispatch uses focused-path hints as part of active binding lifecycle updates.
    fn binding_hints(&self) -> Vec<BindingHint> {
        Vec::new()
    }
    /// Declarative key bindings for this widget (analogous to Python Textual's BINDINGS).
    ///
    /// The runtime collects these along the focused widget chain. Priority bindings
    /// are checked first across the entire chain, then normal bindings (focused→root).
    fn bindings(&self) -> Vec<BindingDecl> {
        Vec::new()
    }
    /// The namespace this widget owns for action routing (e.g. `"button"`).
    ///
    /// Used by [`crate::action::resolve_action`] to route namespaced actions.
    fn action_namespace(&self) -> &str {
        ""
    }
    /// List of actions this widget can handle.
    fn action_registry(&self) -> &[ActionDecl] {
        &[]
    }
    /// Execute a parsed action. Returns `true` if the action was handled.
    fn execute_action(&mut self, _action: &ParsedAction, _ctx: &mut EventCtx) -> bool {
        false
    }
    /// Optional focused HELP markup exposed to framework-level help panels.
    fn help_markup(&self) -> Option<&str> {
        None
    }
    fn on_mouse_move(&mut self, _x: u16, _y: u16) -> bool {
        false
    }

    /// Return content scroll offset applied to descendants during render.
    ///
    /// Default widgets do not offset descendants.
    fn scroll_offset(&self) -> (usize, usize) {
        (0, 0)
    }

    /// Whether descendant rendering should be clipped to this widget's content box.
    ///
    /// Default widgets do not clip descendants.
    fn clips_descendants_to_content(&self) -> bool {
        false
    }
    /// Mouse wheel / touchpad scroll input.
    ///
    /// `delta_y > 0` scrolls down, `delta_y < 0` scrolls up.
    /// `delta_x > 0` scrolls right, `delta_x < 0` scrolls left.
    fn on_mouse_scroll(&mut self, _delta_x: i32, _delta_y: i32, _ctx: &mut EventCtx) {}
    /// Called after a render pass to inform the widget of its content-box size in cells.
    ///
    /// Coordinates for mouse events (`MouseDownEvent` / `MouseUpEvent` / `on_mouse_move`) are
    /// relative to this content box.
    fn focusable(&self) -> bool {
        false
    }
    /// Whether this widget type can receive focus (inherent capability, ignoring disabled state).
    /// Used for `:can-focus` CSS pseudo-class matching. Defaults to `focusable()`.
    fn can_focus(&self) -> bool {
        self.focusable()
    }
    fn set_focus(&mut self, _focused: bool) {}
    /// Whether the widget is disabled (used for `:disabled` selector matching).
    fn is_disabled(&self) -> bool {
        false
    }
    /// Set disabled state (used by DOM query bulk mutations).
    fn set_disabled_state(&mut self, _disabled: bool) {}
    /// Whether the widget is in loading state.
    fn is_loading(&self) -> bool {
        false
    }
    /// Set loading state (used by DOM query bulk mutations).
    fn set_loading_state(&mut self, _loading: bool) {}
    /// Whether the widget currently has focus (used for `:focus` selector matching).
    fn has_focus(&self) -> bool {
        false
    }
    /// Whether the widget is hovered (mouse support not yet implemented).
    fn is_hovered(&self) -> bool {
        false
    }
    fn set_hovered(&mut self, _hovered: bool) {}
    /// Whether the widget should be treated as interactive for mouse hover / cursor feedback.
    ///
    /// This is intentionally distinct from `focusable()`: some widgets (e.g. disabled buttons)
    /// are not focusable but should still provide hover affordances (like a "not-allowed" cursor).
    fn mouse_interactive(&self) -> bool {
        self.focusable()
    }
    /// Whether the widget is active (e.g. pressed/dragging).
    fn is_active(&self) -> bool {
        false
    }
    /// Whether styled rendering should preserve underlying frame cells that
    /// are not explicitly painted by this widget.
    ///
    /// Default `false` keeps classic box-widget behavior (auto background fill
    /// across the full layout rect). Overlay-style widgets can return `true`
    /// to avoid full-rect fills and paint sparse content over existing frame
    /// cells.
    fn preserve_underlay(&self) -> bool {
        false
    }
    /// Optional intrinsic content width hint (in cells), used by layout when `width: auto`.
    ///
    /// This should return the width of the widget's *content* (excluding margins and borders).
    fn content_width(&self) -> Option<usize> {
        None
    }
    fn layout_height(&self) -> Option<usize> {
        helpers::fixed_height_from_constraints(self.layout_constraints())
    }
    fn layout_constraints(&self) -> LayoutConstraints {
        self.styles()
            .map(|styles| styles.layout)
            .unwrap_or_default()
    }
    fn style(&self) -> Option<Style> {
        self.styles().map(|styles| styles.style.clone())
    }
    fn styles(&self) -> Option<&WidgetStyles> {
        None
    }
    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        None
    }
    fn style_type(&self) -> &'static str {
        std::any::type_name::<Self>()
            .rsplit("::")
            .next()
            .unwrap_or("Widget")
    }
    /// Optional super-type aliases used for CSS type selector matching.
    ///
    /// This enables Python-style subclass selector behavior where a widget can
    /// match both its concrete type and a base type selector (e.g.
    /// `CommandInput` also matching `Input` rules).
    fn style_type_aliases(&self) -> &[&'static str] {
        &[]
    }
    fn style_id(&self) -> Option<&str> {
        self.styles().and_then(|styles| styles.style_id.as_deref())
    }
    fn style_classes(&self) -> &[String] {
        helpers::empty_classes()
    }
    /// Set this widget's CSS id (if backed by WidgetStyles).
    fn set_style_id(&mut self, id: Option<String>) {
        if let Some(styles) = self.styles_mut() {
            styles.style_id = id;
        }
    }
    /// Optional text rendered on the top border when border-title styling is active.
    fn border_title(&self) -> Option<&str> {
        None
    }
    /// Optional text rendered on the bottom border when border-subtitle styling is active.
    fn border_subtitle(&self) -> Option<&str> {
        None
    }
    /// Legacy convenience wrapper: render with styling.
    ///
    /// Widget-to-widget rendering calls this during migration. Once the runtime
    /// renders via the arena tree (P1-12), this path becomes unused and the
    /// runtime calls `render_styled_dyn_obj` directly with the real `NodeId`.
    fn render_styled(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        self.render_styled_dyn_obj(console, options, None, NodeId::default())
    }
    /// Legacy convenience wrapper with debug layout.
    fn render_styled_with_debug(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        debug: &DebugLayout,
    ) -> Segments {
        self.render_styled_dyn_obj(console, options, Some(debug), NodeId::default())
    }
    fn set_width(&mut self, value: usize) {
        if let Some(styles) = self.styles_mut() {
            styles.set_width(value);
        }
    }

    fn set_height(&mut self, value: usize) {
        if let Some(styles) = self.styles_mut() {
            styles.set_height(value);
        }
    }

    fn set_min_width(&mut self, value: usize) {
        if let Some(styles) = self.styles_mut() {
            styles.set_min_width(value);
        }
    }

    fn set_max_width(&mut self, value: usize) {
        if let Some(styles) = self.styles_mut() {
            styles.set_max_width(value);
        }
    }

    fn set_min_height(&mut self, value: usize) {
        if let Some(styles) = self.styles_mut() {
            styles.set_min_height(value);
        }
    }

    fn set_max_height(&mut self, value: usize) {
        if let Some(styles) = self.styles_mut() {
            styles.set_max_height(value);
        }
    }
}

/// Tag all segments that lack a `textual:widget_id` metadata entry with the
/// given arena `NodeId` (encoded via `node_id_to_ffi` for FFI compatibility).
fn tag_widget_meta(node_id: NodeId, segments: Segments) -> Segments {
    let ffi_value = node_id::node_id_to_ffi(node_id) as i64;
    tag_widget_meta_raw(ffi_value, segments)
}

/// Shared implementation: tag segments with a raw i64 metadata value.
fn tag_widget_meta_raw(ffi_value: i64, segments: Segments) -> Segments {
    let mut out = Segments::new();
    for mut seg in segments {
        if seg.control.is_some() {
            out.push(seg);
            continue;
        }

        let has_widget_id = seg
            .meta
            .as_ref()
            .and_then(|m| m.meta.as_ref())
            .map(|map| map.contains_key(META_WIDGET_ID))
            .unwrap_or(false);
        if has_widget_id {
            out.push(seg);
            continue;
        }

        let mut map = seg
            .meta
            .as_ref()
            .and_then(|m| m.meta.as_ref())
            .map(|m| (**m).clone())
            .unwrap_or_default();
        map.insert(META_WIDGET_ID.to_string(), MetaValue::Int(ffi_value));

        let mut meta = seg.meta.unwrap_or_else(StyleMeta::new);
        meta.meta = Some(std::sync::Arc::new(map));
        seg.meta = Some(meta);
        out.push(seg);
    }
    out
}

#[derive(Debug, Clone, Copy, Default)]
pub struct LayoutConstraints {
    pub min_width: Option<usize>,
    pub max_width: Option<usize>,
    pub min_height: Option<usize>,
    pub max_height: Option<usize>,
}

pub(crate) fn render_widget_with_meta<W: Widget + ?Sized>(
    widget: &W,
    console: &Console,
    options: &ConsoleOptions,
    debug: Option<&DebugLayout>,
    node_id: NodeId,
    meta: &crate::css::SelectorMeta,
    resolved: &Style,
    debug_widget_label: &str,
) -> Segments {
    let parent_style = crate::css::current_parent_style();
    let line_pad = resolved.line_pad.unwrap_or(0) as usize;
    let full_width = options.size.0.max(1);
    let full_height = options.size.1.max(1);
    let (border_top, border_bottom, border_left, border_right) =
        helpers::border_spacing_from_style(resolved);
    let padding = resolved.effective_padding();

    let content_width = full_width
        .saturating_sub(border_left + border_right)
        .saturating_sub(line_pad.saturating_mul(2))
        .saturating_sub(padding.left as usize + padding.right as usize)
        .max(1);
    let content_height = full_height
        .saturating_sub(border_top + border_bottom)
        .saturating_sub(padding.top as usize + padding.bottom as usize)
        .max(1);

    // Textual's `line-pad` is horizontal padding applied to each line. To model this, render the
    // widget into a smaller content width and then wrap each line with `line_pad` spaces.
    let mut content_options = options.clone();
    content_options.size = (content_width, content_height);
    content_options.max_width = content_width;
    content_options.max_height = content_height;

    let segments = crate::css::with_style_stack(meta.clone(), resolved.clone(), || match debug {
        Some(debug) => widget.render_with_debug(console, &content_options, debug),
        None => widget.render(console, &content_options),
    });
    let segments = tag_widget_meta(node_id, segments);
    let segments_empty = segments.is_empty();

    let inner_width = content_width
        .saturating_add(line_pad.saturating_mul(2))
        .saturating_add(padding.left as usize + padding.right as usize)
        .max(1);
    let mut lines = if line_pad > 0 {
        let padded = helpers::apply_line_pad(
            segments,
            content_width,
            content_width + line_pad * 2,
            line_pad,
        );
        rich_rs::Segment::split_and_crop_lines(
            padded,
            content_width + line_pad * 2,
            None,
            false,
            false,
        )
    } else {
        rich_rs::Segment::split_and_crop_lines(segments, content_width, None, false, false)
    };

    let has_surface_paint = resolved.bg.is_some()
        || resolved.hatch.is_some()
        || resolved.border_top.is_set()
        || resolved.border_right.is_set()
        || resolved.border_bottom.is_set()
        || resolved.border_left.is_set()
        || resolved.outline_top.is_set()
        || resolved.outline_right.is_set()
        || resolved.outline_bottom.is_set()
        || resolved.outline_left.is_set();
    let preserve_underlay = widget.preserve_underlay()
        || (resolved.position == Some(Position::Absolute) && !has_surface_paint)
        || (!has_surface_paint && segments_empty);

    if preserve_underlay && segments_empty {
        return Segments::new();
    }
    let mut final_lines: Vec<Vec<rich_rs::Segment>> = Vec::new();
    if preserve_underlay {
        // Overlay-style path: keep sparse output and don't auto-fill the whole
        // layout rect. This allows widgets to paint only explicit cells.
        final_lines.extend(lines);
    } else {
        // Ensure content height is respected before padding.
        let mut fill = rich_rs::Style::new();
        let fallback_bg = crate::style::parse_color_like("$background")
            .unwrap_or(crate::style::Color::rgb(0, 0, 0));
        let parent_bg = parent_style
            .clone()
            .and_then(|s| s.bg)
            .unwrap_or(fallback_bg);
        let inner_bg = resolved
            .bg
            .map(|c| c.flatten_over(parent_bg))
            .unwrap_or(parent_bg);
        fill = fill.with_bgcolor(inner_bg.to_simple_opaque());
        if let Some(fg) = resolved.fg {
            fill = fill.with_color(fg.flatten_over(inner_bg).to_simple_opaque());
        }
        lines = rich_rs::Segment::set_shape(
            &lines,
            content_width + line_pad * 2,
            Some(content_height),
            Some(fill),
            false,
        );

        // Apply left/right padding.
        let pad_left = padding.left as usize;
        let pad_right = padding.right as usize;
        let mut padded_lines: Vec<Vec<rich_rs::Segment>> = Vec::with_capacity(lines.len());
        for line in lines {
            let mut out = Vec::new();
            if pad_left > 0 {
                out.push(rich_rs::Segment::styled(" ".repeat(pad_left), fill));
            }
            out.extend(line);
            if pad_right > 0 {
                out.push(rich_rs::Segment::styled(" ".repeat(pad_right), fill));
            }
            padded_lines.push(out);
        }

        // Apply top/bottom padding.
        let pad_top = padding.top as usize;
        let pad_bottom = padding.bottom as usize;
        let padded_width = content_width + line_pad * 2 + pad_left + pad_right;
        if pad_top > 0 {
            let blank = vec![rich_rs::Segment::styled(" ".repeat(padded_width), fill)];
            for _ in 0..pad_top {
                final_lines.push(blank.clone());
            }
        }
        final_lines.extend(padded_lines);
        if pad_bottom > 0 {
            let blank = vec![rich_rs::Segment::styled(" ".repeat(padded_width), fill)];
            for _ in 0..pad_bottom {
                final_lines.push(blank.clone());
            }
        }
    }

    let mut segments = Segments::new();
    let line_count = final_lines.len();
    for (idx, line) in final_lines.into_iter().enumerate() {
        segments.extend(line);
        if idx + 1 < line_count {
            segments.push(rich_rs::Segment::line());
        }
    }

    let styled = crate::css::apply_style_to_segments(
        node_id,
        segments,
        resolved.clone(),
        parent_style.clone(),
    );
    let segments = helpers::apply_border_edges(
        styled,
        inner_width,
        resolved.clone(),
        parent_style.clone(),
        full_width,
        full_height,
        debug_widget_label,
        widget.border_title(),
        widget.border_subtitle(),
    );
    let segments = if let Some(opacity) = resolved.opacity {
        crate::css::apply_widget_opacity_to_segments(segments, opacity, parent_style)
    } else {
        segments
    };
    tag_widget_meta(node_id, segments)
}

impl LayoutConstraints {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn min_width(mut self, value: usize) -> Self {
        self.min_width = Some(value.max(1));
        self
    }

    pub fn max_width(mut self, value: usize) -> Self {
        self.max_width = Some(value.max(1));
        self
    }

    pub fn min_height(mut self, value: usize) -> Self {
        self.min_height = Some(value.max(1));
        self
    }

    pub fn max_height(mut self, value: usize) -> Self {
        self.max_height = Some(value.max(1));
        self
    }
}

#[derive(Debug, Clone, Default)]
pub struct WidgetStyles {
    pub style: Style,
    pub layout: LayoutConstraints,
    pub style_id: Option<String>,
}

impl WidgetStyles {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn fg(mut self, color: Color) -> Self {
        self.style = self.style.fg(color);
        self
    }

    pub fn bg(mut self, color: Color) -> Self {
        self.style = self.style.bg(color);
        self
    }

    pub fn bold(mut self, value: bool) -> Self {
        self.style = self.style.bold(value);
        self
    }

    pub fn dim(mut self, value: bool) -> Self {
        self.style = self.style.dim(value);
        self
    }

    pub fn italic(mut self, value: bool) -> Self {
        self.style = self.style.italic(value);
        self
    }

    pub fn underline(mut self, value: bool) -> Self {
        self.style = self.style.underline(value);
        self
    }

    pub fn border(mut self, value: bool) -> Self {
        self.style = self.style.border(value);
        self
    }

    pub fn set_fg(&mut self, color: Color) {
        self.style = std::mem::take(&mut self.style).fg(color);
    }

    pub fn set_bg(&mut self, color: Color) {
        self.style = std::mem::take(&mut self.style).bg(color);
    }

    pub fn set_bold(&mut self, value: bool) {
        self.style = std::mem::take(&mut self.style).bold(value);
    }

    pub fn set_dim(&mut self, value: bool) {
        self.style = std::mem::take(&mut self.style).dim(value);
    }

    pub fn set_italic(&mut self, value: bool) {
        self.style = std::mem::take(&mut self.style).italic(value);
    }

    pub fn set_underline(&mut self, value: bool) {
        self.style = std::mem::take(&mut self.style).underline(value);
    }

    pub fn set_border(&mut self, value: bool) {
        self.style = std::mem::take(&mut self.style).border(value);
    }

    pub fn set_style_id(&mut self, id: impl Into<String>) {
        self.style_id = Some(id.into());
    }

    pub fn clear_style_id(&mut self) {
        self.style_id = None;
    }

    pub fn width(mut self, value: usize) -> Self {
        let value = value.max(1);
        self.layout.min_width = Some(value);
        self.layout.max_width = Some(value);
        self
    }

    pub fn height(mut self, value: usize) -> Self {
        let value = value.max(1);
        self.layout.min_height = Some(value);
        self.layout.max_height = Some(value);
        self
    }

    pub fn min_width(mut self, value: usize) -> Self {
        self.layout.min_width = Some(value.max(1));
        self
    }

    pub fn max_width(mut self, value: usize) -> Self {
        self.layout.max_width = Some(value.max(1));
        self
    }

    pub fn min_height(mut self, value: usize) -> Self {
        self.layout.min_height = Some(value.max(1));
        self
    }

    pub fn max_height(mut self, value: usize) -> Self {
        self.layout.max_height = Some(value.max(1));
        self
    }

    pub fn set_width(&mut self, value: usize) {
        let value = value.max(1);
        self.layout.min_width = Some(value);
        self.layout.max_width = Some(value);
    }

    pub fn set_height(&mut self, value: usize) {
        let value = value.max(1);
        self.layout.min_height = Some(value);
        self.layout.max_height = Some(value);
    }

    pub fn set_min_width(&mut self, value: usize) {
        self.layout.min_width = Some(value.max(1));
    }

    pub fn set_max_width(&mut self, value: usize) {
        self.layout.max_width = Some(value.max(1));
    }

    pub fn set_min_height(&mut self, value: usize) {
        self.layout.min_height = Some(value.max(1));
    }

    pub fn set_max_height(&mut self, value: usize) {
        self.layout.max_height = Some(value.max(1));
    }

    /// Compare with another set of widget styles and classify the change
    /// for invalidation purposes.
    pub fn invalidation_kind(&self, other: &WidgetStyles) -> StyleChangeKind {
        if self.style_id != other.style_id {
            return StyleChangeKind::Layout;
        }
        classify_style_change(&self.style, &other.style)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node_id::NodeId;
    use crate::runtime::dispatch_ctx::{dispatch_recipient, set_dispatch_recipient};
    use crate::style::{Display, Layout, Overflow, Visibility};

    #[test]
    fn classify_identical_styles_returns_none() {
        let s = Style::default();
        assert_eq!(classify_style_change(&s, &s), StyleChangeKind::None);
    }

    #[test]
    fn classify_visual_only_change_returns_visual() {
        let old = Style::default();
        let mut new = old.clone();
        new.fg = Some(crate::style::Color::rgb(255, 0, 0));
        assert_eq!(classify_style_change(&old, &new), StyleChangeKind::Visual);
    }

    #[test]
    fn classify_bg_change_returns_visual() {
        let old = Style::default();
        let mut new = old.clone();
        new.bg = Some(crate::style::Color::rgb(0, 0, 255));
        assert_eq!(classify_style_change(&old, &new), StyleChangeKind::Visual);
    }

    #[test]
    fn classify_bold_change_returns_visual() {
        let old = Style::default();
        let mut new = old.clone();
        new.bold = Some(true);
        assert_eq!(classify_style_change(&old, &new), StyleChangeKind::Visual);
    }

    #[test]
    fn classify_border_change_returns_layout() {
        // Borders affect content box sizing via border_spacing_from_style.
        let old = Style::default();
        let mut new = old.clone();
        new.border = Some(true);
        assert_eq!(classify_style_change(&old, &new), StyleChangeKind::Layout);
    }

    #[test]
    fn classify_opacity_change_returns_visual() {
        let old = Style::default();
        let mut new = old.clone();
        new.opacity = Some(128);
        assert_eq!(classify_style_change(&old, &new), StyleChangeKind::Visual);
    }

    #[test]
    fn classify_display_change_returns_layout() {
        let old = Style::default();
        let mut new = old.clone();
        new.display = Some(Display::None);
        assert_eq!(classify_style_change(&old, &new), StyleChangeKind::Layout);
    }

    #[test]
    fn classify_visibility_change_returns_layout() {
        let old = Style::default();
        let mut new = old.clone();
        new.visibility = Some(Visibility::Hidden);
        assert_eq!(classify_style_change(&old, &new), StyleChangeKind::Layout);
    }

    #[test]
    fn classify_overflow_change_returns_layout() {
        let old = Style::default();
        let mut new = old.clone();
        new.overflow = Some(Overflow::Hidden);
        assert_eq!(classify_style_change(&old, &new), StyleChangeKind::Layout);
    }

    #[test]
    fn classify_layout_change_returns_layout() {
        let old = Style::default();
        let mut new = old.clone();
        new.layout = Some(Layout::Horizontal);
        assert_eq!(classify_style_change(&old, &new), StyleChangeKind::Layout);
    }

    #[test]
    fn classify_width_change_returns_layout() {
        let old = Style::default();
        let mut new = old.clone();
        new.width = Some(crate::style::Scalar::Cells(42));
        assert_eq!(classify_style_change(&old, &new), StyleChangeKind::Layout);
    }

    #[test]
    fn classify_margin_change_returns_layout() {
        let old = Style::default();
        let mut new = old.clone();
        new.margin = Some(crate::style::Spacing {
            top: 1,
            right: 1,
            bottom: 1,
            left: 1,
        });
        assert_eq!(classify_style_change(&old, &new), StyleChangeKind::Layout);
    }

    #[test]
    fn classify_padding_change_returns_layout() {
        let old = Style::default();
        let mut new = old.clone();
        new.padding = Some(crate::style::Spacing {
            top: 2,
            right: 0,
            bottom: 2,
            left: 0,
        });
        assert_eq!(classify_style_change(&old, &new), StyleChangeKind::Layout);
    }

    #[test]
    fn classify_mixed_changes_returns_layout() {
        // When both visual and layout properties change, Layout wins.
        let old = Style::default();
        let mut new = old.clone();
        new.fg = Some(crate::style::Color::rgb(255, 0, 0)); // visual
        new.display = Some(Display::None); // layout
        assert_eq!(classify_style_change(&old, &new), StyleChangeKind::Layout);
    }

    #[test]
    fn classify_layer_change_returns_visual() {
        let old = Style::default();
        let mut new = old.clone();
        new.layer = Some("overlay".into());
        assert_eq!(classify_style_change(&old, &new), StyleChangeKind::Visual);
    }

    #[test]
    fn widget_styles_invalidation_kind_delegates() {
        let a = WidgetStyles::default();
        let mut b = WidgetStyles::default();
        b.style.bg = Some(crate::style::Color::rgb(0, 255, 0));
        assert_eq!(a.invalidation_kind(&b), StyleChangeKind::Visual);
    }

    fn make_node_id() -> NodeId {
        let mut sm: slotmap::SlotMap<NodeId, ()> = slotmap::SlotMap::new();
        sm.insert(())
    }

    /// Minimal widget for dispatch context tests.
    struct CtxProbe;
    impl Widget for CtxProbe {
        fn render(
            &self,
            _console: &rich_rs::Console,
            _options: &rich_rs::ConsoleOptions,
        ) -> rich_rs::Segments {
            rich_rs::Segments::new()
        }
    }

    #[test]
    fn node_id_returns_dispatch_recipient() {
        let w = CtxProbe;
        // Without context, falls back to default.
        assert_eq!(w.node_id(), NodeId::default());

        let id = make_node_id();
        let _guard = set_dispatch_recipient(id);
        assert_eq!(w.node_id(), id);
        assert_ne!(id, NodeId::default());
    }

    #[test]
    fn render_styled_dyn_obj_sets_dispatch_context_and_restores() {
        let console = rich_rs::Console::new();
        let mut opts = rich_rs::ConsoleOptions::default();
        opts.size = (10, 1);
        opts.max_width = 10;
        opts.max_height = 1;

        // Create from same SlotMap so keys are distinct.
        let mut sm: slotmap::SlotMap<NodeId, ()> = slotmap::SlotMap::new();
        let id_a = sm.insert(());
        let id_b = sm.insert(());
        assert_ne!(id_a, id_b);

        // Set an outer context (simulating parent render).
        let _outer = set_dispatch_recipient(id_a);
        assert_eq!(dispatch_recipient(), Some(id_a));

        // Rendering widget B should temporarily set context to id_b.
        let widget_b = CtxProbe;
        let _ = widget_b.render_styled_dyn_obj(&console, &opts, None, id_b);

        // After render_styled_dyn_obj returns, the guard should restore id_a.
        assert_eq!(
            dispatch_recipient(),
            Some(id_a),
            "dispatch context must be restored after sibling render"
        );
    }
}
