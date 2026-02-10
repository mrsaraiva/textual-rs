use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use rich_rs::Console;
use textual::event::MouseDownEvent;
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

#[test]
fn tree_right_key_expands_selected_node() {
    let mut tree = Tree::new(vec![
        TreeNode::new("Root")
            .expanded(false)
            .with_child(TreeNode::new("Child")),
    ]);
    tree.set_focus(true);
    tree.on_layout(24, 5);
    let key = KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    let mut ctx = EventCtx::default();
    tree.on_event(&Event::Key(key), &mut ctx);
    assert!(ctx.handled());

    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (24, 5);
    options.max_width = 24;
    options.max_height = 5;
    let buf = FrameBuffer::from_renderable(&console, &options, &tree, None);
    let lines = buf.as_plain_lines();
    assert!(lines.iter().any(|line| line.contains("Child")));
}

#[test]
fn tree_click_on_branch_toggles() {
    let mut tree = Tree::new(vec![
        TreeNode::new("Root")
            .expanded(true)
            .with_child(TreeNode::new("Child")),
    ]);
    tree.on_layout(24, 5);
    let id = tree.id();
    let mut ctx = EventCtx::default();
    tree.on_event(
        &Event::MouseDown(MouseDownEvent {
            target: id,
            screen_x: 0,
            screen_y: 0,
            x: 0,
            y: 0,
        }),
        &mut ctx,
    );
    assert!(ctx.handled());

    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (24, 5);
    options.max_width = 24;
    options.max_height = 5;
    let buf = FrameBuffer::from_renderable(&console, &options, &tree, None);
    let lines = buf.as_plain_lines();
    assert!(!lines.iter().any(|line| line.contains("Child")));
}

#[test]
fn tree_content_width_accounts_for_wide_labels() {
    let tree = Tree::new(vec![TreeNode::new("👩‍🚀 Launch")]);
    let width = tree.content_width().expect("tree intrinsic width");
    assert!(width >= rich_rs::cell_len("👩‍🚀 Launch") + 4);
}
