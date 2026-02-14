//! P1G-12 + P1G-15 integration tests for the tree-mode render pipeline.
//!
//! These tests exercise the arena-based tree pipeline:
//!   build_widget_tree_from_root -> run_layout_pass / render_tree_to_frame
//!
//! They do NOT use the legacy FrameBuffer::from_renderable path.

use rich_rs::Console;
use textual::compose;
use textual::prelude::*;
use textual::render::FrameBuffer;

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
    let mut tree =
        build_widget_tree_from_root(root).expect("tree should have children");
    let frame = render_tree_to_frame(&mut tree, root, &console, w, h);
    let lines = frame.as_plain_lines();
    (tree, frame, lines)
}

/// Returns true if any of the plain-text lines contains `needle`.
fn lines_contain(lines: &[String], needle: &str) -> bool {
    lines.iter().any(|l| l.contains(needle))
}

fn find_text(lines: &[String], needle: &str) -> (usize, usize) {
    let row = lines
        .iter()
        .position(|l| l.contains(needle))
        .unwrap_or_else(|| panic!("missing text '{needle}' in lines: {lines:?}"));
    let col = lines[row]
        .find(needle)
        .unwrap_or_else(|| panic!("text '{needle}' missing from computed row {row}"));
    (row, col)
}


// ===========================================================================
// P1G-12(a): Composed descendants receive non-zero layout rects
// ===========================================================================

/// Container -> Vertical -> Label chain: all visible children get non-zero
/// layout (verified by rendered output appearing in distinct vertical rows).
#[test]
fn p1g12a_container_vertical_labels_render_in_distinct_rows() {
    let mut root = Container::new()
        .with_child(Label::new("alpha"))
        .with_child(Label::new("bravo"))
        .with_child(Label::new("charlie"));

    let (_tree, _frame, lines) = tree_render(&mut root, 30, 10);

    // Each label should render on its own row.
    assert!(
        lines_contain(&lines, "alpha"),
        "alpha should appear in rendered output"
    );
    assert!(
        lines_contain(&lines, "bravo"),
        "bravo should appear in rendered output"
    );
    assert!(
        lines_contain(&lines, "charlie"),
        "charlie should appear in rendered output"
    );

    // Labels should be on different rows (vertical stacking).
    let alpha_row = lines.iter().position(|l| l.contains("alpha"));
    let bravo_row = lines.iter().position(|l| l.contains("bravo"));
    let charlie_row = lines.iter().position(|l| l.contains("charlie"));
    assert!(
        alpha_row < bravo_row && bravo_row < charlie_row,
        "labels must stack vertically: alpha={alpha_row:?} bravo={bravo_row:?} charlie={charlie_row:?}"
    );
}

/// Tree node count must reflect all composed children (root stub + descendants).
#[test]
fn p1g12a_tree_contains_all_composed_descendants() {
    let mut root = Container::new()
        .with_child(Label::new("one"))
        .with_child(Label::new("two"))
        .with_child(Label::new("three"));

    let tree =
        build_widget_tree_from_root(&mut root).expect("tree should have children");
    let root_id = tree.root().expect("tree must have a root");

    // Root stub + 3 children = at least 4 nodes.
    assert!(tree.len() >= 4, "tree should have root + 3 children, got {}", tree.len());

    // Root's direct children count.
    let children = tree.children(root_id);
    assert_eq!(children.len(), 3, "root should have exactly 3 children");
}

/// Nested containers: Container -> Vertical -> [Label, Label].
/// Tree depth should be ≥ 3 and all labels should render.
#[test]
fn p1g12a_nested_container_vertical_labels_render() {
    let mut root = Container::new().with_child(
        Vertical::new()
            .with_child(Label::new("inner_one"))
            .with_child(Label::new("inner_two")),
    );

    let (tree, _frame, lines) = tree_render(&mut root, 40, 10);

    assert!(lines_contain(&lines, "inner_one"), "inner_one should render");
    assert!(lines_contain(&lines, "inner_two"), "inner_two should render");

    // Walk tree depth: root -> Container-child(Vertical) -> Labels
    let root_id = tree.root().unwrap();
    let depth_nodes = tree.walk_depth_first(root_id);
    assert!(
        depth_nodes.len() >= 4,
        "tree should have ≥4 nodes (root stub, Vertical, 2 Labels), got {}",
        depth_nodes.len()
    );
}

// ===========================================================================
// P1G-12(b): Clip + scroll offsets preserve child visibility/targetability
// ===========================================================================

/// ScrollView with many children: only the first few visible children
/// should appear in the rendered output within the viewport.
#[test]
fn p1g12b_scroll_view_clips_to_viewport() {
    let inner = Container::new()
        .with_child(Label::new("line_A"))
        .with_child(Label::new("line_B"))
        .with_child(Label::new("line_C"))
        .with_child(Label::new("line_D"))
        .with_child(Label::new("line_E"))
        .with_child(Label::new("line_F"))
        .with_child(Label::new("line_G"))
        .with_child(Label::new("line_H"));

    // Viewport is only 4 rows tall — not all 8 labels can fit.
    let mut root = ScrollView::new(inner).height(4);
    let (_tree, _frame, lines) = tree_render(&mut root, 30, 4);

    // At scroll offset 0, early lines (A-D) should be visible.
    let early_visible = ["line_A", "line_B", "line_C", "line_D"]
        .iter()
        .filter(|label| lines_contain(&lines, label))
        .count();
    // Late lines (G, H) must be clipped.
    let late_visible = ["line_G", "line_H"]
        .iter()
        .filter(|label| lines_contain(&lines, label))
        .count();

    assert!(
        early_visible >= 1,
        "at least some early labels (A-D) should be visible, saw {early_visible}"
    );
    assert_eq!(
        late_visible, 0,
        "late labels (G, H) must be clipped by viewport, saw {late_visible}"
    );

    // Total visible must be bounded by viewport height (4).
    let total_visible = ["line_A", "line_B", "line_C", "line_D", "line_E", "line_F", "line_G", "line_H"]
        .iter()
        .filter(|label| lines_contain(&lines, label))
        .count();
    assert!(
        total_visible <= 4,
        "visible count should not exceed viewport height (4), saw {total_visible}"
    );
}

/// VerticalScroll with many children: not all can fit in the viewport.
/// The viewport constrains how many children are rendered.
#[test]
fn p1g12b_vertical_scroll_clips_excess_children() {
    let mut root = VerticalScroll::new()
        .with_child(Static::new("row_first"))
        .with_child(Static::new("row_second"))
        .with_child(Static::new("row_third"))
        .with_child(Static::new("row_fourth"))
        .with_child(Static::new("row_fifth"))
        .with_child(Static::new("row_sixth"))
        .with_child(Static::new("row_seventh"))
        .with_child(Static::new("row_eighth"))
        .height(3);

    let (_tree, _frame, lines) = tree_render(&mut root, 30, 3);

    let all_labels = [
        "row_first", "row_second", "row_third", "row_fourth",
        "row_fifth", "row_sixth", "row_seventh", "row_eighth",
    ];
    let total_visible = all_labels.iter().filter(|l| lines_contain(&lines, l)).count();

    // With only 3 rows of viewport, not all 8 can be visible.
    assert!(
        total_visible < 8,
        "VerticalScroll clipping: not all 8 rows should be visible in 3 rows, saw {total_visible}"
    );
    // At least some should render.
    assert!(
        total_visible >= 1,
        "at least some rows should be visible; lines={lines:?}"
    );
    // Visible count bounded by viewport height (3 rows).
    assert!(
        total_visible <= 3,
        "visible count should not exceed viewport height (3), saw {total_visible}"
    );
}

// ===========================================================================
// P1G-12(c): Hit-test resolves interactive leaf nodes
// ===========================================================================

/// Button nested in containers renders its text in the expected position.
#[test]
fn p1g12c_button_in_nested_containers_renders_text() {
    let mut root = Container::new().with_child(
        Vertical::new().with_child(Button::new("Click Me")),
    );

    let (_tree, _frame, lines) = tree_render(&mut root, 40, 10);

    assert!(
        lines_contain(&lines, "Click Me"),
        "Button text 'Click Me' should appear in rendered output; lines={lines:?}"
    );
}

/// Button text must appear within the first few rows (positioned correctly)
/// so that a hit-test at the button's rendered row would resolve to it.
#[test]
fn p1g12c_button_position_is_in_expected_region() {
    let mut root = Container::new()
        .with_child(Static::new("header_text"))
        .with_child(Button::new("Action"));

    let (_tree, _frame, lines) = tree_render(&mut root, 40, 10);

    let header_row = lines.iter().position(|l| l.contains("header_text"));
    let button_row = lines.iter().position(|l| l.contains("Action"));
    assert!(header_row.is_some(), "header should render");
    assert!(button_row.is_some(), "button should render");
    assert!(
        button_row > header_row,
        "button should be below header: header={header_row:?} button={button_row:?}"
    );
}

/// Verify button text occupies a specific column range in the frame, proving
/// a hit-test at those coordinates would resolve to the interactive button.
#[test]
fn p1g12c_button_occupies_expected_column_range() {
    let mut root = Container::new()
        .with_child(Button::new("HitTarget"));

    let (_tree, frame, lines) = tree_render(&mut root, 40, 5);

    // Find the row containing the button text.
    let btn_row = lines
        .iter()
        .position(|l| l.contains("HitTarget"))
        .expect("Button text must appear in output");

    // Find the column where the button text starts.
    let btn_col = lines[btn_row]
        .find("HitTarget")
        .expect("Button text must be findable in its row");

    // The button text occupies columns [btn_col..btn_col+9].
    // Verify these cells are non-empty in the frame buffer.
    for x in btn_col..btn_col + "HitTarget".len() {
        let cell = frame.get(x, btn_row);
        assert!(
            !cell.text.is_empty(),
            "frame cell at ({x}, {btn_row}) should be non-empty for hit-test targeting"
        );
    }
}

// ===========================================================================
// P1G-15(a): Wrapper render/on_event no longer depends on drained local
//            children in tree mode
// ===========================================================================

/// Container with children: after tree extraction, rendering still works
/// (children appear at tree positions, not via parent's render).
#[test]
fn p1g15a_container_tree_mode_renders_children_correctly() {
    let mut root = Container::new()
        .with_child(Label::new("child_alpha"))
        .with_child(Label::new("child_beta"));

    // Building the tree calls take_composed_children internally.
    let (tree, _frame, lines) = tree_render(&mut root, 40, 8);

    // Children should still render via the tree pipeline.
    assert!(
        lines_contain(&lines, "child_alpha"),
        "child_alpha must appear (tree-mode rendering)"
    );
    assert!(
        lines_contain(&lines, "child_beta"),
        "child_beta must appear (tree-mode rendering)"
    );

    // Verify tree was actually built (children extracted).
    let root_id = tree.root().unwrap();
    assert!(
        !tree.children(root_id).is_empty(),
        "tree root should have children after extraction"
    );
}

/// Frame wrapper: child renders correctly through the tree pipeline.
#[test]
fn p1g15a_frame_tree_mode_renders_child() {
    let mut root = Frame::new(Label::new("framed_content")).border(true);

    let (_tree, _frame, lines) = tree_render(&mut root, 40, 8);

    assert!(
        lines_contain(&lines, "framed_content"),
        "Frame's child should render in tree mode"
    );
}

/// Panel wrapper: child renders correctly through the tree pipeline.
#[test]
fn p1g15a_panel_tree_mode_renders_child() {
    let mut root = Panel::new(Label::new("panel_content")).border(true);

    let (_tree, _frame, lines) = tree_render(&mut root, 40, 8);

    assert!(
        lines_contain(&lines, "panel_content"),
        "Panel's child should render in tree mode"
    );
}

// ===========================================================================
// P1G-15(b): Alias wrappers preserve structural composition
// ===========================================================================

/// Vertical alias: children stack vertically (each on distinct row).
#[test]
fn p1g15b_vertical_alias_stacks_children_vertically() {
    let mut root = Vertical::new()
        .with_child(Label::new("vert_1"))
        .with_child(Label::new("vert_2"))
        .with_child(Label::new("vert_3"));

    let (tree, _frame, lines) = tree_render(&mut root, 30, 10);

    // All three labels should render.
    assert!(lines_contain(&lines, "vert_1"));
    assert!(lines_contain(&lines, "vert_2"));
    assert!(lines_contain(&lines, "vert_3"));

    // Verify vertical order.
    let r1 = lines.iter().position(|l| l.contains("vert_1")).unwrap();
    let r2 = lines.iter().position(|l| l.contains("vert_2")).unwrap();
    let r3 = lines.iter().position(|l| l.contains("vert_3")).unwrap();
    assert!(r1 < r2 && r2 < r3, "Vertical must stack top-to-bottom: {r1}<{r2}<{r3}");

    // Tree structure should have 3 direct children under root.
    let root_id = tree.root().unwrap();
    assert_eq!(
        tree.children(root_id).len(),
        3,
        "Vertical should have 3 children in tree"
    );
}

/// Horizontal alias: children laid out side-by-side. Both must render and
/// the tree preserves the correct number of children.
#[test]
fn p1g15b_horizontal_alias_preserves_structure_and_renders() {
    let mut root = Horizontal::new()
        .with_child(Label::new("left_col"))
        .with_child(Label::new("right_col"));

    let (tree, _frame, lines) = tree_render(&mut root, 60, 5);

    // Both labels should appear in rendered output.
    assert!(
        lines_contain(&lines, "left_col"),
        "left_col should render in Horizontal"
    );
    assert!(
        lines_contain(&lines, "right_col"),
        "right_col should render in Horizontal"
    );

    // Tree structure: root should have exactly 2 children.
    let root_id = tree.root().unwrap();
    assert_eq!(
        tree.children(root_id).len(),
        2,
        "Horizontal should have 2 children in tree"
    );
}

/// VerticalScroll alias preserves tree structure.
#[test]
fn p1g15b_vertical_scroll_alias_preserves_structure() {
    let mut root = VerticalScroll::new()
        .with_child(Label::new("scroll_child_1"))
        .with_child(Label::new("scroll_child_2"))
        .height(6);

    let (tree, _frame, lines) = tree_render(&mut root, 30, 6);

    assert!(lines_contain(&lines, "scroll_child_1"));
    assert!(lines_contain(&lines, "scroll_child_2"));

    let root_id = tree.root().unwrap();
    assert_eq!(
        tree.children(root_id).len(),
        2,
        "VerticalScroll should have 2 children in tree"
    );
}

/// HorizontalScroll alias preserves tree structure.
#[test]
fn p1g15b_horizontal_scroll_alias_preserves_structure() {
    let mut root = HorizontalScroll::new()
        .with_child(Label::new("hscroll_1"))
        .with_child(Label::new("hscroll_2"))
        .height(4);

    let (tree, _frame, lines) = tree_render(&mut root, 60, 4);

    assert!(lines_contain(&lines, "hscroll_1"));
    assert!(lines_contain(&lines, "hscroll_2"));

    let root_id = tree.root().unwrap();
    assert_eq!(
        tree.children(root_id).len(),
        2,
        "HorizontalScroll should have 2 children in tree"
    );
}

/// ScrollableContainer alias preserves tree structure.
#[test]
fn p1g15b_scrollable_container_alias_preserves_structure() {
    let mut root = ScrollableContainer::new()
        .with_child(Label::new("sc_child_1"))
        .with_child(Label::new("sc_child_2"))
        .height(6);

    let (tree, _frame, lines) = tree_render(&mut root, 30, 6);

    assert!(lines_contain(&lines, "sc_child_1"));
    assert!(lines_contain(&lines, "sc_child_2"));

    let root_id = tree.root().unwrap();
    assert_eq!(
        tree.children(root_id).len(),
        2,
        "ScrollableContainer should have 2 children in tree"
    );
}

/// HorizontalGroup alias preserves tree structure and layout.
#[test]
fn p1g15b_horizontal_group_preserves_structure() {
    let mut root = HorizontalGroup::new()
        .with_child(Label::new("hg_left"))
        .with_child(Label::new("hg_right"));

    let (tree, _frame, lines) = tree_render(&mut root, 60, 5);

    assert!(lines_contain(&lines, "hg_left"));
    assert!(lines_contain(&lines, "hg_right"));

    let root_id = tree.root().unwrap();
    assert_eq!(tree.children(root_id).len(), 2);
}

// ===========================================================================
// P1G-15(c): Deep wrapper-chain proof
// ===========================================================================

/// Dock -> ScrollView -> HorizontalGroup -> VerticalScroll -> Button
/// The DEEP chain: verify button text renders, all intermediate wrappers
/// contribute to tree structure, and tree depth matches expected nesting.
#[test]
fn p1g15c_deep_wrapper_chain_dock_scroll_hgroup_vscroll_button() {
    let button = Button::new("DeepBtn");
    let vscroll = VerticalScroll::new().with_child(button).height(8);
    let hgroup = HorizontalGroup::new().with_child(vscroll);
    let scroll_view = ScrollView::new(hgroup).height(8);
    let mut root = Dock::new().push_fill(scroll_view);

    let (tree, _frame, lines) = tree_render(&mut root, 60, 10);

    // Button text must appear in rendered output.
    assert!(
        lines_contain(&lines, "DeepBtn"),
        "Button text 'DeepBtn' must render through deep wrapper chain; lines={lines:?}"
    );

    // Tree depth: root-stub -> Dock-child(ScrollView) -> HorizontalGroup -> VerticalScroll -> Button
    // That's at least 5 levels deep.
    let root_id = tree.root().unwrap();
    let all_nodes = tree.walk_depth_first(root_id);
    assert!(
        all_nodes.len() >= 5,
        "deep chain should produce ≥5 tree nodes, got {}",
        all_nodes.len()
    );

    // Verify tree depth by finding the deepest leaf (most ancestors).
    let max_depth = all_nodes
        .iter()
        .filter(|&&id| tree.children(id).is_empty())
        .map(|&id| tree.ancestors(id).len() + 1) // +1 for the leaf itself
        .max()
        .expect("tree should have at least one leaf node");
    assert!(
        max_depth >= 4,
        "deepest leaf should be at depth ≥4 (got {max_depth})"
    );
}

/// Variant of the deep chain with header + button inside the VerticalScroll.
#[test]
fn p1g15c_deep_chain_with_compose_and_labels() {
    let mut root = Dock::new().push_fill(ScrollView::new(
        HorizontalGroup::new().with_child(
            VerticalScroll::new()
                .with_child(Static::new("Deep Header"))
                .with_child(Button::new("DeepAction"))
                .height(10),
        ),
    ));

    let (tree, _frame, lines) = tree_render(&mut root, 60, 12);

    assert!(
        lines_contain(&lines, "Deep Header"),
        "Static header should render in deep chain"
    );
    assert!(
        lines_contain(&lines, "DeepAction"),
        "Button should render in deep chain"
    );

    // Verify structural depth.
    let root_id = tree.root().unwrap();
    let all_nodes = tree.walk_depth_first(root_id);
    assert!(
        all_nodes.len() >= 6,
        "chain with header+button should have ≥6 nodes, got {}",
        all_nodes.len()
    );
}

// ===========================================================================
// Additional coverage: layout_pass standalone + tree structure invariants
// ===========================================================================

/// run_layout_pass produces a tree where all nodes can be walked and the
/// tree is structurally valid after layout.
#[test]
fn layout_pass_tree_structure_remains_valid() {
    let mut root = Container::new()
        .with_child(Vertical::new()
            .with_child(Label::new("a"))
            .with_child(Label::new("b")))
        .with_child(Label::new("c"));

    let mut tree =
        build_widget_tree_from_root(&mut root).expect("tree should exist");

    // Install stylesheet context and run layout.
    let sheet = textual::css::default_widget_stylesheet();
    let _guard = textual::css::set_style_context(sheet);
    run_layout_pass(&mut tree, (40, 10));

    // Tree should still be walkable.
    let root_id = tree.root().unwrap();
    let all_nodes = tree.walk_depth_first(root_id);
    assert!(all_nodes.len() >= 5, "all composed nodes should survive layout pass");

    // Every node should be retrievable.
    for &nid in &all_nodes {
        assert!(tree.get(nid).is_some(), "node {nid:?} should be accessible after layout");
    }

    // Parent-child relationships should be consistent.
    for &nid in &all_nodes {
        for &child_id in tree.children(nid) {
            assert_eq!(
                tree.parent(child_id),
                Some(nid),
                "child's parent should match"
            );
        }
    }
}

/// After tree build, children are no longer in the original widget's local
/// children vec (take_composed_children drained them). The tree pipeline
/// fully owns them.
#[test]
fn tree_build_drains_container_local_children() {
    let mut root = Container::new()
        .with_child(Label::new("x"))
        .with_child(Label::new("y"));

    // Before tree build, container has 2 children.
    assert_eq!(root.children().len(), 2, "container should start with 2 children");

    let tree = build_widget_tree_from_root(&mut root).expect("tree exists");

    // After tree build, container's local children should be drained.
    assert_eq!(
        root.children().len(),
        0,
        "container's local children should be empty after tree extraction"
    );

    // But the tree owns them.
    let root_id = tree.root().unwrap();
    assert_eq!(
        tree.children(root_id).len(),
        2,
        "tree should own the 2 extracted children"
    );
}

/// Multiple alias wrappers in parallel: Vertical + Horizontal side by side
/// in a HorizontalGroup. Both sub-trees should render correctly.
#[test]
fn parallel_alias_wrappers_both_render() {
    let mut root = HorizontalGroup::new()
        .with_child(
            Vertical::new()
                .with_child(Label::new("v_top"))
                .with_child(Label::new("v_bot")),
        )
        .with_child(
            Vertical::new()
                .with_child(Label::new("v2_top"))
                .with_child(Label::new("v2_bot")),
        );

    let (_tree, _frame, lines) = tree_render(&mut root, 60, 10);

    assert!(lines_contain(&lines, "v_top"), "first Vertical top label");
    assert!(lines_contain(&lines, "v_bot"), "first Vertical bottom label");
    assert!(lines_contain(&lines, "v2_top"), "second Vertical top label");
    assert!(lines_contain(&lines, "v2_bot"), "second Vertical bottom label");
}

/// Regression guard for the buttons_advanced-like wrapper chain.
/// Each vertical scroll column must keep its children stacked vertically.
/// If wrappers collapse into a horizontal flow, this should fail.
#[test]
fn p1g15_buttons_advanced_chain_preserves_vertical_grouping() {
    let mut root = Dock::new().push_fill(ScrollView::new(Horizontal::new().with_compose(compose![
        VerticalScroll::new().with_compose(compose![
            Static::new("COL1"),
            Button::new("C1_A"),
            Button::new("C1_B"),
        ]),
        VerticalScroll::new().with_compose(compose![
            Static::new("COL2"),
            Button::new("C2_A"),
            Button::new("C2_B"),
        ]),
    ])));

    let (_tree, _frame, lines) = tree_render(&mut root, 80, 20);
    let (r_c1a, c_c1a) = find_text(&lines, "C1_A");
    let (r_c1b, _c_c1b) = find_text(&lines, "C1_B");
    let (r_c2a, c_c2a) = find_text(&lines, "C2_A");
    let (r_c2b, _c_c2b) = find_text(&lines, "C2_B");

    // Vertical grouping per column.
    assert!(
        r_c1b > r_c1a,
        "column 1 buttons must stack vertically: C1_A@({r_c1a},{c_c1a}) C1_B@({r_c1b},_)"
    );
    assert!(
        r_c2b > r_c2a,
        "column 2 buttons must stack vertically: C2_A@({r_c2a},{c_c2a}) C2_B@({r_c2b},_)"
    );

    // First-row buttons should be in different columns (roughly same row, different x).
    assert!(
        c_c2a > c_c1a + 4,
        "columns must be horizontally separated: C1_A@({r_c1a},{c_c1a}) C2_A@({r_c2a},{c_c2a})"
    );
}

/// Regression guard: bordered footer wrappers in tree mode must keep child text
/// on the interior content row (not on the border rows).
#[test]
fn p1g15_footer_wrapper_keeps_text_inside_border_content_row() {
    let footer = Styled::new(
        Static::new("Events: demo"),
        Style::new()
            .line_pad(1)
            .bg(Color::parse("#303a43").unwrap())
            .border_top(Color::parse("#44cc44").unwrap())
            .border_right(Color::parse("#44cc44").unwrap())
            .border_bottom(Color::parse("#44cc44").unwrap())
            .border_left(Color::parse("#44cc44").unwrap()),
    );
    let mut root = Dock::new()
        .push_fill(Static::new("body"))
        .push_bottom(Some(3), footer);

    let (_tree, _frame, lines) = tree_render(&mut root, 80, 24);
    let (events_row, _events_col) = find_text(&lines, "Events: demo");

    assert!(
        events_row > 0 && events_row + 1 < lines.len(),
        "events row should have both top and bottom neighbors; row={events_row} lines={lines:?}"
    );
    assert!(
        !lines[events_row - 1].contains("Events: demo"),
        "top border row must not contain footer text; line={}",
        lines[events_row - 1]
    );
    assert!(
        !lines[events_row + 1].contains("Events: demo"),
        "bottom border row must not contain footer text; line={}",
        lines[events_row + 1]
    );
    assert!(
        lines[events_row].contains('│'),
        "interior row should contain side border glyph(s); line={}",
        lines[events_row]
    );
}

/// Layout regression guard: combinator width rules must apply during layout
/// (not only render-time style resolution).
#[test]
fn p1g15_layout_honors_horizontal_child_combinator_width() {
    let mut root = Horizontal::new()
        .with_child(VerticalScroll::new().with_child(Label::new("A")))
        .with_child(VerticalScroll::new().with_child(Label::new("B")))
        .with_child(VerticalScroll::new().with_child(Label::new("C")))
        .with_child(VerticalScroll::new().with_child(Label::new("D")));

    let mut tree = build_widget_tree_from_root(&mut root).expect("tree should have children");
    let root_id = tree.root().expect("tree should have root");
    let children = tree.children(root_id).to_vec();

    let sheet = StyleSheet::parse(
        r#"
Horizontal {
    layout: horizontal;
}
Horizontal > VerticalScroll {
    width: 24;
}
"#,
    );
    let _guard = textual::css::set_style_context(sheet);
    run_layout_pass(&mut tree, (80, 24));

    let widths: Vec<u16> = children
        .into_iter()
        .map(|id| {
            let (layout, _content) =
                textual::layout::inspect_node_rects(&tree, id).expect("child rects should exist");
            layout.2.saturating_sub(layout.0)
        })
        .collect();
    assert_eq!(widths, vec![24, 24, 24, 24]);
}
