use rich_rs::Console;
use textual::prelude::*;
use textual::render::FrameBuffer;
use textual::runtime::{build_widget_tree_from_root, render_tree_to_frame};

fn render_tree(root: &mut dyn Widget, width: usize, height: usize) -> FrameBuffer {
    let console = Console::new();
    let mut tree = build_widget_tree_from_root(root).expect("tree should build");
    render_tree_to_frame(&mut tree, root, &console, width, height)
}

#[test]
fn constrained_limits_child_height() {
    let list = ListView::new(vec![
        "one".to_string(),
        "two".to_string(),
        "three".to_string(),
    ]);
    let constrained = Constrained::new(list).max_height(2);
    let mut root = Container::new().with_child(constrained);

    let buf = render_tree(&mut root, 12, 5);
    insta::assert_snapshot!(buf.debug_dump());
}
