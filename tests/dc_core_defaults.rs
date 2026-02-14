// DC-01..12, DC-26, DC-36 parity tests for core layout CSS defaults.
//
// Since StyleRule::selector_chain() is pub(crate), these tests parse inline CSS
// and verify properties on the first (and only) rule's style.

use textual::css::StyleSheet;
use textual::style::{
    Constrain, Display, HorizontalAlign, Layout, Overflow, Pointer, Scalar, VerticalAlign,
};

/// Parse a single-rule CSS block and return its Style.
fn parse_single(css: &str) -> textual::style::Style {
    let sheet = StyleSheet::parse(css);
    let rules = sheet.rules();
    assert!(!rules.is_empty(), "CSS should produce at least one rule: {css}");
    rules[0].style()
}

// =====================================================================
// DC-01: Screen — overflow-y: auto, bg: $background
// =====================================================================

#[test]
fn dc_01_screen_has_layout_vertical() {
    let s = parse_single("Screen { layout: vertical; overflow-y: auto; bg: $background; }");
    assert_eq!(s.layout, Some(Layout::Vertical));
}

#[test]
fn dc_01_screen_has_overflow_y_auto() {
    let s = parse_single("Screen { layout: vertical; overflow-y: auto; bg: $background; }");
    assert_eq!(s.overflow_y, Some(Overflow::Auto));
}

#[test]
fn dc_01_screen_has_bg() {
    let s = parse_single("Screen { layout: vertical; overflow-y: auto; bg: $background; }");
    assert!(s.bg.is_some(), "Screen should have bg");
}

// =====================================================================
// DC-02: ScrollView — overflow-y: auto; overflow-x: auto
// =====================================================================

#[test]
fn dc_02_scrollview_has_overflow_y_auto() {
    let s = parse_single("ScrollView { overflow-y: auto; overflow-x: auto; }");
    assert_eq!(s.overflow_y, Some(Overflow::Auto));
}

#[test]
fn dc_02_scrollview_has_overflow_x_auto() {
    let s = parse_single("ScrollView { overflow-y: auto; overflow-x: auto; }");
    assert_eq!(s.overflow_x, Some(Overflow::Auto));
}

// =====================================================================
// DC-03: ModalScreen — layout: vertical; overflow-y: auto; bg: $background
// =====================================================================

#[test]
fn dc_03_modalscreen_has_layout_vertical() {
    let s = parse_single("ModalScreen { layout: vertical; overflow-y: auto; bg: $background; }");
    assert_eq!(s.layout, Some(Layout::Vertical));
}

#[test]
fn dc_03_modalscreen_has_overflow_y_auto() {
    let s = parse_single("ModalScreen { layout: vertical; overflow-y: auto; bg: $background; }");
    assert_eq!(s.overflow_y, Some(Overflow::Auto));
}

#[test]
fn dc_03_modalscreen_has_bg() {
    let s = parse_single("ModalScreen { layout: vertical; overflow-y: auto; bg: $background; }");
    assert!(s.bg.is_some(), "ModalScreen should have bg");
}

// =====================================================================
// DC-04: Widget base — scrollbar-*, link-*, background: transparent
// =====================================================================

#[test]
fn dc_04_widget_scrollbar_size_vertical() {
    let s = parse_single("Widget { scrollbar-size-vertical: 2; scrollbar-size-horizontal: 1; }");
    assert_eq!(s.scrollbar_size_vertical, Some(2));
}

#[test]
fn dc_04_widget_scrollbar_size_horizontal() {
    let s = parse_single("Widget { scrollbar-size-vertical: 2; scrollbar-size-horizontal: 1; }");
    assert_eq!(s.scrollbar_size_horizontal, Some(1));
}

#[test]
fn dc_04_widget_scrollbar_colors() {
    let s = parse_single(
        "Widget { scrollbar-color: $scrollbar; scrollbar-background: $scrollbar-background; }",
    );
    assert!(s.scrollbar_color.is_some(), "should have scrollbar-color");
    assert!(s.scrollbar_background.is_some(), "should have scrollbar-background");
}

#[test]
fn dc_04_widget_link_color() {
    let s = parse_single("Widget { link-color: $link-color; }");
    assert!(s.link_color.is_some(), "should have link-color");
}

#[test]
fn dc_04_widget_link_style_underline() {
    let s = parse_single("Widget { link-style: underline; }");
    let ls = s.link_style.expect("should have link-style");
    assert!(ls.underline, "link-style should include underline");
}

#[test]
fn dc_04_widget_transparent_bg() {
    let s = parse_single("Widget { background: transparent; }");
    let bg = s.bg.expect("Widget should have bg");
    assert_eq!(bg.a, 0, "Widget bg should be transparent (alpha=0)");
}

// =====================================================================
// DC-05: Label variants — width: auto; height: auto; min-height: 1
// =====================================================================

#[test]
fn dc_05_label_width_auto() {
    let s = parse_single("Label { width: auto; height: auto; min-height: 1; }");
    assert_eq!(s.width, Some(Scalar::Auto));
}

#[test]
fn dc_05_label_height_auto() {
    let s = parse_single("Label { width: auto; height: auto; min-height: 1; }");
    assert_eq!(s.height, Some(Scalar::Auto));
}

#[test]
fn dc_05_label_min_height_1() {
    let s = parse_single("Label { width: auto; height: auto; min-height: 1; }");
    assert_eq!(s.min_height, Some(Scalar::Cells(1)));
}

#[test]
fn dc_05_label_variant_success_parses() {
    let sheet = StyleSheet::parse(
        r#"Label { &.success { color: $text-success; bg: $success-muted; } }"#,
    );
    // Should produce rules (base + nested) without panicking.
    assert!(sheet.rules().len() >= 1);
}

#[test]
fn dc_05_label_variant_error_parses() {
    let sheet =
        StyleSheet::parse(r#"Label { &.error { color: $text-error; bg: $error-muted; } }"#);
    assert!(sheet.rules().len() >= 1);
}

#[test]
fn dc_05_label_variant_warning_parses() {
    let sheet = StyleSheet::parse(
        r#"Label { &.warning { color: $text-warning; bg: $warning-muted; } }"#,
    );
    assert!(sheet.rules().len() >= 1);
}

#[test]
fn dc_05_label_variant_primary_parses() {
    let sheet = StyleSheet::parse(
        r#"Label { &.primary { color: $text-primary; bg: $primary-muted; } }"#,
    );
    assert!(sheet.rules().len() >= 1);
}

// =====================================================================
// DC-06: Container — overflow: hidden; layout: vertical
// =====================================================================

#[test]
fn dc_06_container_overflow_hidden() {
    let s = parse_single(
        "Container { width: 1fr; height: 1fr; layout: vertical; overflow: hidden; }",
    );
    assert_eq!(s.overflow_x, Some(Overflow::Hidden));
    assert_eq!(s.overflow_y, Some(Overflow::Hidden));
}

#[test]
fn dc_06_container_layout_vertical() {
    let s = parse_single(
        "Container { width: 1fr; height: 1fr; layout: vertical; overflow: hidden; }",
    );
    assert_eq!(s.layout, Some(Layout::Vertical));
}

#[test]
fn dc_06_container_1fr_dimensions() {
    let s = parse_single(
        "Container { width: 1fr; height: 1fr; layout: vertical; overflow: hidden; }",
    );
    assert_eq!(s.width, Some(Scalar::Fraction(1.0)));
    assert_eq!(s.height, Some(Scalar::Fraction(1.0)));
}

// =====================================================================
// DC-07: ScrollableContainer — overflow: auto; width/height: 1fr
// =====================================================================

#[test]
fn dc_07_scrollable_container_1fr_dims() {
    let s = parse_single(
        "ScrollableContainer { width: 1fr; height: 1fr; layout: vertical; overflow: auto; }",
    );
    assert_eq!(s.width, Some(Scalar::Fraction(1.0)));
    assert_eq!(s.height, Some(Scalar::Fraction(1.0)));
}

#[test]
fn dc_07_scrollable_container_overflow_auto() {
    let s = parse_single(
        "ScrollableContainer { width: 1fr; height: 1fr; layout: vertical; overflow: auto; }",
    );
    assert_eq!(s.overflow_x, Some(Overflow::Auto));
    assert_eq!(s.overflow_y, Some(Overflow::Auto));
}

// =====================================================================
// DC-08: Center — align-horizontal: center; height: auto
// =====================================================================

#[test]
fn dc_08_center_align_horizontal() {
    let s = parse_single("Center { align-horizontal: center; width: 1fr; height: auto; }");
    let a = s.align.expect("Center should have align");
    assert_eq!(a.horizontal, HorizontalAlign::Center);
}

#[test]
fn dc_08_center_height_auto() {
    let s = parse_single("Center { align-horizontal: center; width: 1fr; height: auto; }");
    assert_eq!(s.height, Some(Scalar::Auto));
}

// =====================================================================
// DC-09: Right — align-horizontal: right; height: auto
// =====================================================================

#[test]
fn dc_09_right_align_horizontal() {
    let s = parse_single("Right { align-horizontal: right; width: 1fr; height: auto; }");
    let a = s.align.expect("Right should have align");
    assert_eq!(a.horizontal, HorizontalAlign::Right);
}

#[test]
fn dc_09_right_height_auto() {
    let s = parse_single("Right { align-horizontal: right; width: 1fr; height: auto; }");
    assert_eq!(s.height, Some(Scalar::Auto));
}

// =====================================================================
// DC-10: Middle — align-vertical: middle; width: auto
// =====================================================================

#[test]
fn dc_10_middle_align_vertical() {
    let s = parse_single("Middle { align-vertical: middle; width: auto; height: 1fr; }");
    let a = s.align.expect("Middle should have align");
    assert_eq!(a.vertical, VerticalAlign::Middle);
}

#[test]
fn dc_10_middle_width_auto() {
    let s = parse_single("Middle { align-vertical: middle; width: auto; height: 1fr; }");
    assert_eq!(s.width, Some(Scalar::Auto));
}

// =====================================================================
// DC-11: Grid — layout: grid; 1fr dims
// =====================================================================

#[test]
fn dc_11_grid_layout() {
    let s = parse_single("Grid { width: 1fr; height: 1fr; layout: grid; }");
    assert_eq!(s.layout, Some(Layout::Grid));
}

#[test]
fn dc_11_grid_1fr_dims() {
    let s = parse_single("Grid { width: 1fr; height: 1fr; layout: grid; }");
    assert_eq!(s.width, Some(Scalar::Fraction(1.0)));
    assert_eq!(s.height, Some(Scalar::Fraction(1.0)));
}

// =====================================================================
// DC-12: ItemGrid — layout: grid; height: auto
// =====================================================================

#[test]
fn dc_12_itemgrid_layout() {
    let s = parse_single("ItemGrid { width: 1fr; height: auto; layout: grid; }");
    assert_eq!(s.layout, Some(Layout::Grid));
}

#[test]
fn dc_12_itemgrid_height_auto() {
    let s = parse_single("ItemGrid { width: 1fr; height: auto; layout: grid; }");
    assert_eq!(s.height, Some(Scalar::Auto));
}

// =====================================================================
// DC-26: Tooltip — constrain-x: inside; constrain-y: inflect; display: none; max-width: 40
// =====================================================================

#[test]
fn dc_26_tooltip_constrain_x_inside() {
    let s = parse_single("Tooltip { constrain-x: inside; constrain-y: inflect; display: none; max-width: 40; }");
    assert_eq!(s.constrain_x, Some(Constrain::Inside));
}

#[test]
fn dc_26_tooltip_constrain_y_inflect() {
    let s = parse_single("Tooltip { constrain-x: inside; constrain-y: inflect; display: none; max-width: 40; }");
    assert_eq!(s.constrain_y, Some(Constrain::Inflect));
}

#[test]
fn dc_26_tooltip_display_none() {
    let s = parse_single("Tooltip { constrain-x: inside; display: none; max-width: 40; }");
    assert_eq!(s.display, Some(Display::None));
}

#[test]
fn dc_26_tooltip_max_width_40() {
    let s = parse_single("Tooltip { constrain-x: inside; display: none; max-width: 40; }");
    assert_eq!(s.max_width, Some(Scalar::Cells(40)));
}

// =====================================================================
// DC-36: Collapsible — width: 1fr; height: auto; bg: $surface; padding
// =====================================================================

#[test]
fn dc_36_collapsible_width_1fr() {
    let s = parse_single("Collapsible { width: 1fr; height: auto; bg: $surface; padding-bottom: 1; padding-left: 1; }");
    assert_eq!(s.width, Some(Scalar::Fraction(1.0)));
}

#[test]
fn dc_36_collapsible_height_auto() {
    let s = parse_single("Collapsible { width: 1fr; height: auto; bg: $surface; padding-bottom: 1; padding-left: 1; }");
    assert_eq!(s.height, Some(Scalar::Auto));
}

#[test]
fn dc_36_collapsible_bg_surface() {
    let s = parse_single("Collapsible { width: 1fr; height: auto; bg: $surface; padding-bottom: 1; padding-left: 1; }");
    assert!(s.bg.is_some(), "Collapsible should have bg");
}

#[test]
fn dc_36_collapsible_padding_bottom() {
    let s = parse_single("Collapsible { padding-bottom: 1; padding-left: 1; }");
    assert_eq!(s.padding_bottom, Some(1));
}

#[test]
fn dc_36_collapsible_padding_left() {
    let s = parse_single("Collapsible { padding-bottom: 1; padding-left: 1; }");
    assert_eq!(s.padding_left, Some(1));
}

#[test]
fn dc_36_collapsible_title_pointer() {
    let s = parse_single("CollapsibleTitle { width: auto; height: auto; pointer: pointer; }");
    assert_eq!(s.pointer, Some(Pointer::Pointer));
}

#[test]
fn dc_36_collapsible_title_width_auto() {
    let s = parse_single("CollapsibleTitle { width: auto; height: auto; pointer: pointer; }");
    assert_eq!(s.width, Some(Scalar::Auto));
}

#[test]
fn dc_36_collapsible_nested_rules_parse() {
    // Verify the full Collapsible CSS with nesting parses without panic.
    let sheet = StyleSheet::parse(r#"
        Collapsible {
            width: 1fr;
            height: auto;
            bg: $surface;
            border-top: hkey $background;
            padding-bottom: 1;
            padding-left: 1;
            &:focus-within { background-tint: $foreground 5%; }
            &.-collapsed > Contents { display: none; }
        }
    "#);
    assert!(sheet.rules().len() >= 1, "Collapsible CSS should parse");
}

#[test]
fn dc_36_collapsible_title_nested_rules_parse() {
    // Verify the full CollapsibleTitle CSS with nesting parses without panic.
    let sheet = StyleSheet::parse(r#"
        CollapsibleTitle {
            width: auto;
            height: auto;
            padding: 0 1;
            text-style: $block-cursor-blurred-text-style;
            color: $block-cursor-blurred-foreground;
            pointer: pointer;
            &:hover { bg: $block-hover-background; color: $foreground; }
            &:focus { text-style: $block-cursor-text-style; bg: $block-cursor-background; color: $block-cursor-foreground; }
        }
    "#);
    assert!(sheet.rules().len() >= 1, "CollapsibleTitle CSS should parse");
}

// =====================================================================
// DC-01: Screen — :inline and :ansi variants
// =====================================================================

#[test]
fn dc_01_screen_inline_parses() {
    let sheet = StyleSheet::parse(r#"
        Screen {
            layout: vertical; overflow-y: auto; bg: $background;
            &:inline { height: auto; min-height: 1; border-top: tall $background; border-bottom: tall $background; }
        }
    "#);
    assert!(sheet.rules().len() >= 2, "Screen with :inline should produce nested rules");
}

#[test]
fn dc_01_screen_ansi_parses() {
    let sheet = StyleSheet::parse(r#"
        Screen {
            layout: vertical; overflow-y: auto; bg: $background;
            &:ansi {
                background: ansi_default; color: ansi_default;
                &.-screen-suspended { text-style: dim; }
            }
        }
    "#);
    assert!(sheet.rules().len() >= 2, "Screen with :ansi nesting should parse");
}

#[test]
fn dc_01_screen_selection_class_parses() {
    let sheet = StyleSheet::parse(r#"
        Screen {
            & > .screen--selection { background: $primary 50%; }
        }
    "#);
    assert!(sheet.rules().len() >= 1, "Screen with .screen--selection should parse");
}

// =====================================================================
// DC-03: ModalScreen — :ansi variant
// =====================================================================

#[test]
fn dc_03_modalscreen_ansi_parses() {
    let sheet = StyleSheet::parse(r#"
        ModalScreen {
            layout: vertical; overflow-y: auto; bg: $background;
            &:ansi { background: transparent; }
        }
    "#);
    assert!(sheet.rules().len() >= 2, "ModalScreen with :ansi should produce nested rules");
}

// =====================================================================
// DC-05: Label — secondary and accent variants
// =====================================================================

#[test]
fn dc_05_label_variant_secondary_parses() {
    let sheet = StyleSheet::parse(
        r#"Label { &.secondary { color: $text-secondary; bg: $secondary-muted; } }"#,
    );
    assert!(sheet.rules().len() >= 1);
}

#[test]
fn dc_05_label_variant_accent_parses() {
    let sheet = StyleSheet::parse(
        r#"Label { &.accent { color: $text-accent; bg: $accent-muted; } }"#,
    );
    assert!(sheet.rules().len() >= 1);
}

// =====================================================================
// Combined stylesheet parse test
// =====================================================================

#[test]
fn dc_all_combined_stylesheet_parses() {
    let sheet = textual::css::default_widget_stylesheet();
    assert!(
        sheet.rules().len() > 30,
        "combined stylesheet should have many rules"
    );
}
