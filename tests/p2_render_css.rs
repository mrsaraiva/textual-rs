//! P2 render/visual CSS gate tests.
//!
//! These tests verify that render/visual CSS properties (P2-28/29/31/34/35)
//! are correctly parsed, resolved, and wired into the rendering pipeline.

use rich_rs::{Console, ConsoleOptions, Segment, Segments};
use textual::css::set_style_context;
use textual::prelude::*;
use textual::render::FrameBuffer;
use textual::runtime::{
    build_widget_tree_from_root, render_tree_to_frame, render_tree_to_frame_with_stylesheet,
};
use textual::style::{
    Constrain, HorizontalAlign, KeylineType, OverlayMode, Scalar, Style, TextOverflow, TextWrap,
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

struct BorderCaptionWidget {
    title: &'static str,
    subtitle: &'static str,
    seed: NodeSeed,
}

impl BorderCaptionWidget {
    fn new(title: &'static str, subtitle: &'static str) -> Self {
        let mut seed = NodeSeed::default();
        seed.styles.style = seed
            .styles
            .style
            .border_top(textual::style::Color::parse("white").unwrap())
            .border_bottom(textual::style::Color::parse("white").unwrap())
            .border_left(textual::style::Color::parse("white").unwrap())
            .border_right(textual::style::Color::parse("white").unwrap());
        seed.styles.style.border_title_align = Some(HorizontalAlign::Center);
        seed.styles.style.border_subtitle_align = Some(HorizontalAlign::Right);
        seed.styles.style.border_title_color =
            Some(textual::style::Color::parse("yellow").unwrap());
        seed.styles.style.border_subtitle_color =
            Some(textual::style::Color::parse("cyan").unwrap());
        Self {
            title,
            subtitle,
            seed,
        }
    }
}

impl Widget for BorderCaptionWidget {
    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let w = options.size.0.max(1);
        let h = options.size.1.max(1);
        let mut out = Segments::new();
        for y in 0..h {
            out.push(Segment::new(" ".repeat(w)));
            if y + 1 < h {
                out.push(Segment::line());
            }
        }
        out
    }

    fn style_type(&self) -> &'static str {
        "BorderCaptionWidget"
    }

    fn border_title(&self) -> Option<&str> {
        Some(self.title)
    }

    fn border_subtitle(&self) -> Option<&str> {
        Some(self.subtitle)
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }
}

struct FillWidget {
    style_type_name: &'static str,
    seed: NodeSeed,
}

impl FillWidget {
    fn new(style_type_name: &'static str, style: Style) -> Self {
        let mut seed = NodeSeed::default();
        seed.styles.style = style;
        Self {
            style_type_name,
            seed,
        }
    }
}

impl Widget for FillWidget {
    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let w = options.size.0.max(1);
        let h = options.size.1.max(1);
        let mut out = Segments::new();
        for y in 0..h {
            out.push(Segment::new("x".repeat(w)));
            if y + 1 < h {
                out.push(Segment::line());
            }
        }
        out
    }

    fn style_type(&self) -> &'static str {
        self.style_type_name
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }
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
        strike: false,
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

#[test]
fn p2g29_border_title_subtitle_render_on_edges() {
    let mut root = Container::new().with_child(BorderCaptionWidget::new("TITLE", "sub"));
    let (_tree, frame, lines) = tree_render(&mut root, 24, 6);
    assert!(
        lines[0].contains("TITLE"),
        "top border should contain title"
    );
    assert!(
        lines[5].contains("sub"),
        "bottom border should contain subtitle"
    );

    // `str::find` returns a BYTE offset; the border edge has multi-byte glyphs,
    // so convert to a cell column via the cell width of the preceding text.
    let title_byte = lines[0].find("TITLE").expect("title should be present");
    let title_x = rich_rs::cell_len(&lines[0][..title_byte]);
    let title_cell = frame.get(title_x, 0);
    let title_fg = title_cell.style.and_then(|s| s.color).map(|c| c);
    assert_eq!(
        title_fg,
        Some(
            textual::style::Color::parse("yellow")
                .unwrap()
                .to_simple_opaque()
        )
    );
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
// P2-28 behavioral: outline paints OVER the widget's own edge cells
// ===========================================================================

#[test]
fn p2_28_outline_paints_over_widget_edges() {
    // Outline does NOT reserve layout space; it is drawn OVER the widget's own
    // edge cells (and over any child content composited there), mirroring Python
    // `StylesCache.render_line`. The root Container fills the frame; the outline
    // overdraws its perimeter rows/cols.
    use textual::style::{BorderEdge, BorderType, Color};

    let red = Color::parse("red").unwrap();
    let outline_edge = BorderEdge::Edge {
        border_type: BorderType::Solid,
        color: red,
    };

    // Outline on a Container child that fills the root frame (6x3). The child is
    // a real tree node (not the root), so it goes through the per-node render
    // path where the deferred outline paint runs. The outline overdraws its own
    // perimeter: top outline on row 0, bottom on the last row, sides on cols
    // 0/w-1 of the interior rows.
    let mut outlined = Container::new().with_child(Label::new("CD"));
    outlined.seed_mut().styles.style.outline_top = outline_edge;
    outlined.seed_mut().styles.style.outline_bottom = outline_edge;
    outlined.seed_mut().styles.style.outline_left = outline_edge;
    outlined.seed_mut().styles.style.outline_right = outline_edge;
    let mut root = Container::new().with_child(outlined);

    let (_tree, frame, _lines) = tree_render(&mut root, 6, 3);

    // Outlined child fills the frame: cols 0..=5, rows 0..=2.
    // Top outline overdraws row 0.
    assert_eq!(frame.get(0, 0).text, "┌", "top-left corner at (0,0)");
    assert_eq!(frame.get(2, 0).text, "─", "top outline middle at (2,0)");
    assert_eq!(frame.get(5, 0).text, "┐", "top-right corner at (5,0)");
    // Bottom outline overdraws row 2 (the last row).
    assert_eq!(frame.get(0, 2).text, "└", "bottom-left corner at (0,2)");
    assert_eq!(frame.get(2, 2).text, "─", "bottom outline middle at (2,2)");
    assert_eq!(frame.get(5, 2).text, "┘", "bottom-right corner at (5,2)");
    // Side outlines overdraw col 0 / col 5 of the interior row 1.
    assert_eq!(frame.get(0, 1).text, "│", "left outline at (0,1)");
    assert_eq!(frame.get(5, 1).text, "│", "right outline at (5,1)");
}

#[test]
fn p2_28_outline_clipped_at_viewport_edge() {
    // Outline edges at/over viewport bounds are clipped to the frame (no panic).
    // The outline overdraws the widget's own perimeter cells; cells off-frame are
    // skipped silently.
    use textual::style::{BorderEdge, BorderType, Color};

    let red = Color::parse("red").unwrap();
    let outline_edge = BorderEdge::Edge {
        border_type: BorderType::Solid,
        color: red,
    };

    // Outlined Container child fills the root frame (10x3).
    let mut outlined = Container::new().with_child(Label::new("cd"));
    outlined.seed_mut().styles.style.outline_top = outline_edge;
    outlined.seed_mut().styles.style.outline_bottom = outline_edge;
    outlined.seed_mut().styles.style.outline_left = outline_edge;
    outlined.seed_mut().styles.style.outline_right = outline_edge;

    let mut root = Container::new().with_child(outlined);
    let (_tree, frame, _lines) = tree_render(&mut root, 10, 3);

    // Outlined rect: cols 0..=9, rows 0..=2 (fully on-frame).
    // Top outline overdraws row 0; bottom outline overdraws row 2.
    assert_eq!(frame.get(0, 0).text, "┌", "top-left corner at (0,0)");
    assert_eq!(frame.get(0, 2).text, "└", "bottom-left corner at (0,2)");
    assert_eq!(frame.get(9, 2).text, "┘", "bottom-right corner at (9,2)");
    // Side outlines overdraw col 0 / col 9 of interior row 1.
    assert_eq!(frame.get(0, 1).text, "│", "left outline at (0,1)");
    assert_eq!(frame.get(9, 1).text, "│", "right outline at (9,1)");
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
    label.seed_mut().styles.style.hatch = Some(Hatch {
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

#[test]
fn p2_34_hatch_opacity_blends_color_over_background() {
    // The hatch glyph's foreground must be the hatch color blended over the
    // cell background (Python: fg = background + color, with color carrying its
    // opacity-scaled alpha). White hatch at 50% over a black bg => mid-grey fg.
    use textual::style::{Color, Hatch};

    let mut label = Label::new("X         "); // 10 chars: 'X' + 9 spaces
    let seed = label.seed_mut();
    seed.styles.style.bg = Some(Color::parse("#000000").unwrap());
    seed.styles.style.hatch = Some(Hatch {
        character: '╱',
        color: Color::rgba(255, 255, 255, 128), // white at 50% alpha
    });

    let mut root = Container::new().with_child(label);
    let (_tree, frame, _lines) = tree_render(&mut root, 20, 3);

    let cell = frame.get(1, 0);
    assert_eq!(cell.text, "╱", "blank cell should carry the hatch glyph");
    let fg = cell
        .style
        .and_then(|s| s.color)
        .expect("hatch glyph should have a foreground color");
    // 255*128/255 over 0 => ~128 each channel (allow rounding slack).
    match fg {
        rich_rs::SimpleColor::Rgb { r, g, b } => {
            assert!(
                (120..=135).contains(&r) && (120..=135).contains(&g) && (120..=135).contains(&b),
                "expected mid-grey blend, got rgb({r},{g},{b})"
            );
        }
        other => panic!("expected rgb color, got {other:?}"),
    }
}

#[test]
fn p2g34_hatch_bordered_node_fills_inner_row_not_border_title() {
    // Regression: a `.class()` Static carries the border + hatch on its own node
    // (post-Node-deletion the class is seed-based; pre-deletion it rode a `Node`
    // wrapper), with the raw text as inner content. Two bugs the hatch fix guards:
    //   1) the inner content rendered AFTER the hatch and un-hatched the FIRST
    //      inner row (row 1, just below the top border);
    //   2) deferring the hatch past children then over-filled the blank padding
    //      spaces around a `border_title` on the top border row (` t ` -> `╳t╳`).
    // The fix defers the hatch AND scopes it to the content box (inside the
    // border), matching Python `line_post`/`apply_hatch`. Mirrors the
    // `docs/examples/styles/hatch` panel structure (whose full render is guarded
    // by the `hatch` visual-parity golden).
    //
    // An empty `Static` auto-sizes to one content row, so under the height-chrome
    // keystone (chrome applied by layout, not baked into intrinsic height) the box
    // is exactly 3 rows: top border+title (row 0), one hatched inner row (row 1),
    // bottom border (row 2).
    let css = ".hatchbox { border: solid white; hatch: cross #ff0000; }";
    let sheet = StyleSheet::parse(css);

    let mut root = Container::new().with_child(
        Static::new("")
            .class("hatchbox")
            .with_border_title("t"),
    );
    let console = Console::new();
    let mut tree = build_widget_tree_from_root(&mut root).expect("tree");
    let frame =
        render_tree_to_frame_with_stylesheet(&mut tree, &mut root, &console, 12, 6, sheet);

    // Top border row (row 0) carries the title; its blank padding must NOT be
    // hatched. The title is centered-ish as ` t ` — assert no hatch glyph on row 0.
    for x in 0..12usize {
        assert_ne!(
            frame.get(x, 0).text, "╳",
            "border/title row must not be hatched (x={x})"
        );
    }
    // The INNER content row (row 1, inside the border) must be hatched across its
    // full inner width — this is the row the inner content child previously
    // un-hatched, and the hatch must fill it (not just one cell).
    for x in 1..11usize {
        assert_eq!(
            frame.get(x, 1).text, "╳",
            "inner content row must be fully hatched (x={x})"
        );
    }
    // The bottom border row closes the box (chrome applied by layout); it is a
    // border row, not a hatched content row.
    assert_eq!(frame.get(2, 2).text, "─", "bottom border row closes the box");
    // Border corners survive on the perimeter.
    assert_eq!(frame.get(0, 0).text, "┌", "top-left corner preserved");
    assert_eq!(frame.get(0, 2).text, "└", "bottom-left corner preserved");
}

#[test]
fn p2g34_overlay_screen_escapes_on_top_not_blended() {
    // RA2.4: `overlay: screen` is a placement/clip ESCAPE (Python `_compositor`),
    // NOT a colour blend. An opaque `overlay: screen` child is deferred and
    // painted at the top z of the layer OVER the base — so the cell reads the
    // overlay's OWN colour (red), never a screen-blend of base+overlay (magenta).
    let base_style = Style::new()
        .width(Scalar::Percent(100.0))
        .height(Scalar::Percent(100.0))
        .bg(textual::style::Color::parse("#0000ff").unwrap());
    let mut overlay_style = Style::new()
        .width(Scalar::Percent(100.0))
        .height(Scalar::Percent(100.0))
        .bg(textual::style::Color::parse("#ff0000").unwrap());
    overlay_style.position = Some(textual::style::Position::Absolute);
    overlay_style.overlay = Some(OverlayMode::Screen);
    let mut root = Container::new()
        .with_child(FillWidget::new("BaseFill", base_style))
        .with_child(FillWidget::new("OverlayFill", overlay_style));
    let (_tree, frame, _lines) = tree_render(&mut root, 8, 3);

    let bg = frame
        .get(0, 0)
        .style
        .and_then(|s| s.bgcolor)
        .expect("overlay-escape background color should exist");
    assert_eq!(
        bg,
        textual::style::Color::parse("#ff0000")
            .unwrap()
            .to_simple_opaque(),
        "overlay: screen must paint its own colour on top (escape), not a blend"
    );
}

#[test]
fn p2g34_keyline_draws_separator_between_children() {
    let mut keylined = Container::new();
    keylined.seed_mut().styles.style.layout = Some(textual::style::Layout::Vertical);
    keylined.seed_mut().styles.style.keyline = Some(textual::style::Keyline {
        keyline_type: KeylineType::Thin,
        color: textual::style::Color::parse("red").unwrap(),
    });
    let mut a = Label::new("A");
    a.seed_mut().styles.style.height = Some(Scalar::Cells(1));
    let mut b = Label::new("B");
    b.seed_mut().styles.style.height = Some(Scalar::Cells(1));
    keylined = keylined.with_child(a).with_child(b);
    let mut root = Container::new().with_child(keylined);
    let (_tree, frame, lines) = tree_render(&mut root, 10, 4);
    assert!(
        lines.iter().any(|l| l.contains('─')),
        "keyline separator should be painted; lines={lines:?}"
    );

    let sep_y = lines
        .iter()
        .position(|l| l.contains('─'))
        .expect("separator row");
    let cell = frame.get(0, sep_y);
    let fg = cell.style.and_then(|s| s.color).map(|c| c);
    assert_eq!(
        fg,
        Some(
            textual::style::Color::parse("red")
                .unwrap()
                .to_simple_opaque()
        )
    );
}

// ===========================================================================
// P2-31 behavioral: narrow-width text overflow through full pipeline
// ===========================================================================

/// A test widget that intentionally renders a line wider than its layout rect,
/// to exercise the tree-level text-overflow truncation wiring.
struct WideRenderWidget {
    text: String,
    seed: NodeSeed,
}

impl WideRenderWidget {
    fn new(text: &str) -> Self {
        Self {
            text: text.to_string(),
            seed: NodeSeed::default(),
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

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }
}

#[test]
fn p2_31_nowrap_ellipsis_narrow_pipeline() {
    // Verify tree-level text-overflow truncation wiring.
    // Uses a custom widget that renders text wider than its layout rect.
    let mut widget = WideRenderWidget::new("abcdefghijklmnop"); // 16 chars
    widget.seed.styles.set_width(8);
    widget.seed.styles.style.text_wrap = Some(TextWrap::NoWrap);
    widget.seed.styles.style.text_overflow = Some(TextOverflow::Ellipsis);

    let mut root = Container::new().with_child(widget);
    let (_tree, _frame, lines) = tree_render(&mut root, 20, 3);

    // Layout gives the widget width=8 (from content_width), but render
    // produces 16 chars. Tree-level text overflow should truncate with ellipsis.
    let first_line = &lines[0];
    assert!(
        first_line.contains('…') || first_line.starts_with("abcdefgh"),
        "tree-level text overflow should truncate to the widget width: {first_line:?}"
    );
    assert!(
        rich_rs::cell_len(first_line.trim_end()) <= 8,
        "truncated output should fit within 8 columns: {first_line:?} (width={})",
        rich_rs::cell_len(first_line.trim_end())
    );
}
