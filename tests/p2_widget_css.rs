//! P2 gate tests: widget-specific CSS property consumption.
//!
//! P2G-30: ScrollView scrollbar CSS properties (color/size/gutter/visibility).
//! P2G-32: Link widget CSS link-styling properties (normal + hover).
//! P2G-36: Per-property transitions (parser + resolution).

use std::time::Duration;

use rich_rs::Console;
use textual::css::set_style_context;
use textual::prelude::*;
use textual::style::{
    PropertyTransition, ScrollbarGutter, ScrollbarVisibility, TransitionTiming,
};

// ───────────────────────────────────────────────────────────────────────
// P2G-30  Scrollbar CSS
// ───────────────────────────────────────────────────────────────────────

#[test]
fn p2g30_scrollbar_color_parses() {
    // NOTE: parse-only — verifies CSS parser, not runtime scrollbar rendering.
    let css = r#"ScrollView { scrollbar-color: #ff0000; }"#;
    let sheet = StyleSheet::parse(css);
    let rules = sheet.rules();
    assert!(!rules.is_empty(), "should parse scrollbar-color rule");
    let style = &rules[0].style();
    assert_eq!(
        style.scrollbar_color,
        Some(Color::parse("#ff0000").unwrap()),
        "scrollbar_color should be red"
    );
}

#[test]
fn p2g30_scrollbar_background_parses() {
    // NOTE: parse-only — verifies CSS parser, not runtime scrollbar rendering.
    let css = r#"ScrollView { scrollbar-background: #112233; }"#;
    let sheet = StyleSheet::parse(css);
    let style = &sheet.rules()[0].style();
    assert_eq!(
        style.scrollbar_background,
        Some(Color::parse("#112233").unwrap())
    );
}

#[test]
fn p2g30_scrollbar_hover_active_colors() {
    // NOTE: parse-only — verifies CSS parser, not runtime scrollbar rendering.
    let css = r#"
        ScrollView {
            scrollbar-color-hover: #aabbcc;
            scrollbar-color-active: #ddeeff;
            scrollbar-background-hover: #001122;
            scrollbar-background-active: #334455;
        }
    "#;
    let sheet = StyleSheet::parse(css);
    let style = &sheet.rules()[0].style();
    assert_eq!(
        style.scrollbar_color_hover,
        Some(Color::parse("#aabbcc").unwrap())
    );
    assert_eq!(
        style.scrollbar_color_active,
        Some(Color::parse("#ddeeff").unwrap())
    );
    assert_eq!(
        style.scrollbar_background_hover,
        Some(Color::parse("#001122").unwrap())
    );
    assert_eq!(
        style.scrollbar_background_active,
        Some(Color::parse("#334455").unwrap())
    );
}

#[test]
fn p2g30_scrollbar_corner_color() {
    // NOTE: parse-only — verifies CSS parser, not runtime scrollbar rendering.
    let css = r#"ScrollView { scrollbar-corner-color: #abcdef; }"#;
    let sheet = StyleSheet::parse(css);
    let style = &sheet.rules()[0].style();
    assert_eq!(
        style.scrollbar_corner_color,
        Some(Color::parse("#abcdef").unwrap())
    );
}

#[test]
fn p2g30_scrollbar_gutter_stable() {
    // NOTE: parse-only — verifies CSS parser, not runtime scrollbar rendering.
    let css = r#"ScrollView { scrollbar-gutter: stable; }"#;
    let sheet = StyleSheet::parse(css);
    let style = &sheet.rules()[0].style();
    assert_eq!(style.scrollbar_gutter, Some(ScrollbarGutter::Stable));
}

#[test]
fn p2g30_scrollbar_gutter_auto() {
    // NOTE: parse-only — verifies CSS parser, not runtime scrollbar rendering.
    let css = r#"ScrollView { scrollbar-gutter: auto; }"#;
    let sheet = StyleSheet::parse(css);
    let style = &sheet.rules()[0].style();
    assert_eq!(style.scrollbar_gutter, Some(ScrollbarGutter::Auto));
}

#[test]
fn p2g30_scrollbar_size_shorthand() {
    // NOTE: parse-only — verifies CSS parser, not runtime scrollbar rendering.
    let css = r#"ScrollView { scrollbar-size: 3; }"#;
    let sheet = StyleSheet::parse(css);
    let style = &sheet.rules()[0].style();
    assert_eq!(style.scrollbar_size, Some(3));
}

#[test]
fn p2g30_scrollbar_size_per_axis() {
    // NOTE: parse-only — verifies CSS parser, not runtime scrollbar rendering.
    let css = r#"
        ScrollView {
            scrollbar-size-horizontal: 2;
            scrollbar-size-vertical: 4;
        }
    "#;
    let sheet = StyleSheet::parse(css);
    let style = &sheet.rules()[0].style();
    assert_eq!(style.scrollbar_size_horizontal, Some(2));
    assert_eq!(style.scrollbar_size_vertical, Some(4));
}

#[test]
fn p2g30_scrollbar_visibility_hidden() {
    // NOTE: parse-only — verifies CSS parser, not runtime scrollbar rendering.
    let css = r#"ScrollView { scrollbar-visibility: hidden; }"#;
    let sheet = StyleSheet::parse(css);
    let style = &sheet.rules()[0].style();
    assert_eq!(
        style.scrollbar_visibility,
        Some(ScrollbarVisibility::Hidden)
    );
}

#[test]
fn p2g30_scrollbar_visibility_visible() {
    // NOTE: parse-only — verifies CSS parser, not runtime scrollbar rendering.
    let css = r#"ScrollView { scrollbar-visibility: visible; }"#;
    let sheet = StyleSheet::parse(css);
    let style = &sheet.rules()[0].style();
    assert_eq!(
        style.scrollbar_visibility,
        Some(ScrollbarVisibility::Visible)
    );
}

#[test]
fn p2g30_scrollbar_visibility_auto() {
    // NOTE: parse-only — verifies CSS parser, not runtime scrollbar rendering.
    let css = r#"ScrollView { scrollbar-visibility: auto; }"#;
    let sheet = StyleSheet::parse(css);
    let style = &sheet.rules()[0].style();
    assert_eq!(style.scrollbar_visibility, Some(ScrollbarVisibility::Auto));
}

#[test]
fn p2g30_scroll_view_render_with_css_scrollbar_size() {
    // Verify that ScrollView respects CSS scrollbar-size-vertical for
    // vertical scrollbar width (default is 2, we set 3).
    let css = r#"ScrollView { scrollbar-size-vertical: 3; }"#;
    let sheet = StyleSheet::parse(css);
    let _guard = set_style_context(sheet);

    let mut sv = ScrollView::new(Label::new("content"));
    let _ = sv.take_composed_children(); // Enter tree mode.
    sv.set_virtual_content_size(10, 100);

    let console = Console::default();
    let mut opts = console.options().clone();
    opts.size = (30, 10);
    opts.max_width = 30;
    opts.max_height = 10;

    let segments = Widget::render(&sv, &console, &opts);
    let lines =
        rich_rs::Segment::split_and_crop_lines(segments, 30, None, true, false);
    // The scrollbar should be present (content > viewport).
    assert_eq!(lines.len(), 10);
    // With scrollbar-size-vertical: 3, the content viewport should be 30-3=27 wide
    // and the last 3 cells of each line should be scrollbar chrome.
    // We verify that lines have segments for scrollbar.
    let has_scrollbar = lines.iter().any(|line| line.len() > 1);
    assert!(
        has_scrollbar,
        "scrollbar chrome should be present when content overflows"
    );
}

#[test]
fn p2g30_scroll_view_visibility_hidden_no_scrollbar() {
    let css = r#"ScrollView { scrollbar-visibility: hidden; }"#;
    let sheet = StyleSheet::parse(css);
    let _guard = set_style_context(sheet);

    let mut sv = ScrollView::new(Label::new("content"));
    let _ = sv.take_composed_children();
    sv.set_virtual_content_size(10, 100);

    let console = Console::default();
    let mut opts = console.options().clone();
    opts.size = (20, 10);
    opts.max_width = 20;
    opts.max_height = 10;

    let segments = Widget::render(&sv, &console, &opts);
    let lines =
        rich_rs::Segment::split_and_crop_lines(segments, 20, None, true, false);
    // With visibility: hidden, no scrollbar should appear.
    // All lines should be single segments (space fills, no scrollbar chrome).
    for (i, line) in lines.iter().enumerate() {
        assert_eq!(
            line.len(),
            1,
            "line {} should have 1 segment (no scrollbar chrome) when visibility=hidden",
            i
        );
    }
}

// ───────────────────────────────────────────────────────────────────────
// P2G-32  Link CSS styling
// ───────────────────────────────────────────────────────────────────────

#[test]
fn p2g32_link_color_parses() {
    // NOTE: parse-only — verifies CSS parser, not runtime link rendering.
    let css = r#"Link { link-color: #ff0000; }"#;
    let sheet = StyleSheet::parse(css);
    let style = &sheet.rules()[0].style();
    assert_eq!(
        style.link_color,
        Some(Color::parse("#ff0000").unwrap()),
        "link_color should be parsed"
    );
}

#[test]
fn p2g32_link_background_parses() {
    // NOTE: parse-only — verifies CSS parser, not runtime link rendering.
    let css = r#"Link { link-background: #00ff00; }"#;
    let sheet = StyleSheet::parse(css);
    let style = &sheet.rules()[0].style();
    assert_eq!(
        style.link_background,
        Some(Color::parse("#00ff00").unwrap())
    );
}

#[test]
fn p2g32_link_style_parses() {
    // NOTE: parse-only — verifies CSS parser, not runtime link rendering.
    let css = r#"Link { link-style: bold underline; }"#;
    let sheet = StyleSheet::parse(css);
    let style = &sheet.rules()[0].style();
    let flags = style.link_style.expect("link_style should be set");
    assert!(flags.bold, "bold flag should be set");
    assert!(flags.underline, "underline flag should be set");
    assert!(!flags.italic, "italic flag should not be set");
}

#[test]
fn p2g32_link_hover_variants_parse() {
    // NOTE: parse-only — verifies CSS parser, not runtime link rendering.
    let css = r#"
        Link {
            link-color-hover: #aabb00;
            link-background-hover: #00aabb;
            link-style-hover: italic;
        }
    "#;
    let sheet = StyleSheet::parse(css);
    let style = &sheet.rules()[0].style();
    assert_eq!(
        style.link_color_hover,
        Some(Color::parse("#aabb00").unwrap())
    );
    assert_eq!(
        style.link_background_hover,
        Some(Color::parse("#00aabb").unwrap())
    );
    let flags = style
        .link_style_hover
        .expect("link_style_hover should be set");
    assert!(flags.italic, "hover italic flag should be set");
}

#[test]
fn p2g32_link_render_applies_css_color() {
    let css = r#"Link { link-color: #ff0000; }"#;
    let sheet = StyleSheet::parse(css);
    let _guard = set_style_context(sheet);

    let link = Link::new("hello").with_url("https://example.com");
    let console = Console::new();
    let mut opts = console.options().clone();
    opts.size = (10, 1);
    opts.max_width = 10;
    opts.max_height = 1;

    let segments = Widget::render(&link, &console, &opts);
    let first = segments
        .iter()
        .find(|s| s.control.is_none())
        .expect("text segment");
    let style = first.style.expect("style should be set");
    let red = Color::parse("#ff0000").unwrap();
    assert_eq!(
        style.color,
        Some(red.to_simple_opaque()),
        "link should render with CSS link-color"
    );
}

#[test]
fn p2g32_link_hover_applies_hover_css() {
    let css = r#"
        Link {
            link-color: #aaaaaa;
            link-color-hover: #ff0000;
        }
    "#;
    let sheet = StyleSheet::parse(css);
    let _guard = set_style_context(sheet);

    let mut link = Link::new("hello").with_url("https://example.com");
    link.set_hovered(true);

    let console = Console::new();
    let mut opts = console.options().clone();
    opts.size = (10, 1);
    opts.max_width = 10;
    opts.max_height = 1;

    let segments = Widget::render(&link, &console, &opts);
    let first = segments
        .iter()
        .find(|s| s.control.is_none())
        .expect("text segment");
    let style = first.style.expect("style should be set");
    let red = Color::parse("#ff0000").unwrap();
    assert_eq!(
        style.color,
        Some(red.to_simple_opaque()),
        "hovered link should use link-color-hover"
    );
}

#[test]
fn p2g32_link_normal_does_not_use_hover_css() {
    let css = r#"
        Link {
            link-color: #aaaaaa;
            link-color-hover: #ff0000;
        }
    "#;
    let sheet = StyleSheet::parse(css);
    let _guard = set_style_context(sheet);

    let link = Link::new("hello").with_url("https://example.com");
    // NOT hovered.
    assert!(!link.is_hovered());

    let console = Console::new();
    let mut opts = console.options().clone();
    opts.size = (10, 1);
    opts.max_width = 10;
    opts.max_height = 1;

    let segments = Widget::render(&link, &console, &opts);
    let first = segments
        .iter()
        .find(|s| s.control.is_none())
        .expect("text segment");
    let style = first.style.expect("style should be set");
    let grey = Color::parse("#aaaaaa").unwrap();
    assert_eq!(
        style.color,
        Some(grey.to_simple_opaque()),
        "non-hovered link should use normal link-color, not hover"
    );
}

// ───────────────────────────────────────────────────────────────────────
// P2G-36  Per-property transitions
// ───────────────────────────────────────────────────────────────────────

#[test]
fn p2g36_transition_shorthand_parses_single_property() {
    // NOTE: parse-only — verifies CSS parser, not runtime transition behavior.
    let css = r#"ScrollView { transition: offset_y 500ms linear; }"#;
    let sheet = StyleSheet::parse(css);
    let style = &sheet.rules()[0].style();
    let transitions = style.transitions.as_ref().expect("transitions should be set");
    assert_eq!(transitions.len(), 1);
    assert_eq!(transitions[0].property, "offset_y");
    assert_eq!(transitions[0].duration, Duration::from_millis(500));
    assert_eq!(transitions[0].timing, TransitionTiming::Linear);
    assert_eq!(transitions[0].delay, Duration::ZERO);
}

#[test]
fn p2g36_transition_shorthand_parses_multi_property() {
    // NOTE: parse-only — verifies CSS parser, not runtime transition behavior.
    let css = r#"
        ScrollView {
            transition: opacity 300ms linear 100ms, background 200ms;
        }
    "#;
    let sheet = StyleSheet::parse(css);
    let style = &sheet.rules()[0].style();
    let transitions = style.transitions.as_ref().expect("transitions should be set");
    assert_eq!(transitions.len(), 2, "should parse 2 per-property transitions");

    // First: opacity 300ms linear 100ms
    assert_eq!(transitions[0].property, "opacity");
    assert_eq!(transitions[0].duration, Duration::from_millis(300));
    assert_eq!(transitions[0].timing, TransitionTiming::Linear);
    assert_eq!(transitions[0].delay, Duration::from_millis(100));

    // Second: background 200ms (defaults: linear timing, 0 delay)
    assert_eq!(transitions[1].property, "background");
    assert_eq!(transitions[1].duration, Duration::from_millis(200));
    assert_eq!(transitions[1].delay, Duration::ZERO);
}

#[test]
fn p2g36_transition_also_sets_generic_fields_from_first_item() {
    // NOTE: parse-only — verifies CSS parser, not runtime transition behavior.
    let css = r#"ScrollView { transition: offset_y 400ms in_out_cubic 50ms; }"#;
    let sheet = StyleSheet::parse(css);
    let style = &sheet.rules()[0].style();
    // Generic transition fields should match the first item.
    assert_eq!(style.transition_duration, Some(Duration::from_millis(400)));
    assert_eq!(style.transition_delay, Some(Duration::from_millis(50)));
    assert_eq!(
        style.transition_timing,
        Some(TransitionTiming::InOutCubic)
    );
}

#[test]
fn p2g36_resolve_transition_for_specific_property() {
    let style = Style::new()
        .transition_duration(Duration::from_millis(1000))
        .transition_timing(TransitionTiming::OutCubic);
    let mut style = style;
    style.transitions = Some(vec![
        PropertyTransition {
            property: "opacity".to_string(),
            duration: Duration::from_millis(200),
            timing: TransitionTiming::Linear,
            delay: Duration::ZERO,
        },
        PropertyTransition {
            property: "background".to_string(),
            duration: Duration::from_millis(500),
            timing: TransitionTiming::InOutCubic,
            delay: Duration::from_millis(100),
        },
    ]);

    // "opacity" should use per-property transition.
    let (dur, del, ease) =
        textual::runtime::resolve_transition_for_property(&style, "opacity").unwrap();
    assert_eq!(dur, Duration::from_millis(200));
    assert_eq!(del, Duration::ZERO);
    assert_eq!(ease, AnimationEase::Linear);

    // "background" should use per-property transition.
    let (dur, del, ease) =
        textual::runtime::resolve_transition_for_property(&style, "background").unwrap();
    assert_eq!(dur, Duration::from_millis(500));
    assert_eq!(del, Duration::from_millis(100));
    assert_eq!(ease, AnimationEase::InOutCubic);

    // "unknown" should fall back to generic transition.
    let (dur, _del, ease) =
        textual::runtime::resolve_transition_for_property(&style, "unknown").unwrap();
    assert_eq!(dur, Duration::from_millis(1000));
    assert_eq!(ease, AnimationEase::OutCubic);
}

#[test]
fn p2g36_resolve_transition_all_wildcard() {
    let mut style = Style::new();
    style.transitions = Some(vec![PropertyTransition {
        property: "all".to_string(),
        duration: Duration::from_millis(300),
        timing: TransitionTiming::Linear,
        delay: Duration::from_millis(50),
    }]);

    // "all" matches any property name.
    let (dur, del, ease) =
        textual::runtime::resolve_transition_for_property(&style, "anything").unwrap();
    assert_eq!(dur, Duration::from_millis(300));
    assert_eq!(del, Duration::from_millis(50));
    assert_eq!(ease, AnimationEase::Linear);
}

#[test]
fn p2g36_resolve_transition_zero_duration_returns_none() {
    let mut style = Style::new();
    style.transitions = Some(vec![PropertyTransition {
        property: "opacity".to_string(),
        duration: Duration::ZERO,
        timing: TransitionTiming::Linear,
        delay: Duration::ZERO,
    }]);

    assert!(
        textual::runtime::resolve_transition_for_property(&style, "opacity").is_none(),
        "zero-duration transition should return None"
    );
}

#[test]
fn p2g36_resolve_transition_no_transitions_no_generic() {
    let style = Style::new();
    assert!(
        textual::runtime::resolve_transition_for_property(&style, "anything").is_none(),
        "empty style should return None"
    );
}

// ───────────────────────────────────────────────────────────────────────
// P2-32 behavioral: disabled link ignores hover styling
// ───────────────────────────────────────────────────────────────────────

#[test]
fn p2_32_disabled_link_ignores_hover_style() {
    // A disabled link that is also hovered should use normal link-color,
    // NOT link-color-hover. This matches Python Textual behavior.
    let css = r#"
        Link {
            link-color: #aaaaaa;
            link-color-hover: #ff0000;
        }
    "#;
    let sheet = StyleSheet::parse(css);
    let _guard = set_style_context(sheet);

    let mut link = Link::new("hello").with_url("https://example.com").with_disabled(true);
    link.set_hovered(true);

    let console = Console::new();
    let mut opts = console.options().clone();
    opts.size = (10, 1);
    opts.max_width = 10;
    opts.max_height = 1;

    let segments = Widget::render(&link, &console, &opts);
    let first = segments
        .iter()
        .find(|s| s.control.is_none())
        .expect("text segment");
    let style = first.style.expect("style should be set");
    let grey = Color::parse("#aaaaaa").unwrap();
    let red = Color::parse("#ff0000").unwrap();
    assert_eq!(
        style.color,
        Some(grey.to_simple_opaque()),
        "disabled+hovered link should use normal link-color, not hover"
    );
    assert_ne!(
        style.color,
        Some(red.to_simple_opaque()),
        "disabled link should NOT use link-color-hover"
    );
}
