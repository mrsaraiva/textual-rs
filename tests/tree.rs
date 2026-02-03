use rich_rs::Console;
use textual::prelude::*;
use textual::render::FrameBuffer;

#[test]
fn tree_renders_expanded_and_collapsed_nodes() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (24, 5);
    options.max_width = 24;
    options.max_height = 5;

    let tree = Tree::new(vec![
        TreeNode::new("Root")
            .with_child(TreeNode::new("Child A"))
            .with_child(
                TreeNode::new("Child B")
                    .expanded(false)
                    .with_child(TreeNode::new("Leaf")),
            ),
        TreeNode::new("Other"),
    ]);
    let mut tree = tree;
    tree.set_focus(true);
    tree.set_selected(1);

    let buf = FrameBuffer::from_renderable(&console, &options, &tree, None);
    insta::assert_snapshot!(buf.debug_dump());
}
