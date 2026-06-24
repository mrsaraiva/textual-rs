mod defaults;
mod selectors;

pub use defaults::default_widget_stylesheet;
pub use selectors::{
    AppActiveGuard, AppRuntimePseudos, AppRuntimePseudosGuard, PseudoClass, StyleContextGuard,
    StyleRule, StyleSelector, StyleSheet, set_app_active, set_app_runtime_pseudos,
    set_style_context,
};
/// Resolve a custom widget's component-class CSS into a [`crate::style::Style`].
///
/// Public entry point backing [`crate::widgets::Widget::get_component_styles`]
/// (Python parity: `Widget.get_component_styles`). Custom widgets that paint
/// sub-elements can resolve the CSS rules declared for their component classes
/// instead of hardcoding colours.
pub use selectors::resolve_component_style;
pub(crate) use selectors::{
    Combinator, SelectorChain, SelectorMeta, apply_display_visibility_to_tree,
    apply_style_to_segments, apply_widget_opacity_to_segments, begin_style_render_pass,
    current_ancestor_composited_background, current_composited_background, current_host_style,
    current_parent_style, current_self_style,
    node_selector_meta, node_selector_meta_from_node, parse_selector_list, pop_style_context,
    push_style_context,
    resolve_node_style, resolve_style, resolve_style_for_meta,
    selector_meta_component, selector_meta_generic, take_layout_affected_style_changes,
    with_style_stack,
};
