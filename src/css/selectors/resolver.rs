use crate::node_id::NodeId;
use crate::style::{Display, Style, Visibility};
use crate::widget_tree::{WidgetNode, WidgetTree};
use crate::widgets::Widget;

use super::ast::{SelectorMeta, SelectorStates, StyleSheet};
use super::context::{
    COMPUTED_STYLE_CACHE, SELECTOR_STACK, STYLE_CONTEXT, STYLE_STACK, app_is_active,
    app_runtime_pseudos, is_focus_within,
};
use super::debug::{style_debug_matches, style_debug_meta_label, style_debug_summary};
use super::matching::rule_specificity;

fn widget_is_screen<T: Widget + ?Sized>(widget: &T) -> bool {
    widget.style_type() == "Screen"
        || widget
            .style_type_aliases().contains(&"Screen")
}

/// Interaction states for type-only/off-tree meta, sourced from the dispatch
/// context (the widget no longer owns state). Screen rule: an active screen
/// behaves as `:focus` while the app is active (Python parity).
///
/// Off-tree fallback: when no dispatch context is active (e.g. inside
/// `layout_height()` or a `FrameBuffer::from_renderable` render), the widget's
/// own `is_hovered()` override is consulted so that per-widget state (e.g.
/// `FooterKey.hovered`) participates in CSS resolution.
fn dispatch_states<T: Widget + ?Sized>(widget: &T) -> SelectorStates {
    let pseudos = app_runtime_pseudos();
    let ctx_state = crate::runtime::dispatch_ctx::dispatch_node_state();
    let state = ctx_state.unwrap_or_default();
    let focused = (state.focused || widget_is_screen(widget)) && app_is_active();
    // When dispatch context is absent, fall back to the widget's own hover signal.
    let hovered = if ctx_state.is_some() {
        state.hovered
    } else {
        widget.is_hovered()
    };
    SelectorStates {
        disabled: state.disabled,
        focused,
        hovered,
        active: widget.is_active(),
        dark: pseudos.dark,
        inline: pseudos.inline,
        ansi: pseudos.ansi,
        nocolor: pseudos.nocolor,
        can_focus: widget.can_focus(),
        ..Default::default()
    }
}

impl StyleSheet {
    pub(super) fn style_for<T: Widget + ?Sized>(&self, _widget: &T, meta: &SelectorMeta) -> Style {
        self.style_for_meta(meta)
    }

    pub(super) fn style_for_meta(&self, meta: &SelectorMeta) -> Style {
        // Cascade key mirrors Python `Styles.extract_rules`: user CSS outranks
        // widget DEFAULT_CSS before specificity is even considered.
        let mut matches: Vec<((u8, u8, usize), Style)> = Vec::new();
        let debug_style_meta = style_debug_matches(meta);
        for (idx, rule) in self.rules.iter().enumerate() {
            if let Some(score) = rule_specificity(rule, meta) {
                let layer = if rule.is_default { 0 } else { 1 };
                matches.push(((layer, score, idx), rule.style.clone()));
                if debug_style_meta {
                    crate::debug::debug_style(&format!(
                        "[style] match widget={} selector=\"{}\" score={} rule={}",
                        style_debug_meta_label(meta),
                        super::debug::selector_chain_string(&rule.selector_chain),
                        score,
                        style_debug_summary(&rule.style),
                    ));
                }
            }
        }
        matches.sort_by(|a, b| a.0.cmp(&b.0));
        let mut out = Style::new();
        for (_, style) in matches {
            out = out.combine(&style);
        }
        if debug_style_meta {
            let stack = SELECTOR_STACK.with(|stack| {
                stack
                    .borrow()
                    .iter()
                    .map(style_debug_meta_label)
                    .collect::<Vec<_>>()
            });
            crate::debug::debug_style(&format!(
                "[style] resolved widget={} stack={:?} style={}",
                style_debug_meta_label(meta),
                stack,
                style_debug_summary(&out),
            ));
        }
        out
    }
}

/// Type-only selector meta for off-tree paths (component styles, legacy
/// widget-to-widget rendering, resolver unit tests). DOM identity (id/classes)
/// lives on the node record in tree-mode; for off-tree use the widget's
/// `style_classes()` override is consulted so that component-style rules like
/// `Button.-primary { ... }` and `Input.command-palette--input > .input--placeholder`
/// resolve correctly without an active arena node.
pub(crate) fn selector_meta_generic<T: Widget + ?Sized>(widget: &T) -> SelectorMeta {
    SelectorMeta {
        type_name: widget.style_type().to_string(),
        type_aliases: widget
            .style_type_aliases()
            .iter()
            .map(|name| (*name).to_string())
            .collect(),
        id: widget.style_id().map(|s| s.to_string()),
        classes: widget.style_classes().to_vec(),
        states: dispatch_states(widget),
        component_phantom: false,
    }
}

/// Selector meta for a node's COVER widget (Python `Widget._cover`).
///
/// Python `Widget.set_loading` adds the `-textual-loading-indicator` class to
/// the loading widget before covering, so the
/// `LoadingIndicator.-textual-loading-indicator { … }` default rule applies.
pub(crate) fn cover_selector_meta<T: Widget + ?Sized>(widget: &T) -> SelectorMeta {
    let mut meta = selector_meta_generic(widget);
    let class = "-textual-loading-indicator";
    if !meta.classes.iter().any(|c| c == class) {
        meta.classes.push(class.to_string());
    }
    meta
}

/// The component-class virtual node (Python's typeless `DOMNode(classes=...)`
/// from `Stylesheet._process_component_classes`).
///
/// Identity mirrors Python exactly:
/// - NO type name and `component_phantom = true`: no type selector matches it,
///   including the `Widget` universal special case (matching.rs).
/// - NO id.
/// - classes = the component name(s) under resolution.
/// - states: global runtime pseudos only (`:dark`/`:inline`/`:ansi`/`:nocolor`),
///   never the widget's interaction states — `.part:hover` must not match a
///   hovered widget's part, while the NEGATIVE pseudos `.part:blur` /
///   `.part:light` keep matching (stateless-node semantics, same as Python).
///   Widget-state styling flows through the live parent meta on
///   `SELECTOR_STACK` (`&:focus > .part { ... }`).
pub(crate) fn selector_meta_component_phantom(classes: &[&str]) -> SelectorMeta {
    let pseudos = app_runtime_pseudos();
    SelectorMeta {
        type_name: String::new(),
        type_aliases: Vec::new(),
        id: None,
        classes: classes.iter().map(|s| (*s).to_string()).collect(),
        states: SelectorStates {
            dark: pseudos.dark,
            inline: pseudos.inline,
            ansi: pseudos.ansi,
            nocolor: pseudos.nocolor,
            ..Default::default()
        },
        component_phantom: true,
    }
}

/// Canonical selector meta for an arena node (§T-7).
///
/// Reads type identity from the widget, id from `node.css_id`, classes from
/// `node.classes`, and interaction states from `node.state` plus
/// `widget.is_active()` and `widget.can_focus()`.
///
/// # Panics
/// Panics if `node_id` is not present in `tree`. All callers in render/layout/
/// hit-test hot-paths already guard with `let Some(n) = tree.get(node_id)` before
/// calling this; the panic is acceptable for absent nodes (would be a logic bug).
pub(crate) fn node_selector_meta(tree: &WidgetTree, node_id: NodeId) -> SelectorMeta {
    let node = tree
        .get(node_id)
        .expect("node_selector_meta called with absent node_id");
    node_selector_meta_from_node(node, node_id)
}

/// Build `SelectorMeta` directly from a `WidgetNode` reference.
///
/// Separated from `node_selector_meta` so `apply_display_visibility_to_tree`
/// (which already holds the node reference) can call this without re-borrowing
/// the arena.
pub(crate) fn node_selector_meta_from_node(node: &WidgetNode, node_id: NodeId) -> SelectorMeta {
    let pseudos = app_runtime_pseudos();
    let state = node.state;
    let is_screen = node.widget.style_type() == "Screen"
        || node
            .widget
            .style_type_aliases().contains(&"Screen");
    // Python parity: an active screen behaves as :focus while the app is active.
    let focused = (state.focused || is_screen) && app_is_active();
    SelectorMeta {
        type_name: node.widget.style_type().to_string(),
        type_aliases: node
            .widget
            .style_type_aliases()
            .iter()
            .map(|name| (*name).to_string())
            .collect(),
        id: node.css_id.clone(),
        classes: node.classes.iter().cloned().collect(),
        states: SelectorStates {
            disabled: state.disabled,
            focused,
            hovered: state.hovered,
            active: node.widget.is_active(),
            dark: pseudos.dark,
            inline: pseudos.inline,
            ansi: pseudos.ansi,
            nocolor: pseudos.nocolor,
            can_focus: node.widget.can_focus(),
            focus_within: is_focus_within(node_id),
            ..Default::default()
        },
        component_phantom: false,
    }
}

/// Canonical resolved style for an arena node (§T-7).
///
/// Cache keyed by the real `NodeId`. Combines stylesheet + `widget.style()`
/// contribution + node inline styles (node wins), then inherits from parent.
pub(crate) fn resolve_node_style(tree: &WidgetTree, node_id: NodeId, meta: &SelectorMeta) -> Style {
    let node = tree
        .get(node_id)
        .expect("resolve_node_style called with absent node_id");
    // Inline style: node record wins over widget behavior contribution.
    let node_inline = if node.styles.style != Default::default() {
        Some(node.styles.style.clone())
    } else {
        node.widget.style()
    };
    let key = super::context::ComputedStyleKey {
        meta: meta.clone(),
        ancestors: SELECTOR_STACK.with(|stack| stack.borrow().clone()),
        parent_style: STYLE_STACK.with(|stack| stack.borrow().last().cloned()),
        inline_style: node_inline.clone(),
    };
    if let Some(cached) = COMPUTED_STYLE_CACHE.with(|cache| cache.borrow_mut().get(node_id, &key)) {
        return cached;
    }
    let sheet_style = STYLE_CONTEXT
        .with(|ctx| {
            ctx.borrow()
                .as_ref()
                .map(|sheet| sheet.style_for_meta(meta))
        })
        .unwrap_or_default();
    let mut style = sheet_style;
    if let Some(inline) = node_inline {
        style = style.combine(&inline);
    }
    if let Some(parent) = STYLE_STACK.with(|stack| stack.borrow().last().cloned()) {
        style = style.inherit_from(&parent);
    }
    let layout_affected_changed = COMPUTED_STYLE_CACHE
        .with(|cache| cache.borrow().prior_resolved(node_id))
        .is_some_and(|prior| !layout_fields_equal(&prior, &style));
    COMPUTED_STYLE_CACHE.with(|cache| {
        cache
            .borrow_mut()
            .store(node_id, key, style.clone(), layout_affected_changed)
    });
    style
}

pub(crate) fn current_parent_style() -> Option<Style> {
    STYLE_STACK.with(|stack| stack.borrow().last().cloned())
}

/// Returns the resolved style of the **widget currently being rendered**.
///
/// During a `render()` call, `render_widget_with_meta` pushes the widget's own
/// resolved style onto `STYLE_STACK` before invoking `widget.render(...)`, so the
/// top-of-stack entry *is* the widget's own style. Widgets that need to read their
/// own resolved style (e.g. ScrollBar reading scrollbar color tokens from the CSS)
/// call this instead of holding a `WidgetStyles` field.
///
/// Outside of a render call (e.g. in event handlers) this returns `None`.
pub(crate) fn current_self_style() -> Option<Style> {
    STYLE_STACK.with(|stack| stack.borrow().last().cloned())
}

/// Returns the resolved style of the **host (parent) of the widget currently
/// being rendered**, i.e. the entry directly below the top of `STYLE_STACK`.
///
/// During a `render()` call the top of the stack is the widget's own style (see
/// [`current_self_style`]); the element below it is the host that owns this
/// widget. Mirrors Python `ScrollBar`/`ScrollBarCorner`, which read
/// `self.parent.styles.scrollbar_*` (textual/src/textual/scrollbar.py): scrollbar
/// color/background/corner tokens are NOT inherited, so a dedicated scrollbar must
/// read them from its host widget's resolved styles, not from its own.
///
/// Returns `None` when there is no host (e.g. the widget is the stack root, or
/// this is called outside a render call).
pub(crate) fn current_host_style() -> Option<Style> {
    STYLE_STACK.with(|stack| {
        let stack = stack.borrow();
        let len = stack.len();
        (len >= 2).then(|| stack[len - 2].clone())
    })
}

/// Returns the effective painted background color from the current ancestor stack.
///
/// CSS `bg` is not inherited semantically, but render-time composition needs the
/// nearest painted ancestor surface so transparent descendants don't fall back to
/// terminal-default background.
pub(crate) fn current_composited_background() -> Option<crate::style::Color> {
    let fallback =
        crate::style::parse_color_like("$background").unwrap_or(crate::style::Color::rgb(0, 0, 0));
    STYLE_STACK.with(|stack| {
        let stack = stack.borrow();
        if stack.is_empty() {
            return None;
        }
        let mut saw_background = false;
        let mut composited = fallback;
        for style in stack.iter() {
            if let Some(bg) = style.bg {
                let flat = bg.flatten_over(composited);
                // Mirror Python dom.py: `background += styles.background.tint(styles.background_tint)`
                // `background_tint` blends the tint color into the widget's own bg at `percent`
                // intensity. Apply it here so children see the tinted surface as parent bg.
                let effective = if let Some(tint) = style.background_tint {
                    crate::renderables::Tint::<()>::blend_color_with_percent(
                        flat, tint.color, tint.percent,
                    )
                } else {
                    flat
                };
                composited = effective;
                saw_background = true;
            }
        }
        if saw_background {
            Some(composited)
        } else {
            None
        }
    })
}

// -- Frozen ancestor surface override (Python `visual_style` cache parity) ---

thread_local! {
    /// The FROZEN ancestor surface for the node currently rendering, when its
    /// live ancestor composite has diverged from the surface it captured at its
    /// own last content re-render (an ancestor-only INLINE background change,
    /// e.g. a Screen background animation). Python keeps the content strip's
    /// BACKGROUND baked over the OLD surface in that case because `visual_style`
    /// is cached on the widget's own `styles._cache_key`, which an ancestor
    /// inline mutation never bumps.
    ///
    /// Installed by `runtime::render` (see `FROZEN_ANCESTOR_BG` there) and read
    /// ONLY by `render_widget_with_meta`'s blank FILL cells (content-align pad
    /// and the fg-bearing vertical extend), which Python renders from the cached
    /// `visual_style.rich_style` — a bg-only concern here (those cells are
    /// spaces, so their frozen fg is invisible / an opaque `$foreground`).
    ///
    /// It deliberately does NOT flow through
    /// `current_ancestor_composited_background()`: content GLYPH strips must
    /// bake their FOREGROUND (link colour / `color: auto` contrast / fg over
    /// bg) against the LIVE surface — Python resolves the link/contrast fg from
    /// `background_colors` (live), not the cached `visual_style`. Their frozen
    /// BACKGROUND is applied post-render, bg-only, by `recolor_frozen_content_bg`.
    ///
    /// Surface/padding fills that Python renders live from `background_colors`
    /// (`StylesCache` border/CSS-padding/trailing-pad) are unaffected.
    static FROZEN_ANCESTOR_BG_OVERRIDE: std::cell::Cell<Option<crate::style::Color>> =
        const { std::cell::Cell::new(None) };
}

pub(crate) struct FrozenAncestorBgGuard(Option<crate::style::Color>);

/// Install the frozen ancestor surface for the duration of one node's
/// `render_widget_with_meta` pass (RAII; restores the previous override).
pub(crate) fn set_frozen_ancestor_bg_override(
    frozen: crate::style::Color,
) -> FrozenAncestorBgGuard {
    let prev = FROZEN_ANCESTOR_BG_OVERRIDE.with(|cell| cell.replace(Some(frozen)));
    FrozenAncestorBgGuard(prev)
}

impl Drop for FrozenAncestorBgGuard {
    fn drop(&mut self) {
        let prev = self.0;
        FROZEN_ANCESTOR_BG_OVERRIDE.with(|cell| cell.set(prev));
    }
}

/// The active frozen ancestor surface, if a divergence override is installed.
/// Consumed ONLY by the blank FILL cells in `render_widget_with_meta`
/// (content-align padding / fg-bearing vertical extend), which Python renders
/// from the cached `visual_style.rich_style` (`Strip.align` /
/// `render_line` IndexError -> `Strip.blank(width, visual_style.rich_style)`).
/// NOT read by `current_ancestor_composited_background()`, so content GLYPH
/// foregrounds keep resolving against the LIVE surface (see the thread-local).
pub(crate) fn frozen_ancestor_bg_override() -> Option<crate::style::Color> {
    FROZEN_ANCESTOR_BG_OVERRIDE.with(|cell| cell.get())
}

/// Returns the composited background of ALL ANCESTORS of the widget currently
/// being rendered, i.e. the composited background from all style stack entries
/// EXCEPT the top (which is the current widget's own style).
///
/// Call this from inside `render()` to get the same parent-bg that
/// `apply_style_to_segments` would compute when it runs AFTER `render()` returns
/// (at which point the current widget's style has been popped from the stack).
///
/// This is the bake-surface for widget CONTENT strips. It stays LIVE even
/// under a frozen-ancestor-bg divergence: content glyph FOREGROUNDS (link
/// colour, `color: auto` contrast, fg-over-bg) must resolve against the
/// current surface — Python computes those from `background_colors` (live),
/// not the cached `visual_style` (`guide/actions` `Red`/`Green`/`Blue` link
/// labels tint to the live surface). The glyph BACKGROUND is frozen post-
/// render, bg-only, by `recolor_frozen_content_bg`.
///
/// Returns `None` if the ancestor chain has no explicit background.
pub(crate) fn current_ancestor_composited_background() -> Option<crate::style::Color> {
    let fallback =
        crate::style::parse_color_like("$background").unwrap_or(crate::style::Color::rgb(0, 0, 0));
    STYLE_STACK.with(|stack| {
        let stack = stack.borrow();
        // All entries except the last (= current widget's own style).
        let len = stack.len();
        let ancestor_slice = if len >= 2 { &stack[..len - 1] } else { &stack[..0] };
        let mut saw_background = false;
        let mut composited = fallback;
        for style in ancestor_slice.iter() {
            if let Some(bg) = style.bg {
                let flat = bg.flatten_over(composited);
                // Mirror Python dom.py: apply background_tint when compositing
                // ancestor backgrounds so the tinted surface propagates correctly.
                let effective = if let Some(tint) = style.background_tint {
                    crate::renderables::Tint::<()>::blend_color_with_percent(
                        flat, tint.color, tint.percent,
                    )
                } else {
                    flat
                };
                composited = effective;
                saw_background = true;
            }
        }
        if saw_background {
            Some(composited)
        } else {
            None
        }
    })
}

/// Resolve a style for off-tree/type-only paths (no arena node, no cache).
///
/// Tree-mode rendering uses `resolve_node_style` (cached by real `NodeId`).
/// This path is reachable only from cold contexts (component styles, legacy
/// widget-to-widget rendering, unit tests), so it computes directly.
pub(crate) fn resolve_style<T: Widget + ?Sized>(widget: &T, meta: &SelectorMeta) -> Style {
    let sheet_style = STYLE_CONTEXT
        .with(|ctx| {
            ctx.borrow()
                .as_ref()
                .map(|sheet| sheet.style_for(widget, meta))
        })
        .unwrap_or_default();
    let mut style = sheet_style;
    if let Some(inline) = widget.style() {
        style = style.combine(&inline);
    }
    if let Some(parent) = STYLE_STACK.with(|stack| stack.borrow().last().cloned()) {
        style = style.inherit_from(&parent);
    }
    style
}

pub(crate) fn resolve_style_for_meta(meta: &SelectorMeta) -> Style {
    let sheet_style = STYLE_CONTEXT
        .with(|ctx| {
            ctx.borrow()
                .as_ref()
                .map(|sheet| sheet.style_for_meta(meta))
        })
        .unwrap_or_default();
    let mut style = sheet_style;
    if let Some(parent) = STYLE_STACK.with(|stack| stack.borrow().last().cloned()) {
        style = style.inherit_from(&parent);
    }
    style
}

/// Resolve the component-class CSS for `widget` into a [`Style`].
///
/// Public so external/custom widgets can read component-class styling from CSS
/// (Python parity: `Widget.get_component_styles`). The component name(s) are
/// resolved on a typeless virtual phantom node
/// ([`selector_meta_component_phantom`]).
///
/// Multi-name semantics: all `classes` go onto ONE phantom (the COMPOUND
/// form), so compound rules like `.a.b { ... }` match. This is the right form
/// for STATE-MARKER usage (e.g. `["tabs--underline", "-active"]`). For
/// Python's `get_component_styles(*names)` sequential-merge semantics use
/// [`resolve_component_style_merged`].
///
/// Parent context (the live-stack contract):
/// - During a widget's `render()` the widget's own LIVE meta (real arena id,
///   runtime classes, dispatch pseudo states) is already the top of the
///   selector stack, pushed by the render pipeline. The phantom resolves
///   DIRECTLY against that stack, so id-, class- and pseudo-qualified parent
///   rules (`#my-table > .part`, `Type.some-class > .part`,
///   `Type:focus > .part`) all match, exactly as Python's virtual node (whose
///   parent IS the live node).
/// - Off-tree (unit tests via `set_style_context`, cold renderable paths) the
///   widget's SEED meta is pushed as a fallback parent so type-qualified rules
///   still resolve.
pub fn resolve_component_style<T: Widget + ?Sized>(widget: &T, classes: &[&str]) -> Style {
    let meta = selector_meta_component_phantom(classes);
    if widget_live_meta_on_top(widget) {
        return resolve_style_for_meta(&meta);
    }
    let parent_meta = selector_meta_generic(widget);
    SELECTOR_STACK.with(|stack| {
        stack.borrow_mut().push(parent_meta);
        let out = resolve_style_for_meta(&meta);
        stack.borrow_mut().pop();
        out
    })
}

/// True when `widget`'s own live meta is the marked top of `SELECTOR_STACK`
/// (arena render path). The identity check disambiguates a widget rendering
/// another widget's content inline (R7).
fn widget_live_meta_on_top<T: Widget + ?Sized>(widget: &T) -> bool {
    super::context::live_widget_meta_on_top(widget.style_type(), widget.style_type_aliases())
}

/// Python `get_component_styles(*names)` parity: resolve each name against its
/// OWN phantom and merge the results sequentially in argument order — a later
/// name's properties win regardless of rule specificity (`dom.py`), and
/// compound `.a.b` rules never match (each phantom carries one name).
pub fn resolve_component_style_merged<T: Widget + ?Sized>(widget: &T, names: &[&str]) -> Style {
    let mut out = Style::new();
    for name in names {
        out = out.combine(&resolve_component_style(widget, &[name]));
    }
    out
}

/// Partial variant of [`resolve_component_style`]: only properties explicitly
/// set by matching sheet rules, WITHOUT the inherit-from-parent step (Python
/// `partial_rich_style` semantics). The [`Style`]'s `Option` fields express
/// the partiality directly.
pub fn resolve_component_style_partial<T: Widget + ?Sized>(
    widget: &T,
    classes: &[&str],
) -> Style {
    let meta = selector_meta_component_phantom(classes);
    let resolve = || {
        STYLE_CONTEXT
            .with(|ctx| {
                ctx.borrow()
                    .as_ref()
                    .map(|sheet| sheet.style_for_meta(&meta))
            })
            .unwrap_or_default()
    };
    if widget_live_meta_on_top(widget) {
        return resolve();
    }
    let parent_meta = selector_meta_generic(widget);
    SELECTOR_STACK.with(|stack| {
        stack.borrow_mut().push(parent_meta);
        let out = resolve();
        stack.borrow_mut().pop();
        out
    })
}

pub(crate) fn with_style_stack<T>(meta: SelectorMeta, resolved: Style, f: impl FnOnce() -> T) -> T {
    STYLE_STACK.with(|style_stack| {
        SELECTOR_STACK.with(|selector_stack| {
            style_stack.borrow_mut().push(resolved);
            selector_stack.borrow_mut().push(meta);
            let out = f();
            selector_stack.borrow_mut().pop();
            style_stack.borrow_mut().pop();
            out
        })
    })
}

/// Push a widget's resolved style + selector meta onto the style stack.
///
/// Used by the tree-driven compositor to establish parent CSS context
/// before rendering children. Must be paired with `pop_style_context`.
pub(crate) fn push_style_context(meta: SelectorMeta, resolved: Style) {
    STYLE_STACK.with(|stack| stack.borrow_mut().push(resolved));
    SELECTOR_STACK.with(|stack| stack.borrow_mut().push(meta));
}

/// Pop a previously pushed style context.
pub(crate) fn pop_style_context() {
    SELECTOR_STACK.with(|stack| stack.borrow_mut().pop());
    STYLE_STACK.with(|stack| stack.borrow_mut().pop());
}

pub(crate) fn layout_fields_equal(a: &Style, b: &Style) -> bool {
    a.margin == b.margin
        && a.padding == b.padding
        && a.border_top == b.border_top
        && a.border_right == b.border_right
        && a.border_bottom == b.border_bottom
        && a.border_left == b.border_left
        && a.width == b.width
        && a.height == b.height
        && a.min_width == b.min_width
        && a.max_width == b.max_width
        && a.min_height == b.min_height
        && a.max_height == b.max_height
        && a.layout == b.layout
        && a.display == b.display
        && a.visibility == b.visibility
        && a.dock == b.dock
        && a.grid_size_columns == b.grid_size_columns
        && a.grid_size_rows == b.grid_size_rows
        && a.grid_columns == b.grid_columns
        && a.grid_rows == b.grid_rows
        && a.grid_gutter_horizontal == b.grid_gutter_horizontal
        && a.grid_gutter_vertical == b.grid_gutter_vertical
        && a.layer == b.layer
        && a.layers == b.layers
        // P2 CSS gap layout-affecting fields
        && a.position == b.position
        && a.box_sizing == b.box_sizing
        && a.split == b.split
        && a.outline_top == b.outline_top
        && a.outline_right == b.outline_right
        && a.outline_bottom == b.outline_bottom
        && a.outline_left == b.outline_left
        && a.row_span == b.row_span
        && a.column_span == b.column_span
        && a.padding_top == b.padding_top
        && a.padding_right == b.padding_right
        && a.padding_bottom == b.padding_bottom
        && a.padding_left == b.padding_left
        && a.margin_top == b.margin_top
        && a.margin_right == b.margin_right
        && a.margin_bottom == b.margin_bottom
        && a.margin_left == b.margin_left
        && a.scrollbar_size == b.scrollbar_size
        && a.scrollbar_size_horizontal == b.scrollbar_size_horizontal
        && a.scrollbar_size_vertical == b.scrollbar_size_vertical
        && a.scrollbar_visibility == b.scrollbar_visibility
        && a.constrain_x == b.constrain_x
        && a.constrain_y == b.constrain_y
        && a.expand == b.expand
        // `offset` moves the widget's placement (Python `OffsetProperty` is
        // `refresh(layout=True)`): a change must count as layout-affecting.
        && a.offset == b.offset
}

pub(crate) fn begin_style_render_pass() {
    COMPUTED_STYLE_CACHE.with(|cache| cache.borrow_mut().begin_render_pass());
}

pub(crate) fn take_layout_affected_style_changes() -> bool {
    COMPUTED_STYLE_CACHE.with(|cache| cache.borrow_mut().take_layout_affected_change())
}

/// Walk the tree and sync resolved CSS `display` and `visibility` values
/// to the corresponding `WidgetNode` fields.
///
/// Must be called after the CSS stylesheet context is active so that
/// `resolve_style` returns correct results. Typically invoked once per
/// render pass, right after `begin_style_render_pass()`.
pub(crate) fn apply_display_visibility_to_tree(tree: &mut WidgetTree) {
    let root = match tree.root() {
        Some(r) => r,
        None => return,
    };

    // Build the :focus-within set: the focused node + all its ancestors.
    let mut focus_within_ids = std::collections::HashSet::new();
    for node_id in tree.walk_depth_first(root) {
        if let Some(node) = tree.get(node_id) {
            if node.state.focused {
                focus_within_ids.insert(node_id);
                for ancestor in tree.ancestors(node_id) {
                    focus_within_ids.insert(ancestor);
                }
                break;
            }
        }
    }
    let _fw_guard = super::context::set_focus_within(focus_within_ids);

    // `inherited_vis` is the effective visibility flowing down from ancestors.
    // Python (`DOMNode.visible`): a node with no OWN `visibility` rule inherits
    // its parent's effective visibility; an explicit rule overrides it. So a
    // `visibility:hidden` container hides its descendants, but a descendant with
    // an explicit `visibility:visible` (e.g. `#bot > Placeholder`) shows again.
    fn apply_node(tree: &mut WidgetTree, node_id: NodeId, inherited_vis: Visibility) {
        let (meta, resolved, child_ids) = {
            if tree.get(node_id).is_none() {
                return;
            }
            let meta = node_selector_meta(tree, node_id);
            let resolved = resolve_node_style(tree, node_id, &meta);
            let child_ids = tree.children(node_id).to_vec();
            (meta, resolved, child_ids)
        };

        // Apply CSS display state. Runtime-controlled display (for example
        // tab switching) is merged in WidgetTree as `effective = css && runtime`.
        let display_bool = !matches!(resolved.display, Some(Display::None));
        tree.set_css_display(node_id, display_bool);

        // Effective visibility: own rule if explicitly set, else inherited.
        let effective_vis = resolved.visibility.unwrap_or(inherited_vis);
        tree.set_visibility(node_id, effective_vis);

        with_style_stack(meta, resolved, || {
            for child_id in child_ids {
                apply_node(tree, child_id, effective_vis);
            }
        });
    }

    apply_node(tree, root, Visibility::Visible);
}

#[cfg(test)]
pub(crate) fn reset_computed_style_cache_for_tests() {
    COMPUTED_STYLE_CACHE.with(|cache| cache.borrow_mut().reset_for_tests());
}

#[cfg(test)]
pub(crate) fn computed_style_cache_stats_for_tests() -> (u64, u64) {
    COMPUTED_STYLE_CACHE.with(|cache| {
        let stats = cache.borrow().stats_for_tests();
        (stats.hits, stats.misses)
    })
}
