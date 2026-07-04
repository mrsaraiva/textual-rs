//! Contract test for `Static::layout_height()` (Python how-to `render_compose.py`
//! Gap-A area).
//!
//! Post height-chrome keystone (symmetric with the width axis), `layout_height()`
//! reports PURE CONTENT height; the box model (padding/border) is applied by the
//! LAYOUT pass, not baked into the reported intrinsic height. So a `Static` with
//! `padding: 2 4` reports its single content row (1), and the layout pass grows
//! the outer box by the 4 vertical padding rows. The end-to-end Gap-A render
//! (padded auto-height box centered correctly under `align: center middle`) is
//! guarded by the `docs_render_compose` PTY parity golden, not by baking chrome
//! into this intrinsic-height number.

use textual::prelude::*;

fn with_css<T>(css: &str, f: impl FnOnce() -> T) -> T {
    let mut sheet = textual::css::default_widget_stylesheet();
    sheet.extend(&textual::css::StyleSheet::parse(css));
    let _guard = textual::css::set_style_context(sheet);
    f()
}

#[test]
fn static_layout_height_is_pure_content_chrome_applied_by_layout() {
    // One content line + `padding: 2 4`. Under the keystone convention
    // `layout_height()` is pure content (1); the 4 vertical padding rows are
    // added by the layout pass, not by this intrinsic-height reader.
    let height = with_css("Static { padding: 2 4; }", || {
        let s = Static::new("hello");
        s.layout_height()
    });
    assert_eq!(
        height,
        Some(1),
        "Static reports pure content height; padding is layout-applied (keystone)"
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
