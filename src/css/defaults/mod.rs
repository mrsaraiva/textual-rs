// Minimal built-in widget defaults to help demos look like Textual (Python) without requiring
// demo-specific CSS for core widget visuals.
//
// Note: this is a pragmatic subset of Textual's built-in widget CSS. We intentionally avoid
// full TCSS features (nesting, `&`, `!important`, advanced opacity) until the style engine grows.
//
// Each submodule exports a `DEFAULT_CSS` constant with the CSS fragment for its widget(s).

mod base;
mod button;
mod checkbox;
mod data_table;
mod header_footer;
mod input;
mod list_view;
mod misc;
mod select;
mod tabs;
mod text_area;
mod tree;

use super::StyleSheet;

pub fn default_widget_stylesheet() -> StyleSheet {
    let combined = [
        base::DEFAULT_CSS,
        misc::DEFAULT_CSS,
        header_footer::DEFAULT_CSS,
        text_area::DEFAULT_CSS,
        input::DEFAULT_CSS,
        checkbox::DEFAULT_CSS,
        select::DEFAULT_CSS,
        list_view::DEFAULT_CSS,
        tree::DEFAULT_CSS,
        tabs::DEFAULT_CSS,
        button::DEFAULT_CSS,
        data_table::DEFAULT_CSS,
    ]
    .join("\n");
    StyleSheet::parse(&combined)
}
