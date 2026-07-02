/// Widget method delegation macros.
///
/// **Superseded by the `#[widget(base = ..)]` attribute derive** (see
/// `textual_macros::widget` / re-exported as `textual::widget`). New compound
/// widgets should use `#[widget(base = <Container>)]`, which generates the same
/// full `impl Widget` forwarding as a first-class attribute (and additionally
/// supports `style_type = ".."`, `reactive`, and `override(..)`). The derive is
/// the single place that knows the delegated method surface, so a future
/// trait-shape change (RA-2) becomes a macro-internal edit. These declarative
/// macros are kept only because existing widgets still reference them.
///
/// **Deprecated (migration-period only).**
///
/// These macros exist to bridge the pre-RA-2 period where widgets carry their own
/// identity/style fields. Once RA-2 (node-record split) fully lands, widgets will no
/// longer own `id`/`classes`/`WidgetStyles`, and the delegated methods (`styles`,
/// `styles_mut`, `style_id`, `style_classes`, `set_style_id`) will be removed from
/// the `Widget` trait. At that point, these macros become obsolete and will be
/// removed.
///
/// Removal criteria (both must be met):
/// 1. RA-2 is fully landed and the `styles` / `styles_mut` / `style_id` /
///    `style_classes` / `set_style_id` / `style_type_aliases` methods are removed
///    from `Widget`.
/// 2. All delegation sites (inventoried in SPEC-RA5) have been migrated to the
///    RA-2 node-record pattern.
///
/// **Do not add new uses of these macros.** For new widget-wrapper code, use the
/// containment pattern (SPEC-RA5).
///
/// ---
///
/// Two macros are provided:
///
/// ## `delegate_widget_to!` (existing, expanded)
///
/// Generates a complete `impl Widget` block that forwards **every** method.
/// Use for thin wrappers with zero overrides.
///
/// ```rust,ignore
/// delegate_widget_to!(VerticalScroll, inner);
/// ```
///
/// ## `delegate_widget_method!` (new)
///
/// Generates individual method forwarding bodies, usable **inside** a
/// hand-written `impl Widget for ... { }` block. Accepts either a single
/// method name or a bracketed list.
///
/// ```rust,ignore
/// impl Widget for MarkdownViewer {
///     // Your overrides — only the methods with custom logic
///     fn style_type(&self) -> &'static str { "MarkdownViewer" }
///     fn on_message(&mut self, message: &MessageEvent, ctx: &mut EventCtx) {
///         /* custom handling, then self.inner.on_message(message, ctx) */
///     }
///
///     // Delegate everything else to `self.inner`
///     delegate_widget_method!(inner, [
///         render, render_with_debug, render_line, render_lines,
///         compose,
///         focusable, can_focus, can_focus_children,
///         on_mount, on_unmount, on_tick, on_resize, on_layout,
///         set_virtual_content_size,
///         on_event_capture, on_event,
///         on_mouse_scroll, on_mouse_move,
///         scroll_offset, scroll_offset_f32, scroll_viewport_size,
///         scroll_virtual_content_size, clips_descendants_to_content,
///         layout_height, content_width,
///         bindings, binding_hints, execute_action,
///         action_namespace, action_registry,
///         is_active,
///         mouse_interactive, preserve_underlay,
///         border_title, border_subtitle,
///         tooltip, tooltip_anchor,
///         help_markup,
///         allow_select, selection_at, selection_word_range_at,
///         selection_all_range, update_selection, clear_selection,
///         get_selection, selection_updated,
///     ]);
/// }
/// ```

// ── Per-method delegation ─────────────────────────────────────────────

#[allow(clippy::empty_line_after_doc_comments)]
#[macro_export]
macro_rules! delegate_widget_method {
    // ── Dispatch: list of names ────────────────────────────────────────
    ($field:ident, [$($method:ident),* $(,)?]) => {
        $($crate::widgets::delegate::delegate_widget_method!($field, $method);)*
    };

    // ── Rendering ──────────────────────────────────────────────────────

    ($field:ident, render) => {
        fn render(
            &self,
            console: &rich_rs::Console,
            options: &rich_rs::ConsoleOptions,
        ) -> rich_rs::Segments {
            self.$field.render(console, options)
        }
    };

    ($field:ident, render_with_debug) => {
        fn render_with_debug(
            &self,
            console: &rich_rs::Console,
            options: &rich_rs::ConsoleOptions,
            debug: &$crate::debug::DebugLayout,
        ) -> rich_rs::Segments {
            self.$field.render_with_debug(console, options, debug)
        }
    };

    ($field:ident, render_line) => {
        fn render_line(
            &self,
            y: usize,
            console: &rich_rs::Console,
            options: &rich_rs::ConsoleOptions,
        ) -> rich_rs::Segments {
            self.$field.render_line(y, console, options)
        }
    };

    ($field:ident, render_lines) => {
        fn render_lines(
            &self,
            start_y: usize,
            line_count: usize,
            console: &rich_rs::Console,
            options: &rich_rs::ConsoleOptions,
        ) -> Vec<rich_rs::Segments> {
            self.$field.render_lines(start_y, line_count, console, options)
        }
    };

    // ── Composition ────────────────────────────────────────────────────

    ($field:ident, compose) => {
        fn compose(&mut self) -> $crate::compose::ComposeResult {
            self.$field.compose()
        }
    };

    ($field:ident, take_pending_mount_messages) => {
        fn take_pending_mount_messages(&mut self) -> Vec<Box<dyn $crate::message::Message>> {
            self.$field.take_pending_mount_messages()
        }
    };

    // ── Focus ──────────────────────────────────────────────────────────

    ($field:ident, focusable) => {
        fn focusable(&self) -> bool { self.$field.focusable() }
    };

    ($field:ident, can_focus) => {
        fn can_focus(&self) -> bool { self.$field.can_focus() }
    };

    ($field:ident, can_focus_children) => {
        fn can_focus_children(&self) -> bool { self.$field.can_focus_children() }
    };

    ($field:ident, on_node_state_changed) => {
        fn on_node_state_changed(&mut self, old: $crate::widgets::NodeState, new: $crate::widgets::NodeState) {
            self.$field.on_node_state_changed(old, new);
        }
    };

    // ── Lifecycle ──────────────────────────────────────────────────────

    ($field:ident, on_mount) => {
        fn on_mount(&mut self) { self.$field.on_mount(); }
    };

    ($field:ident, on_unmount) => {
        fn on_unmount(&mut self) { self.$field.on_unmount(); }
    };

    ($field:ident, on_tick) => {
        fn on_tick(&mut self, tick: u64) { self.$field.on_tick(tick); }
    };

    ($field:ident, on_resize) => {
        fn on_resize(&mut self, width: u16, height: u16) { self.$field.on_resize(width, height); }
    };

    ($field:ident, on_layout) => {
        fn on_layout(&mut self, width: u16, height: u16) { self.$field.on_layout(width, height); }
    };

    ($field:ident, set_virtual_content_size) => {
        fn set_virtual_content_size(&mut self, width: usize, height: usize) {
            self.$field.set_virtual_content_size(width, height);
        }
    };

    // ── Events ─────────────────────────────────────────────────────────

    ($field:ident, on_event_capture) => {
        fn on_event_capture(
            &mut self,
            event: &$crate::event::Event,
            ctx: &mut $crate::event::EventCtx,
        ) {
            self.$field.on_event_capture(event, ctx);
        }
    };

    ($field:ident, on_event) => {
        fn on_event(
            &mut self,
            event: &$crate::event::Event,
            ctx: &mut $crate::event::EventCtx,
        ) {
            self.$field.on_event(event, ctx);
        }
    };

    ($field:ident, on_message) => {
        fn on_message(
            &mut self,
            message: &$crate::message::MessageEvent,
            ctx: &mut $crate::event::EventCtx,
        ) {
            self.$field.on_message(message, ctx);
        }
    };

    ($field:ident, on_mouse_scroll) => {
        fn on_mouse_scroll(
            &mut self,
            delta_x: i32,
            delta_y: i32,
            ctx: &mut $crate::event::EventCtx,
        ) {
            self.$field.on_mouse_scroll(delta_x, delta_y, ctx);
        }
    };

    ($field:ident, on_mouse_move) => {
        fn on_mouse_move(&mut self, x: u16, y: u16) -> bool {
            self.$field.on_mouse_move(x, y)
        }
    };

    // ── App-level hooks ────────────────────────────────────────────────

    ($field:ident, on_app_key) => {
        fn on_app_key(
            &mut self,
            app: &mut $crate::App,
            key: &$crate::keys::KeyEventData,
            ctx: &mut $crate::event::EventCtx,
        ) {
            self.$field.on_app_key(app, key, ctx);
        }
    };

    ($field:ident, on_app_action) => {
        fn on_app_action(
            &mut self,
            app: &mut $crate::App,
            action: $crate::event::Action,
            ctx: &mut $crate::event::EventCtx,
        ) {
            self.$field.on_app_action(app, action, ctx);
        }
    };

    ($field:ident, on_app_message) => {
        fn on_app_message(
            &mut self,
            app: &mut $crate::App,
            message: &$crate::message::MessageEvent,
            ctx: &mut $crate::event::EventCtx,
        ) {
            self.$field.on_app_message(app, message, ctx);
        }
    };

    ($field:ident, on_app_tick) => {
        fn on_app_tick(
            &mut self,
            app: &mut $crate::App,
            tick: u64,
            ctx: &mut $crate::event::EventCtx,
        ) {
            self.$field.on_app_tick(app, tick, ctx);
        }
    };

    ($field:ident, on_app_mount) => {
        fn on_app_mount(
            &mut self,
            app: &mut $crate::App,
            ctx: &mut $crate::event::EventCtx,
        ) {
            self.$field.on_app_mount(app, ctx);
        }
    };

    // ── Scroll ─────────────────────────────────────────────────────────

    ($field:ident, scroll_offset) => {
        fn scroll_offset(&self) -> (usize, usize) { self.$field.scroll_offset() }
    };

    ($field:ident, scroll_offset_f32) => {
        fn scroll_offset_f32(&self) -> (f32, f32) { self.$field.scroll_offset_f32() }
    };

    ($field:ident, scroll_viewport_size) => {
        fn scroll_viewport_size(&self) -> Option<(usize, usize)> {
            self.$field.scroll_viewport_size()
        }
    };

    ($field:ident, scroll_virtual_content_size) => {
        fn scroll_virtual_content_size(&self) -> Option<(usize, usize)> {
            self.$field.scroll_virtual_content_size()
        }
    };

    ($field:ident, clips_descendants_to_content) => {
        fn clips_descendants_to_content(&self) -> bool {
            self.$field.clips_descendants_to_content()
        }
    };

    // ── Tree / layout ──────────────────────────────────────────────────

    ($field:ident, child_display_for_tree) => {
        fn child_display_for_tree(&self, child_index: usize) -> Option<bool> {
            self.$field.child_display_for_tree(child_index)
        }
    };

    ($field:ident, tree_child_content_inset) => {
        fn tree_child_content_inset(&self) -> (u16, u16, u16, u16) {
            self.$field.tree_child_content_inset()
        }
    };

    ($field:ident, layout_height) => {
        fn layout_height(&self) -> Option<usize> { self.$field.layout_height() }
    };

    ($field:ident, content_width) => {
        fn content_width(&self) -> Option<usize> { self.$field.content_width() }
    };

    ($field:ident, preserve_underlay) => {
        fn preserve_underlay(&self) -> bool { self.$field.preserve_underlay() }
    };

    // ── Actions / bindings ─────────────────────────────────────────────

    ($field:ident, bindings) => {
        fn bindings(&self) -> Vec<$crate::widgets::BindingDecl> { self.$field.bindings() }
    };

    ($field:ident, binding_hints) => {
        fn binding_hints(&self) -> Vec<$crate::event::BindingHint> { self.$field.binding_hints() }
    };

    ($field:ident, execute_action) => {
        fn execute_action(
            &mut self,
            action: &$crate::action::ParsedAction,
            ctx: &mut $crate::event::EventCtx,
        ) -> bool {
            self.$field.execute_action(action, ctx)
        }
    };

    ($field:ident, action_namespace) => {
        fn action_namespace(&self) -> &str { self.$field.action_namespace() }
    };

    ($field:ident, action_registry) => {
        fn action_registry(&self) -> &[$crate::action::ActionDecl] { self.$field.action_registry() }
    };

    // ── Styles ─────────────────────────────────────────────────────────

    ($field:ident, style) => {
        fn style(&self) -> Option<$crate::style::Style> { self.$field.style() }
    };

    ($field:ident, set_inline_style) => {
        fn set_inline_style(&mut self, style: $crate::style::Style) {
            self.$field.set_inline_style(style);
        }
    };

    ($field:ident, take_node_seed) => {
        fn take_node_seed(&mut self) -> $crate::widgets::NodeSeed {
            self.$field.take_node_seed()
        }
    };

    ($field:ident, style_type) => {
        fn style_type(&self) -> &'static str { self.$field.style_type() }
    };

    ($field:ident, style_type_aliases) => {
        fn style_type_aliases(&self) -> &[&'static str] { self.$field.style_type_aliases() }
    };

    ($field:ident, border_title) => {
        fn border_title(&self) -> Option<&str> { self.$field.border_title() }
    };

    ($field:ident, border_subtitle) => {
        fn border_subtitle(&self) -> Option<&str> { self.$field.border_subtitle() }
    };

    // ── State queries ──────────────────────────────────────────────────

    ($field:ident, is_active) => {
        fn is_active(&self) -> bool { self.$field.is_active() }
    };

    ($field:ident, mouse_interactive) => {
        fn mouse_interactive(&self) -> bool { self.$field.mouse_interactive() }
    };

    // ── Tooltip / help ─────────────────────────────────────────────────

    ($field:ident, tooltip) => {
        fn tooltip(&self) -> Option<String> { self.$field.tooltip() }
    };

    ($field:ident, tooltip_anchor) => {
        fn tooltip_anchor(&self) -> Option<(u16, u16)> { self.$field.tooltip_anchor() }
    };

    ($field:ident, help_markup) => {
        fn help_markup(&self) -> Option<&str> { self.$field.help_markup() }
    };

    // ── Selection ──────────────────────────────────────────────────────

    ($field:ident, allow_select) => {
        fn allow_select(&self) -> bool { self.$field.allow_select() }
    };

    ($field:ident, selection_at) => {
        fn selection_at(&self, x: u16, y: u16) -> Option<$crate::widgets::WidgetSelectionAnchor> {
            self.$field.selection_at(x, y)
        }
    };

    ($field:ident, selection_word_range_at) => {
        fn selection_word_range_at(
            &self,
            x: u16,
            y: u16,
        ) -> Option<($crate::widgets::WidgetSelectionAnchor, $crate::widgets::WidgetSelectionAnchor)> {
            self.$field.selection_word_range_at(x, y)
        }
    };

    ($field:ident, selection_all_range) => {
        fn selection_all_range(
            &self,
        ) -> Option<($crate::widgets::WidgetSelectionAnchor, $crate::widgets::WidgetSelectionAnchor)> {
            self.$field.selection_all_range()
        }
    };

    ($field:ident, update_selection) => {
        fn update_selection(
            &mut self,
            from: $crate::widgets::WidgetSelectionAnchor,
            to: $crate::widgets::WidgetSelectionAnchor,
        ) -> bool {
            self.$field.update_selection(from, to)
        }
    };

    ($field:ident, clear_selection) => {
        fn clear_selection(&mut self) -> bool { self.$field.clear_selection() }
    };

    ($field:ident, get_selection) => {
        fn get_selection(&self) -> Option<String> { self.$field.get_selection() }
    };

    ($field:ident, selection_updated) => {
        fn selection_updated(&mut self, ctx: &mut $crate::event::EventCtx) {
            self.$field.selection_updated(ctx);
        }
    };

    // ── Reactive ───────────────────────────────────────────────────────

    ($field:ident, reactive_widget) => {
        fn reactive_widget(&mut self) -> Option<&mut dyn $crate::reactive::ReactiveWidget> {
            self.$field.reactive_widget()
        }
    };
}

/// Also generate `impl Renderable` when used alongside `delegate_widget_method!`.
/// Place this after the `impl Widget for ...` block.
#[macro_export]
macro_rules! delegate_renderable {
    ($wrapper:ty) => {
        impl rich_rs::Renderable for $wrapper {
            fn render(
                &self,
                console: &rich_rs::Console,
                options: &rich_rs::ConsoleOptions,
            ) -> rich_rs::Segments {
                $crate::widgets::Widget::render(self, console, options)
            }
        }
    };
}

// ── Full delegation (existing API, expanded coverage) ──────────────────

/// Canonical method count in `delegate_widget_to!`'s full delegation list.
/// If this changes, update the expected value and audit partial delegation sites:
/// `rg -n "delegate-audit:" src/widgets`
#[cfg(test)]
const WIDGET_DELEGATE_METHOD_COUNT_EXPECTED: usize = 60;

/// Generate a complete `impl Widget + impl Renderable` block forwarding
/// **every** method to `self.$field`. Use for thin wrappers with zero
/// overrides. For wrappers that override some methods, use
/// `delegate_widget_method!` inside a hand-written `impl Widget` block.
#[macro_export]
macro_rules! delegate_widget_to {
    ($wrapper:ty, $field:ident) => {
        impl Widget for $wrapper {
            $crate::widgets::delegate::delegate_widget_method!(
                $field,
                [
                    // WIDGET_DELEGATE_LIST_BEGIN
                    // Rendering
                    render,
                    render_with_debug,
                    render_line,
                    render_lines,
                    // Composition
                    compose,
                    take_pending_mount_messages,
                    // Focus / node state
                    focusable,
                    can_focus,
                    can_focus_children,
                    on_node_state_changed,
                    // Lifecycle
                    on_mount,
                    on_unmount,
                    on_tick,
                    on_resize,
                    on_layout,
                    set_virtual_content_size,
                    // Events
                    on_event_capture,
                    on_event,
                    on_message,
                    on_mouse_scroll,
                    on_mouse_move,
                    // App-level hooks
                    on_app_key,
                    on_app_action,
                    on_app_message,
                    on_app_tick,
                    on_app_mount,
                    // Scroll
                    scroll_offset,
                    scroll_offset_f32,
                    scroll_viewport_size,
                    scroll_virtual_content_size,
                    clips_descendants_to_content,
                    // Tree / layout
                    child_display_for_tree,
                    tree_child_content_inset,
                    layout_height,
                    content_width,
                    preserve_underlay,
                    // Actions / bindings
                    bindings,
                    binding_hints,
                    execute_action,
                    action_namespace,
                    action_registry,
                    // Styles / seed
                    style,
                    set_inline_style,
                    take_node_seed,
                    border_title,
                    border_subtitle,
                    // State
                    is_active,
                    mouse_interactive,
                    // Tooltip / help
                    tooltip,
                    tooltip_anchor,
                    help_markup,
                    // Selection
                    allow_select,
                    selection_at,
                    selection_word_range_at,
                    selection_all_range,
                    update_selection,
                    clear_selection,
                    get_selection,
                    selection_updated,
                    // Reactive
                    reactive_widget,
                    // WIDGET_DELEGATE_LIST_END
                ]
            );
        }

        $crate::widgets::delegate::delegate_renderable!($wrapper);
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_delegate_method_count_matches_expected() {
        let src = include_str!("delegate.rs");
        let start = src
            .find("WIDGET_DELEGATE_LIST_BEGIN")
            .expect("delegate list start marker must exist");
        let end = src
            .find("WIDGET_DELEGATE_LIST_END")
            .expect("delegate list end marker must exist");
        let body = &src[start..end];
        let count = body
            .lines()
            .map(str::trim)
            .filter(|line| {
                !line.is_empty()
                    && !line.starts_with("//")
                    && line.ends_with(',')
                    && line
                        .chars()
                        .next()
                        .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
            })
            .count();
        assert_eq!(
            count, WIDGET_DELEGATE_METHOD_COUNT_EXPECTED,
            "Widget delegate list changed: update expected count and audit partial delegation sites"
        );
    }
}

// DEFERRED(RA-2): remove delegate macros after RA-2 node-record split is landed
pub use delegate_renderable;
pub use delegate_widget_method;
pub use delegate_widget_to;
