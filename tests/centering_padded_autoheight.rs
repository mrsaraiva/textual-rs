//! Regression test for the Gap-A root cause (Python how-to `render_compose.py`).
//!
//! `Static::layout_height()` delegated straight to its inner `Label`, whose
//! chrome resolves against the *label's* selector — so an app rule like
//! `Static { padding: 2 4 }` (which targets the `Static`, not the label) was
//! invisible to the reported outer height. The auto-height box was therefore 4
//! rows too short, which both clipped it and (because `align: center middle`
//! measures the resulting `layout_rect`) centered the widget ~2 rows too low.
//!
//! This guards the contract at its source: a `Static`'s reported outer height
//! must include its own (app-CSS) vertical padding/border.

use textual::prelude::*;

fn with_css<T>(css: &str, f: impl FnOnce() -> T) -> T {
    let mut sheet = textual::css::default_widget_stylesheet();
    sheet.extend(&textual::css::StyleSheet::parse(css));
    let _guard = textual::css::set_style_context(sheet);
    f()
}

#[test]
fn static_layout_height_includes_app_css_padding() {
    // One content line + `padding: 2 4` (4 vertical) → outer height 5.
    let height = with_css("Static { padding: 2 4; }", || {
        let s = Static::new("hello");
        s.layout_height()
    });
    assert_eq!(
        height,
        Some(5),
        "Static must report content + its own app-CSS vertical padding"
    );
}

#[test]
fn static_layout_height_without_padding_is_content_only() {
    // No padding rule → outer height equals the single content line.
    let height = with_css("", || {
        let s = Static::new("hello");
        s.layout_height()
    });
    assert_eq!(height, Some(1), "bare Static is one content row tall");
}
