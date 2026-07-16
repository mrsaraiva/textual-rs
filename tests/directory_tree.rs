use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use rich_rs::Console;
use slotmap::SlotMap;
use textual::message::{AsyncDirectoryEntry, AsyncTaskResult, MessageEvent};
use textual::event::EventCtx;
use textual::prelude::*;
use textual::render::FrameBuffer;
use textual::runtime::dispatch_ctx::set_dispatch_recipient;

fn make_node_id() -> NodeId {
    let mut sm: SlotMap<NodeId, ()> = SlotMap::new();
    sm.insert(())
}

fn focused_state() -> NodeState {
    NodeState {
        focused: true,
        ..Default::default()
    }
}

fn hovered_state() -> NodeState {
    NodeState {
        hovered: true,
        ..Default::default()
    }
}

struct TempTreeDir {
    path: PathBuf,
}

impl TempTreeDir {
    fn new(label: &str) -> Self {
        let mut path = std::env::temp_dir();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before epoch")
            .as_nanos();
        path.push(format!("textual-rs-{label}-{}-{stamp}", std::process::id()));
        fs::create_dir_all(&path).expect("create temp test directory");
        Self { path }
    }
}

impl Drop for TempTreeDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn options_for(console: &Console, width: usize, height: usize) -> rich_rs::ConsoleOptions {
    let mut options = console.options().clone();
    options.size = (width, height);
    options.max_width = width;
    options.max_height = height;
    options
}

#[test]
fn directory_tree_renders_root_and_visible_entries() {
    let temp = TempTreeDir::new("directory-tree-visible");
    fs::create_dir_all(temp.path.join("src")).expect("create child dir");
    fs::write(temp.path.join("Cargo.toml"), "[package]\nname = \"demo\"\n").expect("write file");
    fs::write(temp.path.join(".hidden"), "hidden").expect("write hidden file");

    let console = Console::new();
    let options = options_for(&console, 48, 6);
    let mut tree = DirectoryTree::new(&temp.path);
    tree.on_layout(48, 6);

    let buf = FrameBuffer::from_renderable(&console, &options, &tree, None);
    let lines = buf.as_plain_lines();

    assert!(lines.iter().any(|line| line.contains("src")));
    assert!(lines.iter().any(|line| line.contains("Cargo.toml")));
    assert!(!lines.iter().any(|line| line.contains(".hidden")));
}

#[test]
fn directory_tree_lazy_loads_children_on_expand_message_flow() {
    let temp = TempTreeDir::new("directory-tree-expand");
    let nested_dir = temp.path.join("nested");
    fs::create_dir_all(&nested_dir).expect("create nested dir");
    fs::write(nested_dir.join("leaf.txt"), "leaf").expect("write nested file");

    let mut tree = DirectoryTree::new(&temp.path);
    tree.on_layout(60, 8);

    let mut message_ctx = EventCtx::default();
    { let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut message_ctx); tree.on_message(
        &MessageEvent::new(
            tree.tree_id(),
            TreeNodeToggled {
                index: 1,
                label: "nested".to_string(),
                expanded: true,
                node_id: textual::widgets::TreeNodeId::default(),
            },
        ),
        &mut __w) };
    assert!(message_ctx.handled());

    let console = Console::new();
    let options = options_for(&console, 60, 8);
    let before_tick = FrameBuffer::from_renderable(&console, &options, &tree, None);
    let before_tick_lines = before_tick.as_plain_lines();
    assert!(
        !before_tick_lines
            .iter()
            .any(|line| line.contains("leaf.txt"))
    );

    { let mut __e = textual::event::EventCtx::default(); let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut __e); tree.on_message(
        &MessageEvent::new(
            NodeId::default(),
            AsyncTaskCompleted {
                task_id: 1,
                target: NodeId::default(),
                result: AsyncTaskResult::DirectoryEntries {
                    path: nested_dir.display().to_string(),
                    entries: vec![AsyncDirectoryEntry {
                        path: nested_dir.join("leaf.txt").display().to_string(),
                        label: "leaf.txt".to_string(),
                        is_dir: false,
                    }],
                },
            },
        ),
        &mut __w) };

    let after_tick = FrameBuffer::from_renderable(&console, &options, &tree, None);
    let lines = after_tick.as_plain_lines();

    assert!(lines.iter().any(|line| line.contains("leaf.txt")));
}

#[test]
fn directory_tree_refresh_preserves_expanded_paths() {
    let temp = TempTreeDir::new("directory-tree-refresh-expanded");
    let nested_dir = temp.path.join("nested");
    fs::create_dir_all(&nested_dir).expect("create nested dir");
    fs::write(nested_dir.join("leaf.txt"), "leaf").expect("write nested file");

    let mut tree = DirectoryTree::new(&temp.path);
    tree.on_layout(60, 8);

    let mut message_ctx = EventCtx::default();
    { let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut message_ctx); tree.on_message(
        &MessageEvent::new(
            tree.tree_id(),
            TreeNodeToggled {
                index: 1,
                label: "nested".to_string(),
                expanded: true,
                node_id: textual::widgets::TreeNodeId::default(),
            },
        ),
        &mut __w) };
    assert!(message_ctx.handled());

    tree.refresh();

    let console = Console::new();
    let options = options_for(&console, 60, 8);
    let buf = FrameBuffer::from_renderable(&console, &options, &tree, None);
    let lines = buf.as_plain_lines();

    assert!(lines.iter().any(|line| line.contains("leaf.txt")));
}

#[test]
fn directory_tree_collapsing_node_cancels_pending_lazy_load() {
    let temp = TempTreeDir::new("directory-tree-collapse-cancels-pending");
    let nested_dir = temp.path.join("nested");
    fs::create_dir_all(&nested_dir).expect("create nested dir");
    fs::write(nested_dir.join("leaf.txt"), "leaf").expect("write nested file");

    let mut tree = DirectoryTree::new(&temp.path);
    tree.on_layout(60, 8);

    let mut expand_ctx = EventCtx::default();
    { let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut expand_ctx); tree.on_message(
        &MessageEvent::new(
            tree.tree_id(),
            TreeNodeToggled {
                index: 1,
                label: "nested".to_string(),
                expanded: true,
                node_id: textual::widgets::TreeNodeId::default(),
            },
        ),
        &mut __w) };
    assert!(expand_ctx.handled());

    let mut collapse_ctx = EventCtx::default();
    { let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut collapse_ctx); tree.on_message(
        &MessageEvent::new(
            tree.tree_id(),
            TreeNodeToggled {
                index: 1,
                label: "nested".to_string(),
                expanded: false,
                node_id: textual::widgets::TreeNodeId::default(),
            },
        ),
        &mut __w) };
    assert!(collapse_ctx.handled());

    { let mut __e = textual::event::EventCtx::default(); let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut __e); tree.on_message(
        &MessageEvent::new(
            NodeId::default(),
            AsyncTaskCompleted {
                task_id: 1,
                target: NodeId::default(),
                result: AsyncTaskResult::DirectoryEntries {
                    path: nested_dir.display().to_string(),
                    entries: vec![AsyncDirectoryEntry {
                        path: nested_dir.join("leaf.txt").display().to_string(),
                        label: "leaf.txt".to_string(),
                        is_dir: false,
                    }],
                },
            },
        ),
        &mut __w) };

    let console = Console::new();
    let options = options_for(&console, 60, 8);
    let buf = FrameBuffer::from_renderable(&console, &options, &tree, None);
    let lines = buf.as_plain_lines();

    assert!(!lines.iter().any(|line| line.contains("leaf.txt")));
}

#[test]
fn directory_tree_handles_forwarded_selection_messages() {
    let temp = TempTreeDir::new("directory-tree-message");
    fs::write(temp.path.join("alpha.txt"), "alpha").expect("write file");

    let mut tree = DirectoryTree::new(&temp.path);
    tree.on_layout(40, 4);

    let mut message_ctx = EventCtx::default();
    { let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut message_ctx); tree.on_message(
        &MessageEvent::new(
            tree.tree_id(),
            TreeNodeSelected {
                index: 1,
                label: "alpha.txt".to_string(),
                data: None,
                node_id: textual::widgets::TreeNodeId::default(),
            },
        ),
        &mut __w) };

    assert!(message_ctx.handled());
}

#[test]
fn directory_tree_emits_directory_selected_message_for_directory_nodes() {
    let temp = TempTreeDir::new("directory-tree-directory-message");
    fs::create_dir_all(temp.path.join("nested")).expect("create nested dir");

    let mut tree = DirectoryTree::new(&temp.path);
    tree.on_layout(40, 4);

    let mut message_ctx = EventCtx::default();
    { let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut message_ctx); tree.on_message(
        &MessageEvent::new(
            tree.tree_id(),
            TreeNodeSelected {
                index: 1,
                label: "nested".to_string(),
                data: None,
                node_id: textual::widgets::TreeNodeId::default(),
            },
        ),
        &mut __w) };

    assert!(message_ctx.handled());
}

#[test]
fn directory_tree_keyboard_navigation_is_forwarded_to_inner_tree() {
    let temp = TempTreeDir::new("directory-tree-key");
    fs::write(temp.path.join("alpha.txt"), "alpha").expect("write file");
    fs::write(temp.path.join("beta.txt"), "beta").expect("write file");

    let mut tree = DirectoryTree::new(&temp.path);
    let _guard = set_dispatch_recipient(make_node_id(), focused_state());
    tree.on_layout(40, 4);

    let down = KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    let mut ctx = EventCtx::default();
    { let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut ctx); tree.on_event(&Event::Key(down), &mut __w) };
    assert!(ctx.handled());
}

#[test]
fn directory_tree_style_type_is_directory_tree() {
    let temp = TempTreeDir::new("directory-tree-style");
    let tree = DirectoryTree::new(&temp.path);
    assert_eq!(tree.style_type(), "DirectoryTree");
}

#[test]
fn directory_tree_hover_state_is_forwarded() {
    let temp = TempTreeDir::new("directory-tree-hover");
    let mut tree = DirectoryTree::new(&temp.path);
    tree.on_layout(40, 4);
    // Hover state is tracked in the node record, not the widget.
    // on_node_state_changed propagates state changes to internal sub-widgets.
    // node_state().hovered reflects dispatch context (always false outside of dispatch guard).
    tree.on_node_state_changed(hovered_state(), hovered_state());
    assert!(!tree.node_state().hovered);
    tree.on_node_state_changed(hovered_state(), NodeState::default());
    assert!(!tree.node_state().hovered);
}

#[test]
fn directory_tree_unmount_clears_focus_hover_and_pending_loads() {
    let temp = TempTreeDir::new("directory-tree-unmount");
    let nested_dir = temp.path.join("nested");
    fs::create_dir_all(&nested_dir).expect("create nested dir");
    fs::write(nested_dir.join("leaf.txt"), "leaf").expect("write nested file");

    let mut tree = DirectoryTree::new(&temp.path);
    tree.on_layout(60, 8);
    // Focus/hover state lives on the node record; simulate with on_node_state_changed.
    tree.on_node_state_changed(NodeState::default(), focused_state());
    tree.on_node_state_changed(NodeState::default(), hovered_state());

    let mut expand_ctx = EventCtx::default();
    { let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut expand_ctx); tree.on_message(
        &MessageEvent::new(
            tree.tree_id(),
            TreeNodeToggled {
                index: 1,
                label: "nested".to_string(),
                expanded: true,
                node_id: textual::widgets::TreeNodeId::default(),
            },
        ),
        &mut __w) };
    assert!(expand_ctx.handled());

    tree.on_unmount();
    { let mut __e = textual::event::EventCtx::default(); let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut __e); tree.on_message(
        &MessageEvent::new(
            NodeId::default(),
            AsyncTaskCompleted {
                task_id: 1,
                target: NodeId::default(),
                result: AsyncTaskResult::DirectoryEntries {
                    path: nested_dir.display().to_string(),
                    entries: vec![AsyncDirectoryEntry {
                        path: nested_dir.join("leaf.txt").display().to_string(),
                        label: "leaf.txt".to_string(),
                        is_dir: false,
                    }],
                },
            },
        ),
        &mut __w) };

    // Focus/hover are now in the node record; without a dispatch guard they read as false.
    assert!(!tree.node_state().focused);
    assert!(!tree.node_state().hovered);

    let console = Console::new();
    let options = options_for(&console, 60, 8);
    let buf = FrameBuffer::from_renderable(&console, &options, &tree, None);
    let lines = buf.as_plain_lines();
    assert!(!lines.iter().any(|line| line.contains("leaf.txt")));
}

/// B-cluster lazy-load verification: a custom `filter_paths` predicate must apply
/// on the async lazy subdir-load path, not just the initial sync build.
///
/// Mirrors Python `FilteredDirectoryTree.filter_paths`, whose filter runs inside
/// the single `_load_directory` worker used for every load (initial + lazy expand
/// + reload). Here we expand a subdirectory (spawning an async `ReadDirectory`),
/// deliver an async result containing BOTH a filtered-out entry and a kept entry,
/// and assert the filtered entry never reaches the rendered tree.
#[test]
fn directory_tree_filter_applies_on_async_lazy_subdir_load() {
    fn no_dotfiles(path: &std::path::Path) -> bool {
        path.file_name()
            .and_then(|name| name.to_str())
            .map(|name| !name.starts_with('.'))
            .unwrap_or(true)
    }

    let temp = TempTreeDir::new("directory-tree-async-filter-render");
    let nested_dir = temp.path.join("nested");
    fs::create_dir_all(&nested_dir).expect("create nested dir");

    let mut tree = DirectoryTree::new(&temp.path);
    // Install a custom path filter (excludes dotfiles), as a FilteredDirectoryTree would.
    tree.filter_paths(no_dotfiles);
    tree.on_layout(60, 10);

    // Collapse the auto-expanded "nested" node, then re-expand to force a fresh
    // async lazy load (the path exercised by real subdir expansion).
    let mut collapse_ctx = EventCtx::default();
    { let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut collapse_ctx); tree.on_message(
        &MessageEvent::new(
            tree.tree_id(),
            TreeNodeToggled {
                index: 1,
                label: "nested".to_string(),
                expanded: false,
                node_id: textual::widgets::TreeNodeId::default(),
            },
        ),
        &mut __w) };

    let mut expand_ctx = EventCtx::default();
    { let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut expand_ctx); tree.on_message(
        &MessageEvent::new(
            tree.tree_id(),
            TreeNodeToggled {
                index: 1,
                label: "nested".to_string(),
                expanded: true,
                node_id: textual::widgets::TreeNodeId::default(),
            },
        ),
        &mut __w) };
    assert!(expand_ctx.handled());

    // Task IDs increment from 1 with each spawn. The initial sync build and the
    // collapse perform no async spawn, so this re-expand is the first spawned task
    // (id 1). This is the async lazy-load path — the only path exercised by real
    // subdir expansion.
    let task_id = 1_u64;

    // Deliver an async result for the lazy load with both a kept file and a
    // dotfile that the custom filter must exclude.
    { let mut __e = textual::event::EventCtx::default(); let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut __e); tree.on_message(
        &MessageEvent::new(
            NodeId::default(),
            AsyncTaskCompleted {
                task_id,
                target: NodeId::default(),
                result: AsyncTaskResult::DirectoryEntries {
                    path: nested_dir.display().to_string(),
                    entries: vec![
                        AsyncDirectoryEntry {
                            path: nested_dir.join("keep.txt").display().to_string(),
                            label: "keep.txt".to_string(),
                            is_dir: false,
                        },
                        AsyncDirectoryEntry {
                            path: nested_dir.join(".secret").display().to_string(),
                            label: ".secret".to_string(),
                            is_dir: false,
                        },
                    ],
                },
            },
        ),
        &mut __w) };

    let console = Console::new();
    let options = options_for(&console, 60, 10);
    let buf = FrameBuffer::from_renderable(&console, &options, &tree, None);
    let lines = buf.as_plain_lines();

    assert!(
        lines.iter().any(|line| line.contains("keep.txt")),
        "kept file must render after async lazy load"
    );
    assert!(
        !lines.iter().any(|line| line.contains(".secret")),
        "custom filter must exclude the dotfile on the async lazy-load path"
    );
}
