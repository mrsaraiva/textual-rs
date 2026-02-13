use rich_rs::{Console, ConsoleOptions, MetaValue, Segments, StyleMeta};

use crate::action::{ActionDecl, ParsedAction};
use crate::compose::ComposeResult;
use crate::debug::DebugLayout;
use crate::event::{BindingHint, Event, EventCtx};
use crate::message::MessageEvent;
use crate::node_id::{self, NodeId};
use crate::style::{Color, Style};

use super::helpers;

const META_WIDGET_ID: &str = "textual:widget_id";

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
pub trait Widget: Send + Sync {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments;

    /// Declare child widgets for this widget.
    ///
    /// The runtime materializes these declarations into arena nodes during
    /// mount. The default implementation returns an empty list (leaf widget).
    fn compose(&self) -> ComposeResult {
        Vec::new()
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
        // Use the arena NodeId for metadata tagging — `apply_style_to_segments`
        // checks this value, so it must match the tag used here.
        let meta = crate::css::selector_meta_generic(self);
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
        let resolved = crate::css::resolve_style(self, &meta);
        let parent_style = crate::css::current_parent_style();
        let line_pad = resolved.padding.map(|s| s.left as usize).unwrap_or(0);
        let full_width = options.size.0.max(1);
        let full_height = options.size.1.max(1);
        let (border_top, border_bottom, border_left, border_right) =
            helpers::border_spacing_from_style(&resolved);

        let content_width = full_width
            .saturating_sub(border_left + border_right)
            .saturating_sub(line_pad.saturating_mul(2))
            .max(1);
        let content_height = full_height
            .saturating_sub(border_top + border_bottom)
            .max(1);

        // Textual's `line-pad` is horizontal padding applied to each line. To model this, render the
        // widget into a smaller content width and then wrap each line with `line_pad` spaces.
        let mut content_options = options.clone();
        content_options.size = (content_width, content_height);
        content_options.max_width = content_width;
        content_options.max_height = content_height;

        let segments = crate::css::with_style_stack(meta, resolved.clone(), || match debug {
            Some(debug) => self.render_with_debug(console, &content_options, debug),
            None => self.render(console, &content_options),
        });
        let segments = tag_widget_meta(_node_id, segments);

        let inner_width = content_width
            .saturating_add(line_pad.saturating_mul(2))
            .max(1);
        let segments = if line_pad > 0 {
            let padded = helpers::apply_line_pad(segments, content_width, inner_width, line_pad);
            tag_widget_meta(_node_id, padded)
        } else {
            segments
        };

        let styled =
            crate::css::apply_style_to_segments(_node_id, segments, resolved.clone(), parent_style.clone());
        let segments = helpers::apply_border_edges(
            styled,
            inner_width,
            resolved.clone(),
            parent_style.clone(),
            full_width,
            full_height,
            &debug_widget_label,
        );
        let segments = if let Some(opacity) = resolved.opacity {
            crate::css::apply_widget_opacity_to_segments(segments, opacity, parent_style)
        } else {
            segments
        };
        tag_widget_meta(_node_id, segments)
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
    fn on_event_capture(&mut self, _event: &Event, _ctx: &mut EventCtx) {}
    fn on_event(&mut self, _event: &Event, _ctx: &mut EventCtx) {}
    fn on_message(&mut self, _message: &MessageEvent, _ctx: &mut EventCtx) {}
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
    /// Mouse wheel / touchpad scroll input.
    ///
    /// `delta_y > 0` scrolls down, `delta_y < 0` scrolls up.
    /// `delta_x > 0` scrolls right, `delta_x < 0` scrolls left.
    fn on_mouse_scroll(&mut self, _delta_x: i32, _delta_y: i32, _ctx: &mut EventCtx) {}
    /// Called after a render pass to inform the widget of its content-box size in cells.
    ///
    /// Coordinates for mouse events (`MouseDownEvent` / `MouseUpEvent` / `on_mouse_move`) are
    /// relative to this content box.
    fn on_layout(&mut self, _width: u16, _height: u16) {}
    fn focusable(&self) -> bool {
        false
    }
    fn set_focus(&mut self, _focused: bool) {}
    /// Whether the widget is disabled (used for `:disabled` selector matching).
    fn is_disabled(&self) -> bool {
        false
    }
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
    fn style_id(&self) -> Option<&str> {
        None
    }
    fn style_classes(&self) -> &[String] {
        helpers::empty_classes()
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
}
