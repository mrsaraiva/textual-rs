//! Regression: composed children carrying a `ChildDecl::with_id` must be
//! addressable by `#id` after mount.
//!
//! Root cause ("with_compose id-drop"): several `*::with_compose` builders and
//! the `ScrollableContainer` flatten path dropped the `ChildDecl` id/class
//! metadata instead of threading it onto the mounted arena node, so a composed
//! child (e.g. `ChildDecl::from(Checkbox::new(..)).with_id("initial_focus")`)
//! became an id-less node and `query("#initial_focus")` returned no match.
//!
//! This blocked, among others, the `checkbox` demo whose mount-time
//! `query_mut("#initial_focus").focus()` silently found nothing.

use textual::compose::ChildDecl;
use textual::prelude::*;
use textual::runtime::build_widget_tree_from_root;

/// Host that composes an id'd child directly under a `VerticalScroll`
/// (`ScrollableContainer` -> `ScrollView` -> `Container` flatten path).
struct VScrollHost {
    root: AppRoot,
}

impl VScrollHost {
    fn new() -> Self {
        let vs = VerticalScroll::new().with_compose(vec![
            ChildDecl::from(Checkbox::new("Arrakis")),
            ChildDecl::from(Checkbox::new("Caladan")),
            ChildDecl::from(Checkbox::new("Kaitain")).with_id("initial_focus"),
            ChildDecl::from(Checkbox::new("Novebruns")).with_classes(&["last"]),
        ]);
        Self {
            root: AppRoot::new().with_child(vs),
        }
    }
}

impl Widget for VScrollHost {
    fn compose(&mut self) -> textual::compose::ComposeResult {
        self.root.compose()
    }

    fn render(
        &self,
        _console: &rich_rs::Console,
        _options: &rich_rs::ConsoleOptions,
    ) -> rich_rs::Segments {
        rich_rs::Segments::new()
    }
}

#[test]
fn composed_id_under_vertical_scroll_is_addressable() {
    let mut host = VScrollHost::new();
    let tree = build_widget_tree_from_root(&mut host).expect("tree should build with children");

    let by_id = tree.query("#initial_focus").expect("query should parse");
    assert_eq!(
        by_id.len(),
        1,
        "composed `#initial_focus` checkbox must resolve to exactly one node"
    );

    let by_class = tree.query(".last").expect("class query should parse");
    assert_eq!(
        by_class.len(),
        1,
        "composed `.last` class must reach the mounted node"
    );
}

/// The same must hold for a `Grid::with_compose` (layout.rs path).
struct GridHost {
    root: AppRoot,
}

impl GridHost {
    fn new() -> Self {
        let grid = Grid::new(2, 2).with_compose(vec![
            ChildDecl::from(Label::new("a")),
            ChildDecl::from(Label::new("b")).with_id("cell-b"),
        ]);
        Self {
            root: AppRoot::new().with_child(grid),
        }
    }
}

impl Widget for GridHost {
    fn compose(&mut self) -> textual::compose::ComposeResult {
        self.root.compose()
    }

    fn render(
        &self,
        _console: &rich_rs::Console,
        _options: &rich_rs::ConsoleOptions,
    ) -> rich_rs::Segments {
        rich_rs::Segments::new()
    }
}

#[test]
fn composed_id_under_grid_is_addressable() {
    let mut host = GridHost::new();
    let tree = build_widget_tree_from_root(&mut host).expect("tree should build with children");
    let by_id = tree.query("#cell-b").expect("query should parse");
    assert_eq!(
        by_id.len(),
        1,
        "composed `#cell-b` label under Grid must resolve to exactly one node"
    );
}
