//! Authoring-side capability traits — the FROZEN public surface a widget author
//! implements, split out of the 83-method [`Widget`](super::Widget) trait.
//!
//! # The split (Widget trait split — 1.0 freeze shape)
//!
//! `Widget` remains the single **object-safe dispatch trait** the runtime calls
//! through `Box<dyn Widget>`; its method list is UNCHANGED and stays evolvable
//! behind the derive. The **authoring** surface, however, is split into a small
//! required core ([`Render`]) plus opt-in capability traits. A from-scratch leaf
//! widget implements [`Render`] (often just `render`) and, only when it needs
//! them, one or more capability traits — instead of staring down 83 methods.
//!
//! These traits are wired onto `Widget` by the `#[widget(..)]` attribute macro
//! (own-widget mode): it generates `impl Widget for YourType` that forwards each
//! opted-in capability's methods to your capability-trait impl and lets every
//! other method fall through to the `Widget` default.
//!
//! ```ignore
//! #[widget(Layout)]                 // opt into the Layout capability
//! struct Spacer { height: usize, seed: NodeSeed }
//!
//! impl Render for Spacer {
//!     fn render(&self, _c: &Console, o: &ConsoleOptions) -> Segments { /* ... */ }
//! }
//! impl Layout for Spacer {
//!     fn layout_height(&self) -> Option<usize> { Some(self.height) }
//! }
//! ```
//!
//! # LOUD authoring rule (read this)
//!
//! To make a capability's methods actually run, you MUST do BOTH:
//!   1. `impl <Capability> for YourType { .. }`, AND
//!   2. list `<Capability>` in the `#[widget(..)]` attribute.
//!
//! If you implement a capability trait but FORGET to list it in `#[widget(..)]`,
//! the generated `Widget` impl silently keeps the DEFAULT behavior for those
//! methods and your capability code never runs. The reverse mistake — listing a
//! capability you didn't implement — is a normal compile error. Only the
//! forgot-to-list case is silent, so double-check the attribute list.
//
// FUTURE: a lint / compile-time check could flag "capability trait implemented
// but not listed in #[widget(..)]" (the derive cannot see trait impls today).

use rich_rs::{Console, ConsoleOptions, Segments};

use crate::action::{ActionDecl, ParsedAction};
use crate::compose::ComposeResult;
use crate::event::{Action, BindingHint, Event, WidgetCtx};
use crate::message::MessageEvent;
use crate::style::Style;

use super::{BindingDecl, NodeState, Widget, WidgetSelectionAnchor};

// ── Render (REQUIRED core) ─────────────────────────────────────────────

/// The required core authoring trait: how a widget produces content.
///
/// Implement `render` (a leaf) **or** `compose` (a container). Both have
/// defaults — a widget that implements neither renders blank (Python-faithful).
/// `style_type` defaults to the concrete type's short name.
pub trait Render {
    /// Render this widget's content. Default: empty (implement this OR `compose`).
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let _ = (console, options);
        Segments::new()
    }

    /// Declare child widgets (the single child-declaration path). Default: leaf.
    fn compose(&mut self) -> ComposeResult {
        Vec::new()
    }

    /// Render a single visual line at `y` in widget-local coordinates.
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

    /// Render with a debug-layout overlay. Default delegates to `render`.
    fn render_with_debug(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        _debug: &crate::debug::DebugLayout,
    ) -> Segments {
        self.render(console, options)
    }

    /// CSS type name for this widget (default: concrete type short name).
    fn style_type(&self) -> &'static str {
        super::short_type_name::<Self>()
    }

    /// Optional super-type aliases used for CSS type selector matching.
    fn style_type_aliases(&self) -> &[&'static str] {
        &[]
    }

    /// Optional text rendered on the top border.
    fn border_title(&self) -> Option<&str> {
        None
    }

    /// Optional text rendered on the bottom border.
    fn border_subtitle(&self) -> Option<&str> {
        None
    }
}

// ── Interactive (events + lifecycle) ───────────────────────────────────

/// Event handling and lifecycle hooks. Opt in with `#[widget(Interactive)]`.
pub trait Interactive {
    /// Mount hook, called once when the node is mounted.
    fn on_mount(&mut self, _ctx: &mut WidgetCtx) {}
    fn on_unmount(&mut self) {}
    fn on_tick(&mut self, _tick: u64) {}
    fn on_resize(&mut self, _width: u16, _height: u16) {}
    fn on_layout(&mut self, _width: u16, _height: u16) {}
    fn on_event_capture(&mut self, _event: &Event, _ctx: &mut WidgetCtx) {}
    fn on_event(&mut self, _event: &Event, _ctx: &mut WidgetCtx) {}
    fn on_message(&mut self, _message: &MessageEvent, _ctx: &mut WidgetCtx) {}
    fn on_mouse_move(&mut self, _x: u16, _y: u16) -> bool {
        false
    }
    /// Notification hook invoked after this node's interaction state changed.
    fn on_node_state_changed(&mut self, _old: NodeState, _new: NodeState) {}
}

// ── Layout (sizing + tree-layout participation) ────────────────────────

/// Intrinsic sizing and tree-layout participation. Opt in with `#[widget(Layout)]`.
///
/// NOTE: this is the widget authoring trait; it is unrelated to the
/// `layout:` CSS property enum `crate::style::Layout`.
pub trait Layout {
    /// Intrinsic content width hint (cells), used when `width: auto`.
    fn content_width(&self) -> Option<usize> {
        None
    }
    /// Intrinsic content width for `width: auto` measurement (defaults to
    /// `content_width()`).
    fn auto_content_width(&self) -> Option<usize> {
        self.content_width()
    }
    /// Intrinsic content height only.
    fn layout_height(&self) -> Option<usize> {
        None
    }
    /// Intrinsic content height for `height: auto` measurement (defaults to
    /// `layout_height()`).
    fn auto_content_height(&self) -> Option<usize> {
        self.layout_height()
    }
    /// Inform scroll host widgets of their virtual content size.
    fn set_virtual_content_size(&mut self, _width: usize, _height: usize) {}
    /// Extra insets reserved before laying out tree children `(top,right,bottom,left)`.
    fn tree_child_content_inset(&self) -> (u16, u16, u16, u16) {
        (0, 0, 0, 0)
    }
    /// Optional visibility override for tree child nodes by child index.
    fn child_display_for_tree(&self, _child_index: usize) -> Option<bool> {
        None
    }
    /// Optional per-child CSS class overrides driven by this widget's state.
    fn child_classes_for_tree(&self, _child_index: usize) -> Vec<(&'static str, bool)> {
        Vec::new()
    }
    /// Whether this widget is a transparent styling wrapper.
    fn is_transparent_wrapper(&self) -> bool {
        false
    }
    /// Whether styled rendering should preserve underlying frame cells.
    fn preserve_underlay(&self) -> bool {
        false
    }
    /// Whether descendant rendering should be clipped to this widget's content box.
    fn clips_descendants_to_content(&self) -> bool {
        false
    }
    /// Behavior-derived style contribution.
    fn style(&self) -> Option<Style> {
        None
    }
}

// ── Scrollable ─────────────────────────────────────────────────────────

/// Scroll behavior. Opt in with `#[widget(Scrollable)]`.
pub trait Scrollable {
    /// Content scroll offset applied to descendants during render.
    fn scroll_offset(&self) -> (usize, usize) {
        (0, 0)
    }
    /// Content scroll offset (float precision).
    fn scroll_offset_f32(&self) -> (f32, f32) {
        let (x, y) = self.scroll_offset();
        (x as f32, y as f32)
    }
    /// Effective visible scroll viewport size `(width, height)`.
    fn scroll_viewport_size(&self) -> Option<(usize, usize)> {
        None
    }
    /// Virtual scrollable content size `(width, height)`.
    fn scroll_virtual_content_size(&self) -> Option<(usize, usize)> {
        None
    }
    /// Mouse wheel / touchpad scroll input.
    fn on_mouse_scroll(&mut self, _delta_x: i32, _delta_y: i32, _ctx: &mut WidgetCtx) {}
}

// ── Focus (focus + bindings + actions) ─────────────────────────────────

/// Focus state, key bindings, and actions. Opt in with `#[widget(Focus)]`.
pub trait Focus {
    fn focusable(&self) -> bool {
        false
    }
    /// Whether this widget type can receive focus (ignoring disabled state).
    fn can_focus(&self) -> bool {
        self.focusable()
    }
    /// Whether this widget allows focus traversal into descendant widgets.
    fn can_focus_children(&self) -> bool {
        true
    }
    /// Whether the widget is interactive for mouse hover / cursor feedback.
    fn mouse_interactive(&self) -> bool {
        self.focusable()
    }
    /// Whether the widget is active (e.g. pressed/dragging).
    fn is_active(&self) -> bool {
        false
    }
    /// Initial disabled state for off-tree rendering and tree-mount init.
    fn is_initially_disabled(&self) -> bool {
        false
    }
    /// Whether this widget should be treated as focused from the first render.
    fn is_initially_focused(&self) -> bool {
        false
    }
    /// Declarative key bindings for this widget.
    fn bindings(&self) -> Vec<BindingDecl> {
        Vec::new()
    }
    /// Key-binding hints exposed by this widget.
    fn binding_hints(&self) -> Vec<BindingHint> {
        Vec::new()
    }
    /// The namespace this widget owns for action routing.
    fn action_namespace(&self) -> &str {
        ""
    }
    /// List of actions this widget can handle.
    fn action_registry(&self) -> &[ActionDecl] {
        &[]
    }
    /// Execute a parsed action. Returns `true` if handled.
    fn execute_action(&mut self, _action: &ParsedAction, _ctx: &mut WidgetCtx) -> bool {
        false
    }
    /// Gate whether an action may run on this widget.
    fn check_action(
        &self,
        _action: &str,
        _parameters: &[crate::action::ActionArgument],
    ) -> Option<bool> {
        Some(true)
    }
    /// Optional focused HELP markup exposed to framework-level help panels.
    fn help_markup(&self) -> Option<&str> {
        None
    }
}

// ── Selectable (screen-level text selection) ───────────────────────────

/// Screen-level text selection participation. Opt in with `#[widget(Selectable)]`.
pub trait Selectable {
    fn allow_select(&self) -> bool {
        false
    }
    fn selection_at(&self, _x: u16, _y: u16) -> Option<WidgetSelectionAnchor> {
        None
    }
    fn selection_word_range_at(
        &self,
        _x: u16,
        _y: u16,
    ) -> Option<(WidgetSelectionAnchor, WidgetSelectionAnchor)> {
        None
    }
    fn selection_all_range(&self) -> Option<(WidgetSelectionAnchor, WidgetSelectionAnchor)> {
        None
    }
    fn update_selection(
        &mut self,
        _from: WidgetSelectionAnchor,
        _to: WidgetSelectionAnchor,
    ) -> bool {
        false
    }
    fn clear_selection(&mut self) -> bool {
        false
    }
    fn get_selection(&self) -> Option<String> {
        None
    }
    fn selection_updated(&mut self, _ctx: &mut WidgetCtx) {}
}

// ── HasTooltip ─────────────────────────────────────────────────────────

/// Tooltip provider. Opt in with `#[widget(HasTooltip)]`.
///
/// Named `HasTooltip` to avoid colliding with the `Tooltip` widget.
pub trait HasTooltip {
    /// Optional tooltip text for hover overlays.
    fn tooltip(&self) -> Option<String> {
        None
    }
    /// Optional tooltip anchor point in content-local coordinates.
    fn tooltip_anchor(&self) -> Option<(u16, u16)> {
        None
    }
}

// ── StyleIdentity (dynamic off-tree CSS identity + seed identity) ───────

/// Off-tree CSS identity for DYNAMIC-identity widgets — those that compute a
/// LIVE class list / id / hover flag rather than storing them in a `seed:
/// NodeSeed` field. Opt in with `#[widget(StyleIdentity)]`.
///
/// `style_classes` / `style_id` / `is_hovered` are consulted ONLY off-tree (when
/// no dispatch context is active — e.g. during `layout_height` measurement or a
/// direct `FrameBuffer::from_renderable` render). After mount the arena node
/// record is the single source of truth. `set_seed_css_id` / `set_seed_classes`
/// propagate a compose-declared id/classes (`ChildDecl::with_id(..)`) into the
/// widget's own `NodeSeed` before mount.
///
/// Widgets whose identity IS seed-backed do NOT need this trait — the
/// `#[widget(..)]` seed-field autowiring already handles `take_node_seed` /
/// `set_inline_style`, and `style_classes` / `style_id` default to the seed via
/// `crate::seed_style_identity_methods!`. Use `StyleIdentity` when the widget
/// keeps a live class list (e.g. `Button`, `Input`, `DataTable`) that is not a
/// simple view of `seed.classes`.
pub trait StyleIdentity {
    /// CSS classes for off-tree style resolution.
    fn style_classes(&self) -> &[String] {
        &[]
    }
    /// CSS id for off-tree style resolution.
    fn style_id(&self) -> Option<&str> {
        None
    }
    /// Hover state for off-tree style resolution.
    fn is_hovered(&self) -> bool {
        false
    }
    /// Pre-mount CSS-id injection (propagate a `ChildDecl`-declared id into seed).
    fn set_seed_css_id(&mut self, _id: Option<String>) {}
    /// Pre-mount CSS-class injection (companion to `set_seed_css_id`).
    fn set_seed_classes(&mut self, _classes: Vec<String>) {}

    // Seed lifecycle. Opting `StyleIdentity` takes FULL ownership of the widget's
    // seed surface (the `#[widget(..)]` seed-field autowiring is suppressed), so a
    // `StyleIdentity` widget with a `seed: NodeSeed` field MUST provide these two
    // (usually the canonical bodies below; override `take_node_seed` when it has a
    // side effect, e.g. `Button` caching its css id). This is why `StyleIdentity`
    // exists: dynamic-identity widgets own the whole identity+seed surface.

    /// Consume the one-shot mount seed. Default drops it — a seed-owning widget
    /// MUST override (canonically `std::mem::take(&mut self.seed)`).
    fn take_node_seed(&mut self) -> super::NodeSeed {
        super::NodeSeed::default()
    }
    /// Pre-mount inline style injection. Default no-op — a seed-owning widget
    /// overrides (canonically `self.seed.styles.style = style`).
    fn set_inline_style(&mut self, _style: Style) {}
}

// ── Components (component-class styling hooks) ──────────────────────────

/// Component-class styling hooks (Python `COMPONENT_CLASSES`). Opt in with
/// `#[widget(Components)]`.
///
/// Requires `Self: Widget` because the default `get_component_styles` resolves
/// the widget's own style context.
pub trait Components: Widget {
    /// Component-class names this widget declares.
    fn component_classes(&self) -> &[&'static str] {
        &[]
    }
    /// Resolve the CSS [`Style`] for a declared component class.
    fn get_component_styles(&self, name: &str) -> Style {
        super::core::debug_component_class_declared(
            self.style_type(),
            Components::component_classes(self),
            name,
        );
        crate::css::resolve_component_style(self, &[name])
    }
    /// Resolve a declared component class as a ready-to-paint `rich_rs::Style`
    /// (composited over the widget's effective painted surface).
    fn get_component_rich_style(&self, name: &str) -> Option<rich_rs::Style> {
        let style = Components::get_component_styles(self, name);
        let surface = crate::css::component_surface_bg(self);
        crate::css::component_style_to_rich(&style, surface)
    }
}

// ── AppHooks (advanced app-wrapper hooks) ──────────────────────────────

/// Advanced runtime-level app hooks (only app wrappers/adapters override these).
/// Opt in with `#[widget(AppHooks)]`.
pub trait AppHooks {
    fn on_app_key(
        &mut self,
        _app: &mut crate::App,
        _key: &crate::keys::KeyEventData,
        _ctx: &mut WidgetCtx,
    ) {
    }
    fn on_app_action(&mut self, _app: &mut crate::App, _action: Action, _ctx: &mut WidgetCtx) {}
    fn on_app_unhandled_action(
        &mut self,
        _app: &mut crate::App,
        _action: &str,
        _ctx: &mut WidgetCtx,
    ) {
    }
    fn on_app_message(
        &mut self,
        _app: &mut crate::App,
        _message: &MessageEvent,
        _ctx: &mut WidgetCtx,
    ) {
    }
    fn on_app_tick(&mut self, _app: &mut crate::App, _tick: u64, _ctx: &mut WidgetCtx) {}
    fn on_app_timer(&mut self, _app: &mut crate::App, _ctx: &mut WidgetCtx) {}
    fn on_app_mount(&mut self, _app: &mut crate::App, _ctx: &mut WidgetCtx) {}
}
