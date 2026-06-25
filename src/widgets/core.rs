use rich_rs::{Console, ConsoleOptions, MetaValue, Segments, StyleMeta};
use std::any::Any;

use crate::action::{ActionDecl, ParsedAction};
use crate::compose::ComposeResult;
use crate::debug::DebugLayout;
use crate::event::{Action, BindingHint, Event, EventCtx};
use crate::message::MessageEvent;
use crate::node_id::{self, NodeId};
use crate::reactive::ReactiveWidget;
use crate::style::{Color, HorizontalAlign, Position, Style, VerticalAlign};

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
    /// Optional extended help shown in key/help panels.
    pub tooltip: Option<String>,
    /// Optional namespace used by HelpPanel grouping/sectioning.
    pub namespace: Option<String>,
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
            tooltip: None,
            namespace: None,
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

    /// Attach optional extended help text for key/help panel rows.
    pub fn with_tooltip(mut self, tooltip: impl Into<String>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }

    /// Attach an optional namespace/grouping marker for help panel sections.
    pub fn with_namespace(mut self, namespace: impl Into<String>) -> Self {
        self.namespace = Some(namespace.into());
        self
    }
}

/// Selection anchor used by the runtime-level selection pipeline.
///
/// Widgets may use whichever fields are meaningful for their own text model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WidgetSelectionAnchor {
    /// Visual row in widget-local coordinates.
    pub row: usize,
    /// Visual column in widget-local coordinates (cell-based).
    pub col: usize,
    /// Optional widget-defined logical index (for example grapheme offset).
    pub index: usize,
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

    /// Render a single visual line (row) at `y` in widget-local coordinates.
    ///
    /// Default implementation renders the full widget and extracts one row.
    fn render_line(&self, y: usize, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let lines = rich_rs::Segment::split_and_crop_lines(
            self.render(console, options),
            width,
            None,
            true,
            false,
        );
        lines.get(y).cloned().unwrap_or_default().into()
    }

    /// Render a contiguous range of visual lines starting at `start_y`.
    ///
    /// Default implementation delegates to [`Self::render_line`] for each row.
    fn render_lines(
        &self,
        start_y: usize,
        line_count: usize,
        console: &Console,
        options: &ConsoleOptions,
    ) -> Vec<Segments> {
        (0..line_count)
            .map(|offset| self.render_line(start_y + offset, console, options))
            .collect()
    }

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

    /// Drain compose-time handle sinks for the children returned by the most
    /// recent `take_composed_children()` call, as `(child_index, sink)` pairs.
    /// Containers that offer `with_child_handle`-style builders override this.
    /// Default: no sinks (leaf widgets and containers without handle builders).
    ///
    /// Migration-period shape — RA-2's node-record direction will eventually
    /// fold child declaration metadata into `ChildDecl`-only compose, at which
    /// point this hook is deleted together with `take_composed_children`.
    fn take_child_handle_sinks(&mut self) -> Vec<(usize, crate::handle::HandleSink)> {
        Vec::new()
    }

    /// Drain compose-time CSS id/class metadata for the children returned by the
    /// most recent `take_composed_children()` call, as
    /// `(child_index, css_id, classes)` tuples.
    ///
    /// Containers built via `with_compose` (which receive `ChildDecl`s carrying
    /// `.with_id()` / `.with_classes()` metadata) override this so the runtime
    /// mount path applies that metadata to the mounted node — keeping the same
    /// effect as `App::mount_declarations` while children still flow through the
    /// `take_composed_children()` extraction path. Default: no metadata.
    ///
    /// Migration-period shape (see `take_child_handle_sinks`): folds into
    /// `ChildDecl`-only compose under RA-2.
    fn take_child_decl_meta(&mut self) -> Vec<ChildDeclMeta> {
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

    /// This node's interaction state, delivered via the dispatch context
    /// (same mechanism as `node_id()`). Outside dispatch it returns
    /// `NodeState::default()`.
    fn node_state(&self) -> NodeState {
        crate::runtime::dispatch_ctx::dispatch_node_state().unwrap_or_default()
    }

    /// Notification hook invoked by the tree's state writers after the node
    /// record changed. Widgets needing side effects override this. Default: no-op.
    fn on_node_state_changed(&mut self, _old: NodeState, _new: NodeState) {}

    /// One-shot mount seed (see NodeSeed).
    fn take_node_seed(&mut self) -> NodeSeed {
        NodeSeed::default()
    }

    /// Drain messages this widget wants posted *at mount time*, as boxed
    /// [`crate::message::Message`] payloads.
    ///
    /// Python Textual widgets can post messages from `on_mount` (which has an
    /// app/message context). In the arena runtime `on_mount(&mut self)` has no
    /// `EventCtx`, so widgets that need to emit a message as part of their
    /// initial state (for example `Select(allow_blank=false)` posting
    /// `SelectChanged` for its auto-selected value) stage those messages here.
    ///
    /// The runtime drains this **once, right after the node is mounted**, and
    /// routes each message through the normal message bus with the mounted
    /// node as the sender/control — exactly as if the widget had called
    /// `ctx.post_message(..)`. This is a drain-at-mount adapter over the core
    /// message flow (the same pattern as [`Widget::take_child_decl_meta`]); it
    /// is **not** a separate dispatch path.
    ///
    /// Default: no messages.
    fn take_pending_mount_messages(&mut self) -> Vec<Box<dyn crate::message::Message>> {
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
        // Set dispatch context so self.node_id() returns the correct arena
        // NodeId during render(). The guard restores the previous recipient
        // on drop, so nested/sibling renders don't leak context.
        let _dispatch_guard =
            crate::runtime::dispatch_ctx::set_dispatch_recipient(_node_id, NodeState::default());

        // Use the arena NodeId for metadata tagging — `apply_style_to_segments`
        // checks this value, so it must match the tag used here.
        // Debug label is built from type identity + dispatch-context state only;
        // DOM identity (id/classes) lives on the node record, not the widget.
        let debug_widget_label = {
            let mut label = self.style_type().to_string();
            let state = self.node_state();
            if state.disabled {
                label.push_str(":disabled");
            }
            if state.focused {
                label.push_str(":focus");
            }
            if state.hovered {
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
    /// Set the virtual content size for scroll host widgets.
    ///
    /// Called by the runtime layout pass to inform scroll containers of the total
    /// content extent. Scroll hosts override this to update their internal state
    /// (e.g. scrollbar thumb sizing). The default is a no-op for non-scroll widgets.
    fn set_virtual_content_size(&mut self, _width: usize, _height: usize) {}
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
    /// Called by the runtime when a declarative binding's action string cannot be
    /// resolved by `resolve_action` and `execute_action`.  Only `TextualAppAdapter`
    /// overrides this.
    fn on_app_unhandled_action(
        &mut self,
        _app: &mut crate::App,
        _action: &str,
        _ctx: &mut EventCtx,
    ) {
    }
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
    /// Optional runtime-level app-timer hook.
    ///
    /// Invoked by the runtime when one or more app-level timers (registered via
    /// [`crate::App::set_interval`] / [`crate::App::set_timer`]) are due. App
    /// wrappers run the registered timer callbacks here and then dispatch the
    /// app-reactive bridge, so a callback that mutates a reactive field (e.g.
    /// `self.time = now`) fires its watcher in the same turn — mirroring Python's
    /// `Timer._tick` invoking the callback in the target's context.
    fn on_app_timer(&mut self, _app: &mut crate::App, _ctx: &mut EventCtx) {}
    /// Optional runtime-level app mount hook.
    ///
    /// Called once after the widget tree is fully built and mounted, passing
    /// the active [`crate::App`] handle. Used by app wrappers to dispatch
    /// init-phase reactive changes after the widget tree is available.
    fn on_app_mount(&mut self, _app: &mut crate::App, _ctx: &mut EventCtx) {}
    /// Optional visibility override for tree child nodes by child index.
    ///
    /// Tree mode can query this every frame and mirror it to child node
    /// `display` flags. Returning `None` leaves display unchanged.
    fn child_display_for_tree(&self, _child_index: usize) -> Option<bool> {
        None
    }
    /// Optional per-child CSS class overrides driven by this widget's state.
    ///
    /// Tree mode queries this every frame for each child (by child index) and
    /// mirrors the returned `(class, on)` pairs onto the child node's class set
    /// (the same sync pass as [`Widget::child_display_for_tree`]). This is the
    /// canonical arena mechanism for a `can_focus_children=False` container (for
    /// example `ListView`) to drive a child's `-highlight` / `-hovered` state
    /// without owning the child's `NodeId`. Returning an empty list leaves the
    /// child's classes unchanged.
    fn child_classes_for_tree(&self, _child_index: usize) -> Vec<(&'static str, bool)> {
        Vec::new()
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
    /// Gate whether an action may run on this widget (its action namespace).
    ///
    /// Mirrors Python `DOMNode.check_action`:
    /// - `Some(true)` — action is enabled (default).
    /// - `Some(false)` — action is hidden / not runnable.
    /// - `None` — action is disabled (shown dimmed in footer, not runnable).
    ///
    /// The runtime consults this on the *resolved* action target before
    /// dispatch, so a `[@click=...]` span or `run_action(...)` call is gated the
    /// same way a key binding is.
    fn check_action(&self, _action: &str, _parameters: &[String]) -> Option<bool> {
        Some(true)
    }
    /// Optional focused HELP markup exposed to framework-level help panels.
    fn help_markup(&self) -> Option<&str> {
        None
    }
    /// Optional tooltip text for hover overlays.
    ///
    /// Runtime may query this for the currently hovered widget to show a
    /// tooltip popup. Return `None` (or empty text) when no tooltip is active.
    fn tooltip(&self) -> Option<String> {
        None
    }
    /// Optional tooltip anchor point in content-local coordinates.
    ///
    /// Widgets may override this to keep tooltip placement stable for a logical
    /// sub-region (for example a hovered footer key) rather than the entire
    /// widget bounds.
    fn tooltip_anchor(&self) -> Option<(u16, u16)> {
        None
    }
    fn on_mouse_move(&mut self, _x: u16, _y: u16) -> bool {
        false
    }
    /// Whether this widget participates in screen-level text selection.
    fn allow_select(&self) -> bool {
        false
    }
    /// Convert widget-local pointer coordinates into a selection anchor.
    fn selection_at(&self, _x: u16, _y: u16) -> Option<WidgetSelectionAnchor> {
        None
    }
    /// Resolve a word selection range at widget-local pointer coordinates.
    ///
    /// Used by runtime double-click selection behavior.
    fn selection_word_range_at(
        &self,
        _x: u16,
        _y: u16,
    ) -> Option<(WidgetSelectionAnchor, WidgetSelectionAnchor)> {
        None
    }
    /// Resolve a full-content selection range.
    ///
    /// Used by runtime triple-click selection behavior.
    fn selection_all_range(&self) -> Option<(WidgetSelectionAnchor, WidgetSelectionAnchor)> {
        None
    }
    /// Update this widget's current selection.
    ///
    /// Returns `true` when visible selection state changed.
    fn update_selection(
        &mut self,
        _from: WidgetSelectionAnchor,
        _to: WidgetSelectionAnchor,
    ) -> bool {
        false
    }
    /// Clear this widget's selection.
    ///
    /// Returns `true` when visible selection state changed.
    fn clear_selection(&mut self) -> bool {
        false
    }
    /// Read this widget's selected text as plain text.
    fn get_selection(&self) -> Option<String> {
        None
    }
    /// Optional callback invoked after runtime updates this widget's selection.
    fn selection_updated(&mut self, _ctx: &mut EventCtx) {}

    /// Return content scroll offset applied to descendants during render.
    ///
    /// Default widgets do not offset descendants.
    fn scroll_offset(&self) -> (usize, usize) {
        (0, 0)
    }

    /// Return content scroll offset (float precision) applied to descendants
    /// during render.
    ///
    /// Default implementation preserves compatibility by deriving from
    /// `scroll_offset()`.
    fn scroll_offset_f32(&self) -> (f32, f32) {
        let (x, y) = self.scroll_offset();
        (x as f32, y as f32)
    }

    /// Return the effective visible scroll viewport size `(width, height)`.
    ///
    /// Widgets that reserve space for scrollbars should override this so tree
    /// rendering can clip scrolled descendants to the real viewport.
    fn scroll_viewport_size(&self) -> Option<(usize, usize)> {
        None
    }

    /// Return the virtual scrollable content size `(width, height)` used by
    /// dedicated scrollbar lanes.
    ///
    /// Widgets that render scrollable content directly (without a scrollable
    /// child subtree) should override this so the runtime can size scrollbar
    /// thumbs from real content extents.
    fn scroll_virtual_content_size(&self) -> Option<(usize, usize)> {
        None
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
    /// Whether this widget allows focus traversal into descendant widgets.
    ///
    /// Scrollable containers in Python Textual expose this as a constructor-level
    /// policy (`can_focus_children`). The default keeps existing behavior by
    /// allowing traversal into descendants.
    fn can_focus_children(&self) -> bool {
        true
    }
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
    /// Initial disabled state for off-tree rendering and tree-mount initialization.
    ///
    /// Widgets that carry their own disabled flag (e.g. `Button`) override this so
    /// that `WidgetTree::make_node_from_seed` can seed `node.state.disabled` correctly
    /// and CSS `:disabled` rules apply from the very first render.
    fn is_initially_disabled(&self) -> bool {
        false
    }
    /// Whether this widget should be treated as focused from the first render.
    ///
    /// Widgets that track focus internally (e.g. `Tabs`) override this so that
    /// `build_widget_tree_from_root` can seed `node.state.focused` correctly when
    /// `on_node_state_changed(focused: true)` is called before tree construction
    /// (as is common in unit tests).
    ///
    /// After mount the canonical source of truth is `WidgetNode.state.focused` in
    /// the tree, set via `WidgetTree::set_focus_state`.
    fn is_initially_focused(&self) -> bool {
        false
    }
    /// CSS classes for off-tree style resolution.
    ///
    /// Used by `selector_meta_generic` when no dispatch context is active (e.g.
    /// during `layout_height()` or direct `FrameBuffer::from_renderable` renders).
    /// Widgets that maintain their own class list (Button, FooterKey, Input) override
    /// this so that CSS rules like `Button.-primary { ... }` resolve correctly even
    /// outside the arena render path.
    ///
    /// After mount the canonical source of truth is `WidgetNode.classes` in the tree;
    /// this method is ONLY consulted off-tree.
    fn style_classes(&self) -> &[String] {
        &[]
    }
    /// CSS id for off-tree style resolution.
    ///
    /// Widgets that carry their own CSS id (e.g. `Node`) override this so that
    /// id-selector rules (`#hero { ... }`) are visible in off-tree renders and
    /// during `build_widget_tree_from_root` identity propagation.
    fn style_id(&self) -> Option<&str> {
        None
    }
    /// Hover state for off-tree style resolution.
    ///
    /// Used by `selector_meta_generic` when no dispatch context is active. Widgets
    /// that track hover internally (e.g. `FooterKey`) override this so that
    /// `FooterKey:hover .footer-key--key` rules apply correctly in off-tree renders.
    fn is_hovered(&self) -> bool {
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
    /// Whether this widget is a transparent styling wrapper (e.g. `Node`, created
    /// by `Static::id(..)` / `Widget::class(..)`) that drains a single child into
    /// the arena tree and carries CSS identity on the wrapper itself.
    ///
    /// Python Textual applies `#id`/`.class` styles directly to the widget, so a
    /// wrapper with an UNSET width/height must size to its wrapped child's content
    /// (shrink-to-content), exactly like the widget it stands in for — rather than
    /// flex-filling the parent. The layout uses this to opt unset dimensions into
    /// the same intrinsic content measurement that explicit `auto` triggers.
    fn is_transparent_wrapper(&self) -> bool {
        false
    }
    /// Drain pending class add/remove ops that were staged by widget methods called
    /// outside of an event handler (e.g. via `App::with_query_one_mut_as`).
    ///
    /// Returns a list of `(class_name, add)` pairs where `add = true` means add the
    /// class and `add = false` means remove it. The runtime calls this after each
    /// `with_widget_mut` invocation and applies the ops directly to the arena node.
    ///
    /// Widgets that stage class changes from non-event-handler contexts should
    /// override this method to drain and return those pending ops.
    fn drain_pending_class_ops(&mut self) -> Vec<(String, bool)> {
        Vec::new()
    }
    /// Drain a pending inline-style write-through staged by a post-mount
    /// `set_inline_style` call.
    ///
    /// `take_node_seed()` moves a widget's seed (including its inline style) into
    /// the arena node at mount, emptying the widget-held seed. A subsequent
    /// post-mount `set_inline_style` (e.g. a reactive `watch_color` doing
    /// `widget.set_inline_style(Style::new().bg(c))` via `App::with_widget_mut`)
    /// only updates the now-detached widget seed and never reaches the node's
    /// rendered style. Widgets that support post-mount inline styling override
    /// this to return the style that the runtime must cascade onto the arena
    /// node (`node.styles.style.combine(&returned)`). Mirrors Python's
    /// `widget.styles.background = color` writing directly to the node styles.
    ///
    /// The returned style is *combined over* the node's existing inline style, so
    /// only the explicitly-set fields override (matching Python's per-property
    /// `styles.<prop> = value` assignment). Returns `None` when nothing is staged.
    fn take_inline_style_writethrough(&mut self) -> Option<Style> {
        None
    }
    /// Optional intrinsic content width hint (in cells), used by layout when `width: auto`.
    ///
    /// This should return the width of the widget's *content* (excluding margins and borders).
    fn content_width(&self) -> Option<usize> {
        None
    }
    /// Intrinsic content width used specifically for `width: auto` measurement
    /// (e.g. via `measure_intrinsic_content_width` for drained-container
    /// wrappers). Defaults to `content_width()`.
    ///
    /// Widgets whose `width: auto` should shrink-to-content but whose UNSET width
    /// must still flex-fill (Python's `1fr` default) override this WITHOUT
    /// reporting a `content_width()` hint — so a bare instance (unset width)
    /// fills, while an explicit `width: auto` (or an auto wrapper measuring it)
    /// sizes to content. Example: `Label`/`Static`.
    fn auto_content_width(&self) -> Option<usize> {
        self.content_width()
    }
    /// Intrinsic content height only. Size constraints from CSS/node styles are
    /// applied by layout callsites (which read `tree.styles(node).layout`) before
    /// falling back to this value.
    fn layout_height(&self) -> Option<usize> {
        None
    }
    /// Intrinsic content height used specifically for `height: auto` measurement
    /// (the height counterpart of [`auto_content_width`](Self::auto_content_width)).
    ///
    /// Defaults to `layout_height()` (the OUTER height). Widgets whose `height:
    /// auto` should shrink-to-content but whose UNSET height must still flex-fill
    /// (Python's container-fill default) override this WITHOUT reporting a
    /// `layout_height()` hint — so a bare instance (unset height) fills, while an
    /// explicit `height: auto` (or an auto wrapper measuring it) sizes to content.
    /// Example: `Placeholder` (unset height fills the column for the layout05
    /// Tweet stack; `height: auto` shrinks to the label's line count).
    fn auto_content_height(&self) -> Option<usize> {
        self.layout_height()
    }
    /// Behavior-derived style contribution (e.g. a widget computing `grid_rows`
    /// from its content model). User/inline styles live on the node record.
    fn style(&self) -> Option<Style> {
        None
    }
    /// Pre-mount inline style injection.
    ///
    /// Used by layout containers (e.g. Dock) to attach dock/size styles to a child
    /// before the child is mounted to the arena tree.  The default is a no-op; widgets
    /// that hold a NodeSeed override this to write into `seed.styles.style`.
    fn set_inline_style(&mut self, _style: Style) {}
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
    /// Optional text rendered on the top border when border-title styling is active.
    fn border_title(&self) -> Option<&str> {
        None
    }
    /// Optional text rendered on the bottom border when border-subtitle styling is active.
    fn border_subtitle(&self) -> Option<&str> {
        None
    }
    /// Component-class names this widget declares (Python `COMPONENT_CLASSES`).
    ///
    /// A component class is a sub-element style hook: CSS rules targeting
    /// `WidgetType .component--name` (or `WidgetType > .component--name`) let
    /// users/themes restyle internal parts of a custom widget. Declaring the
    /// class names here documents the public styling surface and mirrors
    /// Python's `COMPONENT_CLASSES: ClassVar[set[str]]`.
    ///
    /// The default is empty. Custom widgets that paint sub-elements via
    /// [`Widget::get_component_rich_style`] should override this.
    fn component_classes(&self) -> &[&'static str] {
        &[]
    }
    /// Resolve the CSS [`Style`] for a declared component class.
    ///
    /// Mirrors Python `Widget.get_component_styles(name)`: it resolves the
    /// stylesheet rules for `SelfType .name` against the current style context
    /// (so the widget's own pseudo-classes / ancestor context apply), combined
    /// with the widget's own resolved style as the base surface.
    ///
    /// This is the canonical, public entry point for custom widgets to read
    /// component-class colours/attributes from CSS instead of hardcoding them.
    fn get_component_styles(&self, name: &str) -> Style {
        crate::css::resolve_component_style(self, &[name])
    }
    /// Resolve a declared component class as a ready-to-paint `rich_rs::Style`.
    ///
    /// Mirrors Python `Widget.get_component_rich_style(name)`: resolve the
    /// component-class CSS, then convert to a Rich style (flattening any
    /// semi-transparent colours over the effective background). Returns `None`
    /// only when the resolved style carries no paintable attributes (no fg/bg
    /// and no text attributes) — equivalent to an empty Rich style.
    ///
    /// Custom widgets use this directly inside `render` / `render_line`:
    /// ```ignore
    /// let white = self.get_component_rich_style("checkerboard--white-square");
    /// ```
    fn get_component_rich_style(&self, name: &str) -> Option<rich_rs::Style> {
        self.get_component_styles(name).to_rich()
    }
    /// Type-meta-only styled render wrapper for off-tree (non-arena) rendering.
    ///
    /// Children rendered through this path have no DOM identity: the selector
    /// meta is built from type + dispatch-context state only. Reachable from
    /// container-internal rendering when children were not drained into the
    /// arena (unit-test/snapshot contexts).
    fn render_styled(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        self.render_styled_dyn_obj(console, options, None, NodeId::default())
    }
    /// Type-meta-only styled render wrapper with debug layout (see `render_styled`).
    fn render_styled_with_debug(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        debug: &DebugLayout,
    ) -> Segments {
        self.render_styled_dyn_obj(console, options, Some(debug), NodeId::default())
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

#[allow(clippy::too_many_arguments)]
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

    // Generic `text-align` propagation: feed the resolved CSS `text-align` into
    // the text renderer's justify so Label/Static (and any text widget) honor
    // `text-align: center|right|justify` exactly like Python Textual. Widgets
    // that need bespoke justify behavior may still override inside render().
    if let Some(text_align) = resolved.text_align {
        content_options.justify = Some(match text_align {
            crate::style::TextAlign::Left => rich_rs::JustifyMethod::Left,
            crate::style::TextAlign::Center => rich_rs::JustifyMethod::Center,
            crate::style::TextAlign::Right => rich_rs::JustifyMethod::Right,
            crate::style::TextAlign::Justify => rich_rs::JustifyMethod::Full,
        });
    }

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

    // Shared inner background for fill surfaces (content-align pad below and the
    // set_shape/CSS-pad fill further down).
    let fill_fallback_bg =
        crate::style::parse_color_like("$background").unwrap_or(crate::style::Color::rgb(0, 0, 0));
    let fill_parent_bg = crate::css::current_composited_background()
        .or_else(|| parent_style.clone().and_then(|s| s.bg))
        .unwrap_or(fill_fallback_bg);
    let fill_inner_bg = resolved
        .bg
        .map(|c| c.flatten_over(fill_parent_bg))
        .unwrap_or(fill_parent_bg);

    // Fill style that carries the resolved foreground over the inner background.
    // Mirrors Python's `visual_style.rich_style` (color = background + $foreground,
    // or auto-contrast for `color: auto`). Used for BOTH content-align padding
    // (visual.py `Strip.align`) and the vertical extend beyond content (widget.py
    // `render_line` IndexError fallback `Strip.blank(width, visual_style.rich_style)`).
    let fill_fg_style = {
        let mut s = rich_rs::Style::new().with_bgcolor(fill_inner_bg.to_simple_opaque());
        if let Some(fg) = resolved.fg {
            s = s.with_color(fg.flatten_over(fill_inner_bg).to_simple_opaque());
        } else if let Some(auto) = resolved.fg_auto {
            let contrast = crate::style::contrast_text(fill_inner_bg)
                .blend_over_float(fill_inner_bg, auto.alpha());
            s = s.with_color(contrast.to_simple_opaque());
        }
        s
    };

    // Only run the alignment fill for a NON-default content-align. Python's
    // `_visual_to_strips` guards `Strip.align` with `if content_align !=
    // ("left", "top")`: for the default the content stays top-left and the
    // trailing space comes from the background-only `adjust_cell_length` /
    // `inner.rich_style` extend (fg = default), NOT the fg-bearing align pad.
    if let Some(content_align) = resolved
        .content_align
        .filter(|ca| !(ca.horizontal == HorizontalAlign::Left && ca.vertical == VerticalAlign::Top))
    {
        // Content-align padding carries the resolved fg (Strip.align semantics).
        let align_pad = fill_fg_style;
        lines = apply_content_alignment(
            lines,
            content_width + line_pad * 2,
            content_height,
            content_align.horizontal,
            content_align.vertical,
            align_pad,
        );
    }

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
        // Two distinct fill surfaces, mirroring Python:
        //  - Trailing HORIZONTAL pad of a content row is BACKGROUND-ONLY
        //    (_styles_cache `adjust_cell_length(content_width, inner.rich_style)`,
        //    where `inner` = Style(background=...) has no fg → default fg).
        //  - VERTICAL extend beyond the content rows carries the resolved fg
        //    (widget.py `render_line` IndexError → Strip.blank(width,
        //    visual_style.rich_style)`, where visual_style includes $foreground).
        let inner_bg = fill_inner_bg;
        let fill = rich_rs::Style::new().with_bgcolor(inner_bg.to_simple_opaque());
        let pad_fill = fill;
        let fill_width = content_width + line_pad * 2;
        // Horizontal pad of existing content rows: background-only.
        let mut shaped: Vec<Vec<rich_rs::Segment>> = lines
            .iter()
            .take(content_height)
            .map(|line| rich_rs::Segment::adjust_line_length(line, fill_width, Some(fill), true))
            .collect();
        // Vertical extend rows: which fill style to use depends on which Python
        // surface this widget's blank rows correspond to.
        //
        //  - CONTENT widgets (Static/Label, etc.) render their text Visual. When
        //    the widget is TALLER than its content lines, Python's
        //    `widget.render_line` hits `IndexError` and fills with
        //    `Strip.blank(width, visual_style.rich_style)` — visual_style carries
        //    `$foreground`. So those extend rows are FG-bearing.
        //
        //  - CHROME-ONLY CONTAINERS (Container/Vertical/Horizontal, etc.) do not
        //    render text content; Python's `Widget.render` returns
        //    `Blank(self.background_colors[1])`, a BG-ONLY visual with no
        //    foreground. Their whole interior (including the vertical extend) is
        //    therefore BG-ONLY, even though `color` is inherited from an ancestor
        //    (`Screen { color: $foreground }`) and is present in `resolved.fg`.
        //    (This matches the `inner.rich_style` / `get_inner_outer` bg-only
        //    style; see `_styles_cache.render_line`.)
        //
        // `segments_empty` is the in-render discriminator: a chrome-only
        // container produces no content segments (its render returns empty / only
        // surface chrome), whereas a content widget produces at least one content
        // line. So carry fg into the extend rows ONLY for content widgets.
        // fg and fg_auto are a linked pair (explicit color vs `color: auto`). A
        // content widget with EITHER set has a fg-bearing visual_style, so its
        // vertical-extend rows carry that color (auto-contrast for `color: auto`,
        // already baked into fill_fg_style). Checking only `resolved.fg` missed
        // the `color: auto` case (e.g. text_align), dropping fg on extend rows.
        let vfill_style = if !segments_empty
            && (resolved.fg.is_some() || resolved.fg_auto.is_some())
        {
            fill_fg_style // content widget with explicit/auto fg → visual_style extend
        } else {
            fill // chrome-only container (or no fg) → bg-only extend (Blank/inner.rich_style)
        };
        let vfill_blank =
            vec![rich_rs::Segment::styled(" ".repeat(fill_width), vfill_style)];
        while shaped.len() < content_height {
            shaped.push(vfill_blank.clone());
        }
        lines = shaped;

        // Apply left/right padding.
        let pad_left = padding.left as usize;
        let pad_right = padding.right as usize;
        let mut padded_lines: Vec<Vec<rich_rs::Segment>> = Vec::with_capacity(lines.len());
        for line in lines {
            let mut out = Vec::new();
            if pad_left > 0 {
                out.push(rich_rs::Segment::styled(" ".repeat(pad_left), pad_fill));
            }
            out.extend(line);
            if pad_right > 0 {
                out.push(rich_rs::Segment::styled(" ".repeat(pad_right), pad_fill));
            }
            padded_lines.push(out);
        }

        // Apply top/bottom padding.
        let pad_top = padding.top as usize;
        let pad_bottom = padding.bottom as usize;
        let padded_width = content_width + line_pad * 2 + pad_left + pad_right;
        if pad_top > 0 {
            let blank = vec![rich_rs::Segment::styled(" ".repeat(padded_width), pad_fill)];
            for _ in 0..pad_top {
                final_lines.push(blank.clone());
            }
        }
        final_lines.extend(padded_lines);
        if pad_bottom > 0 {
            let blank = vec![rich_rs::Segment::styled(" ".repeat(padded_width), pad_fill)];
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
        resolved.opacity,
    );
    let segments = if let Some(opacity) = resolved.opacity {
        crate::css::apply_widget_opacity_to_segments(segments, opacity, parent_style)
    } else {
        segments
    };
    tag_widget_meta(node_id, segments)
}

fn apply_content_alignment(
    lines: Vec<Vec<rich_rs::Segment>>,
    content_width: usize,
    content_height: usize,
    horizontal: HorizontalAlign,
    vertical: VerticalAlign,
    pad_style: rich_rs::Style,
) -> Vec<Vec<rich_rs::Segment>> {
    // Content-align padding (both axes) is painted with the full visual style
    // (fg = resolved color), matching Python Textual's `Strip.align`, which pads
    // alignment space with the widget's rich_style (visual.py `to_strips`). This
    // is distinct from non-aligned trailing space, which is `to_strip`
    // background-only padding.
    let pad_segment =
        |width: usize| rich_rs::Segment::styled(" ".repeat(width), pad_style);
    let mut aligned: Vec<Vec<rich_rs::Segment>> = Vec::with_capacity(lines.len());
    for mut line in lines {
        let has_synthetic_padding = line.iter().any(|segment| {
            segment
                .meta
                .as_ref()
                .and_then(|meta| meta.meta.as_ref())
                .and_then(|meta| meta.get("textual:no_text_style"))
                .is_some_and(|value| matches!(value, MetaValue::Bool(true)))
        });
        let line_width = if has_synthetic_padding {
            rich_rs::Segment::get_line_length(&line).min(content_width)
        } else {
            let line_text = line
                .iter()
                .map(|segment| segment.text.as_ref())
                .collect::<String>();
            rich_rs::cell_len(line_text.trim_end()).min(content_width)
        };
        let left_pad = match horizontal {
            HorizontalAlign::Left => 0,
            HorizontalAlign::Center => content_width.saturating_sub(line_width) / 2,
            HorizontalAlign::Right => content_width.saturating_sub(line_width),
        };
        if left_pad > 0 {
            line.insert(0, pad_segment(left_pad));
        }
        // Right-extend with the same fg-carrying pad style (not no_text_style),
        // so the trailing alignment space matches Python's Strip.align fg.
        let cur = rich_rs::Segment::get_line_length(&line);
        if cur < content_width {
            line.push(pad_segment(content_width - cur));
        } else if cur > content_width {
            line = rich_rs::Segment::adjust_line_length(&line, content_width, None, false);
        }
        aligned.push(line);
    }

    if aligned.len() >= content_height {
        aligned.truncate(content_height);
        return aligned;
    }

    let extra_rows = content_height.saturating_sub(aligned.len());
    let top_pad = match vertical {
        VerticalAlign::Top => 0,
        VerticalAlign::Middle => extra_rows / 2,
        VerticalAlign::Bottom => extra_rows,
    };
    let bottom_pad = extra_rows - top_pad;

    // Pad BOTH top and bottom to fill content_height here, so every alignment
    // blank row carries the fg-bearing pad style (Strip.align). If we left the
    // bottom to set_shape, those rows would be background-only and mismatch
    // Python (which paints all alignment space with the widget rich_style).
    let blank_row = vec![pad_segment(content_width)];
    let mut out = Vec::with_capacity(content_height);
    for _ in 0..top_pad {
        out.push(blank_row.clone());
    }
    out.extend(aligned);
    for _ in 0..bottom_pad {
        out.push(blank_row.clone());
    }
    out
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

/// Framework-owned per-node interaction state. Lives on the arena node record;
/// widgets read it via `Widget::node_state()` (dispatch context), never store it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NodeState {
    pub focused: bool,
    pub hovered: bool,
    pub disabled: bool,
    pub loading: bool,
}

/// Compose-time CSS id/class metadata for a child, paired with its index in the
/// most recent `take_composed_children()` result.
///
/// `(child_index, css_id, classes)`. Produced by `Widget::take_child_decl_meta`
/// and applied to the mounted node by the runtime mount path (see
/// `apply_child_decl_meta`), so `ChildDecl`-carried `.with_id()`/`.with_classes()`
/// metadata reaches the tree even though children are mounted via the
/// `take_composed_children()` extraction path.
pub type ChildDeclMeta = (usize, Option<String>, Vec<String>);

/// Apply compose-time id/class metadata (from `take_child_decl_meta`) to a freshly
/// mounted node, mirroring what `App::mount_declarations` does for `ChildDecl`s.
pub(crate) fn apply_child_decl_meta(
    tree: &mut crate::widget_tree::WidgetTree,
    node_id: NodeId,
    css_id: Option<String>,
    classes: &[String],
) {
    if let Some(id) = css_id {
        tree.set_css_id(node_id, Some(id));
    }
    for class in classes {
        tree.add_class(node_id, class);
    }
}

/// One-shot identity/style payload set by widget builder methods before mount
/// and consumed exactly once by `WidgetTree::mount`/`set_root`. Not live state:
/// after mount the node record is the single source of truth.
#[derive(Debug, Clone, Default)]
pub struct NodeSeed {
    pub css_id: Option<String>,
    pub classes: Vec<String>,
    pub styles: WidgetStyles,
}

/// Generate the canonical `.id()` / `.class()` builder methods for a widget that
/// owns a `seed: NodeSeed` field. Mirrors Python, where every widget accepts
/// `id=` / `classes=`; both set the widget's OWN seed (a single node), so a
/// type selector and an id/class selector resolve to the same widget — unlike
/// wrapping in a `Node`, which splits them across two nodes.
///
/// Invoke inside the widget's inherent `impl` block (same module, so the private
/// `seed` field is accessible): `crate::seed_ident_methods!();`
#[macro_export]
macro_rules! seed_ident_methods {
    () => {
        /// Set this widget's CSS id (Python `id=`).
        pub fn id(mut self, value: impl ::std::convert::Into<String>) -> Self {
            self.seed.css_id = Some(value.into());
            self
        }
        /// Add a CSS class (Python `classes=`). Idempotent.
        pub fn class(mut self, value: impl ::std::convert::Into<String>) -> Self {
            let v = value.into();
            if !self.seed.classes.iter().any(|c| c == &v) {
                self.seed.classes.push(v);
            }
            self
        }
    };
}

/// Like [`seed_ident_methods!`] but for a thin wrapper widget that delegates its
/// identity to an inner field (which itself exposes `.id()`/`.class()`).
#[macro_export]
macro_rules! delegate_ident_methods {
    ($field:ident) => {
        /// Set this widget's CSS id (delegated to the inner widget).
        pub fn id(mut self, value: impl ::std::convert::Into<String>) -> Self {
            self.$field = self.$field.id(value);
            self
        }
        /// Add a CSS class (delegated to the inner widget). Idempotent.
        pub fn class(mut self, value: impl ::std::convert::Into<String>) -> Self {
            self.$field = self.$field.class(value);
            self
        }
    };
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

    /// Compare with another set of widget styles and classify the change
    /// for invalidation purposes.
    pub fn invalidation_kind(&self, other: &WidgetStyles) -> StyleChangeKind {
        classify_style_change(&self.style, &other.style)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node_id::NodeId;
    use crate::runtime::dispatch_ctx::{dispatch_recipient, set_dispatch_recipient};
    use crate::style::{Display, Layout, Overflow, Visibility};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

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
        let _guard = set_dispatch_recipient(id, NodeState::default());
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
        let _outer = set_dispatch_recipient(id_a, NodeState::default());
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

    fn segments_text(segments: &Segments) -> String {
        segments
            .iter()
            .map(|seg| seg.text.to_string())
            .collect::<String>()
    }

    struct DefaultLineProbe;
    impl Widget for DefaultLineProbe {
        fn render(
            &self,
            _console: &rich_rs::Console,
            _options: &rich_rs::ConsoleOptions,
        ) -> rich_rs::Segments {
            vec![rich_rs::Segment::new("alpha\nbeta".to_string())].into()
        }
    }

    #[test]
    fn render_line_default_extracts_requested_row() {
        let widget = DefaultLineProbe;
        let console = rich_rs::Console::new();
        let mut options = rich_rs::ConsoleOptions::default();
        options.size = (16, 2);
        options.max_width = 16;
        options.max_height = 2;

        let line0 = widget.render_line(0, &console, &options);
        let line1 = widget.render_line(1, &console, &options);
        assert_eq!(segments_text(&line0).trim_end(), "alpha");
        assert_eq!(segments_text(&line1).trim_end(), "beta");
    }

    struct CustomLineProbe {
        calls: Arc<AtomicUsize>,
    }
    impl Widget for CustomLineProbe {
        fn render(
            &self,
            _console: &rich_rs::Console,
            _options: &rich_rs::ConsoleOptions,
        ) -> rich_rs::Segments {
            rich_rs::Segments::new()
        }

        fn render_line(
            &self,
            y: usize,
            _console: &rich_rs::Console,
            _options: &rich_rs::ConsoleOptions,
        ) -> rich_rs::Segments {
            self.calls.fetch_add(1, Ordering::SeqCst);
            vec![rich_rs::Segment::new(format!("line-{y}"))].into()
        }
    }

    #[test]
    fn render_lines_default_delegates_to_render_line() {
        let calls = Arc::new(AtomicUsize::new(0));
        let widget = CustomLineProbe {
            calls: calls.clone(),
        };
        let console = rich_rs::Console::new();
        let mut options = rich_rs::ConsoleOptions::default();
        options.size = (16, 3);
        options.max_width = 16;
        options.max_height = 3;

        let lines = widget.render_lines(2, 3, &console, &options);
        assert_eq!(calls.load(Ordering::SeqCst), 3);
        assert_eq!(segments_text(&lines[0]), "line-2");
        assert_eq!(segments_text(&lines[1]), "line-3");
        assert_eq!(segments_text(&lines[2]), "line-4");
    }
}
