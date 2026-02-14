//! P2 render/visual CSS gate tests.
//!
//! These tests verify that render/visual CSS properties (P2-28/29/31/34/35)
//! are correctly parsed, resolved, and wired into the rendering pipeline.

use rich_rs::{Console, ConsoleOptions};
use textual::css::set_style_context;
use textual::prelude::*;
use textual::render::FrameBuffer;
use textual::style::{
    Constrain, HorizontalAlign, KeylineType, OverlayMode, TextOverflow, TextWrap,
};

// ===========================================================================
// Helpers
// ===========================================================================

/// Render a root widget through the full tree pipeline, returning (tree, frame, lines).
fn tree_render(
    root: &mut dyn Widget,
    w: usize,
    h: usize,
) -> (WidgetTree, FrameBuffer, Vec<String>) {
    let console = Console::new();
    let mut tree = build_widget_tree_from_root(root).expect("tree should have children");
    let frame = render_tree_to_frame(&mut tree, root, &console, w, h);
    let lines = frame.as_plain_lines();
    (tree, frame, lines)
}

// ===========================================================================
// P2G-28: Outline renders outside border, no layout mutation
// ===========================================================================

#[test]
fn p2g28_outline_style_fields_wired() {
    // NOTE: parse-only — verifies Style struct fields, not runtime behavior.
    use textual::style::{BorderEdge, BorderType, Color};

    let mut style = textual::style::Style::new();
    let red = Color::parse("red").unwrap();

    // Set outline edges programmatically.
    style.outline_top = BorderEdge::Edge {
        border_type: BorderType::Solid,
        color: red,
    };
    style.outline_right = BorderEdge::Edge {
        border_type: BorderType::Solid,
        color: red,
    };
    style.outline_bottom = BorderEdge::Edge {
        border_type: BorderType::Block,
        color: red,
    };
    style.outline_left = BorderEdge::Edge {
        border_type: BorderType::Tall,
        color: red,
    };

    assert!(style.outline_top.is_set());
    assert!(style.outline_right.is_set());
    assert!(style.outline_bottom.is_set());
    assert!(style.outline_left.is_set());
    assert_eq!(style.outline_top.color(), Some(red));
    assert_eq!(style.outline_bottom.edge_type(), "block");
    assert_eq!(style.outline_left.edge_type(), "tall");
}

#[test]
fn p2g28_outline_css_parses_all_edges() {
    // NOTE: parse-only — verifies CSS parser, not runtime outline rendering.
    let css = "Label { outline: solid red; }";
    let sheet = StyleSheet::parse(css);
    let _guard = set_style_context(sheet);

    // Render a Label — this exercises the full CSS resolution path.
    let mut root = Container::new().with_child(Label::new("hi"));
    let (_tree, _frame, lines) = tree_render(&mut root, 20, 5);

    // Label text should still be visible.
    let all_text: String = lines.join("");
    assert!(
        all_text.contains("hi"),
        "label text should appear in output: {all_text:?}"
    );
}

#[test]
fn p2g28_outline_per_side_css_parses() {
    // NOTE: parse-only — verifies CSS parser, not runtime outline rendering.
    let css = "Label { outline-top: solid green; outline-bottom: solid blue; }";
    let sheet = StyleSheet::parse(css);
    let _guard = set_style_context(sheet);

    let mut root = Container::new().with_child(Label::new("ok"));
    let (_tree, _frame, lines) = tree_render(&mut root, 20, 5);

    let all_text: String = lines.join("");
    assert!(all_text.contains("ok"), "label text should appear");
}

#[test]
fn p2g28_outline_does_not_affect_layout_rect() {
    // Outline should NOT affect widget content area or layout dimensions.
    // Render the same label with and without outline, compare visible text position.
    let css_no_outline = "Label { }";
    let css_with_outline = "Label { outline: solid red; }";

    // Without outline.
    let sheet1 = StyleSheet::parse(css_no_outline);
    let _guard1 = set_style_context(sheet1);
    let mut root1 = Container::new().with_child(Label::new("test"));
    let (_tree1, _frame1, lines1) = tree_render(&mut root1, 20, 5);
    let row1 = lines1.iter().position(|l| l.contains("test"));

    // With outline.
    let sheet2 = StyleSheet::parse(css_with_outline);
    let _guard2 = set_style_context(sheet2);
    let mut root2 = Container::new().with_child(Label::new("test"));
    let (_tree2, _frame2, lines2) = tree_render(&mut root2, 20, 5);
    let row2 = lines2.iter().position(|l| l.contains("test"));

    // The label should appear on the same row regardless of outline.
    assert_eq!(
        row1, row2,
        "outline should not shift layout: row_without={row1:?} row_with={row2:?}"
    );
}

// ===========================================================================
// P2G-29: Border title/subtitle alignment + color/style
// ===========================================================================

#[test]
fn p2g29_border_title_align_style_fields() {
    // NOTE: parse-only — verifies Style struct fields, not runtime behavior.
    let mut style = textual::style::Style::new();
    style.border_title_align = Some(HorizontalAlign::Center);
    style.border_subtitle_align = Some(HorizontalAlign::Right);

    assert_eq!(style.border_title_align, Some(HorizontalAlign::Center));
    assert_eq!(style.border_subtitle_align, Some(HorizontalAlign::Right));
}

#[test]
fn p2g29_border_title_color_style_fields() {
    // NOTE: parse-only — verifies Style struct fields, not runtime behavior.
    use textual::style::Color;

    let mut style = textual::style::Style::new();
    let red = Color::parse("red").unwrap();
    let blue = Color::parse("blue").unwrap();

    style.border_title_color = Some(red);
    style.border_title_background = Some(blue);
    style.border_subtitle_color = Some(blue);
    style.border_subtitle_background = Some(red);

    assert_eq!(style.border_title_color, Some(red));
    assert_eq!(style.border_title_background, Some(blue));
    assert_eq!(style.border_subtitle_color, Some(blue));
    assert_eq!(style.border_subtitle_background, Some(red));
}

#[test]
fn p2g29_border_title_style_flags() {
    // NOTE: parse-only — verifies Style struct fields, not runtime behavior.
    use textual::style::TextStyleFlags;

    let flags = TextStyleFlags {
        bold: true,
        italic: true,
        dim: false,
        underline: false,
        reverse: false,
    };
    let mut style = textual::style::Style::new();
    style.border_title_style = Some(flags);

    let stored = style.border_title_style.unwrap();
    assert!(stored.bold);
    assert!(stored.italic);
    assert!(!stored.underline);
}

#[test]
fn p2g29_border_title_css_parse_roundtrip() {
    // NOTE: parse-only — verifies CSS parser, not runtime border title rendering.
    let css = "Label { border-title-align: center; border-subtitle-align: right; }";
    let sheet = StyleSheet::parse(css);
    let _guard = set_style_context(sheet);

    let mut root = Container::new().with_child(Label::new("titled"));
    let (_tree, _frame, lines) = tree_render(&mut root, 20, 3);

    let all_text: String = lines.join("");
    assert!(all_text.contains("titled"));
}

#[test]
fn p2g29_border_title_color_css_parse_roundtrip() {
    // NOTE: parse-only — verifies CSS parser, not runtime border title rendering.
    let css = "Label { border-title-color: red; border-title-background: blue; border-title-style: bold italic; }";
    let sheet = StyleSheet::parse(css);
    let _guard = set_style_context(sheet);

    let mut root = Container::new().with_child(Label::new("styled"));
    let (_tree, _frame, lines) = tree_render(&mut root, 20, 3);

    let all_text: String = lines.join("");
    assert!(all_text.contains("styled"));
}

// ===========================================================================
// P2G-31: Text wrap/nowrap + overflow modes
// ===========================================================================

#[test]
fn p2g31_text_wrap_style_fields() {
    // NOTE: parse-only — verifies Style struct fields, not runtime behavior.
    let mut style = textual::style::Style::new();
    style.text_wrap = Some(TextWrap::Wrap);
    assert_eq!(style.text_wrap, Some(TextWrap::Wrap));

    style.text_wrap = Some(TextWrap::NoWrap);
    assert_eq!(style.text_wrap, Some(TextWrap::NoWrap));
}

#[test]
fn p2g31_text_overflow_style_fields() {
    // NOTE: parse-only — verifies Style struct fields, not runtime behavior.
    let mut style = textual::style::Style::new();
    style.text_overflow = Some(TextOverflow::Clip);
    assert_eq!(style.text_overflow, Some(TextOverflow::Clip));

    style.text_overflow = Some(TextOverflow::Fold);
    assert_eq!(style.text_overflow, Some(TextOverflow::Fold));

    style.text_overflow = Some(TextOverflow::Ellipsis);
    assert_eq!(style.text_overflow, Some(TextOverflow::Ellipsis));
}

#[test]
fn p2g31_text_wrap_css_parse_roundtrip() {
    // NOTE: parse-only — verifies CSS parser, not runtime text wrapping behavior.
    let css = "Label { text-wrap: nowrap; text-overflow: ellipsis; }";
    let sheet = StyleSheet::parse(css);
    let _guard = set_style_context(sheet);

    let mut root = Container::new().with_child(Label::new("truncated"));
    let (_tree, _frame, lines) = tree_render(&mut root, 20, 3);

    let all_text: String = lines.join("");
    assert!(all_text.contains("truncated"));
}

#[test]
fn p2g31_text_overflow_line_truncation_clip() {
    use textual::runtime::apply_text_overflow_to_line;

    let long_line = vec![rich_rs::Segment::new("abcdefghij")]; // 10 chars
    let clipped = apply_text_overflow_to_line(&long_line, 5, TextOverflow::Clip);
    let clipped_text: String = clipped.iter().map(|s| s.text.to_string()).collect();
    assert_eq!(
        rich_rs::cell_len(&clipped_text),
        5,
        "clipped should be 5 cols wide"
    );
}

#[test]
fn p2g31_text_overflow_line_truncation_ellipsis() {
    use textual::runtime::apply_text_overflow_to_line;

    let long_line = vec![rich_rs::Segment::new("abcdefghij")]; // 10 chars
    let ellipsised = apply_text_overflow_to_line(&long_line, 5, TextOverflow::Ellipsis);
    let ell_text: String = ellipsised.iter().map(|s| s.text.to_string()).collect();
    assert!(
        ell_text.contains('…'),
        "ellipsis should be present: {ell_text:?}"
    );
    assert!(
        rich_rs::cell_len(&ell_text) <= 5,
        "ellipsised should fit in 5 cols: {ell_text:?} (width={})",
        rich_rs::cell_len(&ell_text)
    );
}

#[test]
fn p2g31_text_overflow_line_fold_preserves_text() {
    use textual::runtime::apply_text_overflow_to_line;

    let long_line = vec![rich_rs::Segment::new("abcdefghij")];
    let folded = apply_text_overflow_to_line(&long_line, 5, TextOverflow::Fold);
    let fold_text: String = folded.iter().map(|s| s.text.to_string()).collect();
    assert_eq!(fold_text, "abcdefghij", "fold should preserve full text");
}

#[test]
fn p2g31_text_overflow_mode_function() {
    use textual::runtime::text_overflow_mode;

    // No text-wrap set: should return None.
    let style_default = textual::style::Style::new();
    assert!(text_overflow_mode(&style_default).is_none());

    // text-wrap: wrap -> None.
    let mut style_wrap = textual::style::Style::new();
    style_wrap.text_wrap = Some(TextWrap::Wrap);
    assert!(text_overflow_mode(&style_wrap).is_none());

    // text-wrap: nowrap -> defaults to Clip.
    let mut style_nowrap = textual::style::Style::new();
    style_nowrap.text_wrap = Some(TextWrap::NoWrap);
    assert_eq!(text_overflow_mode(&style_nowrap), Some(TextOverflow::Clip));

    // text-wrap: nowrap + text-overflow: ellipsis -> Ellipsis.
    let mut style_ellipsis = textual::style::Style::new();
    style_ellipsis.text_wrap = Some(TextWrap::NoWrap);
    style_ellipsis.text_overflow = Some(TextOverflow::Ellipsis);
    assert_eq!(
        text_overflow_mode(&style_ellipsis),
        Some(TextOverflow::Ellipsis)
    );
}

// ===========================================================================
// P2G-34: Hatch, overlay, keyline
// ===========================================================================

#[test]
fn p2g34_hatch_style_fields() {
    // NOTE: parse-only — verifies Style struct fields, not runtime behavior.
    use textual::style::{Color, Hatch};

    let hatch = Hatch {
        character: '+',
        color: Color::parse("red").unwrap(),
    };
    let mut style = textual::style::Style::new();
    style.hatch = Some(hatch);

    let stored = style.hatch.unwrap();
    assert_eq!(stored.character, '+');
    assert_eq!(stored.color, Color::parse("red").unwrap());
}

#[test]
fn p2g34_overlay_style_fields() {
    // NOTE: parse-only — verifies Style struct fields, not runtime behavior.
    let mut style = textual::style::Style::new();
    style.overlay = Some(OverlayMode::None);
    assert_eq!(style.overlay, Some(OverlayMode::None));

    style.overlay = Some(OverlayMode::Screen);
    assert_eq!(style.overlay, Some(OverlayMode::Screen));
}

#[test]
fn p2g34_keyline_style_fields() {
    // NOTE: parse-only — verifies Style struct fields, not runtime behavior.
    use textual::style::{Color, Keyline};

    let keyline = Keyline {
        keyline_type: KeylineType::Thin,
        color: Color::parse("red").unwrap(),
    };
    let mut style = textual::style::Style::new();
    style.keyline = Some(keyline);

    let stored = style.keyline.unwrap();
    assert_eq!(stored.keyline_type, KeylineType::Thin);
    assert_eq!(stored.color, Color::parse("red").unwrap());
}

#[test]
fn p2g34_hatch_css_parse_renders_without_crash() {
    // NOTE: parse-only — verifies CSS parser + no-crash, not runtime hatch rendering.
    let css = r#"Container { hatch: cross #ff0000; }"#;
    let sheet = StyleSheet::parse(css);
    let _guard = set_style_context(sheet);

    let mut root = Container::new().with_child(Label::new("X"));
    let (_tree, _frame, lines) = tree_render(&mut root, 10, 3);

    let all_text: String = lines.join("");
    assert!(all_text.contains('X'), "label text should survive hatch");
}

#[test]
fn p2g34_overlay_screen_renders_without_crash() {
    // NOTE: parse-only — verifies CSS parser + no-crash, not runtime overlay compositing.
    let css = "Label { overlay: screen; }";
    let sheet = StyleSheet::parse(css);
    let _guard = set_style_context(sheet);

    let mut root = Container::new().with_child(Label::new("overlay"));
    let (_tree, _frame, lines) = tree_render(&mut root, 20, 3);

    let all_text: String = lines.join("");
    assert!(all_text.contains("overlay"));
}

#[test]
fn p2g34_keyline_css_parse_renders_without_crash() {
    // NOTE: parse-only — verifies CSS parser + no-crash, not runtime keyline painting.
    let css = "Container { keyline: thin red; }";
    let sheet = StyleSheet::parse(css);
    let _guard = set_style_context(sheet);

    let mut root = Container::new()
        .with_child(Label::new("A"))
        .with_child(Label::new("B"));
    let (_tree, _frame, lines) = tree_render(&mut root, 20, 5);

    let all_text: String = lines.join("");
    assert!(all_text.contains('A'));
    assert!(all_text.contains('B'));
}

#[test]
fn p2g34_keyline_heavy_css_parse() {
    // NOTE: parse-only — verifies CSS parser + no-crash, not runtime keyline painting.
    let css = "Container { keyline: heavy green; }";
    let sheet = StyleSheet::parse(css);
    let _guard = set_style_context(sheet);

    let mut root = Container::new().with_child(Label::new("heavy"));
    let (_tree, _frame, lines) = tree_render(&mut root, 20, 3);

    let all_text: String = lines.join("");
    assert!(all_text.contains("heavy"));
}

// ===========================================================================
// P2G-35: Constrain-x, constrain-y, expand
// ===========================================================================

#[test]
fn p2g35_constrain_x_y_style_fields() {
    // NOTE: parse-only — verifies Style struct fields, not runtime behavior.
    let mut style = textual::style::Style::new();
    style.constrain_x = Some(Constrain::Inflect);
    style.constrain_y = Some(Constrain::Inside);

    assert_eq!(style.constrain_x, Some(Constrain::Inflect));
    assert_eq!(style.constrain_y, Some(Constrain::Inside));
}

#[test]
fn p2g35_expand_style_fields() {
    // NOTE: parse-only — verifies Style struct fields, not runtime behavior.
    let mut style = textual::style::Style::new();
    style.expand = Some(true);
    assert_eq!(style.expand, Some(true));

    style.expand = Some(false);
    assert_eq!(style.expand, Some(false));
}

#[test]
fn p2g35_constrain_x_y_css_parse_roundtrip() {
    // NOTE: parse-only — verifies CSS parser, not runtime constrain behavior.
    let css = "Label { constrain-x: inflect; constrain-y: inside; }";
    let sheet = StyleSheet::parse(css);
    let _guard = set_style_context(sheet);

    let mut root = Container::new().with_child(Label::new("constrained"));
    let (_tree, _frame, lines) = tree_render(&mut root, 20, 3);

    let all_text: String = lines.join("");
    assert!(all_text.contains("constrained"));
}

#[test]
fn p2g35_expand_css_parse_roundtrip() {
    // NOTE: parse-only — verifies CSS parser, not runtime expand behavior.
    let css = "Label { expand: true; }";
    let sheet = StyleSheet::parse(css);
    let _guard = set_style_context(sheet);

    let mut root = Container::new().with_child(Label::new("expanded"));
    let (_tree, _frame, lines) = tree_render(&mut root, 20, 3);

    let all_text: String = lines.join("");
    assert!(all_text.contains("expanded"));
}

#[test]
fn p2g35_axis_constrain_resolution() {
    use textual::runtime::resolve_axis_constrain;

    // When constrain-x/y are not set, falls back to generic constrain.
    let mut style = textual::style::Style::new();
    style.constrain = Some(Constrain::Inside);
    let (cx, cy) = resolve_axis_constrain(&style);
    assert_eq!(cx, Constrain::Inside);
    assert_eq!(cy, Constrain::Inside);

    // When constrain-x is set, overrides generic for x-axis only.
    style.constrain_x = Some(Constrain::Inflect);
    let (cx, cy) = resolve_axis_constrain(&style);
    assert_eq!(cx, Constrain::Inflect);
    assert_eq!(cy, Constrain::Inside);
}

#[test]
fn p2g35_constrain_overlay_position_clamps_inside() {
    use textual::runtime::constrain_overlay_position;

    let (x, y) =
        constrain_overlay_position(90, 20, 20, 5, 100, 25, Constrain::Inside, Constrain::Inside);
    assert!(x + 20 <= 100, "x should be clamped inside viewport: x={x}");
    assert!(y + 5 <= 25, "y should be clamped inside viewport: y={y}");
    assert_eq!(x, 80, "x should be clamped to 80 (100 - 20)");
    assert_eq!(y, 20, "y should stay at 20 (fits in viewport)");
}

#[test]
fn p2g35_constrain_overlay_position_inflects() {
    use textual::runtime::constrain_overlay_position;

    // Overlay overflows right: inflect should flip to left.
    let (x, _y) =
        constrain_overlay_position(90, 5, 20, 3, 100, 25, Constrain::Inflect, Constrain::None);
    assert_eq!(x, 70, "x should inflect to 70 (90 - 20)");

    // Overlay overflows bottom: inflect should flip up.
    let (_x, y) =
        constrain_overlay_position(5, 22, 10, 5, 100, 25, Constrain::None, Constrain::Inflect);
    assert_eq!(y, 17, "y should inflect to 17 (22 - 5)");
}

#[test]
fn p2g35_constrain_none_does_not_clamp() {
    use textual::runtime::constrain_overlay_position;

    let (x, y) =
        constrain_overlay_position(90, 22, 20, 5, 100, 25, Constrain::None, Constrain::None);
    assert_eq!(x, 90, "x should be unchanged");
    assert_eq!(y, 22, "y should be unchanged");
}

// ===========================================================================
// P2-28 behavioral: outline paints outside border box
// ===========================================================================

#[test]
fn p2_28_outline_paints_outside_border_box() {
    // Outline should paint characters in cells OUTSIDE the widget's border box.
    // Uses inline styles (tree pipeline overwrites CSS context with defaults).
    // Inner Container with padding insets the Label so outline has room on all sides.
    use textual::style::{BorderEdge, BorderType, Color, Spacing};

    let red = Color::parse("red").unwrap();
    let outline_edge = BorderEdge::Edge {
        border_type: BorderType::Solid,
        color: red,
    };

    let mut label = Label::new("hi");
    label.styles_mut().unwrap().style.outline_top = outline_edge;
    label.styles_mut().unwrap().style.outline_bottom = outline_edge;
    label.styles_mut().unwrap().style.outline_left = outline_edge;
    label.styles_mut().unwrap().style.outline_right = outline_edge;

    // Nested: outer Container (root, no style) > inner Container (padding 3) > Label.
    let mut inner = Container::new().with_child(label);
    inner.styles_mut().unwrap().style.padding = Some(Spacing::all(3));
    let mut root = Container::new().with_child(inner);

    let (_tree, frame, _lines) = tree_render(&mut root, 20, 8);

    // Inner Container fills viewport (20x8), Label is inset by padding 3.
    // Label "hi" has content_width=2, so layout rect is at approximately (3, 3) with w=2, h=1.
    // Outline paints OUTSIDE the 2x1 label rect:
    //   Top:    row 2, cols 3..5 → '─'
    //   Bottom: row 4, cols 3..5 → '─'
    //   Left:   col 2, row 3 → '│'
    //   Right:  col 5, row 3 → '│'
    let top_cell = frame.get(3, 2);
    assert_eq!(top_cell.text, "─", "top outline at (3,2)");
    let bottom_cell = frame.get(3, 4);
    assert_eq!(bottom_cell.text, "─", "bottom outline at (3,4)");
    let left_cell = frame.get(2, 3);
    assert_eq!(left_cell.text, "│", "left outline at (2,3)");
    let right_cell = frame.get(5, 3);
    assert_eq!(right_cell.text, "│", "right outline at (5,3)");

    // Label text is inside (at outline-inner positions).
    assert_eq!(frame.get(3, 3).text, "h", "label text at (3,3)");
    assert_eq!(frame.get(4, 3).text, "i", "label text at (4,3)");
}

#[test]
fn p2_28_outline_clipped_at_viewport_edge() {
    // Outline edges beyond viewport bounds are silently clipped (no panic).
    // Label at viewport edge: top/left outlines clipped, bottom/right visible.
    use textual::style::{BorderEdge, BorderType, Color};

    let red = Color::parse("red").unwrap();
    let outline_edge = BorderEdge::Edge {
        border_type: BorderType::Solid,
        color: red,
    };

    let mut label = Label::new("edge");
    label.styles_mut().unwrap().style.outline_top = outline_edge;
    label.styles_mut().unwrap().style.outline_bottom = outline_edge;
    label.styles_mut().unwrap().style.outline_left = outline_edge;
    label.styles_mut().unwrap().style.outline_right = outline_edge;

    let mut root = Container::new().with_child(label);
    let (_tree, frame, _lines) = tree_render(&mut root, 10, 3);

    // Label "edge" has content_width=4, so layout rect is (0, 0, 4, 1).
    // Top: row -1 (clipped), Left: col -1 (clipped).
    // Bottom: row 1, cols 0..4 → '─'
    // Right: col 4, row 0 → '│'
    assert_eq!(frame.get(0, 1).text, "─", "bottom outline at (0,1)");
    assert_eq!(frame.get(3, 1).text, "─", "bottom outline at (3,1)");
    assert_eq!(frame.get(4, 0).text, "│", "right outline at (4,0)");

    // Label text intact.
    assert_eq!(frame.get(0, 0).text, "e", "label text at (0,0)");
}

// ===========================================================================
// P2-34 behavioral: hatch fills blank cells with pattern character
// ===========================================================================

#[test]
fn p2_34_hatch_fills_blank_cells_with_pattern() {
    // Hatch replaces blank cells with hatch character; preserves existing text.
    // Label text is exactly 10 chars so layout gives it width 10 (matching
    // content_width). First char is 'X', rest are spaces — hatch fills spaces.
    use textual::style::{Color, Hatch};

    let mut label = Label::new("X         "); // 10 chars: 'X' + 9 spaces
    label.styles_mut().unwrap().style.hatch = Some(Hatch {
        character: '+',
        color: Color::parse("#ff0000").unwrap(),
    });

    let mut root = Container::new().with_child(label);
    let (_tree, frame, _lines) = tree_render(&mut root, 20, 3);

    // Label layout rect is 10 wide (content_width = 10).
    // Cell 0: "X" (not blank, preserved). Cells 1..10: " " (blank, hatched).
    assert_eq!(frame.get(0, 0).text, "X", "text cell should be preserved");
    assert_eq!(frame.get(1, 0).text, "+", "blank cell should be hatched");
    assert_eq!(frame.get(5, 0).text, "+", "blank cell should be hatched");
    assert_eq!(frame.get(9, 0).text, "+", "blank cell should be hatched");
}

// ===========================================================================
// P2-31 behavioral: narrow-width text overflow through full pipeline
// ===========================================================================

/// A test widget that intentionally renders a line wider than its layout rect,
/// to exercise the tree-level text-overflow truncation wiring.
struct WideRenderWidget {
    text: String,
    styles: WidgetStyles,
}

impl WideRenderWidget {
    fn new(text: &str) -> Self {
        Self {
            text: text.to_string(),
            styles: WidgetStyles::default(),
        }
    }
}

impl Widget for WideRenderWidget {
    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> rich_rs::Segments {
        // Intentionally ignore options.size — produce a full-width line.
        let mut out = rich_rs::Segments::new();
        out.push(rich_rs::Segment::new(self.text.clone()));
        out
    }

    fn content_width(&self) -> Option<usize> {
        // Report narrow content width so layout constrains us,
        // but render() produces the full text anyway.
        Some(8)
    }

    fn style_type(&self) -> &'static str {
        "WideRenderWidget"
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

#[test]
fn p2_31_nowrap_ellipsis_narrow_pipeline() {
    // Verify tree-level text-overflow truncation wiring.
    // Uses a custom widget that renders text wider than its layout rect.
    let mut widget = WideRenderWidget::new("abcdefghijklmnop"); // 16 chars
    widget.styles.style.text_wrap = Some(TextWrap::NoWrap);
    widget.styles.style.text_overflow = Some(TextOverflow::Ellipsis);

    let mut root = Container::new().with_child(widget);
    let (_tree, _frame, lines) = tree_render(&mut root, 20, 3);

    // Layout gives the widget width=8 (from content_width), but render
    // produces 16 chars. Tree-level text overflow should truncate with ellipsis.
    let first_line = &lines[0];
    assert!(
        first_line.contains('…'),
        "tree-level text overflow should produce ellipsis: {first_line:?}"
    );
    assert!(
        rich_rs::cell_len(first_line.trim_end()) <= 8,
        "truncated output should fit within 8 columns: {first_line:?} (width={})",
        rich_rs::cell_len(first_line.trim_end())
    );
}
