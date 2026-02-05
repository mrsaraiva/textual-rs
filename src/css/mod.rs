mod defaults;
mod selectors;

pub use defaults::default_widget_stylesheet;
pub use selectors::{StyleContextGuard, StyleRule, StyleSelector, StyleSheet, set_style_context};
pub(crate) use selectors::{
    apply_style_to_segments, current_parent_style, resolve_style, selector_meta_generic,
    with_style_stack,
};
