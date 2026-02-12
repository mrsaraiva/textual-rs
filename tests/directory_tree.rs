use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use rich_rs::Console;
use textual::message::{AsyncDirectoryEntry, AsyncTaskResult, MessageEvent};
use textual::prelude::*;
use textual::render::FrameBuffer;

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
    tree.on_message(
        &MessageEvent {
            sender: tree.tree_id(),
            message: Message::TreeNodeToggled {
                index: 1,
                label: "nested".to_string(),
                expanded: true,
            },
        },
        &mut message_ctx,
    );
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

    tree.on_message(
        &MessageEvent {
            sender: NodeId::default(),
            message: Message::AsyncTaskCompleted {
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
        },
        &mut EventCtx::default(),
    );

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
    tree.on_message(
        &MessageEvent {
            sender: tree.tree_id(),
            message: Message::TreeNodeToggled {
                index: 1,
                label: "nested".to_string(),
                expanded: true,
            },
        },
        &mut message_ctx,
    );
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
    tree.on_message(
        &MessageEvent {
            sender: tree.tree_id(),
            message: Message::TreeNodeToggled {
                index: 1,
                label: "nested".to_string(),
                expanded: true,
            },
        },
        &mut expand_ctx,
    );
    assert!(expand_ctx.handled());

    let mut collapse_ctx = EventCtx::default();
    tree.on_message(
        &MessageEvent {
            sender: tree.tree_id(),
            message: Message::TreeNodeToggled {
                index: 1,
                label: "nested".to_string(),
                expanded: false,
            },
        },
        &mut collapse_ctx,
    );
    assert!(collapse_ctx.handled());

    tree.on_message(
        &MessageEvent {
            sender: NodeId::default(),
            message: Message::AsyncTaskCompleted {
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
        },
        &mut EventCtx::default(),
    );

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
    tree.on_message(
        &MessageEvent {
            sender: tree.tree_id(),
            message: Message::TreeNodeSelected {
                index: 1,
                label: "alpha.txt".to_string(),
            },
        },
        &mut message_ctx,
    );

    assert!(message_ctx.handled());
}

#[test]
fn directory_tree_emits_directory_selected_message_for_directory_nodes() {
    let temp = TempTreeDir::new("directory-tree-directory-message");
    fs::create_dir_all(temp.path.join("nested")).expect("create nested dir");

    let mut tree = DirectoryTree::new(&temp.path);
    tree.on_layout(40, 4);

    let mut message_ctx = EventCtx::default();
    tree.on_message(
        &MessageEvent {
            sender: tree.tree_id(),
            message: Message::TreeNodeSelected {
                index: 1,
                label: "nested".to_string(),
            },
        },
        &mut message_ctx,
    );

    assert!(message_ctx.handled());
}

#[test]
fn directory_tree_keyboard_navigation_is_forwarded_to_inner_tree() {
    let temp = TempTreeDir::new("directory-tree-key");
    fs::write(temp.path.join("alpha.txt"), "alpha").expect("write file");
    fs::write(temp.path.join("beta.txt"), "beta").expect("write file");

    let mut tree = DirectoryTree::new(&temp.path);
    tree.set_focus(true);
    tree.on_layout(40, 4);

    let down = KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    let mut ctx = EventCtx::default();
    tree.on_event(&Event::Key(down), &mut ctx);
    assert!(ctx.handled());
}

#[test]
fn directory_tree_exposes_inner_tree_via_child_visit() {
    let temp = TempTreeDir::new("directory-tree-visit");
    let mut tree = DirectoryTree::new(&temp.path);

    let mut visited = 0usize;
    tree.visit_children_mut(&mut |child| {
        visited += 1;
        assert_eq!(child.style_type(), "Tree");
    });

    assert_eq!(visited, 1);
}

#[test]
fn directory_tree_hover_state_is_forwarded_to_inner_tree() {
    let temp = TempTreeDir::new("directory-tree-hover");
    let mut tree = DirectoryTree::new(&temp.path);
    tree.set_hovered(true);

    let mut child_hovered = false;
    tree.visit_children_mut(&mut |child| {
        child_hovered = child.is_hovered();
    });

    assert!(child_hovered);
}

#[test]
fn directory_tree_unmount_clears_focus_hover_and_pending_loads() {
    let temp = TempTreeDir::new("directory-tree-unmount");
    let nested_dir = temp.path.join("nested");
    fs::create_dir_all(&nested_dir).expect("create nested dir");
    fs::write(nested_dir.join("leaf.txt"), "leaf").expect("write nested file");

    let mut tree = DirectoryTree::new(&temp.path);
    tree.on_layout(60, 8);
    tree.set_focus(true);
    tree.set_hovered(true);

    let mut expand_ctx = EventCtx::default();
    tree.on_message(
        &MessageEvent {
            sender: tree.tree_id(),
            message: Message::TreeNodeToggled {
                index: 1,
                label: "nested".to_string(),
                expanded: true,
            },
        },
        &mut expand_ctx,
    );
    assert!(expand_ctx.handled());

    tree.on_unmount();
    tree.on_message(
        &MessageEvent {
            sender: NodeId::default(),
            message: Message::AsyncTaskCompleted {
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
        },
        &mut EventCtx::default(),
    );

    assert!(!tree.has_focus());
    assert!(!tree.is_hovered());

    let console = Console::new();
    let options = options_for(&console, 60, 8);
    let buf = FrameBuffer::from_renderable(&console, &options, &tree, None);
    let lines = buf.as_plain_lines();
    assert!(!lines.iter().any(|line| line.contains("leaf.txt")));
}
