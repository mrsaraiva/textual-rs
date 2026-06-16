//! Regression coverage for the border-box explicit-size chrome clamp
//! (`src/layout/common.rs` / `src/layout/split.rs`).
//!
//! Python's `Widget.get_box_model` computes `content = max(0, size - gutter)`
//! and the box is `content + gutter`. So a border-box widget whose explicit
//! size is smaller than its own chrome (border + padding) does NOT collapse
//! below that chrome — content goes to zero but every border row/column still
//! renders. The motivating case is the dictionary demo's `Input`
//! (`height: 1; border: tall`, chrome = 2): it must render both border rows,
//! not just the top one.

use rich_rs::{Console, ConsoleOptions, Segments};
use textual::layout::{Region, inspect_node_rects, resolve_layout};
use textual::style::{BoxSizing, Color, Dock, Scalar, Style};
use textual::widget_tree::WidgetTree;
use textual::widgets::Widget;

/// Minimal inline-styled leaf for white-box layout assertions.
struct StyledLeaf {
    inline_style: Option<Style>,
}

impl Widget for StyledLeaf {
    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        Segments::new()
    }
    fn style_type(&self) -> &'static str {
        "StyledLeaf"
    }
    fn style(&self) -> Option<Style> {
        self.inline_style.clone()
    }
}

#[test]
fn border_box_height_does_not_collapse_below_chrome() {
    let mut tree = WidgetTree::new();
    let root = tree.set_root(Box::new(StyledLeaf { inline_style: None }));

    // Docked child mirroring the dictionary `Input` (`dock: top`): an explicit
    // `height: 1` with a tall (top + bottom) border contributes 2 rows of chrome.
    let docked = tree.mount(
        root,
        Box::new(StyledLeaf {
            inline_style: Some({
                let mut s = Style::new()
                    .height(Scalar::Cells(1))
                    .border_top(Color::rgb(255, 255, 255))
                    .border_bottom(Color::rgb(255, 255, 255));
                s.dock = Some(Dock::Top);
                s.box_sizing = Some(BoxSizing::BorderBox);
                s
            }),
        }),
    );

    // Flow child (the demo's VerticalScroll body).
    let flow = tree.mount(root, Box::new(StyledLeaf { inline_style: None }));

    resolve_layout(&mut tree, root, Region::new(0, 0, 40, 20), (40, 20));

    let (docked_layout, _) = inspect_node_rects(&tree, docked).expect("docked node");
    // Box must be 2 rows tall (y1 - y0 == 2), not collapsed to 1.
    assert_eq!(
        docked_layout.3 - docked_layout.1,
        2,
        "border-box height:1 + tall border must render both border rows"
    );

    let (flow_layout, _) = inspect_node_rects(&tree, flow).expect("flow node");
    // Flow child starts after the full 2-row docked box.
    assert_eq!(
        flow_layout.1, 2,
        "flow child must start below the full input box"
    );
}
