use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use rich_rs::Console;
use slotmap::SlotMap;
use textual::event::MouseDownEvent;
use textual::prelude::*;
use textual::reactive::ReactiveCtx;
use textual::render::FrameBuffer;
use textual::runtime::dispatch_ctx::set_dispatch_recipient;

fn make_node_id() -> NodeId {
    let mut sm: SlotMap<NodeId, ()> = SlotMap::new();
    sm.insert(())
}

fn focused_state() -> NodeState {
    NodeState { focused: true, ..Default::default() }
}

#[test]
fn tree_renders_expanded_and_collapsed_nodes() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (24, 5);
    options.max_width = 24;
    options.max_height = 5;

    let tree = Tree::new(vec![
        TreeNode::new("Root")
            .expanded(true)
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
    let mut rctx = ReactiveCtx::new(NodeId::default());
    tree.set_selected(1, &mut rctx);

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
    let _guard = set_dispatch_recipient(make_node_id(), focused_state());
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
    let id = NodeId::default();
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
    assert!(width >= rich_rs::cell_len("👩‍🚀 Launch"));
}

#[test]
fn tree_mouse_scroll_clamps_to_bounds() {
    let mut tree = Tree::new(
        (0..10)
            .map(|idx| TreeNode::new(format!("Node {idx}")))
            .collect(),
    );
    tree.on_layout(24, 3);

    let mut ctx = EventCtx::default();
    tree.on_mouse_scroll(0, 100, &mut ctx);
    assert!(ctx.handled());

    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (24, 3);
    options.max_width = 24;
    options.max_height = 3;
    let after_down = FrameBuffer::from_renderable(&console, &options, &tree, None);
    assert!(after_down.as_plain_lines()[0].contains("Node 7"));

    let mut ctx = EventCtx::default();
    tree.on_mouse_scroll(0, -100, &mut ctx);
    assert!(ctx.handled());

    let after_up = FrameBuffer::from_renderable(&console, &options, &tree, None);
    assert!(after_up.as_plain_lines()[0].contains("Node 0"));
}

#[test]
fn tree_navigation_skips_disabled_nodes() {
    let mut tree = Tree::new(vec![
        TreeNode::new("Root")
            .expanded(true)
            .with_child(TreeNode::new("Disabled Child").disabled(true))
            .with_child(TreeNode::new("Enabled Child")),
    ]);
    let _guard = set_dispatch_recipient(make_node_id(), focused_state());
    tree.on_layout(24, 5);
    let mut rctx = ReactiveCtx::new(NodeId::default());
    tree.set_selected(0, &mut rctx);

    let key = KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    let mut ctx = EventCtx::default();
    tree.on_event(&Event::Key(key), &mut ctx);
    assert!(ctx.handled());
    assert_eq!(tree.selected(), 2);
}

#[test]
fn tree_mouse_click_ignores_disabled_nodes() {
    let mut tree = Tree::new(vec![
        TreeNode::new("Root")
            .expanded(true)
            .with_child(TreeNode::new("Child").disabled(true)),
    ]);
    tree.on_layout(24, 5);

    let id = NodeId::default();
    let mut ctx = EventCtx::default();
    tree.on_event(
        &Event::MouseDown(MouseDownEvent {
            target: id,
            screen_x: 0,
            screen_y: 1,
            x: 0,
            y: 1,
        }),
        &mut ctx,
    );
    assert!(!ctx.handled());
}

#[test]
fn tree_allows_expansion_without_preloaded_children() {
    let mut tree = Tree::new(vec![
        TreeNode::new("Lazy Root")
            .expanded(false)
            .allow_expand(true),
    ]);
    let _guard = set_dispatch_recipient(make_node_id(), focused_state());
    tree.on_layout(24, 5);

    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (24, 5);
    options.max_width = 24;
    options.max_height = 5;

    let before = FrameBuffer::from_renderable(&console, &options, &tree, None);
    let before_lines = before.as_plain_lines();
    assert!(before_lines.iter().any(|line| line.contains("▶ Lazy Root")));

    let key = KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    let mut ctx = EventCtx::default();
    tree.on_event(&Event::Key(key), &mut ctx);
    assert!(ctx.handled());

    let after = FrameBuffer::from_renderable(&console, &options, &tree, None);
    let after_lines = after.as_plain_lines();
    assert!(after_lines.iter().any(|line| line.contains("▼ Lazy Root")));
}

#[test]
fn tree_all_disabled_nodes_do_not_render_highlight_marker() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (24, 3);
    options.max_width = 24;
    options.max_height = 3;

    let tree = Tree::new(vec![
        TreeNode::new("Root").disabled(true),
        TreeNode::new("Other").disabled(true),
    ]);
    let buf = FrameBuffer::from_renderable(&console, &options, &tree, None);
    for line in buf.as_plain_lines() {
        assert!(!line.contains("›"));
    }
}

#[test]
fn tree_enter_posts_activation_message_without_toggling() {
    let mut tree = Tree::new(vec![
        TreeNode::new("Root")
            .expanded(false)
            .with_child(TreeNode::new("Child")),
    ]);
    let _guard = set_dispatch_recipient(make_node_id(), focused_state());
    tree.on_layout(24, 4);

    let key = KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    let mut ctx = EventCtx::default();
    tree.on_event(&Event::Key(key), &mut ctx);
    assert!(ctx.handled());

    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (24, 4);
    options.max_width = 24;
    options.max_height = 4;
    let buf = FrameBuffer::from_renderable(&console, &options, &tree, None);
    let lines = buf.as_plain_lines();
    assert!(!lines.iter().any(|line| line.contains("Child")));
}

#[test]
fn tree_twisty_click_toggles_without_activation_message() {
    let mut tree = Tree::new(vec![
        TreeNode::new("Root")
            .expanded(true)
            .with_child(TreeNode::new("Child")),
    ]);
    tree.on_layout(24, 4);
    let id = NodeId::default();

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
    options.size = (24, 4);
    options.max_width = 24;
    options.max_height = 4;
    let buf = FrameBuffer::from_renderable(&console, &options, &tree, None);
    let lines = buf.as_plain_lines();
    assert!(!lines.iter().any(|line| line.contains("Child")));
}
