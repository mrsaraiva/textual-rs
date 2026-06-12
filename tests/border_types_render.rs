//! Border-types render tests.
//!
//! Part A: programmatic styles (no parser dependency) — added in Step 2.
//! Part B: CSS end-to-end through the parser — added in Step 3.

use rich_rs::{Console, ConsoleOptions, Segment, Segments};
use textual::prelude::*;
use textual::render::FrameBuffer;
use textual::style::{BorderEdge, BorderType, Color, HorizontalAlign, Spacing, Style};

// ===========================================================================
// Helpers (mirrors p2_render_css.rs)
// ===========================================================================

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

fn tree_render_with_css(
    root: &mut dyn Widget,
    w: usize,
    h: usize,
    css: &str,
) -> (WidgetTree, FrameBuffer, Vec<String>) {
    let console = Console::new();
    let mut tree = build_widget_tree_from_root(root).expect("tree should have children");
    let sheet = StyleSheet::parse(css);
    let frame = render_tree_to_frame_with_stylesheet(&mut tree, root, &console, w, h, sheet);
    let lines = frame.as_plain_lines();
    (tree, frame, lines)
}

struct FillWidget {
    style_type_name: &'static str,
    styles: WidgetStyles,
}

impl FillWidget {
    fn new(style_type_name: &'static str, style: Style) -> Self {
        let mut styles = WidgetStyles::default();
        styles.style = style;
        Self {
            style_type_name,
            styles,
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

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }
}

/// Build a bordered 10×4 frame: Container(FillWidget with all four border edges set).
fn bordered_lines(border_type: BorderType) -> (FrameBuffer, Vec<String>) {
    let red = Color::parse("red").unwrap();
    let edge = BorderEdge::Edge { border_type, color: red };
    let mut style = Style::new();
    style.border_top = edge;
    style.border_right = edge;
    style.border_bottom = edge;
    style.border_left = edge;
    let fill = FillWidget::new("BorderBox", style);
    let mut root = Container::new().with_child(fill);
    let (_tree, frame, lines) = tree_render(&mut root, 10, 4);
    (frame, lines)
}

// ===========================================================================
// Part A — programmatic style tests (Step 2)
// ===========================================================================

#[test]
fn render_round_border_glyphs() {
    let (_frame, lines) = bordered_lines(BorderType::Round);
    // top row: ╭──────────╮ (width 10: corner + 8 dashes + corner)
    let top = &lines[0];
    assert_eq!(
        top.chars().next().unwrap(),
        '╭',
        "top-left corner: {top:?}"
    );
    assert_eq!(
        top.chars().last().unwrap(),
        '╮',
        "top-right corner: {top:?}"
    );
    // interior of top row is all '─'
    let interior: String = top.chars().skip(1).take(8).collect();
    assert!(
        interior.chars().all(|c| c == '─'),
        "top interior should be dashes: {interior:?}"
    );
    // middle row: left '│', right '│'
    let mid = &lines[1];
    assert_eq!(mid.chars().next().unwrap(), '│', "mid-left: {mid:?}");
    assert_eq!(mid.chars().last().unwrap(), '│', "mid-right: {mid:?}");
    // bottom row corners
    let bot = &lines[3];
    assert_eq!(bot.chars().next().unwrap(), '╰', "bot-left: {bot:?}");
    assert_eq!(bot.chars().last().unwrap(), '╯', "bot-right: {bot:?}");
}

#[test]
fn render_double_border_glyphs() {
    let (_frame, lines) = bordered_lines(BorderType::Double);
    let top = &lines[0];
    assert_eq!(top.chars().next().unwrap(), '╔');
    assert_eq!(top.chars().last().unwrap(), '╗');
    let interior: String = top.chars().skip(1).take(8).collect();
    assert!(interior.chars().all(|c| c == '═'), "double top interior: {interior:?}");
    let mid = &lines[1];
    assert_eq!(mid.chars().next().unwrap(), '║');
    assert_eq!(mid.chars().last().unwrap(), '║');
    let bot = &lines[3];
    assert_eq!(bot.chars().next().unwrap(), '╚');
    assert_eq!(bot.chars().last().unwrap(), '╝');
}

#[test]
fn render_dashed_border_glyphs() {
    let (_frame, lines) = bordered_lines(BorderType::Dashed);
    let top = &lines[0];
    assert_eq!(top.chars().next().unwrap(), '┏');
    assert_eq!(top.chars().last().unwrap(), '┓');
    let interior: String = top.chars().skip(1).take(8).collect();
    assert!(interior.chars().all(|c| c == '╍'), "dashed top interior: {interior:?}");
    let mid = &lines[1];
    assert_eq!(mid.chars().next().unwrap(), '╏');
    assert_eq!(mid.chars().last().unwrap(), '╏');
    let bot = &lines[3];
    assert_eq!(bot.chars().next().unwrap(), '┗');
    assert_eq!(bot.chars().last().unwrap(), '┛');
}

#[test]
fn render_ascii_border_glyphs() {
    let (_frame, lines) = bordered_lines(BorderType::Ascii);
    let top = &lines[0];
    assert_eq!(top.chars().next().unwrap(), '+');
    assert_eq!(top.chars().last().unwrap(), '+');
    let interior: String = top.chars().skip(1).take(8).collect();
    assert!(interior.chars().all(|c| c == '-'), "ascii top interior: {interior:?}");
    let mid = &lines[1];
    assert_eq!(mid.chars().next().unwrap(), '|');
    assert_eq!(mid.chars().last().unwrap(), '|');
    let bot = &lines[3];
    assert_eq!(bot.chars().next().unwrap(), '+');
    assert_eq!(bot.chars().last().unwrap(), '+');
}

#[test]
fn render_inner_border_glyphs() {
    let (_frame, lines) = bordered_lines(BorderType::Inner);
    let top = &lines[0];
    assert_eq!(top.chars().next().unwrap(), '▗', "inner top-left: {top:?}");
    assert_eq!(top.chars().last().unwrap(), '▖', "inner top-right: {top:?}");
    let interior: String = top.chars().skip(1).take(8).collect();
    assert!(interior.chars().all(|c| c == '▄'), "inner top interior: {interior:?}");
    let mid = &lines[1];
    assert_eq!(mid.chars().next().unwrap(), '▐', "inner mid-left: {mid:?}");
    assert_eq!(mid.chars().last().unwrap(), '▌', "inner mid-right: {mid:?}");
    let bot = &lines[3];
    assert_eq!(bot.chars().next().unwrap(), '▝', "inner bot-left: {bot:?}");
    assert_eq!(bot.chars().last().unwrap(), '▘', "inner bot-right: {bot:?}");
}

#[test]
fn render_thick_border_glyphs() {
    let (_frame, lines) = bordered_lines(BorderType::Thick);
    let top = &lines[0];
    assert_eq!(top.chars().next().unwrap(), '█', "thick top-left: {top:?}");
    assert_eq!(top.chars().last().unwrap(), '█', "thick top-right: {top:?}");
    let interior: String = top.chars().skip(1).take(8).collect();
    assert!(interior.chars().all(|c| c == '▀'), "thick top interior: {interior:?}");
    let mid = &lines[1];
    assert_eq!(mid.chars().next().unwrap(), '█', "thick mid-left: {mid:?}");
    assert_eq!(mid.chars().last().unwrap(), '█', "thick mid-right: {mid:?}");
    let bot = &lines[3];
    assert_eq!(bot.chars().next().unwrap(), '█', "thick bot-left: {bot:?}");
    assert_eq!(bot.chars().last().unwrap(), '█', "thick bot-right: {bot:?}");
    // bottom interior
    let bot_interior: String = bot.chars().skip(1).take(8).collect();
    assert!(bot_interior.chars().all(|c| c == '▄'), "thick bot interior: {bot_interior:?}");
}

#[test]
fn render_panel_border_glyphs() {
    let (_frame, lines) = bordered_lines(BorderType::Panel);
    let top = &lines[0];
    assert_eq!(top.chars().next().unwrap(), '▊', "panel top-left: {top:?}");
    assert_eq!(top.chars().last().unwrap(), '▎', "panel top-right: {top:?}");
    // top interior is '█'
    let interior: String = top.chars().skip(1).take(8).collect();
    assert!(interior.chars().all(|c| c == '█'), "panel top interior: {interior:?}");
    let mid = &lines[1];
    assert_eq!(mid.chars().next().unwrap(), '▊', "panel mid-left: {mid:?}");
    assert_eq!(mid.chars().last().unwrap(), '▎', "panel mid-right: {mid:?}");
    let bot = &lines[3];
    assert_eq!(bot.chars().next().unwrap(), '▊', "panel bot-left: {bot:?}");
    assert_eq!(bot.chars().last().unwrap(), '▎', "panel bot-right: {bot:?}");
}

#[test]
fn render_tab_and_wide_border_glyphs() {
    for btype in [BorderType::Tab, BorderType::Wide] {
        let (_frame, lines) = bordered_lines(btype);
        let top = &lines[0];
        // top row is all '▁'
        assert!(
            top.chars().all(|c| c == '▁'),
            "{btype:?} top row should be all ▁: {top:?}"
        );
        let mid = &lines[1];
        assert_eq!(mid.chars().next().unwrap(), '▎', "{btype:?} mid-left: {mid:?}");
        assert_eq!(mid.chars().last().unwrap(), '▊', "{btype:?} mid-right: {mid:?}");
        let bot = &lines[3];
        // bottom row is all '▔'
        assert!(
            bot.chars().all(|c| c == '▔'),
            "{btype:?} bot row should be all ▔: {bot:?}"
        );
    }
}

#[test]
fn render_blank_border_consumes_space() {
    let (_frame, lines) = bordered_lines(BorderType::Blank);
    // Top border row: all spaces (blank = space chars)
    assert!(
        lines[0].chars().all(|c| c == ' '),
        "blank border top should be all spaces: {:?}",
        lines[0]
    );
    // Content starts at row 1, col 1 (border consumed 1 cell on each side)
    // Row 1, col 0 is the left blank border = space
    assert_eq!(
        lines[1].chars().next().unwrap(),
        ' ',
        "blank border left should be space: {:?}",
        lines[1]
    );
    // Row 1, col 1 is the 'x' fill content
    let ch = lines[1].chars().nth(1).unwrap();
    assert_eq!(ch, 'x', "content starts at col 1: {:?}", lines[1]);
}

#[test]
fn render_outline_uses_table_chars() {
    // Outline cells are painted into the parent's region (outside the child's box).
    // Parent 12×6, child with margin:1 and round outline on all sides.
    // Child occupies rows 1..4, cols 1..10 inside the parent.
    // Outline cells: top=row 0, bottom=row 5, left=col 0, right=col 11.
    let red = Color::parse("red").unwrap();
    let round_edge = BorderEdge::Edge {
        border_type: BorderType::Round,
        color: red,
    };
    let mut style = Style::new();
    style.outline_top = round_edge;
    style.outline_bottom = round_edge;
    style.outline_left = round_edge;
    style.outline_right = round_edge;
    style.margin = Some(Spacing::all(1));
    let fill = FillWidget::new("FillBox", style);
    let mut root = Container::new().with_child(fill);
    let (_tree, frame, _lines) = tree_render(&mut root, 12, 6);

    // top outline, middle column → chars[0][1] = '─'
    assert_eq!(
        frame.get(5, 0).text,
        "─",
        "top outline middle col should be ─"
    );
    // bottom outline, middle column → chars[2][1] = '─'
    assert_eq!(
        frame.get(5, 5).text,
        "─",
        "bottom outline middle col should be ─"
    );
    // left outline, middle row → chars[1][0] = '│'
    assert_eq!(
        frame.get(0, 2).text,
        "│",
        "left outline middle row should be │"
    );
    // right outline, middle row → chars[1][2] = '│'
    assert_eq!(
        frame.get(11, 2).text,
        "│",
        "right outline middle row should be │"
    );

    // --- Outer outline: locks new top-vs-bottom distinction ---
    let outer_edge = BorderEdge::Edge {
        border_type: BorderType::Outer,
        color: red,
    };
    let mut style2 = Style::new();
    style2.outline_top = outer_edge;
    style2.outline_bottom = outer_edge;
    style2.outline_left = outer_edge;
    style2.outline_right = outer_edge;
    style2.margin = Some(Spacing::all(1));
    let fill2 = FillWidget::new("FillBox", style2);
    let mut root2 = Container::new().with_child(fill2);
    let (_tree2, frame2, _lines2) = tree_render(&mut root2, 12, 6);

    // top outline → chars[0][1] = '▀'
    assert_eq!(frame2.get(5, 0).text, "▀", "outer top outline should be ▀");
    // bottom outline → chars[2][1] = '▄' (NEW: different from old '▀')
    assert_eq!(frame2.get(5, 5).text, "▄", "outer bottom outline should be ▄");
    // left outline → chars[1][0] = '▌'
    assert_eq!(frame2.get(0, 2).text, "▌", "outer left outline should be ▌");
    // right outline → chars[1][2] = '▐' (NEW: different from old '▌')
    assert_eq!(frame2.get(11, 2).text, "▐", "outer right outline should be ▐");
}

#[test]
fn render_panel_title_flip() {
    // Panel border: fg/bg are swapped for the title text (BORDER_TITLE_FLIP).
    // Build a widget with Panel border on all sides, title "T", left-aligned.
    let red = Color::parse("red").unwrap();
    let panel_edge = BorderEdge::Edge {
        border_type: BorderType::Panel,
        color: red,
    };
    let mut style = Style::new();
    style.border_top = panel_edge;
    style.border_right = panel_edge;
    style.border_bottom = panel_edge;
    style.border_left = panel_edge;
    style.border_title_align = Some(HorizontalAlign::Left);

    struct PanelCaptionWidget {
        title: &'static str,
        styles: WidgetStyles,
    }
    impl Widget for PanelCaptionWidget {
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
            "PanelCaptionWidget"
        }
        fn border_title(&self) -> Option<&str> {
            Some(self.title)
        }
        fn styles(&self) -> Option<&WidgetStyles> {
            Some(&self.styles)
        }
    }

    let mut styles = WidgetStyles::default();
    styles.style = style;
    let panel_widget = PanelCaptionWidget {
        title: "T",
        styles,
    };
    let mut root = Container::new().with_child(panel_widget);
    let (_tree, frame, lines) = tree_render(&mut root, 12, 4);

    // Find the "T" on row 0
    let title_col = lines[0].find('T').expect("title 'T' should appear on row 0");
    let title_cell = frame.get(title_col, 0);
    // With flip, the base style's bg (red for panel border) becomes the cell bgcolor
    let bgcolor = title_cell.style.and_then(|s| s.bgcolor);
    assert_eq!(
        bgcolor,
        Some(red.to_simple_opaque()),
        "panel title should have red bgcolor (fg/bg flipped): cell style={:?}",
        title_cell.style
    );

    // Contrast: Solid border without flip — title bgcolor is NOT red
    let solid_edge = BorderEdge::Edge {
        border_type: BorderType::Solid,
        color: red,
    };
    let mut style2 = Style::new();
    style2.border_top = solid_edge;
    style2.border_right = solid_edge;
    style2.border_bottom = solid_edge;
    style2.border_left = solid_edge;
    style2.border_title_align = Some(HorizontalAlign::Left);

    struct SolidCaptionWidget {
        title: &'static str,
        styles: WidgetStyles,
    }
    impl Widget for SolidCaptionWidget {
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
            "SolidCaptionWidget"
        }
        fn border_title(&self) -> Option<&str> {
            Some(self.title)
        }
        fn styles(&self) -> Option<&WidgetStyles> {
            Some(&self.styles)
        }
    }

    let mut styles2 = WidgetStyles::default();
    styles2.style = style2;
    let solid_widget = SolidCaptionWidget {
        title: "T",
        styles: styles2,
    };
    let mut root2 = Container::new().with_child(solid_widget);
    let (_tree2, frame2, lines2) = tree_render(&mut root2, 12, 4);
    let title_col2 = lines2[0].find('T').expect("solid title 'T' should appear on row 0");
    let solid_title_cell = frame2.get(title_col2, 0);
    let solid_bgcolor = solid_title_cell.style.and_then(|s| s.bgcolor);
    // For solid, the bgcolor should NOT be red (it's the widget background, not the border color)
    assert_ne!(
        solid_bgcolor,
        Some(red.to_simple_opaque()),
        "solid border title bgcolor should not be red (no flip): cell style={:?}",
        solid_title_cell.style
    );
}

// ===========================================================================
// Part B — CSS end-to-end tests (Step 3)
// ===========================================================================

#[test]
fn css_round_border_renders() {
    let css = "BorderBox { border: round red; }";
    let fill = FillWidget::new("BorderBox", Style::new());
    let mut root = Container::new().with_child(fill);
    let (_tree, _frame, lines) = tree_render_with_css(&mut root, 10, 4, css);

    let top = &lines[0];
    assert_eq!(
        top.chars().next().unwrap(),
        '╭',
        "CSS round border top-left should be ╭: {top:?}"
    );
    let bot = &lines[3];
    assert_eq!(
        bot.chars().last().unwrap(),
        '╯',
        "CSS round border bot-right should be ╯: {bot:?}"
    );
}

#[test]
fn css_invalid_border_renders_nothing() {
    let css = "BorderBox { border: bogus red; }";
    let fill = FillWidget::new("BorderBox", Style::new());
    let mut root = Container::new().with_child(fill);
    let (_tree, _frame, lines) = tree_render_with_css(&mut root, 10, 4, css);

    // No border: fill content starts at row 0 col 0
    assert_eq!(
        lines[0].chars().next().unwrap(),
        'x',
        "invalid border should produce no border, content at row 0 col 0: {:?}",
        lines[0]
    );
}

#[test]
fn css_hidden_border_renders_nothing_and_takes_no_space() {
    let css = "BorderBox { border: hidden; }";
    let fill = FillWidget::new("BorderBox", Style::new());
    let mut root = Container::new().with_child(fill);
    let (_tree, _frame, lines) = tree_render_with_css(&mut root, 10, 4, css);

    // hidden → BorderEdge::None → no space consumed, content at row 0 col 0
    assert_eq!(
        lines[0].chars().next().unwrap(),
        'x',
        "hidden border should take no space, content at row 0 col 0: {:?}",
        lines[0]
    );
}

#[test]
fn css_blank_border_takes_space() {
    let css = "BorderBox { border: blank; }";
    let fill = FillWidget::new("BorderBox", Style::new());
    let mut root = Container::new().with_child(fill);
    let (_tree, _frame, lines) = tree_render_with_css(&mut root, 10, 4, css);

    // blank → space-consuming but invisible; top row all spaces
    assert!(
        lines[0].chars().all(|c| c == ' '),
        "blank border top row should be all spaces: {:?}",
        lines[0]
    );
    // Content starts at row 1, col 1
    let ch = lines[1].chars().nth(1).unwrap();
    assert_eq!(
        ch, 'x',
        "blank border: content should start at row 1 col 1: {:?}",
        lines[1]
    );
}
