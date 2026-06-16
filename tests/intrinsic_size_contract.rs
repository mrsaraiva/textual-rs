//! Intrinsic size contract gates.
//!
//! Purpose:
//! - Lock the engine contract for `width/height: auto` sizing.
//! - Catch widget-level regressions where intrinsic hints omit required chrome.

use rich_rs::{Console, ConsoleOptions, Segments};
use textual::layout::{Region, inspect_node_rects, resolve_layout};
use textual::prelude::*;
use textual::style::{BoxSizing, Scalar, Spacing, Style};
use textual::widget_tree::WidgetTree;
use textual::widgets::Widget;

struct IntrinsicWidget {
    style: Style,
    intrinsic_w: usize,
    intrinsic_h: usize,
}

impl IntrinsicWidget {
    fn boxed(style: Style, intrinsic_w: usize, intrinsic_h: usize) -> Box<dyn Widget> {
        Box::new(Self {
            style,
            intrinsic_w,
            intrinsic_h,
        })
    }
}

impl Widget for IntrinsicWidget {
    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        Segments::new()
    }

    fn style_type(&self) -> &'static str {
        "IntrinsicWidget"
    }

    fn style(&self) -> Option<Style> {
        Some(self.style.clone())
    }

    fn set_inline_style(&mut self, style: Style) {
        self.style = style;
    }

    fn content_width(&self) -> Option<usize> {
        Some(self.intrinsic_w)
    }

    fn layout_height(&self) -> Option<usize> {
        Some(self.intrinsic_h)
    }
}

fn layout_rect_wh(tree: &WidgetTree, node: textual::node_id::NodeId) -> (u16, u16) {
    let ((x0, y0, x1, y1), _) = inspect_node_rects(tree, node).expect("node rect should exist");
    (x1.saturating_sub(x0), y1.saturating_sub(y0))
}

fn measure_child_width(child: Box<dyn Widget>) -> u16 {
    let mut tree = WidgetTree::new();
    let root = tree.set_root(Box::new(Container::new()));
    let child_id = tree.mount(root, child);
    resolve_layout(&mut tree, root, Region::new(0, 0, 120, 24), (120, 24));
    let (w, _h) = layout_rect_wh(&tree, child_id);
    w
}

/// Variant that applies inline styles via `tree.update_styles()` after mounting,
/// for widgets that no longer expose `styles_mut()` (migrated to `NodeSeed`).
fn measure_child_width_with_tree_style(child: Box<dyn Widget>, horizontal: u16) -> u16 {
    let mut tree = WidgetTree::new();
    let root = tree.set_root(Box::new(Container::new()));
    let child_id = tree.mount(root, child);
    tree.update_styles(child_id, |styles| {
        styles.style = Style::new()
            .width(Scalar::Auto)
            .height(Scalar::Auto)
            .border(false)
            .padding(Spacing::new(0, horizontal, 0, horizontal));
    });
    resolve_layout(&mut tree, root, Region::new(0, 0, 120, 24), (120, 24));
    let (w, _h) = layout_rect_wh(&tree, child_id);
    w
}

fn set_inline_border_box_padding(widget: &mut dyn Widget, horizontal: u16) {
    widget.set_inline_style(
        Style::new()
            .width(Scalar::Auto)
            .height(Scalar::Auto)
            .border(false)
            .padding(Spacing::new(0, horizontal, 0, horizontal)),
    );
}

/// Regression: `Label::layout_height()` must report OUTER height (content +
/// own padding/border), per the `extract_child_spec` height-arm convention.
/// A `Label { padding: 1 2 }` previously reported only its 1-row content height
/// and overflowed its box (two stacked padded labels overlapped). The height
/// must now include the resolved vertical chrome.
#[test]
fn label_layout_height_includes_vertical_padding() {
    let mut compact = Label::new("Item");
    compact.set_inline_style(Style::new().padding(Spacing::new(0, 2, 0, 2)));

    let mut padded = Label::new("Item");
    padded.set_inline_style(Style::new().padding(Spacing::new(1, 2, 1, 2)));

    let compact_h = compact
        .layout_height()
        .expect("Label reports a layout height");
    let padded_h = padded
        .layout_height()
        .expect("Label reports a layout height");

    assert_eq!(
        padded_h.saturating_sub(compact_h),
        2,
        "Label layout_height must include top+bottom padding (outer height)"
    );
}

#[test]
fn engine_border_box_auto_adds_padding_chrome() {
    let mut tree = WidgetTree::new();
    let root = tree.set_root(Box::new(Container::new()));
    let child = tree.mount(
        root,
        IntrinsicWidget::boxed(
            Style::new()
                .width(Scalar::Auto)
                .height(Scalar::Auto)
                .padding(Spacing::new(1, 2, 1, 2)),
            10,
            3,
        ),
    );

    resolve_layout(&mut tree, root, Region::new(0, 0, 80, 20), (80, 20));
    let (w, h) = layout_rect_wh(&tree, child);

    // Post-RA-2 contract: `content_width()` is PURE content (widgets no longer
    // fold their own chrome into the intrinsic hint — the layout owns chrome),
    // so `width: auto` adds full horizontal chrome regardless of box-sizing
    // (box-sizing only governs how an EXPLICIT width is interpreted).
    // Padding left+right = 4, so outer width = 10 + 4 = 14.
    assert_eq!(w, 14);
    // Known asymmetry (tracked follow-up): the height path still adds only
    // margin to the intrinsic hint, because several real widgets report
    // `layout_height()` that already includes border/padding (e.g. bordered
    // grid cells in five_by_five). So vertical padding is not added here.
    assert_eq!(h, 3);
}

#[test]
fn engine_content_box_auto_adds_padding_chrome() {
    let mut tree = WidgetTree::new();
    let root = tree.set_root(Box::new(Container::new()));
    let child = tree.mount(
        root,
        IntrinsicWidget::boxed(
            {
                let mut s = Style::new()
                    .width(Scalar::Auto)
                    .height(Scalar::Auto)
                    .padding(Spacing::new(1, 2, 1, 2));
                s.box_sizing = Some(BoxSizing::ContentBox);
                s
            },
            10,
            3,
        ),
    );

    resolve_layout(&mut tree, root, Region::new(0, 0, 80, 20), (80, 20));
    let (w, _h) = layout_rect_wh(&tree, child);

    // Content-box contract: layout adds padding chrome.
    assert_eq!(w, 14);
}

#[test]
fn tab_auto_width_keeps_horizontal_padding_gap() {
    let mut root = Tabs::new().with_tab("A").with_tab("B");
    let console = Console::new();
    let mut tree = build_widget_tree_from_root(&mut root).expect("tree should exist");
    let frame = render_tree_to_frame(&mut tree, &mut root, &console, 20, 4);
    let lines = frame.as_plain_lines();
    let header = lines.first().expect("tab header row");

    assert!(
        header.contains(" A "),
        "tab label should keep horizontal padding; header={header:?}"
    );
    assert!(
        header.contains(" B "),
        "tab label should keep horizontal padding; header={header:?}"
    );
}

#[test]
fn markdown_line_does_not_wrap_when_viewport_has_room() {
    let sentence = "Bene Gesserit and concubine of Leto, and mother of Paul and Alia.";
    let mut root =
        Container::new().with_child(Markdown::new(format!("# Lady Jessica\n\n{sentence}")));
    let console = Console::new();
    let mut tree = build_widget_tree_from_root(&mut root).expect("tree should exist");
    let frame = render_tree_to_frame(&mut tree, &mut root, &console, 80, 10);
    let lines = frame.as_plain_lines();

    assert!(
        lines.iter().any(|line| line.contains(sentence)),
        "expected full sentence on one rendered line; lines={lines:?}"
    );
}

#[test]
fn button_layout_height_matches_tree_layout_height() {
    let button = Button::new("Click");
    let expected_h = button
        .layout_height()
        .expect("button should report intrinsic layout height");

    let mut root = Container::new().with_child(button);
    let mut tree = build_widget_tree_from_root(&mut root).expect("tree should exist");
    run_layout_pass(&mut tree, (80, 24));
    let button_id = *tree
        .query("Button")
        .expect("selector should parse")
        .first()
        .expect("button node should exist");
    let (_w, h) = layout_rect_wh(&tree, button_id);

    assert_eq!(
        h as usize, expected_h,
        "button tree layout height must match widget intrinsic height hint"
    );
}

#[test]
fn link_width_tracks_padding_delta() {
    let mut compact = Box::new(Link::new("ParityLink"));
    set_inline_border_box_padding(compact.as_mut(), 0);
    let compact_w = measure_child_width(compact);

    let mut padded = Box::new(Link::new("ParityLink"));
    set_inline_border_box_padding(padded.as_mut(), 2);
    let padded_w = measure_child_width(padded);

    assert_eq!(
        padded_w.saturating_sub(compact_w),
        4,
        "Link auto width must grow by left+right padding delta"
    );
}

#[test]
fn checkbox_width_tracks_padding_delta() {
    let mut compact = Box::new(Checkbox::new("ParityCheck"));
    set_inline_border_box_padding(compact.as_mut(), 0);
    let compact_w = measure_child_width(compact);

    let mut padded = Box::new(Checkbox::new("ParityCheck"));
    set_inline_border_box_padding(padded.as_mut(), 2);
    let padded_w = measure_child_width(padded);

    assert_eq!(
        padded_w.saturating_sub(compact_w),
        4,
        "Checkbox auto width must grow by left+right padding delta"
    );
}

#[test]
fn radio_button_width_tracks_padding_delta() {
    let mut compact = Box::new(RadioButton::new("ParityRadio"));
    set_inline_border_box_padding(compact.as_mut(), 0);
    let compact_w = measure_child_width(compact);

    let mut padded = Box::new(RadioButton::new("ParityRadio"));
    set_inline_border_box_padding(padded.as_mut(), 2);
    let padded_w = measure_child_width(padded);

    assert_eq!(
        padded_w.saturating_sub(compact_w),
        4,
        "RadioButton auto width must grow by left+right padding delta"
    );
}

#[test]
fn switch_width_tracks_padding_delta() {
    let mut compact = Box::new(Switch::new(false));
    set_inline_border_box_padding(compact.as_mut(), 0);
    let compact_w = measure_child_width(compact);

    let mut padded = Box::new(Switch::new(false));
    set_inline_border_box_padding(padded.as_mut(), 2);
    let padded_w = measure_child_width(padded);

    assert_eq!(
        padded_w.saturating_sub(compact_w),
        4,
        "Switch auto width must grow by left+right padding delta"
    );
}

#[test]
fn select_width_tracks_padding_delta() {
    let mut compact = Box::new(Select::new(
        vec![("Alpha".to_string(), 1), ("Beta".to_string(), 2)],
        "Pick",
    ));
    set_inline_border_box_padding(compact.as_mut(), 0);
    let compact_w = measure_child_width(compact);

    let mut padded = Box::new(Select::new(
        vec![("Alpha".to_string(), 1), ("Beta".to_string(), 2)],
        "Pick",
    ));
    set_inline_border_box_padding(padded.as_mut(), 2);
    let padded_w = measure_child_width(padded);

    assert_eq!(
        padded_w.saturating_sub(compact_w),
        4,
        "Select auto width must grow by left+right padding delta"
    );
}

#[test]
fn option_list_width_tracks_padding_delta() {
    let mut compact = Box::new(OptionList::with_items(vec![
        OptionItem::new("Alpha"),
        OptionItem::new("Beta"),
    ]));
    set_inline_border_box_padding(compact.as_mut(), 0);
    let compact_w = measure_child_width(compact);

    let mut padded = Box::new(OptionList::with_items(vec![
        OptionItem::new("Alpha"),
        OptionItem::new("Beta"),
    ]));
    set_inline_border_box_padding(padded.as_mut(), 2);
    let padded_w = measure_child_width(padded);

    assert_eq!(
        padded_w.saturating_sub(compact_w),
        4,
        "OptionList auto width must grow by left+right padding delta"
    );
}

#[test]
fn tooltip_width_tracks_padding_delta() {
    let mut compact = Box::new(Tooltip::new(Label::new("Anchor"), "Tip"));
    set_inline_border_box_padding(compact.as_mut(), 0);
    let compact_w = measure_child_width(compact);

    let mut padded = Box::new(Tooltip::new(Label::new("Anchor"), "Tip"));
    set_inline_border_box_padding(padded.as_mut(), 2);
    let padded_w = measure_child_width(padded);

    assert_eq!(
        padded_w.saturating_sub(compact_w),
        4,
        "Tooltip auto width must grow by left+right padding delta"
    );
}

#[test]
fn toast_width_tracks_padding_delta() {
    let mut compact =
        Box::new(Toast::new("Longer toast content", ToastSeverity::Information).with_title("Info"));
    set_inline_border_box_padding(compact.as_mut(), 0);
    let compact_w = measure_child_width(compact);

    let mut padded =
        Box::new(Toast::new("Longer toast content", ToastSeverity::Information).with_title("Info"));
    set_inline_border_box_padding(padded.as_mut(), 2);
    let padded_w = measure_child_width(padded);

    assert_eq!(
        padded_w.saturating_sub(compact_w),
        4,
        "Toast auto width must grow by left+right padding delta"
    );
}

#[test]
fn rule_vertical_width_tracks_padding_delta() {
    let mut compact = Box::new(Rule::vertical());
    set_inline_border_box_padding(compact.as_mut(), 0);
    let compact_w = measure_child_width(compact);

    let mut padded = Box::new(Rule::vertical());
    set_inline_border_box_padding(padded.as_mut(), 2);
    let padded_w = measure_child_width(padded);

    assert_eq!(
        padded_w.saturating_sub(compact_w),
        4,
        "Vertical Rule auto width must grow by left+right padding delta"
    );
}

#[test]
fn radio_set_width_tracks_padding_delta() {
    let mut compact = Box::new(RadioSet::from_labels(&["Alpha", "Beta"]));
    set_inline_border_box_padding(compact.as_mut(), 0);
    let compact_w = measure_child_width(compact);

    let mut padded = Box::new(RadioSet::from_labels(&["Alpha", "Beta"]));
    set_inline_border_box_padding(padded.as_mut(), 2);
    let padded_w = measure_child_width(padded);

    assert_eq!(
        padded_w.saturating_sub(compact_w),
        4,
        "RadioSet auto width must grow by left+right padding delta"
    );
}

#[test]
fn selection_list_width_tracks_padding_delta() {
    let selections = vec![
        Selection::new("Alpha", "alpha".to_string()),
        Selection::new("Beta", "beta".to_string()),
    ];
    let mut compact = Box::new(SelectionList::with_selections(selections.clone()));
    set_inline_border_box_padding(compact.as_mut(), 0);
    let compact_w = measure_child_width(compact);

    let mut padded = Box::new(SelectionList::with_selections(selections));
    set_inline_border_box_padding(padded.as_mut(), 2);
    let padded_w = measure_child_width(padded);

    assert_eq!(
        padded_w.saturating_sub(compact_w),
        4,
        "SelectionList auto width must grow by left+right padding delta"
    );
}

#[test]
fn list_view_width_tracks_padding_delta() {
    let mut compact = Box::new(ListView::new(vec!["Alpha".to_string(), "Beta".to_string()]));
    set_inline_border_box_padding(compact.as_mut(), 0);
    let compact_w = measure_child_width(compact);

    let mut padded = Box::new(ListView::new(vec!["Alpha".to_string(), "Beta".to_string()]));
    set_inline_border_box_padding(padded.as_mut(), 2);
    let padded_w = measure_child_width(padded);

    assert_eq!(
        padded_w.saturating_sub(compact_w),
        4,
        "ListView auto width must grow by left+right padding delta"
    );
}

#[test]
fn collapsible_width_tracks_padding_delta() {
    let compact_w = measure_child_width_with_tree_style(Box::new(Collapsible::new("Section")), 0);
    let padded_w = measure_child_width_with_tree_style(Box::new(Collapsible::new("Section")), 2);

    assert_eq!(
        padded_w.saturating_sub(compact_w),
        4,
        "Collapsible auto width must grow by left+right padding delta"
    );
}

#[test]
fn content_switcher_width_tracks_padding_delta() {
    let mut compact_switcher = ContentSwitcher::new();
    compact_switcher.add_content(
        Label::new("Switcher child").with_shrink(true),
        Some("pane-a"),
        true,
    );
    let compact_w = measure_child_width_with_tree_style(Box::new(compact_switcher), 0);

    let mut padded_switcher = ContentSwitcher::new();
    padded_switcher.add_content(
        Label::new("Switcher child").with_shrink(true),
        Some("pane-a"),
        true,
    );
    let padded_w = measure_child_width_with_tree_style(Box::new(padded_switcher), 2);

    assert_eq!(
        padded_w.saturating_sub(compact_w),
        4,
        "ContentSwitcher auto width must grow by left+right padding delta"
    );
}

#[test]
fn tree_width_tracks_padding_delta() {
    let nodes = vec![
        TreeNode::new("Root")
            .allow_expand(true)
            .expanded(true)
            .with_child(TreeNode::new("Child")),
    ];
    let mut compact = Box::new(Tree::new(nodes.clone()));
    set_inline_border_box_padding(compact.as_mut(), 0);
    let compact_w = measure_child_width(compact);

    let mut padded = Box::new(Tree::new(nodes));
    set_inline_border_box_padding(padded.as_mut(), 2);
    let padded_w = measure_child_width(padded);

    assert_eq!(
        padded_w.saturating_sub(compact_w),
        4,
        "Tree auto width must grow by left+right padding delta"
    );
}

#[test]
fn directory_tree_width_tracks_padding_delta() {
    let temp_dir = std::env::temp_dir().join(format!(
        "textual-rs-intrinsic-size-contract-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should be creatable");

    let mut compact = Box::new(DirectoryTree::new(&temp_dir));
    set_inline_border_box_padding(compact.as_mut(), 0);
    let compact_w = measure_child_width(compact);

    let mut padded = Box::new(DirectoryTree::new(&temp_dir));
    set_inline_border_box_padding(padded.as_mut(), 2);
    let padded_w = measure_child_width(padded);

    assert_eq!(
        padded_w.saturating_sub(compact_w),
        4,
        "DirectoryTree auto width must grow by left+right padding delta"
    );
}

#[test]
fn log_width_tracks_padding_delta() {
    let mut compact_log = Log::new();
    compact_log.write_line("Alpha line");
    compact_log.write_line("Second line");
    let mut compact = Box::new(compact_log);
    set_inline_border_box_padding(compact.as_mut(), 0);
    let compact_w = measure_child_width(compact);

    let mut padded_log = Log::new();
    padded_log.write_line("Alpha line");
    padded_log.write_line("Second line");
    let mut padded = Box::new(padded_log);
    set_inline_border_box_padding(padded.as_mut(), 2);
    let padded_w = measure_child_width(padded);

    assert_eq!(
        padded_w.saturating_sub(compact_w),
        4,
        "Log auto width must grow by left+right padding delta"
    );
}

#[test]
fn data_table_width_tracks_padding_delta() {
    let headers = vec!["Name".to_string(), "Role".to_string()];
    let rows = vec![vec!["Paul".to_string(), "Duke".to_string()]];

    let mut compact = Box::new(DataTable::new(headers.clone(), rows.clone()));
    set_inline_border_box_padding(compact.as_mut(), 0);
    let compact_w = measure_child_width(compact);

    let mut padded = Box::new(DataTable::new(headers, rows));
    set_inline_border_box_padding(padded.as_mut(), 2);
    let padded_w = measure_child_width(padded);

    assert_eq!(
        padded_w.saturating_sub(compact_w),
        4,
        "DataTable auto width must grow by left+right padding delta"
    );
}

#[test]
fn panel_width_tracks_padding_delta() {
    let mut compact =
        Box::new(Panel::new(Label::new("Panel child").with_shrink(true)).border(false));
    set_inline_border_box_padding(compact.as_mut(), 0);
    let compact_w = measure_child_width(compact);

    let mut padded =
        Box::new(Panel::new(Label::new("Panel child").with_shrink(true)).border(false));
    set_inline_border_box_padding(padded.as_mut(), 2);
    let padded_w = measure_child_width(padded);

    assert_eq!(
        padded_w.saturating_sub(compact_w),
        4,
        "Panel auto width must grow by left+right padding delta"
    );
}
